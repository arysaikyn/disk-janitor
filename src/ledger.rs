//! `ledger/` — JSONL 台账 + WAL journal (SPEC Part B §3.5, §5.2).
//! OWNER: ledger agent. crc32-chained envelope, flock single-writer, skip-and-continue replay.
//!
//! Two files under `<home>/`: `ledger.jsonl` (permanent R5 history) and `wal.jsonl`
//! (crash journal, emptied at each checkpoint). A third sidecar, `<home>/janitor.lock`,
//! carries the advisory flock that guarantees a single writer process.
//!
//! Record envelope (format v1 frozen — ADR-17/D-18; unknown kinds/fields are skipped AND
//! counted, `deny_unknown_fields` is OFF everywhere):
//!
//! ```json
//! {"v":1,"seq":42,"ts":"2026-08-30T12:00:00Z","kind":"apply","crc32":123,"prev_crc":122,"rec":{...}}
//! ```
//!
//! `crc32` = crc32fast over the canonical `serde_json::to_vec(rec)` bytes followed by the
//! `seq` as **little-endian** bytes — the exact same bytes are fed on write and on replay
//! validation (research note 08 §4). `prev_crc` chains each record to the previous
//! **ledger-shaped** record's `crc32`, so a lost/reordered ledger record breaks the chain
//! (the chain is consumer-verifiable; replay itself only validates per-line CRCs so that a
//! corrupt middle line does not poison the lines after it — SPEC §5.2 skip-and-continue).
//!
//! `wal.jsonl` holds the same envelope shape with `kind` = `"intent"|"result"` (WalKind)
//! and `rec` = a full `WalEntry`, plus its own independent `seq` counter. Two writers put
//! lines there: `Store::wal_intent` (the mutation journal, always `sync_data()`-durable —
//! std maps to `F_FULLFSYNC` on apple) and `Store::append` with `Kind::Purge|Restore`,
//! which mirrors the committed ledger record as a derived `WalEntry` (fresh uuid op_id,
//! kind=result) through the same flow.
//!
//! `Store::checkpoint` folds the WAL into the ledger: each valid line is appended verbatim
//! to `ledger.jsonl` unless its `op_id` is already present there (the dedupe set is rebuilt
//! from a ledger scan at `open`), the ledger is `sync_data()`-durable *before* the journal
//! is dropped (SQLite lesson, 08 §1), then the WAL is atomically replaced with an empty
//! file via the temp → sync → rename → sync-parent dance (08 §1/§5).
//!
//! `replay` on a ledger treats folded WAL lines as known-but-not-ledger records: they are
//! neither returned nor counted as skipped (they are readable via `record_iter`). Unknown
//! kind strings and future format versions count as `skipped_unknown` (D-18).

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::plan::PolicyConstraintsShape;
use crate::{PathHash, PlanEntry, WalEntry, WalKind};

/// Record kinds — SPEC §3.5 table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Scan,
    Verdict,
    Plan,
    Apply,
    Purge,
    Restore,
    Budget,
    Session,
}

/// Envelope — SPEC §3.5: `{"v":1,"seq":N,"ts":"…","kind":"apply","crc32":…,"prev_crc":…,"rec":{…}}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub v: u8,
    pub seq: u64,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub kind: Kind,
    pub crc32: u32,
    pub prev_crc: Option<u32>,
    pub rec: LedgerRecord,
}

/// rec payloads — SPEC §3.5 + §6 `LedgerRecord`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum LedgerRecord {
    Scan {
        root: PathHash,
        files_scanned: u64,
        total_bytes: u64,
        duration_ms: u64,
        session_id: String,
    },
    Verdict {
        path_hash: PathHash,
        size: u64,
        grade: crate::Grade,
        evidence: Vec<crate::Evidence>,
    },
    Plan {
        plan_id: String,
        entries: Vec<PlanEntry>,
        policy_snapshot: PolicyConstraintsShape,
    },
    Apply {
        plan_id: String,
        entry: PlanEntry,
        original_path: std::path::PathBuf,
        quarantine_path: std::path::PathBuf,
        renamex_np: bool,
        undo: Undo,
    },
    Purge {
        plan_id: String,
        entry: PlanEntry,
        bytes_freed: u64,
        quarantine_path: std::path::PathBuf,
    },
    Restore {
        plan_id: String,
        entry: PlanEntry,
        from: std::path::PathBuf,
        to: std::path::PathBuf,
        undoes: u64,
    },
    Budget {
        model: String,
        calls: u32,
        prompt_tokens: u64,
        completion_tokens: u64,
        cost_usd: f64,
    },
    Session {
        run_id: String,
        byte_offset: u64,
    },
}

/// `undo{original_path, byte_count, grace_expires_at}` — SPEC §3.5 apply payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Undo {
    pub original_path: std::path::PathBuf,
    pub byte_count: u64,
    pub grace_expires_at: chrono::DateTime<chrono::Utc>,
}

/// Replay report — skip-and-continue accounting (ADR-4, D-18).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub records: u64,
    pub skipped_corrupt: u64,
    pub skipped_unknown: u64,
    pub seq_gaps: Vec<u64>,
    pub torn_tail: Option<std::path::PathBuf>,
}

/// Format version of every envelope we write (frozen — ADR-17/D-18).
const FORMAT_V: u8 = 1;
const WAL_KIND_INTENT: &str = "intent";
const WAL_KIND_RESULT: &str = "result";

/// WAL envelope — same shape as `Record` but `kind` is the WalKind and `rec` is a
/// `WalEntry`. Written by `Store::wal_intent` and by `Store::append` mirrors.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalEnvelope {
    v: u8,
    seq: u64,
    ts: chrono::DateTime<chrono::Utc>,
    kind: WalKind,
    crc32: u32,
    prev_crc: Option<u32>,
    rec: WalEntry,
}

/// Typed envelope used for replay validation. The CRC is recomputed over
/// `serde_json::to_vec(&rec)` — byte-identical to the writer's bytes because serde
/// serialization of a given struct is deterministic (same field order, same escaping,
/// ryu shortest-float). This avoids serde_json's `raw_value` feature entirely.
#[derive(Debug, Deserialize)]
struct RawLedgerEnvelope {
    v: u8,
    seq: u64,
    ts: chrono::DateTime<chrono::Utc>,
    kind: Kind,
    crc32: u32,
    prev_crc: Option<u32>,
    rec: LedgerRecord,
}

#[derive(Debug, Deserialize)]
struct RawWalEnvelope {
    v: u8,
    seq: u64,
    ts: chrono::DateTime<chrono::Utc>,
    kind: WalKind,
    crc32: u32,
    prev_crc: Option<u32>,
    rec: WalEntry,
}

/// CRC over the canonical `rec` serialization followed by `seq` as little-endian bytes.
fn crc_of(rec_json: &[u8], seq: u64) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(rec_json);
    h.update(&seq.to_le_bytes());
    h.finalize()
}

/// A valid line, classified by shape.
enum LineKind {
    Ledger(Record),
    Wal(WalEnvelope),
}

/// A line that failed validation.
enum BadLine {
    /// Malformed JSON, unknown shape for a known kind, or CRC mismatch.
    Corrupt { seq: Option<u64> },
    /// Unknown kind string or future format version (D-18: skip AND count).
    Unknown,
}

#[allow(clippy::large_enum_variant)] // Valid carries the parsed envelope; boxing would touch every call site
enum Class {
    Valid(LineKind),
    Bad(BadLine),
}

/// Classify one newline-terminated line.
///
/// A `serde_json::Value` probe first discriminates the kind string (so an unknown kind
/// counts as `Unknown` per D-18 instead of failing as corrupt), then the line is
/// deserialized into the typed envelope and the CRC is recomputed over the deterministic
/// re-serialization of `rec` + `seq`.
fn classify(line: &[u8]) -> Class {
    let val: serde_json::Value = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(_) => return Class::Bad(BadLine::Corrupt { seq: None }),
    };
    let v: u8 = match val.get("v").and_then(serde_json::Value::as_u64) {
        Some(n) if n <= u64::from(u8::MAX) => n as u8,
        _ => return Class::Bad(BadLine::Corrupt { seq: None }),
    };
    if v != FORMAT_V {
        // Future format version: forward-compat, skip and count as unknown (D-18).
        return Class::Bad(BadLine::Unknown);
    }
    let seq: u64 = match val.get("seq").and_then(serde_json::Value::as_u64) {
        Some(s) => s,
        None => return Class::Bad(BadLine::Corrupt { seq: None }),
    };
    let kind_str = match val.get("kind").and_then(serde_json::Value::as_str) {
        Some(k) => k,
        None => return Class::Bad(BadLine::Corrupt { seq: Some(seq) }),
    };
    match kind_str {
        WAL_KIND_INTENT | WAL_KIND_RESULT => {
            let env: RawWalEnvelope = match serde_json::from_slice(line) {
                Ok(e) => e,
                Err(_) => return Class::Bad(BadLine::Corrupt { seq: Some(seq) }),
            };
            let rec_bytes = match serde_json::to_vec(&env.rec) {
                Ok(b) => b,
                Err(_) => return Class::Bad(BadLine::Corrupt { seq: Some(seq) }),
            };
            if crc_of(&rec_bytes, env.seq) != env.crc32 {
                return Class::Bad(BadLine::Corrupt { seq: Some(seq) });
            }
            Class::Valid(LineKind::Wal(WalEnvelope {
                v: env.v,
                seq: env.seq,
                ts: env.ts,
                kind: env.kind,
                crc32: env.crc32,
                prev_crc: env.prev_crc,
                rec: env.rec,
            }))
        }
        _ => {
            // `kind_str` is a bare string; discriminate via the JSON node (the Value holds
            // the quoted string, which from_str on the bare text would miss). A parseable
            // Kind here means a known ledger record; anything else is Unknown (D-18).
            if !val
                .get("kind")
                .is_some_and(|k| serde_json::from_value::<Kind>(k.clone()).is_ok())
            {
                return Class::Bad(BadLine::Unknown);
            }
            let env: RawLedgerEnvelope = match serde_json::from_slice(line) {
                Ok(e) => e,
                Err(_) => return Class::Bad(BadLine::Corrupt { seq: Some(seq) }),
            };
            let rec_bytes = match serde_json::to_vec(&env.rec) {
                Ok(b) => b,
                Err(_) => return Class::Bad(BadLine::Corrupt { seq: Some(seq) }),
            };
            if crc_of(&rec_bytes, env.seq) != env.crc32 {
                return Class::Bad(BadLine::Corrupt { seq: Some(seq) });
            }
            Class::Valid(LineKind::Ledger(Record {
                v: env.v,
                seq: env.seq,
                ts: env.ts,
                kind: env.kind,
                crc32: env.crc32,
                prev_crc: env.prev_crc,
                rec: env.rec,
            }))
        }
    }
}

fn settle_bad(bad: &BadLine, report: &mut RecoveryReport) {
    match bad {
        BadLine::Corrupt { seq } => {
            report.skipped_corrupt += 1;
            if let Some(s) = seq {
                report.seq_gaps.push(*s);
            }
        }
        BadLine::Unknown => report.skipped_unknown += 1,
    }
}

/// Read `path[from..]` (fresh handle, so it is unaffected by any BufReader read-ahead).
fn read_tail(path: &Path, from: u64) -> anyhow::Result<Vec<u8>> {
    let mut f = fs_err::File::open(path).with_context(|| format!("open {}", path.display()))?;
    f.seek(SeekFrom::Start(from))?;
    let mut v = Vec::new();
    f.read_to_end(&mut v)?;
    Ok(v)
}

/// Park the discarded tail bytes in `<path>.torn` (overwrite), then truncate the file to
/// the last good offset. Torn bytes are parked *before* truncation so a crash mid-repair
/// loses nothing.
fn park_tail(path: &Path, from: u64, tail: &[u8]) -> anyhow::Result<PathBuf> {
    let torn_path = path.with_extension("jsonl.torn");
    fs_err::write(&torn_path, tail)
        .with_context(|| format!("park torn tail at {}", torn_path.display()))?;
    let f = fs_err::OpenOptions::new()
        .write(true)
        .open(path)
        .with_context(|| format!("open {} for tail truncation", path.display()))?;
    f.set_len(from)
        .with_context(|| format!("truncate {} to byte {}", path.display(), from))?;
    Ok(torn_path)
}

/// Scan a JSONL file with byte-exact `BufRead::read_until` framing (research note 08 §5).
///
/// * Torn tail (final line without `\n`, or final line failing CRC/JSON) — `repair=true`:
///   truncated to the last good offset, discarded bytes parked in `<path>.torn`.
/// * Mid-file corrupt line — skip-and-continue, counted in `skipped_corrupt` (+ seq gap).
/// * Unknown kind string / future version — counted in `skipped_unknown` (D-18).
///
/// `repair=false` is read-only and never touches the file (the `record_iter` path).
/// Returns ordered valid lines (raw bytes incl. trailing `\n` + parsed shape) and the
/// report. Wal-shaped lines are valid lines here; the *ledger* accounting in `report`
/// counts only ledger-shaped lines.
/// Raw scanned lines: `(line bytes incl. trailing \n, parsed shape)` pairs.
type ScanLines = Vec<(Vec<u8>, LineKind)>;

fn scan_file(path: &Path, repair: bool) -> anyhow::Result<(ScanLines, RecoveryReport)> {
    let mut opts = fs_err::OpenOptions::new();
    opts.read(true);
    if repair {
        opts.write(true);
    }
    let file = opts
        .open(path)
        .with_context(|| format!("open ledger file {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut buf: Vec<u8> = Vec::new();
    let mut consumed: u64 = 0;
    let mut last_good_end: u64 = 0;
    let mut lines: Vec<(Vec<u8>, LineKind)> = Vec::new();
    let mut report = RecoveryReport::default();
    // A bad line is only known to be a torn tail once the next read proves EOF; defer.
    let mut pending_bad: Option<BadLine> = None;
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        let has_nl = buf.last() == Some(&b'\n');
        consumed += n as u64;
        if !has_nl {
            // Unterminated final line: a torn write by construction (we always append \n).
            if let Some(bad) = pending_bad.take() {
                settle_bad(&bad, &mut report);
            }
            if repair {
                report.torn_tail = Some(park_tail(
                    path,
                    last_good_end,
                    &read_tail(path, last_good_end)?,
                )?);
            }
            break;
        }
        let line_bytes = buf[..n].to_vec();
        match classify(&line_bytes) {
            Class::Valid(kind) => {
                if let Some(bad) = pending_bad.take() {
                    settle_bad(&bad, &mut report);
                }
                if let LineKind::Ledger(_) = &kind {
                    report.records += 1;
                }
                lines.push((line_bytes, kind));
                last_good_end = consumed;
            }
            Class::Bad(bad) => pending_bad = Some(bad),
        }
    }
    if let Some(bad) = pending_bad.take() {
        // Final line failed CRC/JSON → torn tail (SPEC §5.2: stop at first invalid frame).
        settle_bad(&bad, &mut report);
        if repair {
            report.torn_tail = Some(park_tail(
                path,
                last_good_end,
                &read_tail(path, last_good_end)?,
            )?);
        }
    }
    Ok((lines, report))
}

pub struct Store {
    home: PathBuf,
    /// Flock sidecar — held (and thus the lock held) for the Store lifetime.
    _lock: std::fs::File,
    ledger: fs_err::File,
    wal: fs_err::File,
    /// Next ledger seq (1-based; max replayed ledger-shaped seq + 1).
    next_seq: u64,
    /// Next wal seq (1-based, wal-local counter; max replayed wal seq + 1).
    wal_seq: u64,
    /// crc32 of the last ledger-shaped record — the `prev_crc` chain.
    last_crc: Option<u32>,
    /// crc32 of the last wal record — the wal-local chain.
    wal_last_crc: Option<u32>,
    /// op_ids of wal records already folded into the ledger (checkpoint dedupe).
    wal_applied: HashSet<String>,
}

/// Open the store at `home` (holds `<home>/ledger.jsonl` + `<home>/wal.jsonl` + flock).
///
/// Creates `<home>` (0700) if missing, takes a non-blocking exclusive flock on
/// `<home>/janitor.lock` (second open on the same home fails fast), opens both journals
/// append-only, then rebuilds the in-memory counters (`next_seq`, `prev_crc` chain, wal
/// seq, checkpoint dedupe set) purely from durable state (research note 08 §7).
pub fn open(home: &Path) -> anyhow::Result<Store> {
    // 1. Home dir, 0700 if missing.
    let mut db = std::fs::DirBuilder::new();
    db.recursive(true).mode(0o700);
    db.create(home)
        .with_context(|| format!("create ledger home {}", home.display()))?;

    // 2. Single-writer flock (advisory; every janitor process must cooperate — 08 §6).
    // std's `File::try_lock` (stabilized 1.89, flock(2) LOCK_EX|LOCK_NB on unix) fills the
    // fs4 role; the sidecar handle stays alive for the Store lifetime so the lock holds.
    let lock_path = home.join("janitor.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false) // lock sidecar must persist across opens; flock is the gate
        .open(&lock_path)
        .with_context(|| format!("open lock file {}", lock_path.display()))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => anyhow::bail!(
            "another janitor instance is running (flock held on {})",
            lock_path.display()
        ),
        Err(std::fs::TryLockError::Error(e)) => return Err(e).context("acquire janitor lock"),
    }

    let ledger_path = home.join("ledger.jsonl");
    let wal_path = home.join("wal.jsonl");
    let ledger = fs_err::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger_path)
        .with_context(|| format!("open ledger {}", ledger_path.display()))?;
    let wal = fs_err::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&wal_path)
        .with_context(|| format!("open wal {}", wal_path.display()))?;

    // 3. Rebuild counters from durable state (derived state never lives only in memory).
    let (ledger_lines, _) = scan_file(&ledger_path, true)?;
    let mut next_seq: u64 = 1;
    let mut last_crc: Option<u32> = None;
    let mut wal_applied: HashSet<String> = HashSet::new();
    for (_raw, kind) in &ledger_lines {
        match kind {
            LineKind::Ledger(r) => {
                next_seq = r.seq + 1;
                last_crc = Some(r.crc32);
            }
            LineKind::Wal(w) => {
                wal_applied.insert(w.rec.op_id.clone());
            }
        }
    }
    let (wal_lines, _) = scan_file(&wal_path, true)?;
    let mut wal_seq: u64 = 0;
    let mut wal_last_crc: Option<u32> = None;
    for (_raw, kind) in &wal_lines {
        if let LineKind::Wal(w) = kind {
            wal_seq = w.seq;
            wal_last_crc = Some(w.crc32);
        }
    }

    Ok(Store {
        home: home.to_path_buf(),
        _lock: lock,
        ledger,
        wal,
        next_seq,
        wal_seq,
        last_crc,
        wal_last_crc,
        wal_applied,
    })
}

/// Derive the wal mirror for a committed Purge/Restore ledger record. `Store::append`
/// writes this through the same wal flow as `wal_intent` (kind=result, fresh op_id), so
/// the checkpoint can fold it back into the ledger and dedupe by op_id.
fn mirror_wal_entry(kind: Kind, rec: &LedgerRecord) -> WalEntry {
    let op_id = uuid::Uuid::new_v4().to_string();
    match (kind, rec) {
        (
            Kind::Purge,
            LedgerRecord::Purge {
                plan_id,
                entry,
                bytes_freed,
                quarantine_path,
            },
        ) => WalEntry {
            op_id,
            kind: WalKind::Result,
            plan_id: plan_id.clone(),
            entry: entry.path_hash,
            from: Some(quarantine_path.clone()),
            to: None,
            bytes: *bytes_freed,
            undo_of: None,
        },
        (
            Kind::Restore,
            LedgerRecord::Restore {
                plan_id,
                entry,
                from,
                to,
                undoes,
            },
        ) => WalEntry {
            op_id,
            kind: WalKind::Result,
            plan_id: plan_id.clone(),
            entry: entry.path_hash,
            from: Some(from.clone()),
            to: Some(to.clone()),
            bytes: entry.alloc_bytes,
            undo_of: Some(*undoes),
        },
        _ => unreachable!("append mirrors wal only for Purge/Restore kinds"),
    }
}

impl Store {
    /// Append one ledger record. Returns the assigned seq.
    ///
    /// For `Kind::Purge|Restore` the same operation is ALSO written to `wal.jsonl` as a
    /// derived result `WalEntry` via the wal flow (crash journal for FS ops — SPEC §3.5),
    /// so a checkpoint can fold it into the ledger if the ledger line itself was lost.
    pub fn append(&mut self, kind: Kind, rec: &LedgerRecord) -> anyhow::Result<u64> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let rec_bytes = serde_json::to_vec(rec)?;
        let crc32 = crc_of(&rec_bytes, seq);
        let env = Record {
            v: FORMAT_V,
            seq,
            ts: Utc::now(),
            kind,
            crc32,
            prev_crc: self.last_crc,
            rec: rec.clone(),
        };
        self.last_crc = Some(crc32);
        let mut line = serde_json::to_vec(&env)?;
        line.push(b'\n');
        self.ledger
            .write_all(&line)
            .with_context(|| "append ledger record")?;
        if matches!(kind, Kind::Purge | Kind::Restore) {
            let w = mirror_wal_entry(kind, rec);
            self.wal_write(&w)?;
        }
        Ok(seq)
    }

    /// Write a WAL intent/result record and make it durable (`sync_data()` — std maps to
    /// `F_FULLFSYNC` on apple — research note 08 §2) BEFORE the caller mutates the FS.
    pub fn wal_intent(&mut self, w: &WalEntry) -> anyhow::Result<()> {
        self.wal_write(w)
    }

    /// Shared wal append path: envelope with the wal-local seq + crc chain, then sync.
    fn wal_write(&mut self, w: &WalEntry) -> anyhow::Result<()> {
        let seq = self.wal_seq + 1;
        self.wal_seq = seq;
        let rec_bytes = serde_json::to_vec(w)?;
        let crc32 = crc_of(&rec_bytes, seq);
        let env = WalEnvelope {
            v: FORMAT_V,
            seq,
            ts: Utc::now(),
            kind: w.kind,
            crc32,
            prev_crc: self.wal_last_crc,
            rec: w.clone(),
        };
        self.wal_last_crc = Some(crc32);
        let mut line = serde_json::to_vec(&env)?;
        line.push(b'\n');
        self.wal
            .write_all(&line)
            .with_context(|| "append wal record")?;
        self.wal
            .sync_data()
            .with_context(|| "sync wal (F_FULLFSYNC on apple)")?;
        Ok(())
    }

    /// fsync the parent directory of the journals. On APFS two independent fullfsyncs are
    /// not ordered; the directory sync closes that gap (SQLite 3.46.1 lesson — 08 §1).
    pub fn sync_boundary(&mut self) -> anyhow::Result<()> {
        let dir = std::fs::File::open(&self.home)
            .with_context(|| format!("open dir {} for sync", self.home.display()))?;
        dir.sync_all()
            .with_context(|| format!("sync dir {}", self.home.display()))?;
        Ok(())
    }

    /// Fold the WAL into the ledger, then atomically replace it with an empty file.
    ///
    /// 1. Replay `wal.jsonl` (torn tail repaired + parked).
    /// 2. Append each valid record verbatim to `ledger.jsonl`, skipping op_ids already
    ///    present in the ledger (dedupe set rebuilt from the ledger scan at `open` — a
    ///    crash between "fold" and "compact" therefore cannot duplicate records).
    /// 3. `sync_data()` the ledger BEFORE dropping the journal (08 §1/§5).
    /// 4. Compaction dance: empty temp in the same dir → sync temp → rename over the wal
    ///    → `sync_boundary()` the parent dir; reopen the wal handle (rename replaced the
    ///    inode) and restart the wal-local seq.
    pub fn checkpoint(&mut self) -> anyhow::Result<()> {
        let wal_path = self.home.join("wal.jsonl");
        let (lines, _rep) = scan_file(&wal_path, true)?;
        let mut folded = 0usize;
        for (raw, kind) in &lines {
            if let LineKind::Wal(w) = kind
                && self.wal_applied.insert(w.rec.op_id.clone())
            {
                self.ledger
                    .write_all(raw)
                    .with_context(|| "fold wal record into ledger")?;
                folded += 1;
            }
        }
        if folded > 0 {
            self.ledger
                .sync_data()
                .with_context(|| "sync ledger before wal compaction")?;
        }
        // Atomic compaction: temp (same dir) -> sync temp -> rename over wal -> sync parent.
        let tmp = tempfile::NamedTempFile::new_in(&self.home)
            .with_context(|| format!("create wal compaction temp in {}", self.home.display()))?;
        tmp.as_file()
            .sync_all()
            .with_context(|| "sync wal compaction temp")?;
        tmp.persist(&wal_path)
            .with_context(|| format!("rename over {}", wal_path.display()))?;
        // The rename replaced the inode; reopen so future appends land in the new file.
        self.wal = fs_err::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .with_context(|| format!("reopen wal {}", wal_path.display()))?;
        self.wal_seq = 0;
        self.wal_last_crc = None;
        self.sync_boundary()
    }
}

/// Replay a JSONL file with skip-and-continue; torn tail truncated + parked `*.torn`.
///
/// Returns the ledger-shaped records in file order plus the recovery report. Records
/// whose `prev_crc` chain is broken are still returned (per-line CRC is the gate; the
/// chain is consumer-verifiable — see the module docs), so a corrupt middle line does
/// not poison later, independent operations (SPEC §5.2).
pub fn replay(path: &Path) -> anyhow::Result<(Vec<Record>, RecoveryReport)> {
    let (lines, report) = scan_file(path, true)?;
    let mut records = Vec::with_capacity(lines.len());
    for (_raw, kind) in lines {
        if let LineKind::Ledger(r) = kind {
            records.push(r);
        }
    }
    Ok((records, report))
}

/// Read WAL records from a JSONL file: `(wal_seq, WalEntry)` for every line that parses
/// as a wal envelope AND validates its CRC, skipping everything else (bad lines, torn
/// tail, ledger-shaped lines). Read-only — never truncates, never parks. Usable on both
/// `wal.jsonl` and `ledger.jsonl` (the latter contains folded wal lines after a
/// checkpoint). ExecEngine's recovery path.
pub fn record_iter(path: &Path) -> Vec<(u64, WalEntry)> {
    match scan_file(path, false) {
        Ok((lines, _)) => lines
            .into_iter()
            .filter_map(|(_raw, kind)| match kind {
                LineKind::Wal(w) => Some((w.seq, w.rec)),
                LineKind::Ledger(_) => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}
