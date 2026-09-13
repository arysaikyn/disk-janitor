//! Shared contract types: `DirStat`, `ScanEvent`, `Candidate`, `Evidence`, `PlanEntry`,
//! `LedgerRecord`, `WalEntry` — SPEC Part B §6 (field lists are the contract).
//!
//! Every module (scan/classify/plan/execute/ledger/agent) builds on these types; they are
//! the cross-subagent interface. Field lists follow SPEC §6 verbatim; additions are
//! forbidden without an SPEC/ADR sync.

pub mod agent;
pub mod classify;
pub mod config;
pub mod cost;
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
    /// `st_dev` of the mount point — the same-volume test for quarantine moves
    /// (SPEC §5.1: a rename cannot cross filesystems).
    pub dev: u64,
    pub mount: PathBuf,
    /// `statfs.f_fstypename` (e.g. `apfs`) — drives the `rename_excl` probe.
    pub fs: String,
    /// Total bytes on the volume (`statfs.f_blocks × f_bsize`).
    pub total: u64,
    /// Raw filesystem free space from `statfs(2)` `f_bavail × f_bsize`. Excludes
    /// purgeable space (local Time Machine snapshots, caches, swap) — what a
    /// non-privileged caller can allocate right now [02 §3].
    pub free_raw: u64,
    /// Free + purgeable — the Finder / About-This-Mac "available" figure, read
    /// from `kCFURLVolumeAvailableCapacityForImportantUsageKey` via sysinfo
    /// (`sysinfo-0.39.6/src/unix/apple/disk.rs:379-392`; falls back to the raw
    /// key only when that one reads 0) [02 §3]. Always ≥ [`Self::free_raw`].
    pub free_important: u64,
    /// `NSURLVolumeAvailableCapacityForOpportunisticUsageKey` — important space
    /// plus opportunistic (long-idle) space. Hard-zero: no sysinfo path and no
    /// CoreFoundation read in-tree (`TODO(M3)`).
    pub free_opportunistic: u64,
    /// Mount is read-only for our purposes — `statfs` `MNT_RDONLY` *and* a
    /// create+remove probe failed. The sealed-system `/` reports the flag while
    /// firmlinked writes land on the Data slice, so the probe decides.
    pub read_only: bool,
    /// `renamex_np(RENAME_EXCL)` is supported (APFS/HFS+) — the primitive that
    /// makes quarantine collision-free without a check-then-act race (SPEC §5.1).
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
    /// (dev, ino) of the underlying inode — the hardlink dedup key for byte
    /// totals (SPEC §3.3: the same inode can only be reclaimed once). `None`
    /// for manifests predating this field: such entries are treated as their
    /// own inode (legacy Σ semantics), never collapsed into one dedup group.
    pub inode: Option<(u64, u64)>,
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
    /// Deny-glob hit (D-32: policy refusals distinguishable from grade refusals).
    PolicyDeny,
    /// target_bytes reached or max_entries hit (whole-entries-only selection caps).
    TargetReached,
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

// --- macOS BSD file flags (`st_flags`) — the single canonical table ------------------
//
// ONE SDK-correct definition, shared by `classify/` (block/handoff evidence), `execute/`
// (apply-time TOCTOU re-probe) and the fixture generator. Do NOT re-declare these bits
// per module: a second copy previously drifted, and `SF_RESTRICTED` held two different
// values in this one crate.
//
// Values verified against `$(xcrun --show-sdk-path)/usr/include/sys/stat.h`. `UF_*` are
// the user-settable (UP) flags, `SF_*` the super-user ones. libc 0.2.189 exposes only
// UF_SETTABLE…UF_HIDDEN, SF_SETTABLE, SF_ARCHIVED, SF_IMMUTABLE and SF_APPEND — the
// remaining bits are spelled out here because libc does not have them.
//
// Absent on purpose: `UF_NOUNLINK` and `SF_SNAPSHOT` (xnu comments both out as
// FreeBSD-reserved, so no live bit is assigned), and `SF_HIDDEN` (the header states it
// does not exist).

/// Mask of user-settable (`UF_*`) bits.
pub const UF_SETTABLE: u32 = 0x0000_ffff;
pub const UF_NODUMP: u32 = 0x0000_0001;
/// Immutable: cannot be written, unlinked or renamed.
pub const UF_IMMUTABLE: u32 = 0x0000_0002;
/// Append-only.
pub const UF_APPEND: u32 = 0x0000_0004;
/// Opaque (union-mount member).
pub const UF_OPAQUE: u32 = 0x0000_0008;
/// Compressed by the filesystem.
pub const UF_COMPRESSED: u32 = 0x0000_0020;
pub const UF_TRACKED: u32 = 0x0000_0040;
/// Data-vault protected — an entitlement is required to read or write.
pub const UF_DATAVAULT: u32 = 0x0000_0080;
/// Hidden from Finder.
pub const UF_HIDDEN: u32 = 0x0000_8000;

/// Mask of super-user (`SF_*`) bits supported on this platform.
pub const SF_SUPPORTED: u32 = 0x009f_0000;
/// Mask of super-user (`SF_*`) bits settable via `chflags(2)`.
pub const SF_SETTABLE: u32 = 0x3fff_0000;
/// Mask marking the flags a filesystem synthesises rather than stores.
pub const SF_SYNTHETIC: u32 = 0xc000_0000;
pub const SF_ARCHIVED: u32 = 0x0001_0000;
/// Immutable: the super-user counterpart of `UF_IMMUTABLE`.
pub const SF_IMMUTABLE: u32 = 0x0002_0000;
/// Append-only: the super-user counterpart of `UF_APPEND`.
pub const SF_APPEND: u32 = 0x0004_0000;
/// Data-vault protected — an entitlement is required to write.
pub const SF_RESTRICTED: u32 = 0x0008_0000;
/// Cannot be unlinked even by an owning process.
pub const SF_NOUNLINK: u32 = 0x0010_0000;
/// Firmlink: the entry redirects to the data volume.
pub const SF_FIRMLINK: u32 = 0x0080_0000;
/// Dataless: the content lives in iCloud and must be materialised first.
pub const SF_DATALESS: u32 = 0x4000_0000;
