# Disk Janitor — Ecosystem Survey: Competitive Landscape & Reuse Catalog

> Companion to `docs/SPEC.md` (draft v0). Answers: **what can we copy or adapt?**
> Sources — research notes verified 2026-08-30: [01] OSS analyzers (`docs/research/01-oss-disk-analyzers.md`),
> [02] macOS cleanup landscape (`docs/research/02-macos-cleanup-landscape.md`), [03] dev-junk taxonomy
> (`docs/research/03-dev-junk-taxonomy.md`). Citations: `[NN §section]`; SPEC claims: `[SPEC §n]`.
> All licenses below were verified at the primary source (LICENSE file, `Cargo.toml`/`package.json`,
> crates.io/npm registry field, or official site) on 2026-08-30 by the research lanes — nothing restated from memory.

---

## 1. Comparison matrix

### 1.1 Open-source analyzers & dev-junk cleaners

| Tool | License (source) | Platform | Judgment model | Safety / undo model | Progress UX | What to copy | What to reject |
|---|---|---|---|---|---|---|---|
| **ncdu** 2.9.2 (Zig) / 1.22 (C) [01 §ncdu] | MIT — official site | cross-plat | none — pure byte map | `d` = permanent unlink; Trash only via user-supplied `--delete-command` (e.g. `gio trash`); `-r`/`-rr` read-only modes; `--confirm-delete` on by default | `-0` silent / `-1` line / `-2` full ncurses; `-q` throttles 10/s→1/2s | scan/export decoupling (`-O` binary export = the plan→apply manifest shape); `CACHEDIR.TAG` honoring (`--exclude-caches`); `--shared-column` hardlink accounting; `-x` one-file-system | firmlink & directory-hardlink double-count (man-page BUGS) — exactly a macOS edge we must beat; no native macOS Trash |
| **gdu** v5.37.0 [01 §gdu] | MIT — repo `LICENSE.md` ("Daniel Milde") | cross-plat (Go) | none; *mtime scan filters* only | `os.RemoveAll` permanent; `askBeforeDelete` confirm; `--no-delete`; **macOS Trash unsupported** (`trash_darwin.go` errors out) | live tview/tcell progress bar; `-p` disables; `--max-cores` cap (default 8) | `--max-age`/`--min-age`/`--since`/`--until` scan-time mtime gating (primitive for our "actively written ⇒ refuse"); SQLite/Badger persistence (`--db`) = cross-session memory; constant-memory top-dir mode; `2×GOMAXPROCS` semaphore; `Nlink>1→ino` dedup, `stat.Blocks×512` accounting | macOS trash gap; Go (wrong runtime); interactive-first design with persistence as afterthought |
| **dua-cli** 2.43.1 / dua-core 3.3.0 [01 §dua-cli] | MIT — repo LICENSE ("2019 Sebastian Thiel") | cross-plat, macOS-first walker (Rust) | name-based: 9-entry bisect whitelist (`.mypy_cache`…`node_modules`,`target`…), "deliberately conservative" | two `MarkMode`s: `Delete` (parallel, dirs deepest-first) and `Trash` (**default-on**, `trash` crate 5.2.0); multi-stage mark→pane→dispatch→confirm; config can disable `delete_marked`/`trash_marked` | throttled `TraversalProgress` (100 ms) in aggregate; live `TraversalStats` in TUI | the whole macOS walker: `getattrlistbulk(2)` bulk enumeration, APFS clone/firmlink flags (`ATTR_CMNEXT_CLONEID`, `EF_MAY_SHARE_BLOCKS`, `SF_FIRMLINK`); crossbeam work-stealing pool; `gix`-based `--ignore-from` gitignore excludes; staged mark→confirm delete UX; `trash` crate integration | the name-bisect cleanup list as *judgment* (our evidence chains supersede it — this is the tool the SPEC positions against) |
| **dust** 1.2.5 [01 §dust] | Apache-2.0 (single) — repo `Cargo.toml` | cross-plat (Rust) | none — read-only treemap | n/a (no delete at all) | dedicated spinner thread (100 ms) + atomic counters; `-P` disables | `par_bridge()`-over-`read_dir` idiom (cheapest correct parallelism); `clean_inodes` `(dev,ino)` HashSet dedup; `-t` group-by-filetype; XDG config file | nothing to reject for our purposes; if vendoring, keep Apache NOTICE (single-license, no MIT fallback) |
| **diskus** 0.9.0 [01 §diskus] | MIT OR Apache-2.0 — repo `Cargo.toml` | cross-plat (Rust) | none — read-only `du -sh` | n/a | none (completes in seconds) | `par_iter().for_each_with(channel)` + single-accumulator scan skeleton; `3×available_parallelism` (cap 64) cold-cache rationale; `UniqueID{dev,ino}` generated only when `nlink>1`; `Directories::Auto` du-semantics | nothing (read-only) |
| **kondo** 0.9.0 / kondo-lib 0.9.0 [01 §kondo] | MIT — `kondo/Cargo.toml` + `kondo-lib/Cargo.toml` | cross-plat (Rust) | marker-file project detection → 23-ecosystem `PROJECT_*_DIRS` name tables (Cargo, Node, Swift, Zig, Dotnet, …) | `clean()` = `fs::remove_dir_all` per artifact dir — **permanent, no trash, no dry-run** in CLI (GUI adds confirm) | none (single-threaded scan; GUI variant separate) | the 23-ecosystem artifact-dir table verbatim as seed data for `classify/`; marker-file detection constants; artifact vs non-artifact size split; `ignore::WalkBuilder` `last_modified` activity gating | name-only deletion as verdict; Bevy 0.18 + `rfd` GUI as UI precedent (a game engine as TUI alternative — do not copy); single-threaded scan |
| **cargo-sweep** 0.8.0 [01 §cargo-sweep] | MIT — repo `Cargo.toml` | cross-plat (Rust) | rustc-fingerprint→toolchain mapping (`StableSipHasher128` ≥1.85, legacy `SipHasher`; 16-hex suffix parsing) + `--time`/`--installed`/`--toolchains`/`--maxsize`/`--recursive` | `remove_dir_all`/`remove_file` permanent; `--dry-run` only; no trash | none | fingerprint evidence ("this `target/` predates your installed toolchain → rebuildable"); `--maxsize` goal-driven shrinking = our "free 25 GB" policy; `--recursive` workspace sweep | as a *dependency*: README banner "currently unmaintained" — reimplement the ~80-line matcher instead |
| **npkill** 0.12.2 [01 §npkill] | MIT — npm `package.json` `"license"` | cross-plat (TS/Node) | basename ∈ profile target lists (`node`: 24 dirs, `python`: 12, `data-science`); `is-safe-to-delete.ts` gate is literally `basename ∈ targets` | `execFile('rm', ['-rf', path])` — **permanent, shells out to rm**; `--delete-all` warning confirm; `--yes` skips confirm | `SpinnerService` frames + `ScanStatus{pendingSearchTasks, completedSearchTasks, workersJobs}`; TUI list with size bars | profile-shaped target lists (complement to kondo); bounded worker pool `min(cores−1, 8)` with `MAX_PROCS=100` in-flight tasks; flag surface `--targets`/`--delete-all`/`--yes` | shelling out to `rm -rf` (typed byte-count purge is our counter-example); basename-only gate |
| **node-prune** v1.2.0 [01 §node-prune] | MIT — repo LICENSE ("2017 TJ Holowaychuk") | cross-plat (Go) | yarn-derived prune lists *inside* `node_modules` (~70 file names, dir names, extensions; overridable) | `os.RemoveAll`/`os.Remove` permanent; no dry-run, no trash; final stats only | none during run (end summary) | the yarn-derived in-`node_modules` prune lists as curated ruleset for a future "slim node_modules in place" mode; channel-worker deletion pool idiom | treat as historical snapshot: last push 2022-11-30, low activity |
| **Mole** [03 §Reusable code] | GPL-3.0 — GitHub repo license | macOS | per-path cleanup, whitelist-driven | dry-run UX patterns | GUI | whitelist/dry-run UX patterns and per-path cleanup precedent on macOS — **ideas only** | any code copy (GPL-3.0 is viral and outside our MIT/Apache comfort zone) |

### 1.2 Commercial / freeware macOS tools

| Tool | License (source) | Platform | Judgment model | Safety / undo model | Progress UX | What to copy | What to reject |
|---|---|---|---|---|---|---|---|
| **CleanMyMac** 5.5.7 (MacPaw) [02 §1,§4] | Proprietary — ~US$39.95/yr, one-time path exists [02 S3] | macOS | proprietary **Safety Database** (since 2008): rules + per-app exceptions (caches holding licenses, quit-before-clean); black-box, human-curated, not user-inspectable | "Move to Trash" default; Remove Immediately / Remove Securely (shred); review list before acting; no undo once Trash emptied | Smart Scan progress + category review list | curated rules *with per-app exceptions* ≈ our Evidence model (the concept, not the implementation); review-before-act flow | black-box DB; "Remove Securely" shred = deliberately irreversible (anti-pattern — our typed byte-count purge replaces it) |
| **DaisyDisk** 4.10+ [02 §1,§2.4,§4] | Proprietary — US$9.99 one-time, ≤5 personal Macs [02 S6] | macOS | none — pretty sunburst map; user judges | **Collector staging → review → Delete → 5 s cancel countdown → permanent unlink, NOT Trash** ("cannot be 'undeleted'" [02 S4]); refuses /System, /Library, home in Collector; MAS edition has reduced deletion powers | sunburst animation; hidden/purgeable/snapshot space shown as first-class "hidden space" | the staging-area (Collector) UX = our `plan` manifest preview; refuse-list for system dirs; explicit purgeable/snapshot breakdown instead of hiding them | ⚠ the countdown-as-undo illusion: post-confirm action is permanent — keep the staging affordance, replace the countdown with quarantine + 72 h grace [02 §4] |
| **AppCleaner** 3.6.8 (2023-07-04) [02 §1,§4] | Freeware, closed source — free | macOS | bundle-id/name heuristics for related files (prefs, caches, app support); no published DB | checkbox review list; **moves app + related files to Trash** (real Put Back); SmartDelete daemon intercepts Finder trash-drag | review checkbox list | trash-interception UX (hooking the *existing* deletion gesture); reliance on OS Trash as genuine undo layer | name/bundle-id matching with false positives and no evidence trail — exactly what our evidence engine supersedes; single undo layer |
| **OmniDiskSweeper** 1.16 (2025-09-12 build) [02 §1] | Freeware, closed source [02 S12] | macOS | none — list sorted largest→smallest | "Trash or open them" — no staging, no rules; undo only via Trash | none notable | zero-friction list + Trash; the fastest *failure* mode — proof that judgment + reversibility is the actual product gap | nothing else worth taking: no memory, no judgment, no undo design |
| **GrandPerspective** 3.8.0 (2026-08-24) [02 §1,§4] | **GPL-2.0** — LICENSE verified in source tree [02 S15]; free on SourceForge, US$2.99 MAS | macOS | none — treemap viz + persisted filters | delete from view; **undo destination undocumented on official site** — verify in source before citing [02 §Risks 4] | scan progress; filters persisted | scan-result persistence (re-review without rescan) — cheap and valuable for our ledger UX; advanced filter predicates; TM hard-link analysis; dataless/cloud-file awareness; delete must never be one-click on a visualization | any code reuse (GPL-2.0 viral for linked code — copy-with-care per table; we read, not vendor) |
| **Pearcleaner** 5.4.3 (dev On Hold since end-2025) [02 §1,§4] | **Apache-2.0 + Commons Clause v1.0** — LICENSE.md verified [02 S17] (no "Sell"; free to read) | macOS (Swift) | orphaned-file search w/ adjustable sensitivity + include/exclude dirs; dev-env cleanup; Homebrew manager; no vendor DB — heuristics open source | Sentinel background Trash-event monitor (~2 MB RAM) offers related-file cleanup when an app is trashed; privileged helper for system folders | orphan scan progress + sensitivity UI | orphaned-file search heuristics; Sentinel event-driven Trash monitoring; privileged-helper install pattern (behavior-level) | vendoring its code (Commons Clause + project On Hold — even a good open-source scanner can't promise its own future; design our ledger to be self-sufficient) |
| **Hazel** 6.1.x [02 §1,§4] | Proprietary — ≈US$42 single / US$65 family-5 (secondary source [02 S22]; confirm before quoting in course docs) | macOS | **user-authored rules as first-class artifact**: ordered first-match rules, per-rule enable/disable, test/preview runs, run-status view | actions → Move to Trash; Trash rules then delete **permanently** (age/size-based); Hazel 6 "Revert Files" undo | rule run-status view | rule-preview UX (ordered, testable, previewable rules — closest philosophical cousin to our policy→plan compiler); Trash age/size policies mirror our 72 h grace; "Revert Files" validates post-hoc undo demand | rule semantics as name/metadata-matchers with no build-system or rebuild-cost awareness |
| **Disk Drill** 6.3.2326 [02 §1,§4] | Proprietary — Basic free (100 MB recovery); PRO US$89/yr or US$149 lifetime [02 S23] | macOS | none for cleanup — included as the "undo-first" reference product | **Recovery Vault / Guaranteed Recovery**: pre-deletion metadata/byte-level retention; per-file recovery-chance predictions (high/medium/low) | scan/recovery progress with per-item chance labels | the *inversion*: capture reversibility before loss; per-item recovery-confidence framing ("rebuildable in ≈40 s") as UX vocabulary | paying storage to insure deletions, only for things it saw deleted — our WAL+quarantine is cheaper insurance (zero bytes until purge) |
| **macOS Storage Management** (built-in) [02 §1,§4] | Bundled with macOS | macOS | category browse (Store in iCloud, Optimize, Empty Trash 30 d, Reduce Clutter); "System Data" opaque aggregate; Apple purge rules internal (CacheDelete plists, ~32) — not published [02 S26][02 S27] | all deletions user-confirmed via standard flow; system may purge "safe caches and logs" itself; no evidence chains | Settings-pane category list | no-op-first philosophy (purgeable reclaim before user deletion — CacheDelete computes "available" via `volumeAvailableCapacityForImportantUsageKey`); simple age-based Trash policy | opaque "System Data" aggregate; no evidence; no cross-session memory — our JSONL ledger + evidence chains are the two features the OS never offered |

### 1.3 Built-in / OS-level primitives (treated as tools)

| Tool | License | Platform | Judgment model | Safety / undo model | What to copy | What to reject |
|---|---|---|---|---|---|---|
| **tmutil / APFS local snapshots** [02 §3] | Apple built-in (man pages) | macOS | none — snapshot lifecycle is time-driven (hourly, kept 24 h; one per backup; one pre-update) | snapshots are the OS's own undo channel; space they hold counts as *available* (purgeable) | `tmutil listlocalsnapshots` / `deletelocalsnapshots {mount\|date}` / `thinlocalsnapshots <mount> [purge_amount] [urgency 1-4]`; `diskutil apfs listSnapshots` / `deleteSnapshot` (root required) — our bytes-freed report must anticipate snapshot-held space and query/warn [02 §Risks 2] | consumer-facing snapshot removal (only toggle-TM-off/on path) as an undo mechanism for *us* — our quarantine is explicit and app-owned |
| **CacheDelete purgeable space** [02 §3] | Apple built-in | macOS | internal (~32 plists, unpublished) | zero user risk; phased: caches first, **TM local snapshots last**, only TM purges TM snapshots [02 S27] | the no-op-first reclaim order as the mental model for our "before you delete anything, here's what the OS reclaims anyway" panel; `statfs(2)` reports raw FS free ≠ purgeable — must use the URLResourceKey triple to report honestly [02 §3] | relying on undocumented CacheDelete plists as rules; presenting purgeable as user-deletable bytes |

> SPEC correction carried forward [02 §1 ⚠]: the pain-point table in [SPEC §2] claims DaisyDisk's undo "depends on Trash not being emptied" — the official guide states deletion is **permanent**. Our design must not cite a Trash round-trip for DaisyDisk; it *lacks* any restore path, which strengthens the gap analysis below.

---

## 2. Judgment & reversibility gap analysis

The SPEC's core claim [SPEC §2]: **judgment ("can this be deleted?") and reversibility ("what if I'm
wrong?") are 99% of cleanup work; no existing tool owns either end-to-end.** Mapping every surveyed
tool onto these two axes shows the claim is accurate and quantifies where Disk Janitor lands.

### 2.1 Axis 1 — Judgment (none → names → fingerprint → evidence)

| Level | Tools | What "judgment" means there | Why it isn't enough |
|---|---|---|---|
| **0. None** | ncdu, gdu, dust, diskus, OmniDiskSweeper, GrandPerspective, macOS Storage Mgmt (categories) | byte locations / category aggregates; the human is the judgment engine | the tool answers "where are the bytes", never "which of these are safe to reclaim, why" [SPEC §1] |
| **1. Name-based** | dua-cli (9-name bisect list), kondo (23-ecosystem dir tables), npkill (`basename ∈ targets`), node-prune (yarn prune lists), AppCleaner (bundle-id heuristics) | "directory named `node_modules` ⇒ junk" | dua's own framing: "naming-based, deliberately conservative" [01 §dua-cli]; can't distinguish orphan vs live project, actively-written vs stale, lockfile-pinned vs not |
| **2. Fingerprint / rule-based** | cargo-sweep (rustc-version hash → toolchain), CleanMyMac (proprietary Safety Database w/ per-app exceptions), Hazel (user-authored rules), Mole (whitelist) | "artifact built by a toolchain you no longer have" / "human-curated rules + exceptions" | cargo-sweep is unmaintained and its judgment is single-ecosystem; CleanMyMac's DB is black-box and not inspectable [02 §4]; Hazel's rules are name/metadata matchers with no rebuild-cost awareness [02 §4] |
| **3. Evidence chains (ours)** | *no surveyed tool* | `node_modules` + valid lockfile + registry reachable → `RebuildableFromSource` ≈ 40 s; sibling manifest absent → orphan, promoted; `~/Library/Caches` entry mtime < 10 min → actively written, refuse auto-action [SPEC §3.1; 03 §0] | the [03 §0] signal table (lockfile, orphan detection, mtime recency, file locks, git presence, content-addressable stores, clone/hardlink discount) is the specification of this level |

[01 §Risks 4] states the strategic risk explicitly: kondo's and dua's lists "delete on **name match
only**" and porting them into an `Evidence`-graded engine is the core differentiator.

### 2.2 Axis 2 — Reversibility (permanent → Trash → quarantine → privileged undo)

Spectrum from [02 §2], with the surveyed tools placed on it:

| Reversibility tier | Tools | Failure mode |
|---|---|---|
| **Permanent unlink, no staging** | npkill (`rm -rf` via execFile), node-prune, kondo, cargo-sweep, gdu (macOS) | one confirmation or flag (`--yes`, `--delete-all`) away from data loss |
| **Permanent unlink, staged confirm** | DaisyDisk (Collector + 5 s countdown — *not* Trash [02 S4]); ncdu `d` key (permanent by default) | staging is UX, not a recovery channel |
| **Trash (OS Put Back)** | dua-cli (default-on `trash` crate), AppCleaner, OmniDiskSweeper, Hazel, Apple's own flows | survives only until Trash emptied; Put-Back metadata format undocumented — our restore must store original paths in the WAL ourselves [02 §Risks 5] |
| **Pre-capture / recovery** | Disk Drill (Recovery Vault / Guaranteed Recovery), Apple's TM snapshots | storage-hungry; only for things seen/captured before loss; snapshots only purgeable by TM itself [02 §3] |
| **Same-volume quarantine move** | **no surveyed tool** — closest cousins: Apple snapshot-then-delete, Disk Drill pre-capture [02 §2.5] | this is the unowned end-to-end tier: atomic `renamex_np(RENAME_EXCL)` move, zero bytes freed until `purge`, WAL-crash-recoverable, 72 h `restore` [SPEC §3.2] |
| **Deliberately irreversible** | CleanMyMac "Remove Securely" (shred) | anti-pattern; our typed byte-count purge confirm replaces it [02 §2.6] |

### 2.3 The gap Disk Janitor fills

```
              judgment axis →
   none ─────────── names ──────── rules ─────────── evidence
   │ ncdu/gdu/dust    │ dua/kondo    │ cargo-sweep       │ ★ Disk Janitor
   │ Omni/GrandPersp  │ npkill       │ CleanMyMac/Hazel  │   (only occupant)
   │ macOS Storage    │ node-prune   │ Mole              │
  ─┼──────────────────┼──────────────┼───────────────────┼──────
   │                  │              │                   │
   permanent ─── staged ─── Trash ─── pre-capture ─── quarantine
   unlink        confirm     (dua,     (Disk Drill,    ★ Disk Janitor
   │ npkill      │ DaisyDisk    AppCleaner,  TM snaps)    (only occupant;
   │ kondo       │ ncdu `d`     Hazel,                    WAL + 72 h grace)
   │ node-prune  │              Omni)
```

- **Unowned quadrant**: evidence-graded judgment × same-volume quarantine. Every tool occupies at most
  one interesting cell of each axis, never the far corner of either, and no tool spans both [02 §2].
- **The strongest direct competitors fail on macOS specifics**: gdu's Trash errors out on macOS
  (`trash_darwin.go`) [01 §gdu]; ncdu double-counts firmlinks (documented BUGS) [01 §ncdu]; only dua-cli
  trashes on macOS at all [01 §Risks 1]. None handles APFS clones, purgeable accounting, or dataless
  iCloud files.
- **Even the judgment leaders don't own the evidence**: cargo-sweep's fingerprint logic is the best
  "which toolchain built this artifact" reference but is unmaintained [01 §cargo-sweep; 03 §Risks]; the
  kondo/dua/npkill lists are seed data, not verdicts [01 §Risks 4].
- **Even the reversibility leaders borrow OS mechanisms**: Trash survival is at the mercy of the user
  emptying it; Disk Drill charges storage and coverage; snapshots are system-owned. Our quarantine is
  app-owned, atomic, zero-byte until purge, and WAL-recoverable [SPEC §3.2] — 删除是操作，恢复是权利.

---

## 3. Borrowable architecture catalog

Per project: the exact mechanism to adapt, where it lives, and license compatibility. Copy-with-care
flags: **Apache-single** = keep NOTICE if vendoring; **Commons Clause** = read-only, never vendor;
**GPL** = no code reuse, ideas only; **unmaintained** = reimplement, don't pin.

| Project | Mechanism to adapt | Lives in | License note |
|---|---|---|---|
| **dua-cli** | macOS bulk walker: `getattrlistbulk(2)` enumeration returning packed metadata (name, type, size, mtime, inode, clone-id) per readdir record, no per-entry `stat`; APFS clone dedup via `ATTR_CMNEXT_CLONEID` + `EF_MAY_SHARE_BLOCKS`, firmlink tracking via `SF_FIRMLINK`; hand-rolled Darwin constants not in `libc` (verify against XNU `vfs_attrlist.c` when vendoring) | `crates/dua-lib/src/macos/mod.rs`, `.../macos/attributes.rs` [01 §dua-cli, §Risks 2] | MIT — clean to adapt [01 S3] |
| dua-cli | work-stealing walker: `crossbeam::deque::{Injector, Stealer, Worker}` + `Parker/Unparker`; `Order::{ParentFirst, Completion}`; threads default = logical processors | `crates/dua-lib/src/lib.rs` [01 §dua-cli] | MIT |
| dua-cli | staged-delete UX: mark → Mark pane → dispatch → confirm; `MarkMode::{Delete, Trash}`; parallel delete collecting files+dirs then removing dirs deepest-first; config keys to disable `delete_marked`/`trash_marked` | `src/interactive/widgets/mark.rs`, `src/interactive/app/handlers.rs` [01 §dua-cli, S8] | MIT |
| dua-cli | name-bisect cleanup candidate list `CLEANUP_DIR_NAMES_SORTED_FOR_BISECT` — **seed data only**, upgrade names → evidence chains | `src/interactive/app/cleanup.rs` [01 §dua-cli] | MIT |
| dua-cli | gitignore-style excludes via `gix` (features `excludes,sha1,sha256`); TUI gitignored-entry marking via `gix::ignore::Kind` | `src/interactive/app/gitignore.rs` [01 §dua-cli] | MIT |
| dua-cli | throttled aggregate progress (`"Enumerating N items\r"`, 100 ms); live `TraversalStats` in `AppState` | `src/aggregate.rs` [01 §dua-cli] | MIT |
| **trash-rs** | macOS Trash move: `NSFileManager::defaultManager()` + `trashItemAtURL:resultingItemURL:error:` via `objc2_foundation` — the reversible primitive our quarantine complements. ⚠ moves *across* volumes to `~/.Trash` → quarantine `renamex_np` stays fast path, Trash is fallback | `src/macos/mod.rs` [01 §Risks 1, S9] | MIT (`trash` 5.2.6) |
| **gdu** | scan-time mtime gates `--max-age`/`--min-age`/`--since`/`--until` — the primitive under "mtime < 10 min ⇒ actively written"; constant-memory top-dir totals for non-interactive runs; SQLite/Badger analysis persistence = cross-session memory (maps to our ledger); `concurrencyLimit = 2×GOMAXPROCS` semaphore; hardlink dedup `Nlink>1 → ino` + `stat.Blocks×512` | `pkg/analyze/parallel.go`, `pkg/analyze/dir_unix.go`, `pkg/analyze/sqlite.go` [01 §gdu, S13] | MIT — Go code, port the *semantics* |
| **dust** | `par_bridge()`-over-`read_dir` recursion (unraveled because naive recursion ≈3× drops rayon parallelism); `(dev,ino)` HashSet inode dedup post-pass; spinner-thread + atomic-counter progress | `src/dir_walker.rs`, `src/progress.rs` [01 §dust, S15] | Apache-2.0 single — attribute if vendoring [01 S14] |
| **diskus** | `par_iter().for_each_with(channel)` + single accumulator thread (drop-in scan skeleton without going native); `default_num_workers() = 3×available_parallelism` cap 64 (cold-cache wants IO threads); `UniqueID{device, inode}` generated only when `is_file() && nlink()>1`; `Directories::Auto` du-semantics | `src/walk.rs`, `src/unique_id.rs` [01 §diskus, S16] | MIT OR Apache-2.0 |
| **kondo** | 23-ecosystem `ProjectType` table → `PROJECT_*_DIRS` consts (Cargo→`["target",".xwin-cache"]`, Node→`["node_modules",".angular"]`, Python→`["__pycache__",".mypy_cache",…]`, Swift, Zig, Dotnet, Terraform, …); marker-file detection constants (`Cargo.toml`, `package.json`, `pyvenv.cfg`-equivalents); artifact vs non-artifact size split; `ignore::WalkBuilder` with `ScanOptions{follow_symlinks, same_file_system}` for size + `last_modified` | `kondo-lib/src/lib.rs` [01 §kondo, S17] | MIT |
| **cargo-sweep** | rustc-fingerprint matching replicating Cargo's own hashing: `rustc_stable_hash::StableSipHasher128` (≥1.85) + legacy `SipHasher`; `hash_from_path_name` parses 16-hex suffix → "which toolchain built this" evidence; `--maxsize` goal-driven shrink (mirrors "free 25 GB" policy); `--stamp` timestamp-marker workflow | `src/fingerprint.rs` [01 §cargo-sweep; 03 §Rust, S10] | MIT — ⚠ unmaintained banner: reimplement the ~80-line matcher, don't pin |
| **npkill** | profile target lists (`node` 24 dirs, `python` 12, `data-science`) complementing kondo; worker pool `min(cores−1, 8)` with `MAX_PROCS=100` in-flight tasks per worker; flag surface `--targets`/`--delete-all`/`--yes`; `getRecentModificationInDir` activity probe | `src/core/constants/profiles.constants.ts`, `src/core/services/files/*`, `src/utils/is-safe-to-delete.ts` [01 §npkill, S19] | MIT — TypeScript, port lists + semantics |
| **node-prune** | yarn-derived in-`node_modules` prune lists (~70 file names, dirs, extensions; overridable `WithFiles/WithDirectories/WithExtensions/WithGlobs/WithExceptions`) for a future "slim node_modules in place" mode; `chan func()` deletion pool while walk stays serial | `internal/prune/prune.go` [01 §node-prune, S20] | MIT — historical snapshot (last push 2022-11-30) |
| **ncdu** | scan/export decoupling (`-O` binary export browsed later) = the plan→apply manifest shape; `CACHEDIR.TAG` honoring (`--exclude-caches`); `--shared-column` shared/unique hardlink UI; `-x` one-file-system; `-q` throttle model | C/Zig source in repo; behavior verified via official site + man page [01 §ncdu, S1, S2] | MIT [01 S1] |
| **Pearcleaner** | orphaned-file search heuristics (adjustable sensitivity, include/exclude dirs); Sentinel event-driven Trash monitoring; privileged-helper install pattern | `Sources/Sentinel`, search logic [02 §1, §Reusable code] | Apache-2.0 **+ Commons Clause v1.0** — read-only, never vendor (no "Sell" clause; project On Hold) |
| **GrandPerspective** | scan-result persistence format (save/reload); filter predicates; dataless/cloud-file awareness; TM hard-link analysis | Objective-C sources in src tgz [02 §1, §Reusable code] | GPL-2.0 — copy-with-care (viral); read, don't link |
| **Hazel** | rule-as-artifact UX: ordered first-match, per-rule enable/disable, test/preview runs, run-status view; Trash age/size policies; "Revert Files" | closed source — behavior reference only [02 §1, §4] | Proprietary — UX patterns only |
| **AppCleaner** | SmartDelete trash-interception (hook the existing deletion gesture); Trash-as-undo reliance | closed source — behavior reference [02 §4] | Freeware — UX patterns only |
| **Disk Drill** | per-item recovery-confidence framing ("rebuildable in ≈40 s") as UX vocabulary | closed source — vocabulary reference [02 §4] | Proprietary — framing only |
| **Mole** | whitelist/dry-run UX patterns; per-path cleanup precedent on macOS | repo — **ideas only** [03 §Reusable code, S29] | GPL-3.0 — no code copy |
| **cargo-cache** | per-component `~/.cargo` size accounting (`registry/cache/src`, `git/db`) for Rust-panel aggregates | `src/main.rs` + size walk modules [03 §Rust, S11] | MIT/Apache-2.0 [03 S11] |
| **OS primitives** | `renamex_np(2)` + `RENAME_EXCL` (fails `EEXIST` on destination-exists; `EXDEV` cross-device → per-volume quarantine; `EDEADLK` on dataless files under restrictive I/O policy); volume-capacity URLResourceKeys for honest free-space reporting; `tmutil thinlocalsnapshots` for snapshot-aware bytes-freed reporting | man pages [02 §3, S28–S32] | Apple APIs — no licensing issue; `libc` 0.2.189 already exports `renamex_np`/`RENAME_EXCL` bindings (`src/unix/bsd/apple/mod.rs` L3199-3200, L4634) |

---

## 4. Rule extraction — safety rules learned from each tool

1. **Staging without a recovery channel is theater** (DaisyDisk). Collector + 5 s countdown still ends in
   permanent unlink — files "cannot be 'undeleted'" [02 S4]. A confirm step, even a pretty one, is not an
   undo channel. ⇒ Our `apply`/`purge`/`restore` split with a 72 h grace window is the fix; the purge
   gate is a *typed byte count*, not a button [SPEC §3.2].
2. **"Permanently delete by default" is the ecosystem norm — treat it as the cautionary tale**
   (npkill shells `rm -rf` via `execFile` [01 §npkill]; node-prune, kondo, cargo-sweep, gdu-on-macOS all
   permanent-unlink). Only dua-cli defaults to Trash on macOS [01 §Risks 1]. ⇒ Reversibility must be the
   default path, not an opt-in flag.
3. **Trash is not a safe-deletion API** (gdu errors out on macOS; ncdu has no native Trash; Put-Back
   metadata format is undocumented [01 §gdu, S12; 02 §Risks 5]). ⇒ Our restore must record original
   paths in our own WAL, never rely on `.DS_Store`-style Trash metadata.
4. **Content-addressable stores are pruned only via their own GC** (uv: "it is *never* safe to modify the
   cache directly" — only whole-store `uv cache prune`/`clean` [03 §Python, S14]; npm `_cacache` never
   self-GCs but is integrity-verified and self-healing [03 §Node, S1]; pnpm store: only `pnpm store
   prune` [03 §Node, S4]; Homebrew: invoke `brew cleanup --prune=<days>` rather than re-implementing
   prefix logic [03 §Homebrew]; Docker: `docker system prune` inside the VM, never delete `Docker.raw`
   [03 §Docker]). ⇒ For CAS-structured dirs, our verdict must be an *invoked GC command*, not a file-level
   deletion plan — detect by hash-named children (`content-v2/sha512/…`, `archive-v0`, `files/`)
   [03 §0].
5. **Shred/secure-erase is an anti-pattern** (CleanMyMac "Remove Securely" is deliberately irreversible
   [02 §2.6]). ⇒ Deliberate irreversibility has no place in a tool whose promise is reversibility.
6. **Actively-written data refuses auto-action** (our own SPEC rule, sharpened by gdu's `--max-age`/
   `--min-age` scan-time gates [01 §gdu] and kondo's `last_modified` activity gating [01 §kondo]; npkill's
   `getRecentModificationInDir` is the same idea [01 §npkill]). mtime < 10 min ⇒ "in use", no auto-action
   [SPEC §3.1]. Quirk list [03 §Risks 7]: npm `_cacache` mtimes update on any install (always "hot") yet
   whole-store prune stays correct ⇒ the mtime rule gates *file-level* deletion, not GC-command
   proposals. Running dev servers hold file handles ⇒ file-lock signal (flock markers like
   `.package-lock.json`) ⇒ HandsOff until process exits [03 §0].
7. **Name match is a floor, never a verdict** (npkill's gate is literally `basename ∈ targets`
   [01 §npkill]; kondo/dua lists delete on name only [01 §Risks 4]; AppCleaner's bundle-id heuristics
   produce false-positive leftovers [02 §4]). ⇒ Ported lists are seed *candidates*; every candidate needs
   ≥1 strong evidence signal, and orphans (manifest absent above the dir) get promoted to a visible
   "project deleted?" prompt rather than auto-deleted [03 §0].
8. **Byte counts must be honest** (ncdu double-counts firmlinks/directory hardlinks — man-page BUGS
   [01 §ncdu]; pnpm store→node_modules and uv hardlinks mean deleting one of N links frees 0 bytes
   [03 §0]; Docker.raw is sparse — logical size ≫ allocated [03 §Docker]; APFS clones share blocks
   [01 §dua-cli]). ⇒ Ledger records allocated bytes (`st_blocks`), dedups `(dev,ino)`, discounts clones
   with `ATTR_CMNEXT_CLONEID`; freed-byte estimates are discounted accordingly [03 §Risks 6].
9. **Scan must never trigger materialization or damage** (dataless iCloud files: `renamex_np` →
   `EDEADLK` under restrictive I/O policy [02 §3]; Safari caches TCC-protected — treat unreadable as
   HandsOff [03 §Browsers]; scanner must not evoke/download iCloud placeholders [SPEC §7]). ⇒
   Dataless-detection flag + HandsOff class for anything unreadable.
10. **Zero-install / committed caches are contract, not junk** (Yarn Berry zero-install commits
    `.yarn/cache` to git [03 §Node, S5]). ⇒ Require `.git` presence + gitignore inspection before
    proposing anything inside a repo tree.
11. **Cargo's `bin/` and config are not caches** (`~/.cargo/bin` + `.crates.toml` — Cargo Book: "Do NOT
    edit by hand", install = recompile minutes ⇒ Expensive; `credentials.toml`/`config.toml` ⇒ HandsOff
    [03 §Rust, S9]). Similarly `~/.rustup/toolchains` = Expensive (hundreds of MB each, redownload not
    guaranteed forever) [03 §Rust]. ⇒ Distinguish cache-adjacent *state* from cache: same parent dir,
    different grade.
12. **Personal data hides inside junk-adjacent trees** (Docker containers/volumes are NOT regenerable
    [03 §Docker]; simulator device data loses personal install state even though devices are recreatable
    [03 §Xcode]; `.xcarchive` holds unregenerable dSYMs for symbolication [03 §Xcode, S16]; Swift
    Playgrounds content is user-authored [03 §Xcode]; browser profiles = HandsOff, only the `Caches`
    mirror is PureCache [03 §Browsers]; sandboxed app containers HandsOff unless targeting only their
    `Caches/` child [03 §8]). ⇒ Grade by *what* is lost, not by directory convention.
13. **Preferences can relocate standard paths** (DerivedData moved by `IDECustomDerivedDataLocation`
    [03 §Xcode, S15]; pnpm store volume-bound at `<volume>/.pnpm-store` [03 §Node, S3]; uv honors
    `UV_CACHE_DIR` [03 §Python, S14]). ⇒ Never hard-code home paths; resolve from prefs/env before
    trusting defaults.
14. **User-inspectable rules beat black-box DBs** (CleanMyMac's Safety Database is effective but not
    user-inspectable [02 §4]; Hazel proves users will author and preview rules if given the artifact
    [02 §4]). ⇒ Every verdict ships machine-derived evidence the user can inspect (the `Evidence` chain
    in the plan manifest [SPEC §3.1]) — this is also what "review-before-act" done right looks like.
15. **Keep memory between runs** (gdu's SQLite persistence [01 §gdu]; GrandPerspective's scan-result
    save/reload [02 §4]; ncdu's binary export [01 §ncdu]; vs OmniDiskSweeper's zero memory = "fastest
    failure mode" [02 §4]). ⇒ JSONL ledger as persistent 台账: verdicts, operations, cross-session
    trajectory [SPEC §3.2, R5].
16. **Deleting over a snapshot frees nothing yet** (TM local snapshots hold space counted as available;
    bytes freed only when snapshots thin/purge [02 §3, §Risks 2]). ⇒ Bytes-freed report queries
    `diskutil apfs listSnapshots` and warns.

---

## 5. Verdict table — per module

Read: **build** = implement fresh; **adapt-from-X** = port/port-the-idea (mechanism named); **library-Y**
= use the crate as-is. License flags from §3.

| SPEC module [SPEC §4] | Verdict | Basis |
|---|---|---|
| `scan/` walker | **adapt-from-dua-cli** (`getattrlistbulk` macOS walker + APFS clone/firmlink flags), with **diskus/dust as fallback skeleton** (`par_iter().for_each_with(channel)` + accumulator, or `par_bridge()`) | dua's walker is byte-for-byte reuse-worthy [01 §dua-cli]; diskus's shape is the drop-in alternative [01 §diskus]; dust warns naive recursion kills rayon parallelism [01 §dust] |
| scan parallelism | **library-crossbeam** (deque work-stealing) or **library-rayon** (`par_bridge`) | dua pattern [01 §dua-cli]; dust idiom [01 §dust]; `jwalk` = generic fallback, no longer used by dua itself [01 §Recommended crates] |
| hardlink/clone accounting | **adapt-from-dua-cli** (`ATTR_CMNEXT_CLONEID`, `EF_MAY_SHARE_BLOCKS`, `SF_FIRMLINK`) + **gdu's `Nlink>1→ino`** dedup + **dust/diskus** `(dev,ino)` dedup | [01 §dua-cli §gdu §dust §diskus] |
| mtime/activity gating | **adapt-from-gdu** (`--max-age`/`--min-age` scan-time filters) + **kondo** `last_modified` | the primitive under SPEC's 10-min rule [01 §gdu; 03 §Risks 7] |
| gitignore excludes | **library-gix** (`excludes` feature) | dua's own choice [01 §dua-cli] |
| `classify/` rule engine | **build** (evidence chains, rebuild-cost grading), seeded with **adapt-from-kondo** (23-ecosystem dir tables + marker-file detection) + **adapt-from-npkill** (profile lists) + **adapt-from-dua-cli** (bisect list) + **adapt-from-cargo-sweep** (rustc-fingerprint → toolchain evidence; reimplemented, not pinned) + [03 §0] signal table | name lists are seed data; evidence grading is the differentiator [01 §Risks 4; SPEC §3.1] |
| ecosystem verdicts (per-path tiers) | **adapt-from-03** — the [03 §1–§8] taxonomy tables *are* the rule data (Node, Rust, Python, Xcode, Homebrew, Docker, Browsers, XDG/Caches) | verified regen costs + tiers per path |
| per-ecosystem GC invocation | **build** (thin exec wrappers) over the tools' own commands (`pnpm store prune`, `uv cache prune`, `pip cache purge`, `brew cleanup --prune=`, `docker system prune`, `xcrun simctl delete unavailable`) | [03 §1–§6] — never re-implement store internals |
| Cargo workspace/`target/` enumeration | **library-cargo_metadata** | [01 §Recommended crates; 03 §Recommended crates] |
| `plan/` manifest | **build**, shape borrowed from **ncdu** `-O` export decoupling + **DaisyDisk** Collector staging + **Hazel** rule-preview (ordered, previewable) | [01 §ncdu; 02 §4] |
| `execute/` quarantine | **build** — `renamex_np(RENAME_EXCL)` via **library-libc/nix** (bindings already exported, `src/unix/bsd/apple/mod.rs` L3199-3200); per-volume quarantine dir (`EXDEV` risk) | [02 §3, S32, §Reusable code; SPEC §7] |
| execute/ Trash fallback | **library-trash** (5.2.6, MIT; `NSFileManager.trashItemAtURL` backend) | dua's choice; cross-volume caveat → fallback only [01 §Risks 1, S9] |
| WAL / crash recovery | **build** | no surveyed tool has one; plumbing via **library-crossbeam-channel** (diskus's channel shape) [01 §diskus] |
| `ledger/` persistent JSONL | **build**, concept validated by **gdu** SQLite persistence + **GrandPerspective** scan-result save/reload | [01 §gdu; 02 §4] |
| `policy/` NL → constraints | **build**; goal-driven flag precedent = **cargo-sweep** `--maxsize` | "free 25 GB" ≈ `--maxsize` shrinking [01 §cargo-sweep] |
| `agent/` loop | **build** | no analog in any surveyed tool (all are one-shot batch/interactive, not agent loops) [SPEC §2 LLM row] |
| `ui/` | ~~**library-ratatui + crossterm**~~ (TUI verdict — **SUPERSEDED 08-31 by the GUI pivot, ADR-5/18: Tauri + Svelte desktop App per UX-SPEC.md**); progress patterns **adapt-from-dua-cli** (throttled `TraversalProgress`, live `TraversalStats`) + **dust** (spinner+atomic counters) — the *streaming-progress discipline* carries over to the GUI via `ipc::Channel` [12]; staged-mark UX **adapt-from-dua-cli** (mark→pane→confirm — reborn as Review→Plan→Purge-ceremony screens); evidence-review + ledger-trajectory panes **build** | [01 §Recommended crates, §dua-cli, §dust; 12] |
| `config/` | **build** (serde/serde_json for manifest+ledger), XDG config precedent from **dust** (`$XDG_CONFIG_HOME/dust/config.toml`) | [02 §Recommended crates; 01 §dust] |
| free-space / purgeable reporting | **build** over **library-objc2-foundation** volume-capacity keys (`volumeAvailableCapacityForImportantUsageKey`) — `statfs` alone is wrong | [02 §3, S28, S29] |
| snapshot awareness (bytes-freed honesty) | **build** over `tmutil`/`diskutil apfs listSnapshots` | [02 §3, §Risks 2] |
| dataless-file detection | **build** over `lstat`/`getattrlist` (`SF_DATALESS 0x40000000`, verified in SDK headers [05 §6]; `st_blocks==0` heuristic); never trigger materialization | [05 §6; 02 §Risks 3; SPEC §7] |
| Trash/watcher interception (bonus, later) | **adapt-from-Pearcleaner Sentinel** (behavior; **library-notify** CC0-1.0 as the FS-event crate) | [02 §1, §4, §Recommended crates] |
| orphaned-file search (bonus) | **adapt-from-Pearcleaner/AppCleaner heuristics** (ideas — Commons Clause / closed), then upgrade to evidence | [02 §4] |
| app-uninstall mode | **out of scope** — AppCleaner/Pearcleaner own it; Disk Janitor services disk-cleanup only | [SPEC §1 boundary] |

**License totals:** MIT-adaptable: dua-cli, trash-rs, gdu, kondo, npkill, node-prune, ncdu, cargo-sweep,
cargo-cache, libc/nix, ratatui/crossterm, gix, crossbeam(-channel), rayon, cargo_metadata, walkdir,
jwalk, serde, sysinfo, objc2-foundation, trash. Dual MIT/Apache: diskus. Apache-single (NOTICE if
vendored): dust. Apache+Commons Clause (read-only): Pearcleaner. GPL-2.0 (no code): GrandPerspective.
GPL-3.0 (ideas only): Mole. Proprietary/closed (behavior reference): CleanMyMac, DaisyDisk,
AppCleaner, OmniDiskSweeper, Hazel, Disk Drill, Apple built-ins.

---

*Compiled 2026-08-30 from the three research notes; every license re-verified at source by the research
lanes on the same date. Facts marked ⚠ or "unconfirmed"/"verify" are open items carried into
`docs/SPEC.md` §7 (open questions) and must be resolved before citing in DESIGN.md.*
