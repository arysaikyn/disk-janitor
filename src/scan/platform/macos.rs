//! macOS scan implementation: dua-core 3.3.0 walker + setiopolicy_np + sysinfo volumes.

use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Sender;

use super::{DirStat, Kind, ScanEvent, ScanOptions, ScanStats, ScanTotals};
use crate::VolumeInfo;

/// Constants from xnu bsd/sys/resource.h (verified [04 risks]): libc does not export them.
const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: libc::c_int = 3;
const IOPOL_SCOPE_PROCESS: libc::c_int = 0;
const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: libc::c_int = 1;

unsafe extern "C" {
    fn setiopolicy_np(iotype: libc::c_int, scope: libc::c_int, policy: libc::c_int) -> libc::c_int;
}

/// Set process-wide dataless-materialization OFF (idempotent; inert without iCloud files).
pub fn set_dataless_off() -> libc::c_int {
    unsafe {
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_PROCESS,
            IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
        )
    }
}

pub fn start(
    opts: ScanOptions,
    tx: Sender<ScanEvent>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    stats: Arc<ScanStats>,
) -> std::thread::JoinHandle<ScanTotals> {
    std::thread::Builder::new()
        .name("janitor-scan".into())
        .spawn(move || run(opts, tx, cancel, stats))
        .expect("spawn scan thread")
}

fn run(
    opts: ScanOptions,
    tx: Sender<ScanEvent>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    stats: Arc<ScanStats>,
) -> ScanTotals {
    set_dataless_off();
    let started = Instant::now();
    stats.started.set(std::time::SystemTime::now()).ok();
    let vols = volumes();
    let threads = if opts.threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        opts.threads
    };
    // dua-core walk: per-entry dev compare in descend (mount skipping, firmlink-safe),
    // drop of the iterator = cancel + join (R4 primitive).
    let roots: Vec<(usize, std::path::PathBuf)> = opts.roots.iter().cloned().enumerate().collect();
    let mut ignore_builder = globset::GlobSet::builder();
    for g in &opts.ignore {
        if let Ok(glob) = globset::Glob::new(g) {
            ignore_builder.add(glob);
        }
    }
    let ignore_set = ignore_builder.build().ok();
    let first_dev: Option<u64> = opts
        .roots
        .first()
        .and_then(|p| std::fs::symlink_metadata(p).ok())
        .map(|md| std::os::unix::fs::MetadataExt::dev(&md));
    let walk_opts = dua_core::Options {
        apfs_clone_metadata: true,
    };
    let mut totals_files: u64 = 0;
    let mut totals_alloc: u64 = 0;
    {
        let _guard = tx.clone();
        let iter = dua_core::walk_roots(
            roots,
            threads,
            dua_core::Order::ParentFirst,
            walk_opts,
            move |_root_idx, entry| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return false;
                }
                let md = match &entry.metadata {
                    Ok(md) => *md,
                    Err(_) => return false,
                };
                if !entry.file_type.is_dir() {
                    return false;
                }
                if let Some(dev) = first_dev
                    && md.dev() != dev
                {
                    return false; // cross-filesystem reject — firmlink-safe st_dev check
                }
                if let Some(set) = &ignore_set
                    && set.is_match(entry.file_name.to_string_lossy().as_ref())
                {
                    return false;
                }
                true
            },
        );
        let mut batch: Vec<DirStat> = Vec::with_capacity(256);
        for (root_idx, event) in iter {
            match event {
                dua_core::RootEvent::Entry(Ok(entry)) => {
                    let Ok(md) = entry.metadata else { continue };
                    let kind = if entry.file_type.is_dir() {
                        Kind::Dir
                    } else if entry.file_type.is_symlink() {
                        Kind::Symlink
                    } else {
                        Kind::File
                    };
                    stats
                        .entries
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if kind == Kind::File {
                        totals_files += 1;
                        totals_alloc += md.allocated_size();
                        stats
                            .bytes
                            .fetch_add(md.allocated_size(), std::sync::atomic::Ordering::Relaxed);
                    }
                    batch.push(DirStat {
                        dev: md.dev(),
                        ino: md.ino(),
                        parent: (*entry.parent_path).to_path_buf(),
                        name: entry.file_name.to_string_lossy().into_owned(),
                        depth: u16::try_from(entry.depth).unwrap_or(u16::MAX),
                        kind,
                        app_bytes: md.len(),
                        alloc_bytes: md.allocated_size(),
                        entries: 0,
                        mtime: md.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        flags: 0, // dua-core does not surface BSD flags; classify re-probes
                        nlink: u32::try_from(md.nlink()).unwrap_or(0),
                        clone_id: md.clone_id().map(|c| c.get()),
                    });
                    let _ = root_idx;
                    if batch.len() >= 256
                        && tx
                            .send(ScanEvent::Batch(std::mem::take(&mut batch)))
                            .is_err()
                    {
                        break; // receiver gone: user quit — dropping iter cancels the walk
                    }
                }
                dua_core::RootEvent::Entry(Err(err)) => {
                    let _ = tx.send(ScanEvent::RootError {
                        root: opts.roots.first().cloned().unwrap_or_default(),
                        err: err.to_string(),
                    });
                }
                dua_core::RootEvent::Finished => {}
            }
        }
        if !batch.is_empty() {
            let _ = tx.send(ScanEvent::Batch(batch));
        }
    } // iter dropped here: dua-core stops + joins workers (cancel primitive)
    ScanTotals {
        files: totals_files,
        alloc: totals_alloc,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        snapshots: local_snapshots(),
        volumes: vols,
    }
}

/// `tmutil listlocalsnapshots /` for freed-space honesty (SPEC §3.1).
fn local_snapshots() -> Vec<String> {
    std::process::Command::new("tmutil")
        .args(["listlocalsnapshots", "/"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.strip_prefix("com.apple.TimeMachine."))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn volumes() -> Vec<VolumeInfo> {
    use sysinfo::{Disks, RefreshKind};
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for d in disks.list() {
        let mount = d.mount_point().to_path_buf();
        let dev = dev_of(&mount).unwrap_or(0);
        // rename_excl probe: APFS/HFS+ support VOL_CAP_INT_RENAME_EXCL; exFAT/FAT do not.
        let fs = d.file_system().to_string_lossy().into_owned();
        let rename_excl = matches!(fs.as_str(), "apfs" | "hfs" | "apfs (case-sensitive)");
        out.push(VolumeInfo {
            dev,
            mount,
            fs,
            total: d.total_space(),
            free_raw: d.available_space(),
            free_important: d.available_space(), // sysinfo ImportantUsage preferred on macOS
            free_opportunistic: 0,
            read_only: d.is_read_only(),
            rename_excl,
        });
    }
    let _ = RefreshKind::nothing(); // keep import honest; trimmed refresh in M2 perf pass
    out
}

fn dev_of(path: &std::path::Path) -> Option<u64> {
    std::os::unix::fs::MetadataExt::dev(&std::fs::metadata(path).ok()?).into()
}
