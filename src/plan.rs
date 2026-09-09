//! `plan/` — candidate selection under policy constraints → reviewable JSON manifest
//! (SPEC Part B §3.3). Zero side effects; pre-flight refusals so apply never surprises.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::{Candidate, EvidenceKind, PlanAction, PlanEntry, Refusal, VolumeInfo};

/// Compiled policy constraints — SPEC §6 `PolicyConstraints`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConstraintsShape {
    pub target_bytes: Option<u64>,
    pub min_bytes: u64,
    pub allow_grades: Vec<crate::Grade>,
    pub deny_globs: Vec<String>,
    pub deny_under_mta_secs: u64,
    pub allow_network_regen: bool,
    pub allow_prune_commands: bool,
    pub grace_hours: u64,
    pub max_entries: u32,
}

impl Default for PolicyConstraintsShape {
    fn default() -> Self {
        Self {
            target_bytes: None,
            min_bytes: 50 * 1024 * 1024,
            allow_grades: vec![
                crate::Grade::PureCache,
                crate::Grade::RebuildableFromSource,
                crate::Grade::Redownloadable,
            ],
            deny_globs: Vec::new(),
            deny_under_mta_secs: 0,
            allow_network_regen: true,
            allow_prune_commands: true,
            grace_hours: 72,
            max_entries: 10_000,
        }
    }
}

/// The reviewable manifest (terraform-plan style) — written/loaded as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub plan_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub entries: Vec<PlanEntry>,
    pub totals: PlanTotals,
    pub policy_snapshot: PolicyConstraintsShape,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanTotals {
    pub quarantine_bytes: u64,
    pub quarantine_entries: u32,
    pub prune_bytes: u64,
    pub prune_cmds: u32,
    pub refused: u32,
    /// Snapshot-pinning caveat text when TM local snapshots exist.
    pub snapshot_caveat: Option<String>,
}

/// Pre-flight decision for one candidate, in SPEC §3.3 rule order.
///
/// 1. Hard refusals (evidence-backed): MtimeHot, OrphanTree, FileLock (evidence or
///    `locked`), Dataless, flag-blocks → ImmutableFlag, below min_bytes, grade outside
///    the allow-list.
/// 2. Environment refusals: dev unknown to `vols` → CrossVolume; read-only volume;
///    volume without RENAME_EXCL.
/// 3. Policy refusals: deny-glob hit (path or any ancestor), mtime inside the
///    deny_under_mta window.
/// 4. Selection caps: target_bytes reached (whole entries only, ≥1 always kept),
///    max_entries.
///
/// `acc_bytes` is the deduped quota already scheduled; `quarantine_entries` is the
/// number of Quarantine actions already assigned.
#[allow(clippy::too_many_arguments)]
fn decide(
    c: &Candidate,
    pc: &PolicyConstraintsShape,
    vols: &[VolumeInfo],
    deny: &[globset::GlobMatcher],
    now: SystemTime,
    acc_bytes: u64,
    quarantine_entries: u32,
) -> Option<Refusal> {
    let has = |kind: &EvidenceKind| c.evidence.iter().any(|e| &e.kind == kind);

    // --- 1. hard refusals (evidence-backed) -----------------------------------
    if has(&EvidenceKind::MtimeHot) {
        return Some(Refusal::MtimeHot);
    }
    if has(&EvidenceKind::OrphanTree) {
        return Some(Refusal::OrphanTree);
    }
    if has(&EvidenceKind::FileLock) || c.locked {
        return Some(Refusal::FileLock);
    }
    if has(&EvidenceKind::Dataless) {
        return Some(Refusal::Dataless);
    }
    if c.evidence
        .iter()
        .any(|e| matches!(e.kind, EvidenceKind::FlagBlock(_)))
    {
        return Some(Refusal::ImmutableFlag);
    }
    if c.alloc_bytes < pc.min_bytes {
        return Some(Refusal::BelowMinBytes);
    }
    if !pc.allow_grades.contains(&c.grade) {
        return Some(Refusal::GradeNotAllowed);
    }

    // --- 2. environment refusals ----------------------------------------------
    // A device may appear under several mounts (macOS firmlinks: sealed-RO `/` and
    // writable `/System/Volumes/Data` share st_dev). Refuse only if EVERY mount for
    // the device is unusable; prefer a writable mount for the quarantine target.
    let dev_vols: Vec<&VolumeInfo> = vols.iter().filter(|v| v.dev == c.dev).collect();
    if dev_vols.is_empty() {
        return Some(Refusal::CrossVolume);
    }
    let vol = match dev_vols.iter().copied().find(|v| !v.read_only) {
        Some(v) => v,
        None => return Some(Refusal::ReadOnly),
    };
    if !vol.rename_excl {
        return Some(Refusal::NoRenameExcl);
    }

    // --- 3. policy refusals ----------------------------------------------------
    // deny-globs protect the path itself and any ancestor subtree (a plain
    // "~/Library/…/Archives" glob must shield everything under it).
    if deny.iter().any(|m| {
        let mut p = Some(c.path.as_path());
        while let Some(cur) = p {
            if m.is_match(cur) {
                return true;
            }
            p = cur.parent();
        }
        false
    }) {
        // NOTE: Refusal has no PolicyDeny variant; GradeNotAllowed is the overflow
        // bucket for policy-level exclusions (see agent report).
        return Some(Refusal::GradeNotAllowed);
    }
    if pc.deny_under_mta_secs > 0 {
        let hot = match now.duration_since(c.mtime) {
            Ok(age) => age.as_secs() < pc.deny_under_mta_secs,
            // mtime in the future ⇒ hottest possible.
            Err(_) => true,
        };
        if hot {
            return Some(Refusal::MtimeHot);
        }
    }

    // --- 4. selection caps ------------------------------------------------------
    let target_hit = match pc.target_bytes {
        Some(t) => acc_bytes >= t && quarantine_entries > 0, // keep ≥1 entry
        None => false,
    };
    if target_hit || quarantine_entries >= pc.max_entries {
        return Some(Refusal::GradeNotAllowed);
    }

    None
}

/// Compile deny_globs once per build; a malformed pattern is skipped (counted nowhere —
/// surfaced in the report, never fatal).
fn compile_deny_globs(patterns: &[String]) -> Vec<globset::GlobMatcher> {
    patterns
        .iter()
        .filter_map(|p| globset::Glob::new(p).ok().map(|g| g.compile_matcher()))
        .collect()
}

fn entry(c: &Candidate, action: PlanAction) -> PlanEntry {
    PlanEntry {
        cid: c.id.clone(),
        path: c.path.clone(),
        path_hash: c.path_hash,
        alloc_bytes: c.alloc_bytes,
        grade: c.grade,
        action,
        evidence: c.evidence.clone(),
    }
}

/// Time Machine local-snapshot probe: `tmutil listlocalsnapshots /`; any failure
/// (missing binary, non-zero exit) ⇒ `None`. Non-empty output ⇒ caveat text.
fn snapshot_caveat() -> Option<String> {
    let out = Command::new("tmutil")
        .arg("listlocalsnapshots")
        .arg("/")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let n = stdout.lines().filter(|l| !l.trim().is_empty()).count();
    (n > 0).then(|| {
        format!(
            "Time Machine local snapshots active ({n} on /); reclaimed space may be \
             pinned until the snapshots expire"
        )
    })
}

pub fn build(cands: &[Candidate], pc: &PolicyConstraintsShape, vols: &[VolumeInfo]) -> Plan {
    let deny = compile_deny_globs(&pc.deny_globs);
    let now = SystemTime::now();

    let mut entries: Vec<PlanEntry> = Vec::with_capacity(cands.len());
    // Per-(dev,ino) hardlink dedup: the same inode can only be reclaimed once.
    let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut quarantine_bytes: u64 = 0;
    let mut quarantine_entries: u32 = 0;
    let mut acc_bytes: u64 = 0;
    let mut refused: u32 = 0;

    // Candidates arrive in deterministic size-desc order (SPEC §6 invariant); greedy
    // target selection follows that order. Whole entries only — no mid-candidate cuts.
    for c in cands {
        match decide(c, pc, vols, &deny, now, acc_bytes, quarantine_entries) {
            Some(reason) => {
                refused += 1;
                entries.push(entry(c, PlanAction::Refuse { reason }));
            }
            None => {
                // M2 keeps PruneCommand structural (PlanAction::PruneCommand exists) but
                // generates none: brew/docker/CAS pruning needs [03 §5/§6] integration
                // (M2+). Those candidates quarantine for now.
                if seen_inodes.insert((c.dev, c.ino)) {
                    quarantine_bytes += c.alloc_bytes;
                    acc_bytes += c.alloc_bytes;
                }
                quarantine_entries += 1;
                entries.push(entry(c, PlanAction::Quarantine));
            }
        }
    }

    Plan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Utc::now(),
        entries,
        totals: PlanTotals {
            quarantine_bytes,
            quarantine_entries,
            prune_bytes: 0,
            prune_cmds: 0,
            refused,
            snapshot_caveat: snapshot_caveat(),
        },
        policy_snapshot: pc.clone(),
    }
}

fn json_io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

pub fn write_manifest(p: &Plan, path: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(p).map_err(json_io_err)?;
    fs_err::write(path, json)
}

pub fn load_manifest(path: &Path) -> std::io::Result<Plan> {
    let text = fs_err::read_to_string(path)?;
    serde_json::from_str(&text).map_err(json_io_err)
}

pub fn summarize(p: &Plan) -> String {
    let gib = |b: u64| b as f64 / 1024.0f64.powi(3);
    let mut s = format!(
        "quarantine {:.1} GiB / {} entries; refuse {}",
        gib(p.totals.quarantine_bytes),
        p.totals.quarantine_entries,
        p.totals.refused
    );
    if p.totals.prune_cmds > 0 {
        s = format!(
            "quarantine {:.1} GiB / {} entries; prune {:.1} GiB via {} cmds; refuse {}",
            gib(p.totals.quarantine_bytes),
            p.totals.quarantine_entries,
            gib(p.totals.prune_bytes),
            p.totals.prune_cmds,
            p.totals.refused
        );
    }
    if let Some(caveat) = &p.totals.snapshot_caveat {
        s.push_str("; caveat: ");
        s.push_str(caveat);
    }
    s
}
