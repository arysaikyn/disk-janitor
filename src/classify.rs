//! `classify/` — evidence engine + rebuild-cost grading (SPEC Part B §3.2).
//! OWNER: classify agent. Two layers: path-class rules (node/rust/python/xcode/…)
//! then evidence probes (lockfile, orphan, mtime, nlink/clone, flags, xattr).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::scan::{DirAgg, DirStat, Kind, dirstat_path_hash};
use crate::{Candidate, Evidence, EvidenceKind, Grade, Regen, Strength, Thresholds};

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

/// M1 aggregation pass: fold `scan` batches into per-directory subtree aggregates, probe
/// ancestry, classify, and assign deterministic ids (SPEC §6: size-desc, path_hash tie).
/// Order-independent — batch ordering / streaming splits never change the result.
/// Single-threaded by design — classification is I/O-free work over an already-walked
/// batch, and the `rayon` parallelization candidate was surveyed but **not adopted**
/// (no `rayon` dependency; the walker's own pool does the parallel part).
pub fn candidates_from_batch(batches: Vec<DirStat>, th: &Thresholds) -> Vec<Candidate> {
    // 1. Index directories, then fold files into their containing directory's aggregate
    //    (`fold_file` semantics: parent = the dir named by parent.join(name) for kind==Dir).
    //    Two passes so batch ordering never matters — a file may precede its parent dir.
    let mut dirs: HashMap<PathBuf, DirStat> = HashMap::new();
    let mut aggs: HashMap<PathBuf, DirAgg> = HashMap::new();
    for d in &batches {
        if d.kind == Kind::Dir {
            let full = d.parent.join(&d.name);
            aggs.entry(full.clone()).or_default();
            dirs.insert(full, d.clone());
        }
    }
    // Inode ownership: a hardlinked file is ONE inode on disk, but it appears once per
    // link, so a naive fold adds the same bytes N times. That inflation is not caught
    // downstream — `plan::build`'s seen_inodes and `execute::purge`'s expected total both
    // key on the *candidate* (dev, ino), and a directory candidate has one inode, so the
    // gate re-derives the inflated number and agrees with it instead of refusing.
    // Exactly ONE link per inode folds, chosen deterministically as the deepest containing
    // directory then the lexicographically smallest full path — the same ordering rule the
    // subtree fold below uses, and independent of batch order / streaming splits.
    let mut owners: HashMap<(u64, u64), (u16, &PathBuf, &String)> = HashMap::new();
    for d in &batches {
        if d.kind != Kind::File {
            continue;
        }
        let Some(dir) = dirs.get(&d.parent) else {
            continue; // file outside every indexed dir: not folded either way
        };
        let key = (d.dev, d.ino);
        let better = match owners.get(&key) {
            None => true,
            Some((depth, parent, name)) => {
                dir.depth > *depth
                    || (dir.depth == *depth && (&d.parent, &d.name) < (*parent, *name))
            }
        };
        if better {
            owners.insert(key, (dir.depth, &d.parent, &d.name));
        }
    }
    for d in &batches {
        if d.kind == Kind::File
            && let Some(a) = aggs.get_mut(&d.parent)
            && owners.get(&(d.dev, d.ino)) == Some(&(dirs[&d.parent].depth, &d.parent, &d.name))
        {
            a.fold_file(d.app_bytes, d.alloc_bytes, d.mtime);
        }
    }

    // 2. Bottom-up subtree fold (depth desc): after this pass each directory's aggregate is
    //    the sum over all descendant *files* (alloc_bytes, entry count, max file mtime).
    let mut order: Vec<PathBuf> = dirs.keys().cloned().collect();
    order.sort_by(|a, b| dirs[b].depth.cmp(&dirs[a].depth).then_with(|| a.cmp(b)));
    for full in &order {
        let Some(parent) = full.parent() else {
            continue;
        };
        let sub = aggs.get(full).cloned().unwrap_or_default();
        if let Some(parent_agg) = aggs.get_mut(parent) {
            fold_subtree(parent_agg, &sub);
        }
    }

    // 3. Directory candidates: size gate → ancestry probe → classify.
    let mut cands: Vec<Candidate> = Vec::new();
    for (full, ds) in &dirs {
        let agg = &aggs[full];
        if agg.alloc_bytes < th.min_bytes {
            continue; // below-min gate: never a candidate [03 §0]
        }
        let anc = probe_ancestry(full, th.ancestor_depth);
        if let Some(c) = classify_dir(ds, agg, &anc, th) {
            cands.push(c);
        }
    }

    // 4. File candidates: sparse probe only (Docker.raw rule [03 §6]) — files surface as
    //    HandsOff (or Redownloadable with quarantine); never auto-deleted.
    for d in &batches {
        if d.kind != Kind::File {
            continue;
        }
        // Sparse: apparent size ≫ allocated (>4x and >1 MiB).
        if d.app_bytes <= d.alloc_bytes.saturating_mul(4) || d.app_bytes <= 1 << 20 {
            continue;
        }
        // Honest gate: reclaimable (allocated) bytes, mirroring the directory gate.
        if d.alloc_bytes <= th.min_bytes {
            continue;
        }
        let full = d.parent.join(&d.name);
        let mut evidence = vec![Evidence {
            kind: EvidenceKind::Sparse,
            detail: format!(
                "sparse file: {} B apparent / {} B allocated",
                d.app_bytes, d.alloc_bytes
            ),
            strength: Strength::Medium,
        }];
        // Hardlink byte discount [03 §0]: deleting one of N links frees 0 bytes.
        if d.nlink > 1 {
            evidence.push(Evidence {
                kind: EvidenceKind::Hardlinked,
                detail: format!("nlink={} (deleting one of N frees 0)", d.nlink),
                strength: Strength::Medium,
            });
        }
        let mut grade = Grade::HandsOff;
        if path_class_of(&d.name) == PathClass::Unknown {
            // Downloaded-by-app ⇒ re-fetchable [05 §6]: grade boost for unknown-class files.
            if let Some(q) = quarantine_evidence_impl(&full) {
                grade = Grade::Redownloadable;
                evidence.push(q);
            }
        }
        cands.push(Candidate {
            id: "C-000".into(),
            path: full,
            path_hash: dirstat_path_hash(d),
            dev: d.dev,
            ino: d.ino,
            alloc_bytes: d.alloc_bytes,
            app_bytes: d.app_bytes,
            grade,
            evidence,
            regen: None,
            mtime: d.mtime,
            nlink: d.nlink,
            clone_id: d.clone_id,
            locked: false,
        });
    }

    // 5. Deterministic ids: alloc desc, path_hash asc tie (SPEC §6 invariant).
    cands.sort_by(|a, b| {
        b.alloc_bytes
            .cmp(&a.alloc_bytes)
            .then_with(|| a.path_hash.cmp(&b.path_hash))
    });
    for (i, c) in cands.iter_mut().enumerate() {
        c.id = format!("C-{:03}", i + 1);
    }
    cands
}

/// Merge a child directory's subtree aggregate into its parent — `fold_file` semantics for
/// a whole subtree (no extra entry count for the directory node itself).
fn fold_subtree(parent: &mut DirAgg, sub: &DirAgg) {
    parent.app_bytes += sub.app_bytes;
    parent.alloc_bytes += sub.alloc_bytes;
    parent.entries += sub.entries;
    parent.max_mtime = match (parent.max_mtime, sub.max_mtime) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
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

#[cfg(target_os = "macos")]
fn quarantine_evidence_impl(path: &Path) -> Option<Evidence> {
    crate::classify::macos::quarantine_evidence(path)
}
#[cfg(not(target_os = "macos"))]
fn quarantine_evidence_impl(_p: &Path) -> Option<Evidence> {
    None
}

#[cfg(target_os = "macos")]
fn path_class_of(name: &str) -> PathClass {
    crate::classify::macos::path_class(name)
}
#[cfg(not(target_os = "macos"))]
fn path_class_of(_name: &str) -> PathClass {
    PathClass::Unknown
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
