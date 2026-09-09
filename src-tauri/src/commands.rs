//! Named IPC commands (SPEC Part C §2.3): ~10 commands, typed payloads, no raw fs.
//! Scan/apply progress cross the boundary via `tauri::ipc::Channel<T>`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

/// Shared scan session state (cancel flag + last result path).
pub struct ScanState {
    pub cancel: AtomicBool,
    pub last_scan: Mutex<Option<PathBuf>>,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            last_scan: Mutex::new(None),
        }
    }
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
pub fn get_volumes() -> Vec<VolumeDto> {
    disk_janitor::scan::volumes()
        .into_iter()
        .map(|v| VolumeDto {
            mount: v.mount.display().to_string(),
            fs: v.fs,
            total: v.total,
            free_raw: v.free_raw,
            read_only: v.read_only,
            writable: !v.read_only,
        })
        .collect()
}

/// Start a scan of `roots`, streaming batches + candidates over `channel`.
/// Cancel via `cancel_scan` (R4; D-15: drop incomplete batches, keep completed).
#[tauri::command]
pub fn start_scan(
    roots: Vec<String>,
    min_bytes: Option<u64>,
    channel: Channel<ScanProgress>,
    state: State<'_, ScanState>,
) -> Result<String, String> {
    state.cancel.store(false, Ordering::SeqCst);
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
    let cancel = Arc::new(AtomicBool::new(false));
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
    let mut last_emit = std::time::Instant::now();
    for event in rx {
        match event {
            disk_janitor::scan::ScanEvent::Batch(b) => {
                batches.extend(b);
                if last_emit.elapsed().as_millis() >= 100 {
                    last_emit = std::time::Instant::now();
                    let files = stats.entries.load(Ordering::Relaxed);
                    let bytes = stats.bytes.load(Ordering::Relaxed);
                    let rate = bytes as f64 / started.elapsed().as_secs_f64().max(0.001);
                    let _ = channel.send(ScanProgress::Files {
                        files,
                        bytes,
                        rate_bps: rate,
                        current: String::new(),
                    });
                }
            }
            disk_janitor::scan::ScanEvent::RootError { root, err } => {
                let _ = channel.send(ScanProgress::RootError {
                    root: root.display().to_string(),
                    err,
                });
            }
            disk_janitor::scan::ScanEvent::Finished(t) => {
                let _ = channel.send(ScanProgress::Finished {
                    files: t.files,
                    bytes: t.alloc,
                    elapsed_ms: t.elapsed_ms,
                });
                break;
            }
        }
    }
    let _ = handle.join();
    // Classify from the collected batches, emit candidates as they're found.
    let th = disk_janitor::Thresholds {
        min_bytes: min_bytes.unwrap_or(disk_janitor::Thresholds::default().min_bytes),
        ..disk_janitor::Thresholds::default()
    };
    let candidates =
        disk_janitor::classify::candidates_from_batch(std::mem::take(&mut batches), &th);
    for c in &candidates {
        let _ = channel.send(ScanProgress::Candidate {
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
        });
    }
    // Persist last scan (owner decision 09-08: get_last_scan reopens without rescan).
    let out_dir = dirs_home();
    let scan_path = out_dir.join("last-scan.json");
    let envelope = serde_json::json!({
        "roots": roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        "min_bytes": th.min_bytes,
        "candidates": candidates,
    });
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    std::fs::write(
        &scan_path,
        serde_json::to_vec_pretty(&envelope).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    *state.last_scan.lock().map_err(|p| p.to_string())? = Some(scan_path.clone());
    Ok(scan_path.display().to_string())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, ScanState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Reopen the last scan result without rescanning (SPEC §2.3 persistence row).
#[tauri::command]
pub fn get_last_scan(state: State<'_, ScanState>) -> Result<Option<serde_json::Value>, String> {
    let p = state
        .last_scan
        .lock()
        .map_err(|e| e.to_string())?
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
pub fn build_plan(
    scan_json_path: String,
    policy: Option<String>,
    min_bytes: Option<u64>,
) -> Result<serde_json::Value, String> {
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
pub fn apply_plan(
    plan_id: String,
    channel: Channel<ExecProgress>,
) -> Result<serde_json::Value, String> {
    let home = disk_janitor::execute::home_ledger_dir();
    let ledger = disk_janitor::ledger::open(&home).map_err(|e| e.to_string())?;
    let plan = find_plan(&plan_id)?;
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
pub fn purge_plan(
    plan_id: String,
    typed_bytes: u64,
    channel: Channel<ExecProgress>,
) -> Result<serde_json::Value, String> {
    let home = disk_janitor::execute::home_ledger_dir();
    let ledger = disk_janitor::ledger::open(&home).map_err(|e| e.to_string())?;
    let plan = find_plan(&plan_id)?;
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
pub fn restore_op(
    op_id: String,
    channel: Channel<ExecProgress>,
) -> Result<serde_json::Value, String> {
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
pub fn get_ledger_tail(limit: Option<usize>) -> Result<Vec<serde_json::Value>, String> {
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
pub fn recover_on_launch() -> Result<serde_json::Value, String> {
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

/// Locate a plan manifest by id in the state home.
fn find_plan(plan_id: &str) -> Result<disk_janitor::plan::Plan, String> {
    let dir = dirs_home();
    let candidate = dir.join(format!("plan-{plan_id}.json"));
    let path = if candidate.exists() {
        candidate
    } else {
        // Fall back: newest plan-*.json in the home (last actioned plan).
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("plan-") && name.ends_with(".json") {
                let md = entry.metadata().map_err(|e| e.to_string())?;
                let modified = md.modified().map_err(|e| e.to_string())?;
                if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                    best = Some((modified, entry.path()));
                }
            }
        }
        best.ok_or_else(|| format!("no plan manifest for {plan_id}"))?
            .1
    };
    disk_janitor::plan::load_manifest(&path).map_err(|e| e.to_string())
}
