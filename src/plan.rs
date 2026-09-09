//! `plan/` — candidate selection under policy constraints → reviewable JSON manifest
//! (SPEC Part B §3.3). Zero side effects; pre-flight refusals so apply never surprises.
use serde::{Deserialize, Serialize};

use crate::{PlanEntry, VolumeInfo};

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

pub fn build(
    _cands: &[crate::Candidate],
    _pc: &PolicyConstraintsShape,
    _vols: &[VolumeInfo],
) -> Plan {
    unimplemented!("M2: plan builder")
}

pub fn write_manifest(_p: &Plan, _path: &std::path::Path) -> std::io::Result<()> {
    unimplemented!("M2: manifest write")
}

pub fn load_manifest(_path: &std::path::Path) -> std::io::Result<Plan> {
    unimplemented!("M2: manifest load")
}

pub fn summarize(_p: &Plan) -> String {
    unimplemented!("M2: summarize")
}
