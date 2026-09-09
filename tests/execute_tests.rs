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
use disk_janitor::{Grade, PlanAction, PlanEntry, VolumeInfo, WalKind, path_hash};
use tempfile::TempDir;

/// UF_HIDDEN (macOS <sys/stat.h>) — execute::macos keeps its consts crate-private.
const UF_HIDDEN: u32 = 0x0000_8000;

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
    }
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
