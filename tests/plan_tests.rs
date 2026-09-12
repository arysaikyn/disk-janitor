//! Golden tests for the plan builder + NL policy compiler (SPEC Part B §3.3/§3.6).
//! All fixtures are synthetic (fake absolute paths, no fs access — plan is
//! zero-side-effect); tempdirs only for the manifest round trip (course rule 4).
//!
//! NOTE: the frozen contract types `PlanAction`/`Evidence` do NOT derive PartialEq
//! (lib.rs), so equality is compared structurally via `action_eq`/`evidence_eq`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use disk_janitor::plan::{self, Plan, PlanTotals, PolicyConstraintsShape};
use disk_janitor::policy::{self, TWO_WEEKS_SECS};
use disk_janitor::{
    Candidate, Evidence, EvidenceKind, Grade, PlanAction, Refusal, Strength, VolumeInfo,
};

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
const KIB: u64 = 1024;
const TIB: u64 = 1024 * 1024 * 1024 * 1024;

// --- equality helpers (frozen types lack PartialEq) -------------------------------

fn action_eq(a: &PlanAction, b: &PlanAction) -> bool {
    match (a, b) {
        (PlanAction::Quarantine, PlanAction::Quarantine) => true,
        (
            PlanAction::PruneCommand { cmd: x, dry_run: y },
            PlanAction::PruneCommand { cmd: u, dry_run: v },
        ) => x == u && y == v,
        (PlanAction::Refuse { reason: x }, PlanAction::Refuse { reason: y }) => x == y,
        _ => false,
    }
}

fn evidence_eq(a: &Evidence, b: &Evidence) -> bool {
    a.kind == b.kind && a.detail == b.detail && a.strength == b.strength
}

// --- fixture helpers --------------------------------------------------------------

fn cand(id: &str, grade: Grade, bytes: u64, dev: u64, ino: u64) -> Candidate {
    cand_at(id, &format!("/tmp/plan-test/{id}"), grade, bytes, dev, ino)
}

fn cand_at(id: &str, path: &str, grade: Grade, bytes: u64, dev: u64, ino: u64) -> Candidate {
    Candidate {
        id: id.to_string(),
        path: PathBuf::from(path),
        path_hash: disk_janitor::path_hash(Path::new(path)),
        dev,
        ino,
        alloc_bytes: bytes,
        app_bytes: bytes,
        grade,
        evidence: Vec::new(),
        regen: None,
        // UNIX_EPOCH = maximally cold; never inside a deny_under_mta window.
        mtime: SystemTime::UNIX_EPOCH,
        nlink: 1,
        clone_id: None,
        locked: false,
    }
}

fn ev(kind: EvidenceKind) -> Evidence {
    Evidence {
        kind,
        detail: String::new(),
        strength: Strength::Strong,
    }
}

fn with_ev(mut c: Candidate, kind: EvidenceKind) -> Candidate {
    c.evidence.push(ev(kind));
    c
}

fn with_mtime(mut c: Candidate, age_secs: u64) -> Candidate {
    c.mtime = SystemTime::now() - Duration::from_secs(age_secs);
    c
}

fn vol(dev: u64) -> VolumeInfo {
    VolumeInfo {
        dev,
        mount: PathBuf::from("/"),
        fs: "apfs".to_string(),
        total: 1 << 40,
        free_raw: 1 << 30,
        free_important: 1 << 30,
        free_opportunistic: 1 << 30,
        read_only: false,
        rename_excl: true,
    }
}

fn refuse(reason: Refusal) -> PlanAction {
    PlanAction::Refuse { reason }
}

// --- §3.3 rule table: hard refusals ----------------------------------------------

/// (a) candidates with mixed evidence → the exact Quarantine/Refuse split.
#[test]
fn mixed_evidence_yields_expected_refusal_split() {
    let pc = PolicyConstraintsShape::default();
    let vols = [vol(1)];
    let cands = vec![
        cand("C-001", Grade::PureCache, 100 * MIB, 1, 101), // → Quarantine
        with_ev(
            cand("C-002", Grade::PureCache, 100 * MIB, 1, 102),
            EvidenceKind::MtimeHot,
        ),
        with_ev(
            cand("C-003", Grade::PureCache, 100 * MIB, 1, 103),
            EvidenceKind::OrphanTree,
        ),
        with_ev(
            cand("C-004", Grade::PureCache, 100 * MIB, 1, 104),
            EvidenceKind::FileLock,
        ),
        with_ev(
            cand("C-005", Grade::PureCache, 100 * MIB, 1, 105),
            EvidenceKind::Dataless,
        ),
        with_ev(
            cand("C-006", Grade::PureCache, 100 * MIB, 1, 106),
            EvidenceKind::FlagBlock("UF_IMMUTABLE".into()),
        ),
        cand("C-007", Grade::PureCache, 10 * MIB, 1, 107), // < min_bytes
        cand("C-008", Grade::Expensive, 100 * MIB, 1, 108), // not in allow_grades
        {
            let mut c = cand("C-009", Grade::PureCache, 100 * MIB, 1, 109);
            c.locked = true; // FileLock
            c
        },
    ];
    let plan = plan::build(&cands, &pc, &vols);

    assert_eq!(plan.entries.len(), 9);
    assert!(action_eq(&plan.entries[0].action, &PlanAction::Quarantine));
    assert!(action_eq(
        &plan.entries[1].action,
        &refuse(Refusal::MtimeHot)
    ));
    assert!(action_eq(
        &plan.entries[2].action,
        &refuse(Refusal::OrphanTree)
    ));
    assert!(action_eq(
        &plan.entries[3].action,
        &refuse(Refusal::FileLock)
    ));
    assert!(action_eq(
        &plan.entries[4].action,
        &refuse(Refusal::Dataless)
    ));
    assert!(action_eq(
        &plan.entries[5].action,
        &refuse(Refusal::ImmutableFlag)
    ));
    assert!(action_eq(
        &plan.entries[6].action,
        &refuse(Refusal::BelowMinBytes)
    ));
    assert!(action_eq(
        &plan.entries[7].action,
        &refuse(Refusal::GradeNotAllowed)
    ));
    assert!(action_eq(
        &plan.entries[8].action,
        &refuse(Refusal::FileLock)
    ));

    assert_eq!(plan.totals.quarantine_bytes, 100 * MIB);
    assert_eq!(plan.totals.quarantine_entries, 1);
    assert_eq!(plan.totals.refused, 8);
    assert_eq!(plan.totals.prune_cmds, 0);
}

// --- §3.3 honesty: per-(dev,ino) hardlink dedup -----------------------------------

/// (b) two candidates sharing (dev,ino) → both entries, but bytes counted once.
#[test]
fn totals_dedup_hardlinked_inodes() {
    let pc = PolicyConstraintsShape::default();
    let vols = [vol(1)];
    let cands = vec![
        cand("C-001", Grade::PureCache, 100 * MIB, 1, 7),
        cand("C-002", Grade::PureCache, 100 * MIB, 1, 7), // same inode
        cand("C-003", Grade::PureCache, 100 * MIB, 1, 8),
    ];
    let plan = plan::build(&cands, &pc, &vols);

    assert_eq!(plan.totals.quarantine_entries, 3);
    assert_eq!(plan.totals.quarantine_bytes, 2 * 100 * MIB); // 1 + 1 distinct inodes
    assert_eq!(plan.totals.refused, 0);
    // B1: entries carry the (dev,ino) dedup key so purge() can recompute the
    // SAME deduped total from the manifest alone.
    assert_eq!(plan.entries[0].inode, Some((1, 7)));
    assert_eq!(plan.entries[1].inode, Some((1, 7)));
    assert_eq!(plan.entries[2].inode, Some((1, 8)));
}

// --- §3.3 target_bytes greedy (whole entries only) --------------------------------

/// (c) 3 × 10 GiB, target 15 GiB → 2 included (overshoot is fine), 1 refused.
#[test]
fn target_bytes_greedy_whole_entries() {
    let mut pc = PolicyConstraintsShape::default();
    pc.target_bytes = Some(15 * GIB);
    let vols = [vol(1)];
    let cands = vec![
        cand("C-001", Grade::PureCache, 10 * GIB, 1, 1),
        cand("C-002", Grade::PureCache, 10 * GIB, 1, 2),
        cand("C-003", Grade::PureCache, 10 * GIB, 1, 3),
    ];
    let plan = plan::build(&cands, &pc, &vols);

    assert!(action_eq(&plan.entries[0].action, &PlanAction::Quarantine));
    assert!(action_eq(&plan.entries[1].action, &PlanAction::Quarantine));
    assert!(action_eq(
        &plan.entries[2].action,
        &refuse(Refusal::TargetReached)
    ));
    assert_eq!(plan.totals.quarantine_bytes, 20 * GIB);
    assert_eq!(plan.totals.quarantine_entries, 2);
    assert_eq!(plan.totals.refused, 1);
}

/// target 0 must still keep at least one entry (never an empty action list).
#[test]
fn target_bytes_zero_keeps_one_entry() {
    let mut pc = PolicyConstraintsShape::default();
    pc.target_bytes = Some(0);
    let vols = [vol(1)];
    let cands = vec![
        cand("C-001", Grade::PureCache, 10 * GIB, 1, 1),
        cand("C-002", Grade::PureCache, 10 * GIB, 1, 2),
    ];
    let plan = plan::build(&cands, &pc, &vols);
    assert_eq!(plan.totals.quarantine_entries, 1);
    assert_eq!(plan.totals.refused, 1);
}

/// max_entries caps the number of Quarantine actions.
#[test]
fn max_entries_caps_quarantines() {
    let mut pc = PolicyConstraintsShape::default();
    pc.max_entries = 2;
    let vols = [vol(1)];
    let cands = vec![
        cand("C-001", Grade::PureCache, 1 * GIB, 1, 1),
        cand("C-002", Grade::PureCache, 1 * GIB, 1, 2),
        cand("C-003", Grade::PureCache, 1 * GIB, 1, 3),
        cand("C-004", Grade::PureCache, 1 * GIB, 1, 4),
    ];
    let plan = plan::build(&cands, &pc, &vols);
    assert_eq!(plan.totals.quarantine_entries, 2);
    assert_eq!(plan.totals.refused, 2);
    assert!(action_eq(
        &plan.entries[2].action,
        &refuse(Refusal::TargetReached)
    ));
}

// --- §3.3 policy refusals: deny_globs / deny_under_mta ----------------------------

/// A deny glob shields the named dir and its whole subtree (ancestor match).
#[test]
fn deny_glob_refuses_path_and_subtree() {
    let mut pc = PolicyConstraintsShape::default();
    pc.deny_globs
        .push("/tmp/plan-test/Library/Developer/Xcode/Archives".to_string());
    let vols = [vol(1)];
    let inside = cand_at(
        "C-001",
        "/tmp/plan-test/Library/Developer/Xcode/Archives/2026-09-01.xcarchive",
        Grade::PureCache,
        100 * MIB,
        1,
        1,
    );
    let outside = cand_at(
        "C-002",
        "/tmp/plan-test/Library/Developer/Xcode/DerivedData/ModuleCache",
        Grade::PureCache,
        100 * MIB,
        1,
        2,
    );
    let plan = plan::build(&[inside, outside], &pc, &vols);
    assert!(action_eq(
        &plan.entries[0].action,
        &refuse(Refusal::PolicyDeny)
    ));
    assert!(action_eq(&plan.entries[1].action, &PlanAction::Quarantine));
}

/// deny_under_mta: fresh mtime → MtimeHot refusal; old mtime passes.
#[test]
fn deny_under_mta_window_refuses_fresh_mtime() {
    let mut pc = PolicyConstraintsShape::default();
    pc.deny_under_mta_secs = 2 * 3600; // 2 h
    let vols = [vol(1)];
    let fresh = with_mtime(cand("C-001", Grade::PureCache, 100 * MIB, 1, 1), 3600); // 1 h old
    let old = with_mtime(cand("C-002", Grade::PureCache, 100 * MIB, 1, 2), 3 * 3600); // 3 h old
    let plan = plan::build(&[fresh, old], &pc, &vols);
    assert!(action_eq(
        &plan.entries[0].action,
        &refuse(Refusal::MtimeHot)
    ));
    assert!(action_eq(&plan.entries[1].action, &PlanAction::Quarantine));
}

// --- §3.3 environment refusals -----------------------------------------------------

#[test]
fn cross_volume_refused_when_dev_not_in_vols() {
    let pc = PolicyConstraintsShape::default();
    let vols = [vol(1)];
    let c = cand("C-001", Grade::PureCache, 100 * MIB, 99, 1); // dev 99 unknown
    let plan = plan::build(&[c], &pc, &vols);
    assert!(action_eq(
        &plan.entries[0].action,
        &refuse(Refusal::CrossVolume)
    ));
}

/// (g) read-only volume → ReadOnly refusal.
#[test]
fn read_only_volume_refused() {
    let pc = PolicyConstraintsShape::default();
    let vols = [VolumeInfo {
        read_only: true,
        ..vol(1)
    }];
    let c = cand("C-001", Grade::PureCache, 100 * MIB, 1, 1);
    let plan = plan::build(&[c], &pc, &vols);
    assert!(action_eq(
        &plan.entries[0].action,
        &refuse(Refusal::ReadOnly)
    ));
}

/// (g) volume without RENAME_EXCL (exFAT/FAT) → NoRenameExcl refusal.
#[test]
fn no_rename_excl_volume_refused() {
    let pc = PolicyConstraintsShape::default();
    let vols = [VolumeInfo {
        rename_excl: false,
        ..vol(1)
    }];
    let c = cand("C-001", Grade::PureCache, 100 * MIB, 1, 1);
    let plan = plan::build(&[c], &pc, &vols);
    assert!(action_eq(
        &plan.entries[0].action,
        &refuse(Refusal::NoRenameExcl)
    ));
}

// --- manifest round trip -----------------------------------------------------------

fn assert_plan_eq(a: &Plan, b: &Plan) {
    assert_eq!(a.plan_id, b.plan_id);
    assert_eq!(a.created_at, b.created_at, "created_at round trips exactly");
    assert_eq!(a.entries.len(), b.entries.len());
    for (x, y) in a.entries.iter().zip(&b.entries) {
        assert_eq!(x.cid, y.cid);
        assert_eq!(x.path, y.path);
        assert_eq!(x.path_hash, y.path_hash);
        assert_eq!(x.alloc_bytes, y.alloc_bytes);
        assert_eq!(x.grade, y.grade);
        assert!(action_eq(&x.action, &y.action));
        assert_eq!(x.evidence.len(), y.evidence.len());
        for (xe, ye) in x.evidence.iter().zip(&y.evidence) {
            assert!(evidence_eq(xe, ye));
        }
    }
    assert_eq!(a.totals.quarantine_bytes, b.totals.quarantine_bytes);
    assert_eq!(a.totals.quarantine_entries, b.totals.quarantine_entries);
    assert_eq!(a.totals.prune_bytes, b.totals.prune_bytes);
    assert_eq!(a.totals.prune_cmds, b.totals.prune_cmds);
    assert_eq!(a.totals.refused, b.totals.refused);
    assert_eq!(a.totals.snapshot_caveat, b.totals.snapshot_caveat);
    let (x, y) = (&a.policy_snapshot, &b.policy_snapshot);
    assert_eq!(x.target_bytes, y.target_bytes);
    assert_eq!(x.min_bytes, y.min_bytes);
    assert_eq!(x.allow_grades, y.allow_grades);
    assert_eq!(x.deny_globs, y.deny_globs);
    assert_eq!(x.deny_under_mta_secs, y.deny_under_mta_secs);
    assert_eq!(x.allow_network_regen, y.allow_network_regen);
    assert_eq!(x.allow_prune_commands, y.allow_prune_commands);
    assert_eq!(x.grace_hours, y.grace_hours);
    assert_eq!(x.max_entries, y.max_entries);
}

/// (d) write_manifest → load_manifest → deep-equal (field by field; the frozen
/// structs have no PartialEq).
#[test]
fn manifest_round_trip_deep_equal() {
    let pc = PolicyConstraintsShape::default();
    let vols = [vol(1)];
    let cands = vec![
        cand("C-001", Grade::PureCache, 100 * MIB, 1, 1),
        with_ev(
            cand("C-002", Grade::PureCache, 100 * MIB, 1, 2),
            EvidenceKind::MtimeHot,
        ),
    ];
    let plan = plan::build(&cands, &pc, &vols);
    assert_eq!(plan.totals.refused, 1);

    let dir = tempfile::Builder::new()
        .prefix("plan-roundtrip")
        .tempdir()
        .expect("tempdir");
    let path = dir.path().join("plan.json");
    plan::write_manifest(&plan, &path).expect("write manifest");
    assert!(path.exists(), "manifest file written");

    let loaded = plan::load_manifest(&path).expect("load manifest");
    assert_plan_eq(&plan, &loaded);
}

// --- summarize ----------------------------------------------------------------------

/// (e) exact human-line shape; caveat appended when present; prune clause when used.
#[test]
fn summarize_shape() {
    let plan = Plan {
        plan_id: "p".into(),
        created_at: chrono::Utc::now(),
        entries: Vec::new(),
        totals: PlanTotals {
            quarantine_bytes: 200 * MIB, // 0.1953125 GiB → "0.2"
            quarantine_entries: 2,
            prune_bytes: 0,
            prune_cmds: 0,
            refused: 1,
            snapshot_caveat: None,
        },
        policy_snapshot: PolicyConstraintsShape::default(),
    };
    assert_eq!(
        plan::summarize(&plan),
        "quarantine 0.2 GiB / 2 entries; refuse 1"
    );
}

#[test]
fn summarize_appends_prune_and_caveat_when_present() {
    let plan = Plan {
        plan_id: "p".into(),
        created_at: chrono::Utc::now(),
        entries: Vec::new(),
        totals: PlanTotals {
            quarantine_bytes: 200 * MIB,
            quarantine_entries: 2,
            prune_bytes: 3 * GIB / 2, // 1.5 GiB
            prune_cmds: 2,
            refused: 1,
            snapshot_caveat: Some("snapshots pinned".into()),
        },
        policy_snapshot: PolicyConstraintsShape::default(),
    };
    assert_eq!(
        plan::summarize(&plan),
        "quarantine 0.2 GiB / 2 entries; prune 1.5 GiB via 2 cmds; refuse 1; caveat: snapshots pinned"
    );
}

// --- policy compiler (§3.6) ---------------------------------------------------------

fn assert_default_constraints(pc: &PolicyConstraintsShape) {
    assert_eq!(pc.target_bytes, None);
    assert_eq!(pc.min_bytes, 50 * MIB);
    assert_eq!(
        pc.allow_grades,
        vec![
            Grade::PureCache,
            Grade::RebuildableFromSource,
            Grade::Redownloadable,
        ]
    );
    assert!(pc.deny_globs.is_empty());
    assert_eq!(pc.deny_under_mta_secs, 0);
    assert!(pc.allow_network_regen);
    assert!(pc.allow_prune_commands);
    assert_eq!(pc.grace_hours, 72);
    assert_eq!(pc.max_entries, 10_000);
}

/// (f) 腾出 25 GB + 别动我这两周在跑的项目 → target_bytes + deny_under_mta.
#[test]
fn policy_combines_target_and_mta_rules() {
    let c = policy::compile("腾出 25 GB，别动我这两周在跑的项目").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(25 * GIB));
    assert_eq!(c.constraints.deny_under_mta_secs, TWO_WEEKS_SECS);
    assert!(c.applied_rules.contains(&policy::RULE_TARGET_BYTES));
    assert!(c.applied_rules.contains(&policy::RULE_DENY_UNDER_MTA));
    assert!(c.unmatched.is_empty(), "no residue: {:?}", c.unmatched);
}

/// (f) 别动 Xcode 归档 → deny_globs entry expanded to the real Archives root.
#[test]
fn policy_xcode_archives_expands_glob() {
    let c = policy::compile("别动 Xcode 归档").expect("policy compiles");
    let home = dirs::home_dir().expect("home dir");
    let want = home
        .join("Library/Developer/Xcode/Archives")
        .to_string_lossy()
        .into_owned();
    assert_eq!(c.constraints.deny_globs, vec![want]);
    assert!(c.applied_rules.contains(&policy::RULE_DENY_GLOBS));
}

/// 别动 ~/Library/... → the tail becomes a glob with ~ expanded.
#[test]
fn policy_tilde_path_expands_glob() {
    let c = policy::compile("别动 ~/Library/Developer/Xcode/DerivedData").expect("policy compiles");
    let home = dirs::home_dir().expect("home dir");
    let want = home
        .join("Library/Developer/Xcode/DerivedData")
        .to_string_lossy()
        .into_owned();
    assert_eq!(c.constraints.deny_globs, vec![want]);
}

/// (f) unmatched free text lands in unmatched; constraints stay at defaults.
#[test]
fn policy_unmatched_text_lands_in_unmatched() {
    let c = policy::compile("随便清理点东西").expect("policy compiles");
    assert!(c.applied_rules.is_empty());
    assert_default_constraints(&c.constraints);
    assert_eq!(c.unmatched, vec!["随便清理点东西"]);
}

/// Partial match: rule fires on the first clause; the rest stays unmatched.
#[test]
fn policy_partial_match_leaves_residue() {
    let c = policy::compile("腾出 25 GB，顺便清理").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(25 * GIB));
    assert_eq!(c.unmatched, vec!["顺便清理"]);
}

/// (f) English 'free up 50 GB' works.
#[test]
fn policy_english_free_up_works() {
    let c = policy::compile("free up 50 GB").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(50 * GIB));
    assert!(c.applied_rules.contains(&policy::RULE_TARGET_BYTES));
}

/// CJK-spacing tolerant: 腾出25GB with no spaces.
#[test]
fn policy_no_space_cjk_target() {
    let c = policy::compile("腾出25GB").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(25 * GIB));
}

/// Every advertised unit sizes correctly (Bug 1): MB/TB and the `iB` spellings
/// used to fall through as silently-uncapped `None`, so 「腾出 500 MB」 selected
/// every candidate. Binary multiples throughout, matching the GiB accounting.
#[test]
fn policy_unit_alphabet_sizes_correctly() {
    for (sentence, want) in [
        ("腾出 500 MB", 500 * MIB),
        ("腾出 4MB", 4 * MIB),
        ("腾出 500 MB 空间", 500 * MIB),
        ("free up 500 MB", 500 * MIB),
        ("腾出 4 TB", 4 * TIB),
        ("腾出 4 GiB", 4 * GIB),
        ("腾出 2 kib", 2 * KIB),
        ("腾出 0.5 GB", GIB / 2),
    ] {
        let c = policy::compile(sentence).unwrap_or_else(|e| panic!("{sentence}: {e}"));
        assert_eq!(
            c.constraints.target_bytes,
            Some(want),
            "target_bytes for {sentence:?}"
        );
        assert!(c.applied_rules.contains(&policy::RULE_TARGET_BYTES));
    }
}

/// (Bug 1) A single-letter unit must end on a token boundary: 「腾出 25 goals」
/// used to read as 25 GiB because the old arm was `starts_with('g')`.
#[test]
fn policy_single_letter_unit_needs_token_boundary() {
    let err = policy::compile("腾出 25 goals").expect_err("goals is not giga");
    assert_eq!(err, policy::PolicyError::UnrecognizedUnit("goals".into()));
}

/// 别动最近 (no explicit 两周) also maps to deny_under_mta.
#[test]
fn policy_bie_dong_zui_jin_is_mta() {
    let c = policy::compile("别动最近在跑的东西").expect("policy compiles");
    assert_eq!(c.constraints.deny_under_mta_secs, TWO_WEEKS_SECS);
    assert!(c.applied_rules.contains(&policy::RULE_DENY_UNDER_MTA));
}

/// Two 别动 clauses: one mta, one glob — both rules fire, neither double-read.
#[test]
fn policy_two_bie_dong_clauses_both_rules_fire() {
    let c = policy::compile("别动最近在跑的项目，别动 Xcode 归档").expect("policy compiles");
    assert_eq!(c.constraints.deny_under_mta_secs, TWO_WEEKS_SECS);
    assert_eq!(c.constraints.deny_globs.len(), 1);
    assert!(c.applied_rules.contains(&policy::RULE_DENY_UNDER_MTA));
    assert!(c.applied_rules.contains(&policy::RULE_DENY_GLOBS));
}

/// (D1) Space-glued clause (no punctuation): the target span is consumed by Rule 1,
/// yet Rule 3 still protects the tail — target_bytes AND deny_globs both fire.
#[test]
fn policy_space_glued_clause_target_and_deny_globs_both_fire() {
    let c = policy::compile("腾出 25 GB 别动 Xcode 归档").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(25 * GIB));
    let home = dirs::home_dir().expect("home dir");
    let want = home
        .join("Library/Developer/Xcode/Archives")
        .to_string_lossy()
        .into_owned();
    assert_eq!(c.constraints.deny_globs, vec![want]);
    assert!(c.applied_rules.contains(&policy::RULE_TARGET_BYTES));
    assert!(c.applied_rules.contains(&policy::RULE_DENY_GLOBS));
    assert!(c.unmatched.is_empty(), "no residue: {:?}", c.unmatched);
}

/// (D1) Space-glued MTA clause: Rule 2 fires before Rule 3, so the fragment is not
/// double-read as a glob.
#[test]
fn policy_space_glued_mta_clause_not_double_read() {
    let c = policy::compile("腾出 25 GB 别动最近的项目").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(25 * GIB));
    assert_eq!(c.constraints.deny_under_mta_secs, TWO_WEEKS_SECS);
    assert!(
        c.constraints.deny_globs.is_empty(),
        "no glob from an MTA fragment"
    );
    assert!(c.applied_rules.contains(&policy::RULE_TARGET_BYTES));
    assert!(c.applied_rules.contains(&policy::RULE_DENY_UNDER_MTA));
    assert!(c.unmatched.is_empty(), "no residue: {:?}", c.unmatched);
}

/// (D2) A target verb with a number it cannot size: Chinese numerals carry no
/// digits, so they stay soft residue (the NL compiler's normal mode). A number
/// with an *unknown unit word* is different — it is a hard error, because
/// silently dropping the byte target removes the selection cap.
#[test]
fn policy_unmatched_reports_unrecognized_fragment() {
    let c = policy::compile("腾出 五十 GB").expect("policy compiles");
    assert!(c.applied_rules.is_empty());
    assert_default_constraints(&c.constraints);
    assert_eq!(c.unmatched, vec!["腾出 五十 GB"]);

    // A bare number with no unit at all stays soft too.
    let c2 = policy::compile("腾出 25 个文件").expect("policy compiles");
    assert_default_constraints(&c2.constraints);
    assert_eq!(c2.unmatched, vec!["腾出 25 个文件"]);

    let err = policy::compile("腾出 500 XB").expect_err("unknown unit is fatal");
    assert_eq!(err, policy::PolicyError::UnrecognizedUnit("xb".into()));
}

/// (D3) 「这两周」 alone is a description, not a protection: no MTA rule without
/// the protective verb 别动; the fragment stays unmatched.
#[test]
fn policy_two_weeks_alone_no_mta_rule() {
    let c = policy::compile("腾出 25 GB，这两周").expect("policy compiles");
    assert_eq!(c.constraints.target_bytes, Some(25 * GIB));
    assert_eq!(c.constraints.deny_under_mta_secs, 0);
    assert!(!c.applied_rules.contains(&policy::RULE_DENY_UNDER_MTA));
    assert_eq!(c.unmatched, vec!["这两周"]);
}

/// (D3 positive) 别动最近… carries the protective verb → MTA fires.
#[test]
fn policy_bie_dong_zui_jin_fires_mta() {
    let c = policy::compile("别动最近的项目").expect("policy compiles");
    assert_eq!(c.constraints.deny_under_mta_secs, TWO_WEEKS_SECS);
    assert!(c.applied_rules.contains(&policy::RULE_DENY_UNDER_MTA));
    assert!(
        c.constraints.deny_globs.is_empty(),
        "MTA fragment must not become a glob"
    );
}
