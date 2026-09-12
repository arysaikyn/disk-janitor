//! `scan/` — parallel metadata-first walker (SPEC Part B §3.1).
//! OWNER: scan agent. macOS walker = dua-core 4.0.0 `walk_roots(Order::ParentFirst)`;
//! `setiopolicy_np` extern "C"; `VolumeInfo` via sysinfo; emits `ScanEvent` batches.

use std::sync::Arc;
use std::{path::PathBuf, time::SystemTime};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use crate::{PathHash, VolumeInfo, path_hash};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirStat {
    pub dev: u64,
    pub ino: u64,
    pub parent: PathBuf,
    pub name: String,
    pub depth: u16,
    pub kind: Kind,
    pub app_bytes: u64,
    pub alloc_bytes: u64,
    pub entries: u32,
    pub mtime: SystemTime,
    pub flags: u32,
    pub nlink: u32,
    pub clone_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Dir,
    File,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanEvent {
    Batch(Vec<DirStat>),
    RootError { root: PathBuf, err: String },
    Finished(ScanTotals),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanTotals {
    pub files: u64,
    pub alloc: u64,
    pub elapsed_ms: u64,
    pub snapshots: Vec<String>,
    pub volumes: Vec<VolumeInfo>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    pub threads: usize,
    pub cross_filesystems: bool,
    pub ignore: Vec<String>,
    pub min_dir_bytes: u64,
}

/// Aggregates `DirStat` subtree totals per directory while streaming batches downstream.
#[derive(Debug, Clone, Default)]
pub struct DirAgg {
    pub app_bytes: u64,
    pub alloc_bytes: u64,
    pub entries: u32,
    /// Max mtime in the subtree — the MtimeHot signal source.
    pub max_mtime: Option<SystemTime>,
    pub path_hash: PathHash,
}

impl DirAgg {
    pub fn fold_file(&mut self, app: u64, alloc: u64, mtime: SystemTime) {
        self.app_bytes += app;
        self.alloc_bytes += alloc;
        self.entries += 1;
        self.max_mtime = Some(self.max_mtime.map_or(mtime, |m| m.max(mtime)));
    }
}

/// FNV hash a DirStat's full path (parent + name).
pub fn dirstat_path_hash(d: &DirStat) -> PathHash {
    let mut full = d.parent.clone();
    full.push(&d.name);
    path_hash(&full)
}

pub mod platform;

/// Start the scan thread — SPEC §3.1 signature.
pub fn start(
    opts: ScanOptions,
    tx: Sender<ScanEvent>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    stats: Arc<ScanStats>,
) -> std::thread::JoinHandle<ScanTotals> {
    platform::start(opts, tx, cancel, stats)
}

/// Shared atomic scan stats (SPEC §2.2).
#[derive(Debug, Default)]
pub struct ScanStats {
    pub entries: std::sync::atomic::AtomicU64,
    pub bytes: std::sync::atomic::AtomicU64,
    pub started: std::sync::OnceLock<SystemTime>,
}

/// List volumes — SPEC §3.1 `volumes()`; sysinfo Disks, ImportantUsage preferred on macOS.
pub fn volumes() -> Vec<VolumeInfo> {
    platform::volumes()
}
