//! macOS classify impl: ancestry probes, path rules v1, evidence probes (SPEC §3.2).

use std::path::Path;
use std::time::SystemTime;

use crate::classify::{Ancestry, PathClass};
use crate::scan::{DirAgg, DirStat, Kind, dirstat_path_hash};
use crate::{Candidate, Evidence, EvidenceKind, Grade, Strength, Thresholds};

const MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "go.mod",
    "Gemfile",
    "composer.json",
];
const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Cargo.lock",
    "uv.lock",
    "poetry.lock",
    "Pipfile.lock",
    "Gemfile.lock",
];
/// BSD flag bits (xnu bsd/sys/stat.h) — classify treats any as a block/handoff signal.
pub const UF_IMMUTABLE: u32 = 0x0000_0002;
pub const UF_DATAVAULT: u32 = 0x1000_0000;
pub const SF_RESTRICTED: u32 = 0x0000_0800;
pub const SF_NOUNLINK: u32 = 0x0010_0000;
pub const SF_DATALESS: u32 = 0x4000_0000;

/// lstat + BSD flags probe (dua-core does not surface flags; see scan contract).
pub fn probe_flags(path: &Path) -> u32 {
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
        return 0;
    };
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: c points to a valid NUL-terminated path; st is a valid `stat` buffer.
    let rc = unsafe { libc::lstat(c.as_ptr(), &mut st) };
    if rc != 0 { 0 } else { st.st_flags }
}

pub fn probe_ancestry(path: &Path, depth_cap: u8) -> Ancestry {
    let mut anc = Ancestry::default();
    let mut cur = path.parent().map(Path::to_path_buf);
    let mut depth: u8 = 0;
    while let Some(dir) = cur {
        if depth >= depth_cap {
            break;
        }
        for m in MANIFESTS {
            let p = dir.join(m);
            if p.is_file() && anc.manifest.is_none() {
                anc.manifest = Some(p);
            }
        }
        for l in LOCKFILES {
            let p = dir.join(l);
            if p.is_file() && anc.lockfile.is_none() {
                anc.lockfile = Some(p);
            }
        }
        let git = dir.join(".git");
        if git.exists() && anc.git.is_none() {
            anc.git = Some(git);
        }
        let venv_marker = dir.join("pyvenv.cfg");
        if venv_marker.is_file() && anc.venv.is_none() {
            anc.venv = Some(venv_marker);
        }
        cur = std::path::PathBuf::from(&dir)
            .parent()
            .map(Path::to_path_buf);
        depth += 1;
    }
    anc.depth_scanned = depth;
    anc
}

/// Path-class rule table v1 (node [03 §1], rust [03 §2], python [03 §3], xcode [03 §4],
/// CAS stores [03 §1/§3]).
pub fn path_class(name: &str) -> PathClass {
    match name {
        "node_modules" => PathClass::NodeModules,
        "target" => PathClass::RustTarget,
        ".venv" | "venv" | "__pycache__" => PathClass::PythonVenv,
        "DerivedData" => PathClass::XcodeDerivedData,
        "_cacache" => PathClass::CasStore("npm"),
        ".pnpm-store" | "pnpm" => PathClass::CasStore("pnpm"),
        ".uv" | "uv" => PathClass::CasStore("uv"),
        ".gradle" | ".m2" | ".cargo" | ".npm" | ".cache" => PathClass::GenericCache("dotdir"),
        _ => PathClass::Unknown,
    }
}

/// Classify one aggregated directory into a Candidate (None = not a candidate).
pub fn classify_dir(
    dir: &DirStat,
    agg: &DirAgg,
    anc: &Ancestry,
    th: &Thresholds,
) -> Option<Candidate> {
    if dir.kind != Kind::Dir {
        return None;
    }
    if agg.alloc_bytes < th.min_bytes {
        return None;
    }
    let class = path_class(&dir.name);
    if class == PathClass::Unknown {
        return None;
    }
    let mut evidence: Vec<Evidence> = Vec::new();
    let mtime_now = SystemTime::now();
    let hot = agg
        .max_mtime
        .map(|m| {
            mtime_now
                .duration_since(m)
                .map(|d| d < th.hot_mtime)
                .unwrap_or(true)
        })
        .unwrap_or(false);

    // Orphan probe FIRST (promotion rule [03 §0]): no manifest in any ancestor.
    let orphan = anc.manifest.is_none();
    if orphan {
        evidence.push(Evidence {
            kind: EvidenceKind::OrphanTree,
            detail: "no build manifest in any ancestor (project deleted?)".into(),
            strength: Strength::Strong,
        });
    }
    if let Some(lock) = &anc.lockfile {
        evidence.push(Evidence {
            kind: EvidenceKind::LockfilePresent,
            detail: format!("lockfile present: {}", lock.display()),
            strength: Strength::Strong,
        });
    }
    if let Some(git) = &anc.git {
        evidence.push(Evidence {
            kind: EvidenceKind::GitRepoAncestor,
            detail: format!(".git ancestor: {}", git.display()),
            strength: Strength::Medium,
        });
    }
    if hot {
        evidence.push(Evidence {
            kind: EvidenceKind::MtimeHot,
            detail: "subtree written within hot-mtime window".into(),
            strength: Strength::Strong,
        });
    }
    if dir.clone_id.is_some() {
        evidence.push(Evidence {
            kind: EvidenceKind::ApfsClone,
            detail: "APFS clone identity present (byte discount)".into(),
            strength: Strength::Medium,
        });
    }
    if dir.nlink > 1 {
        evidence.push(Evidence {
            kind: EvidenceKind::Hardlinked,
            detail: format!("nlink={} (deleting one of N frees 0)", dir.nlink),
            strength: Strength::Medium,
        });
    }
    // Flag probe on the actual candidate (walker carries flags=0; apply re-verifies — TOCTOU armor).
    let flags = probe_flags(&dir.parent.join(&dir.name));
    if flags & (UF_IMMUTABLE | SF_RESTRICTED | SF_NOUNLINK | UF_DATAVAULT | SF_DATALESS) != 0 {
        evidence.push(Evidence {
            kind: EvidenceKind::FlagBlock(flag_name(flags)),
            detail: "BSD flags block move/purge".into(),
            strength: Strength::Strong,
        });
    }
    if evidence.is_empty() {
        return None; // every verdict carries ≥1 signal [03 §0]
    }

    let grade = if orphan {
        Grade::Expensive // surface as prompt, never auto-delete [03 §0]
    } else if hot {
        Grade::HandsOff
    } else {
        crate::classify::grade_for(&class)
    };
    Some(Candidate {
        id: "C-000".into(), // re-numbered deterministically at scan end (size desc, path_hash tie)
        path: dir.parent.join(&dir.name),
        path_hash: dirstat_path_hash(dir),
        dev: dir.dev,
        ino: dir.ino,
        alloc_bytes: agg.alloc_bytes,
        app_bytes: agg.app_bytes,
        grade,
        evidence,
        regen: crate::classify::regen_for(&class),
        mtime: agg.max_mtime.unwrap_or(dir.mtime),
        nlink: dir.nlink,
        clone_id: dir.clone_id,
        locked: false, // flock-marker probe lands with M2 FileLock evidence
    })
}

fn flag_name(flags: u32) -> String {
    if flags & UF_IMMUTABLE != 0 {
        "UF_IMMUTABLE".into()
    } else if flags & SF_RESTRICTED != 0 {
        "SF_RESTRICTED".into()
    } else if flags & SF_NOUNLINK != 0 {
        "SF_NOUNLINK".into()
    } else if flags & UF_DATAVAULT != 0 {
        "UF_DATAVAULT".into()
    } else {
        "SF_DATALESS".into()
    }
}
