//! `execute/` — WAL-guarded quarantine / purge / restore + crash recovery
//! (SPEC Part B §3.4, §5). macOS-only: the syscall substrate lives in
//! `execute::macos` (renamex_np / chflags / lstat probes / dir-fd syncs).
//!
//! Protocol per entry (§5.2): WAL intent (per-volume store) → sync → pre-checks
//! (TOCTOU armor) → `renamex_np(RENAME_EXCL|RENAME_NOFOLLOW_ANY)` → WAL result →
//! home-ledger `apply` rec; op-group end → parent-dir sync. Purge requires the
//! typed byte count to equal the plan's quarantine total — mismatch refuses with
//! zero side effects (P4). Restore replays the per-volume WAL and reverses
//! intent/result pairs; purge `result`s (`from: None`) are never restored.

#![cfg(target_os = "macos")]

pub mod macos;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Duration as ChronoDuration, Utc};
use crossbeam_channel::Sender;

use crate::ledger::{self, Kind, LedgerRecord, Record, Store, Undo};
use crate::{PathHash, PlanAction, PlanEntry, VolumeInfo, WalEntry, WalKind};

/// Quarantine grace period before an entry surfaces as a purge candidate (SPEC
/// Part A §7 provisional: 72 h; mirrored by `config.grace_hours` in M4).
pub const GRACE_HOURS: i64 = 72;

/// Progress/result events streamed to the UI/CLI consumer (SPEC §3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecEvent {
    EntryApplied { cid: String, to: String },
    EntryPurged { cid: String, bytes: u64 },
    EntryRestored { cid: String, to: String },
    Error { cid: String, err: String },
    Done,
}

/// Crash-injection points (SPEC D-15): each fires ONLY when `kill_at` is passed
/// or the `KILL_AT` env var names it; the process exits 137 at the named point,
/// AFTER the sync step that precedes it, exactly like a kill -9 there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillPoint {
    /// Before the first WAL intent sync — nothing durable.
    BeforeWalSync,
    /// Intent synced, rename not yet executed.
    AfterWalSyncBeforeRename,
    /// Rename executed, WAL result not yet written.
    AfterRenameBeforeResult,
    /// After one entry's result, before the next entry's intent.
    MidBatch,
    /// After one entry's full purge (unlink + WAL result + ledger rec),
    /// before the next entry's unlink.
    MidPurge,
}

impl KillPoint {
    pub fn parse(s: &str) -> Option<KillPoint> {
        match s {
            "BeforeWalSync" | "before_wal_sync" => Some(KillPoint::BeforeWalSync),
            "AfterWalSyncBeforeRename" | "after_wal_sync_before_rename" => {
                Some(KillPoint::AfterWalSyncBeforeRename)
            }
            "AfterRenameBeforeResult" | "after_rename_before_result" => {
                Some(KillPoint::AfterRenameBeforeResult)
            }
            "MidBatch" | "mid_batch" => Some(KillPoint::MidBatch),
            "MidPurge" | "mid_purge" => Some(KillPoint::MidPurge),
            _ => None,
        }
    }
}

/// Per-volume recovery findings.
#[derive(Debug, Clone)]
pub struct DanglingIntent {
    pub op_id: String,
    pub entry: PathHash,
    pub from: Option<PathBuf>,
    pub to: Option<PathBuf>,
}

/// A quarantined entry whose grace window has expired — surfaced, NEVER auto-purged.
#[derive(Debug, Clone)]
pub struct ExpiredQuarantine {
    pub op_id: String,
    pub original_path: PathBuf,
    pub quarantine_path: PathBuf,
    pub bytes: u64,
    pub grace_expires_at: chrono::DateTime<chrono::Utc>,
}

/// Crash-recovery report (SPEC §5.2 recovery sequence). NOTE: this is the
/// execute-level report; `ledger::RecoveryReport` is the raw replay accounting.
#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub volumes_checked: u32,
    /// Intents whose quarantine target does not exist — tail dropped, source intact.
    pub dangling_intents: Vec<DanglingIntent>,
    /// Intents whose quarantine target exists — WAL result appended (roll forward).
    pub rolled_forward: u32,
    /// Committed quarantines past their grace window.
    pub expired_grace: Vec<ExpiredQuarantine>,
    /// Replay accounting carried from the home ledger (ADR-4, D-18).
    pub records: u64,
    pub skipped_corrupt: u64,
    pub skipped_unknown: u64,
    pub seq_gaps: Vec<u64>,
    pub torn_tail: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct ApplyReport {
    pub op_id: String,
    pub applied: u32,
    /// Entries already moved (source ENOENT) or non-quarantine plan actions.
    pub skipped: u32,
    pub refused: u32,
    pub quarantined_bytes: u64,
}

#[derive(Debug, Default)]
pub struct PurgeReport {
    pub freed_bytes: u64,
    pub entries: u32,
}

#[derive(Debug, Default)]
pub struct RestoreReport {
    pub restored: Vec<String>,
    /// Quarantine paths that no longer exist (already purged) — benign skip.
    pub missing: Vec<String>,
    /// Entries restored to `original-<n>` because the original path was
    /// occupied (D-26 keep-both default) — the user's file was never clobbered.
    pub collided: Vec<String>,
}

/// The execute engine. Holds one plan's entries plus the home ledger store.
pub struct Executor {
    plan_id: String,
    entries: Vec<PlanEntry>,
    vols: Vec<VolumeInfo>,
    ledger: Store,
}

/// Open an executor for `entries` of `plan_id`. `vols` is the scan-time volume
/// table used for per-volume quarantine roots; `ledger` is the HOME ledger store
/// (`~/.disk-janitor`) — the per-volume WALs are separate `ledger::Store`
/// instances opened on each quarantine root.
pub fn open(
    plan_id: String,
    entries: Vec<PlanEntry>,
    vols: &[VolumeInfo],
    ledger: Store,
) -> Result<Executor> {
    Ok(Executor {
        plan_id,
        entries,
        vols: vols.to_vec(),
        ledger,
    })
}

/// Home ledger directory: `$DISK_JANITOR_HOME` override, else `~/.disk-janitor`.
pub fn home_ledger_dir() -> PathBuf {
    if let Some(h) = std::env::var_os("DISK_JANITOR_HOME") {
        return PathBuf::from(h);
    }
    dirs::home_dir()
        .map(|h| h.join(".disk-janitor"))
        .unwrap_or_else(|| PathBuf::from(".disk-janitor"))
}

/// CLI-facing fallible variant of [`home_ledger_dir`] (the `?`-compatible form
/// the janitor binary uses for its command plumbing).
pub fn ledger_home() -> Result<PathBuf> {
    Ok(home_ledger_dir())
}

fn parse_kill_env() -> Option<KillPoint> {
    std::env::var("KILL_AT")
        .ok()
        .and_then(|v| KillPoint::parse(&v))
}

fn maybe_kill(kill: Option<KillPoint>, point: KillPoint) {
    if kill == Some(point) {
        std::process::exit(137);
    }
}

/// All wal-shaped records for a quarantine root, merged from `wal.jsonl` and
/// the checkpoint-folded `ledger.jsonl` (after a checkpoint wal.jsonl is empty
/// and the records live folded in the ledger — LedgerStore contract). Both are
/// parsed via `ledger::record_iter` (CRC-validated skip-and-continue). Returns
/// (records, raw_line_count, skipped_line_count).
fn wal_entries_for_root(root: &Path) -> (Vec<WalEntry>, u64, u64) {
    let mut out = Vec::new();
    let mut raw = 0u64;
    for f in [root.join("wal.jsonl"), root.join("ledger.jsonl")] {
        if !f.exists() {
            continue;
        }
        if let Ok(data) = fs::read(&f) {
            raw += data
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .count() as u64;
        }
        out.extend(ledger::record_iter(&f).into_iter().map(|(_, w)| w));
    }
    let skipped = raw.saturating_sub(out.len() as u64);
    (out, raw, skipped)
}

/// True when the WAL file ends mid-line (crash during an append). Detected
/// before `sanitize_wal_tail` truncates, so recovery can report it (D-18).
pub fn wal_has_torn_tail(path: &Path) -> bool {
    fs::read(path)
        .map(|d| !d.is_empty() && d.last() != Some(&b'\n'))
        .unwrap_or(false)
}

/// Truncate a torn WAL tail (partial final line after a crash mid-append) and
/// park the bytes at `<wal>.torn`, mirroring the ledger's torn-tail handling
/// (D-18). Call before appending to a per-volume WAL; no-op on clean files.
pub fn sanitize_wal_tail(path: &Path) -> Result<()> {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).context("read WAL for tail sanitize"),
    };
    if data.is_empty() || data.last() == Some(&b'\n') {
        return Ok(());
    }
    let cut = data
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let mut torn_name = path.as_os_str().as_encoded_bytes().to_vec();
    torn_name.extend_from_slice(b".torn");
    use std::os::unix::ffi::OsStringExt;
    fs::write(
        PathBuf::from(std::ffi::OsString::from_vec(torn_name)),
        &data[cut..],
    )?;
    let f = fs::OpenOptions::new().write(true).open(path)?;
    f.set_len(cut as u64)?;
    f.sync_all()?;
    Ok(())
}

fn quarantine_root_for(vol: &VolumeInfo) -> PathBuf {
    vol.mount.join(macos::QUARANTINE_ROOT_NAME)
}

/// Resolve the per-volume quarantine root for a SOURCE path (SPEC §5.1:
/// `<mnt>/.Trash-janitor/`, same-volume by st_dev). On the boot Data volume the
/// mount root is SIP-locked, so fall back to the topmost WRITABLE same-dev
/// ancestor of the source — the quarantine stays on-volume and atomic; the WAL
/// lives next to it, so crash recovery still works.
fn quarantine_root_for_source(src: &Path, vol: &VolumeInfo) -> PathBuf {
    let primary = quarantine_root_for(vol);
    if ensure_root(&primary).is_ok() {
        return primary;
    }
    let src = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    let mut best: Option<PathBuf> = None;
    let mut cur: Option<&std::path::Path> = Some(src.as_path());
    while let Some(p) = cur {
        if macos::lstat_dev(p).ok() == Some(vol.dev) && is_dir_writable(p) {
            best = Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    let anchor = best.unwrap_or_else(|| vol.mount.clone());
    anchor.join(macos::QUARANTINE_ROOT_NAME)
}

/// Quarantine-root discovery probe list (SPEC §5.1/§5.2 — DOCUMENTED):
///
/// 1. `<mount>/.Trash-janitor` — the standard per-volume primary root;
/// 2. canonicalized `<mount>/.Trash-janitor` — `/tmp` and `/private/tmp` are
///    one directory with two spellings, so both must reconcile once;
/// 3. `<mount>/private/tmp/.Trash-janitor` — the SIP-fallback anchor for
///    `/tmp` sources: the topmost WRITABLE same-dev ancestor of a
///    `/private/tmp/…` source is `/private/tmp` itself (1777);
/// 4. `<ancestor>/.Trash-janitor` for EVERY direct ancestor of the home
///    ledger dir and of each candidate root from 1-3 plus `extra` — this
///    covers the boot-volume anchor `~/.Trash-janitor` (topmost writable
///    same-dev ancestor of home-dir sources) and any deeper anchor, even when
///    a crash between rename and ledger append left NO record referencing it.
///
/// `extra` = roots derived from ledger records (each apply rec's quarantine
/// path's grandparent). "Keep it cheap": a candidate counts only when
/// `wal.jsonl` or `ledger.jsonl` exists inside it (existence probe, never a
/// directory walk) — the caller re-checks before opening. Results are deduped
/// by FILESYSTEM IDENTITY and reported under their canonical spelling, so the
/// two spellings of one directory reconcile exactly once.
fn discover_quarantine_roots(
    vols: &[VolumeInfo],
    extra: impl Iterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vols.iter().map(quarantine_root_for).collect();
    roots.extend(extra);
    // Chains walked in probe 4: every volume mount, plus the home ledger dir.
    let mut chains: Vec<PathBuf> = vols.iter().map(|v| v.mount.clone()).collect();
    for vol in vols {
        if let Ok(canonical) = vol.mount.canonicalize() {
            roots.push(canonical.join(macos::QUARANTINE_ROOT_NAME));
            chains.push(canonical);
        }
        roots.push(
            vol.mount
                .join("private/tmp")
                .join(macos::QUARANTINE_ROOT_NAME),
        );
    }
    chains.push(home_ledger_dir());
    chains.extend(roots.iter().cloned());
    for anchor in &chains {
        let mut cur = Some(anchor.as_path());
        while let Some(p) = cur {
            roots.push(p.join(macos::QUARANTINE_ROOT_NAME));
            cur = p.parent();
        }
    }
    // Dedupe by FILESYSTEM IDENTITY, not path string (Bug 3). `canonicalize()`
    // resolves symlinks but NOT APFS firmlinks: `/private/tmp/x` and
    // `/System/Volumes/Data/private/tmp/x` canonicalize to different strings
    // while being the same directory (same st_dev + st_ino). String dedupe let
    // both survive, so every root-iterating pass — restore's pair loop and
    // recover's WAL reconcile — processed one op twice: restore counted N
    // phantoms in `missing`, because pass 1 moved the files back and pass 2
    // found them gone. Identity is the invariant; the canonical string is only
    // the fallback for a root that does not exist yet (stat fails).
    let mut seen_ident: HashSet<(u64, u64)> = HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen_str: HashSet<PathBuf> = HashSet::new();
    for root in roots {
        match macos::lstat_identity(&root) {
            Ok(id) => {
                if seen_ident.insert(id) {
                    // Report the canonical spelling (the pre-Bug-3 contract: one
                    // stable path per directory); identity alone picks WHICH.
                    out.push(root.canonicalize().unwrap_or(root));
                }
            }
            // Unstattable root (not yet created, or unreadable): fall back to the
            // canonical string so a genuine duplicate spelling still collapses.
            Err(_) => {
                let canon = root.canonicalize().unwrap_or_else(|_| root.clone());
                if seen_str.insert(canon) {
                    out.push(root);
                }
            }
        }
    }
    out.sort();
    out
}

fn is_dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".janitor-ro-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Ensure `<mnt>/.Trash-janitor/` exists, mode 0700, UF_HIDDEN set (SPEC §5.1).
fn ensure_root(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(md) if md.is_dir() => {}
        Ok(_) => bail!(
            "quarantine root {} exists and is not a directory",
            root.display()
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root)
                .with_context(|| format!("mkdir {}", root.display()))?;
            macos::set_hidden(root)
                .with_context(|| format!("chflags UF_HIDDEN {}", root.display()))?;
        }
        Err(e) => return Err(e).context("stat quarantine root"),
    }
    Ok(())
}

/// Pre-checks at apply time (TOCTOU armor, SPEC §3.4 table): same device both
/// ends, flag re-probe (UF_IMMUTABLE / SF_RESTRICTED / SF_NOUNLINK), dataless
/// refusal, read-only / no-EXCL volume refusal.
fn precheck(from: &Path, op_dir: &Path, vol: &VolumeInfo) -> std::result::Result<(), String> {
    if vol.read_only {
        return Err("volume is read-only".into());
    }
    if !vol.rename_excl {
        return Err("volume lacks RENAME_EXCL support".into());
    }
    let src_dev = macos::lstat_dev(from).map_err(|e| format!("lstat source: {e}"))?;
    let dst_dev = macos::lstat_dev(op_dir).map_err(|e| format!("lstat op dir: {e}"))?;
    if src_dev != dst_dev {
        return Err("cross-volume move (EXDEV)".into());
    }
    let flags = macos::lstat_flags(from).map_err(|e| format!("lstat flags: {e}"))?;
    if flags & (macos::UF_IMMUTABLE | macos::SF_RESTRICTED | macos::SF_NOUNLINK) != 0 {
        return Err("flag-blocked (UF_IMMUTABLE/SF_RESTRICTED/SF_NOUNLINK)".into());
    }
    if flags & macos::SF_DATALESS != 0 {
        return Err("dataless file".into());
    }
    Ok(())
}

/// Advisory target pick: first free `base`, `base-1`, `base-2`, … name in the op
/// dir (lstat probe). The intent records this path; `rename_with_retry` is the
/// authoritative existence check at rename time.
pub(crate) fn pick_free_target(op_dir: &Path, base: &std::ffi::OsStr) -> Result<PathBuf> {
    for n in 0..=100u32 {
        let name = if n == 0 {
            base.to_os_string()
        } else {
            // OsString push keeps arbitrary bytes intact (a to_string_lossy
            // suffix would mangle non-UTF8 basenames mid-path).
            let mut name = base.to_os_string();
            name.push(format!("-{n}"));
            name
        };
        let to = op_dir.join(&name);
        match fs::symlink_metadata(&to) {
            Ok(_) => continue,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(to),
            Err(e) => return Err(e).context("probe quarantine target"),
        }
    }
    bail!(
        "quarantine target suffix space exhausted for {}",
        base.to_string_lossy()
    )
}

/// `renamex_np` with the `-<n>` EEXIST retry loop: the syscall is the existence
/// check (no probe-then-rename TOCTOU). Returns the final target path.
pub(crate) fn rename_with_retry(
    from: &Path,
    op_dir: &Path,
    base: &std::ffi::OsStr,
) -> Result<PathBuf> {
    for n in 0..=100u32 {
        let name = if n == 0 {
            base.to_os_string()
        } else {
            let mut name = base.to_os_string();
            name.push(format!("-{n}"));
            name
        };
        let to = op_dir.join(name);
        match macos::rename_no_replace(from, &to) {
            Ok(()) => return Ok(to),
            Err(e) if e.raw_os_error() == Some(libc::EEXIST) => continue,
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("renamex_np {} -> {}", from.display(), to.display()));
            }
        }
    }
    bail!(
        "rename target suffix space exhausted for {}",
        from.display()
    )
}

/// D-26 keep-both restore: move `from` to `to`, or `to-1`, `to-2`, … when the
/// target is occupied — the syscall (`RENAME_EXCL`) is the existence check, so
/// an occupied original path is NEVER clobbered. Returns the final target.
/// `Err(NotFound)` means the SOURCE (quarantine) is gone — already
/// purged/restored; the caller maps that to a benign missing.
pub(crate) fn restore_keep_both(from: &Path, to: &Path) -> io::Result<PathBuf> {
    let base = match to.file_name() {
        Some(b) => b.to_os_string(),
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "restore target has no file name",
            ));
        }
    };
    let parent = to.parent().unwrap_or(Path::new("/"));
    for n in 0..=100u32 {
        let name = if n == 0 {
            base.clone()
        } else {
            let mut name = base.clone();
            name.push(format!("-{n}"));
            name
        };
        let target = parent.join(name);
        match macos::rename_no_replace(from, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.raw_os_error() == Some(libc::EEXIST) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!(
        "restore suffix space exhausted for {}",
        to.display()
    )))
}

impl Executor {
    /// Apply the plan's quarantine entries (SPEC §5.2 sequence). Each entry:
    /// per-volume WAL intent → sync → pre-checks → renamex_np → WAL result →
    /// home-ledger `apply` rec; op-group end → per-volume `sync_boundary()`.
    ///
    /// Idempotent: a source that is already gone (ENOENT) is a benign skip; an
    /// EEXIST target is retried with a `-<n>` suffix. Refusals emit an `Error`
    /// event and leave a dangling intent (dropped by `recover`), per §5.2.
    pub fn apply(
        &mut self,
        tx: Sender<ExecEvent>,
        kill_at: Option<KillPoint>,
    ) -> Result<ApplyReport> {
        let kill = kill_at.or_else(parse_kill_env);
        let op_id = uuid::Uuid::new_v4().to_string();
        let mut report = ApplyReport {
            op_id: op_id.clone(),
            ..Default::default()
        };
        let quarantine_entries: Vec<&PlanEntry> = self
            .entries
            .iter()
            .filter(|e| matches!(e.action, PlanAction::Quarantine))
            .collect();

        // Per-volume WAL stores (opened once per volume; sync_boundary at end).
        let mut wal_stores: BTreeMap<u64, Store> = BTreeMap::new();
        let mut wal_devs: Vec<u64> = Vec::new();

        maybe_kill(kill, KillPoint::BeforeWalSync);

        let last_idx = quarantine_entries.len().saturating_sub(1);
        for (idx, entry) in quarantine_entries.iter().enumerate() {
            // The JSONL WAL/ledger record paths via serde PathBuf serialization,
            // which rejects invalid UTF-8 — refuse such entries cleanly per
            // entry (before any WAL/dir write for it) instead of failing the
            // whole batch mid-way with a serde error. Full support would need a
            // byte-encoded path format (lib.rs WalEntry — out of execute scope).
            if entry.path.to_str().is_none() {
                report.refused += 1;
                let _ = tx.send(ExecEvent::Error {
                    cid: entry.cid.clone(),
                    err: "entry path is not valid UTF-8 (cannot be recorded in the WAL)".into(),
                });
                continue;
            }
            // Source already moved (idempotent re-apply) — benign skip BEFORE any
            // WAL write or directory creation.
            let src_dev = match macos::lstat_dev(&entry.path) {
                Ok(d) => d,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    report.skipped += 1;
                    continue;
                }
                Err(e) => {
                    report.refused += 1;
                    let _ = tx.send(ExecEvent::Error {
                        cid: entry.cid.clone(),
                        err: e.to_string(),
                    });
                    continue;
                }
            };
            // Volume match by st_dev (SPEC §5.1: same-volume is the real precondition;
            // [05 §2] EXDEV raised even across APFS volumes in one container). Path
            // prefix matching fails on macOS firmlink/symlink roots (/tmp → /private/tmp;
            // /tmp files live on the Data slice with a DIFFERENT mount string).
            let vol = match self.vols.iter().find(|v| v.dev == src_dev) {
                Some(v) => v,
                None => {
                    report.refused += 1;
                    let _ = tx.send(ExecEvent::Error {
                        cid: entry.cid.clone(),
                        err: format!("no scanned volume for {}", entry.path.display()),
                    });
                    continue;
                }
            };
            let root = quarantine_root_for_source(&entry.path, vol);
            if let Err(e) = ensure_root(&root) {
                report.refused += 1;
                let _ = tx.send(ExecEvent::Error {
                    cid: entry.cid.clone(),
                    err: e.to_string(),
                });
                continue;
            }
            let wal_store = match wal_stores.get_mut(&src_dev) {
                Some(w) => w,
                None => {
                    wal_devs.push(src_dev);
                    // A previous crashed op may have left a torn tail; truncate
                    // it before appending so new records land on clean lines.
                    sanitize_wal_tail(&root.join("wal.jsonl")).ok();
                    wal_stores.insert(
                        src_dev,
                        ledger::open(&root).with_context(|| {
                            format!("open per-volume WAL at {}", root.display())
                        })?,
                    );
                    wal_stores.get_mut(&src_dev).expect("just inserted")
                }
            };
            // WAL intent + sync (wal_intent syncs internally; std maps to F_FULLFSYNC on Apple).
            let base = match entry.path.file_name() {
                Some(b) => b.to_os_string(),
                None => {
                    report.refused += 1;
                    let _ = tx.send(ExecEvent::Error {
                        cid: entry.cid.clone(),
                        err: "entry path has no file name".into(),
                    });
                    continue;
                }
            };
            let op_dir = root.join(&op_id);
            fs::DirBuilder::new()
                .recursive(true)
                .create(&op_dir)
                .with_context(|| format!("mkdir op dir {}", op_dir.display()))?;
            macos::set_hidden(&op_dir).ok(); // op dir hidden too; non-fatal if flags unsupported
            // Advisory suffix pick (lstat probe) so the intent names the concrete
            // target recovery will check; the rename below still re-probes with
            // the syscall itself as the existence check.
            let planned = pick_free_target(&op_dir, &base)?;
            let intent = WalEntry {
                op_id: op_id.clone(),
                kind: WalKind::Intent,
                plan_id: self.plan_id.clone(),
                entry: entry.path_hash,
                from: Some(entry.path.clone()),
                to: Some(planned),
                bytes: entry.alloc_bytes,
                undo_of: None,
            };
            wal_store.wal_intent(&intent)?;
            maybe_kill(kill, KillPoint::AfterWalSyncBeforeRename);

            // TOCTOU armor before the mutation.
            if let Err(reason) = precheck(&entry.path, &op_dir, vol) {
                report.refused += 1;
                let _ = tx.send(ExecEvent::Error {
                    cid: entry.cid.clone(),
                    err: reason,
                });
                continue;
            }
            let to = rename_with_retry(&entry.path, &op_dir, &base)?;
            maybe_kill(kill, KillPoint::AfterRenameBeforeResult);

            let result = WalEntry {
                op_id: op_id.clone(),
                kind: WalKind::Result,
                plan_id: self.plan_id.clone(),
                entry: entry.path_hash,
                from: Some(entry.path.clone()),
                to: Some(to.clone()),
                bytes: entry.alloc_bytes,
                undo_of: None,
            };
            wal_store.wal_intent(&result)?;

            let undo = Undo {
                original_path: entry.path.clone(),
                byte_count: entry.alloc_bytes,
                grace_expires_at: Utc::now() + ChronoDuration::hours(GRACE_HOURS),
            };
            self.ledger.append(
                Kind::Apply,
                &LedgerRecord::Apply {
                    plan_id: self.plan_id.clone(),
                    entry: (*entry).clone(),
                    original_path: entry.path.clone(),
                    quarantine_path: to.clone(),
                    renamex_np: true,
                    undo,
                },
            )?;
            report.applied += 1;
            report.quarantined_bytes += entry.alloc_bytes;
            let _ = tx.send(ExecEvent::EntryApplied {
                cid: entry.cid.clone(),
                to: to.display().to_string(),
            });
            if kill == Some(KillPoint::MidBatch) && idx != last_idx {
                std::process::exit(137);
            }
        }

        for dev in &wal_devs {
            wal_stores
                .remove(dev)
                .expect("store present")
                .sync_boundary()?;
        }
        let _ = tx.send(ExecEvent::Done);
        Ok(report)
    }

    /// Purge the plan's quarantined entries. `typed_bytes` MUST equal the plan's
    /// quarantine total — the DEDUPED per-(dev,ino) sum over quarantine entries,
    /// mirroring plan::build's `seen_inodes` rule exactly, i.e. the number the
    /// plan displays (B1: a hardlinked pair counts once, not twice). Any
    /// mismatch refuses with zero side effects (P4, SPEC §5.3). Purge runs to
    /// completion (no cancel checks, D-15): per entry recursive unlink, WAL
    /// result (`from: None` — never restorable), home-ledger `purge` rec.
    pub fn purge(&mut self, typed_bytes: u64, tx: Sender<ExecEvent>) -> Result<PurgeReport> {
        // Same (dev,ino) rule as plan::build (SPEC §3.3): an inode can only be
        // reclaimed once. Entries without an inode key (pre-field manifests)
        // are each their own group — legacy Σ semantics, never collapsed.
        let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
        let expected: u64 = self
            .entries
            .iter()
            .filter(|e| matches!(e.action, PlanAction::Quarantine))
            .filter(|e| e.inode.is_none_or(|ino| seen_inodes.insert(ino)))
            .map(|e| e.alloc_bytes)
            .sum();
        if typed_bytes != expected {
            bail!(
                "typed byte count {typed_bytes} does not match quarantine total {expected} — nothing purged"
            );
        }

        let home = home_ledger_dir();
        let ledger_path = home.join("ledger.jsonl");
        let mut report = PurgeReport::default();
        let mut per_vol_wal: BTreeMap<PathBuf, Store> = BTreeMap::new();
        let kill = parse_kill_env();
        if ledger_path.exists() {
            let (records, _) = ledger::replay(&ledger_path)?;
            let apply_recs: Vec<&Record> =
                records.iter().filter(|r| r.kind == Kind::Apply).collect();
            let last_idx = apply_recs.len().saturating_sub(1);
            let mut purged = 0u32;
            // FRESH set (the gate's set is spent above): ledger order may
            // differ from plan order, and the first link seen here must free
            // the inode's bytes.
            let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
            for (idx, rec) in apply_recs.iter().enumerate() {
                let apply = match &rec.rec {
                    LedgerRecord::Apply {
                        plan_id,
                        entry,
                        quarantine_path,
                        ..
                    } if *plan_id == self.plan_id => (entry.clone(), quarantine_path.clone()),
                    _ => continue,
                };
                let (entry, qpath) = apply;
                let op_dir = match qpath.parent() {
                    Some(d) => d.to_path_buf(),
                    None => continue,
                };
                let root = match op_dir.parent() {
                    Some(r) => r.to_path_buf(),
                    None => continue,
                };
                if !qpath.exists() {
                    continue; // already purged/restored — idempotent
                }
                // Honest accounting (B1): one inode frees its bytes exactly
                // once — the second hardlink's unlink frees 0. The dedup set
                // mirrors plan::build / the gate above (None = own group).
                let freed = if entry.inode.is_none_or(|ino| seen_inodes.insert(ino)) {
                    entry.alloc_bytes
                } else {
                    0
                };
                macos::remove_all(&qpath).with_context(|| format!("unlink {}", qpath.display()))?;
                report.freed_bytes += freed;
                report.entries += 1;

                let wal = match per_vol_wal.get_mut(&root) {
                    Some(w) => w,
                    None => {
                        sanitize_wal_tail(&root.join("wal.jsonl")).ok();
                        per_vol_wal.insert(root.clone(), ledger::open(&root)?);
                        per_vol_wal.get_mut(&root).expect("just inserted")
                    }
                };
                // WAL result per purged entry: FRESH op_id — an orphan Result
                // (from:None/to:None = not restorable) so checkpoint dedupe by
                // op_id can never collapse the apply intent/result pair.
                wal.wal_intent(&WalEntry {
                    op_id: uuid::Uuid::new_v4().to_string(),
                    kind: WalKind::Result,
                    plan_id: self.plan_id.clone(),
                    entry: entry.path_hash,
                    from: None,
                    to: None,
                    bytes: freed,
                    undo_of: None,
                })?;
                let cid = entry.cid.clone();
                self.ledger.append(
                    Kind::Purge,
                    &LedgerRecord::Purge {
                        plan_id: self.plan_id.clone(),
                        entry,
                        bytes_freed: freed,
                        quarantine_path: qpath,
                    },
                )?;
                let _ = tx.send(ExecEvent::EntryPurged { cid, bytes: freed });
                purged += 1;
                // D-15 crash point: after one entry's COMPLETE purge (unlink +
                // WAL result + ledger rec), before the next entry's unlink.
                if kill == Some(KillPoint::MidPurge) && purged == 1 && idx != last_idx {
                    std::process::exit(137);
                }
            }
        }
        // Remove op dirs that became empty.
        for root in per_vol_wal.keys() {
            if let Ok(rd) = fs::read_dir(root) {
                for ent in rd.flatten() {
                    if ent.path().is_dir()
                        && fs::read_dir(ent.path())
                            .map(|mut d| d.next().is_none())
                            .unwrap_or(false)
                    {
                        let _ = fs::remove_dir(ent.path());
                    }
                }
            }
        }
        let _ = tx.send(ExecEvent::Done);
        Ok(report)
    }

    /// Restore one op: replay the per-volume WALs, reverse the rename for every
    /// intent/result pair of `op_id` whose quarantine still exists (ENOENT =
    /// already purged → reported missing, benign). A deleted ORIGINAL parent
    /// dir is recreated (restore is the user's explicit undo; only a missing
    /// QUARANTINE path is benign). Appends a home-ledger `restore` rec per
    /// entry with `undoes` = the apply record's seq.
    pub fn restore(&mut self, op_id: &str, tx: Sender<ExecEvent>) -> Result<RestoreReport> {
        let mut report = RestoreReport::default();
        let home = home_ledger_dir();
        let ledger_path = home.join("ledger.jsonl");
        // apply recs keyed by quarantine path (needed for cid/plan_id/undoes).
        let mut apply_by_qpath: BTreeMap<PathBuf, (u64, String, PlanEntry)> = BTreeMap::new();
        if ledger_path.exists() {
            let (records, _) = ledger::replay(&ledger_path)?;
            for rec in &records {
                if let LedgerRecord::Apply {
                    plan_id,
                    entry,
                    quarantine_path,
                    ..
                } = &rec.rec
                {
                    apply_by_qpath.insert(
                        quarantine_path.clone(),
                        (rec.seq, plan_id.clone(), entry.clone()),
                    );
                }
            }
        }

        // Root discovery: ledger apply recs are the source of truth for where
        // entries actually live (the apply path may anchor on a same-dev
        // writable ancestor, e.g. /private/tmp, that is NOT any vol.mount
        // string), but a crashed apply whose ledger rec never landed is only
        // reachable through the fallback-anchor probe list — see
        // `discover_quarantine_roots`.
        let roots = discover_quarantine_roots(
            &self.vols,
            apply_by_qpath
                .keys()
                .filter_map(|q| q.parent().and_then(|p| p.parent()).map(|r| r.to_path_buf())),
        );
        for root in &roots {
            let root: &PathBuf = root;
            let wal_path = root.join("wal.jsonl");
            if !wal_path.exists() && !root.join("ledger.jsonl").exists() {
                continue;
            }
            sanitize_wal_tail(&wal_path)?;
            // After a checkpoint the WAL is folded into this dir's ledger.jsonl
            // and wal.jsonl is empty — read both (LedgerStore contract).
            let (entries, _, _) = wal_entries_for_root(root);
            // intent/result pairs for this op (purge results have from:None and pair with nothing).
            let mut pairs: Vec<(WalEntry, WalEntry)> = Vec::new();
            for (i, w) in entries.iter().enumerate() {
                if w.kind != WalKind::Intent || w.op_id != op_id {
                    continue;
                }
                if let Some(r) = entries[i + 1..].iter().find(|r| {
                    r.kind == WalKind::Result
                        && r.op_id == op_id
                        && r.entry == w.entry
                        && r.from.is_some()
                }) {
                    pairs.push((w.clone(), r.clone()));
                }
            }
            for (i, (intent, result)) in pairs.iter().enumerate() {
                let qpath = result.to.clone().or_else(|| intent.to.clone());
                let orig = result.from.clone().or_else(|| intent.from.clone());
                let (qpath, orig) = match (qpath, orig) {
                    (Some(q), Some(o)) => (q, o),
                    _ => continue,
                };
                let cid = apply_by_qpath
                    .get(&qpath)
                    .map(|(_, _, e)| e.cid.clone())
                    .unwrap_or_else(|| format!("op-{op_id}#{i}"));
                if !qpath.exists() {
                    report.missing.push(cid);
                    continue;
                }
                // C2: the original PARENT dir may have been deleted since the
                // quarantine — restore is the user's explicit undo, so recreate
                // the chain (an empty dir, never the file). Without this the
                // rename fails ENOENT and the file is stranded, misreported as
                // benign "missing" although the quarantine path is alive.
                if let Some(parent) = orig.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("recreate original parent {}", parent.display())
                    })?;
                }
                // D-26: default restore NEVER clobbers — an occupied original
                // path (user recreated the file) gets `original-<n>` instead.
                let target = match restore_keep_both(&qpath, &orig) {
                    Ok(t) => t,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        report.missing.push(cid);
                        continue;
                    }
                    Err(e) => {
                        return Err(e).with_context(|| {
                            format!("restore rename {} -> {}", qpath.display(), orig.display())
                        });
                    }
                };
                if target != orig {
                    report.collided.push(cid.clone());
                }
                // Durability is the home-ledger restore rec (undoes = apply seq);
                // the per-volume WAL is append-only and keeps the intent/result.
                if let Some((seq, plan_id, entry)) = apply_by_qpath.get(&qpath) {
                    self.ledger.append(
                        Kind::Restore,
                        &LedgerRecord::Restore {
                            plan_id: plan_id.clone(),
                            entry: entry.clone(),
                            from: qpath.clone(),
                            to: target.clone(),
                            undoes: *seq,
                        },
                    )?;
                }
                report.restored.push(cid.clone());
                let _ = tx.send(ExecEvent::EntryRestored {
                    cid,
                    to: target.display().to_string(),
                });
            }
            // Remove the op dir when empty (SPEC data-flow step 7).
            let op_dir = root.join(op_id);
            if fs::read_dir(&op_dir)
                .map(|mut d| d.next().is_none())
                .unwrap_or(false)
            {
                let _ = fs::remove_dir(&op_dir);
            }
        }
        let _ = tx.send(ExecEvent::Done);
        Ok(report)
    }
}

/// Crash recovery (SPEC §5.2): per volume, replay the per-volume WALs found at
/// every quarantine root on the `discover_quarantine_roots` probe list and
/// reconcile committed intents against FS state — quarantine target exists ⇒
/// roll forward (append the WAL result); intent without result and no target ⇒
/// drop tail (report, never touch user data). Also surfaces home-ledger apply
/// recs whose grace window has expired as purge candidates (never auto-purged).
pub fn recover(vols: &[VolumeInfo]) -> RecoveryReport {
    let mut rep = RecoveryReport::default();

    // Home ledger replay accounting + grace scan.
    let home = home_ledger_dir();
    let ledger_path = home.join("ledger.jsonl");
    let mut apply_recs: Vec<Record> = Vec::new();
    let mut purge_paths: Vec<PathBuf> = Vec::new();
    let mut restore_from: Vec<PathBuf> = Vec::new();
    if ledger_path.exists()
        && let Ok((records, rr)) = ledger::replay(&ledger_path)
    {
        rep.records = rr.records;
        rep.skipped_corrupt = rr.skipped_corrupt;
        rep.skipped_unknown = rr.skipped_unknown;
        rep.seq_gaps = rr.seq_gaps;
        rep.torn_tail = rr.torn_tail;
        for r in &records {
            match &r.rec {
                LedgerRecord::Apply { .. } => apply_recs.push(r.clone()),
                LedgerRecord::Purge {
                    quarantine_path, ..
                } => purge_paths.push(quarantine_path.clone()),
                LedgerRecord::Restore { from, .. } => restore_from.push(from.clone()),
                _ => {}
            }
        }
    }
    for rec in &apply_recs {
        let (quarantine_path, undo) = match &rec.rec {
            LedgerRecord::Apply {
                quarantine_path,
                undo,
                ..
            } => (quarantine_path, undo),
            _ => continue,
        };
        if undo.grace_expires_at >= Utc::now() {
            continue;
        }
        if !quarantine_path.exists() {
            continue;
        }
        if purge_paths.contains(quarantine_path) || restore_from.contains(quarantine_path) {
            continue;
        }
        let op_id = quarantine_path
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        rep.expired_grace.push(ExpiredQuarantine {
            op_id,
            original_path: undo.original_path.clone(),
            quarantine_path: quarantine_path.clone(),
            bytes: undo.byte_count,
            grace_expires_at: undo.grace_expires_at,
        });
    }

    // Per-volume WAL reconciliation.
    // Root discovery: ledger apply recs (same-dev writable anchors recorded in
    // quarantine paths — the boot-Data /private/tmp case) PLUS the documented
    // fallback-anchor probe list — see `discover_quarantine_roots`. A crash
    // between rename and ledger append leaves a WAL at the SIP-fallback anchor
    // (~/.Trash-janitor, /private/tmp/.Trash-janitor) with NO record anywhere
    // referencing it; the parent-chain probes surface it anyway.
    let roots = discover_quarantine_roots(
        vols,
        apply_recs.iter().filter_map(|rec| match &rec.rec {
            LedgerRecord::Apply {
                quarantine_path, ..
            } => quarantine_path
                .parent()
                .and_then(|p| p.parent())
                .map(|r| r.to_path_buf()),
            _ => None,
        }),
    );
    for root in &roots {
        let wal_path = root.join("wal.jsonl");
        if !wal_path.exists() && !root.join("ledger.jsonl").exists() {
            continue;
        }
        rep.volumes_checked += 1;
        // Capture torn-tail before sanitizing (truncation would hide it), then
        // re-parse the clean file so roll-forward appends land after valid lines.
        if wal_has_torn_tail(&wal_path) {
            rep.torn_tail = Some(wal_path.clone());
            sanitize_wal_tail(&wal_path).ok();
        }
        let (entries, raw_lines, skipped) = wal_entries_for_root(root);
        rep.skipped_corrupt += skipped;
        rep.skipped_unknown += raw_lines.saturating_sub(entries.len() as u64 + skipped);

        let mut store: Option<Store> = None;
        for (i, w) in entries.iter().enumerate() {
            if w.kind != WalKind::Intent {
                continue;
            }
            let committed = entries[i + 1..]
                .iter()
                .any(|r| r.kind == WalKind::Result && r.op_id == w.op_id && r.entry == w.entry);
            if committed {
                continue;
            }
            match &w.to {
                Some(to) if to.exists() => {
                    // Rename landed; result lost in the crash — roll forward.
                    if store.is_none() {
                        store = ledger::open(root).ok();
                    }
                    let appended = store.as_mut().is_some_and(|s| {
                        s.wal_intent(&WalEntry {
                            op_id: w.op_id.clone(),
                            kind: WalKind::Result,
                            plan_id: w.plan_id.clone(),
                            entry: w.entry,
                            from: w.from.clone(),
                            to: Some(to.clone()),
                            bytes: w.bytes,
                            undo_of: None,
                        })
                        .is_ok()
                    });
                    if appended {
                        rep.rolled_forward += 1;
                    }
                }
                other => {
                    // Neither source nor quarantine exists (or source still in
                    // place and nothing moved): drop tail — report only.
                    rep.dangling_intents.push(DanglingIntent {
                        op_id: w.op_id.clone(),
                        entry: w.entry,
                        from: w.from.clone(),
                        to: w.to.clone(),
                    });
                    // Best-effort: remove an op dir left empty by a dropped tail.
                    if let Some(op_dir) = other
                        .as_deref()
                        .and_then(|t| t.parent())
                        .filter(|d| d.starts_with(root))
                        && fs::read_dir(op_dir)
                            .map(|mut d| d.next().is_none())
                            .unwrap_or(false)
                    {
                        let _ = fs::remove_dir(op_dir);
                    }
                }
            }
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SPEC §5.1: EEXIST ⇒ `-<n>` suffix retry, syscall-as-existence-check.
    #[test]
    fn eexist_retry_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("f.bin");
        let op = dir.path().join("op");
        fs::create_dir(&op).unwrap();
        fs::write(&src, b"fresh").unwrap();
        fs::write(op.join("f.bin"), b"occupied").unwrap(); // collide base name
        let to = rename_with_retry(&src, &op, std::ffi::OsStr::new("f.bin")).unwrap();
        assert_eq!(to, op.join("f.bin-1"), "EEXIST must retry with -1 suffix");
        assert_eq!(fs::read(&to).unwrap(), b"fresh");
        assert!(!src.exists());
        assert_eq!(fs::read(op.join("f.bin")).unwrap(), b"occupied");
    }

    /// Advisory pick must skip every occupied suffix, matching the retry loop.
    #[test]
    fn pick_free_target_skips_occupied() {
        let dir = tempfile::tempdir().unwrap();
        let op = dir.path().join("op");
        fs::create_dir(&op).unwrap();
        fs::write(op.join("f.bin"), b"1").unwrap();
        fs::write(op.join("f.bin-1"), b"2").unwrap();
        let to = pick_free_target(&op, std::ffi::OsStr::new("f.bin")).unwrap();
        assert_eq!(to, op.join("f.bin-2"));
    }

    /// Bug 3: `/private/tmp/x` and `/System/Volumes/Data/private/tmp/x` are ONE
    /// directory — an APFS firmlink, so they share st_dev+st_ino while
    /// `canonicalize()` returns two different strings. Both spellings used to
    /// survive discovery, so every root-iterating pass handled one op twice:
    /// restore's second pass found the files already moved back and reported
    /// them as phantoms in `missing`. Discovery must never return one directory
    /// twice.
    #[test]
    fn discover_roots_dedupes_firmlink_spelling() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        let aliased = PathBuf::from("/System/Volumes/Data").join(real.strip_prefix("/").unwrap());
        assert_eq!(
            macos::lstat_identity(&real).unwrap(),
            macos::lstat_identity(&aliased).unwrap(),
            "premise: {aliased:?} must firmlink-alias {real:?} on this machine"
        );

        // The primary root must exist, else both spellings fall to the string
        // fallback and the identity path is never exercised.
        let quar = real.join(macos::QUARANTINE_ROOT_NAME);
        fs::create_dir(&quar).unwrap();
        let want = macos::lstat_identity(&quar).unwrap();

        let vol = VolumeInfo {
            dev: want.0,
            mount: real.clone(),
            fs: "apfs".into(),
            total: 0,
            free_raw: 0,
            free_important: 0,
            free_opportunistic: 0,
            read_only: false,
            rename_excl: true,
        };
        let roots = discover_quarantine_roots(
            std::slice::from_ref(&vol),
            std::iter::once(aliased.join(macos::QUARANTINE_ROOT_NAME)),
        );

        let ids: Vec<(u64, u64)> = roots
            .iter()
            .filter_map(|r| macos::lstat_identity(r).ok())
            .collect();
        let mut uniq = ids.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(
            ids.len(),
            uniq.len(),
            "one directory must not appear twice: {roots:?}"
        );
        assert_eq!(
            ids.iter().filter(|id| **id == want).count(),
            1,
            "the quarantine root itself must be returned once: {roots:?}"
        );
    }
}
