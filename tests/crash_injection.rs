//! Crash-injection tests (M2 exit criterion): the test binary re-executes ITSELF
//! as a child with `KILL_AT=<point>` + `--crash-run`; the child applies a plan
//! and `process::exit(137)` fires exactly at the named kill point (after the
//! sync step that precedes it), simulating kill -9 there. The parent then runs
//! `execute::recover()` and asserts the state reconciles per SPEC §5.2:
//! quarantine exists ⇒ roll forward; intent without result and nothing moved ⇒
//! drop tail, source intact, no phantom quarantine.

#![cfg(target_os = "macos")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crossbeam_channel;
use disk_janitor::execute::{self, ExecEvent};
use disk_janitor::ledger::{self, Kind};
use disk_janitor::{Grade, PlanAction, PlanEntry, VolumeInfo, WalKind, path_hash};
use tempfile::TempDir;

const CRASH_FLAG: &str = "CRASH_CHILD";
const A_BYTES: u64 = 128;
const B_BYTES: u64 = 256;

/// Parse a per-volume wal.jsonl (LedgerStore envelope format) into bare entries.
fn wal_entries(path: &Path) -> Vec<disk_janitor::WalEntry> {
    ledger::record_iter(path)
        .into_iter()
        .map(|(_, w)| w)
        .collect()
}

/// env is process-global; parent-side tests serialize on this while setting
/// DISK_JANITOR_HOME. Children get their env via Command::env (copy + override),
/// so they never race.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn is_crash_child() -> bool {
    // Env-var detection: the libtest harness rejects unknown args, so the
    // parent signals child mode through the environment instead.
    std::env::var_os(CRASH_FLAG).is_some()
}

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

fn build_entries(vol: &Path) -> Vec<PlanEntry> {
    vec![
        entry("C-001", vol.join("data-a.bin"), A_BYTES),
        entry("C-002", vol.join("data-b.bin"), B_BYTES),
    ]
}

fn create_files(vol: &Path) {
    fs::write(vol.join("data-a.bin"), vec![0xABu8; A_BYTES as usize]).unwrap();
    fs::write(vol.join("data-b.bin"), vec![0xCDu8; B_BYTES as usize]).unwrap();
}

/// Spawn this same test binary as the crash child; returns its exit code.
fn spawn_crash(kill_point: &str, home: &Path, vol: &Path, plan_id: &str) -> Option<i32> {
    let status = Command::new(std::env::current_exe().expect("test binary path"))
        .env(CRASH_FLAG, "1")
        .env("KILL_AT", kill_point)
        .env("CHILD_HOME", home)
        .env("CHILD_VOL", vol)
        .env("CHILD_PLAN_ID", plan_id)
        .env("DISK_JANITOR_HOME", home)
        .status()
        .expect("spawn crash child");
    status.code()
}

/// Child-mode body: apply the plan; `apply` exits 137 at the KILL_AT point.
fn child_apply(home: &Path, vol: &Path, plan_id: &str) {
    let store = ledger::open(home).expect("child home ledger");
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol),
        &[vol_info(vol)],
        store,
    )
    .expect("child executor");
    let (tx, _rx) = crossbeam_channel::unbounded::<ExecEvent>();
    let _ = ex.apply(tx, None);
    // Unreachable when KILL_AT is honored — the kill fires inside apply.
    panic!("child apply returned without hitting the kill point");
}

fn child_env_path(key: &str) -> PathBuf {
    PathBuf::from(std::env::var(key).expect(key))
}

/// `--crash-run` dispatch shared by every case: only the fn whose KILL_AT
/// matches runs the child scenario; the rest no-op so the child harness passes
/// quickly before the exit(137).
fn crash_dispatch(point: &str) -> bool {
    if is_crash_child() && std::env::var("KILL_AT").as_deref() == Ok(point) {
        let home = child_env_path("CHILD_HOME");
        let vol = child_env_path("CHILD_VOL");
        let plan_id = std::env::var("CHILD_PLAN_ID").expect("CHILD_PLAN_ID");
        if point == "MidPurge" {
            child_apply_then_purge(&home, &vol, &plan_id);
        } else {
            child_apply(&home, &vol, &plan_id);
        }
    }
    is_crash_child()
}

/// KillPoint::BeforeWalSync — exit before the first intent sync: nothing durable,
/// sources intact, recovery reports nothing.
#[test]
fn crash_before_wal_sync() {
    if crash_dispatch("BeforeWalSync") {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };
    create_files(vol.path());

    let code = spawn_crash("BeforeWalSync", home.path(), vol.path(), "crash-p1");
    assert_eq!(code, Some(137), "child dies at the kill point");

    assert!(vol.path().join("data-a.bin").exists());
    assert!(vol.path().join("data-b.bin").exists());
    let root = vol.path().join(".Trash-janitor");
    assert!(
        !root.exists(),
        "no quarantine root before the first intent sync"
    );

    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.volumes_checked, 0);
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 0);
    assert!(
        vol.path().join("data-a.bin").exists(),
        "state consistent: sources untouched"
    );
}

/// KillPoint::AfterWalSyncBeforeRename — intent synced, rename not executed:
/// WAL holds one committed intent, source still in place, no phantom
/// quarantine; recovery drops the tail and reports the dangling intent.
#[test]
fn crash_after_wal_sync_before_rename() {
    if crash_dispatch("AfterWalSyncBeforeRename") {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };
    create_files(vol.path());
    let plan_id = "crash-p2";

    let code = spawn_crash("AfterWalSyncBeforeRename", home.path(), vol.path(), plan_id);
    assert_eq!(code, Some(137), "child dies at the kill point");

    let root = vol.path().join(".Trash-janitor");
    let wal_path = root.join("wal.jsonl");
    assert!(wal_path.exists(), "intent synced before the kill");
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 1, "exactly the first intent committed");
    assert_eq!(wal[0].kind, WalKind::Intent);
    let op_id = wal[0].op_id.clone();
    assert_eq!(wal[0].entry, path_hash(&vol.path().join("data-a.bin")));

    // No phantom quarantine: source intact, target absent.
    assert_eq!(
        fs::read(vol.path().join("data-a.bin")).unwrap(),
        vec![0xABu8; A_BYTES as usize]
    );
    assert!(
        !root.join(&op_id).join("data-a.bin").exists(),
        "rename never executed"
    );
    assert!(
        vol.path().join("data-b.bin").exists(),
        "second entry never started"
    );

    // Recovery: dangling intent reported, nothing moved, state consistent.
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.volumes_checked, 1);
    assert_eq!(rr.dangling_intents.len(), 1, "dangling intent reported");
    assert_eq!(rr.dangling_intents[0].op_id, op_id);
    assert_eq!(rr.rolled_forward, 0);
    assert!(
        vol.path().join("data-a.bin").exists(),
        "recovery leaves the source in place"
    );
    assert_eq!(
        fs::read(vol.path().join("data-a.bin")).unwrap(),
        vec![0xABu8; A_BYTES as usize]
    );

    // Restore of the dangling op restores nothing and breaks nothing.
    let store = ledger::open(home.path()).unwrap();
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol.path()),
        &[vol_info(vol.path())],
        store,
    )
    .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.restore(&op_id, tx).unwrap();
    assert!(
        rep.restored.is_empty() && rep.missing.is_empty(),
        "no pairs to restore"
    );
    assert!(vol.path().join("data-a.bin").exists());
}

/// KillPoint::AfterRenameBeforeResult — rename landed, WAL result lost:
/// quarantine exists ⇒ recovery rolls forward (appends the result), then a
/// restore returns the file byte-identical.
#[test]
fn crash_after_rename_before_result() {
    if crash_dispatch("AfterRenameBeforeResult") {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };
    create_files(vol.path());
    let plan_id = "crash-p3";

    let code = spawn_crash("AfterRenameBeforeResult", home.path(), vol.path(), plan_id);
    assert_eq!(code, Some(137), "child dies at the kill point");

    let root = vol.path().join(".Trash-janitor");
    let wal_path = root.join("wal.jsonl");
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 1);
    assert_eq!(wal[0].kind, WalKind::Intent);
    let op_id = wal[0].op_id.clone();

    // Rename executed but the result record was never written.
    assert!(!vol.path().join("data-a.bin").exists(), "source moved");
    let qpath = root.join(&op_id).join("data-a.bin");
    assert!(qpath.exists(), "quarantine landed");
    assert_eq!(fs::read(&qpath).unwrap(), vec![0xABu8; A_BYTES as usize]);
    assert!(
        vol.path().join("data-b.bin").exists(),
        "second entry never started"
    );

    // Recovery: quarantine exists ⇒ roll forward, not drop.
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 1, "result appended by recovery");
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 2, "intent + rolled-forward result");
    assert_eq!(wal[1].kind, WalKind::Result);
    assert!(wal[1].from.is_some() && wal[1].to.is_some());

    // Now a restore reverses the move byte-identically. The child died before
    // the home-ledger apply rec, so the cid is a synthetic op-#i fallback —
    // assert the observable contract: exactly the quarantined file restored.
    let store = ledger::open(home.path()).unwrap();
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol.path()),
        &[vol_info(vol.path())],
        store,
    )
    .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.restore(&op_id, tx).unwrap();
    assert_eq!(rep.restored.len(), 1, "exactly the landed rename restored");
    assert!(rep.missing.is_empty());
    assert_eq!(
        fs::read(vol.path().join("data-a.bin")).unwrap(),
        vec![0xABu8; A_BYTES as usize]
    );
    assert!(!qpath.exists());
}

/// KillPoint::MidBatch — first entry fully committed (WAL intent+result, ledger
/// apply rec), second entry untouched; recovery reconciles without action.
#[test]
fn crash_mid_batch() {
    if crash_dispatch("MidBatch") {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };
    create_files(vol.path());
    let plan_id = "crash-p4";

    let code = spawn_crash("MidBatch", home.path(), vol.path(), plan_id);
    assert_eq!(code, Some(137), "child dies at the kill point");

    let root = vol.path().join(".Trash-janitor");
    let wal_path = root.join("wal.jsonl");
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 2, "first entry committed: intent + result");
    assert_eq!(wal[0].kind, WalKind::Intent);
    assert_eq!(wal[1].kind, WalKind::Result);
    let op_id = wal[0].op_id.clone();
    assert_eq!(wal[0].entry, path_hash(&vol.path().join("data-a.bin")));
    assert_eq!(wal[1].entry, wal[0].entry);

    // Entry 1 quarantined; entry 2 never started.
    let qpath = root.join(&op_id).join("data-a.bin");
    assert!(qpath.exists());
    assert_eq!(fs::read(&qpath).unwrap(), vec![0xABu8; A_BYTES as usize]);
    assert!(!vol.path().join("data-a.bin").exists());
    assert!(vol.path().join("data-b.bin").exists());

    // Home ledger: the apply rec is best-effort history — append() is not
    // required to be crash-durable (the synced WAL result is the durability
    // point), so a kill -9 here may drop the in-flight rec. At most one.
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    let applies = records.iter().filter(|r| r.kind == Kind::Apply).count();
    assert!(
        applies <= 1,
        "at most the committed entry's history rec: {applies}"
    );

    // Recovery: nothing dangling, nothing to roll forward.
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 0);

    // Restore brings entry 1 back; entry 2 stays untouched.
    let store = ledger::open(home.path()).unwrap();
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol.path()),
        &[vol_info(vol.path())],
        store,
    )
    .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.restore(&op_id, tx).unwrap();
    assert_eq!(rep.restored.len(), 1, "exactly entry 1 restored");
    assert!(rep.missing.is_empty());
    assert_eq!(
        fs::read(vol.path().join("data-a.bin")).unwrap(),
        vec![0xABu8; A_BYTES as usize]
    );
    assert_eq!(
        fs::read(vol.path().join("data-b.bin")).unwrap(),
        vec![0xCDu8; B_BYTES as usize]
    );
}

/// Child-mode body: apply fully, then purge; `purge` exits 137 at MidPurge.
fn child_apply_then_purge(home: &Path, vol: &Path, plan_id: &str) {
    let store = ledger::open(home).expect("child home ledger");
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol),
        &[vol_info(vol)],
        store,
    )
    .expect("child executor");
    let (tx, _rx) = crossbeam_channel::unbounded::<ExecEvent>();
    ex.apply(tx, None).expect("child apply");
    let (tx, _rx) = crossbeam_channel::unbounded::<ExecEvent>();
    let _ = ex.purge(A_BYTES + B_BYTES, tx);
    // Unreachable when KILL_AT is honored — the kill fires inside purge.
    panic!("child purge returned without hitting the kill point");
}

/// KillPoint::MidPurge — the first entry is FULLY purged (unlink + WAL result +
/// ledger rec), the second is still quarantined. Asserts the per-entry WAL
/// ordering (case 4: results are appended after each unlink, not batched at the
/// end — exactly one purge result exists) and that recovery + a re-purge
/// reconcile the crash idempotently (case 1: no dangling intent, no roll
/// forward — the apply pair is committed; the freed entry is skipped on re-run).
#[test]
fn crash_mid_purge() {
    if crash_dispatch("MidPurge") {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };
    create_files(vol.path());
    let plan_id = "crash-p5";

    let code = spawn_crash("MidPurge", home.path(), vol.path(), plan_id);
    assert_eq!(code, Some(137), "child dies at the kill point");

    let root = vol.path().join(".Trash-janitor");
    let wal_path = root.join("wal.jsonl");
    let wal = wal_entries(&wal_path);
    let op_id = wal
        .iter()
        .find(|w| w.kind == WalKind::Intent)
        .expect("apply intent")
        .op_id
        .clone();
    // Entry 1 fully purged; entry 2 still quarantined.
    assert!(
        !root.join(&op_id).join("data-a.bin").exists(),
        "entry 1 purged before the crash"
    );
    let q2 = root.join(&op_id).join("data-b.bin");
    assert!(q2.exists(), "entry 2 survives the crash");
    assert_eq!(fs::read(&q2).unwrap(), vec![0xCDu8; B_BYTES as usize]);
    assert!(!vol.path().join("data-a.bin").exists());
    // WAL ordering (case 4): apply pair per entry + EXACTLY ONE purge result —
    // per-entry appends, not a batch at the end.
    let purge_results = wal
        .iter()
        .filter(|w| w.kind == WalKind::Result && w.from.is_none() && w.to.is_none())
        .count();
    assert_eq!(
        purge_results, 1,
        "one WAL result for the one completed purge"
    );
    // Ledger: exactly one purge rec (the second entry's rec never landed).
    let (records, _) = ledger::replay(&home.path().join("ledger.jsonl")).unwrap();
    assert_eq!(records.iter().filter(|r| r.kind == Kind::Purge).count(), 1);

    // Recovery: the apply pairs are committed and the lost purge result needs
    // no roll-forward (rule 3's else-branch: path gone, nothing dangling).
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 0);
    assert!(rr.expired_grace.is_empty());

    // Re-purge after the crash: the freed entry is skipped idempotently; only
    // the survivor is freed.
    let store = ledger::open(home.path()).unwrap();
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol.path()),
        &[vol_info(vol.path())],
        store,
    )
    .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.purge(A_BYTES + B_BYTES, tx).unwrap();
    assert_eq!(rep.freed_bytes, B_BYTES, "only the surviving entry freed");
    assert_eq!(rep.entries, 1);
    assert!(!q2.exists());
    let wal = wal_entries(&wal_path);
    assert_eq!(
        wal.iter()
            .filter(|w| w.kind == WalKind::Result && w.from.is_none())
            .count(),
        2,
        "second purge appends its own result"
    );
}

/// Case 7: re-running the SAME plan after a MidBatch crash. The committed entry
/// is skipped (source ENOENT — benign, before any WAL write), only the missing
/// entry is applied under a NEW op id; the WAL ends with exactly two
/// intent/result pairs — no duplicates for the already-committed entry.
#[test]
fn crash_mid_batch_then_reapply_applies_only_missing() {
    // In the crash child, crash_mid_batch owns the MidBatch dispatch (one
    // dispatcher per point — a second concurrent dispatch would race on the
    // home flock); just stand aside and let it kill the process.
    if is_crash_child() {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = TempDir::new().unwrap();
    let vol = TempDir::new().unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };
    create_files(vol.path());
    let plan_id = "crash-p6";

    let code = spawn_crash("MidBatch", home.path(), vol.path(), plan_id);
    assert_eq!(code, Some(137), "child dies at the kill point");

    let root = vol.path().join(".Trash-janitor");
    let wal_path = root.join("wal.jsonl");
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 2, "first entry committed before the crash");
    let op1 = wal[0].op_id.clone();
    let h1 = path_hash(&vol.path().join("data-a.bin"));
    assert!(
        vol.path().join("data-b.bin").exists(),
        "second entry untouched"
    );

    // Re-run the SAME plan.
    let store = ledger::open(home.path()).unwrap();
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol.path()),
        &[vol_info(vol.path())],
        store,
    )
    .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.apply(tx, None).unwrap();
    assert_eq!(rep.applied, 1, "only the missing entry applied");
    assert_eq!(rep.skipped, 1, "committed entry skipped as already moved");
    assert!(!vol.path().join("data-a.bin").exists());
    assert!(!vol.path().join("data-b.bin").exists());
    assert!(
        root.join(&op1).join("data-a.bin").exists(),
        "first op intact"
    );

    // No duplicate pairs: each path hash appears exactly twice (intent+result),
    // the first op's records all under op1, the second op's under one NEW op.
    let wal = wal_entries(&wal_path);
    assert_eq!(wal.len(), 4);
    let h2 = path_hash(&vol.path().join("data-b.bin"));
    let a_recs: Vec<&disk_janitor::WalEntry> = wal.iter().filter(|w| w.entry == h1).collect();
    assert_eq!(a_recs.len(), 2, "committed entry: no duplicate WAL records");
    assert!(a_recs.iter().all(|w| w.op_id == op1));
    let b_recs: Vec<&disk_janitor::WalEntry> = wal.iter().filter(|w| w.entry == h2).collect();
    assert_eq!(b_recs.len(), 2);
    assert!(
        b_recs
            .iter()
            .all(|w| w.op_id == b_recs[0].op_id && w.op_id != op1),
        "re-applied entry under a fresh op"
    );
    assert_eq!(
        b_recs.iter().filter(|w| w.kind == WalKind::Intent).count(),
        1
    );
    assert_eq!(
        b_recs.iter().filter(|w| w.kind == WalKind::Result).count(),
        1
    );

    // Recovery: everything committed, nothing dangling.
    let rr = execute::recover(&[vol_info(vol.path())]);
    assert_eq!(rr.dangling_intents.len(), 0);
    assert_eq!(rr.rolled_forward, 0);
}
