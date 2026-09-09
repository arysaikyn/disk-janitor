//! Named IPC commands (SPEC Part C §2.3): ~10 commands, typed payloads, no raw fs.
//! Scan/apply progress cross the boundary via `tauri::ipc::Channel<T>`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

/// Shared scan session state (cancel flag + in-flight guard + last result path + demo mode).
pub struct ScanState {
    /// Shared cancel flag — the SAME `Arc` the walker polls (A1): `cancel_scan`
    /// stores true here and `start_scan` hands `state.cancel.clone()` to
    /// `scan::start`, so a cancel lands on the running walker's atomic.
    pub cancel: Arc<AtomicBool>,
    /// True while a scan is in flight (A2 re-entry guard) — see `ScanGuard`.
    pub scanning: AtomicBool,
    pub last_scan: Mutex<Option<PathBuf>>,
    /// D-38: `--demo-data` launch — every command returns the synthetic payloads
    /// from `mod demo` (mirroring app/src/lib/ipc.ts) so screen recordings are
    /// reproducible (P7). Set once in main.rs before `.run()`.
    pub demo: bool,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            scanning: AtomicBool::new(false),
            last_scan: Mutex::new(None),
            demo: false,
        }
    }
}

/// RAII in-flight guard (A2): `acquire` fails while another scan holds the
/// flag; `Drop` resets it on every exit path — early returns, errors, and even
/// the command future being dropped (webview gone mid-scan).
struct ScanGuard<'a>(&'a AtomicBool);

impl<'a> ScanGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        if flag.swap(true, Ordering::SeqCst) {
            return Err("scan already in progress".into());
        }
        Ok(Self(flag))
    }
}

impl Drop for ScanGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// D-38: demo-mode probe. The webview asks once at startup; when true it routes
/// every command through the local demo fallbacks (app/src/lib/ipc.ts) instead
/// of the real IPC.
#[tauri::command]
pub async fn is_demo(state: State<'_, ScanState>) -> Result<bool, String> {
    Ok(state.demo)
}

/// Scan progress delta — typed channel payload (SPEC §3.3: ≥100 ms throttle).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ScanProgress {
    Files {
        files: u64,
        bytes: u64,
        rate_bps: f64,
        current: String,
    },
    Candidate {
        id: String,
        bytes: u64,
        grade: String,
        evidence: Vec<EvidenceDto>,
    },
    RootError {
        root: String,
        err: String,
    },
    Finished {
        files: u64,
        bytes: u64,
        elapsed_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceDto {
    pub kind: String,
    pub detail: String,
    pub strength: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VolumeDto {
    pub mount: String,
    pub fs: String,
    pub total: u64,
    pub free_raw: u64,
    pub read_only: bool,
    pub writable: bool,
}

#[tauri::command]
pub async fn get_volumes(state: State<'_, ScanState>) -> Result<Vec<VolumeDto>, String> {
    if state.demo {
        return Ok(demo::volumes());
    }
    Ok(disk_janitor::scan::volumes()
        .into_iter()
        .map(|v| VolumeDto {
            mount: v.mount.display().to_string(),
            fs: v.fs,
            total: v.total,
            free_raw: v.free_raw,
            read_only: v.read_only,
            writable: !v.read_only,
        })
        .collect())
}

/// Start a scan of `roots`, streaming batches + candidates over `channel`.
/// Cancel via `cancel_scan` (R4; D-15: drop incomplete batches, keep completed).
#[tauri::command]
pub async fn start_scan(
    roots: Vec<String>,
    min_bytes: Option<u64>,
    channel: Channel<ScanProgress>,
    state: State<'_, ScanState>,
) -> Result<String, String> {
    if state.demo {
        demo::stream_scan(&channel);
        return Ok("/Users/me/.disk-janitor/last-scan.json".to_string());
    }
    // Fresh scan: clear any cancel left over from a previous one (the walker
    // polls this same flag — A1 — via `state.cancel.clone()` inside run_scan).
    state.cancel.store(false, Ordering::SeqCst);
    run_scan(roots, min_bytes, &channel, &state, &dirs_home())
        .map(|p| p.display().to_string())
}

/// Core scan pipeline (start_scan + tests): drain the walker's events, stream
/// progress over `channel`, classify, persist `last-scan.json`.
///
/// Semantics (D-15: keep completed batches, drop incomplete):
/// - **Completed** scan → classify, emit candidates, emit `Finished`, write
///   last-scan.json, resolve with the new path.
/// - **Cancelled** (A4, `state.cancel` set before completion) → drop the
///   incomplete batches: no classify, no last-scan write; `Finished` is still
///   emitted so the frontend can navigate, and the command resolves with the
///   last COMPLETED scan's path (reopened by Review) or errors if there never
///   was one.
/// - **Abandoned** (A3, a progress send fails — user navigated away) → stop
///   the walker by disconnecting the crossbeam channel (`drop(rx)`), skip
///   classify and the last-scan write, and error out.
fn run_scan(
    roots: Vec<String>,
    min_bytes: Option<u64>,
    channel: &Channel<ScanProgress>,
    state: &ScanState,
    out_dir: &Path,
) -> Result<PathBuf, String> {
    let _guard = ScanGuard::acquire(&state.scanning)?;
    let user_home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let roots: Vec<PathBuf> = roots
        .iter()
        .map(|r| {
            // Expand ~ against the USER home (not the state dir).
            if r == "~" {
                user_home.clone()
            } else if let Some(rest) = r.strip_prefix("~/") {
                user_home.join(rest)
            } else {
                PathBuf::from(r)
            }
        })
        .collect();
    if roots.is_empty() {
        return Err("no roots given".into());
    }
    let (tx, rx) = crossbeam_channel::bounded::<disk_janitor::scan::ScanEvent>(100);
    let cancel = state.cancel.clone();
    let stats = Arc::new(disk_janitor::scan::ScanStats::default());
    let opts = disk_janitor::scan::ScanOptions {
        threads: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
        roots: roots.clone(),
        cross_filesystems: false,
        ignore: Vec::new(),
        min_dir_bytes: 0,
    };
    let handle = disk_janitor::scan::start(opts, tx, cancel, stats.clone());
    let started = std::time::Instant::now();
    let mut batches: Vec<disk_janitor::scan::DirStat> = Vec::new();
    let mut scan_totals: Option<disk_janitor::scan::ScanTotals> = None;
    let mut last_emit = std::time::Instant::now();
    // Classify runs BEFORE Finished: the frontend stops consuming the channel on
    // Finished, so candidate events emitted after it would never render.
    let th = disk_janitor::Thresholds {
        min_bytes: min_bytes.unwrap_or(disk_janitor::Thresholds::default().min_bytes),
        ..disk_janitor::Thresholds::default()
    };
    // A3: any failed progress send means the webview is gone (navigation away) —
    // the scan is abandoned: break out, drop `rx` to disconnect the crossbeam
    // channel so the walker stops promptly, then skip classify and the persist.
    let abandoned = loop {
        let Some(event) = rx.recv().ok() else {
            // Walker disconnected without a terminal Finished (only on a walker
            // panic — it always sends Finished otherwise). Treat as abandoned.
            break true;
        };
        match event {
            disk_janitor::scan::ScanEvent::Batch(b) => {
                batches.extend(b);
                if last_emit.elapsed().as_millis() >= 100 {
                    last_emit = std::time::Instant::now();
                    let files = stats.entries.load(Ordering::Relaxed);
                    let bytes = stats.bytes.load(Ordering::Relaxed);
                    let rate = bytes as f64 / started.elapsed().as_secs_f64().max(0.001);
                    if channel
                        .send(ScanProgress::Files {
                            files,
                            bytes,
                            rate_bps: rate,
                            current: String::new(),
                        })
                        .is_err()
                    {
                        break true;
                    }
                }
            }
            disk_janitor::scan::ScanEvent::RootError { root, err } => {
                if channel
                    .send(ScanProgress::RootError {
                        root: root.display().to_string(),
                        err,
                    })
                    .is_err()
                {
                    break true;
                }
            }
            disk_janitor::scan::ScanEvent::Finished(t) => {
                scan_totals = Some(t);
                break false;
            }
        }
    };
    drop(rx);
    let _ = handle.join();
    if abandoned {
        return Err("scan abandoned: progress channel closed".into());
    }
    // A4: cancelled — the walker stopped early, so its batches are incomplete.
    // Drop them (D-15): no classify, no persist; Finished still goes out below
    // so the frontend can navigate back to the last COMPLETED scan.
    let cancelled = state.cancel.load(Ordering::SeqCst);
    let mut candidates = Vec::new();
    if !cancelled {
        candidates =
            disk_janitor::classify::candidates_from_batch(std::mem::take(&mut batches), &th);
        for c in &candidates {
            if channel
                .send(ScanProgress::Candidate {
                    id: c.id.clone(),
                    bytes: c.alloc_bytes,
                    grade: serde_json::to_value(c.grade)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    evidence: c
                        .evidence
                        .iter()
                        .map(|e| EvidenceDto {
                            kind: serde_json::to_value(&e.kind)
                                .ok()
                                .and_then(|v| v.as_str().map(str::to_owned))
                                .unwrap_or_default(),
                            detail: e.detail.clone(),
                            strength: serde_json::to_value(e.strength)
                                .ok()
                                .and_then(|v| v.as_str().map(str::to_owned))
                                .unwrap_or_default(),
                        })
                        .collect(),
                })
                .is_err()
            {
                // Channel died mid-classify: same abandonment as A3.
                return Err("scan abandoned: progress channel closed".into());
            }
        }
    }
    // Finished LAST — terminal event; the UI closes the channel after it.
    let t = scan_totals.unwrap_or_default();
    let _ = channel.send(ScanProgress::Finished {
        files: t.files,
        bytes: t.alloc,
        elapsed_ms: t.elapsed_ms,
    });
    if cancelled {
        // D-15: keep the last COMPLETED scan — resolve with its path so the
        // frontend navigates to it; refuse only if none has ever completed.
        return state
            .last_scan
            .lock()
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Err("scan cancelled — no completed scan to reopen".into()));
    }
    // Persist last scan (owner decision 09-08: get_last_scan reopens without rescan).
    let scan_path = out_dir.join("last-scan.json");
    let envelope = serde_json::json!({
        "roots": roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        "min_bytes": th.min_bytes,
        "candidates": candidates,
    });
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        &scan_path,
        serde_json::to_vec_pretty(&envelope).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    *state.last_scan.lock() = Some(scan_path.clone());
    Ok(scan_path)
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, ScanState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Reopen the last scan result without rescanning (SPEC §2.3 persistence row).
#[tauri::command]
pub async fn get_last_scan(state: State<'_, ScanState>) -> Result<Option<serde_json::Value>, String> {
    if state.demo {
        return Ok(Some(demo::last_scan()));
    }
    let p = state
        .last_scan
        .lock()
        .clone()
        .unwrap_or_else(|| dirs_home().join("last-scan.json"));
    if !p.exists() {
        return Ok(None);
    }
    let raw = std::fs::read(&p).map_err(|e| e.to_string())?;
    serde_json::from_slice(&raw)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Build a plan from the last scan (policy optional NL sentence or structured).
#[tauri::command]
pub async fn build_plan(
    scan_json_path: String,
    policy: Option<String>,
    min_bytes: Option<u64>,
    state: State<'_, ScanState>,
) -> Result<serde_json::Value, String> {
    if state.demo {
        return Ok(demo::plan());
    }
    let raw = std::fs::read(&scan_json_path).map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let candidates: Vec<disk_janitor::Candidate> =
        serde_json::from_value(parsed["candidates"].clone()).map_err(|e| e.to_string())?;
    let volumes = disk_janitor::scan::volumes();
    let mut pc = disk_janitor::plan::PolicyConstraintsShape::default();
    if let Some(sentence) = policy {
        pc = disk_janitor::policy::compile(sentence.as_str()).constraints;
    }
    if let Some(mb) = min_bytes {
        pc.min_bytes = mb;
    }
    let plan = disk_janitor::plan::build(&candidates, &pc, &volumes);
    let out_dir = dirs_home();
    let path = out_dir.join(format!("plan-{}.json", plan.plan_id));
    disk_janitor::plan::write_manifest(&plan, &path).map_err(|e| e.to_string())?;
    serde_json::to_value(&plan).map_err(|e| e.to_string())
}

/// Stream of exec events for the Activity screen.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ExecProgress {
    Applied { cid: String, to: String },
    Purged { cid: String, bytes: u64 },
    Restored { cid: String, to: String },
    Error { cid: String, err: String },
}

fn drain_exec(
    _ex: &mut disk_janitor::execute::Executor,
    rx: crossbeam_channel::Receiver<disk_janitor::execute::ExecEvent>,
    channel: &Channel<ExecProgress>,
) -> Result<serde_json::Value, String> {
    let mut applied = 0u32;
    let mut freed = 0u64;
    for ev in rx {
        match ev {
            disk_janitor::execute::ExecEvent::EntryApplied { cid, to } => {
                applied += 1;
                let _ = channel.send(ExecProgress::Applied { cid, to });
            }
            disk_janitor::execute::ExecEvent::EntryPurged { cid, bytes } => {
                freed += bytes;
                let _ = channel.send(ExecProgress::Purged { cid, bytes });
            }
            disk_janitor::execute::ExecEvent::EntryRestored { cid, to } => {
                let _ = channel.send(ExecProgress::Restored { cid, to });
            }
            disk_janitor::execute::ExecEvent::Error { cid, err } => {
                let _ = channel.send(ExecProgress::Error { cid, err });
            }
            disk_janitor::execute::ExecEvent::Done => break,
        }
    }
    Ok(serde_json::json!({ "applied": applied, "freed_bytes": freed }))
}

/// Volume table with firmlink dedup: for one st_dev keep the WRITABLE mount
/// (CLI parity — src/main.rs volume_table). Without this, precheck hits the
/// sealed-RO `/` spelling first and refuses every entry.
fn writable_vols() -> Vec<disk_janitor::VolumeInfo> {
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
    out
}

/// Apply the plan manifest (soft gate — reversible; SPEC §5.3).
#[tauri::command]
pub async fn apply_plan(
    plan_id: String,
    channel: Channel<ExecProgress>,
    state: State<'_, ScanState>,
) -> Result<serde_json::Value, String> {
    if state.demo {
        demo::stream_applied(&channel);
        return Ok(demo::apply_report());
    }
    let home = disk_janitor::execute::home_ledger_dir();
    let ledger = disk_janitor::ledger::open(&home).map_err(|e| e.to_string())?;
    let plan = find_plan(&plan_id)?;
    // B2: keep apply's newest-manifest fallback, but never act on a plan the
    // caller did not name — a fallback resolution MUST match the requested id.
    if plan.plan_id != plan_id {
        return Err(format!(
            "no plan manifest for {plan_id}; refusing to act on fallback plan {}",
            plan.plan_id
        ));
    }
    let vols = writable_vols();
    let entries: Vec<disk_janitor::PlanEntry> = plan
        .entries
        .iter()
        .filter(|e| matches!(e.action, disk_janitor::PlanAction::Quarantine))
        .cloned()
        .collect();
    let mut ex = disk_janitor::execute::open(plan.plan_id.clone(), entries, &vols, ledger)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = crossbeam_channel::unbounded();
    let report = ex.apply(tx, None).map_err(|e| e.to_string())?;
    drain_exec(&mut ex, rx, &channel)?;
    Ok(serde_json::json!({
        "op_id": report.op_id,
        "applied": report.applied,
        "skipped": report.skipped,
        "refused": report.refused,
        "quarantined_bytes": report.quarantined_bytes,
    }))
}

/// Purge: the ONLY irreversible step. `typed_bytes` must equal the quarantine
/// total (P4 — no bypass exists at any layer, including this one).
#[tauri::command]
pub async fn purge_plan(
    plan_id: String,
    typed_bytes: u64,
    channel: Channel<ExecProgress>,
    state: State<'_, ScanState>,
) -> Result<serde_json::Value, String> {
    if state.demo {
        // ipc.ts mirrors this with a 700 ms delay so the 销毁中… state is visible.
        std::thread::sleep(std::time::Duration::from_millis(700));
        return Ok(demo::purge_report(typed_bytes));
    }
    let home = disk_janitor::execute::home_ledger_dir();
    let ledger = disk_janitor::ledger::open(&home).map_err(|e| e.to_string())?;
    // B2: purge is irreversible — STRICT id lookup only. The newest-manifest
    // fallback would destroy a plan the user never reviewed.
    let plan = find_plan_strict(&plan_id)?;
    let vols = writable_vols();
    let entries: Vec<disk_janitor::PlanEntry> = plan
        .entries
        .iter()
        .filter(|e| matches!(e.action, disk_janitor::PlanAction::Quarantine))
        .cloned()
        .collect();
    let mut ex = disk_janitor::execute::open(plan.plan_id.clone(), entries, &vols, ledger)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = crossbeam_channel::unbounded();
    let report = ex.purge(typed_bytes, tx).map_err(|e| e.to_string())?;
    drain_exec(&mut ex, rx, &channel)?;
    Ok(serde_json::json!({ "freed_bytes": report.freed_bytes, "entries": report.entries }))
}

/// Restore a quarantined op (one click, within grace).
#[tauri::command]
pub async fn restore_op(
    op_id: String,
    channel: Channel<ExecProgress>,
    state: State<'_, ScanState>,
) -> Result<serde_json::Value, String> {
    if state.demo {
        return Ok(demo::restore_report());
    }
    let home = disk_janitor::execute::home_ledger_dir();
    let ledger = disk_janitor::ledger::open(&home).map_err(|e| e.to_string())?;
    let vols = writable_vols();
    let mut ex = disk_janitor::execute::open("restore".into(), Vec::new(), &vols, ledger)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = crossbeam_channel::unbounded();
    let report = ex.restore(&op_id, tx).map_err(|e| e.to_string())?;
    drain_exec(&mut ex, rx, &channel)?;
    Ok(serde_json::json!({
        "restored": report.restored,
        "missing": report.missing,
        "collided": report.collided,
    }))
}

/// Ledger tail for the 台账 screen (R5).
#[tauri::command]
pub async fn get_ledger_tail(
    limit: Option<usize>,
    state: State<'_, ScanState>,
) -> Result<Vec<serde_json::Value>, String> {
    if state.demo {
        return Ok(Vec::new());
    }
    let home = disk_janitor::execute::home_ledger_dir();
    let path = home.join("ledger.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let (records, _rr) = disk_janitor::ledger::replay(&path).map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(200);
    let tail: Vec<serde_json::Value> = records
        .iter()
        .rev()
        .take(limit)
        .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
        .collect();
    Ok(tail)
}

/// Startup recovery (§5.2): reconcile WALs, surface dangling intents + expired grace.
#[tauri::command]
pub async fn recover_on_launch(state: State<'_, ScanState>) -> Result<serde_json::Value, String> {
    if state.demo {
        return Ok(demo::recovery());
    }
    let vols = writable_vols();
    let rr = disk_janitor::execute::recover(&vols);
    Ok(serde_json::json!({
        "volumes_checked": rr.volumes_checked,
        "rolled_forward": rr.rolled_forward,
        "dangling_intents": rr.dangling_intents.len(),
        "expired_grace": rr.expired_grace.len(),
        "torn_tail": rr.torn_tail.map(|p| p.display().to_string()),
    }))
}

fn dirs_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".disk-janitor")
}

/// Locate a plan manifest by id in the state home. A missing id falls back to
/// the NEWEST manifest (apply's "last actioned plan" convenience) — callers
/// MUST NOT use this where acting on the wrong plan is irreversible.
fn find_plan(plan_id: &str) -> Result<disk_janitor::plan::Plan, String> {
    let dir = dirs_home();
    let candidate = dir.join(format!("plan-{plan_id}.json"));
    let path = if candidate.exists() {
        candidate
    } else {
        newest_manifest(&dir)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no plan manifest for {plan_id}"))?
    };
    disk_janitor::plan::load_manifest(&path).map_err(|e| e.to_string())
}

/// Strict id lookup — NO newest-manifest fallback (B2). A missing id is a hard
/// error that lists the available ids, so purge can never destroy a plan the
/// user did not review (P4, SPEC §5.3).
fn find_plan_strict(plan_id: &str) -> Result<disk_janitor::plan::Plan, String> {
    let dir = dirs_home();
    let candidate = dir.join(format!("plan-{plan_id}.json"));
    if !candidate.exists() {
        let mut ids: Vec<String> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let name = e.file_name();
                        let name = name.to_string_lossy();
                        name.strip_prefix("plan-")
                            .and_then(|s| s.strip_suffix(".json"))
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        let available = if ids.is_empty() {
            "none".to_string()
        } else {
            ids.join(", ")
        };
        return Err(format!(
            "no plan manifest for {plan_id} (available: {available})"
        ));
    }
    disk_janitor::plan::load_manifest(&candidate).map_err(|e| e.to_string())
}

/// Newest `plan-*.json` in the state home by mtime, if any.
fn newest_manifest(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("plan-") && name.ends_with(".json") {
            let md = entry.metadata()?;
            let modified = md.modified()?;
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, entry.path()));
            }
        }
    }
    Ok(best.map(|(_, p)| p))
}

// ── Cluster B tests: purge gate honesty (B2) ─────────────────────────────────
//
// find_plan_strict (purge's ONLY lookup — purge is irreversible) must never
// fall back to the newest manifest: a missing id is a hard error listing the
// available ids. find_plan keeps its fallback for apply, and apply_plan's
// resolved-id guard rejects any fallback resolution that does not match the
// requested id.
#[cfg(test)]
mod purge_gate_tests {
    use super::*;
    use std::sync::Mutex;

    /// HOME is process-global; dirs::home_dir() reads it, so every test here
    /// serializes and restores the real value on drop.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    /// Restores the real HOME when dropped.
    struct HomeGuard(Option<std::ffi::OsString>);

    impl HomeGuard {
        fn set(dir: &Path) -> Self {
            let prev = std::env::var_os("HOME");
            // edition 2024: env mutation is unsafe.
            unsafe { std::env::set_var("HOME", dir) };
            Self(prev)
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => unsafe { std::env::set_var("HOME", v) },
                None => unsafe { std::env::remove_var("HOME") },
            }
        }
    }

    /// Minimal valid manifest at `$HOME/.disk-janitor/plan-<id>.json`.
    fn write_manifest(home: &Path, id: &str) {
        let dir = home.join(".disk-janitor");
        std::fs::create_dir_all(&dir).unwrap();
        let json = format!(
            r#"{{"plan_id":"{id}","created_at":"2026-09-10T00:00:00Z","entries":[],"totals":{{"quarantine_bytes":0,"quarantine_entries":0,"prune_bytes":0,"prune_cmds":0,"refused":0,"snapshot_caveat":null}},"policy_snapshot":{{"target_bytes":null,"min_bytes":1,"allow_grades":["pure_cache"],"deny_globs":[],"deny_under_mta_secs":0,"allow_network_regen":true,"allow_prune_commands":true,"grace_hours":72,"max_entries":1000}}}}"#
        );
        std::fs::write(dir.join(format!("plan-{id}.json")), json).unwrap();
    }

    /// purge_plan must only act on the exact requested id: a missing id is a
    /// hard error that lists the available ids — no newest-manifest fallback.
    #[test]
    fn strict_lookup_errors_on_missing_id_and_lists_available_ids() {
        let _g = HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let home = std::env::temp_dir().join(format!("dj-purge-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let _h = HomeGuard::set(&home);
        write_manifest(&home, "plan-aaa");
        write_manifest(&home, "plan-bbb");

        let plan = find_plan_strict("plan-aaa").expect("exact id loads");
        assert_eq!(plan.plan_id, "plan-aaa");

        let err = find_plan_strict("plan-missing").expect_err("missing id must fail");
        assert!(err.contains("plan-missing"), "names the missing id: {err}");
        assert!(err.contains("plan-aaa"), "lists available ids: {err}");
        assert!(err.contains("plan-bbb"), "lists available ids: {err}");
        std::fs::remove_dir_all(&home).ok();
    }

    /// apply keeps the newest-manifest fallback, but apply_plan's guard then
    /// refuses to act: resolved plan_id != requested ⇒ Err.
    #[test]
    fn fallback_resolves_newest_but_guard_rejects_id_mismatch() {
        let _g = HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let home = std::env::temp_dir().join(format!("dj-purge-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let _h = HomeGuard::set(&home);
        write_manifest(&home, "plan-aaa");
        // APFS mtime resolution is ns; the sleep guarantees plan-bbb is newest.
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_manifest(&home, "plan-bbb");

        let plan = find_plan("plan-missing").expect("fallback resolves the newest");
        assert_eq!(plan.plan_id, "plan-bbb");
        // Same guard as apply_plan: requested id must equal the resolved id.
        let requested = "plan-missing";
        assert_ne!(requested, plan.plan_id);
        assert!(
            !find_plan_strict("plan-missing").is_ok(),
            "strict path has no fallback"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}

// ── D-38: --demo-data synthetic payloads (P7 reproducible screen recordings) ──
//
// Every builder mirrors the browser fallback constants in app/src/lib/ipc.ts
// (DEMO_CANDIDATES + the `demo()` switch) — same ids, same numbers, same shapes,
// no invented values. Keep the two in sync when either changes.

mod demo {
    use super::{ExecProgress, ScanProgress, VolumeDto};
    use serde_json::{json, Value};
    use tauri::ipc::Channel;

    /// DEMO_CANDIDATES from ipc.ts (manifest shape: path/alloc_bytes).
    pub fn candidates() -> Vec<Value> {
        vec![
            json!({
                "id": "c1",
                "path": "~/code/api/node_modules",
                "path_hash": "h1",
                "alloc_bytes": 2_100_000_000u64,
                "grade": "rebuildable_from_source",
                "evidence": [
                    { "kind": "LockfilePresent", "detail": "package-lock.json newer than dir", "strength": "Strong" },
                    { "kind": "GitRepoAncestor", "detail": "HEAD 3 h ago", "strength": "Strong" },
                    { "kind": "Unreadable", "detail": "", "strength": "Weak" },
                ],
            }),
            json!({
                "id": "c2",
                "path": "~/Library/Developer/Xcode/DerivedData",
                "path_hash": "h2",
                "alloc_bytes": 6_800_000_000u64,
                "grade": "rebuildable_from_source",
                "evidence": [
                    { "kind": "LockfilePresent", "detail": "xcuserdata lockfile present", "strength": "Strong" },
                    { "kind": "BuildArtifactShape", "detail": "9 d 2 h since build", "strength": "Medium" },
                ],
            }),
            json!({
                "id": "c3",
                "path": "~/.npm/_cacache",
                "path_hash": "h3",
                "alloc_bytes": 3_200_000_000u64,
                "grade": "pure_cache",
                "evidence": [
                    { "kind": "CasStore", "detail": "content-addressed store, self-healing", "strength": "Strong" },
                ],
            }),
            json!({
                "id": "c4",
                "path": "~/Downloads/old-installers",
                "path_hash": "h4",
                "alloc_bytes": 1_500_000_000u64,
                "grade": "redownloadable",
                "evidence": [
                    { "kind": "SiblingManifestMissing", "detail": "vendor installers, dmg/pkg", "strength": "Medium" },
                ],
            }),
            json!({
                "id": "c5",
                "path": "~/code/legacy/vendor",
                "path_hash": "h5",
                "alloc_bytes": 4_900_000_000u64,
                "grade": "expensive",
                "evidence": [
                    { "kind": "OrphanTree", "detail": "no manifest in ancestors", "strength": "Medium" },
                ],
            }),
            json!({
                "id": "c6",
                "path": "~/code/api/node_modules/.cache",
                "path_hash": "h6",
                "alloc_bytes": 400_000_000u64,
                "grade": "hands_off",
                "evidence": [
                    { "kind": "MtimeHot", "detail": "written 4 min ago", "strength": "Strong" },
                ],
            }),
        ]
    }

    /// get_volumes demo — one writable APFS root (ipc.ts get_volumes case).
    pub fn volumes() -> Vec<VolumeDto> {
        vec![VolumeDto {
            mount: "/".into(),
            fs: "APFS".into(),
            total: 1_000_000_000_000,
            free_raw: 387_600_000_000,
            read_only: false,
            writable: true,
        }]
    }

    /// get_last_scan demo envelope (ipc.ts get_last_scan case).
    pub fn last_scan() -> Value {
        json!({
            "roots": ["/Users/me"],
            "min_bytes": 52_428_800u64,
            "candidates": candidates(),
        })
    }

    /// build_plan demo plan (ipc.ts build_plan case; hands_off candidate excluded).
    pub fn plan() -> Value {
        let entries: Vec<Value> = candidates()
            .into_iter()
            .filter(|c| c.get("grade").and_then(Value::as_str) != Some("hands_off"))
            .map(|c| {
                json!({
                    "cid": c["id"].clone(),
                    "path": c["path"].clone(),
                    "path_hash": c["path_hash"].clone(),
                    "action": { "type": "quarantine" },
                    "grade": c["grade"].clone(),
                    "alloc_bytes": c["alloc_bytes"].clone(),
                })
            })
            .collect();
        json!({
            "plan_id": "plan-demo-20260909",
            "created_at": now_iso(),
            "entries": entries,
            "totals": {
                "quarantine_bytes": 19_300_000_000u64,
                "quarantine_entries": 11u64,
                "prune_bytes": 1_600_000_000u64,
                "prune_cmds": 2u64,
                "refused": 3u64,
                "snapshot_caveat": Value::Null,
            },
            "policy_snapshot": {},
        })
    }

    /// apply_plan demo report (ipc.ts apply_plan case).
    pub fn apply_report() -> Value {
        json!({
            "op_id": "op-demo",
            "applied": 11u64,
            "skipped": 0u64,
            "refused": 3u64,
            "quarantined_bytes": 19_300_000_000u64,
        })
    }

    /// purge_plan demo report (ipc.ts purge_plan case) — freed bytes == typed bytes.
    pub fn purge_report(typed_bytes: u64) -> Value {
        json!({ "ok": true, "bytes_freed": typed_bytes })
    }

    /// restore_op demo report (ipc.ts restore_op case).
    pub fn restore_report() -> Value {
        json!({ "ok": true, "restored": 1u64 })
    }

    /// recover_on_launch demo report (ipc.ts recover_on_launch case).
    pub fn recovery() -> Value {
        json!({ "resumed": Value::Null, "dangling": 0u64, "expired": 0u64 })
    }

    /// start_scan demo: stream Files ticks + Finished over the channel (ipc.ts
    /// start_scan case timings — ticks at 0/320/640/960/1280 ms, finished 240 ms
    /// after the last) so the ring animates, then resolve with the demo path.
    pub fn stream_scan(channel: &Channel<ScanProgress>) {
        const TOTAL_FILES: u64 = 1_204_553;
        const TOTAL_BYTES: u64 = 900_000_000_000;
        const TICKS: [(f64, &str); 5] = [
            (0.12, "~/code/api"),
            (0.31, "~/Library/Developer"),
            (0.55, "~/Library/Caches"),
            (0.74, "~/.npm"),
            (0.93, "~/Downloads"),
        ];
        for (i, (frac, path)) in TICKS.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(320));
            }
            let _ = channel.send(ScanProgress::Files {
                files: (TOTAL_FILES as f64 * frac).round() as u64,
                bytes: (TOTAL_BYTES as f64 * frac).round() as u64,
                rate_bps: 1_300_000_000.0,
                current: (*path).into(),
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(240));
        let _ = channel.send(ScanProgress::Finished {
            files: TOTAL_FILES,
            bytes: TOTAL_BYTES,
            elapsed_ms: 42_000,
        });
    }

    /// apply_plan demo: stream an EntryApplied event per plan entry so the
    /// Activity screen animates (D-38; ipc.ts carries no stream — Rust-side only).
    pub fn stream_applied(channel: &Channel<ExecProgress>) {
        for cid in ["c1", "c2", "c3", "c4", "c5"] {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let _ = channel.send(ExecProgress::Applied {
                cid: cid.into(),
                to: format!("/Users/me/.disk-janitor/quarantine/{cid}"),
            });
        }
    }

    /// RFC3339 UTC timestamp with millis — same shape as `new Date().toISOString()`
    /// and chrono's serde output; std-only (chrono is not a dep of this crate and
    /// D-38 forbids new deps).
    fn now_iso() -> String {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let days = d.as_secs() / 86_400;
        let rem = d.as_secs() % 86_400;
        let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
        // Civil-from-days (Hinnant): days since epoch → y/m/d in the proleptic
        // Gregorian calendar.
        let z = days as i64 + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let dd = doy - (153 * mp + 2) / 5 + 1;
        let mm = if mp < 10 { mp + 3 } else { mp - 9 };
        let yy = if mm <= 2 { y + 1 } else { y };
        format!("{yy:04}-{mm:02}-{dd:02}T{h:02}:{m:02}:{s:02}.{:03}Z", d.subsec_millis())
    }
}

// ── Cluster A tests: cancel/interrupt chain (A1–A4) ──────────────────────────
//
// These run the REAL walker over tiny temp trees (dua-core is fast enough) and
// exercise `run_scan` — the exact pipeline `start_scan` delegates to — through
// real `tauri::ipc::Channel`s, so they cover the cancel wiring, the re-entry
// guard, the abandoned-scan path, and the persist rules end to end.
#[cfg(test)]
mod tests {
    use super::*;
    use tauri::ipc::InvokeResponseBody;

    /// Unique temp dir removed on drop (best-effort).
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("dj-scan-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).expect("create scratch dir");
            Self(p)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        /// A scan root under this scratch dir with `files` 1 KB files inside a
        /// `target` dir — a recognized junk shape (RustTarget) so classify
        /// yields candidates at min_bytes = 1 (every file is above the gate).
        fn tree(&self, files: usize) -> PathBuf {
            let target = self.0.join("tree").join("target");
            for i in 0..files {
                std::fs::create_dir_all(&target).expect("create tree dir");
                std::fs::write(target.join(format!("f{i}.txt")), vec![b'x'; 1024]).expect("write file");
            }
            target.parent().expect("tree root").to_path_buf()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Decode a channel callback body into the JSON text it carried.
    fn body_json(body: &InvokeResponseBody) -> String {
        match body {
            InvokeResponseBody::Json(s) => s.clone(),
            InvokeResponseBody::Raw(b) => String::from_utf8_lossy(b).into_owned(),
        }
    }

    /// Channel that records every event's JSON text into `log`.
    fn recorder() -> (Channel<ScanProgress>, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = log.clone();
        let ch = Channel::new(move |body: InvokeResponseBody| {
            sink.lock().push(body_json(&body));
            Ok(())
        });
        (ch, log)
    }

    /// Channel whose sends always fail — simulates the webview going away (A3).
    fn closed_channel() -> Channel<ScanProgress> {
        Channel::new(|_body: InvokeResponseBody| {
            Err(tauri::Error::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "test: channel closed",
            )))
        })
    }

    /// Gate that blocks sends while held (holds a scan in flight for re-entry).
    struct Gate(AtomicBool);

    impl Gate {
        fn new() -> Self {
            Self(AtomicBool::new(true))
        }

        fn held(&self) -> bool {
            self.0.load(Ordering::Relaxed)
        }

        fn release(&self) {
            self.0.store(false, Ordering::Relaxed);
        }
    }

    fn gated_channel(gate: &Arc<Gate>) -> Channel<ScanProgress> {
        let gate = gate.clone();
        Channel::new(move |_body: InvokeResponseBody| {
            while gate.held() {
                std::thread::yield_now();
            }
            Ok(())
        })
    }

    /// Run the real scan pipeline over `root` into scratch `out`.
    fn run(root: &Path, ch: &Channel<ScanProgress>, state: &ScanState, out: &Scratch) -> Result<PathBuf, String> {
        run_scan(
            vec![root.display().to_string()],
            Some(1),
            ch,
            state,
            out.path(),
        )
    }

    /// The `files` total carried by the Finished event, if one was sent.
    fn finished_files(log: &[String]) -> Option<u64> {
        log.iter()
            .find(|m| m.contains("\"t\":\"finished\""))
            .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
            .and_then(|v| v["files"].as_u64())
    }

    #[test]
    fn completed_scan_streams_candidates_finished_and_persists() {
        let s = Scratch::new();
        let root = s.tree(64);
        let out = Scratch::new();
        let state = ScanState::default();
        let (ch, log) = recorder();

        let path = run(&root, &ch, &state, &out).expect("completed scan succeeds");

        assert_eq!(path, out.path().join("last-scan.json"));
        assert!(path.exists(), "last-scan.json must be written");
        assert_eq!(*state.last_scan.lock(), Some(path.clone()));
        assert!(!state.scanning.load(Ordering::SeqCst), "guard released after completion");

        let env: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(env["candidates"].is_array());
        assert!(
            !env["candidates"].as_array().unwrap().is_empty(),
            "1 KB files are candidates at min_bytes = 1"
        );

        let msgs = log.lock();
        assert!(
            msgs.iter().any(|m| m.contains("\"t\":\"candidate\"")),
            "candidates streamed before Finished"
        );
        assert_eq!(finished_files(&msgs), Some(64), "Finished carries the walker totals");
    }

    #[test]
    fn cancelled_scan_stops_walker_and_skips_persist() {
        let s = Scratch::new();
        let root = s.tree(256);
        let out = Scratch::new();
        let state = ScanState::default();
        let (ch, log) = recorder();

        // Pre-set the SHARED flag — the same Arc the walker polls (A1). With
        // the old disconnected-Arc bug the walker would ignore this, count all
        // 256 files and write last-scan.json; both assertions below fail then.
        state.cancel.store(true, Ordering::SeqCst);

        let res = run(&root, &ch, &state, &out);
        assert!(res.is_err(), "cancelled with no previous scan must error, got {res:?}");
        assert!(
            res.unwrap_err().contains("no completed scan"),
            "error must explain there is nothing to reopen"
        );
        assert!(
            !out.path().join("last-scan.json").exists(),
            "cancelled scan must not write last-scan.json"
        );
        assert!(state.last_scan.lock().is_none(), "last_scan untouched");
        assert!(!state.scanning.load(Ordering::SeqCst), "guard released after cancel");

        let msgs = log.lock();
        // A1 proof: the walker stopped at its very first entry — Finished with
        // zero files. A disconnected Arc would report all 256.
        assert_eq!(
            finished_files(&msgs),
            Some(0),
            "walker must poll the SHARED cancel Arc"
        );
        // A4: incomplete batches are dropped — no classify, no candidates.
        assert!(
            !msgs.iter().any(|m| m.contains("\"t\":\"candidate\"")),
            "cancelled scan must not classify/stream candidates"
        );
        // A4: Finished still sent so the frontend can navigate.
        assert!(
            msgs.iter().any(|m| m.contains("\"t\":\"finished\"")),
            "Finished must still be sent on cancel"
        );
    }

    #[test]
    fn cancelled_scan_resolves_to_previous_completed_scan() {
        let s = Scratch::new();
        let root = s.tree(32);
        let out = Scratch::new();
        let state = ScanState::default();
        let (ch, _log) = recorder();
        // D-15: a previous COMPLETED scan exists and must be kept as-is.
        let prev = out.path().join("last-scan.json");
        std::fs::write(&prev, br#"{"keep":"me"}"#).unwrap();
        *state.last_scan.lock() = Some(prev.clone());

        state.cancel.store(true, Ordering::SeqCst);
        let path = run(&root, &ch, &state, &out).expect("resolves to the previous scan");

        assert_eq!(path, prev, "frontend navigates to the last COMPLETED scan");
        assert_eq!(*state.last_scan.lock(), Some(prev.clone()), "last_scan unchanged");
        assert_eq!(
            std::fs::read(&prev).unwrap(),
            br#"{"keep":"me"}"#,
            "previous scan file untouched"
        );
        assert!(!state.scanning.load(Ordering::SeqCst), "guard released");
    }

    #[test]
    fn scanning_guard_rejects_reentry() {
        let s = Scratch::new();
        let root = s.tree(16);
        let out = Scratch::new();
        let state = Arc::new(ScanState::default());
        let gate = Arc::new(Gate::new());

        // First scan runs on a thread and blocks at its Finished send on the
        // gate — holding the scanning flag until we release it.
        let t_root = root.clone();
        let t_state = state.clone();
        let t_ch = gated_channel(&gate);
        let holder = std::thread::spawn(move || {
            let out = Scratch::new();
            let path = run(&t_root, &t_ch, &t_state, &out)?;
            // Check while `out` is still alive — it is removed on drop.
            assert!(path.exists(), "first scan persisted normally");
            Ok::<PathBuf, String>(path)
        });

        // Wait until the first scan actually holds the flag.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !state.scanning.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            state.scanning.load(Ordering::SeqCst),
            "first scan must hold the scanning flag"
        );

        // A second scan while one is in flight: refused before it can start a
        // walker, touch the channel, or write anything.
        let (ch2, _log2) = recorder();
        let err = run(&root, &ch2, &state, &out).expect_err("re-entry must be refused");
        assert!(err.contains("already in progress"), "got: {err}");
        assert!(
            !out.path().join("last-scan.json").exists(),
            "refused scan must not write anything"
        );

        gate.release();
        holder
            .join()
            .expect("first scan completes after gate release")
            .expect("first scan succeeded");
        assert!(!state.scanning.load(Ordering::SeqCst), "guard released after completion");
    }

    #[test]
    fn closed_channel_abandons_scan_without_persist() {
        let s = Scratch::new();
        let root = s.tree(64);
        let out = Scratch::new();
        let state = ScanState::default();

        // Every send fails, as if the webview navigated away mid-scan (A3).
        let ch = closed_channel();
        let err = run(&root, &ch, &state, &out).expect_err("abandoned scan must error");
        assert!(err.contains("abandoned"), "got: {err}");
        assert!(
            !out.path().join("last-scan.json").exists(),
            "abandoned scan must not write last-scan.json"
        );
        assert!(state.last_scan.lock().is_none(), "last_scan untouched");
        assert!(!state.scanning.load(Ordering::SeqCst), "guard released on abandon");
    }
}
