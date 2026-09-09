//! `ledger/` — JSONL 台账 + WAL journal (SPEC Part B §3.5, §5.2).
//! OWNER: ledger agent. crc32-chained envelope, flock single-writer, skip-and-continue replay.

use serde::{Deserialize, Serialize};

use crate::plan::PolicyConstraintsShape;
use crate::{PathHash, PlanEntry, WalEntry};

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

pub struct Store {
    _priv: (),
}

/// Open the store at `home` (holds `<home>/ledger.jsonl` + `<home>/wal.jsonl` + flock).
pub fn open(_home: &std::path::Path) -> anyhow::Result<Store> {
    unimplemented!("M2: ledger store")
}

impl Store {
    pub fn append(&mut self, _kind: Kind, _rec: &LedgerRecord) -> anyhow::Result<u64> {
        unimplemented!("M2: append")
    }
    pub fn wal_intent(&mut self, _w: &WalEntry) -> anyhow::Result<()> {
        unimplemented!("M2: wal intent + sync_data")
    }
    pub fn sync_boundary(&mut self) -> anyhow::Result<()> {
        unimplemented!("M2: parent dir fd sync_all")
    }
    pub fn checkpoint(&mut self) -> anyhow::Result<()> {
        unimplemented!("M2: replay->temp->sync->rename")
    }
}

/// Replay a JSONL file with skip-and-continue; torn tail truncated + parked `*.torn`.
pub fn replay(_path: &std::path::Path) -> anyhow::Result<(Vec<Record>, RecoveryReport)> {
    unimplemented!("M2: replay")
}
