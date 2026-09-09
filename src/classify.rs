//! `classify/` — evidence engine + rebuild-cost grading (SPEC Part B §3.2).
//! OWNER: classify agent. Two layers: path-class rules (node/rust/python/xcode/…)
//! then evidence probes (lockfile, orphan, mtime, nlink/clone, flags, xattr).

use crate::scan::{DirAgg, DirStat};
use crate::{Candidate, Grade, Regen, Thresholds};

/// Ancestry probe result — what `probe_ancestry` reports for one candidate dir.
#[derive(Debug, Clone, Default)]
pub struct Ancestry {
    /// Manifest file found in the candidate or an ancestor (package.json, Cargo.toml, pyproject.toml, *.xcodeproj).
    pub manifest: Option<std::path::PathBuf>,
    /// Lockfile found (package-lock.json / pnpm-lock.yaml / Cargo.lock / uv.lock / Poetry.lock).
    pub lockfile: Option<std::path::PathBuf>,
    /// `.git` ancestor found.
    pub git: Option<std::path::PathBuf>,
    /// Python venv marker found (pyvenv.cfg / .venv).
    pub venv: Option<std::path::PathBuf>,
    /// Depth at which probing stopped (hit cap or root).
    pub depth_scanned: u8,
}

/// Recognized path-class of junk shape — drives grade + regen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathClass {
    NodeModules,
    RustTarget,
    PythonVenv,
    XcodeDerivedData,
    CasStore(&'static str), // npm _cacache / pnpm store / uv archive-v0
    GenericCache(&'static str),
    Unknown,
}

/// Aggregate a scanned directory tree (per-candidate fold from scan batches).
pub trait DirSink {
    fn on_dirstat(&mut self, d: &DirStat);
    fn finish(self) -> Vec<Candidate>;
}

pub fn regen_for(kind: &PathClass) -> Option<Regen> {
    match kind {
        PathClass::NodeModules => Some(Regen {
            command: "npm ci".into(),
            cost_secs: Some(40),
            network: true,
        }),
        PathClass::RustTarget => Some(Regen {
            command: "cargo build".into(),
            cost_secs: None,
            network: false,
        }),
        PathClass::PythonVenv => Some(Regen {
            command: "python -m venv .venv && pip install -r requirements.txt".into(),
            cost_secs: None,
            network: true,
        }),
        PathClass::XcodeDerivedData => Some(Regen {
            command: "xcodebuild build".into(),
            cost_secs: None,
            network: false,
        }),
        _ => None,
    }
}

pub fn grade_for(kind: &PathClass) -> Grade {
    match kind {
        PathClass::NodeModules
        | PathClass::RustTarget
        | PathClass::PythonVenv
        | PathClass::XcodeDerivedData => Grade::RebuildableFromSource,
        PathClass::CasStore(_) | PathClass::GenericCache(_) => Grade::PureCache,
        PathClass::Unknown => Grade::Expensive,
    }
}

/// Probe ancestors for manifests/lockfiles/.git/venv markers up to `depth_cap`.
pub fn probe_ancestry(path: &std::path::Path, depth_cap: u8) -> Ancestry {
    probe_ancestry_impl(path, depth_cap)
}

/// Classify one aggregated directory into a Candidate (None = not a candidate).
pub fn classify_dir(
    dir: &DirStat,
    agg: &DirAgg,
    anc: &Ancestry,
    th: &Thresholds,
) -> Option<Candidate> {
    classify_dir_impl(dir, agg, anc, th)
}

/// Evidence probe: BSD flags + xattr + lock presence on the actual candidate path.
/// The classifier takes probed flags as INPUT (testable per SPEC §7).
pub fn probe_flags(path: &std::path::Path) -> u32 {
    probe_flags_impl(path)
}

#[cfg(target_os = "macos")]
fn probe_ancestry_impl(path: &std::path::Path, depth_cap: u8) -> Ancestry {
    crate::classify::macos::probe_ancestry(path, depth_cap)
}
#[cfg(target_os = "macos")]
fn classify_dir_impl(
    dir: &DirStat,
    agg: &DirAgg,
    anc: &Ancestry,
    th: &Thresholds,
) -> Option<Candidate> {
    crate::classify::macos::classify_dir(dir, agg, anc, th)
}
#[cfg(target_os = "macos")]
fn probe_flags_impl(path: &std::path::Path) -> u32 {
    crate::classify::macos::probe_flags(path)
}

#[cfg(not(target_os = "macos"))]
fn probe_ancestry_impl(_p: &std::path::Path, _d: u8) -> Ancestry {
    Ancestry::default()
}
#[cfg(not(target_os = "macos"))]
fn classify_dir_impl(
    _d: &DirStat,
    _a: &DirAgg,
    _anc: &Ancestry,
    _t: &Thresholds,
) -> Option<Candidate> {
    None
}
#[cfg(not(target_os = "macos"))]
fn probe_flags_impl(_p: &std::path::Path) -> u32 {
    0
}

#[cfg(target_os = "macos")]
pub(crate) mod macos;
