//! `janitor` — thin CLI subset (ADR-14): scan --json / plan / restore / interactive purge.
//! Purge NEVER gains a --yes bypass (P4). Subcommands grow per milestone (M1: scan).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "janitor",
    version,
    about = "还我磁盘 Giveback — disk janitor CLI"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan roots and emit classified candidates as JSON (ADR-14).
    Scan {
        /// Roots to scan (default: home directory).
        roots: Vec<std::path::PathBuf>,
        /// Emit the raw JSON envelope (machine form).
        #[arg(long, default_value_t = true)]
        json: bool,
        /// Minimum candidate size in bytes.
        #[arg(long)]
        min_bytes: Option<u64>,
    },
    /// Build a plan manifest from a scan JSON file + an NL policy sentence.
    Plan {
        /// Path to `janitor scan --json` output.
        scan_json: std::path::PathBuf,
        /// Policy sentence, e.g. 「腾出 25 GB，别动我这两周在跑的项目」.
        #[arg(long)]
        policy: Option<String>,
        /// Minimum candidate size in bytes for plan inclusion.
        #[arg(long)]
        min_bytes: Option<u64>,
        /// Where to write the manifest (default: ./plan-<ts>.json).
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Apply a plan manifest: atomic same-volume quarantine moves (reversible).
    Apply {
        /// Path to the plan manifest.
        manifest: std::path::PathBuf,
        /// Crash-injection point (crash tests only; unspecified = run to completion).
        #[arg(long, hide = true)]
        kill_at: Option<String>,
    },
    /// Purge quarantined entries — frees bytes. Requires the EXACT allocated byte count.
    Purge {
        /// Path to the plan manifest.
        manifest: std::path::PathBuf,
        /// The exact total allocated bytes of the quarantined plan (typed confirmation, P4).
        #[arg(long)]
        bytes: u64,
    },
    /// Restore a quarantined op back to its original paths.
    Restore {
        /// WAL op id (as printed by apply).
        op_id: String,
    },
    /// Recover from crashes: reconcile per-volume WALs against the filesystem (§5.2).
    Recover,
    /// Live LLM agent run (feature `llm`): scan JSON → tool loop → proposal.
    /// Build with `--features llm`; errors cleanly otherwise. `--resume <run_id>`
    /// continues a saved session from the sessions dir instead of starting fresh.
    Agent {
        /// Path to `janitor scan --json` output (needed for fresh runs; on
        /// `--resume` it re-seeds the session context if given, else empty).
        scan_json: Option<std::path::PathBuf>,
        /// User intent sentence, e.g. 「腾出 25 GB」 (fresh runs only).
        #[arg(long)]
        prompt: Option<String>,
        /// Resume a saved session:
        /// `~/.local/state/disk-janitor/sessions/<run_id>.json`.
        #[arg(long)]
        resume: Option<String>,
        /// Print the trajectory as JSON lines instead of the human summary.
        #[arg(long)]
        json_events: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Scan {
            roots,
            json: _,
            min_bytes,
        } => {
            let roots = if roots.is_empty() {
                vec![dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?]
            } else {
                roots
            };
            janitor_scan::run_scan(roots, min_bytes)
        }
        Cmd::Plan {
            scan_json,
            policy,
            min_bytes,
            out,
        } => janitor_plan::run_plan(scan_json, policy.as_deref(), min_bytes, out),
        Cmd::Apply { manifest, kill_at } => janitor_exec::run_apply(manifest, kill_at.as_deref()),
        Cmd::Purge { manifest, bytes } => janitor_exec::run_purge(manifest, bytes),
        Cmd::Restore { op_id } => janitor_exec::run_restore(&op_id),
        Cmd::Recover => janitor_exec::run_recover(),
        Cmd::Agent {
            scan_json,
            prompt,
            resume,
            json_events,
        } => janitor_agent::run_agent(
            scan_json.as_deref(),
            prompt.as_deref(),
            resume.as_deref(),
            json_events,
        ),
    }
}

pub mod janitor_agent {
    /// R5 sessions directory: `dirs::state_dir()/disk-janitor/sessions`
    /// (SPEC §3.7 — the same dir the GUI Ledger lists). Shared by `--resume`
    /// (load) and the end-of-run persist (save).
    #[cfg(feature = "llm")]
    fn sessions_dir() -> anyhow::Result<std::path::PathBuf> {
        // macOS has no XDG state dir: dirs::state_dir() is None there. Fall back
        // to the SPEC §3.7 spelling ($HOME/.local/state) so CLI and GUI agree on
        // macOS too. create_dir_all so the first save/list both work.
        let state = dirs::state_dir().unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            home.join(".local").join("state")
        });
        let dir = state.join("disk-janitor").join("sessions");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// `janitor agent [<scan-json> --prompt … | --resume <run_id>]` (D-36): live
    /// LLM agent run. A fresh run seeds `SessionCtx` from the scan JSON exactly
    /// like `run_plan`; `--resume` continues a saved session from the sessions
    /// dir. Drives the sans-IO loop (step → execute_tools → step …) until Done,
    /// then prints the trajectory, the proposal (plan id / awaiting-human-confirm),
    /// and cumulative usage/cost from the budget state.
    ///
    /// R4: every model call / tool execution / terminal transition streams a
    /// progress line to stderr (`--json-events`: one JSON line per event on
    /// stdout, human lines + summary on stderr). SIGINT (Ctrl-C) or ANY stdin
    /// line (Enter) cancels the run — Done{UserCancel}, and `finish` still posts
    /// the Budget rec (SPEC §3.2).
    ///
    /// R5: every finished run (fresh or resumed) is persisted to the sessions
    /// dir so the GUI Ledger can list/reload it.
    #[cfg(feature = "llm")]
    pub fn run_agent(
        scan_json: Option<&std::path::Path>,
        prompt: Option<&str>,
        resume: Option<&str>,
        json_events: bool,
    ) -> anyhow::Result<()> {
        use disk_janitor::agent::http::HttpChatClient;
        use disk_janitor::agent::{self, Role, RunState};
        use std::io::BufRead;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        // R4: graceful Ctrl-C → run.cancel() → Done{UserCancel} → finish() still
        // posts the Budget rec. The default SIGINT would kill the process mid-run.
        INTERRUPTED.store(false, Ordering::SeqCst);
        unsafe {
            // Function-pointer → integer cast (sighandler_t); the lint wants an
            // explicit pointer hop first — same ABI, no behavior change.
            libc::signal(
                libc::SIGINT,
                handle_sigint as *const () as libc::sighandler_t,
            );
        }

        // Any stdin line (Enter) cancels too; the listener thread exits on EOF,
        // so a non-interactive run (closed stdin) is never cancelled by it.
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || {
                for _line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    cancel.store(true, Ordering::SeqCst);
                }
            });
        }

        let cfg = disk_janitor::config::load()?;
        eprintln!("agent model: {} @ {}", cfg.model, cfg.endpoint);
        let client = HttpChatClient::new(cfg.clone())?;

        let home = disk_janitor::execute::home_ledger_dir();
        let ledger = disk_janitor::ledger::open(&home)?;

        // Fresh run: the scan JSON seeds the ctx (same parse as run_plan).
        // Resumed run: SessionCtx is caller-seeded by design and not part of the
        // snapshot — an empty ctx lets the model finish gracefully (R5 contract).
        let (mut ctx, mut run) = if let Some(rid) = resume {
            let path = sessions_dir()?.join(format!("{rid}.json"));
            (
                agent::SessionCtx {
                    candidates: Vec::new(),
                    vols: Vec::new(),
                    ledger,
                    files_scanned: 0,
                    last_plan_id: None,
                    last_plan_bytes: None,
                },
                disk_janitor::agent::load_snapshot(&path)?,
            )
        } else {
            let scan_json = scan_json.ok_or_else(|| {
                anyhow::anyhow!("`janitor agent` needs a scan JSON path (or `--resume <run_id>`)")
            })?;
            let prompt = prompt.ok_or_else(|| {
                anyhow::anyhow!("`janitor agent` needs `--prompt` (or `--resume <run_id>`)")
            })?;
            let raw = fs_err::read_to_string(scan_json)?;
            let parsed: serde_json::Value = serde_json::from_str(&raw)?;
            let candidates: Vec<disk_janitor::Candidate> =
                serde_json::from_value(parsed["candidates"].clone())?;
            let scan: disk_janitor::scan::ScanTotals =
                serde_json::from_value(parsed["scan"].clone())?;
            (
                agent::SessionCtx {
                    candidates,
                    vols: scan.volumes,
                    ledger,
                    files_scanned: scan.files,
                    last_plan_id: None,
                    last_plan_bytes: None,
                },
                agent::AgentRun::new(prompt, &cfg.model, cfg.prices.clone(), cfg.budget.clone()),
            )
        };

        // R4 progress streaming: JSON lines on stdout, human lines on stderr.
        let on_progress = move |ev: &agent::ProgressEvent| {
            if json_events {
                println!(
                    "{}",
                    serde_json::to_string(ev).expect("progress event is JSON")
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
            } else {
                eprintln!(
                    "turn {} · {} · {}{} · {} tok · ${:.6}",
                    ev.turn,
                    ev.phase,
                    ev.tool.as_deref().unwrap_or("-"),
                    if ev.detail.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", ev.detail)
                    },
                    ev.budget.tokens,
                    ev.budget.cost_usd,
                );
            }
        };
        run.on_progress(Some(Box::new(on_progress)));

        loop {
            if cancel.load(Ordering::SeqCst) || INTERRUPTED.load(Ordering::SeqCst) {
                run.cancel();
                break;
            }
            match run.step(&client) {
                Ok(RunState::CallTools { calls }) => {
                    run.execute_tools(&calls, &mut ctx);
                }
                Ok(RunState::Done { .. }) => break,
                Ok(RunState::CallModel { .. }) => unreachable!("step never returns CallModel"),
                Err(e) => anyhow::bail!("agent step failed: {e}"),
            }
        }
        run.finish(&mut ctx)?;
        // R5: persist every finished run (fresh or resumed) so the GUI can list it.
        run.persist_snapshot(&sessions_dir()?)?;

        // Final summary: stdout in human mode; in JSON mode stdout is the event
        // stream, so the summary goes to stderr.
        macro_rules! say {
            ($($t:tt)*) => {
                if json_events {
                    eprintln!($($t)*);
                } else {
                    println!($($t)*);
                }
            };
        }

        // --- trajectory ---------------------------------------------------------
        say!("== trajectory (run {}) ==", run.run_id());
        for (i, m) in run.history().iter().enumerate() {
            if m.role == Role::Tool {
                let tag = m
                    .tool_call_id
                    .as_deref()
                    .map(|id| format!("({id})"))
                    .unwrap_or_default();
                say!("[{:>2}] tool{tag}: {}", i + 1, m.content);
            } else {
                say!("[{:>2}] {:?}: {}", i + 1, m.role, m.content);
                for tc in &m.tool_calls {
                    say!("        → {}: {}({})", tc.id, tc.name, tc.arguments);
                }
            }
        }

        // --- result --------------------------------------------------------------
        let reason = match run.state() {
            RunState::Done { reason } => *reason,
            _ => unreachable!("loop exits only on Done"),
        };
        say!("== result ==");
        say!("stop_reason: {reason:?}");
        let mut proposal = None;
        let mut verdict = None;
        for m in run.history().iter().rev() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&m.content) else {
                continue;
            };
            if m.role == Role::Tool {
                if proposal.is_none() && v.get("plan_id").is_some() {
                    proposal = Some(v);
                } else if verdict.is_none() && v.get("verdict").is_some() {
                    verdict = Some(v);
                }
            }
            if proposal.is_some() && verdict.is_some() {
                break;
            }
        }
        match proposal {
            Some(p) => say!(
                "proposal: plan_id={} awaiting={}",
                p["plan_id"],
                p.get("awaiting").and_then(|a| a.as_str()).unwrap_or("n/a")
            ),
            None => {
                let last = run
                    .history()
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant && !m.content.is_empty());
                match last {
                    Some(m) => say!("proposal: (final text) {}", m.content),
                    None => say!("proposal: (none — run ended without a plan)"),
                }
            }
        }
        if let Some(v) = verdict {
            say!(
                "verdict: {} reason={}",
                v["verdict"],
                v.get("reason").and_then(|r| r.as_str()).unwrap_or("")
            );
        }

        // --- budget ---------------------------------------------------------------
        let b = run.budget();
        say!("== budget ==");
        say!(
            "turns: {} / {}",
            b.turns,
            if b.cap.max_turns == 0 {
                "∞".to_string()
            } else {
                b.cap.max_turns.to_string()
            }
        );
        say!(
            "tokens: prompt {} + completion {} = {} / {}",
            b.prompt_tokens,
            b.completion_tokens,
            b.tokens,
            if b.cap.tokens == 0 {
                "∞".to_string()
            } else {
                b.cap.tokens.to_string()
            }
        );
        say!(
            "cost_usd: {:.6} / {}",
            b.cost_usd,
            if b.cap.cost_usd == 0.0 {
                "∞".to_string()
            } else {
                b.cap.cost_usd.to_string()
            }
        );
        Ok(())
    }

    /// R4: SIGINT flag — set by the signal handler, polled by the run loop.
    #[cfg(feature = "llm")]
    static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// Async-signal-safe: an atomic store is lock-free (the standard pattern).
    #[cfg(feature = "llm")]
    extern "C" fn handle_sigint(_sig: libc::c_int) {
        INTERRUPTED.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Clean error when the binary was built without the `llm` feature.
    #[cfg(not(feature = "llm"))]
    pub fn run_agent(
        _scan_json: Option<&std::path::Path>,
        _prompt: Option<&str>,
        _resume: Option<&str>,
        _json_events: bool,
    ) -> anyhow::Result<()> {
        anyhow::bail!(
            "`janitor agent` needs the `llm` feature (live model calls): \
             build with `cargo build --features llm`"
        )
    }
}
pub mod janitor_plan {
    use std::collections::BTreeMap;

    use disk_janitor::Candidate;
    use disk_janitor::plan::{self, PolicyConstraintsShape};

    /// plan pipeline (SPEC §3.3): scan JSON → candidates → policy compile → manifest.
    pub fn run_plan(
        scan_json: std::path::PathBuf,
        policy: Option<&str>,
        min_bytes: Option<u64>,
        out: Option<std::path::PathBuf>,
    ) -> anyhow::Result<()> {
        let raw = fs_err::read_to_string(&scan_json)?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)?;
        let candidates: Vec<Candidate> = serde_json::from_value(parsed["candidates"].clone())?;
        let scan: disk_janitor::scan::ScanTotals = serde_json::from_value(parsed["scan"].clone())?;
        let mut pc = match policy {
            Some(sentence) => {
                let compiled = disk_janitor::policy::compile(sentence);
                for rule in &compiled.applied_rules {
                    eprintln!("policy rule applied: {rule}");
                }
                for frag in &compiled.unmatched {
                    eprintln!("⚠ 未识别的政策片段 (按默认处理): {frag}");
                }
                compiled.constraints
            }
            None => PolicyConstraintsShape::default(),
        };
        // CLI floor wins over the compiled default (last-mile knob, §3.6).
        if let Some(mb) = min_bytes {
            pc.min_bytes = mb;
        }
        let plan = plan::build(&candidates, &pc, &scan.volumes);
        let out_path = out.unwrap_or_else(|| {
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            std::path::PathBuf::from(format!("plan-{ts}.json"))
        });
        plan::write_manifest(&plan, &out_path)?;
        println!("{}", plan::summarize(&plan));
        println!("manifest written: {}", out_path.display());
        Ok(())
    }

    // silence unused warning for the BTreeMap import path when vols empty
    #[allow(dead_code)]
    fn _t(_: BTreeMap<u8, u8>) {}
}

pub mod janitor_exec {
    use crossbeam_channel::unbounded;
    use disk_janitor::execute::{self, ExecEvent};
    use disk_janitor::plan;

    fn load_plan(manifest: &std::path::Path) -> anyhow::Result<plan::Plan> {
        plan::load_manifest(manifest).map_err(|e| anyhow::anyhow!("manifest load failed: {e}"))
    }

    pub fn run_apply(manifest: std::path::PathBuf, kill_at: Option<&str>) -> anyhow::Result<()> {
        let p = load_plan(&manifest)?;
        let vols = volume_table(&p)?;
        let home = disk_janitor::execute::home_ledger_dir();
        let ledger = disk_janitor::ledger::open(&home)?;
        let entries: Vec<disk_janitor::PlanEntry> = p
            .entries
            .iter()
            .filter(|e| matches!(e.action, disk_janitor::PlanAction::Quarantine))
            .cloned()
            .collect();
        let mut ex = execute::open(p.plan_id.clone(), entries, &vols, ledger)?;
        let kill = kill_at.and_then(execute::KillPoint::parse);
        let (tx, rx) = unbounded();
        let report = ex.apply(tx, kill)?;
        for ev in rx.try_iter() {
            print_event(&ev);
        }
        println!(
            "apply {}: op={} applied={} skipped={} refused={} bytes={}",
            p.plan_id,
            report.op_id,
            report.applied,
            report.skipped,
            report.refused,
            report.quarantined_bytes
        );
        Ok(())
    }

    pub fn run_purge(manifest: std::path::PathBuf, bytes: u64) -> anyhow::Result<()> {
        let p = load_plan(&manifest)?;
        let vols = volume_table(&p)?;
        let home = disk_janitor::execute::home_ledger_dir();
        let ledger = disk_janitor::ledger::open(&home)?;
        let entries: Vec<disk_janitor::PlanEntry> = p
            .entries
            .iter()
            .filter(|e| matches!(e.action, disk_janitor::PlanAction::Quarantine))
            .cloned()
            .collect();
        let mut ex = execute::open(p.plan_id.clone(), entries, &vols, ledger)?;
        let (tx, rx) = unbounded();
        let report = ex.purge(bytes, tx)?;
        for ev in rx.try_iter() {
            print_event(&ev);
        }
        println!(
            "purge {}: freed {} bytes across {} entries",
            p.plan_id, report.freed_bytes, report.entries
        );
        Ok(())
    }

    pub fn run_restore(op_id: &str) -> anyhow::Result<()> {
        let home = disk_janitor::execute::home_ledger_dir();
        let ledger = disk_janitor::ledger::open(&home)?;
        // restore needs the volume table + entries from the ledger plan record; the
        // Executor reconstructs from the ledger's stored plan via its op id.
        let mut ex = execute::open(
            "restore".into(),
            Vec::new(),
            &disk_janitor::scan::volumes(),
            ledger,
        )
        .map_err(|e| anyhow::anyhow!("cannot restore {op_id}: {e}"))?;
        let (tx, rx) = unbounded();
        let report = ex.restore(op_id, tx)?;
        for ev in rx.try_iter() {
            print_event(&ev);
        }
        println!(
            "restore {op_id}: {} restored, {} missing",
            report.restored.len(),
            report.missing.len()
        );
        Ok(())
    }

    pub fn run_recover() -> anyhow::Result<()> {
        let vols = disk_janitor::scan::volumes();
        let report = execute::recover(&vols);
        println!(
            "recover: {} volumes, {} rolled forward, {} dangling intents, {} expired-grace entries",
            report.volumes_checked,
            report.rolled_forward,
            report.dangling_intents.len(),
            report.expired_grace.len()
        );
        Ok(())
    }

    fn volume_table(_p: &plan::Plan) -> anyhow::Result<Vec<disk_janitor::VolumeInfo>> {
        // Volume truth comes from the live machine (statfs now), not the manifest.
        // Firmlink dedup: for one st_dev keep the WRITABLE mount when both a sealed-RO
        // `/` and writable `/System/Volumes/Data` appear — quarantine moves must land
        // on a mount we can mkdir in (SPEC §5.1 same-volume rule).
        let vols = disk_janitor::scan::volumes();
        let mut out: Vec<disk_janitor::VolumeInfo> = Vec::new();
        for v in vols {
            match out.iter_mut().find(|o| o.dev == v.dev) {
                Some(existing) => {
                    if existing.read_only && !v.read_only {
                        *existing = v;
                    }
                }
                None => out.push(v),
            }
        }
        Ok(out)
    }

    fn print_event(ev: &ExecEvent) {
        match ev {
            ExecEvent::EntryApplied { cid, to } => println!("  applied {cid} → {to}"),
            ExecEvent::EntryPurged { cid, bytes } => println!("  purged {cid} ({bytes} B)"),
            ExecEvent::EntryRestored { cid, to } => println!("  restored {cid} → {to}"),
            ExecEvent::Error { cid, err } => eprintln!("  error {cid}: {err}"),
            ExecEvent::Done => {}
        }
    }
}

pub mod janitor_scan {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use disk_janitor::Thresholds;
    use disk_janitor::classify;
    use disk_janitor::scan::{self, ScanEvent, ScanOptions, ScanStats};

    /// Scan pipeline (SPEC §3.1→§3.2): scan thread → channel → classify → JSON envelope
    /// to stdout. JSON shape: {"scan": ScanTotals, "candidates": [Candidate…]}.
    pub fn run_scan(roots: Vec<std::path::PathBuf>, min_bytes: Option<u64>) -> anyhow::Result<()> {
        let cancel = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(ScanStats::default());
        let opts = ScanOptions {
            roots: roots.clone(),
            threads: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4),
            cross_filesystems: false,
            ignore: Vec::new(),
            min_dir_bytes: 0,
        };
        let (tx, rx) = crossbeam_channel::bounded::<ScanEvent>(100);
        let handle = scan::start(opts, tx, cancel, stats.clone());
        let mut batches: Vec<disk_janitor::scan::DirStat> = Vec::new();
        let mut totals = None;
        for event in rx {
            match event {
                ScanEvent::Batch(b) => batches.extend(b),
                ScanEvent::RootError { root, err } => {
                    eprintln!("scan error at {}: {err}", root.display());
                }
                ScanEvent::Finished(t) => {
                    totals = Some(t);
                    break;
                }
            }
        }
        let scan_totals = totals.ok_or_else(|| anyhow::anyhow!("scan ended without totals"))?;
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("scan thread panicked"))?;
        let th = Thresholds {
            min_bytes: min_bytes.unwrap_or(Thresholds::default().min_bytes),
            ..Thresholds::default()
        };
        let candidates = classify::candidates_from_batch(batches, &th);
        let envelope = serde_json::json!({ "scan": scan_totals, "candidates": candidates });
        serde_json::to_writer_pretty(std::io::stdout().lock(), &envelope)?;
        println!();
        Ok(())
    }
}
