//! Crash-injection tests (M2 exit criterion): the test binary re-executes ITSELF
//! as a child with `KILL_AT=<point>` + `--crash-run`; the child applies a plan
//! and `process::exit(137)` fires exactly at the named kill point (after the
//! sync step that precedes it), simulating kill -9 there. The parent then runs
//! `execute::recover()` and asserts the state reconciles per SPEC §5.2:
//! quarantine exists ⇒ roll forward; intent without result and nothing moved ⇒
//! drop tail, source intact, no phantom quarantine.

#![cfg(target_os = "macos")]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel;
use disk_janitor::execute::{self, ExecEvent, KillPoint};
use disk_janitor::ledger::{self, Kind};
use disk_janitor::{Grade, PathHash, PlanAction, PlanEntry, VolumeInfo, WalKind, path_hash};
use tempfile::TempDir;

const CRASH_FLAG: &str = "CRASH_CHILD";
const A_BYTES: u64 = 128;
const B_BYTES: u64 = 256;

/// The sweep child touches this file immediately before calling `apply()`.
/// The parent polls for it before starting the kill clock, so the random delay
/// lands inside `apply`'s durable window (measured 19-57 ms for a 2-entry
/// plan) instead of inside dyld/libtest startup, which is ~300 ms of the
/// ~350 ms child lifetime. Without the anchor the sweep is vacuous: a 0-60 ms
/// sleep from `spawn()` almost always kills before any durable work happens.
const SWEEP_READY: &str = "sweep-ready";

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
        inode: None,
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

/// Child-mode body for the row-56 sweep. Runs a FULL apply with no named kill
/// point (`KILL_AT="Sweep"` parses to `None`, so `apply` runs to completion) —
/// the parent SIGKILLs this process at a random moment instead. The ready
/// sentinel is written BEFORE `apply`, i.e. after the executor is built but
/// before the first WAL intent, so the parent's kill window straddles the
/// durable work rather than the process startup.
///
/// No `panic!` tail (unlike `child_apply`): this child is *expected* to either
/// die mid-apply or finish normally, and a finished child must exit 0 so the
/// parent can distinguish "swept past the end" from a real failure.
fn child_apply_sweep(home: &Path, vol: &Path, plan_id: &str) {
    let store = ledger::open(home).expect("child home ledger");
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(vol),
        &[vol_info(vol)],
        store,
    )
    .expect("child executor");
    fs::write(home.join(SWEEP_READY), b"1").expect("ready sentinel");
    let (tx, _rx) = crossbeam_channel::unbounded::<ExecEvent>();
    let _ = ex.apply(tx, None);
}

/// Spawn the sweep child WITHOUT waiting (`.spawn()`, not `.status()`): the
/// parent kills it at a random offset. stdout/stderr are discarded so a swept
/// child's own libtest chatter can never interleave with the parent's.
fn spawn_sweep(home: &Path, vol: &Path, plan_id: &str) -> Child {
    Command::new(std::env::current_exe().expect("test binary path"))
        .env(CRASH_FLAG, "1")
        .env("KILL_AT", "Sweep")
        .env("CHILD_HOME", home)
        .env("CHILD_VOL", vol)
        .env("CHILD_PLAN_ID", plan_id)
        .env("DISK_JANITOR_HOME", home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sweep child")
}

/// True when `dir` (or its probe file) is writable by us — the same probe
/// `execute::quarantine_root_for_source` uses while walking for the
/// SIP-fallback anchor.
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".janitor-probe-{}", std::process::id()));
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

/// The TOPMOST ancestor of `dir` that is on the same device and writable by
/// us — the same rule `execute` uses to pick the SIP-fallback quarantine
/// anchor (`quarantine_root_for_source`). Used by the child to pre-create the
/// anchor exactly where apply will resolve it.
fn topmost_writable_ancestor(dir: &Path) -> PathBuf {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let dev = fs::metadata(&dir)
        .map(|m| std::os::unix::fs::MetadataExt::dev(&m))
        .unwrap();
    let mut best = dir.clone();
    let mut cur: Option<&Path> = Some(dir.as_path());
    while let Some(p) = cur {
        if let Ok(md) = fs::metadata(p) {
            if std::os::unix::fs::MetadataExt::dev(&md) == dev && dir_writable(p) {
                best = p.to_path_buf();
            }
        }
        cur = p.parent();
    }
    best
}

/// Child-mode body for the SIP-fallback case (C1): the volume's MOUNT ROOT is
/// locked read-only (SIP-style — the volume table still reports it writable),
/// so apply's primary root `<mount>/.Trash-janitor` cannot be created and the
/// quarantine lands at the fallback anchor: a DIFFERENT writable ancestor's
/// `.Trash-janitor`, pre-created here like production leaves it. The kill is
/// passed directly (KILL_AT="SipFallback" is a dispatch-only label this test
/// owns; no other test shares the point), firing at AfterRenameBeforeResult —
/// intent + landed rename, no WAL result, no home-ledger apply rec.
///
/// The anchor is `/private/tmp/.Trash-janitor` — the test's vol+home live in a
/// tempdir under `/private/tmp`, whose topmost writable same-dev ancestor is
/// `/private/tmp` itself (1777) — which is exactly the DOCUMENTED fallback
/// anchor for `/tmp` sources (discover_quarantine_roots probe 3). It is
/// private to this test: no other test's mount/home chain crosses it, so no
/// WAL residue can leak into their recover() counts.
fn child_apply_sip_fallback(home: &Path, vol: &Path, plan_id: &str) {
    // Sources live in a writable subdir of the read-only mount root.
    let sub = vol.join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("data-a.bin"), vec![0xABu8; A_BYTES as usize]).unwrap();
    fs::write(sub.join("data-b.bin"), vec![0xCDu8; B_BYTES as usize]).unwrap();
    // Pre-create the fallback quarantine root at the topmost writable same-dev
    // ancestor (production: ~/.Trash-janitor on the boot volume), clearing any
    // residue a crashed parent run may have left behind (this anchor is only
    // ever written by this test).
    let anchor = topmost_writable_ancestor(vol);
    let root = anchor.join(".Trash-janitor");
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    // Lock the mount root read-only, exactly like SIP does to /.
    let mut perms = fs::metadata(vol).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(vol, perms).unwrap();

    let store = ledger::open(home).expect("child home ledger");
    let mut ex = execute::open(
        plan_id.to_string(),
        vec![
            entry("C-001", sub.join("data-a.bin"), A_BYTES),
            entry("C-002", sub.join("data-b.bin"), B_BYTES),
        ],
        &[vol_info(vol)],
        store,
    )
    .expect("child executor");
    let (tx, _rx) = crossbeam_channel::unbounded::<ExecEvent>();
    let _ = ex.apply(tx, Some(KillPoint::AfterRenameBeforeResult));
    // Unreachable when the kill is honored — the kill fires inside apply.
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
        if point == "SipFallback" {
            child_apply_sip_fallback(&home, &vol, &plan_id);
        } else if point == "MidPurge" {
            child_apply_then_purge(&home, &vol, &plan_id);
        } else if point == "Sweep" {
            child_apply_sweep(&home, &vol, &plan_id);
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

/// Walk up from `dir` and return the single ancestor holding a wal-bearing
/// `.Trash-janitor` — the fallback anchor the SIP-fallback child quarantined
/// into. The anchor sits OUTSIDE every TempDir (topmost writable same-dev
/// ancestor of the tempdir chain), so the test removes it explicitly.
fn find_fallback_anchor(dir: &Path) -> PathBuf {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut cur = Some(dir);
    while let Some(p) = cur {
        let root = p.join(".Trash-janitor");
        if root.join("wal.jsonl").exists() {
            roots.push(root);
        }
        cur = p.parent();
    }
    assert_eq!(
        roots.len(),
        1,
        "exactly one fallback anchor holds the WAL (got {roots:?})"
    );
    roots.into_iter().next().expect("asserted non-empty")
}

/// C1 SIP-fallback recovery: the volume's mount root is locked read-only
/// (SIP-style, though the volume table reports it writable), so apply
/// quarantines at the fallback anchor — a DIFFERENT writable ancestor's
/// `.Trash-janitor` — and the crash between rename and ledger append leaves
/// the WAL there with NO ledger rec referencing it. recover() must find the
/// anchor through the parent-chain probes and roll the landed rename forward;
/// restore() (same discovery) then brings the file back byte-identical.
///
/// vol+home live under a tempdir in `/private/tmp`, so the anchor resolves to
/// the DOCUMENTED `/private/tmp/.Trash-janitor` fallback (1777 /tmp is the
/// topmost writable same-dev ancestor) — private to this test's probe chains,
/// so the WAL can never leak into another test's recover() counts.
#[test]
fn crash_sip_fallback_anchor_recovered() {
    if crash_dispatch("SipFallback") {
        return;
    }
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let base = tempfile::Builder::new()
        .tempdir_in("/private/tmp")
        .expect("tempdir under /private/tmp");
    let home = base.path().join("home");
    fs::create_dir(&home).unwrap();
    let vol = base.path().join("vol");
    fs::create_dir(&vol).unwrap();
    unsafe { std::env::set_var("DISK_JANITOR_HOME", &home) };
    let plan_id = "crash-sip";

    let code = spawn_crash("SipFallback", &home, &vol, plan_id);
    assert_eq!(code, Some(137), "child dies at the kill point");

    // The mount root is read-only, so the PRIMARY root was never created; the
    // WAL landed at exactly one fallback anchor above the mount.
    assert!(
        !vol.join(".Trash-janitor").exists(),
        "primary root never created under the read-only mount"
    );
    let anchor = find_fallback_anchor(&vol);
    assert_eq!(
        anchor,
        PathBuf::from("/private/tmp/.Trash-janitor"),
        "the /tmp-source fallback anchor is where the quarantine lands"
    );
    assert!(
        !vol.join("sub").join("data-a.bin").exists(),
        "source moved before the crash"
    );
    let wal = wal_entries(&anchor.join("wal.jsonl"));
    assert_eq!(wal.len(), 1, "intent synced, result lost in the crash");
    assert_eq!(wal[0].kind, WalKind::Intent);
    let op_id = wal[0].op_id.clone();
    let qpath = anchor.join(&op_id).join("data-a.bin");
    assert!(qpath.exists(), "rename landed at the fallback anchor");
    assert_eq!(fs::read(&qpath).unwrap(), vec![0xABu8; A_BYTES as usize]);

    // Recovery finds the anchor with NO ledger rec referencing it and rolls
    // the landed rename forward.
    let rr = execute::recover(&[vol_info(&vol)]);
    assert!(rr.volumes_checked >= 1, "fallback anchor must be probed");
    assert_eq!(
        rr.rolled_forward, 1,
        "crash-between-rename-and-ledger state reconciled"
    );
    assert_eq!(rr.dangling_intents.len(), 0);
    let wal = wal_entries(&anchor.join("wal.jsonl"));
    assert_eq!(wal.len(), 2, "intent + rolled-forward result");
    assert_eq!(wal[1].kind, WalKind::Result);

    // Restore shares the same discovery: the file comes back byte-identical.
    let store = ledger::open(&home).unwrap();
    let mut ex = execute::open(
        plan_id.to_string(),
        build_entries(&vol),
        &[vol_info(&vol)],
        store,
    )
    .unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let rep = ex.restore(&op_id, tx).unwrap();
    assert_eq!(rep.restored.len(), 1, "exactly the landed rename restored");
    assert!(rep.missing.is_empty());
    assert_eq!(
        fs::read(vol.join("sub").join("data-a.bin")).unwrap(),
        vec![0xABu8; A_BYTES as usize]
    );

    // Cleanup: the fallback anchor is OUTSIDE the tempdir base; restore the
    // mount root's permissions for the TempDir drop.
    fs::remove_dir_all(&anchor).unwrap();
    let mut perms = fs::metadata(&vol).unwrap().permissions();
    perms.set_readonly(false);
    fs::set_permissions(&vol, perms).unwrap();
}

/// Which durable boundary a killed child had crossed. Derived only from
/// observable artifacts (WAL lines + home-ledger recs), never from the child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SweepStage {
    /// Nothing synced — kill landed before the first WAL intent.
    PreDurable,
    /// An intent with no matching result — the `AfterWalSyncBeforeRename` shape.
    MidPair,
    /// Every intent has a result, but a home-ledger `Apply` rec is missing —
    /// the `src/execute.rs:688..691` gap that no named `KillPoint` covers.
    ResultNoRec,
    /// Every synced result already has its home-ledger `Apply` rec — no gap.
    /// Not a claim that the whole plan ran: a plan killed after entry 1 of 2
    /// lands here with a clean single-entry boundary.
    RecsComplete,
}

/// Every file under `root` (recursively) whose bytes are exactly `len`
/// repetitions of `pattern`. The sweep oracle matches by CONTENT, never by
/// filename: `pick_free_target`/`rename_with_retry` may append `-<n>` to a
/// quarantine name, so a name-keyed oracle would report a phantom loss.
fn locations_with_content(root: &Path, pattern: u8, len: u64) -> Vec<PathBuf> {
    let want = vec![pattern; len as usize];
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for dent in rd.flatten() {
            let p = dent.path();
            let Ok(md) = fs::symlink_metadata(&p) else {
                continue;
            };
            if md.is_dir() {
                stack.push(p);
            } else if fs::read(&p).is_ok_and(|b| b == want) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Row 56 — the random-moment kill sweep: an UNNAMED kill point, N iterations.
///
/// The five named `KillPoint` values can only fire at five hand-placed sites, so
/// a bug living in the gaps between them is unreachable by construction — most
/// notably the window between the WAL Result sync and the home-ledger `Apply`
/// append (`src/execute.rs:688..691`), which no named point covers. This test
/// SIGKILLs the child at a uniformly random moment instead, and checks the one
/// invariant that must hold at EVERY landing site: **byte conservation** — each
/// fixture exists, with its exact content, in exactly one place (source or
/// quarantine). Zero locations means lost data; two means a phantom copy.
///
/// Anchoring: the kill clock starts only after the child touches the ready
/// sentinel, because ~300 ms of the ~350 ms child lifetime is dyld/libtest
/// startup — a sleep measured from `spawn()` would almost always kill before
/// any durable work. `apply()` itself takes 19-57 ms for this 2-entry plan
/// (measured), which is what the 0-61 ms window is calibrated against.
///
/// What this test deliberately does NOT assert: that the quarantine root is
/// empty after recovery (a kill between `mkdir op_dir` and the first intent
/// legally leaves a stray empty op dir that `recover()` only removes on the
/// dangling branch), and that every quarantined entry has a home-ledger
/// `Apply` rec (a kill in the Result→append gap legitimately leaves none —
/// the file is still fully restorable from the per-volume WAL pair, so
/// requiring the rec would false-positive on a correct implementation). The
/// second is reported as a per-iteration diagnostic instead.
#[test]
fn crash_sweep_unnamed_moment() {
    if crash_dispatch("Sweep") {
        return;
    }
    const N: usize = 40;
    let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let vol = TempDir::new().unwrap();
    let root = vol.path().join(".Trash-janitor");

    let mut failures: Vec<String> = Vec::new();
    let mut swept = 0usize;
    let mut completed = 0usize;
    let mut durable_swept = 0usize;
    let mut sentinel_timeouts = 0usize;
    // Stage histogram: which durable boundary the kill landed past.
    let mut pre_durable = 0usize; // nothing synced yet
    let mut mid_pair = 0usize; // intent without result
    let mut result_no_rec = 0usize; // WAL pair complete, home-ledger Apply rec still missing
    let mut recs_complete = 0usize; // every synced result has its home-ledger Apply rec

    for i in 0..N {
        // A FRESH home every iteration: the child appends its home-ledger
        // `Apply` recs there and `recover()` replays that same file. Reusing one
        // home across iterations would let iteration 0's completed apply satisfy
        // every later iteration's "was a rec written?" probe — the diagnostic
        // would read stale state and the Result->append gap would go unobserved.
        let home = TempDir::new().unwrap();
        // The parent must set this on ITSELF so `recover()`'s `home_ledger_dir()`
        // reads the same ledger the child wrote; `spawn_sweep` passes it to the
        // child via `Command::env`.
        unsafe { std::env::set_var("DISK_JANITOR_HOME", home.path()) };

        // Fresh iteration: clear the sentinel and any stale quarantine, then lay
        // the fixtures down at their source paths.
        fs::write(home.path().join(SWEEP_READY), b"").ok();
        fs::remove_dir_all(&root).ok();
        create_files(vol.path());

        let mut child = spawn_sweep(home.path(), vol.path(), &format!("crash-sweep-{i}"));

        // Wait until the child is inside `apply`'s durable window.
        let sentinel = home.path().join(SWEEP_READY);
        let anchor = Instant::now();
        loop {
            if fs::read(&sentinel).is_ok_and(|b| b == b"1") {
                break;
            }
            if anchor.elapsed() > Duration::from_secs(5) {
                sentinel_timeouts += 1;
                break;
            }
            std::thread::sleep(Duration::from_micros(250));
        }

        // Random delay, then SIGKILL (Unix). The `apply` phase measures ~20 ms
        // from the sentinel for this 2-entry plan, so the window is 0-32 ms: a
        // 0-61 ms draw wastes roughly half its samples past the child's exit,
        // which is why the first calibration run killed only 13/40 children.
        // The window is re-measured, not assumed — an all-completed run trips
        // the `durable_swept > 0` assertion below rather than passing vacuously.
        let nanos = u64::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .subsec_nanos(),
        );
        let mut seed = nanos ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        seed ^= seed >> 30;
        seed = seed.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        seed ^= seed >> 27;
        std::thread::sleep(Duration::from_micros(seed % 32_000));
        let _ = child.kill();
        let status = child.wait().expect("reap sweep child");
        // A SIGKILL reports no exit code at all, so never assert a specific one.
        let was_swept = !status.success();
        if was_swept {
            swept += 1;
        } else {
            completed += 1;
        }

        // Crash-time state, captured BEFORE recovery mutates anything.
        // Classifying after `recover()` would mislabel every mid-pair crash,
        // because roll-forward writes the missing Result line.
        let wal_path = root.join("wal.jsonl");
        let wal = wal_entries(&wal_path);

        // Conservation holds at every instant, not merely after recovery: the
        // rename is atomic, so a fixture is in exactly one place even mid-crash.
        for (pattern, len, name) in [
            (0xABu8, A_BYTES, "data-a.bin"),
            (0xCDu8, B_BYTES, "data-b.bin"),
        ] {
            let found = locations_with_content(vol.path(), pattern, len);
            if found.len() != 1 {
                failures.push(format!(
                    "iter {i}: at crash time {name} existed in {} places ({found:?}) — an atomic rename leaves it in exactly 1",
                    found.len()
                ));
            }
        }

        // WAL well-formedness of what the dead child left.
        let mut seen: HashSet<(String, PathHash, u8)> = HashSet::new();
        for w in &wal {
            let tag = u8::from(matches!(w.kind, WalKind::Result));
            if !seen.insert((w.op_id.clone(), w.entry, tag)) {
                failures.push(format!(
                    "iter {i}: duplicate WAL {:?} line for op_id {}",
                    w.kind, w.op_id
                ));
            }
        }
        for r in wal.iter().filter(|w| matches!(w.kind, WalKind::Result)) {
            let paired = wal.iter().any(|w| {
                matches!(w.kind, WalKind::Intent) && w.op_id == r.op_id && w.entry == r.entry
            });
            if !paired {
                failures.push(format!(
                    "iter {i}: WAL Result for op_id {} has no matching Intent (same op_id AND entry)",
                    r.op_id
                ));
            }
        }

        // Recovery: this is the operation under test.
        let rr = execute::recover(&[vol_info(vol.path())]);

        // Oracle — after recovery, each fixture must still be in exactly one
        // place, matched by content (quarantine names may carry a `-<n>` suffix).
        for (pattern, len, name) in [
            (0xABu8, A_BYTES, "data-a.bin"),
            (0xCDu8, B_BYTES, "data-b.bin"),
        ] {
            let found = locations_with_content(vol.path(), pattern, len);
            if found.len() != 1 {
                failures.push(format!(
                    "iter {i}: after recover() {name} exists in {} places ({found:?}) — expected exactly 1",
                    found.len()
                ));
            }
        }

        // Recovery sanitizes the torn tail before parsing; nothing torn may
        // survive it. (`wal_has_torn_tail` is already false on a missing file.)
        if execute::wal_has_torn_tail(&wal_path) {
            failures.push(format!(
                "iter {i}: recover() left a torn WAL tail at {}",
                wal_path.display()
            ));
        }

        // Idempotency = view stability, not emptiness: `recover()` never removes
        // intent lines, so an intent-with-no-result state is reported identically
        // on every call. Asserting an empty `dangling_intents` would be wrong.
        let rr2 = execute::recover(&[vol_info(vol.path())]);
        if rr2.rolled_forward != 0 {
            failures.push(format!(
                "iter {i}: a second recover() rolled forward {} more intents",
                rr2.rolled_forward
            ));
        }
        if rr2.dangling_intents.len() != rr.dangling_intents.len() {
            failures.push(format!(
                "iter {i}: dangling intents unstable across recover() — {} then {}",
                rr.dangling_intents.len(),
                rr2.dangling_intents.len()
            ));
        }
        if rr.dangling_intents.len() > 2 {
            failures.push(format!(
                "iter {i}: {} dangling intents for a 2-entry plan",
                rr.dangling_intents.len()
            ));
        }

        // Diagnostic only: which durable stage the kill landed in. This reads
        // THIS iteration's home ledger (`home` is fresh per iteration), so a
        // missing `Apply` rec is a real observation, not another iteration's
        // leftover. The oracle above never depends on it — a rec-less but
        // WAL-paired entry is fully restorable, so it is not a failure.
        let rec_paths: HashSet<PathBuf> = ledger::replay(&home.path().join("ledger.jsonl"))
            .map(|(recs, _)| {
                recs.iter()
                    .filter_map(|r| match &r.rec {
                        ledger::LedgerRecord::Apply { original_path, .. } => {
                            Some(original_path.clone())
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let intents = wal
            .iter()
            .filter(|w| matches!(w.kind, WalKind::Intent))
            .count();
        let results = wal.len() - intents;
        let recs_present = [vol.path().join("data-a.bin"), vol.path().join("data-b.bin")]
            .iter()
            .filter(|p| rec_paths.contains(*p))
            .count();
        let stage = if intents == 0 {
            SweepStage::PreDurable
        } else if results < intents {
            SweepStage::MidPair
        } else if recs_present < results {
            SweepStage::ResultNoRec
        } else {
            SweepStage::RecsComplete
        };
        match stage {
            SweepStage::PreDurable => pre_durable += 1,
            SweepStage::MidPair => mid_pair += 1,
            SweepStage::ResultNoRec => result_no_rec += 1,
            SweepStage::RecsComplete => recs_complete += 1,
        }
        // "Reached durable work" excludes PreDurable: that stage wrote no WAL at
        // all, so counting it would inflate the very thing the assertion below
        // exists to prove.
        if was_swept && !matches!(stage, SweepStage::PreDurable) {
            durable_swept += 1;
        }
        println!(
            "sweep {i:>2}: {:<9} stage={stage:?} wal={} (intent {intents} / result {results}) home-apply-recs={recs_present}/2 dangling={} rolled_fwd={}",
            if was_swept { "SIGKILLed" } else { "completed" },
            wal.len(),
            rr.dangling_intents.len(),
            rr.rolled_forward,
        );
    }

    // Collected, so one iteration cannot mask the others.
    assert_eq!(
        sentinel_timeouts, 0,
        "{sentinel_timeouts} iterations never saw the ready sentinel — the child died before apply, so the kill window is meaningless"
    );
    assert!(
        swept > 0,
        "the sweep killed no child mid-flight in {N} iterations — the kill window is miscalibrated"
    );
    assert!(
        durable_swept > 0,
        "in {N} iterations the kill never landed past the first WAL sync (all {completed} children ran to completion) — the window is too wide and the sweep would pass vacuously"
    );
    assert!(
        failures.is_empty(),
        "{} sweep failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!(
        "sweep summary: {swept} SIGKILLed / {completed} completed; stages: {pre_durable} pre-durable, {mid_pair} mid-pair, {result_no_rec} result-without-apply-rec, {recs_complete} recs-complete"
    );
}
