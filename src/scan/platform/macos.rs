//! macOS scan implementation: dua-core 4.0.0 walker + setiopolicy_np + sysinfo volumes.

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
    // Per-root device table, indexed like `roots`. Comparing every entry against the
    // FIRST root's device made the prune order-dependent: `scan /tmp /Volumes/X` kept
    // only /tmp's tree and dropped all of X, while the reverse argument kept only X.
    // A root's own device is what "stay on this filesystem" has to mean.
    let root_devs: Vec<Option<u64>> = opts
        .roots
        .iter()
        .map(|p| {
            std::fs::symlink_metadata(p)
                .ok()
                .map(|md| std::os::unix::fs::MetadataExt::dev(&md))
        })
        .collect();
    let cross_fs = opts.cross_filesystems;
    let walk_opts = dua_core::Options {
        skip_metadata: false,
        apfs_clone_metadata: true,
    };
    let mut totals_files: u64 = 0;
    let mut totals_alloc: u64 = 0;
    {
        let cancel_walk = cancel.clone();
        let iter = dua_core::walk_roots(
            roots,
            threads,
            dua_core::Order::ParentFirst,
            walk_opts,
            move |root_idx, entry| {
                if cancel_walk.load(std::sync::atomic::Ordering::Relaxed) {
                    return false;
                }
                // 4.0.0: metadata is Option<Result<Metadata>> (skip_metadata=false ⇒ Some)
                let Some(Ok(md)) = entry.metadata.as_ref() else {
                    return false;
                };
                if !entry.file_type.is_dir() {
                    return false;
                }
                if !cross_fs
                    && let Some(Some(dev)) = root_devs.get(root_idx)
                    && md.dev() != *dev
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
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break; // cancelled mid-walk: dropping `iter` stops + joins the workers
            }
            match event {
                dua_core::RootEvent::Entry(Ok(entry)) => {
                    let Some(Ok(md)) = entry.metadata.as_ref() else {
                        continue;
                    };
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
                    if batch.len() >= 256
                        && tx
                            .send(ScanEvent::Batch(std::mem::take(&mut batch)))
                            .is_err()
                    {
                        break; // receiver gone: user quit — dropping iter cancels the walk
                    }
                }
                dua_core::RootEvent::Entry(Err(err)) => {
                    // root_idx is the walk_roots index, i.e. our enumerate() position into
                    // opts.roots — attribute the error to the right root.
                    let root = opts.roots.get(root_idx).cloned().unwrap_or_default();
                    if tx
                        .send(ScanEvent::RootError {
                            root,
                            err: err.to_string(),
                        })
                        .is_err()
                    {
                        break; // receiver gone: stop promptly
                    }
                }
                dua_core::RootEvent::Finished => {}
            }
        }
        if !batch.is_empty() {
            let _ = tx.send(ScanEvent::Batch(batch));
        }
    } // iter dropped here: dua-core stops + joins workers (cancel primitive)
    let totals = ScanTotals {
        files: totals_files,
        alloc: totals_alloc,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        snapshots: local_snapshots(),
        volumes: vols,
    };
    // Terminal event: consumers drain the channel until Finished arrives exactly once.
    let _ = tx.send(ScanEvent::Finished(totals.clone()));
    totals
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
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for d in disks.list() {
        let mount = d.mount_point().to_path_buf();
        let dev = dev_of(&mount).unwrap_or(0);
        // rename_excl probe: APFS/HFS+ support VOL_CAP_INT_RENAME_EXCL; exFAT/FAT do not.
        let fs = d.file_system().to_string_lossy().into_owned();
        let rename_excl = matches!(fs.as_str(), "apfs" | "hfs" | "apfs (case-sensitive)");
        // Two free figures, two sources — the SPEC §3.1 pair must be labeled for
        // what each one actually is ([02 §3]: `statfs` excludes purgeable, the
        // CFURL capacity keys include it, and the UI has to pick one and say so).
        //
        // `free_important`: sysinfo `available_space()`. On macOS 0.39.6 that is
        // NOT a statfs read — the apple backend requests
        // `kCFURLVolumeAvailableCapacityForImportantUsageKey` and prefers it,
        // falling back to the raw `kCFURLVolumeAvailableCapacityKey` only when it
        // reads 0 (`sysinfo-0.39.6/src/unix/apple/disk.rs:379-392`). So this is
        // the Finder / About-This-Mac "available" figure: free + purgeable.
        let free_important = d.available_space();
        // `free_raw`: a real `statfs(2)` `f_bavail` read — the literal raw
        // filesystem free space, excluding purgeable. Raw libc FFI per R1
        // (SPEC §5: "syscall layer via raw `libc` FFI"). darwin's `statfs` has no
        // `f_frsize`, so `f_bsize` is the allocation unit.
        // Fallback: statfs can only fail here if the mount vanished mid-enumeration.
        // Report the important figure (the larger of the two) rather than 0, so a
        // "used = total - free" display degrades instead of claiming a full disk.
        let free_raw = statfs_free(&mount).unwrap_or(free_important);
        // `free_opportunistic` stays 0 — see the field doc in `VolumeInfo` for why
        // (no sysinfo path, no in-tree CoreFoundation read).
        let ro = d.is_read_only()
            // sysinfo's is_read_only() is statfs MNT_RDONLY — truthful per mount, but the
            // sealed-system `/` reports RO while firmlinked writes land on the Data slice
            // (same st_dev). Probe actual writability: create+remove a temp file.
            && !is_writable(&mount);
        out.push(VolumeInfo {
            dev,
            mount,
            fs,
            total: d.total_space(),
            free_raw,
            free_important,
            free_opportunistic: 0,
            read_only: ro,
            rename_excl,
        });
    }
    out
}

/// Raw filesystem free space via `statfs(2)` — `f_bavail` blocks × `f_bsize`.
/// Excludes purgeable/snapshot-reclaimable space, unlike the CFURL capacity keys
/// ([02 §3]). darwin's `statfs` has no `f_frsize`, so `f_bsize` is the unit.
/// `None` when the mount is gone or the path does not fit `CString`.
fn statfs_free(mount: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    let c = CString::new(mount.as_os_str().as_encoded_bytes()).ok()?;
    let mut buf = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `c` is a valid NUL-terminated path; on success statfs initializes
    // the whole `statfs` struct, which we then read through `assume_init`.
    let rc = unsafe { libc::statfs(c.as_ptr(), buf.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let st = unsafe { buf.assume_init() };
    // f_bsize is the volume's allocation unit; f_bavail is blocks free to a
    // non-privileged caller (f_bfree would count reserved blocks).
    Some(st.f_bavail.saturating_mul(u64::from(st.f_bsize)))
}

fn dev_of(path: &std::path::Path) -> Option<u64> {
    std::os::unix::fs::MetadataExt::dev(&std::fs::metadata(path).ok()?).into()
}

/// Writability probe: create+remove a unique temp file in `dir`. Detects the
/// firmlink case where a read-only-looking mount accepts writes (sealed-system
/// `/` on modern macOS). Failure = read-only for our purposes.
fn is_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(format!(".janitor-ro-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}
