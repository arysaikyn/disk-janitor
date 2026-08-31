# Disk Janitor — Technical Design

> Build-ready architecture, 2026-08-30. Sources: `docs/SPEC.md` (draft v0) + research notes
> `docs/research/02…11-*.md` (02–08 verified 2026-08-30; 09–11 are the Windows/Linux/
> cross-platform-primitives notes behind the 3-platform amendment), cited inline as `[NN §x]`.
> Layout = single Rust crate `disk_janitor` (lib + bin); modules per SPEC §4.
> Integration tests link the lib; no runtime besides per-thread runtimes (§2).

## 1. Overview

**Product claim.** Disk Janitor is a developer-focused disk-cleanup agent, native on macOS,
Windows, and Linux, that answers the one question `du` can't: not "where are the bytes?" but
"which of these are safe to reclaim, why, and how do I get them back if I'm wrong?" [SPEC §1].
The macOS design was researched first ([02], [04], [05]); the Windows and Linux taxonomies
([09], [10]) and the cross-platform FS primitives ([11]) extend it — the judgment/reversibility
architecture carries over unchanged on all three, only the syscall substrate differs [11 §8].
The two halves of that claim
are exactly what no surveyed tool owns end-to-end [02 §2]. Judgment exists today only as
black-box or name-based heuristics: CleanMyMac's proprietary Safety Database is not
user-inspectable [02 §1], and dua-cli is deliberately "naming-based, conservative" [SPEC §2].
Reversibility ends at the Trash (AppCleaner/Hazel: survives only until the Trash is emptied) or
does not exist at all — DaisyDisk's Collector staging ends in *permanent* unlink, "cannot be
'undeleted'" [02 §1 ⚠ correction to SPEC's pain table]. A generic LLM agent knows DerivedData is
deletable but won't act: it cannot undo, keeps no persistent ledger, and re-judges the same tree
every week [SPEC §2]. Disk Janitor therefore ships: evidence-graded candidates, a reviewable
plan manifest, same-volume quarantine with WAL-guaranteed one-command restore, a cross-session
JSONL ledger, and an optional LLM that compiles intent into policy and never sees a path
[SPEC §3].

**Design philosophy: judgment + reversibility.** Three commitments shape every module:

1. *Evidence over names.* Every candidate carries machine-derived evidence the user can inspect
   (lockfile presence, sibling-manifest absence, subtree mtime heat, flock markers,
   content-addressable-store shape, `st_nlink`/APFS clone-id, `com.apple.quarantine` xattr) and
   a rebuild-cost grade `PureCache / RebuildableFromSource / Redownloadable / Expensive /
   HandsOff` with a regen command and measured cost [03 §0, §1]. Name matching is precisely the
   failure mode we supersede (AppCleaner's false-positive leftovers) [02 §4].
2. *Reversibility as the default state.* `apply` frees zero bytes: `renamex_np(RENAME_EXCL)`
   moves subtrees into a hidden, per-volume quarantine directory [05 §7]. 删除是操作，恢复是权利 —
   deletion is an operation, restoration is a right. The only irreversible step (`purge`) is
   gated by a typed byte count; WAL + checksummed JSONL make every state crash-recoverable
   [08 §4–5].
3. *Honesty — in bytes and in privacy.* Byte counts use allocated size with hardlink / clone /
   sparse discounts (`st_size` lies for pnpm stores, Docker.raw, APFS-compressed files)
   [03 §6, §8-risks; 05 §6]; freed-space reports account for Time-Machine snapshot pinning
   [02 §3]; and the LLM layer is *architecturally* unable to see paths — enforcement lives in
   one serializer chokepoint plus a CI leak test, not in prompt discipline [07 §privacy].

## 2. System architecture

```
                 +------------------------------------------------------------+
                 |                 disk-janitor (one process)                 |
                 |                                                            |
 keyboard ---> [Input thread] --input_rx(32)---------------------------+     |
                 |                                                     |     |
                 |   scan_rx(100)                     +----------------v---+ |
 [Scan thread] --+----------------------------------->|     UI thread      | |
   owns dua-core |   cand_rx(1000)                    | Terminal + AppState| |
   RootWalk iter +----------------------------------->| select! -> draw    | |
   (drop=cancel) |   exec_rx(16)                      +--------------------+ |
                 +----------------------------------->    ^         ^  ^     |
                 |   agent_rx (turns, receipts, trajectory)  |         |       |
 [Exec thread] --+-------------------------------------------+         |       |
   apply/purge/  |                                                     |       |
   restore       |                                    [Agent thread] --+       |
                 |                                     own tokio rt;          |
 [Tick thread] --+-- tick_rx(1) 100 ms AtomicBool flip  async-openai;         |
                 |                                      sans-IO AgentRun;     |
                 |                                      tools call local      |
                 |                                      scan/classify/plan/   |
                 |                                      execute in-process    |
                 |                                                            |
                 |  [Walker pool] dua-core work-steal, n=available_parallelism|
                 |    spawned by Scan thread; DirStat batches -> scan_rx      |
                 |                                                            |
                 |  Durable state (files, not threads):                       |
                 |    ~/.disk-janitor/{ledger.jsonl, wal.jsonl}  (flock)      |
                 |    <vol>/.Trash-janitor/<op>/ + per-volume WAL             |
                 |    plan-<ts>.json manifest                                 |
                 +------------------------------------------------------------+
```

### 2.1 Thread model

| Thread | Owns | Emits | Consumes | Notes |
|---|---|---|---|---|
| Input | `crossterm::event::read()` loop | `input_rx` | — | bounded(32); dua's exact pattern [06 §5] |
| UI | `Terminal<CrosstermBackend>`, `AppState` | `cmd_tx`, draws | all `*_rx` | redraw per event (diffs are cheap) + 100 ms tick during long tasks; ncdu/gdu cadence [06 §5] |
| Scan | dua-core `RootWalk`, dataless I/O policy | `scan_rx` | cancel flag | dropping the iterator cancels + joins workers [04 §dua] |
| Walker pool | (dua-core internal) | → Scan thread | — | work-stealing, `available_parallelism()` threads [04 §stack] |
| Classify | rayon pool | `cand_rx` | `scan_rx`, cancel | evidence IO per candidate; no shared maps, channel boundary [04 §6] |
| Exec | WAL fd, ledger store, quarantine dirs | `exec_rx` | plan, cancel | strictly sequential, WAL-ordered; polls cancel between entries |
| Agent | own tokio rt, `ChatClient` | `agent_rx` | `cmd_tx`, tools | sans-IO `AgentRun` stepped on this thread [07 §rig] |
| Tick | `AtomicBool` | `tick_rx` | — | flips every 100 ms (dua `Throttle`) [04 §progress] |

No tokio outside the agent thread — dua and gitui stay on threads + crossbeam, and ratatui's FAQ
agrees async only pays off with many concurrent tasks [06 §5].

### 2.2 Channel topology

| Channel | Kind (crossbeam-channel) | Bound | Backpressure / drop policy |
|---|---|---|---|
| `input_rx` | `crossterm Event` | 32 | `try_send`; drop key-repeat spam on Full (dua accepts Full drops) [06 risks] |
| `scan_rx` | `ScanEvent` (batched `DirStat`s) | 100 | dua's cap; scanner blocks, UI drains `try_recv` [04 §progress] |
| `cand_rx` | `Candidate` | 1000 | classify blocks |
| `exec_rx` | `ExecEvent` | 16 | exec blocks (user-visible ops are slow anyway) |
| `agent_rx` | `AgentEvent` | unbounded (low rate) | turns are seconds apart |
| `tick_rx` | `()` | 1 | `try_send`, latest-wins |
| `cmd_tx` (UI →) | `UiCmd` | 8 | blocking send — user intent is never dropped |

Shared state: `Arc<AtomicBool>` cancel (checked per directory / candidate / plan entry) and
`Arc<ScanStats>` atomics (entries, bytes, elapsed) for the footer. Nothing else — no
`DashMap`, no cross-thread locks [04 §6].

### 2.3 Process-level invariants

- `setiopolicy_np(IOPOL_MATERIALIZE_DATALESS_FILES_OFF)` at scan start (saved/restored via
  `getiopolicy_np`); scanning iCloud trees never triggers downloads; content reads fail
  `EDEADLK` — fine, scan is metadata-only [05 §5][04 risks].
- `flock(LOCK_EX | LOCK_NB)` on `~/.disk-janitor/janitor.lock` — a second instance fails fast
  with a clear message [08 §6].
- Async confined to the agent thread; UI, scan, classify, exec are plain threads over
  crossbeam channels [06 §5].

### 2.4 Crates by layer

dua-core 3.3.0, crossbeam-channel, rayon, sysinfo, nix 0.31.3 + libc 0.2.189, rustix (xattr),
ratatui 0.30.2 + crossterm 0.29 (pinned pair [06 §1]), unicode-width, signal-hook, indicatif
(CLI only), async-openai 0.41.3 + tokio (agent thread only), tiktoken-rs, serde / serde_json /
toml, crc32fast, tempfile 3.27.0, fs4, fs-err, same-file, dirs, secrecy.

Per-platform additions behind `#[cfg]` gates ([09][10][11]): Windows — `windows` 0.62.2 or
`windows-sys` 0.61.2 (FileRenameInfoEx / FlushFileBuffers / bulk enumeration), `winreg` 0.56.0;
Linux — `rustix` `fs::renameat_with`/`syncfs`/`statx`, `libc` `renameat2` + `RENAME_NOREPLACE`;
portable — `same-file` 1.0.6 (volume/file-index identity), `dirs` 6.0.0 (XDG/FOLDERID),
`filetime` 0.2.29 (mtime preservation). Full version/license table: `crate-map.md` §1f.

## 3. Module designs

For each module: responsibility, key types, public API sketch, crates, and the research that
justifies it. Fields are the contract; method bodies elided.

### 3.1 `scan/` — parallel metadata-first walker

| Aspect | Decision |
|---|---|
| Responsibility | walk chosen roots, aggregate per-dir stats, emit batches; never materialize dataless files; never follow symlinks; stay on one filesystem by default; report volume truth (raw free vs purgeable) |
| Walker | dua-core 3.3.0 `walk_roots(roots, threads, Order::ParentFirst, opts, descend)` — metadata arrives with enumeration via `getattrlistbulk(2)` (one syscall per 64 KiB buffer; the macOS module requests mtime/flags/nlink/allocsize/clone-id in-bulk) [04 §dua] |
| Mount skipping | per-entry `MetadataExt::dev()` compare inside `descend`, not mount-table parsing; firmlink-safe — firmlinked `/`, `/Users`, `/System/Volumes/Data/...` share `st_dev` [05 §1][04 §4] |
| Sizes | capture both apparent (`ATTR_FILE_DATALENGTH`/`len`) and allocated (`ATTR_FILE_DATAALLOCSIZE`/`st_blocks·512`); allocated is what purge actually frees and what the typed confirm shows [04 §5][05 §6] |
| Dataless | read `SF_DATALESS` from `st_flags`/`getattrlist`; scan never opens content (reads fail `EDEADLK` under the OFF policy — fine, metadata-only) [05 §5] |
| Snapshots | record `tmutil listlocalsnapshots` per scan for freed-space honesty [02 §3] |
| Platform note | macOS keeps the dua-core `getattrlistbulk` walker as designed [04 §dua]. Portable baseline everywhere: `ignore` 0.4.33 `WalkParallel` (the only walker fully portable today [11 §5]); per-OS fast paths behind the same `DirStat` output — Linux `statx` birth-time/mtime via rustix [11 §7], Windows `GetFileInformationByHandleEx(FileIdBothDirInfo)` bulk enumeration copied from dua-core's windows module [11 §5]. Linux adds mount resolution per candidate via `/proc/self/mountinfo` (fstype → `renameat2` availability), not just `st_dev` [10 §0][11 §4]. Windows: never follow reparse points/junctions; OneDrive subtrees stat-only; `\\?\` paths past 260 chars [09 §0][09 §3]. |

```rust
pub struct ScanOptions { pub roots: Vec<PathBuf>, pub threads: usize,
    pub cross_filesystems: bool /* default false */, pub ignore: GlobSet,
    pub min_dir_bytes: u64 /* pre-filter before classify */ }
pub struct DirStat { pub dev: u64, pub ino: u64, pub parent: Arc<PathBuf>,
    pub name: OsString, pub depth: u16, pub kind: Kind,        // Dir|File|Symlink
    pub app_bytes: u64, pub alloc_bytes: u64, pub entries: u32,
    pub mtime: SystemTime, pub flags: u32 /* UF_/SF_ */, pub nlink: u32,
    pub clone_id: Option<u64> /* APFS copy-on-write clone [04 §dua] */ }
pub enum ScanEvent { Batch(Vec<DirStat>), RootError { root: PathBuf, err: io::Error },
    Finished(ScanTotals) }
pub struct ScanTotals { pub files: u64, pub alloc: u64, pub elapsed: Duration,
    pub snapshots: Vec<String>, pub volumes: Vec<VolumeInfo> }
pub struct VolumeInfo { pub dev: u64, pub mount: PathBuf, pub fs: String,
    pub total: u64, pub free_raw: u64 /* statfs */, pub free_important: u64,
    pub free_opportunistic: u64, pub read_only: bool, pub rename_excl: bool }

pub fn start(opts: ScanOptions, tx: Sender<ScanEvent>, cancel: Arc<AtomicBool>,
             stats: Arc<ScanStats>) -> thread::JoinHandle<ScanTotals>;
pub fn volumes() -> Vec<VolumeInfo>; // sysinfo Disks + CFURL capacity keys; sysinfo prefers
                                     // ImportantUsage on macOS [04 §sysinfo]
```

Justification: parallel directory-level traversal is the win; per-file `lstat` is eliminated by
bulk enumeration [04 §1]; jwalk is dead (README redirects to dua-core) so dua-core it is
[04 §stack]; `drop(iterator)` = cancel + join is exactly R4's interrupt primitive [04 §progress];
`SF_DATALESS` is a read-only synthetic flag — scanning never triggers materialization [05 §5];
`statfs`/`NSFileSystemFreeSize` excludes purgeable, so the UI surfaces both numbers, labeled
[02 risks #1][02 §3]; EXDEV is empirically raised even across APFS volumes in one container, so
same-`st_dev` is the real precondition [05 §2].

### 3.2 `classify/` — evidence engine + grading

Responsibility: map `DirStat` subtrees → `Candidate`s. Local-only, deterministic. Two layers:
(1) path-class rules recognize known junk shapes (per-ecosystem tables [03 §1–8]); (2) evidence
probes confirm, refute, or regrade. Every verdict carries ≥1 strong signal [03 §0].

Evidence signal table (all from [03 §0]):

| Signal | Read | Effect |
|---|---|---|
| `LockfilePresent` | package-lock.json / pnpm-lock.yaml / Cargo.lock / uv.lock … newer than or matching dir mtime | deterministic regen ⇒ `RebuildableFromSource`; enables timed regen-cost estimate |
| `SiblingManifestMissing` (orphan) | no manifest in any ancestor (node_modules w/o package.json; .venv w/o pyproject.toml) | cannot re-pin deps ⇒ surface as "project deleted?" prompt, never auto-delete |
| `MtimeHot` | subtree max mtime < 10 min | refuse auto-action; gates file-level deletion, *not* whole-store prune commands [03 risks #7] |
| `FileLock` | flock marker `.package-lock.json`, pnpm/uv store locks; `lsof`-equivalent fallback | `HandsOff` until process exits |
| `GitRepoAncestor` | `.git` ancestor | confirms rebuild-from-source; `.git/HEAD` mtime = project activity |
| `CasStore` | hash-named children (cacache `content-v2/sha512`, pnpm `files/`, uv `archive-v0`) | whole-store prune only — uv: "never safe to modify the cache directly" [03 §3] |
| `Hardlinked` / `ApfsClone` | `nlink>1` or `ATTR_CMNEXT_CLONEID` | byte discount: deleting one of N links frees 0 [03 §0] |
| `Dataless` / flag-block | `SF_DATALESS`; `UF_IMMUTABLE`/`SF_RESTRICTED`/`UF_DATAVAULT`/`SF_NOUNLINK` | `HandsOff`; rename/purge would `EPERM`/`EDEADLK` [05 §5][05 §6] |
| `QuarantineXattr` | `com.apple.quarantine` present | downloaded-by-app ⇒ `Redownloadable` evidence [05 §6] |

```rust
pub struct Thresholds { pub hot_mtime: Duration, pub ancestor_depth: u8, pub min_bytes: u64 }
pub fn run(rx: Receiver<ScanEvent>, tx: Sender<Candidate>, cancel: Arc<AtomicBool>,
           th: Thresholds) -> thread::JoinHandle<ClassifyReport>;
fn probe_ancestry(path: &Path, depth_cap: u8) -> Ancestry; // manifests, lockfiles, .git,
                                                           // pyvenv.cfg, .venv markers
fn classify_dir(d: &DirStat, anc: &Ancestry, th: &Thresholds) -> Option<Candidate>;
fn regen_for(kind: PathClass) -> Option<Regen>;             // "npm ci"≈40s, "cargo build"…
```

Path-class rule sources: node [03 §1], rust [03 §2] (cargo-sweep's toolchain-keyed `target/`
rules — MIT but unmaintained: borrow the rules, not the maintenance expectation [03 §2]),
python [03 §3], Xcode [03 §4] (resolve `IDECustomDerivedDataLocation` before trusting the
default path), Homebrew [03 §5] (invoke `brew cleanup`, don't re-implement its logic), Docker
[03 §6] (sparse Docker.raw ⇒ `HandsOff` as a file, propose `docker system prune`), browsers
[03 §7], generic caches [03 §8].

Crates: rayon 1.12 (CPU-bound classify only — `par_bridge` carries a documented IO-iterator
bottleneck warning [04]); rustix `fs::xattr` or libc `getxattr` [05]; cargo_metadata 0.23.1 to
enumerate Rust workspace `target/` dirs correctly [03 §rec]; globset for user ignore globs
[04].

### 3.3 `plan/` — selection + manifest

Responsibility: filter candidates under `PolicyConstraints` → a reviewable JSON manifest
(terraform-plan style); zero side effects; pre-flight refusals so `apply` never surprises.

| Rule | Behavior |
|---|---|
| grades allowed | default allow `{PureCache, RebuildableFromSource, Redownloadable}`; `Expensive`/`HandsOff` require explicit per-entry opt-in |
| hard refusals (evidence-backed) | `MtimeHot`, `FileLock`, `OrphanTree`, `Dataless`, `UF_IMMUTABLE`, `SF_RESTRICTED`, read-only volume |
| environment refusals | cross-volume target (`EXDEV` [05 §2]); no `VOL_CAP_INT_RENAME_EXCL` (`ENOTSUP` on exFAT/FAT [05 §1]); non-APFS quarantine root |
| prune commands | CAS stores / docker / brew become `PruneCommand` entries; their dry-run output is captured into the manifest [03 §3][03 §5][03 §6] |
| honesty | totals = Σ `alloc_bytes` with per-(dev,ino) hardlink dedup; snapshot-pinning caveat attached [02 §3] |

```rust
pub fn build(cands: &[Candidate], pc: &PolicyConstraints, vols: &[VolumeInfo]) -> Plan;
pub fn write_manifest(p: &Plan, path: &Path) -> io::Result<()>; // fs-err; pretty JSON
pub fn load_manifest(path: &Path) -> io::Result<Plan>;
pub fn summarize(p: &Plan) -> String; // "quarantine 5.4 GiB / 11 entries; prune 1.6 GiB via 2 cmds; refuse 3"
```

Justification: the staging-manifest UX is DaisyDisk's Collector minus its permanent-unlink
ending [02 §4]; Hazel treats rules as a first-class, previewable artifact — closest cousin to
our policy→plan compiler [02 §4]; the typed byte-count gate replaces CleanMyMac's deliberately
irreversible "Remove Securely" anti-pattern [02 §2 item 6]; refusals pre-checked here map onto
the exact `errno` families `apply` would otherwise hit (`EXDEV`, `ENOTSUP`, `EDEADLK`, `EPERM`)
[05 §1][05 §2][05 §5][05 §6].

### 3.4 `execute/` — quarantine / purge / restore + WAL

Responsibility: apply, purge, restore driven by the plan; WAL writes; recovery at startup; all
operations idempotent.

Quarantine protocol ([05 §7] + [05 §1]):

| Step | Mechanism | Failure handling |
|---|---|---|
| root | per volume: `<mnt>/.Trash-janitor/` mkdir 0700 + `chflags UF_HIDDEN` (flag survives rename, empirical) | never touch Apple's `.Trashes` (root-owned ACL minefield) [05 §7] |
| op dir | `<root>/<wal-op-id>/` | restore = reverse renames recorded in WAL; purge = recursive unlink of op dir |
| pre-checks | `statfs` both ends: same `st_dev` (`EXDEV` otherwise — even across APFS volumes in one container [05 §2]); capability probe `VOL_CAP_INT_RENAME_EXCL` cached per volume (advisory — trust `errno` `EINVAL`/`ENOTSUP` at call time; the `RENAME_SWAP`-bit-mismatch precedent [05 §1 empirical]); flag pre-check `UF_IMMUTABLE`/`SF_RESTRICTED`/`SF_NOUNLINK` [05 §6]; dataless target ⇒ refuse (`EDEADLK` under the OFF policy) [05 §5] | plan already refused; execute re-verifies (TOCTOU armor) |
| move | `renamex_np(from, to, RENAME_EXCL \| RENAME_NOFOLLOW_ANY)` — moves the symlink itself; `EEXIST` ⇒ retry `-<n>` suffix; the syscall IS the existence check (no TOCTOU vs plain `rename`) [05 §1][05 §7] | `ELOOP` ⇒ a path component became a symlink; re-plan |
| post | `sync_all()` on the parent dir fd (open `O_RDONLY\|O_DIRECTORY`) | — |
| restore | reverse `renameatx_np` from the WAL's recorded `(original, quarantine)` pair; `ENOENT` benign (already restored) | volume absent ⇒ report with recorded `st_dev` + `mntonname`, not a cryptic `errno` [05 §7] |
| cross-volume | refused at plan; documented downgrade only with explicit consent: `copyfile(COPYFILE_MOVE \| COPYFILE_ALL)` non-atomic [05 §2] | — |
| WAL placement | WAL lives on the **same volume as its quarantine** (`<quarantine>/wal.jsonl`) — recovery is impossible after a cross-volume crash otherwise [08 risks #3] | — |

```rust
pub fn open(plan: &Plan, ledger: Ledger) -> Result<Executor>;
impl Executor {
  pub fn apply(&mut self, tx: Sender<ExecEvent>) -> Result<ApplyReport>;
  // per entry: wal.append(intent) -> sync_data -> renamex_np -> wal.append(result) -> parent-dir sync
  pub fn purge(&mut self, typed_bytes: u64, tx: Sender<ExecEvent>) -> Result<PurgeReport>;
  pub fn restore(&mut self, op: WalOpId, tx: Sender<ExecEvent>) -> Result<RestoreReport>;
}
pub fn recover(vols: &[VolumeInfo], ledger: &Ledger) -> RecoveryReport; // §5.2 sequence
```

Crates: libc 0.2.189 raw decls (`renamex_np`, `renameatx_np`; define the missing consts
`RENAME_NOFOLLOW_ANY=0x10`, `SF_DATALESS`, `UF_DATAVAULT` locally [05 §reusable]); nix 0.31.3
(`fstatat`+`AtFlags`, `fcntl(F_FULLFSYNC)`, `Errno` mapping); fs4 flock; fs-err for ledger
paths; `std::fs::sync_all()` already maps to `F_FULLFSYNC` on apple — no extra crate for the
data fsync [08 §2].

### 3.5 `ledger/` — JSONL history + crash recovery

Responsibility: two files in `~/.disk-janitor/` — `ledger.jsonl` (permanent, R5 台账) and
`wal.jsonl` (crash journal, checkpointed). Single writer guarded by flock. Derived state never
lives only in memory — rebuildable from the JSONL [08 §7 lesson].

Record envelope + kinds [08 §4][08 §8]:

```
{"v":1,"seq":42,"ts":"2026-08-30T12:00:00Z","kind":"apply","crc32":…,"prev_crc":…,"rec":{…}}
```

| `kind` | `rec` payload |
|---|---|
| `scan_result` | root, files_scanned, total_bytes, duration_ms, session_id |
| `verdict` | path_hash (never raw path), size, grade, evidence[] |
| `plan` | plan_id, entries[]{path_hash,size,grade,action}, policy_snapshot |
| `apply` | plan_id, entry, original_path, quarantine_path, renamex_np:true, undo{original_path,byte_count,grace_expires_at} |
| `purge` | plan_id, entry, bytes_freed (typed confirm echoed), quarantine_path |
| `restore` | plan_id, entry, from, to, undoes:<apply seq> |
| `budget` | calls, tokens, cost_usd (R6) |

```rust
pub fn open(home: &Path) -> Result<Store>; // flock sidecar LOCK_EX|LOCK_NB [08 §6]
impl Store {
  pub fn append<R: Serialize>(&mut self, kind: Kind, rec: &R) -> Result<u64>;
    // envelope + crc32fast over canonical serde_json::to_vec(rec)+seq [08 §4]
  pub fn wal_intent<W: Serialize>(&mut self, w: &W) -> Result<()>;
    // write_all + sync_data() — std maps to F_FULLFSYNC on apple [08 §2]
  pub fn sync_boundary(&mut self) -> Result<()>;
    // parent dir fd sync_all — APFS reorders two independent fullfsyncs (SQLite 3.46.1) [08 §1]
  pub fn replay(path: &Path) -> Result<(Vec<Record>, RecoveryReport)>;
    // BufRead::read_until(b'\n'); torn tail -> truncate + park *.torn;
    // mid-corrupt -> skip-and-continue + report [08 §5]
  pub fn checkpoint(&mut self) -> Result<()>;
    // replay -> temp (same dir) -> sync temp -> sync parent -> rename over [08 §1][08 §5]
}
```

Privacy asymmetry (documented decision): `verdict`/`plan` recs store `path_hash` only; the
`apply`/`restore` recs store real paths — restore needs them, and the ledger is local-only;
the LLM never reads it [08 risk #6][07 §privacy].

Crates: crc32fast 1.5.1, serde_json (canonical `to_vec` as CRC input), fs4 1.1.0 (or
`nix::fcntl::Flock`), tempfile 3.27.0 for compaction renames (we own the fsyncs — tempfile's
`persist` does NOT sync [08 §3]), fs-err. Rejected: `sled` (alpha line stalled since 2024-10;
the ledger is simple enough to own) [08 §rec].

### 3.6 `policy/` — intent → constraints

Responsibility: compile a user intent (NL sentence, TUI toggles, or CLI flags) into
`PolicyConstraints`. Local rule compiler first; LLM fallback sees aggregates only.

| Input | Path |
|---|---|
| NL sentence | ordered first-match regex/keyword rules (Hazel's rule philosophy [02 §4]): "腾出 25GB" ⇒ `target_bytes`; "别动我这两周在跑的项目" ⇒ `deny_under_mta=14 d`; "别动 Xcode 归档" ⇒ `deny_globs += ~/Library/Developer/Xcode/Archives` |
| TUI toggles / CLI flags | structured → `PolicyConstraints` directly |
| LLM fallback (only when local rules are uncertain) | prompt = intent sentence + `AggregateStats { total_by_grade, count_by_grade, top_dir_shapes }` — no paths; JSON-schema'd output, validated + clamped locally [07 §privacy 1,4] |

```rust
pub fn compile(src: &IntentSource, cfg: &Config) -> Result<CompiledPolicy, PolicyError>;
// CompiledPolicy { constraints, applied_rules: Vec<&'static str>, unmatched: Vec<String> }
pub fn llm_compile(sentence: &str, agg: &AggregateStats, client: &ChatClient) -> Result<PolicyConstraints>;
```

Crates: regex, serde; reuses `agent/`'s `ChatClient` (no second HTTP stack). Thresholds live in
`config/` so tuning the SPEC §7 open questions (grace period, hot-mtime threshold) is a config
change, not a code change.

### 3.7 `agent/` — tool-calling loop (the privacy chokepoint)

Responsibility: an OpenAI-compatible chat-completions tool loop that decides reclaim actions;
enforces budget + stop conditions; streams a trajectory for the TUI. The single architectural
point where the privacy invariant is enforced.

Loop shape — a sans-IO state machine (rig-run pattern; testable + serializable without a
network; matches the WAL/crash-recovery ethos [07 §rig]):

| Step | Meaning |
|---|---|
| `CallModel{turn, history}` | one `create_chat_completion` (stream:false — stream only the TUI text display [07 risks]); pre-check ctx fit with tiktoken [07 §context] |
| `CallTools{calls}` | validate the `arguments` JSON strings (repair/skip/fail hooks; `tool_retry_limit=3` hashed per name+args) [07 §wire][07 risks] |
| `Done{reason}` | `StopReason::{ModelStop, RequestedByTool, MaxTurns, TokenBudget, CostBudget, UserCancel, ToolFailLoop, ContextCap}` [07 §budget] |

Tools — no path ever crosses outward:

| Tool | Returns (redacted shape [07 §privacy 1]) |
|---|---|
| `scan_summary` | volumes + totals; starts an async scan |
| `candidates` | opaque IDs `C-001…` (deterministic size-desc order) with `{id, bytes, grade, evidence_summary[], mtime_bucket, dir_shape:"node_modules-like tree"}` |
| `build_plan` | policy constraints in ⇒ plan summary `{counts, bytes by grade, refusals}` |
| `apply` / `restore` | accept ONLY ID lists; the ID→path map is resolved inside `execute/` after the decision [07 §privacy 2] |
| `ledger_receipts` | `{id, verdict, bytes, grade}` [07 §privacy 3] |
| `record_verdict` | terminal control tool (swiftide `Stop` pattern) [07 §budget] |

Cost accounting [07 §cost]: per-call
`(prompt − cached)·p_miss + cached·p_hit + completion·p_completion` (reasoning tokens are
already inside `completion_tokens` — never double-count); provider-reported usage is ground
truth (never local tokenizers for billing); per-call usage persisted to the ledger; interrupt at
`config.budget`. Provider quirks handled: DeepSeek thinking-mode `reasoning_content` echo bloat
(avoid thinking mode in agent loops), OpenRouter final usage-chunk shape deviation (an
accounting frame, not a second turn), Ollama id-less tool calls (mint a synthetic id), lost
final usage chunk (fall back to a tiktoken estimate marked "unverified" — never billed)
[07 §providers][07 risks].

Leak discipline [07 §privacy 5]: a CI test asserts `serde_json` of every outbound message
rejects `/`, `~`, `/Users/`, `/Volumes/`, `\.(app|dmg|trash)$`; tool `description`/`name` and
JSON-Schema `description` fields carry no path or name hints (e.g. not "delete a node_modules
dir" — that leaks a name) [07 risks].

```rust
pub struct AgentRun { state: RunState, budget: BudgetState, history: Vec<Msg> }
impl AgentRun {
  pub fn next_step(&mut self, client: &ChatClient) -> Step;        // rig-run sans-IO [07 §rig]
  pub fn execute_tools(&mut self, calls: &[ToolCall], ctx: &SessionCtx) -> Vec<ToolMsg>;
  pub fn serialize_tool_result(o: &ToolOutput) -> RedactedJson;    // THE chokepoint
  pub fn snapshot(&self) -> RunSnapshot; // serializable; resume after crash/Ctrl-C
}
pub fn cost_of(u: &Usage, p: &PriceTable) -> f64;
```
Session save/load (R5 half): `RunSnapshot` persists as `~/.local/state/disk-janitor/sessions/<run_id>.json`
(`dirs::state_dir()`), atomic write via tempfile + fsync (same protocol as WAL, no rename-under-lock
hazards — sessions are per-run, no cross-process contention). Ledger gains a `Session` record kind
referencing `<run_id>` + byte offset into the run's trajectory so the ledger trajectory screen can
reload a past run; `janitor agent --resume <run_id>` (CLI) and the trajectory screen's `R` key (TUI)
call `RunSnapshot::load()` and continue `next_step()` from the restored `RunState` [R5; 08 §5].

Crates: async-openai 0.41.3 (typed types, `with_api_base` for DeepSeek/OpenRouter/Ollama/vLLM,
built-in `SimpleRetryPolicy` 429/5xx + `Retry-After`), tokio (`rt-multi-thread` confined to
this thread), tiktoken-rs 0.12 (preflight only), serde_json [07 §rec].

### 3.8 `ui/` — ratatui TUI + CLI fallback

Responsibility: ratatui TUI — evidence review, progress, plan, apply/purge gates, ledger
trajectory, settings. CLI subcommands as fallback (indicatif on stderr, never inside the TUI
loop [06 §4]).

Loop shape [06 §5 synthesis]:

1. UI thread owns `Terminal + AppState`; `terminal.draw` then `crossbeam::select!` over
   `input_rx`/`scan_rx`/`cand_rx`/`exec_rx`/`agent_rx`/`tick_rx`; redraw per event (diffs are
   cheap) + 100 ms tick while a long task runs (ncdu/gdu cadence).
2. Input thread: `crossterm::event::read()` → `bounded(32)` (dua's exact pattern).
3. Workers never touch ratatui — they send events (gdu's `QueueUpdateDraw` discipline).
4. Ctrl-C = cancel current task, not quit: raw mode ⇒ `cfmakeraw` clears ISIG ⇒ Ctrl-C arrives
   as a key event [06 §2]; if a task is active → set cancel flag + "Stopping…" (gdu keeps
   partial results); double-Ctrl-C or idle → quit; `signal-hook` routes real SIGINT/SIGTERM
   into the same path; a panic hook calls `ratatui::restore()` [06 risks].
5. Every exit path flushes WAL/ledger first (R4 crash-consistency) [06 §5].
6. `KeyEventKind::Press` guard; `unicode-width` (+segmentation) for CJK path columns [06 §crates].

Screens & gates:

| Screen | Content / gate |
|---|---|
| Dashboard | volumes (raw free vs important-usage free, labeled [02 risks #1]), snapshot list, running cost |
| Review | virtualized candidate table (`StatefulWidget` windows; rebuild `ListItem`s per draw, O(visible) [06 §3]); re-select by stable path key when the list reorders mid-scan (dua) [06 §3]; evidence panel per candidate |
| Plan | manifest summary + refusals + prune dry-runs |
| Apply | progress + per-entry `Applied` events |
| Purge modal | type the exact allocated byte count; mismatch refuses |
| Ledger | trajectory from ledger records |
| Settings | `config.toml` fields (R3) |
| Agent | trajectory lines + token/cost footer |

```rust
pub enum UiCmd { StartScan(ScanOptions), BuildPlan(PolicyConstraints), ConfirmPurge(u64),
                 Restore(WalOpId), Cancel, Quit }
pub enum UiEvent { Key(KeyEvent), Resize(u16, u16), Scan(ScanEvent), Cand(Candidate),
                   Exec(ExecEvent), Agent(AgentEvent), Tick }
pub fn run(rx: Receiver<UiEvent>, cmd_tx: Sender<UiCmd>, cfg: Config) -> Result<Exit>;
```

Crates: ratatui 0.30.2 + crossterm 0.29 pinned together — two crossterm majors in one tree keep
separate event queues, lose events, and may not restore raw mode [06 §1]; unicode-width 0.2.2;
unicode-segmentation; crossbeam-channel; signal-hook 0.4.4; indicatif (CLI only); ratatui
`TestBackend` for snapshot tests [06 §rec].

### 3.9 `config/` — `config.toml` + env fallback

Responsibility: `config.toml` (R3) + env overrides + TUI settings pane; secrets never logged.

```rust
pub struct Config {
  pub endpoint: Url, pub api_key: SecretString, pub model: String,
  pub ctx_len: u32, pub thinking: ThinkingMode,            // R3 fields [SPEC §5]
  pub prices: Vec<ModelPrice>,                             // per 1M: prompt_miss, prompt_hit, completion [07 §cost]
  pub budget: Budget { cost_usd, max_turns, tokens },      // R6 interrupt
  pub thresholds: Thresholds { hot_mtime: 10 min, grace_hours: 72, min_candidate_bytes, ancestor_depth },
  pub ui: UiConfig { tick_ms: 100, scan_channel_bound: 100 },
}
pub fn load() -> Result<Config>; // dirs::config_dir()/disk-janitor/config.toml -> env overrides
pub fn redact(&self) -> String;  // safe debug/log form
```

Crates: serde, toml, secrecy, dirs. The DeepSeek peak/off-peak hit/miss pricing split [07 §cost]
is modeled as multiple `ModelPrice` entries keyed by model name.


### 3.10 `platform/` — per-OS abstraction layer

Responsibility: one facade per OS behind shared traits so `scan/`, `execute/`, and `ledger/`
stay OS-agnostic. Every syscall substrate difference from notes 09–11 lands here; the macOS
behavior already specified in §3.1–3.4 is the gold-standard baseline [11 §1]. Impl style copies
the proven cfg-gating idioms: trash-rs's per-OS `mod platform;`, dua-core's `mod macos; mod
windows;` + std fallback, xattr's `platforms!` macro [11 §7].

Trait sketches (fields are the contract; bodies elided):

```rust
pub struct AtomicMoveOpts { pub no_clobber: bool, pub nofollow: bool }
pub trait AtomicMove {
    /// Quarantine: no-clobber, same-volume only. `EEXIST`/`ERROR_*_ALREADY_EXISTS` family
    /// mapped to one `MoveError::TargetExists` for the `-<n>` retry loop.
    fn move_noreplace(&self, from: &Path, to: &Path, opts: AtomicMoveOpts) -> Result<(), MoveError>;
    /// Restore: replace semantics. Returns the guarantee actually delivered (§5 delta).
    fn move_replace(&self, from: &Path, to: &Path) -> Result<ReplaceGuarantee, MoveError>;
    fn probe(&self, vol: &VolumeInfo) -> AtomicitySupport; // cached per volume at scan time
}
pub enum ReplaceGuarantee { Atomic /*macOS renamex_np*/, PosixRename /*Win 1709+*/,
    MoveFileEx /*pre-1709: non-atomic w.r.t. open targets*/, CopyFallback }
pub enum AtomicitySupport { Full, FsWithGaps /*Linux NFS/Windows SMB*/, None /*refuse at plan*/ }
```

Per-OS `AtomicMove` impls [11 §1]:

| OS | no-clobber (`apply`) | replace (`restore`) | Probes / fallback |
|---|---|---|---|
| macOS | `renamex_np(from, to, RENAME_EXCL \| RENAME_NOFOLLOW_ANY)` — unchanged from §3.4 [05 §1] | `renameatx_np` reverse rename from the WAL pair | capability probe `VOL_CAP_INT_RENAME_EXCL`; `EEXIST` retry suffix [05 §1] |
| Linux | `renameat2(AT_FDCWD, src, AT_FDCWD, dst, RENAME_NOREPLACE)` — `EEXIST` on target; kernel ≥3.15, glibc wrapper ≥2.28 | plain `rename()` is always POSIX-atomic on Linux | per-volume fstype probe via `/proc/self/mountinfo` field (9): ext4 ≥3.15, btrfs/tmpfs ≥3.17, xfs ≥4.0, many ≥4.9; **NFS rejects any flags (`-EINVAL` in `nfs_rename()`)** — probe once, then `O_TMPFILE`+`linkat(AT_EMPTY_PATH)` staging or refuse auto-action [10 §0][11 §1/risks] |
| Windows | open source with `DELETE` access (`FILE_FLAG_OPEN_REPARSE_POINT \| FILE_FLAG_BACKUP_SEMANTICS`), then `SetFileInformationByHandle(FileRenameInfoEx, Flags=0)` — no-clobber = *omit* `FILE_RENAME_FLAG_REPLACE_IF_EXISTS` (0x1); POSIX_SEMANTICS (0x2) keeps replaced-file handles valid. Win10 1709+ per rust-lang#123985; [MS-FSCC] states no min version — runtime probe, never assumed | Win10 1709+: `REPLACE_IF_EXISTS \| POSIX_SEMANTICS` (atomic replace, old handles stay valid). Pre-1709: `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` — non-atomic w.r.t. open targets (`ERROR_ACCESS_DENIED`) — the exact two-tier fallback rust std uses | startup probe of `FileRenameInfoEx`; same-volume via `same-file` Handle equality (`dwVolumeSerialNumber`), not raw `st_dev`; `ERROR_NOT_SAME_DEVICE` ⇒ refuse cross-volume [09 §delta][11 §1/§4] |

**Walker.** Trait `Walker { fn walk(opts: WalkOpts) -> DirStream }` with the same `DirStat`
output contract on every OS: macOS = dua-core 3.3.0 `getattrlistbulk` module (unchanged, §3.1)
[04 §dua]; portable baseline = `ignore` 0.4.33 `WalkParallel` — the only fully-portable walker
today — with gitignore semantics disabled for honest byte accounting [11 §5]; per-OS fast paths
behind the same output, following dua-core's in-tree precedent: Linux `statx` (rustix
`StatxFlags::BTIME` for birth time; Unix `MetadataExt` `st_dev`/`st_nlink`/`st_blocks` portable
across Unix) [11 §4/§7], Windows `GetFileInformationByHandleEx(FileIdBothDirInfo)` bulk
enumeration (dua-cli measured "30x and more performance on Windows" since 2.41) [11 §5].
Windows walker rules: never descend reparse points (`IO_REPARSE_TAG_MOUNT_POINT` junctions —
cycles, other volumes, `\\?\` escapes; junction target counts as outside the root); skip
`\\wsl$` entirely (virtual 9P view; the ext4.vhdx is hands-off) [09 §0/§4]. Linux: resolve each
candidate's mount from `/proc/self/mountinfo` (field 3 major:minor = `st_dev`, field 9 fstype)
to decide `renameat2` availability [10 §0][11 §4].

**VolumeInfo.** `trait VolumeInfo { fn volumes() -> Vec<VolumeInfo>; fn capacity(&self) -> ...;
fn same_volume(&self, a: &Path, b: &Path) -> bool }`. macOS: sysinfo `Disks` + CFURL capacity
keys, `ImportantUsage` preferred (unchanged §3.1) [04 §sysinfo]. Windows:
`BY_HANDLE_FILE_INFORMATION.dwVolumeSerialNumber` / `FILE_ID_INFO` replaces `st_dev`; the
`same-file` crate's `Handle` equality (volume serial + file index, with its documented
file-index-aliasing fallback) is the portable identity check, valid on UNC paths too [09 §0][11 §4].
Linux: `statvfs`/`statfs` + mountinfo resolution; `st_dev` compare suffices including bind
mounts [10 §0][11 §4].

**DatalessCheck.** `trait DatalessCheck { fn is_dataless(&self, md: &impl MetadataLike) -> bool;
fn scan_open_policy(&self) -> OpenPolicy }` — scan must never trigger hydration on any platform:

| OS | Detection | Rule |
|---|---|---|
| macOS | `SF_DATALESS` in `st_flags`/`getattrlist`; `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` at scan start (unchanged, §2.3/§3.1) [05 §5] | read triggers download; metadata-only [05 §5] |
| Windows | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (0x00400000) from `FindFirstFileW`/`WIN32_FIND_DATAW` or `GetFileInformationByHandleEx(FileBasicInfo)`; siblings `RECALL_ON_OPEN`, `OFFLINE`, `PINNED`, `UNPINNED`; open with `FILE_FLAG_OPEN_NO_RECALL` if a handle is unavoidable | OneDrive/cldflt placeholders (Win10 1709+): never open content, size from metadata, never include hydrated size in purge math, never move cross-volume (forces full recall) [09 §0/§3][11 §3] |
| Linux | no OS-level dataless files — fstype from `/proc/self/mountinfo` is the nearest signal (NFS/autofs/overlayfs analogues) [11 §3] | none of the iCloud/OneDrive kind; no `.placeholder` convention exists in the notes (unverified) |

Dataless evidence stays classify-side on every OS (`Dataless` ⇒ `HandsOff` / refuse move); the
xattr-based tagging alternative is rejected — xattrs are not portable (Linux per-fs, Windows
unsupported in the `xattr` crate) ⇒ the JSONL ledger is the only restore-metadata store [11 §7/risks].

Durability mapping (WAL §5.2 across platforms) [11 §2]: macOS `F_FULLFSYNC` + fsync(dirfd)
(baseline, unchanged); Linux `fsync(file)` + mandatory `fsync(dirfd)` ("calling fsync() does not
necessarily ensure that the entry in the directory … has also reached disk"), `syncfs` available
(error reporting since 5.8); Windows `FlushFileBuffers` (needs `GENERIC_WRITE`) or
`FILE_FLAG_WRITE_THROUGH` — **no directory-fsync API exists**; metadata durability rides NTFS
journaling, weaker than `F_FULLFSYNC` (documented cache→device write, no platter-barrier
request). This is the documented Windows guarantee delta of §5.

Crates (full version/license table in `crate-map.md` §1f): Windows-gated `windows` 0.62.2 /
`windows-sys` 0.61.2 + `winreg` 0.56.0 behind `#[cfg(windows)]` (the crates do not compile
their Win32 modules elsewhere — keep the cfg at the module boundary) [09 §crates]; Linux-gated
`rustix` `fs::renameat_with`/`syncfs`/`statx` + `libc` `renameat2` [10 §crates][11 §rec];
portable `same-file` 1.0.6, `dirs` 6.0.0, `filetime` 0.2.29 (mtime preservation across
move/restore) [11 §rec].

## 4. Data flow — one scenario end-to-end

User request, typed in the TUI agent screen: **"腾出 25 GB，别动我这两周在跑的项目"**.

| # | Stage | What happens | Artifacts |
|---|---|---|---|
| 1 | policy | NL compiler matches local rules: `target_bytes=25 GiB`, `deny_under_mta=14 d`, `allow {PureCache, RebuildableFromSource, Redownloadable}`; no LLM call needed (rules fully cover the intent) | `CompiledPolicy` in memory |
| 2 | scan | volume info read (free_important vs raw, labeled [02 §3]); dua-core walk of `~` with materialization OFF; `DirStat` batches stream over `scan_rx`; footer = atomic snapshot. Finds: `node_modules` A 2.1 GiB, `node_modules` B 0.9 GiB, `DerivedData` 6.8 GiB, `~/.npm/_cacache` 3.2 GiB, `Docker.raw` (apparent 64 GiB / allocated 11 GiB), pnpm store (hardlinked into several projects) | `ScanTotals`; `scan_result` ledger rec |
| 3 | classify | ancestor probes: **A** — `package-lock.json` newer than dir ⇒ `RebuildableFromSource`, regen `npm ci` ≈ 40 s [03 §1]; **B** — no `package.json` in any ancestor ⇒ orphan ⇒ prompt-only [03 §0]; **DerivedData** — sibling project dirs + `IDECustomDerivedDataLocation` resolved [03 §4] ⇒ `RebuildableFromSource`; **_cacache** — hash-named children ⇒ `PureCache` CAS, whole-store action only [03 §1]; **Docker.raw** — sparse (apparent ≫ allocated) ⇒ `HandsOff` as a file + `PruneCommand docker system prune` [03 §6]; pnpm store ⇒ `nlink>1` discount; `.git/HEAD` mtimes feed `deny_under_mta` | `Candidate` stream; `verdict` recs (`path_hash` only) |
| 4 | plan | `build()`: 11 `Quarantine` entries (5.4 GiB) — A, DerivedData, `_cacache` whole-store (the mtime-hot rule is waived for whole-store actions [03 risks #7]); 2 `PruneCommand` (`docker system prune`, `brew cleanup --prune=90d` 0.4 GiB); 3 `Refuse` (node_modules B orphan ×1, `MtimeHot` ×2); Docker.raw excluded (sparse honesty: 11 GiB allocated, not 64 GiB apparent). Totals hardlink-discounted. `write_manifest` → `plan-20260830-1432.json` | manifest JSON; `plan` ledger rec |
| 5 | apply | confirm in TUI → Exec thread, per entry: WAL intent `{op, entry, undo{original_path, grace_expires_at=+72 h}}` → `sync_data` (`F_FULLFSYNC` on Apple [08 §2]) → `renamex_np(RENAME_EXCL\|RENAME_NOFOLLOW_ANY)` into `<vol>/.Trash-janitor/w-0042/` → WAL result → parent-dir sync at the op boundary. **Zero bytes freed.** Ctrl-C mid-apply cancels between entries (WAL stays consistent); flock keeps a second instance out | `apply` recs; `ExecEvent`s |
| 6 | purge | review shows "5.4 GiB quarantined — purge frees 5.4 GiB"; user types `5841673728` (exact allocated); mismatch refuses. Per-entry recursive unlink; `bytes_freed` echoed into the WAL; freed report notes TM-snapshot pinning if snapshots exist [02 §3] | `purge` recs |
| 7 | restore (next morning) | a project won't build ⇒ `restore w-0042` (or one TUI key): reverse renames from the WAL `(original, quarantine)` pairs for entries not yet purged; `undoes` seq recorded; within the 72 h grace; the quarantine op dir is removed when empty | `restore` recs |
| 8 | agent overlay (same data) | user asks the agent "what's safe?": it calls `candidates` ⇒ sees only `C-001…` `{bytes, grade, evidence_summary}`; calls `build_plan`; calls `record_verdict`. It never sees a path (chokepoint + CI leak test); cost accrues per call; `Budget` interrupt fires at the cap | `budget` recs |

Byte ledger across the scenario: `apply` moves 0 bytes; `purge` frees 5.4 GiB allocated;
`restore` returns 0 net for already-purged entries (the WAL only restores surviving entries)
and fully restores any entry purged-then-regretted within the grace window.

## 5. Safety architecture

### 5.1 Quarantine protocol (invariants)

- **Same-volume by construction**: per-volume root; `statfs` re-checks both ends at apply time
  [05 §7][05 §2].
- **Hidden**: `UF_HIDDEN`, mode 0700; never Apple's `.Trashes` (root-owned ACL minefield)
  [05 §7].
- **Atomic**: `RENAME_EXCL | RENAME_NOFOLLOW_ANY`; `EEXIST` retries with a `-<n>` suffix; plain
  `rename`'s crash-atomicity guarantee plus `EXCL` flags mean no partial state [05 §1].
- **Lossless**: xattrs and BSD flags survive `renamex_np` (empirical) [05 §6].
- **Discoverable-by-us only**: the WAL holds the `(original, quarantine)` pairs; Trash Put-Back
  metadata is undocumented and is never relied on [02 risks #5].
- **Honest**: zero bytes freed at apply; `purge` requires a typed byte count equal to allocated
  bytes.

### 5.2 WAL + ledger write sequence (per operation)

1. `flock` held for the process lifetime [08 §6].
2. `wal.append(intent {op, entry, undo})`; `sync_data()` = `F_FULLFSYNC` on Apple (std does it
   since PR #60121) [08 §2].
3. Filesystem mutation (`renamex_np` / `unlinkat`).
4. `wal.append(result)`; at the op-group boundary, `sync_all()` on the parent dir fd — APFS
   reorders two independent `fullfsync`s (the SQLite 3.46.1 backport inserted exactly this
   directory sync between them) [08 §1][08 risks #2].
5. Every N records / at session end: `checkpoint` = replay valid records → temp file in the
   same dir → `sync` temp → `sync` parent → `rename` over the WAL [08 §5].

Sync granularity: per **state transition**, not per record (`F_FULLFSYNC` ≈ 4 ms vs 0.075 ms
for a plain `fsync` on Apple SSDs — amortize at the transition boundary) [08 §1][08 risk #1].

Recovery on startup [08 §5][08 §8][08 risk #5]:

1. Take `flock`; replay `ledger.jsonl` (skip-and-continue; report skipped offsets and `seq`
   gaps).
2. Replay the `wal.jsonl` tail; a record is committed iff its CRC validates.
3. Reconcile against observed FS state: `quarantine_path` exists ⇒ roll forward
   (`apply`/`restore` are re-runnable — `rename` fails benignly with `ENOENT`); otherwise drop
   the tail.
4. Verify `grace_expires_at`: expired quarantines become purge candidates (surfaced, not
   auto-purged).

### 5.3 Confirmation gates

| Gate | Mechanism |
|---|---|
| apply | plan manifest reviewed in TUI; apply is reversible, so the gate is soft (Enter) |
| purge | type the exact allocated byte count; mismatch refuses; this is the only irreversible step |
| prune commands | dry-run output rendered first (`brew -n`, docker filter preview); network regen is opt-in [03 §5][03 §3] |
| hard-evidence refusals | `MtimeHot`/`FileLock`/`OrphanTree`/`Dataless`/flag-blocks never enter the auto plan; surfaced as prompts [03 §0] |
| Expensive/HandsOff grades | excluded from the default policy; require explicit per-entry opt-in |
| external / exFAT volume | refuse auto-apply with evidence (no `RENAME_EXCL` / `EXDEV`) [05 §2] |
| budget | agent loop interrupted at `config.budget` (cost / tokens / turns) [07 §budget] |

### 5.4 Privacy invariant

The LLM never sees paths or names — only the intent sentence (user-authored, passed through
verbatim) and aggregate statistics [SPEC §3.3][07 §privacy]. Enforcement, strongest first:

1. **Structural** — tools return opaque IDs (deterministic size-desc order); `execute` accepts
   only ID lists; the ID→path map lives in the ledger and is resolved inside `execute/` [07
   §privacy 1–2].
2. **Serializer chokepoint** — a single `serialize_tool_result`; receipts are redacted to
   `{id, verdict, bytes, grade}` [07 §privacy 3].
3. **CI leak test** — outbound `serde_json` must reject path-shaped regexes [07 §privacy 5].
4. **Tool-metadata hygiene** — names/descriptions/schema descriptions are path-free [07 risks].

Asymmetry, documented: the ledger stores real paths for undo (local-only; the LLM never reads
it) [08 risk #6].

### 5.5 Platform guarantee delta (Windows)

The §5.1 quarantine invariants hold as written on macOS. On Windows the OS substrate is
weaker and the delta is documented, not hidden [09 §delta][11 §1/§2]:

- **apply (no-clobber)** remains atomic and TOCTOU-free — `SetFileInformationByHandle(
  FileRenameInfoEx, Flags=0)` fails if the target exists, on Win10 ≥1607. This is the
  RENAME_EXCL analog [09 §delta][11 §1].
- **restore (replace)** is fully atomic (old handles stay valid) only on Win10 ≥1709 with
  `REPLACE_IF_EXISTS | POSIX_SEMANTICS`; on 1607–1708 it degrades to `MoveFileExW(
  MOVEFILE_REPLACE_EXISTING)`, which is non-atomic with respect to open targets
  (`ERROR_ACCESS_DENIED`/sharing violations). Restore correctness still holds because the
  WAL is written first; only concurrent-reader races surface as errors [11 §1/risks].
  [MS-FSCC] documents no minimum OS version — the 1709 gate is enforced by a runtime probe
  with `MoveFileExW` fallback, never assumed [09 risks #1][11 §1].
- **Crash durability**: no directory-fsync API exists on Windows and `FlushFileBuffers` is
  weaker than `F_FULLFSYNC` (no platter-barrier request); NTFS journaling carries metadata
  durability. **Consequently, restore-on-Windows relies on the WAL journal — the recorded
  `(original_path, quarantine_path)` pairs — rather than any OS Trash semantics.** The
  shell Recycle Bin (`IFileOperation`) is at most a purge-time handoff surface, never the
  restore path [11 §2/§6].
- Same guarantee-shaping rules as macOS apply: cross-volume moves are refused at plan;
  network shares (SMB has no POSIX rename semantics) are treated as external volumes [11 risks #2].

## 6. Key type definitions

Rust-ish pseudocode. Field lists are the contract; method bodies elided.

```rust
// ---- scan / classify ----
pub struct Candidate {
    pub id: Cid,                 // "C-042": deterministic size-desc order [07 §privacy 1]
    pub path: PathBuf,           // local only — NEVER serialized outward
    pub path_hash: [u8; 8],      // ledger + agent-facing identity
    pub dev: u64, pub ino: u64,  // hardlink dedup key
    pub alloc_bytes: u64,        // honest reclaimable number; used for all displayed totals
    pub app_bytes: u64,          // apparent size, informational only
    pub grade: Grade,
    pub evidence: Vec<Evidence>, // >=1 Strong signal required [03 §0]
    pub regen: Option<Regen>,    // command + measured cost
    pub mtime: SystemTime, pub nlink: u32, pub clone_id: Option<u64>,
    pub locked: bool,            // flock marker / open-handle probe
}
pub enum Grade { PureCache, RebuildableFromSource, Redownloadable, Expensive, HandsOff }
pub struct Evidence { pub kind: EvidenceKind, pub detail: String, pub strength: Strength }
pub enum EvidenceKind {
    LockfilePresent, SiblingManifestMissing, OrphanTree, MtimeHot, FileLock,
    GitRepoAncestor, CasStore, Hardlinked, ApfsClone, Sparse, Dataless,
    FlagBlock(UfSf), QuarantineXattr, Unreadable,
}
pub enum Strength { Weak, Medium, Strong }
pub struct Regen { pub command: String, pub cost_secs: Option<u32>, pub network: bool }

// ---- plan ----
pub struct PlanEntry {
    pub cid: Cid, pub path: PathBuf, pub path_hash: [u8; 8],
    pub alloc_bytes: u64, pub grade: Grade,
    pub action: PlanAction,
    pub evidence: Vec<Evidence>,    // carried into the manifest for review [02 §4]
}
pub enum PlanAction {
    Quarantine,
    PruneCommand { cmd: String, dry_run: String },
    Refuse { reason: Refusal },
}
pub enum Refusal {
    MtimeHot, FileLock, OrphanTree, Dataless, ImmutableFlag, CrossVolume,
    NoRenameExcl, ReadOnly, BelowMinBytes, GradeNotAllowed,
}

// ---- ledger / WAL ----
pub struct Record<R> {
    pub v: u8, pub seq: u64, pub ts: DateTime<Utc>, pub kind: Kind,
    pub crc32: u32, pub prev_crc: Option<u32>, pub rec: R,
}
pub enum LedgerRecord {                 // rec payloads per kind [08 §8]
    Scan(ScanRec), Verdict(VerdictRec), Plan(PlanRec), Apply(ApplyRec),
    Purge(PurgeRec), Restore(RestoreRec), Budget(BudgetRec),
}
pub struct ApplyRec {
    pub plan_id: PlanId, pub entry: PlanEntryHash,
    pub original_path: PathBuf,          // real path: undo requires it [08 risk #6]
    pub quarantine_path: PathBuf, pub byte_count: u64,
    pub grace_expires_at: DateTime<Utc>, // now + 72 h
}
pub struct WalEntry {                    // intent record — written + synced BEFORE the mutation
    pub op_id: WalOpId, pub kind: WalKind,  // Intent | Result
    pub plan_id: PlanId, pub entry: PlanEntryHash,
    pub from: Option<PathBuf>, pub to: Option<PathBuf>,
    pub bytes: u64, pub undo_of: Option<u64>, // seq of the apply this restore undoes
}

// ---- policy ----
pub struct PolicyConstraints {
    pub target_bytes: Option<u64>, pub min_bytes: u64,
    pub allow_grades: Vec<Grade>,
    pub deny_globs: Vec<Pattern>,        // user paths (local only)
    pub deny_under_mta: Duration,        // "don't touch what I ran this fortnight"
    pub allow_network_regen: bool, pub allow_prune_commands: bool,
    pub grace_hours: u64, pub max_entries: u32,
}
```

Invariants worth testing:

- **ID stability** — `Candidate.id` is a pure function of the scan result: sort candidates by
  `alloc_bytes` desc, tie-break by `path_hash`; identical scans yield identical IDs (so a
  resumed agent run re-addresses the same candidates).
- **Byte honesty** — `alloc_bytes` is used wherever a byte count is shown or typed; `app_bytes`
  is informational. Hardlinked/clone bytes are attributed once per `(dev, ino)` / `clone_id`.
- **Replay self-sufficiency** — every `LedgerRecord`/`WalEntry` is self-contained for replay;
  the only cross-record reference is `undo_of` (a `seq`), so a single corrupted line cannot
  poison replay of independent later operations [08 §5].

## 7. Testing strategy

| Layer | Approach |
|---|---|
| Fixture trees | `tests/fixtures/gen.py` (or a Rust bin) builds synthetic trees: node_modules+lockfile, orphan `.venv`, DerivedData-shaped dir, `_cacache` CAS store, pnpm store with hardlinks (`fs::hard_link`), a sparse file (seek+write), a symlink chain + loop, a `UF_HIDDEN` quarantine root (chflags via libc). Dataless cannot be synthesized in userspace (`SF_DATALESS` is kernel-synthetic, read-only [05 §6]) — unit tests mock the flag on `Metadata`-like values (classifier takes the flag as input, not raw lstat); one `#[ignore]`'d integration test runs only where a real iCloud dataless file exists (opt-in, materialization-OFF assertions). Golden classify outputs; property: Σ child `alloc` == parent `alloc` with hardlinks discounted once per `(dev, ino)`. |
| Syscall-level | `#[cfg(target_os = "macos")]` integration tests on real APFS (`/tmp`): `RENAME_EXCL`→`EEXIST`; `EXDEV` probe vs `/System/Volumes/VM` (skip if absent); `F_FULLFSYNC` rc=0 via nix; `UF_HIDDEN` survives `renamex_np`; hardlink `nlink` preserved across rename; `RENAME_NOFOLLOW_ANY`→`ELOOP` on a symlink path component; `clonefile`→`EEXIST`; capability-probe mask asserted advisory-only [05 §1]. |
| Platform matrix | CI matrix `macos-latest` / `windows-latest` / `ubuntu-latest` on every push: macOS runs the macOS `Syscall-level` suite above on APFS; Linux runs `renameat2(RENAME_NOREPLACE)`→`EEXIST` on ext4/tmpfs, `EXDEV` across mounts, fsync(file)+fsync(dirfd) WAL ordering, `/proc/self/mountinfo` fstype resolution, NFS/skip gating; Windows runs `FileRenameInfoEx(Flags=0)`→target-exists error, `MoveFileExW` fallback path, `dwVolumeSerialNumber` same-volume check, junction non-descent, `\\?\` long-path manifest round-trip, OneDrive `RECALL_ON_DATA_ACCESS` detection (skip if OneDrive absent) [09 §0/§delta][10 §0][11 §1/§4]. Syscall tests are `#[cfg]`-gated per OS (`#[cfg(target_os = "macos")]`, `#[cfg(target_os = "linux")]`, `#[cfg(windows)]`); portable layers (ledger/WAL, plan, classify fixtures, TUI snapshots, agent loop) run on all three runners. |
| Crash injection (WAL) | spawn a child `janitor` applying a plan with `KILL_AT ∈ {before_wal_sync, after_wal_sync_before_rename, after_rename_before_result, mid_batch}`; `kill -9`; parent runs `recover()` and asserts: committed ops present exactly once, no phantom ops, torn tail truncated + parked to `*.torn`, `seq` gaps reported. Optional deep probe via `dtrace`/`FSInOut` for cross-file ordering [08 risks #2]. |
| Ledger property tests | random op sequences + random truncation points → replay invariants hold (skip-and-continue, monotonic `seq` after gaps, no CRC false positives) [08 §5]. |
| TUI | ratatui `TestBackend` snapshot tests per screen [06 §rec]; input-channel drop ⇒ scan cancel+join (dua semantics) [04 §progress]; the purge modal rejects a mismatched byte string. |
| Scanner edge cases | firmlink walk of `/` (no double-walk of the Data side) [04 §4]; an iCloud-shaped tree scanned with materialization OFF (no downloads — assert no content reads via unchanged mtime/size); a mounted volume appearing mid-tree (`st_dev` reject in `descend`). |
| Agent | sans-IO `RunState` serde round-trip + resume; a stub OpenAI-compatible HTTP server (e.g. wiremock) covering: malformed `arguments` JSON (repair path), DeepSeek `reasoning_content` echo shape, OpenRouter final usage-chunk deviation, Ollama id-less tool calls, missing final usage chunk (estimate marked unverified, not billed) [07 §providers, risks]; leak-test regexes on every outbound message [07 §privacy 5]. |

## 8. Build sequencing

Course deadlines: 9/1 comments, 9/6 public demo, 9/8 tryouts, 9/10 presentation [SPEC §6].

| Milestone | Scope | Concrete work items | Exit criterion |
|---|---|---|---|
| **M1** scan + classify core | `scan/`, `classify/` + fixtures | cargo workspace skeleton; dua-core integration (`walk_roots`, `Order::ParentFirst`, `descend` dev-check); `setiopolicy_np` `extern "C"` + constants [04 risks]; `VolumeInfo` via sysinfo; fixture generator; classify path rules v1 (node/rust/python/xcode [03 §1–4]) + evidence probes (lockfile, orphan, mtime, nlink); `janitor scan --json` CLI | scanning this Mac yields classified JSON with honest `alloc` bytes |
**Platform sequencing** (deadlines first, platforms second): M1 stays macOS-first for the
9/1 comment deadline — the `scan/`/`classify/` core, fixtures, and the dua-core macOS walker
land on this Mac; the `platform/` traits (§3.10) are sketched at M1 so M2 can slot impls in
without re-plumbing, but only the macOS impl is exercised until M2 [09][10][11].

| **M2** plan / apply / purge / restore + WAL + ledger | `plan/`, `execute/`, `ledger/` | `renamex_np` layer + capability probe; per-volume quarantine + per-volume WAL; record framing (crc32fast envelope); flock; `recover()`; plan builder + manifest write/load; CLI `plan/apply/purge/restore`; crash-injection harness (`KILL_AT`). **Platform: M2 wires the trait impls for Linux** — `AtomicMove` via `renameat2(RENAME_NOREPLACE)` + per-volume fstype probe (`/proc/self/mountinfo`), fsync(file)+fsync(dirfd) durability pair, XDG cache roots, `requires-elevation` refusal class [10 §0/delta][11 §1] | CLI round trip incl. `kill -9` recovery passes |
| **M3** TUI | `ui/` | ratatui + crossterm (pinned) shell; event loop per §3.8; Dashboard/Review/Plan/Apply screens; purge modal; Ctrl-C semantics; settings pane v0. **Platform: M3 keeps Windows on the portable baseline** — `ignore::WalkParallel` walker, std path resolution; no Windows-specific UI work [11 §5] | full flow usable in TUI (9/6 demo) |
| **M4** agent + NL policy + token/cost | `agent/`, `policy/`, `config/` | `config.toml` full (R3); NL policy compiler v1; sans-IO `AgentRun` + 6 tools + chokepoint + leak test; usage/cost accounting + budget interrupt; trajectory screen. **Platform: M4 keeps Windows on the portable baseline** (same as M3); Windows `AtomicMove`/`VolumeInfo` impls (`FileRenameInfoEx`, `dwVolumeSerialNumber`, junction rules) land behind `#[cfg(windows)]` only when CI can exercise them [09 §delta][11 §1/§4] | "腾出 25GB" runs end-to-end under budget |
| **M5** polish | docs, demo, hardening | demo script; README; DESIGN.md final sync; AI-log export; 开销明细表 generated from ledger `budget` recs; threshold tuning (SPEC §7); edge-case hardening list from §7 | 9/10 presentation |

Dependency notes:

- The §5.2 WAL protocol (reconcile WAL records vs observed FS state) must be spec'd *before*
  coding M2 — it is the open question [08 risk #5].
- Platform: the CI matrix of §7 (macos/windows/ubuntu) starts at M2 — before that, Windows
  and Linux builds are compile-checked locally, not behavior-tested; Windows syscall paths
  (`FileRenameInfoEx` probes, junction handling) are untestable until a windows runner exists,
  which is why M3/M4 keep Windows on the portable baseline [09 risks #1][11 §5].
- dua-core is ~10 days old as a standalone crate (first release Aug 2026, ~10k downloads,
  v3.3.0) — pin the exact version, audit `crates/dua-lib/src/macos` unsafe (`attrlist` buffers)
  before shipping; the pre-planned fallback is `ignore::WalkParallel` [04 risks].
- M3's purge modal depends on M2's byte-count contract; M4's agent depends on M1+M2+M3 tool
  surfaces being stable.

## 9. Design references — what each module borrows

| Module | Project | Borrowed | License (verified) | Note |
|---|---|---|---|---|
| scan | Byron/dua-cli → dua-core 3.3.0 | whole walker: work-stealing pool, drop-to-cancel, `getattrlistbulk` macOS module, `descend` device check, `Throttle` redraw gating | MIT [04 §rec] | jwalk is dead — README redirects to dua-core [04] |
| scan | dundee/gdu | ticker→atomic-snapshot→draw progress loop; cancel-keeps-partial-results; OSC 9;4 terminal progress; dataless prevention | MIT [04] | |
| scan | GuillaumeGomez/sysinfo | volume enumeration + `ImportantUsage` preference | MIT [04] | |
| classify | tbillington/kondo | known-project-structure discovery per language | MIT [03 §rec] | direct precedent for path rules |
| classify | holmgr/cargo-sweep | toolchain-keyed `target/` pruning rules; `--stamp` workflow | MIT (repo) [03 §2] | unmaintained — borrow rules only |
| classify | matthiaskrgr/cargo-cache | `~/.cargo` component size accounting | MIT/Apache [03 §rec] | |
| classify | zaldih/npkill | node_modules listing UX | MIT [03 §rec] | |
| plan | DaisyDisk (behavior ref) | Collector staging UX; refuse-list for system dirs | closed [02 §4] | staging kept, permanent-unlink replaced by quarantine |
| plan / policy | Noodlesoft Hazel (behavior ref) | rules as a first-class previewable artifact; Revert Files validates post-hoc undo demand | closed [02 §4] | |
| execute | SQLite | WAL frame validity + recovery algorithm; dir-fsync discipline; journal commit styles | public domain [08 §7] | |
| execute | Stebalien/tempfile | `persist_noclobber` atomic-rename pattern (we add the fsyncs) | MIT/Apache [08 §3] | |
| execute | ArturKovacs/trash | per-volume macOS trash semantics — reference only, NOT used for moves | MIT [05 §rec] | |
| ledger | git (design ref only) | content addressing + trailer-checksum ideas | GPL-2.0 — read-only [08 §7] | do not import code |
| agent | 0xPlaygrounds/rig | sans-IO `AgentRun` state machine; `Tool`/`ToolDefinition`; `max_turns` budget | MIT [07 §rig] | |
| agent | bosun-ai/swiftide | `StopReason` enum + control tools (`Stop`→`record_verdict`); `tool_retry_limit` hashed per call | MIT [07 §rig] | |
| agent | 64bit/async-openai + openai/openai-node | typed chat-completions; `finish_reason` union; `include_usage` semantics | MIT / Apache-2.0 [07] | |
| ui | Byron/dua-cli TUI | event-loop skeleton; name-keyed selection restore; `ctrl+c` keymap | MIT [06 §rec] | |
| ui | gitui-org/gitui | multi-source `select!` loop; `any_work_pending()` gating | MIT [06 §rec] | |
| ui | gdu / yorhel/ncdu | SIGINT→synthetic key; rate-limited redraw; abort-and-keep-results | MIT [06 §rec] | |
| config | dua/gdu config patterns; async-openai `OpenAIConfig` | config shape; `with_api_base` | MIT [07] | |
| cross-cutting | alienator88/Pearcleaner | orphan-search heuristics + Sentinel Trash monitoring to read | Apache-2.0 + Commons Clause — do NOT vendor [02 §rec] | |
| cross-cutting | tw93/Mole; grandperspectiv | dry-run UX ideas; scan-result persistence | GPL-3.0 / GPL-2.0 — ideas only [03 §rec][02 §rec] | |

### Reusable-code license posture

| Repo | License | Posture |
|---|---|---|
| dua-cli / dua-core, kondo, cargo-sweep, cargo-cache, npkill, gdu, ncdu, gitui, ratatui, crossterm, async-openai, rig, swiftide, tempfile, crc32fast, fs4, nix, libc, rustix, sysinfo, serde*, walkdir, ignore/globset, rayon, crossbeam, signal-hook, unicode-width, indicatif, tiktoken-rs, fs-err, same-file | MIT / Apache-2.0 / Unlicense / PD | vendoring & borrowing safe — verify each per-file at adoption |
| Pearcleaner | Apache-2.0 + Commons Clause v1.0 | read for behavior; do not vendor into a product [02 §rec] |
| GrandPerspective, git, tw93/Mole | GPL-2.0 / GPL-3.0 | read as design reference only — never import code [02 §rec][03 §rec] |
| apple/darwin-xnu | APSL-2.0 | reference for syscall semantics; do not paste code [05 §reusable] |

---

*End of DESIGN.md — companion to `docs/SPEC.md`; iterate in lockstep with the code (course
rule). Open questions from SPEC §7 (grace period, threshold tuning, APFS snapshot interactions,
external-volume purge semantics — now a per-OS matrix, see §3.10/§5.5) are addressed in
§5/§7/§8 and remain config-tunable. Platform amendment 2026-08-30: notes 09–11; macOS
baseline preserved, Linux/Windows deltas documented in §3.10 and §5.5.*
