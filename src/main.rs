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
