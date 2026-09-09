//! Shared contract types: `DirStat`, `ScanEvent`, `Candidate`, `Evidence`, `PlanEntry`,
//! `LedgerRecord`, `WalEntry` — SPEC Part B §6 (field lists are the contract).
//!
//! Every module (scan/classify/plan/execute/ledger/agent) builds on these types; they are
//! the cross-subagent interface. Field lists follow SPEC §6 verbatim; additions are
//! forbidden without an SPEC/ADR sync.

pub mod classify;
pub mod config;
pub mod execute;
pub mod ledger;
pub mod plan;
pub mod policy;
pub mod scan;

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// Rebuild-cost grade — SPEC §6. Order matters: it is the default allow-list split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    PureCache,
    RebuildableFromSource,
    Redownloadable,
    Expensive,
    HandsOff,
}

/// Opaque candidate id, "C-042" — deterministic size-desc order, SPEC §6 invariant.
pub type Cid = String;

/// 8-byte path identity for ledger + agent-facing references (never a raw path).
pub type PathHash = [u8; 8];

/// Evidence signal kinds — SPEC §3.2 table + §6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    LockfilePresent,
    SiblingManifestMissing,
    OrphanTree,
    MtimeHot,
    FileLock,
    GitRepoAncestor,
    CasStore,
    Hardlinked,
    ApfsClone,
    Sparse,
    Dataless,
    FlagBlock(String), // UfSf flag name, e.g. "UF_IMMUTABLE"
    QuarantineXattr,
    BuildArtifactShape,
    Unreadable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    Weak,
    Medium,
    Strong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub detail: String,
    pub strength: Strength,
}

/// Rebuild command + measured/estimated cost — SPEC §6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Regen {
    pub command: String,
    pub cost_secs: Option<u32>,
    pub network: bool,
}

/// The classify verdict unit — SPEC §6. `path` is LOCAL ONLY, never serialized outward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: Cid,
    pub path: PathBuf,
    pub path_hash: PathHash,
    pub dev: u64,
    pub ino: u64,
    pub alloc_bytes: u64,
    pub app_bytes: u64,
    pub grade: Grade,
    pub evidence: Vec<Evidence>,
    pub regen: Option<Regen>,
    pub mtime: SystemTime,
    pub nlink: u32,
    pub clone_id: Option<u64>,
    pub locked: bool,
}

/// One volume as reported by scan — SPEC §3.1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub dev: u64,
    pub mount: PathBuf,
    pub fs: String,
    pub total: u64,
    pub free_raw: u64,
    pub free_important: u64,
    pub free_opportunistic: u64,
    pub read_only: bool,
    pub rename_excl: bool,
}

/// Plan entry — SPEC §6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub cid: Cid,
    pub path: PathBuf,
    pub path_hash: PathHash,
    pub alloc_bytes: u64,
    pub grade: Grade,
    pub action: PlanAction,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanAction {
    Quarantine,
    PruneCommand { cmd: String, dry_run: String },
    Refuse { reason: Refusal },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    MtimeHot,
    FileLock,
    OrphanTree,
    Dataless,
    ImmutableFlag,
    CrossVolume,
    NoRenameExcl,
    ReadOnly,
    BelowMinBytes,
    GradeNotAllowed,
}

/// WAL intent record — written + synced BEFORE the mutation — SPEC §6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    pub op_id: String, // WalOpId (uuid v4 string)
    pub kind: WalKind,
    pub plan_id: String,
    pub entry: PathHash,
    pub from: Option<PathBuf>,
    pub to: Option<PathBuf>,
    pub bytes: u64,
    pub undo_of: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalKind {
    Intent,
    Result,
}

/// Classification thresholds — config-tunable (SPEC Part A §7 provisional defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    pub hot_mtime: Duration,
    pub ancestor_depth: u8,
    pub min_bytes: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        // Owner-locked provisional defaults 2026-09-08: mtime 10 min / 50 MB min candidate.
        Self {
            hot_mtime: Duration::from_secs(600),
            ancestor_depth: 8,
            min_bytes: 50 * 1024 * 1024,
        }
    }
}

/// FNV-1a 64-bit path hash — stable identity for ledger/agent references.
pub fn path_hash(path: &std::path::Path) -> PathHash {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash.to_be_bytes()
}
