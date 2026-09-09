//! M2 ledger integration tests — SPEC Part B §3.5/§5.2.
//! All tests run on tempdirs under `std::env::temp_dir()`; never touch real user data.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use disk_janitor::ledger::{self, Kind, LedgerRecord};
use disk_janitor::{Grade, PlanAction, PlanEntry, WalEntry, WalKind};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Fresh, unique temp home per test (course rule 4: tempdirs only).
fn temp_home(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "dj-ledger-{}-{}-{}",
        tag,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&p);
    p
}

fn sample_scan(i: u64) -> LedgerRecord {
    LedgerRecord::Scan {
        root: [i as u8; 8],
        files_scanned: 1000 + i,
        total_bytes: i * 1_000_000,
        duration_ms: 42 + i,
        session_id: format!("scan-{i}"),
    }
}

fn sample_verdict(i: u64) -> LedgerRecord {
    LedgerRecord::Verdict {
        verdict: format!("proceed-{i}"),
        reason: Some(format!("reason-{i}")),
        model: "deepseek".into(),
    }
}

fn sample_session(i: u64) -> LedgerRecord {
    LedgerRecord::Session {
        run_id: format!("run-{i}"),
        byte_offset: i * 4096,
    }
}

fn sample_budget(i: u64) -> LedgerRecord {
    LedgerRecord::Budget {
        model: "deepseek".into(),
        calls: (i % 100) as u32,
        prompt_tokens: i * 10,
        completion_tokens: i * 4,
        cost_usd: (i as f64) / 1000.0,
    }
}

fn sample_entry(i: u64) -> PlanEntry {
    PlanEntry {
        cid: format!("C-{i:03}"),
        path: PathBuf::from(format!("/tmp/fake-{i}.bin")),
        path_hash: [i as u8; 8],
        alloc_bytes: 1024 * i,
        grade: Grade::PureCache,
        action: PlanAction::Quarantine,
        evidence: vec![],
        inode: None,
    }
}

fn kind_of(rec: &LedgerRecord) -> Kind {
    match rec {
        LedgerRecord::Scan { .. } => Kind::Scan,
        LedgerRecord::Verdict { .. } => Kind::Verdict,
        LedgerRecord::Plan { .. } => Kind::Plan,
        LedgerRecord::Apply { .. } => Kind::Apply,
        LedgerRecord::Purge { .. } => Kind::Purge,
        LedgerRecord::Restore { .. } => Kind::Restore,
        LedgerRecord::Budget { .. } => Kind::Budget,
        LedgerRecord::Session { .. } => Kind::Session,
    }
}

/// Independent CRC recomputation over `rec` + `seq` (little-endian), mirroring the writer.
fn rec_crc(rec: &LedgerRecord, seq: u64) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&serde_json::to_vec(rec).unwrap());
    h.update(&seq.to_le_bytes());
    h.finalize()
}

// --- (a) append 100 → replay 100 with valid CRC chain + prev_crc linkage ---

#[test]
fn append_replay_roundtrip_100() {
    let home = temp_home("rt");
    {
        let mut s = ledger::open(&home).unwrap();
        for i in 0..100u64 {
            let rec = match i % 4 {
                0 => sample_scan(i),
                1 => sample_verdict(i),
                2 => sample_budget(i),
                _ => sample_session(i),
            };
            let seq = s.append(kind_of(&rec), &rec).unwrap();
            assert_eq!(seq, i + 1, "seqs assigned monotonically from 1");
        }
    }
    let lp = home.join("ledger.jsonl");

    let (recs, rep) = ledger::replay(&lp).unwrap();
    assert_eq!(recs.len(), 100);
    assert_eq!(rep.records, 100);
    assert_eq!(rep.skipped_corrupt, 0);
    assert_eq!(rep.skipped_unknown, 0);
    assert!(rep.seq_gaps.is_empty(), "no seq gaps in a healthy ledger");
    assert!(rep.torn_tail.is_none());
    for (i, r) in recs.iter().enumerate() {
        assert_eq!(r.v, 1);
        assert_eq!(r.seq, i as u64 + 1);
        assert_eq!(
            r.crc32,
            rec_crc(&r.rec, r.seq),
            "crc mismatch at seq {}",
            r.seq
        );
        match i {
            0 => assert_eq!(r.prev_crc, None, "first record chains from None"),
            _ => assert_eq!(
                r.prev_crc,
                Some(recs[i - 1].crc32),
                "prev_crc linkage broken at seq {}",
                r.seq
            ),
        }
    }

    // Reopening resumes the ledger seq where the replay left off (max seq + 1).
    let mut s2 = ledger::open(&home).unwrap();
    let seq = s2.append(Kind::Session, &sample_session(999)).unwrap();
    assert_eq!(seq, 101);
    let (recs2, _) = ledger::replay(&lp).unwrap();
    assert_eq!(recs2.len(), 101);
}

// --- (b) torn tail: truncate → replay truncates, parks .torn, returns 99 ---

#[test]
fn torn_tail_truncated_and_parked() {
    let home = temp_home("torn");
    {
        let mut s = ledger::open(&home).unwrap();
        for i in 0..100u64 {
            s.append(Kind::Session, &sample_session(i)).unwrap();
        }
    }
    let lp = home.join("ledger.jsonl");
    let data = fs::read(&lp).unwrap();
    let len = data.len() as u64;
    // End of the last COMPLETE line (record 99's newline) — the true last-good offset.
    let last_nl = data.iter().rposition(|&b| b == b'\n').unwrap() as u64; // ends line 100
    let prev_nl = data[..last_nl as usize]
        .iter()
        .rposition(|&b| b == b'\n')
        .unwrap() as u64;
    let last_good = prev_nl + 1;
    // Tear the final line: cut 10 bytes into it (no trailing newline anymore).
    fs::OpenOptions::new()
        .write(true)
        .open(&lp)
        .unwrap()
        .set_len(len - 10)
        .unwrap();

    let (recs, rep) = ledger::replay(&lp).unwrap();
    assert_eq!(recs.len(), 99);
    assert_eq!(rep.records, 99);
    assert_eq!(rep.skipped_corrupt, 0);
    assert_eq!(rep.skipped_unknown, 0);
    assert!(rep.seq_gaps.is_empty());
    let torn = rep.torn_tail.as_ref().expect("torn tail is parked");
    assert!(
        torn.ends_with("ledger.jsonl.torn"),
        "parked at {}",
        torn.display()
    );
    // File truncated to the end of the last complete line; the partial line parked whole.
    assert_eq!(
        fs::metadata(&lp).unwrap().len(),
        last_good,
        "file truncated to last good offset"
    );
    let parked = fs::read(torn).unwrap();
    assert_eq!(
        parked.len(),
        (len - 10) as usize - last_good as usize,
        "parked == discarded partial line"
    );
    assert_eq!(
        fs::read(&lp).unwrap().len() + parked.len(),
        (len - 10) as usize
    );

    // Reopen repairs + continues: next seq is 100, not 101.
    let mut s = ledger::open(&home).unwrap();
    let seq = s.append(Kind::Session, &sample_session(500)).unwrap();
    assert_eq!(seq, 100);
}

// --- (c) corrupt middle line → skipped_corrupt=1, later lines still returned, gap ---

#[test]
fn corrupt_middle_line_skipped_and_gap_reported() {
    let home = temp_home("mid");
    {
        let mut s = ledger::open(&home).unwrap();
        for i in 0..100u64 {
            s.append(Kind::Session, &sample_session(i)).unwrap();
        }
    }
    let lp = home.join("ledger.jsonl");
    let data = fs::read(&lp).unwrap();
    let mut parts: Vec<Vec<u8>> = data.split(|&b| b == b'\n').map(|s| s.to_vec()).collect();
    assert!(
        parts.last().is_some_and(|l| l.is_empty()),
        "file ends with newline"
    );
    // Corrupt record 50 (0-based index 49) by rewriting its crc32.
    let mut v: serde_json::Value = serde_json::from_slice(&parts[49]).unwrap();
    assert_eq!(v["seq"], 50);
    v["crc32"] = serde_json::json!(0xDEADBEEFu32);
    parts[49] = serde_json::to_vec(&v).unwrap();
    fs::write(&lp, parts.join(&b'\n')).unwrap();

    let (recs, rep) = ledger::replay(&lp).unwrap();
    assert_eq!(recs.len(), 99);
    assert_eq!(rep.records, 99);
    assert_eq!(rep.skipped_corrupt, 1);
    assert_eq!(rep.skipped_unknown, 0);
    assert_eq!(rep.seq_gaps, vec![50], "the lost record's seq is reported");
    assert_eq!(recs.first().unwrap().seq, 1);
    assert_eq!(
        recs.last().unwrap().seq,
        100,
        "lines after the corrupt one still replay"
    );
    assert!(
        rep.torn_tail.is_none(),
        "mid-file corruption is not a torn tail"
    );
    // The 51st returned record is the old 51 (skipped 50).
    assert_eq!(recs[49].seq, 51);
}

// --- (d) unknown kind line → skipped_unknown=1, others intact ---

#[test]
fn unknown_kind_line_counted_and_skipped() {
    let home = temp_home("unk");
    {
        let mut s = ledger::open(&home).unwrap();
        for i in 0..100u64 {
            s.append(Kind::Session, &sample_session(i)).unwrap();
        }
    }
    let lp = home.join("ledger.jsonl");
    let data = fs::read(&lp).unwrap();
    let mut parts: Vec<Vec<u8>> = data.split(|&b| b == b'\n').map(|s| s.to_vec()).collect();
    // Inject a forward-compat (D-18) record with an unknown kind string.
    let unknown = br#"{"v":1,"seq":999,"ts":"2026-09-09T00:00:00Z","kind":"frobnicate","crc32":0,"prev_crc":null,"rec":{}}"#;
    parts.insert(50, unknown.to_vec());
    fs::write(&lp, parts.join(&b'\n')).unwrap();

    let (recs, rep) = ledger::replay(&lp).unwrap();
    assert_eq!(recs.len(), 100);
    assert_eq!(rep.records, 100);
    assert_eq!(rep.skipped_unknown, 1);
    assert_eq!(rep.skipped_corrupt, 0);
    assert!(
        rep.seq_gaps.is_empty(),
        "unknown kinds are not corrupt, so no seq gap"
    );
    assert!(rep.torn_tail.is_none());
    assert_eq!(recs[50].seq, 51, "records after the unknown line intact");
}

// --- (e) flock: two opens on the same home → second fails ---

#[test]
fn flock_blocks_second_open() {
    let home = temp_home("flock");
    let s1 = ledger::open(&home).unwrap();
    let err = match ledger::open(&home) {
        Ok(_) => panic!("second open on the same home must fail"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("another janitor instance"),
        "unexpected error: {err}"
    );
    drop(s1);
    let _s2 = ledger::open(&home).unwrap(); // lock released with the first store
}

// --- (f) checkpoint: wal folded + compacted, op_id dedupe, idempotent ---

#[test]
fn checkpoint_folds_wal_and_compacts_idempotently() {
    let home = temp_home("ckpt");
    let lp = home.join("ledger.jsonl");
    let wp = home.join("wal.jsonl");
    {
        let mut s = ledger::open(&home).unwrap();

        // Two raw mutation-journal intents, plus a purge and a restore (append mirrors
        // the latter two into the wal as derived result entries).
        s.wal_intent(&WalEntry {
            op_id: "op-intent-1".into(),
            kind: WalKind::Intent,
            plan_id: "p1".into(),
            entry: [1; 8],
            from: None,
            to: None,
            bytes: 0,
            undo_of: None,
        })
        .unwrap();
        s.wal_intent(&WalEntry {
            op_id: "op-intent-2".into(),
            kind: WalKind::Intent,
            plan_id: "p1".into(),
            entry: [2; 8],
            from: None,
            to: None,
            bytes: 0,
            undo_of: None,
        })
        .unwrap();
        let purge = LedgerRecord::Purge {
            plan_id: "p1".into(),
            entry: sample_entry(1),
            bytes_freed: 4096,
            quarantine_path: PathBuf::from("/tmp/.dj-quarantine/1.bin"),
        };
        let pseq = s.append(Kind::Purge, &purge).unwrap();
        let restore = LedgerRecord::Restore {
            plan_id: "p1".into(),
            entry: sample_entry(2),
            from: PathBuf::from("/tmp/.dj-quarantine/2.bin"),
            to: PathBuf::from("/tmp/restored-2.bin"),
            undoes: 7,
        };
        let rseq = s.append(Kind::Restore, &restore).unwrap();
        assert_eq!(pseq, 1);
        assert_eq!(rseq, 2);
        assert_eq!(
            ledger::record_iter(&wp).len(),
            4,
            "2 intents + 2 append mirrors"
        );
    }

    // Simulate a crash AFTER a fold but BEFORE compaction: wal lines already in the ledger.
    let wal_bytes = fs::read(&wp).unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(&lp)
        .unwrap()
        .write_all(&wal_bytes)
        .unwrap();

    let mut s = ledger::open(&home).unwrap();
    s.checkpoint().unwrap();

    // Wal compacted to empty.
    assert_eq!(
        fs::metadata(&wp).unwrap().len(),
        0,
        "wal emptied by checkpoint"
    );
    // Ledger holds the folded wal records exactly once (op_id dedupe from ledger scan).
    assert_eq!(
        ledger::record_iter(&lp).len(),
        4,
        "folded wal lines present exactly once"
    );
    // The ledger-shaped purge/restore records replay; folded wal lines are known, not unknown.
    let (recs, rep) = ledger::replay(&lp).unwrap();
    assert_eq!(recs.len(), 2);
    assert_eq!(rep.records, 2);
    assert_eq!(rep.skipped_unknown, 0, "folded wal lines are known kinds");
    assert_eq!(recs.iter().filter(|r| r.kind == Kind::Purge).count(), 1);
    assert_eq!(recs.iter().filter(|r| r.kind == Kind::Restore).count(), 1);

    // Second checkpoint on an empty wal: idempotent, nothing duplicated.
    s.checkpoint().unwrap();
    assert_eq!(ledger::record_iter(&lp).len(), 4);
    assert_eq!(ledger::replay(&lp).unwrap().0.len(), 2);
    assert_eq!(fs::metadata(&wp).unwrap().len(), 0);

    // wal handle was reopened after compaction: new intents land in the new file with a
    // fresh wal seq, and the ledger seq continues past the fold.
    s.wal_intent(&WalEntry {
        op_id: "op-intent-3".into(),
        kind: WalKind::Intent,
        plan_id: "p2".into(),
        entry: [3; 8],
        from: None,
        to: None,
        bytes: 0,
        undo_of: None,
    })
    .unwrap();
    let wal = ledger::record_iter(&wp);
    assert_eq!(wal.len(), 1);
    assert_eq!(wal[0].0, 1, "wal seq restarts after compaction");
    assert_eq!(wal[0].1.op_id, "op-intent-3");
    let seq = s.append(Kind::Session, &sample_session(1)).unwrap();
    assert_eq!(seq, 3, "folded wal lines do not consume ledger seqs");
}

// --- (g) concurrent appends through one Store (Mutex) → seq strictly monotonic ---

#[test]
fn concurrent_appends_are_seq_monotonic() {
    let home = temp_home("conc");
    let store = Arc::new(Mutex::new(ledger::open(&home).unwrap()));
    let mut handles = Vec::new();
    for t in 0..2u64 {
        let store = Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            let mut seqs = Vec::with_capacity(50);
            for i in 0..50u64 {
                let rec = sample_session(t * 1000 + i);
                seqs.push(store.lock().unwrap().append(Kind::Session, &rec).unwrap());
            }
            seqs
        }));
    }
    let mut all: Vec<u64> = Vec::new();
    for h in handles {
        all.extend(h.join().unwrap());
    }
    all.sort_unstable();
    assert_eq!(
        all,
        (1..=100u64).collect::<Vec<_>>(),
        "seqs strictly monotonic and gap-free"
    );

    drop(store);
    let (recs, rep) = ledger::replay(&home.join("ledger.jsonl")).unwrap();
    assert_eq!(recs.len(), 100);
    assert_eq!(rep.records, 100);
    assert_eq!(rep.skipped_corrupt, 0);
    assert_eq!(rep.skipped_unknown, 0);
}
