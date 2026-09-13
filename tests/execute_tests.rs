//! execute/ end-to-end on tempdirs (SPEC Part B §3.4, §5): apply moves entries
//! into `<mnt>/.Trash-janitor/<op>/` behind a synced WAL, purge is gated on the
//! typed byte count, restore reverses renames from WAL pairs. Tempdirs only;
//! never real user data (course rule 4).

#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crossbeam_channel::{Receiver, Sender};
use disk_janitor::execute::{self, ExecEvent, Executor};
use disk_janitor::ledger::{self, Kind, LedgerRecord};
use disk_janitor::{
    Grade, PlanAction, PlanEntry, UF_HIDDEN, VolumeInfo, WalEntry, WalKind, path_hash,
};
use tempfile::TempDir;

/// env is process-global; every test that touches DISK_JANITOR_HOME serializes
/// on this so parallel test threads never clobber each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn vol_info(dir: &Path) -> VolumeInfo {
    let md = fs::symlink_metadata(dir).unwrap();
    VolumeInfo {
        dev: std::os::unix::fs::MetadataExt::dev(&md),
        mount: dir.to_path_buf(),
        fs: "apfs".into(),
        total: 0,
        free_raw: 0,
        free_important: 0,
        free_opportunistic: 0,
        read_only: false,
        rename_excl: true,
    }
}

fn entry(cid: &str, path: PathBuf, bytes: u64) -> PlanEntry {
    let path_hash = path_hash(&path);
    PlanEntry {
        cid: cid.to_string(),
        path,
        path_hash,
        alloc_bytes: bytes,
        grade: Grade::PureCache,
        action: PlanAction::Quarantine,
        evidence: vec![],
        inode: None,
    }
}

/// Entry with an explicit (dev, ino) dedup key — as plan::build now emits.
fn entry_inode(cid: &str, path: PathBuf, bytes: u64, inode: (u64, u64)) -> PlanEntry {
    let mut e = entry(cid, path, bytes);
    e.inode = Some(inode);
    e
}

fn open_executor(plan_id: &str, entries: Vec<PlanEntry>, vol: &Path, home: &Path) -> Executor {
    let store = ledger::open(home).unwrap();
    execute::open(plan_id.to_string(), entries, &[vol_info(vol)], store).unwrap()
}

/// Parse a per-volume wal.jsonl (LedgerStore envelope format) into bare entries.
fn wal_entries(path: &Path) -> Vec<disk_janitor::WalEntry> {
    ledger::record_iter(path)
        .into_iter()
        .map(|(_, w)| w)
        .collect()
}

fn drain(tx: Sender<ExecEvent>, rx: Receiver<ExecEvent>) -> Vec<ExecEvent> {
    drop(tx);
    rx.iter().collect()
}

fn quarantine_root(vol: &Path) -> PathBuf {
    vol.join(".Trash-janitor")
}

/// Recursively find every `wal.jsonl` under `dir` (proves a no-op apply creates
/// no per-volume WAL anywhere).
fn collect_wal_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.file_name().is_some_and(|n| n == "wal.jsonl") {
                out.push(p.clone());
            }
            if p.is_dir() {
                out.extend(collect_wal_files(&p));
            }
        }
    }
    out
}

fn lstat_flags(p: &Path) -> u32 {
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::lstat(c.as_ptr(), &mut st) },
        0,
        "lstat {}",
        p.display()
    );
    st.st_flags
}

fn chflags(p: &Path, flags: u32) {
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::chflags(c.as_ptr(), flags) },
        0,
        "chflags {}",
        p.display()
    );
}

fn grace_hours_ok(
    grace: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let d = grace - now;
    d >= chrono::Duration::hours(71) && d <= chrono::Duration::hours(73)
}

/// (a) apply 3 entries: files land in `<vol>/.Trash-janitor/<op>/` (hidden,
/// 0700), zero sources remain, WAL has intent+result pairs, home ledger has 3
/// apply recs with undo.grace = now + 72h. SPEC §3.4 table + §5.1 invariants.
#[test]
fn apply_moves_entries_into_hidden_quarantine_with_wal_and_ledger() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let f1 = vol.path().join("a.bin");
    let f2 = vol.path().join("b.bin");
    let sub = vol.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let f3 = sub.join("c.bin");
    fs::write(&f1, vec![7u8; 100]).unwrap();
    fs::write(&f2, vec![9u8; 200]).unwrap();
    fs::write(&f3, vec![3u8; 300]).unwrap();

    let entries = vec![
        entry("C-001", f1.clone(), 100),
        entry("C-002", f2.clone(), 200),
        entry("C-003", f3.clone(), 300),
    ];
    let mut ex = open_executor("plan-1", entries, vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.apply(tx.clone(), None).unwrap();

    assert_eq!(rep.applied, 3);
    assert_eq!(rep.refused, 0);
    assert!(!rep.op_id.is_empty());
    // Zero source files remain.
    assert!(!f1.exists() && !f2.exists() && !f3.exists());
    // All land in <vol>/.Trash-janitor/<op>/ (basename only, sub flattened).
    let root = quarantine_root(vol.path());
    let op_dir = root.join(&rep.op_id);
    assert!(op_dir.is_dir());
    for (name, content) in [
        ("a.bin", vec![7u8; 100]),
        ("b.bin", vec![9u8; 200]),
        ("c.bin", vec![3u8; 300]),
    ] {
        assert_eq!(
            fs::read(op_dir.join(name)).unwrap(),
            content,
            "{name} quarantined intact"
        );
    }
    // Root is hidden + 0700 (SPEC §5.1).
    assert_ne!(lstat_flags(&root) & UF_HIDDEN, 0, "UF_HIDDEN set");
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "quarantine root mode 0700");

    // WAL: 3 intent + 3 result records, per-entry interleaved, concrete paths.
    let wal = wal_entries(&root.join("wal.jsonl"));
    assert_eq!(wal.len(), 6, "intent+result per entry");
    for (i, w) in wal.iter().enumerate() {
        assert_eq!(w.op_id, rep.op_id);
        assert_eq!(w.plan_id, "plan-1");
        assert_eq!(
            w.kind,
            if i % 2 == 0 {
                WalKind::Intent
            } else {
                WalKind::Result
            }
        );
        assert!(w.from.is_some() && w.to.is_some());
        if w.kind == WalKind::Result {
            assert_eq!(
                w.to.as_deref(),
                Some(op_dir.join(&["a.bin", "b.bin", "c.bin"][i / 2]).as_path())
            );
        }
    }

    // Home ledger: 3 apply recs, grace = now + 72h, renamex_np honored.
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    let applies: Vec<&LedgerRecord> = records
        .iter()
        .filter(|r| r.kind == Kind::Apply)
        .map(|r| &r.rec)
        .collect();
    assert_eq!(applies.len(), 3);
    let now = chrono::Utc::now();
    for a in &applies {
        match a {
            LedgerRecord::Apply {
                plan_id,
                quarantine_path,
                renamex_np,
                undo,
                ..
            } => {
                assert_eq!(plan_id, "plan-1");
                assert!(*renamex_np);
                assert!(quarantine_path.starts_with(&op_dir));
                assert!(
                    grace_hours_ok(undo.grace_expires_at, now),
                    "grace ≈ now + 72h"
                );
            }
            _ => panic!("expected apply record"),
        }
    }
    // byte_count echoes each entry's alloc_bytes (100/200/300), original path recorded.
    let mut counts: Vec<u64> = applies
        .iter()
        .map(|a| match a {
            LedgerRecord::Apply { undo, .. } => undo.byte_count,
            _ => 0,
        })
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![100, 200, 300]);
}

/// (b) apply is idempotent: re-applying the same plan skips every entry (source
/// ENOENT), touches nothing, and recovery reports no dangling intents.
#[test]
fn apply_second_time_is_benign_skip() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let f1 = vol.path().join("a.bin");
    let f2 = vol.path().join("b.bin");
    fs::write(&f1, vec![1u8; 100]).unwrap();
    fs::write(&f2, vec![2u8; 200]).unwrap();
    let entries = vec![
        entry("C-001", f1.clone(), 100),
        entry("C-002", f2.clone(), 200),
    ];

    let mut ex = open_executor("plan-1", entries.clone(), vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let first = ex.apply(tx, None).unwrap();
    drop(ex); // release the home flock before a second Store opens on the same home
    let root = quarantine_root(vol.path());
    let wal_before = wal_entries(&root.join("wal.jsonl")).len();

    let mut ex2 = open_executor("plan-1", entries, vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let second = ex2.apply(tx, None).unwrap();

    assert_eq!(second.applied, 0, "nothing newly applied");
    assert_eq!(second.skipped, 2, "both entries already moved");
    assert_eq!(second.refused, 0);
    assert!(!f1.exists() && !f2.exists(), "sources stay quarantined");
    assert!(root.join(&first.op_id).join("a.bin").exists());
    let wal_after = wal_entries(&root.join("wal.jsonl")).len();
    assert_eq!(
        wal_after, wal_before,
        "no new WAL records on idempotent re-apply"
    );
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.dangling_intents.len(), 0);
}

/// EEXIST retry: two entries sharing a basename (different source dirs) get
/// `-<n>` suffixes inside one op dir — the syscall is the existence check
/// (SPEC §5.1), contents stay distinct.
#[test]
fn apply_same_basename_gets_suffix_retry() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let d1 = vol.path().join("one");
    let d2 = vol.path().join("two");
    fs::create_dir(&d1).unwrap();
    fs::create_dir(&d2).unwrap();
    let f1 = d1.join("cache.bin");
    let f2 = d2.join("cache.bin");
    fs::write(&f1, vec![0x11u8; 64]).unwrap();
    fs::write(&f2, vec![0x22u8; 64]).unwrap();
    let entries = vec![
        entry("C-001", f1.clone(), 64),
        entry("C-002", f2.clone(), 64),
    ];

    let mut ex = open_executor("plan-1", entries, vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.apply(tx, None).unwrap();
    assert_eq!(rep.applied, 2);

    let op_dir = quarantine_root(vol.path()).join(&rep.op_id);
    assert_eq!(
        fs::read(op_dir.join("cache.bin")).unwrap(),
        vec![0x11u8; 64]
    );
    assert_eq!(
        fs::read(op_dir.join("cache.bin-1")).unwrap(),
        vec![0x22u8; 64]
    );
    assert!(!f1.exists() && !f2.exists());
}

/// (c) purge: wrong typed byte count refuses with zero side effects; the exact
/// count unlinks everything, frees the echoed bytes, and appends WAL results +
/// ledger purge recs. Purge runs to completion (D-15).
#[test]
fn purge_gate_refuses_mismatch_and_exact_count_frees() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let f1 = vol.path().join("a.bin");
    let f2 = vol.path().join("b.bin");
    fs::write(&f1, vec![1u8; 100]).unwrap();
    fs::write(&f2, vec![2u8; 200]).unwrap();
    let entries = vec![
        entry("C-001", f1.clone(), 100),
        entry("C-002", f2.clone(), 200),
    ];

    let mut ex = open_executor("plan-1", entries, vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    ex.apply(tx, None).unwrap();
    let root = quarantine_root(vol.path());
    let wal_before = wal_entries(&root.join("wal.jsonl")).len();
    // Quarantined paths under the op dir, discovered from the WAL.
    let wal = wal_entries(&root.join("wal.jsonl"));
    let op_files: Vec<PathBuf> = wal.iter().filter_map(|w| w.to.clone()).collect();

    // Mismatched count → hard refusal, zero side effects.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let err = ex.purge(299, tx).unwrap_err();
    assert!(
        err.to_string().contains("299"),
        "refusal names the mismatch: {err}"
    );
    for p in &op_files {
        assert!(p.exists(), "nothing freed on mismatch: {}", p.display());
    }
    let wal_mid = wal_entries(&root.join("wal.jsonl")).len();
    assert_eq!(wal_mid, wal_before, "no WAL appends on mismatch");
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    assert_eq!(records.iter().filter(|r| r.kind == Kind::Purge).count(), 0);

    // Exact count → everything purged, bytes echoed.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.purge(300, tx.clone()).unwrap();
    assert_eq!(rep.freed_bytes, 300);
    assert_eq!(rep.entries, 2);
    let events = drain(tx, _rx);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, ExecEvent::EntryPurged { .. }))
            .count(),
        2,
        "one EntryPurged per entry"
    );
    for p in &op_files {
        assert!(!p.exists(), "quarantined path purged");
    }
    let wal = wal_entries(&root.join("wal.jsonl"));
    // Purge results use fresh op_ids (checkpoint dedupes by op_id; reusing the
    // apply op_id would collapse the intent/result pair). They are the only
    // Result records with from:None AND to:None — LedgerStore's mirrors carry
    // from:Some(quarantine_path), apply results carry from:Some+to:Some.
    let purge_results = wal
        .iter()
        .filter(|w| w.kind == WalKind::Result && w.from.is_none() && w.to.is_none())
        .count();
    assert_eq!(
        purge_results, 2,
        "WAL result per purged entry (from:None = not restorable)"
    );
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    let purges: Vec<&LedgerRecord> = records
        .iter()
        .filter(|r| r.kind == Kind::Purge)
        .map(|r| &r.rec)
        .collect();
    assert_eq!(purges.len(), 2);
    let freed: u64 = purges
        .iter()
        .map(|p| match p {
            LedgerRecord::Purge { bytes_freed, .. } => *bytes_freed,
            _ => 0,
        })
        .sum();
    assert_eq!(freed, 300, "bytes_freed echoes the typed count");
    assert!(events.iter().any(|e| matches!(e, ExecEvent::Done)));
}

/// (c2) B1: a plan with a HARD-LINKED pair (same (dev,ino), both entries
/// Quarantine) — plan::build dedups the total, so the displayed quarantine
/// number counts the inode once. purge() must gate on that SAME deduped total
/// (not Σ entries) and account freed bytes per inode, not per link: the second
/// link's unlink frees 0. A legacy entry without an inode key (pre-field
/// manifest) keeps its own Σ semantics.
#[test]
fn purge_hardlinked_pair_gates_on_deduped_total_and_frees_once() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let f1 = vol.path().join("a.bin");
    let f2 = vol.path().join("b.bin"); // hard link to the SAME inode
    let f3 = vol.path().join("c.bin"); // legacy entry, no inode key
    fs::write(&f1, vec![1u8; 100]).unwrap();
    fs::hard_link(&f1, &f2).unwrap();
    fs::write(&f3, vec![2u8; 50]).unwrap();
    let md = fs::symlink_metadata(&f1).unwrap();
    let inode = (
        std::os::unix::fs::MetadataExt::dev(&md),
        std::os::unix::fs::MetadataExt::ino(&md),
    );
    assert_eq!(std::os::unix::fs::MetadataExt::nlink(&md), 2);

    let entries = vec![
        entry_inode("C-001", f1.clone(), 100, inode),
        entry_inode("C-002", f2.clone(), 100, inode), // second link
        entry("C-003", f3.clone(), 50),               // inode: None (legacy)
    ];
    let mut ex = open_executor("plan-1", entries, vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    ex.apply(tx, None).unwrap();

    // The plan displays the DEDUPED total (100 + 50 = 150). Typing Σ entries
    // (250) must refuse — and the refusal must name the deduped total.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let err = ex.purge(250, tx).unwrap_err();
    assert!(
        err.to_string().contains("150"),
        "refusal names the deduped quarantine total: {err}"
    );
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    assert_eq!(records.iter().filter(|r| r.kind == Kind::Purge).count(), 0);

    // Exact deduped total → both links' quarantine paths unlinked, but the
    // inode's bytes freed ONCE (second link's unlink frees 0).
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.purge(150, tx).unwrap();
    assert_eq!(rep.freed_bytes, 150, "freed = inode once + legacy entry");
    assert_eq!(rep.entries, 3, "all three quarantine paths unlinked");
    assert!(!f1.exists());
    assert!(!f2.exists());
    assert!(!f3.exists());

    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    let purges: Vec<&LedgerRecord> = records
        .iter()
        .filter(|r| r.kind == Kind::Purge)
        .map(|r| &r.rec)
        .collect();
    assert_eq!(purges.len(), 3);
    let freed: u64 = purges
        .iter()
        .map(|p| match p {
            LedgerRecord::Purge { bytes_freed, .. } => *bytes_freed,
            _ => 0,
        })
        .sum();
    assert_eq!(freed, 150, "ledger bytes_freed sums to the deduped total");
}

/// (d) restore after apply: originals come back byte-identical, entries of
/// other ops in the same WAL are untouched, ledger restore recs record the
/// apply seq as `undoes`.
#[test]
fn restore_returns_originals_and_skips_other_ops() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let f1 = vol.path().join("a.bin");
    let f2 = vol.path().join("b.bin");
    fs::write(&f1, vec![0xAu8; 100]).unwrap();
    fs::write(&f2, vec![0xBu8; 200]).unwrap();
    let mut ex1 = open_executor(
        "plan-1",
        vec![
            entry("C-001", f1.clone(), 100),
            entry("C-002", f2.clone(), 200),
        ],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op1 = ex1.apply(tx, None).unwrap().op_id;
    drop(ex1); // release the home flock before a second Store opens on the same home

    let g1 = vol.path().join("g.bin");
    fs::write(&g1, vec![0xCu8; 50]).unwrap();
    let mut ex2 = open_executor(
        "plan-2",
        vec![entry("C-010", g1.clone(), 50)],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op2 = ex2.apply(tx, None).unwrap().op_id;

    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    // undoes must be the seqs of op1's own apply recs (plan-1 only).
    let root = quarantine_root(vol.path());
    let apply_seqs: Vec<u64> = records
        .iter()
        .filter(|r| r.kind == Kind::Apply)
        .filter(|r| matches!(&r.rec, LedgerRecord::Apply { quarantine_path, .. } if quarantine_path.starts_with(&root.join(&op1))))
        .map(|r| r.seq)
        .collect();

    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex2.restore(&op1, tx.clone()).unwrap();
    assert_eq!(rep.restored, vec!["C-001".to_string(), "C-002".to_string()]);
    assert!(rep.missing.is_empty());
    assert_eq!(
        fs::read(&f1).unwrap(),
        vec![0xAu8; 100],
        "a.bin byte-identical"
    );
    assert_eq!(
        fs::read(&f2).unwrap(),
        vec![0xBu8; 200],
        "b.bin byte-identical"
    );
    // op2's entry untouched, still quarantined.
    assert!(!g1.exists());
    let root = quarantine_root(vol.path());
    assert!(root.join(&op2).join("g.bin").exists());
    // op1's dir removed when empty.
    assert!(
        !root.join(&op1).exists(),
        "empty op dir removed after restore"
    );
    let _ = drain(tx, _rx);

    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    let restores: Vec<&LedgerRecord> = records
        .iter()
        .filter(|r| r.kind == Kind::Restore)
        .map(|r| &r.rec)
        .collect();
    assert_eq!(restores.len(), 2);
    let mut undoes: Vec<u64> = restores
        .iter()
        .map(|r| match r {
            LedgerRecord::Restore { undoes, .. } => *undoes,
            _ => 0,
        })
        .collect();
    undoes.sort_unstable();
    assert_eq!(undoes, apply_seqs, "restore undoes = apply seqs");
    // WAL of op2 untouched by op1's restore.
    let wal = wal_entries(&root.join("wal.jsonl"));
    let op2_recs = wal.iter().filter(|w| w.op_id == op2).count();
    assert_eq!(op2_recs, 2, "op2 intent+result intact");
}

/// (e) restore after purge: purged entries are reported missing (benign skip);
/// the surviving entries of other ops restore fully.
#[test]
fn restore_after_purge_reports_missing_and_restores_survivors() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let fa = vol.path().join("a.bin");
    fs::write(&fa, vec![1u8; 100]).unwrap();
    let mut ex1 = open_executor(
        "plan-1",
        vec![entry("C-001", fa.clone(), 100)],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op1 = ex1.apply(tx, None).unwrap().op_id;
    let (tx, _rx) = crossbeam_channel::unbounded();
    ex1.purge(100, tx).unwrap();
    drop(ex1); // release the home flock before a second Store opens on the same home

    let fb = vol.path().join("b.bin");
    let fc = vol.path().join("c.bin");
    fs::write(&fb, vec![2u8; 200]).unwrap();
    fs::write(&fc, vec![3u8; 300]).unwrap();
    let mut ex2 = open_executor(
        "plan-2",
        vec![
            entry("C-002", fb.clone(), 200),
            entry("C-003", fc.clone(), 300),
        ],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op2 = ex2.apply(tx, None).unwrap().op_id;

    // op1 was fully purged → its only entry is reported missing, nothing restored.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep1 = ex2.restore(&op1, tx).unwrap();
    assert_eq!(rep1.missing, vec!["C-001".to_string()]);
    assert!(rep1.restored.is_empty());
    assert!(!fa.exists(), "purged file stays gone");

    // op2's survivors restore byte-identical.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep2 = ex2.restore(&op2, tx).unwrap();
    assert_eq!(
        rep2.restored,
        vec!["C-002".to_string(), "C-003".to_string()]
    );
    assert!(rep2.missing.is_empty());
    assert_eq!(fs::read(&fb).unwrap(), vec![2u8; 200]);
    assert_eq!(fs::read(&fc).unwrap(), vec![3u8; 300]);
    assert!(
        !quarantine_root(vol.path()).join(&op2).exists(),
        "empty op dir removed"
    );
}

/// (f) flag-blocked entry: UF_IMMUTABLE survives the pre-check as a refusal —
/// Error event, entry stays at source, siblings proceed; recovery reports the
/// dangling intent.
#[test]
fn apply_refuses_flag_blocked_entry_others_proceed() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let f1 = vol.path().join("locked.bin");
    let f2 = vol.path().join("ok.bin");
    let f3 = vol.path().join("ok2.bin");
    fs::write(&f1, vec![1u8; 100]).unwrap();
    fs::write(&f2, vec![2u8; 200]).unwrap();
    fs::write(&f3, vec![3u8; 300]).unwrap();
    chflags(&f1, libc::UF_IMMUTABLE);

    let entries = vec![
        entry("C-001", f1.clone(), 100),
        entry("C-002", f2.clone(), 200),
        entry("C-003", f3.clone(), 300),
    ];
    let mut ex = open_executor("plan-1", entries, vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.apply(tx.clone(), None).unwrap();
    let events = drain(tx, _rx);

    assert_eq!(rep.applied, 2);
    assert_eq!(rep.refused, 1);
    let errs: Vec<&ExecEvent> = events
        .iter()
        .filter(|e| matches!(e, ExecEvent::Error { .. }))
        .collect();
    assert_eq!(errs.len(), 1, "one Error event");
    assert!(matches!(errs[0], ExecEvent::Error { cid, .. } if cid == "C-001"));
    assert!(f1.exists(), "flag-blocked file never moves");
    assert_eq!(fs::read(&f1).unwrap(), vec![1u8; 100]);
    assert!(!f2.exists() && !f3.exists(), "siblings proceed");

    // Refusal left a dangling intent (pre-check runs after the WAL intent,
    // §5.2) — recovery drops the tail and reports it.
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.dangling_intents.len(), 1, "dangling intent reported");
    assert_eq!(rr.dangling_intents[0].entry, path_hash(&f1));
    assert_eq!(rr.rolled_forward, 0);

    chflags(&f1, 0); // let TempDir cleanup succeed
}

/// D-26 restore collision: the original path is occupied (user recreated the
/// file) — restore must NEVER clobber. Default is keep-both: quarantine content
/// lands at `original-1` (then `-2` when that is occupied too), the collision
/// is reported in RestoreReport.collided, and the ledger restore rec records
/// the ACTUAL final path.
#[test]
fn restore_collision_keeps_both() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    // --- Level 1: original occupied -> restore to original-1. ---
    let f1 = vol.path().join("a.bin");
    fs::write(&f1, vec![0xAu8; 100]).unwrap();
    let mut ex1 = open_executor(
        "plan-1",
        vec![entry("C-001", f1.clone(), 100)],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op1 = ex1.apply(tx, None).unwrap().op_id;
    drop(ex1); // release the home flock

    // User recreated the original before restoring.
    fs::write(&f1, b"user-recreated").unwrap();
    let mut ex2 = open_executor("plan-1", vec![], vol.path(), home.path());
    let (tx, rx) = crossbeam_channel::unbounded();
    let rep = ex2.restore(&op1, tx.clone()).unwrap();
    let events = drain(tx, rx);
    drop(ex2);

    assert_eq!(rep.restored, vec!["C-001".to_string()]);
    assert_eq!(
        rep.collided,
        vec!["C-001".to_string()],
        "collision reported"
    );
    assert!(rep.missing.is_empty());
    // The recreated file is untouched; the quarantined content lands at a.bin-1.
    assert_eq!(fs::read(&f1).unwrap(), b"user-recreated");
    assert_eq!(
        fs::read(vol.path().join("a.bin-1")).unwrap(),
        vec![0xAu8; 100]
    );
    // The op dir is removed when empty.
    assert!(!quarantine_root(vol.path()).join(&op1).exists());
    assert!(events.iter().any(|e| matches!(
        e,
        ExecEvent::EntryRestored { cid, to }
            if cid == "C-001" && to == &vol.path().join("a.bin-1").display().to_string()
    )));
    // The ledger restore rec records the ACTUAL target (a.bin-1), not the collision.
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    let tos: Vec<PathBuf> = records
        .iter()
        .filter(|r| r.kind == Kind::Restore)
        .filter_map(|r| match &r.rec {
            LedgerRecord::Restore { to, .. } => Some(to.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tos, vec![vol.path().join("a.bin-1")]);

    // --- Level 2: original AND original-1 occupied -> restore to original-2. ---
    let f2 = vol.path().join("b.bin");
    fs::write(&f2, vec![0xBu8; 200]).unwrap();
    let mut ex3 = open_executor(
        "plan-2",
        vec![entry("C-002", f2.clone(), 200)],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op2 = ex3.apply(tx, None).unwrap().op_id;
    drop(ex3);

    fs::write(&f2, b"recreated").unwrap();
    fs::write(vol.path().join("b.bin-1"), b"also-recreated").unwrap();
    let mut ex4 = open_executor("plan-2", vec![], vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep2 = ex4.restore(&op2, tx).unwrap();
    assert_eq!(rep2.restored, vec!["C-002".to_string()]);
    assert_eq!(rep2.collided, vec!["C-002".to_string()]);
    assert_eq!(fs::read(&f2).unwrap(), b"recreated", "user file untouched");
    assert_eq!(
        fs::read(vol.path().join("b.bin-1")).unwrap(),
        b"also-recreated"
    );
    assert_eq!(
        fs::read(vol.path().join("b.bin-2")).unwrap(),
        vec![0xBu8; 200]
    );
}

/// C2: restore when the original PARENT dir was deleted since quarantine —
/// without parent recreation the rename fails ENOENT and the file is stranded,
/// misreported as a benign "missing" although the quarantine path is alive.
/// Restore is the user's explicit undo: the parent chain is recreated (empty
/// dirs only), the file returns byte-identical to the ORIGINAL path, and only
/// a missing QUARANTINE path is reported missing.
#[test]
fn restore_recreates_deleted_original_parent() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    // Multi-level parent chain — the whole `deep/nested` subtree is deleted.
    let f1 = vol.path().join("deep").join("nested").join("a.bin");
    fs::create_dir_all(f1.parent().unwrap()).unwrap();
    fs::write(&f1, vec![0xAu8; 100]).unwrap();
    let mut ex1 = open_executor(
        "plan-1",
        vec![entry("C-001", f1.clone(), 100)],
        vol.path(),
        home.path(),
    );
    let (tx, _rx) = crossbeam_channel::unbounded();
    let op1 = ex1.apply(tx, None).unwrap().op_id;
    drop(ex1); // release the home flock
    assert!(!f1.exists(), "quarantined");

    // The user deleted the original parent subtree while the file sat in quarantine.
    fs::remove_dir_all(vol.path().join("deep")).unwrap();

    let mut ex2 = open_executor("plan-1", vec![], vol.path(), home.path());
    let (tx, rx) = crossbeam_channel::unbounded();
    let rep = ex2.restore(&op1, tx.clone()).unwrap();
    let events = drain(tx, rx);
    drop(ex2);

    assert_eq!(
        rep.restored,
        vec!["C-001".to_string()],
        "quarantine path was alive — the entry restores"
    );
    assert!(
        rep.missing.is_empty(),
        "a deleted ORIGINAL PARENT is not a benign missing"
    );
    assert!(rep.collided.is_empty());
    // The parent chain is recreated and the file lands at the ORIGINAL path.
    assert_eq!(
        fs::read(&f1).unwrap(),
        vec![0xAu8; 100],
        "byte-identical at the original path"
    );
    assert!(
        !quarantine_root(vol.path()).join(&op1).exists(),
        "op dir removed when empty"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        ExecEvent::EntryRestored { cid, to }
            if cid == "C-001" && to == &f1.display().to_string()
    )));
}

/// C1: a WAL at the SIP-fallback anchor — a writable ANCESTOR of the mount
/// (production: `~/.Trash-janitor` on the boot volume), NOT the mount root —
/// with NO ledger rec referencing it (crash between rename and ledger append)
/// is still discovered by recover() through the parent-chain probes, and the
/// landed rename rolls forward.
#[test]
fn recover_finds_wal_at_fallback_anchor_without_ledger_rec() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let base = TempDir::new().unwrap();
    let home = base.path().join("home");
    fs::create_dir(&home).unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", &home) };
    let vol = base.path().join("vol");
    fs::create_dir(&vol).unwrap();

    // Simulate the crash state directly: intent recorded at
    // <base>/.Trash-janitor (the fallback anchor — vol's writable parent),
    // rename landed, result and home-ledger apply rec never written.
    let root = base.path().join(".Trash-janitor");
    let src = vol.join("a.bin");
    fs::write(&src, vec![0xAu8; 100]).unwrap();
    let op_dir = root.join("op-c1");
    fs::create_dir_all(&op_dir).unwrap();
    let qpath = op_dir.join("a.bin");
    fs::rename(&src, &qpath).unwrap();
    let mut store = ledger::open(&root).unwrap();
    store
        .wal_intent(&WalEntry {
            op_id: "op-c1".into(),
            kind: WalKind::Intent,
            plan_id: "p1".into(),
            entry: path_hash(&src),
            from: Some(src.clone()),
            to: Some(qpath.clone()),
            bytes: 100,
            undo_of: None,
        })
        .unwrap();
    drop(store); // release the per-volume WAL flock
    // The home ledger.jsonl does not exist — NO record references the root.

    let rr = execute::recover(&[vol_info(&vol)]);
    assert!(
        rr.volumes_checked >= 1,
        "the fallback anchor must be probed"
    );
    assert_eq!(
        rr.rolled_forward, 1,
        "landed rename at the fallback anchor rolled forward"
    );
    assert_eq!(rr.dangling_intents.len(), 0);
    let wal = wal_entries(&root.join("wal.jsonl"));
    assert_eq!(wal.len(), 2, "intent + rolled-forward result");
    assert_eq!(wal[1].kind, WalKind::Result);
    assert_eq!(wal[1].from, Some(src));
    assert_eq!(wal[1].to, Some(qpath));
}

/// Zero-quarantine plan: apply succeeds with an empty report, creates NO
/// quarantine root and NO per-volume WAL; purge(0) and restore are empty
/// no-ops (SPEC §5.2 — nothing durable for a no-op batch).
#[test]
fn apply_zero_quarantine_entries_is_noop() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let mut ex = open_executor("plan-0", vec![], vol.path(), home.path());
    let (tx, rx) = crossbeam_channel::unbounded();
    let rep = ex.apply(tx.clone(), None).unwrap();
    let events = drain(tx, rx);
    assert_eq!(rep.applied, 0);
    assert_eq!(rep.skipped, 0);
    assert_eq!(rep.refused, 0);
    assert!(events.iter().any(|e| matches!(e, ExecEvent::Done)));
    assert!(
        !quarantine_root(vol.path()).exists(),
        "no quarantine root for an empty plan"
    );
    assert_eq!(
        collect_wal_files(vol.path()),
        Vec::<PathBuf>::new(),
        "no per-volume WAL created"
    );
    drop(ex); // release the home flock before a second Store opens on the same home

    // Non-quarantine actions only (all refused / prune commands): same no-op.
    let refused = PlanEntry {
        cid: "C-001".into(),
        path: vol.path().join("x.bin"),
        path_hash: [1; 8],
        alloc_bytes: 10,
        grade: Grade::PureCache,
        action: PlanAction::Refuse {
            reason: disk_janitor::Refusal::GradeNotAllowed,
        },
        evidence: vec![],
        inode: None,
    };
    let pruned = PlanEntry {
        cid: "C-002".into(),
        path: vol.path().join("y.bin"),
        path_hash: [2; 8],
        alloc_bytes: 10,
        grade: Grade::PureCache,
        action: PlanAction::PruneCommand {
            cmd: "echo".into(),
            dry_run: String::new(),
        },
        evidence: vec![],
        inode: None,
    };
    let mut ex2 = open_executor("plan-0b", vec![refused, pruned], vol.path(), home.path());
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep2 = ex2.apply(tx, None).unwrap();
    assert_eq!(rep2.applied, 0);
    assert!(!quarantine_root(vol.path()).exists());

    // Purge of an empty plan: typed 0 == expected 0 -> empty success.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let prep = ex2.purge(0, tx).unwrap();
    assert_eq!(prep.entries, 0);
    assert_eq!(prep.freed_bytes, 0);

    // Restore of an unknown op: empty report, no errors.
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rrep = ex2.restore("op-none", tx).unwrap();
    assert!(rrep.restored.is_empty() && rrep.missing.is_empty() && rrep.collided.is_empty());
}

/// Non-UTF8 filename: the JSONL WAL/ledger record paths through serde PathBuf
/// serialization, which rejects invalid UTF-8 — apply refuses such an entry
/// cleanly (Error event, sibling entries proceed) instead of mangling the name
/// mid-path or aborting the batch with a serde error. NOTE: APFS itself
/// rejects non-UTF8 names (EILSEQ, verified on this host), so the fixture
/// exists only as an OsString — the refusal fires before any FS access, which
/// is exactly the guard's contract for legacy/exFAT/network mounts.
#[test]
fn apply_refuses_non_utf8_filename() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    use std::ffi::OsStr;
    let bad_path = vol.path().join(OsStr::from_bytes(b"bad-\xFF.bin"));
    let good = vol.path().join("ok.bin");
    fs::write(&good, b"fine").unwrap();

    // The format boundary: serde cannot serialize a non-UTF8 path — this is
    // WHY the refusal exists (a mid-batch serde error would abort siblings).
    let w = WalEntry {
        op_id: "x".into(),
        kind: WalKind::Intent,
        plan_id: "p".into(),
        entry: path_hash(&bad_path),
        from: Some(bad_path.clone()),
        to: Some(vol.path().join("q")),
        bytes: 7,
        undo_of: None,
    };
    assert!(
        serde_json::to_vec(&w).is_err(),
        "WAL record with a non-UTF8 path cannot serialize"
    );

    let mut ex = open_executor(
        "plan-1",
        vec![
            entry("C-001", bad_path.clone(), 7),
            entry("C-002", good.clone(), 4),
        ],
        vol.path(),
        home.path(),
    );
    let (tx, rx) = crossbeam_channel::unbounded();
    let rep = ex.apply(tx.clone(), None).unwrap();
    let events = drain(tx, rx);

    assert_eq!(rep.applied, 1, "UTF-8 sibling proceeds");
    assert_eq!(rep.refused, 1, "non-UTF8 entry refused cleanly");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ExecEvent::Error { cid, .. } if cid == "C-001"))
    );
    // The sibling landed under its exact name; no lossy-mangled target exists.
    let root = quarantine_root(vol.path());
    let op_dir = root.join(&rep.op_id);
    let names: Vec<std::ffi::OsString> = fs::read_dir(&op_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("ok.bin")]);
    // The WAL records only the UTF-8 entry.
    let wal = wal_entries(&root.join("wal.jsonl"));
    assert_eq!(wal.len(), 2, "intent+result for the good entry only");
    assert!(
        wal.iter()
            .all(|w| w.from.as_deref() == Some(good.as_path()))
    );
}

/// Grace boundary (§5.2 step 4 + case 3): only strictly-past grace
/// (`grace_expires_at < now`) surfaces; entries purged or restored are excluded
/// even when the ledger rec is the ONLY marker — and a purge whose WAL result
/// committed but whose ledger rec crashed away is not double-counted. The
/// exclusion lists are rebuilt from replay on every startup; run recovery twice
/// to prove the answer is stable across restarts.
#[test]
fn recover_expired_grace_boundary_and_exclusions() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let root = quarantine_root(vol.path());
    let op_dir = root.join("op-grace");
    fs::create_dir_all(&op_dir).unwrap();

    let now = chrono::Utc::now();
    let past = now - chrono::Duration::hours(1);
    let future = now + chrono::Duration::hours(1);
    let apply_rec =
        |cid: &str, src: PathBuf, qpath: PathBuf, bytes: u64, grace| LedgerRecord::Apply {
            plan_id: "p1".into(),
            entry: entry(cid, src.clone(), bytes),
            original_path: src.clone(),
            quarantine_path: qpath,
            renamex_np: true,
            undo: ledger::Undo {
                original_path: src,
                byte_count: bytes,
                grace_expires_at: grace,
            },
        };

    let mut store = ledger::open(home.path()).unwrap();

    // (a) expired AND still quarantined -> surfaced.
    let a_src = vol.path().join("a.bin");
    let a_q = op_dir.join("a.bin");
    fs::write(&a_q, b"a").unwrap();
    store
        .append(
            Kind::Apply,
            &apply_rec("C-001", a_src.clone(), a_q.clone(), 100, past),
        )
        .unwrap();

    // (b) grace in the FUTURE + still quarantined -> NOT surfaced (only strictly
    //     past grace expires; `now` itself is not past).
    let b_q = op_dir.join("b.bin");
    fs::write(&b_q, b"b").unwrap();
    store
        .append(
            Kind::Apply,
            &apply_rec("C-002", vol.path().join("b.bin"), b_q, 200, future),
        )
        .unwrap();

    // (c) expired + purged (ledger purge rec) -> excluded.
    let c_q = op_dir.join("c.bin");
    fs::write(&c_q, b"c").unwrap();
    let c_src = vol.path().join("c.bin");
    let c_entry = entry("C-003", c_src.clone(), 300);
    store
        .append(
            Kind::Apply,
            &apply_rec("C-003", c_src.clone(), c_q.clone(), 300, past),
        )
        .unwrap();
    store
        .append(
            Kind::Purge,
            &LedgerRecord::Purge {
                plan_id: "p1".into(),
                entry: c_entry,
                bytes_freed: 300,
                quarantine_path: c_q.clone(),
            },
        )
        .unwrap();
    fs::remove_file(&c_q).unwrap();

    // (d) expired + restored (ledger restore rec) -> excluded.
    let d_q = op_dir.join("d.bin");
    fs::write(&d_q, b"d").unwrap();
    let d_src = vol.path().join("d.bin");
    let d_entry = entry("C-004", d_src.clone(), 400);
    let d_seq = store
        .append(
            Kind::Apply,
            &apply_rec("C-004", d_src.clone(), d_q.clone(), 400, past),
        )
        .unwrap();
    store
        .append(
            Kind::Restore,
            &LedgerRecord::Restore {
                plan_id: "p1".into(),
                entry: d_entry,
                from: d_q.clone(),
                to: d_src.clone(),
                undoes: d_seq,
            },
        )
        .unwrap();
    fs::remove_file(&d_q).unwrap();

    // (e) expired + file gone with NO purge/restore rec — the WAL purge result
    //     committed but the ledger rec crashed away (mid-purge crash): excluded
    //     by FS state, not double-counted.
    let e_q = op_dir.join("e.bin");
    fs::write(&e_q, b"e").unwrap();
    let e_src = vol.path().join("e.bin");
    store
        .append(
            Kind::Apply,
            &apply_rec("C-005", e_src.clone(), e_q.clone(), 500, past),
        )
        .unwrap();
    fs::remove_file(&e_q).unwrap();
    let mut wal_store = ledger::open(&root).unwrap();
    wal_store
        .wal_intent(&WalEntry {
            op_id: "purge-wal-crash".into(),
            kind: WalKind::Result,
            plan_id: "p1".into(),
            entry: path_hash(&e_src),
            from: None,
            to: None,
            bytes: 500,
            undo_of: None,
        })
        .unwrap();
    drop(wal_store);
    drop(store);

    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(
        rr.expired_grace.len(),
        1,
        "only the truly-expired survivor surfaces"
    );
    assert_eq!(rr.expired_grace[0].op_id, "op-grace");
    assert_eq!(rr.expired_grace[0].original_path, a_src);
    assert_eq!(rr.expired_grace[0].bytes, 100);
    // The purge-result WAL record is a Result, not an intent — nothing to reconcile.
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 0);

    // Across restarts the exclusion lists rebuild from the same replay — stable.
    let rr2 = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr2.expired_grace.len(), 1);
    assert_eq!(rr2.expired_grace[0].original_path, a_src);
}

/// Case 1: a torn WAL tail left by a crash mid-purge. The purge unlink landed
/// but the WAL result append was torn; recovery must park the torn bytes,
/// keep the committed apply pair, reconcile nothing (no dangling intent — the
/// apply pair is committed), and NOT surface the expired-grace entry whose
/// quarantine is gone (rule 3's else-branch: path absent => drop, not roll
/// forward).
#[test]
fn recover_parks_torn_tail_after_mid_purge_crash() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

    let root = quarantine_root(vol.path());
    let op_dir = root.join("op-torn");
    fs::create_dir_all(&op_dir).unwrap();
    let src = vol.path().join("a.bin");
    let qpath = op_dir.join("a.bin");
    fs::write(&qpath, b"quarantined").unwrap();

    // Committed apply pair in the per-volume WAL (intent + result).
    {
        let mut wal = ledger::open(&root).unwrap();
        let intent = WalEntry {
            op_id: "op-torn".into(),
            kind: WalKind::Intent,
            plan_id: "p1".into(),
            entry: path_hash(&src),
            from: Some(src.clone()),
            to: Some(qpath.clone()),
            bytes: 12,
            undo_of: None,
        };
        wal.wal_intent(&intent).unwrap();
        wal.wal_intent(&WalEntry {
            kind: WalKind::Result,
            ..intent.clone()
        })
        .unwrap();
    }
    // Home-ledger apply rec whose grace ALREADY expired (would surface if the
    // quarantine still existed).
    {
        let mut store = ledger::open(home.path()).unwrap();
        store
            .append(
                Kind::Apply,
                &LedgerRecord::Apply {
                    plan_id: "p1".into(),
                    entry: entry("C-001", src.clone(), 12),
                    original_path: src.clone(),
                    quarantine_path: qpath.clone(),
                    renamex_np: true,
                    undo: ledger::Undo {
                        original_path: src,
                        byte_count: 12,
                        grace_expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
                    },
                },
            )
            .unwrap();
    }
    // Mid-purge crash state: the unlink landed, the WAL result append was torn
    // (partial final line, no trailing newline).
    fs::remove_file(&qpath).unwrap();
    let wal_path = root.join("wal.jsonl");
    let torn_bytes = br#"{"v":1,"seq":99,"kind":"result","rec":{"op_id":"purge-torn""#;
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&wal_path).unwrap();
        f.write_all(torn_bytes).unwrap();
    }

    let rr = execute::recover(&[vol_info(vol.path())]);
    // recover() canonicalizes roots (/var -> /private/var); compare canonical.
    let wal_canon = wal_path.canonicalize().unwrap();
    assert_eq!(
        rr.torn_tail.as_deref(),
        Some(wal_canon.as_path()),
        "torn tail reported before truncation"
    );
    let parked = wal_path.with_file_name("wal.jsonl.torn");
    assert!(parked.exists(), "torn bytes parked");
    assert_eq!(fs::read(&parked).unwrap(), torn_bytes.to_vec());
    // Committed pair intact after repair; nothing to reconcile, nothing rolled forward.
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 2, "intent+result survive the torn-tail repair");
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 0);
    // The expired-grace rec is NOT surfaced: its quarantine is gone (rule 3
    // else-branch), even though no purge rec exists in the ledger.
    assert!(rr.expired_grace.is_empty());
}
