//! Golden tests for the classify evidence engine (SPEC §3.2) — synthetic trees built
//! with plain std::fs under tempdirs; never touches real user data (course rule 4).

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use disk_janitor::classify::{candidates_from_batch, probe_flags};
use disk_janitor::scan::{DirStat, Kind};
use disk_janitor::{Candidate, EvidenceKind, Grade, Strength, Thresholds};

use tempfile::TempDir;

/// Test thresholds: tiny min_bytes so fixtures stay small; default 600 s hot window.
fn th() -> Thresholds {
    Thresholds {
        hot_mtime: Duration::from_secs(600),
        ancestor_depth: 8,
        min_bytes: 1024,
    }
}

fn fixture(name: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("tempdir")
}

/// Write `size` bytes of pattern `b` into `path` (creating parents).
fn write_file(path: &Path, size: usize, b: u8) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(path, vec![b; size]).unwrap();
}

/// Backdate a file's atime+mtime to `secs` since epoch — the COLD signal.
fn backdate(path: &Path, secs: i64) {
    use std::ffi::CString;
    let c = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    let tv = libc::timeval {
        tv_sec: secs,
        tv_usec: 0,
    };
    let times = [tv, tv];
    let rc = unsafe { libc::utimes(c.as_ptr(), times.as_ptr()) };
    assert_eq!(rc, 0, "utimes failed for {}", path.display());
}

/// Recursive std::fs walk → DirStat batch mirroring the scan contract (real metadata).
fn walk(root: &Path) -> Vec<DirStat> {
    fn dirstat(path: &Path, md: &std::fs::Metadata, root: &Path) -> DirStat {
        let kind = if md.file_type().is_dir() {
            Kind::Dir
        } else if md.file_type().is_symlink() {
            Kind::Symlink
        } else {
            Kind::File
        };
        DirStat {
            dev: md.dev(),
            ino: md.ino(),
            parent: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            depth: path
                .strip_prefix(root)
                .map(|p| p.components().count() as u16)
                .unwrap_or(0),
            kind,
            app_bytes: md.len(),
            alloc_bytes: md.blocks() * 512,
            entries: 0,
            mtime: md.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            flags: 0, // walker contract: flags surface at classify time, not scan time
            nlink: md.nlink() as u32,
            clone_id: None,
        }
    }

    fn rec(path: &Path, root: &Path, out: &mut Vec<DirStat>) {
        let md = std::fs::symlink_metadata(path).unwrap();
        if md.file_type().is_dir() {
            let mut children: Vec<_> = std::fs::read_dir(path)
                .unwrap()
                .filter_map(Result::ok)
                .collect();
            children.sort_by_key(|e| e.file_name());
            let mut d = dirstat(path, &md, root);
            d.entries = children.len() as u32;
            out.push(d);
            for e in children {
                let p = e.path();
                let m = std::fs::symlink_metadata(&p).unwrap();
                if m.file_type().is_dir() {
                    rec(&p, root, out);
                } else {
                    out.push(dirstat(&p, &m, root));
                }
            }
        } else {
            out.push(dirstat(path, &md, root));
        }
    }

    let mut out = Vec::new();
    rec(root, root, &mut out);
    out
}

fn has_evidence(c: &Candidate, kind: EvidenceKind) -> bool {
    c.evidence.iter().any(|e| e.kind == kind)
}

fn by_path<'a>(cands: &'a [Candidate], path: &Path) -> &'a Candidate {
    cands
        .iter()
        .find(|c| c.path == path)
        .expect("candidate for path")
}

// --- [03 §1] node: lockfile ⇒ RebuildableFromSource ---------------------------------

#[test]
fn node_modules_with_sibling_manifest_and_lockfile_is_rebuildable() {
    let t = fixture("dj-lockfile-");
    let root = t.path();
    write_file(&root.join("package.json"), 100, 1);
    write_file(&root.join("package-lock.json"), 500, 2);
    write_file(&root.join("node_modules/dep-a/index.js"), 200 * 1024, 3);
    write_file(&root.join("node_modules/dep-b/index.js"), 200 * 1024, 4);
    backdate(&root.join("package.json"), 1_600_000_000);
    backdate(&root.join("package-lock.json"), 1_600_000_100); // newer than manifest
    backdate(&root.join("node_modules/dep-a/index.js"), 1_600_000_000);
    backdate(&root.join("node_modules/dep-b/index.js"), 1_600_000_000);

    let cands = candidates_from_batch(walk(root), &th());
    assert_eq!(cands.len(), 1, "only node_modules surfaces");
    let c = &cands[0];
    assert_eq!(c.id, "C-001");
    assert_eq!(c.path, root.join("node_modules"));
    assert_eq!(c.grade, Grade::RebuildableFromSource);
    let lock = c
        .evidence
        .iter()
        .find(|e| e.kind == EvidenceKind::LockfilePresent)
        .expect("lockfile evidence");
    assert_eq!(lock.strength, Strength::Strong);
    assert!(
        !has_evidence(c, EvidenceKind::OrphanTree),
        "sibling manifest pins the tree"
    );
    let regen = c.regen.as_ref().expect("regen");
    assert_eq!(regen.command, "npm ci");
    assert!(regen.network);
}

// --- [03 §0] orphan: no manifest anywhere up to the depth cap ------------------------

#[test]
fn orphan_node_modules_is_expensive() {
    let t = fixture("dj-orphan-");
    let root = t.path();
    write_file(&root.join("node_modules/dep-a/index.js"), 200 * 1024, 3);
    write_file(&root.join("node_modules/dep-b/index.js"), 200 * 1024, 4);
    backdate(&root.join("node_modules/dep-a/index.js"), 1_600_000_000);
    backdate(&root.join("node_modules/dep-b/index.js"), 1_600_000_000);

    let cands = candidates_from_batch(walk(root), &th());
    assert_eq!(cands.len(), 1);
    let c = &cands[0];
    assert_eq!(
        c.grade,
        Grade::Expensive,
        "orphan promotes to prompt, never auto-delete"
    );
    let orph = c
        .evidence
        .iter()
        .find(|e| e.kind == EvidenceKind::OrphanTree)
        .expect("orphan evidence");
    assert_eq!(orph.strength, Strength::Strong);
    assert!(
        !has_evidence(c, EvidenceKind::LockfilePresent),
        "nothing pins the dep tree"
    );
}

// --- [03 §1] CAS store: shape probe ⇒ PureCache, whole-store prune only --------------

#[test]
fn cacache_store_is_pure_cache_with_cas_evidence() {
    let t = fixture("dj-cacache-");
    let root = t.path();
    write_file(
        &root.join("_cacache/content-v2/sha512/ab/aa"),
        200 * 1024,
        7,
    );
    write_file(
        &root.join("_cacache/content-v2/sha512/ab/bb"),
        200 * 1024,
        8,
    );
    write_file(
        &root.join("_cacache/content-v2/sha512/cd/cc"),
        200 * 1024,
        9,
    );
    // Files are hot by construction (written now): stores must NOT regrade on MtimeHot —
    // whole-store prune stays correct while hot [03 risks #7].
    let cands = candidates_from_batch(walk(root), &th());
    assert_eq!(cands.len(), 1);
    let c = &cands[0];
    assert_eq!(c.path, root.join("_cacache"));
    assert_eq!(c.grade, Grade::PureCache);
    assert!(has_evidence(c, EvidenceKind::CasStore));
    assert!(!has_evidence(c, EvidenceKind::MtimeHot));
    assert!(
        !has_evidence(c, EvidenceKind::OrphanTree),
        "stores never orphan"
    );
    assert!(
        c.regen.is_none(),
        "CAS stores prune as a whole; no regen command"
    );
}

#[test]
fn pnpm_and_uv_store_shapes_recognized() {
    let t = fixture("dj-stores-");
    let root = t.path();
    write_file(&root.join(".pnpm-store/files/00/aa"), 200 * 1024, 7);
    write_file(&root.join(".pnpm-store/files/00/bb"), 200 * 1024, 8);
    write_file(&root.join(".uv/archive-v0/xx/cc"), 200 * 1024, 9);
    write_file(&root.join(".uv/archive-v0/xx/dd"), 200 * 1024, 10);

    let cands = candidates_from_batch(walk(root), &th());
    assert_eq!(cands.len(), 2, "both stores surface");
    for c in &cands {
        assert_eq!(c.grade, Grade::PureCache);
        assert!(has_evidence(c, EvidenceKind::CasStore));
    }
}

// --- hardlinks: one inode counted once, whatever the link count ------------------------

#[test]
fn hardlinked_pair_inside_one_store_is_counted_once() {
    let t = fixture("dj-hardlink-");
    let root = t.path();
    let a = root.join(".pnpm-store/files/ab/blob.bin");
    let b = root.join(".pnpm-store/files/ab/blob2.bin");
    write_file(&a, 500 * 1024, 5);
    std::fs::hard_link(&a, &b).unwrap();

    let inode_bytes = std::fs::symlink_metadata(&a).unwrap().blocks() * 512;
    assert!(inode_bytes >= 500 * 1024, "fixture must own real bytes");

    let cands = candidates_from_batch(walk(root), &th());
    let c = by_path(&cands, &root.join(".pnpm-store"));
    assert_eq!(
        c.alloc_bytes, inode_bytes,
        "two links to one inode must contribute those bytes once, not twice"
    );
    assert_eq!(c.app_bytes, 500 * 1024, "apparent size dedups the same way");
}

// --- gates: unknown class and below-min bytes are NOT candidates ---------------------

#[test]
fn unknown_dir_without_manifest_is_not_a_candidate() {
    let t = fixture("dj-unknown-");
    let root = t.path();
    write_file(&root.join("random_assets/blob-1.bin"), 400 * 1024, 1);
    write_file(&root.join("random_assets/blob-2.bin"), 400 * 1024, 2);
    assert!(candidates_from_batch(walk(root), &th()).is_empty());
}

#[test]
fn below_min_bytes_dir_is_not_a_candidate() {
    // Custom gate: the 300 KiB node_modules must not surface with min_bytes = 1 MiB.
    let t = fixture("dj-small-");
    let root = t.path();
    write_file(&root.join("node_modules/dep-a/index.js"), 150 * 1024, 3);
    write_file(&root.join("node_modules/dep-b/index.js"), 150 * 1024, 4);
    let tight = Thresholds {
        hot_mtime: Duration::from_secs(600),
        ancestor_depth: 8,
        min_bytes: 1024 * 1024,
    };
    assert!(candidates_from_batch(walk(root), &tight).is_empty());

    // Default gate (50 MiB): a 1 MiB node_modules must not surface either.
    let t2 = fixture("dj-small2-");
    let root2 = t2.path();
    write_file(&root2.join("node_modules/dep-a/index.js"), 512 * 1024, 3);
    write_file(&root2.join("node_modules/dep-b/index.js"), 512 * 1024, 4);
    assert!(candidates_from_batch(walk(root2), &Thresholds::default()).is_empty());
}

// --- mtime: hot ⇒ HandsOff gate -------------------------------------------------------

#[test]
fn hot_subtree_is_handsoff() {
    let t = fixture("dj-hot-");
    let root = t.path();
    write_file(&root.join("package.json"), 100, 1);
    write_file(&root.join("node_modules/dep-a/index.js"), 200 * 1024, 3);
    write_file(&root.join("node_modules/dep-b/index.js"), 200 * 1024, 4);
    // mtimes = now ⇒ inside the 600 s hot window.

    let cands = candidates_from_batch(walk(root), &th());
    assert_eq!(cands.len(), 1);
    let c = &cands[0];
    assert_eq!(c.grade, Grade::HandsOff, "hot subtree gates auto-action");
    assert!(has_evidence(c, EvidenceKind::MtimeHot));
    assert!(!has_evidence(c, EvidenceKind::OrphanTree));
}

#[test]
fn hot_orphan_is_handsoff() {
    let t = fixture("dj-hotorph-");
    let root = t.path();
    write_file(&root.join("node_modules/dep-a/index.js"), 200 * 1024, 3);
    write_file(&root.join("node_modules/dep-b/index.js"), 200 * 1024, 4);

    let cands = candidates_from_batch(walk(root), &th());
    assert_eq!(cands.len(), 1);
    let c = &cands[0];
    assert_eq!(
        c.grade,
        Grade::HandsOff,
        "actively-written orphan: safe default is leave it"
    );
    assert!(has_evidence(c, EvidenceKind::OrphanTree));
    assert!(has_evidence(c, EvidenceKind::MtimeHot));
}

// --- SPEC §6: deterministic ids --------------------------------------------------------

#[test]
fn ids_are_deterministic_size_desc_then_path_hash() {
    let t = fixture("dj-ids-");
    let root = t.path();
    // A: 3 MiB — largest ⇒ C-001.
    write_file(
        &root.join("big/node_modules/dep-a/index.js"),
        1536 * 1024,
        3,
    );
    write_file(
        &root.join("big/node_modules/dep-b/index.js"),
        1536 * 1024,
        4,
    );
    // B, C: exactly equal subtrees ⇒ tie broken by path_hash asc (FNV bytes).
    write_file(
        &root.join("alpha/node_modules/dep-a/index.js"),
        200 * 1024,
        3,
    );
    write_file(
        &root.join("alpha/node_modules/dep-b/index.js"),
        200 * 1024,
        4,
    );
    write_file(
        &root.join("beta/node_modules/dep-a/index.js"),
        200 * 1024,
        3,
    );
    write_file(
        &root.join("beta/node_modules/dep-b/index.js"),
        200 * 1024,
        4,
    );

    let batch = walk(root);
    let ids = |b: Vec<DirStat>| -> Vec<(String, PathBuf, u64)> {
        candidates_from_batch(b, &th())
            .into_iter()
            .map(|c| (c.id, c.path, c.alloc_bytes))
            .collect()
    };
    let first = ids(batch.clone());
    assert_eq!(
        first,
        ids(batch.clone()),
        "same input twice ⇒ identical id sequence"
    );

    let mut reversed = batch.clone();
    reversed.reverse();
    assert_eq!(first, ids(reversed), "batch order must not change ids");

    assert_eq!(first.len(), 3);
    assert_eq!(first[0].0, "C-001");
    assert_eq!(first[0].1, root.join("big/node_modules"), "largest first");
    assert!(first[0].2 > first[1].2);
    assert_eq!(first[1].2, first[2].2, "the two small trees tie on bytes");

    // The equal-size pair must be ordered by path_hash asc — the SPEC §6 tie-break.
    let cands = candidates_from_batch(batch, &th());
    let tied: Vec<_> = cands
        .iter()
        .filter(|c| c.path != root.join("big/node_modules"))
        .collect();
    assert_eq!(tied.len(), 2);
    assert!(
        tied[0].path_hash <= tied[1].path_hash,
        "path_hash asc tie-break"
    );
}

// --- flags: UF_HIDDEN surfaces through the BSD-flag probe ------------------------------

#[test]
fn probe_flags_sees_uf_hidden() {
    use std::ffi::CString;
    let t = fixture("dj-hidden-");
    let dir = t.path().join("hidden");
    std::fs::create_dir(&dir).unwrap();
    let c = CString::new(dir.as_os_str().as_encoded_bytes()).unwrap();
    let rc = unsafe { libc::chflags(c.as_ptr(), libc::UF_HIDDEN) };
    assert_eq!(rc, 0, "chflags UF_HIDDEN failed");
    let flags = probe_flags(&dir);
    assert_ne!(flags, 0, "UF_HIDDEN must surface as nonzero flags");
    assert_ne!(flags & (libc::UF_HIDDEN as u32), 0, "UF_HIDDEN bit set");
}

/// The canonical `st_flags` table in `disk_janitor` must equal the OS ABI, not a
/// remembered copy. A previous local copy declared `UF_DATAVAULT = 0x1000_0000` and
/// `SF_RESTRICTED = 0x0000_0800` — both bit positions the SDK leaves unassigned — so
/// `probe_flags` could never match a genuinely restricted file, and `SF_RESTRICTED`
/// meant two different things in one crate.
///
/// The oracle is the SDK header itself: every `UF_*`/`SF_*` constant we declare is
/// looked up in `$(xcrun --show-sdk-path)/usr/include/sys/stat.h` and compared. This
/// also fails if a future SDK reassigns a bit we depend on.
#[test]
fn bsd_flag_constants_match_the_sdk_header() {
    const CANONICAL: &[(&str, u32)] = &[
        ("UF_SETTABLE", disk_janitor::UF_SETTABLE),
        ("UF_NODUMP", disk_janitor::UF_NODUMP),
        ("UF_IMMUTABLE", disk_janitor::UF_IMMUTABLE),
        ("UF_APPEND", disk_janitor::UF_APPEND),
        ("UF_OPAQUE", disk_janitor::UF_OPAQUE),
        ("UF_COMPRESSED", disk_janitor::UF_COMPRESSED),
        ("UF_TRACKED", disk_janitor::UF_TRACKED),
        ("UF_DATAVAULT", disk_janitor::UF_DATAVAULT),
        ("UF_HIDDEN", disk_janitor::UF_HIDDEN),
        ("SF_SUPPORTED", disk_janitor::SF_SUPPORTED),
        ("SF_SETTABLE", disk_janitor::SF_SETTABLE),
        ("SF_SYNTHETIC", disk_janitor::SF_SYNTHETIC),
        ("SF_ARCHIVED", disk_janitor::SF_ARCHIVED),
        ("SF_IMMUTABLE", disk_janitor::SF_IMMUTABLE),
        ("SF_APPEND", disk_janitor::SF_APPEND),
        ("SF_RESTRICTED", disk_janitor::SF_RESTRICTED),
        ("SF_NOUNLINK", disk_janitor::SF_NOUNLINK),
        ("SF_FIRMLINK", disk_janitor::SF_FIRMLINK),
        ("SF_DATALESS", disk_janitor::SF_DATALESS),
    ];

    let sdk = std::process::Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .expect("xcrun must run on a macOS dev machine");
    assert!(sdk.status.success(), "xcrun --show-sdk-path failed");
    let header = std::path::PathBuf::from(String::from_utf8_lossy(&sdk.stdout).trim())
        .join("usr/include/sys/stat.h");
    let text = std::fs::read_to_string(&header)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", header.display()));

    // Live `#define NAME 0x…` lines only — commented-out ones (UF_NOUNLINK, SF_SNAPSHOT)
    // are FreeBSD-reserved and carry no assigned bit.
    let mut declared: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define") else {
            continue;
        };
        let mut it = rest.split_whitespace();
        let (Some(name), Some(value)) = (it.next(), it.next()) else {
            continue;
        };
        if !(name.starts_with("UF_") || name.starts_with("SF_")) {
            continue;
        }
        if let Some(hex) = value.strip_prefix("0x")
            && let Ok(v) = u64::from_str_radix(hex, 16)
        {
            declared.insert(name.to_string(), v);
        }
    }
    assert!(
        declared.len() > 10,
        "parsed only {} flag defines from {} — header format changed?",
        declared.len(),
        header.display()
    );

    for (name, ours) in CANONICAL {
        let sdk_value = declared
            .get(*name)
            .unwrap_or_else(|| panic!("{name} is not a live #define in the SDK header"));
        assert_eq!(
            u64::from(*ours),
            *sdk_value,
            "{name}: our table says {:#010x}, the SDK says {:#010x}",
            ours,
            sdk_value
        );
    }
}

// --- [03 §6] sparse files: HandsOff; quarantine ⇒ Redownloadable -----------------------

#[test]
fn sparse_files_are_handsoff_and_quarantined_ones_redownloadable() {
    let t = fixture("dj-sparse-");
    let root = t.path().to_path_buf();
    let clean = root.join("Docker.raw");
    let quar = root.join("big-download.bin");
    std::fs::write(&clean, b"x").unwrap();
    std::fs::write(&quar, b"y").unwrap();
    xattr::set(&quar, "com.apple.quarantine", b"0083;5f0a1b2c;Safari;").unwrap();

    let md_root = std::fs::symlink_metadata(&root).unwrap();
    let md_clean = std::fs::symlink_metadata(&clean).unwrap();
    let md_quar = std::fs::symlink_metadata(&quar).unwrap();

    let root_entry = DirStat {
        dev: md_root.dev(),
        ino: md_root.ino(),
        parent: root.parent().unwrap().to_path_buf(),
        name: root.file_name().unwrap().to_string_lossy().into_owned(),
        depth: 0,
        kind: Kind::Dir,
        app_bytes: md_root.len(),
        alloc_bytes: md_root.blocks() * 512,
        entries: 2,
        mtime: md_root.modified().unwrap(),
        flags: 0,
        nlink: md_root.nlink() as u32,
        clone_id: None,
    };
    // Sparse candidates: 64 MiB apparent / 2 MiB allocated (real path + real inode so the
    // quarantine probe reads the actual file).
    let sparse = |md: &std::fs::Metadata, name: &str| DirStat {
        dev: md.dev(),
        ino: md.ino(),
        parent: root.clone(),
        name: name.into(),
        depth: 1,
        kind: Kind::File,
        app_bytes: 64 * 1024 * 1024,
        alloc_bytes: 2 * 1024 * 1024,
        entries: 0,
        mtime: md.modified().unwrap(),
        flags: 0,
        nlink: md.nlink() as u32,
        clone_id: None,
    };
    // Fully-allocated 64 MiB file: not sparse ⇒ never a candidate.
    let filled = DirStat {
        dev: md_clean.dev(),
        ino: md_clean.ino(),
        parent: root.clone(),
        name: "filled.bin".into(),
        depth: 1,
        kind: Kind::File,
        app_bytes: 64 * 1024 * 1024,
        alloc_bytes: 64 * 1024 * 1024,
        entries: 0,
        mtime: md_clean.modified().unwrap(),
        flags: 0,
        nlink: 1,
        clone_id: None,
    };

    let cands = candidates_from_batch(
        vec![
            root_entry,
            sparse(&md_clean, "Docker.raw"),
            sparse(&md_quar, "big-download.bin"),
            filled,
        ],
        &th(),
    );
    assert_eq!(cands.len(), 2, "only the two sparse files surface");
    let c_clean = by_path(&cands, &clean);
    assert_eq!(
        c_clean.grade,
        Grade::HandsOff,
        "sparse file: HandsOff as a file [03 §6]"
    );
    assert!(has_evidence(c_clean, EvidenceKind::Sparse));
    let c_quar = by_path(&cands, &quar);
    assert_eq!(
        c_quar.grade,
        Grade::Redownloadable,
        "quarantine boosts to Redownloadable"
    );
    assert!(has_evidence(c_quar, EvidenceKind::Sparse));
    assert!(has_evidence(c_quar, EvidenceKind::QuarantineXattr));
}
