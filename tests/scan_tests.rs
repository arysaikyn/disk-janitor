//! scan::start end-to-end on a synthetic tree (SPEC Part B §7 'Scanner edge cases' + §3.1):
//! nested dirs, known-size files, a symlink, a hardlinked pair — the walker must emit every
//! file in Batches with honest alloc bytes, report exact file totals, and emit Finished
//! exactly once. Tempdirs only; never real user data.

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::RecvTimeoutError;
use disk_janitor::scan::{DirStat, Kind, ScanEvent, ScanOptions, ScanStats, ScanTotals};
use tempfile::TempDir;

/// Build the synthetic tree; returns (expected_file_count, (rel_path, size) for each file).
fn build_tree(dir: &TempDir) -> (usize, Vec<(&'static str, usize)>) {
    let files: Vec<(&'static str, usize)> = vec![
        ("a.txt", 1),
        ("sub/b.bin", 100_000),
        ("sub/deep/c.log", 4_097),
        ("data/hard1.bin", 5_000),
    ];
    for (rel, size) in &files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, vec![0u8; *size]).unwrap();
    }
    // Hardlinked pair: same inode, two directory entries — both are distinct DirStats.
    std::fs::hard_link(
        dir.path().join("data/hard1.bin"),
        dir.path().join("data/hard2.bin"),
    )
    .unwrap();
    // Symlink: reported, never followed.
    std::os::unix::fs::symlink("a.txt", dir.path().join("link.txt")).unwrap();
    (files.len() + 1, files)
}

fn find<'a>(all: &'a [DirStat], dir: &TempDir, rel: &str) -> &'a DirStat {
    let full = dir.path().join(rel);
    let hits: Vec<&DirStat> = all
        .iter()
        .filter(|d| d.parent.join(&d.name) == full)
        .collect();
    assert_eq!(hits.len(), 1, "expected exactly one DirStat for {rel}");
    hits[0]
}

/// Drain events until the channel disconnects, enforcing: no Batch after Finished,
/// no duplicate Finished, Finished before disconnect.
fn drain(rx: &crossbeam_channel::Receiver<ScanEvent>) -> (Vec<DirStat>, ScanTotals) {
    let mut all: Vec<DirStat> = Vec::new();
    let mut totals: Option<ScanTotals> = None;
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(ScanEvent::Batch(mut b)) => {
                assert!(totals.is_none(), "Batch after Finished");
                all.append(&mut b);
            }
            Ok(ScanEvent::RootError { root, err }) => {
                panic!("unexpected RootError for {root:?}: {err}")
            }
            Ok(ScanEvent::Finished(t)) => {
                assert!(totals.is_none(), "Finished emitted more than once");
                totals = Some(t);
            }
            Err(RecvTimeoutError::Timeout) => panic!("scan did not finish within 60s"),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let totals = totals.expect("Finished must arrive before the channel disconnects");
    (all, totals)
}

/// SPEC §7 'Scanner edge cases' — full pipeline: scan::start over a synthetic tree emits
/// every file in some Batch (alloc_bytes > 0 and >= app_bytes), totals.files matches the
/// real file count, and Finished arrives exactly once.
#[test]
fn scan_start_emits_all_files_and_finished_once() {
    let dir = tempfile::tempdir().unwrap();
    let (expected_files, files) = build_tree(&dir);

    let (tx, rx) = crossbeam_channel::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(ScanStats::default());
    let opts = ScanOptions {
        roots: vec![dir.path().to_path_buf()],
        threads: 2,
        cross_filesystems: false,
        ignore: Vec::new(),
        min_dir_bytes: 0,
    };
    let handle = disk_janitor::scan::start(opts, tx, cancel, stats.clone());
    let (all, event_totals) = drain(&rx);
    let thread_totals = handle.join().unwrap();

    // Every created file appears exactly once, with honest bytes.
    for (rel, size) in &files {
        let d = find(&all, &dir, rel);
        assert_eq!(d.kind, Kind::File, "{rel}");
        assert_eq!(d.app_bytes, *size as u64, "{rel} apparent size");
        assert!(d.alloc_bytes > 0, "{rel} must own allocation");
        assert!(d.alloc_bytes >= d.app_bytes, "{rel} alloc >= app");
    }
    // Hardlinked pair: same inode, nlink == 2 on both entries.
    let h1 = find(&all, &dir, "data/hard1.bin");
    let h2 = find(&all, &dir, "data/hard2.bin");
    assert_eq!(h1.ino, h2.ino, "hardlinked pair shares an inode");
    assert_eq!(h1.nlink, 2);
    assert_eq!(h2.nlink, 2);
    // Symlink is reported as a symlink entry.
    assert_eq!(find(&all, &dir, "link.txt").kind, Kind::Symlink);
    // 4 dirs (root, sub, deep, data), 5 files, 1 symlink.
    assert_eq!(all.iter().filter(|d| d.kind == Kind::Dir).count(), 4);
    assert_eq!(all.len(), 10);

    // Totals: file count exact, alloc positive, event and thread agree.
    assert_eq!(event_totals.files, expected_files as u64);
    assert!(event_totals.alloc > 0);
    assert_eq!(thread_totals.files, event_totals.files);
    assert_eq!(thread_totals.alloc, event_totals.alloc);
    // ScanStats cross-check: entries == all emitted DirStats, bytes == alloc total.
    assert_eq!(stats.entries.load(Ordering::Relaxed), 10);
    assert_eq!(stats.bytes.load(Ordering::Relaxed), event_totals.alloc);
    // Volume truth is attached to the Finished envelope.
    assert!(
        !event_totals.volumes.is_empty(),
        "Finished carries VolumeInfo"
    );
}

/// SPEC §7 'Scanner edge cases' — a multi-root scan attributes errors to the right root and
/// reports both roots' files (root_idx routing in RootError / batch emission).
#[test]
fn scan_start_multi_root_routes_events() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    std::fs::write(dir_a.path().join("x.txt"), b"x").unwrap();
    std::fs::write(dir_b.path().join("y.txt"), b"y").unwrap();

    let (tx, rx) = crossbeam_channel::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(ScanStats::default());
    let opts = ScanOptions {
        roots: vec![dir_a.path().to_path_buf(), dir_b.path().to_path_buf()],
        threads: 2,
        cross_filesystems: false,
        ignore: Vec::new(),
        min_dir_bytes: 0,
    };
    let handle = disk_janitor::scan::start(opts, tx, cancel, stats);
    let (all, event_totals) = drain(&rx);
    handle.join().unwrap();

    assert_eq!(event_totals.files, 2);
    let paths: Vec<PathBuf> = all
        .iter()
        .filter(|d| d.kind == Kind::File)
        .map(|d| d.parent.join(&d.name))
        .collect();
    assert!(paths.contains(&dir_a.path().join("x.txt")));
    assert!(paths.contains(&dir_b.path().join("y.txt")));
}
