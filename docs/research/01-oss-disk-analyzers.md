# Open-source disk analyzers & dev-junk cleaners — Research Note
Comparative scan/delete/trash architecture for the Disk Janitor "oss-analyzers" lane · researched 2026-08-30

Scope: ncdu, gdu, dua-cli, dust, diskus, kondo, cargo-sweep, npkill, node-prune. Every license/claim was read from the primary repo file (LICENSE/Cargo.toml/go.mod/package.json) or official docs on 2026-08-30 — never memory. Versions are latest release as of 2026-08.

## Findings

### Master comparison

| Tool | License | Lang | Version (2026-08) | Scan model | Delete | Trash |
|---|---|---|---|---|---|---|
| [ncdu](https://dev.yorhel.nl/ncdu) | MIT | C (1.x) / Zig (2.x) | 2.9.2 Zig, 1.22 C | threaded (`-t`, default 1) | unlink (`d`) | only via `--delete-command` |
| [gdu](https://github.com/dundee/gdu) | MIT | Go 1.25 | v5.37.0 | goroutine pool, `2×GOMAXPROCS` | `os.RemoveAll` | ✗ on macOS ([`trash_darwin.go`](https://github.com/dundee/gdu/blob/master/pkg/remove/trash_darwin.go)) |
| [dua-cli](https://github.com/Byron/dua-cli) | MIT | Rust (ed. 2024) | 2.43.1 (`dua-core` 3.3.0) | work-stealing pool; macOS `getattrlistbulk` | recursive unlink (parallel) | ✓ `trash` crate 5.2.0 (**default** feat) |
| [dust](https://github.com/bootandy/dust) | Apache-2.0 | Rust (ed. 2024) | 1.2.5 (`du-dust`) | `rayon::ParallelBridge` over `read_dir` | — read-only | — |
| [diskus](https://github.com/sharkdp/diskus) | MIT OR Apache-2.0 | Rust (ed. 2021) | 0.9.0 | `rayon::par_iter` + `crossbeam-channel` | — read-only | — |
| [kondo](https://github.com/tbillington/kondo) | MIT | Rust | 0.9.0 (`kondo-lib`) | `walkdir`/`ignore`, single-thread | `remove_dir_all` | ✗ |
| [cargo-sweep](https://github.com/holmgr/cargo-sweep) | MIT | Rust (ed. 2021) | 0.8.0 | `walkdir` + `cargo_metadata` | `remove_dir_all`/`remove_file` | ✗ (has `--dry-run`) |
| [npkill](https://github.com/voidcosmos/npkill) | MIT | TypeScript/Node | 0.12.2 | `worker_threads` pool (≤8) | `rm -rf` via `execFile` | ✗ |
| [node-prune](https://github.com/tj/node-prune) | MIT | Go 1.13 | v1.2.0 | `filepath.Walk` (single-thread) | `os.RemoveAll`/`os.Remove` | ✗ |

Licenses verified directly: dua-cli [`LICENSE`](https://github.com/Byron/dua-cli/blob/main/LICENSE) "Copyright (c) 2019 Sebastian Thiel"; gdu [`LICENSE.md`](https://github.com/dundee/gdu/blob/master/LICENSE.md) "Copyright 2020-2021 Daniel Milde"; dust `Cargo.toml` `license = "Apache-2.0"`; diskus `Cargo.toml` `license = "MIT OR Apache-2.0"`; kondo `kondo/Cargo.toml` + `kondo-lib/Cargo.toml` `license = "MIT"`; cargo-sweep `Cargo.toml` `license = "MIT"`; npkill `package.json` `"license": "MIT"`; node-prune `LICENSE` "Copyright (c) 2017 TJ Holowaychuk"; ncdu official site states "License: MIT". dust is single Apache-2.0 (not dual) — copy-with-care: keep the LICENSE/NOTICE in any vendored code; Apache-2.0 is university-push-safe.

### ncdu — the durable baseline

- **Scan**: ncdu 2.5 added parallel scanning with `-t N` ([release notes](https://dev.yorhel.nl/ncdu)); **off by default** (default 1 thread). Single-thread JSON export runs in very little memory; multi-thread mode builds the whole tree in memory first. ncdu 2.6 added a binary export format (`-O`) that supports low-memory multi-threaded export and browsing trees larger than RAM ([man page](https://dev.yorhel.nl/ncdu/man)).
- **Symlinks/hardlinks**: `-L/--follow-symlinks` follows symlinks to **files** (counts target size; each symlink counts as a unique file) but **not** to directories. Hard links counted once (`H` flag = "Same file was already counted"). **Directory hard links and macOS firmlinks are NOT supported** — they are scanned and counted multiple times (man page BUGS). `--shared-column {off,shared,unique}` shows shared/unique sizes for dirs with hardlinks. `-x/--one-file-system` constrains to one device.
- **gitignore**: none — uses `--exclude pattern` / `-X/--exclude-from file` (newline patterns, not gitignore syntax). `--exclude-caches` honors `CACHEDIR.TAG`.
- **Progress**: `-0` silent, `-1` progress-line (no full UI), `-2` full-screen ncurses during scan; `-q/--slow-ui-updates` throttles 10/s → once/2s.
- **Delete**: built-in `d` key (enabled by default on a live scan, disabled on import). `--confirm-delete` (default on), `--disable-delete`, `-r` (read-only = disable delete), `-rr` (also disable shell). **No native Trash** — `--delete-command 'gio trash --'` is the documented way to route to trash; permanent unlink otherwise.
- **Borrow**: (1) scan/export decoupling — `-O` binary export browsed later is the exact shape of Disk Janitor's plan→apply manifest; (2) `CACHEDIR.TAG` honoring; (3) `--shared-column` hardlink accounting UI; (4) `-x` one-file-system. Limitation to flag in our ledger: ncdu's firmlink/directory-hardlink blind spot is precisely a macOS edge we must handle better (see dua-cli below).

### gdu — mtime filtering + persistent analysis

- **Scan**: `pkg/analyze/parallel.go` — `concurrencyLimit = make(chan struct{}, 2*runtime.GOMAXPROCS(0))` semaphore; each subdir spawned as `go a.processQueuedDir(...)`. `--sequential` picks a single-thread analyzer for HDDs. Non-interactive mode uses a **memory-efficient analyzer that tracks only top-level totals** (constant memory) unless `--top`/`--depth` force the full tree. Built-in `os.ReadDir`. TUI = `rivo/tview` + `gdamore/tcell`; also a web UI (`--web`) and SQLite/BadgerDB persistence (`--db`, `-r` read-back).
- **Symlinks/hardlinks**: `-L/--follow-symlinks` follows file symlinks only (not dirs), via `followSymlink()`. Hardlinks counted once: [`dir_unix.go`](https://github.com/dundee/gdu/blob/master/pkg/analyze/dir_unix.go) — `if stat.Nlink > 1 { file.Mli = stat.Ino }`; README "Hard links are counted only once"; `H` flag. Usage = `stat.Blocks * 512` (FTS-style block accounting).
- **mtime filters (directly relevant to our evidence engine)**: `--max-age DURATION`, `--min-age DURATION` (e.g. `7d`, `2h30m`), `--since`/`--until` (RFC3339 or `YYYY-MM-DD`). These are *scan-time* filters — the exact primitive for "mtime < 10 min ⇒ actively written ⇒ refuse auto-action."
- **Progress**: live progress bar in TUI; `-p/--no-progress` disables; `--max-cores` caps threads (default 8).
- **Delete**: `d` delete (`os.RemoveAll`, permanent), `e` empty dir, and an `ActionMoveToTrash` action — **but on macOS trash is unsupported**: [`pkg/remove/trash_darwin.go`](https://github.com/dundee/gdu/blob/master/pkg/remove/trash_darwin.go) `MoveItemToTrash` returns `"move to trash is not supported on macOS"` (Finder ignores XDG trash). `askBeforeDelete` confirm; `--no-delete` prevents writes. Experimental `delete-in-parallel` / `delete-in-background` config flags.
- **Borrow**: `--max-age`/`--min-age` scan-time mtime gating; constant-memory non-interactive top-dir mode; SQLite analysis persistence for cross-session memory (maps to our ledger); concurrency-semaphore pattern; archive browsing (zip/tar) is out-of-scope for us but the mtime filter is gold.

### dua-cli — the macOS-native walker + staged delete (most relevant)

- **Scan**: NOT `jwalk` (old versions were) — current 2.43 has a **bespoke work-stealing walker** in [`crates/dua-lib/src/lib.rs`](https://github.com/Byron/dua-cli/blob/master/crates/dua-lib/src/lib.rs): `crossbeam::deque::{Injector, Stealer, Worker}` + `Parker/Unparker`; `walk(roots, threads, order, options, descend)`; threads default = logical processors (`--threads`, 0 = logical). `Order::ParentFirst` vs `Order::Completion`. **On macOS it uses `getattrlistbulk(2)`** ([`crates/dua-lib/src/macos/mod.rs`](https://github.com/Byron/dua-cli/blob/master/crates/dua-lib/src/macos/mod.rs)) — bulk directory enumeration returning packed metadata (name, type, size, mtime, inode, clone-id) in the readdir record, **no per-entry `stat` syscall**. This is the single most transferable asset for a macOS-only tool.
- **APFS clones (unique, important)**: `--deduplicate-apfs-clones` (opt-in, ~6% slower) uses `ATTR_CMNEXT_CLONEID` + `EF_MAY_SHARE_BLOCKS` (`0x1`) extended flags to count fully-shared APFS clones once; tracks `SF_FIRMLINK` (`0x0080_0000`) and `STAT_BLOCK_BYTES = 512` ([`macos/attributes.rs`](https://github.com/Byron/dua-cli/blob/master/crates/dua-lib/src/macos/attributes.rs)). This is the macOS-aware answer to ncdu's firmlink blind spot. `--apparent-size` reports logical length.
- **Symlinks/hardlinks**: "Symbolic links are reported but never followed" (lib.rs). Hardlinks: `count_hard_links` bool in `WalkOptions` — true counts each occurrence; false dedups by `(dev, inode)` (interactive refresh sets it true).
- **gitignore**: `--ignore-from FILE` reads gitignore-style patterns via the **`gix`** crate (features `excludes`,`sha1`,`sha256`); full gitignore syntax (`#`, trailing `/`, `**`, `!`). Interactive `gitignored_entries` ([`app/gitignore.rs`](https://github.com/Byron/dua-cli/blob/master/src/interactive/app/gitignore.rs)) uses `gix::ignore::Kind` to mark gitignored entries in the TUI.
- **Progress**: aggregate mode = throttled `TraversalProgress` writing `"Enumerating N items\r"` (100 ms throttle, [`aggregate.rs`](https://github.com/Byron/dua-cli/blob/master/src/aggregate.rs)); interactive = live `TraversalStats` counters in `AppState`.
- **Delete model (the model to copy)**: two `MarkMode`s ([`widgets/mark.rs`](https://github.com/Byron/dua-cli/blob/master/src/interactive/widgets/mark.rs)): `Delete` → `delete_directory_recursively` (walks with `dua_core::walk`, collects files+dirs, removes files with N threads via `thread::scope`+`AtomicUsize` index, then removes dirs **deepest-first** so each `remove_dir` sees an empty dir); `Trash` (feature `trash-move`, **default-on**) → `trash::delete(path)` via the [`trash` crate 5.2.0](https://github.com/Byron/trash-rs). Multi-stage: mark → Mark pane → dispatch → confirm; README: "great care has been taken to prevent accidental deletions due to a multi-stage process." Config can disable `delete_marked`/`trash_marked` (empty array).
- **Naming-based cleanup (matches SPEC's claim)**: [`app/cleanup.rs`](https://github.com/Byron/dua-cli/blob/master/src/interactive/app/cleanup.rs) — `CLEANUP_DIR_NAMES_SORTED_FOR_BISECT` = `[".mypy_cache",".pytest_cache",".ruff_cache",".tox",".venv","__pycache__","node_modules","target","venv"]` — binary-searched name whitelist for "mark cleanup candidates." This is the "naming-based, deliberately conservative" approach Disk Janitor explicitly wants to beat with evidence chains.
- **Borrow**: macOS `getattrlistbulk` walker (byte-for-byte reuse-worthy); crossbeam work-stealing pool; APFS clone/firmlink handling; `gix`-based gitignore; the mark-pane→confirm staged delete; `trash` crate integration; `--deduplicate-apfs-clones`; `--ignore-from`.

### dust — rayon simplicity, inode dedup

- **Scan**: [`src/dir_walker.rs`](https://github.com/bootandy/dust/blob/master/src/dir_walker.rs) — recursive `walk()` over `fs::read_dir` piped through `rayon::iter::ParallelBridge` (`par_bridge()`); comment notes they unraveled the recursion because "rayon doesn't parallelize as well" with naive recursion (≈3× drop). `WalkData` carries regex/mtime/filesystem filters + `use_apparent_size` + `ignore_hidden` + `follow_links`.
- **Symlinks/hardlinks**: `-L/--dereference-links` treats symlinks as dirs and descends. Hardlinks: post-pass `clean_inodes` dedups by `(device, inode)` HashSet (only when not apparent-size) — "Remove files which have the same inode, we don't want to double count them."
- **gitignore**: none. `-X NAME` (name match), `-e`/`-v` regex include/exclude, `-i` hide dotfiles. Config file at `$XDG_CONFIG_HOME/dust/config.toml`.
- **Progress**: [`src/progress.rs`](https://github.com/bootandy/dust/blob/master/src/progress.rs) — dedicated spinner thread (`['-','\\','|','/']`, 100 ms sleep) + atomic counters (`num_files`, `total_file_size`); `-P` disables. Prints at most one permissions error.
- **Delete**: none — visualization only.
- **Borrow**: the `par_bridge()`-over-`read_dir` idiom is the cheapest correct parallelism; inode-dedup HashSet; spinner+atomic-counter progress pattern; `-t` group-by-filetype. Apache-2.0 only (single license): attribute if vendoring.

### diskus — the minimal `du -sh` (cleanest parallel skeleton)

- **Scan**: [`src/walk.rs`](https://github.com/sharkdp/diskus/blob/master/src/walk.rs) — `entries.into_par_iter().for_each_with(tx, …)` where `tx` is an unbounded `crossbeam_channel` sending `Message::SizeEntry(unique_id, size)` to a single accumulator thread. **`default_num_workers() = 3 × available_parallelism`, capped at 64** — comment: cold-cache wants more threads for IO scheduling; warm-cache pays sync overhead.
- **Symlinks/hardlinks**: `symlink_metadata()` (never follows). [`src/unique_id.rs`](https://github.com/sharkdp/diskus/blob/master/src/unique_id.rs) — `UniqueID { device, inode }` generated **only when `is_file() && nlink() > 1`**, deduped in the accumulator (Unix; Windows deliberately counts duplicates to match Explorer/`dir`).
- **gitignore/progress**: none / none (completes in seconds). `Directories::{Auto,Included,Excluded}` matches `du` semantics (Auto: dirs counted for disk-usage, excluded for apparent-size).
- **Delete**: none.
- **Borrow**: the `par_iter().for_each_with(channel)` + single-accumulator shape is a drop-in scan skeleton if we don't go native-`getattrlistbulk`; the 3×-cores rationale; `Directories::Auto` semantics.

### kondo — per-ecosystem artifact-dir rules (the ruleset reference)

- **Scan**: [`kondo-lib/src/lib.rs`](https://github.com/tbillington/kondo/blob/master/kondo-lib/src/lib.rs) — single-threaded. Project *detection* iterates `walkdir::WalkDir` (with `ScanOptions { follow_symlinks, same_file_system }`) looking for marker files; *size/`last_modified`* use the **`ignore` crate** `WalkBuilder` (honors `.gitignore` by default, configurable follow/same-fs). `dir_size` sums `metadata().len()` per file. `size_dirs` splits `artifact_size` vs `non_artifact_size` per top-level dir.
- **Per-ecosystem rules (23 types)**: `ProjectType` enum — Cargo, Node, Unity, Stack, Cabal, SBT, Maven, Gradle, CMake, Unreal, Jupyter, Python, Pixi, Composer, Pub, Elixir, Swift, Zig, Godot4, Dotnet, Turborepo, Terraform, Cocoapods. Each maps to a `PROJECT_*_DIRS` const: Cargo→`["target",".xwin-cache"]`; Node→`["node_modules",".angular"]` (React-Native variant adds `android/build`, `ios/DerivedData`, `ios/Pods`, …); Python→`["__pycache__",".mypy_cache",".pytest_cache",".ruff_cache",".tox",".nox","__pypackages__"]`; Swift→`[".build",".swiftpm"]`; Zig→`["zig-cache",".zig-cache","zig-out"]`; Dotnet→`["bin","obj"]`; CocoaPods→`["Pods"]`; Terraform→`[".terraform"]`. Detection by marker file constants (`Cargo.toml`, `package.json`, `Package.swift`, `build.zig`, `Podfile`, …); React-Native detected by `ios/` or `android/` subdir presence.
- **Delete**: `Project::clean()` → `fs::remove_dir_all(&artifact_dir)` per artifact dir — **permanent, no trash, no dry-run** (the GUI variant adds its own confirm). Version note: `kondo`/`kondo-lib` 0.9.0, `kondo-ui` 0.8.0 (Bevy 0.18 + `rfd` — a game engine as UI, an interesting-but-heavy choice we should NOT copy for ratatui).
- **Borrow**: the artifact-dir rules table verbatim as seed data for our `classify/` rule engine (then upgrade names → evidence chains: marker file + lockfile + mtime); marker-file project detection; artifact/non-artifact size split; `ignore::WalkBuilder` `last_modified` for activity gating.

### cargo-sweep — fingerprint-aware target/ cleaning

- **Scan**: [`src/fingerprint.rs`](https://github.com/holmgr/cargo-sweep/blob/master/src/fingerprint.rs) — `walkdir::WalkDir` over `target/` (workspace located via `cargo_metadata`). Fingerprint matching replicates **Cargo's own rustc-version hashing**: `rustc_stable_hash::StableSipHasher128` (Rust ≥1.85) plus the legacy `SipHasher` pre-1.85; `hash_from_path_name` parses Cargo's 16-hex-char suffix to attribute each artifact to the toolchain that built it. ⚠️ README banner: **"currently unmaintained"** (suggests `cargo-clean-all` as alternative) — do not treat as a live dependency.
- **Modes**: `--time N` (mtime older than N days), `--installed` / `--toolchains` (keep artifacts of installed toolchains only), `--maxsize 10GiB` (shrink until under limit), `--recursive` (sweep every Cargo project under a root), `--dry-run`.
- **Delete**: `remove_dir_all`/`remove_file`, permanent; `--dry-run` for preview; no trash.
- **Borrow**: the fingerprint→toolchain mapping as evidence ("this `target/` predates your installed toolchain → rebuildable"); `--maxsize` goal-driven shrinking mirrors our "free 25 GB" policy compiler; `--recursive` workspace sweep.

### npkill — worker-pool + profile target lists (TypeScript)

- **Scan**: [`src/core/services/files/`](https://github.com/voidcosmos/npkill/tree/master/src/core/services/files) — `worker_threads` pool; `getOptimalNumberOfWorkers()` = `cores > MAX_WORKERS(8) ? 8 : cores - 1` (≥1). Each worker (`files.worker.ts`) runs a `FileWalker` with `opendir`/`lstat`/`readdir` over a task queue (`explore`, `getFolderSize`), `MAX_PROCS = 100` in-flight tasks per worker. Targets matched by **basename** against profile lists: [`profiles.constants.ts`](https://github.com/voidcosmos/npkill/blob/master/src/core/constants/profiles.constants.ts) — `node` (24 targets: `node_modules`, `.next`, `.nuxt`, `.turbo`, `.parcel-cache`, `.eslintcache`, …), `python` (12), `data-science`. `getRecentModificationInDir` skips `node_modules/.git/coverage/dist` to find newest mtime.
- **Symlinks/hardlinks**: `lstat` (no follow); no hardlink handling. No gitignore.
- **Progress**: `SpinnerService` frames + `ScanStatus { pendingSearchTasks, completedSearchTasks, workersJobs }`; live TUI list with size bars.
- **Delete**: [`unix-files.service.ts`](https://github.com/voidcosmos/npkill/blob/master/src/core/services/files/unix-files.service.ts) — `execFile('rm', ['-rf', path])` — **permanent, shells out to rm, no trash**. Safety gate [`is-safe-to-delete.ts`](https://github.com/voidcosmos/npkill/blob/master/src/utils/is-safe-to-delete.ts) is just `basename ∈ targets`. `--delete-all` (with warning confirm), `--yes` (skip confirm), `--targets` (restrict), space-bar delete in TUI.
- **Borrow**: profile-shaped target lists (complements kondo); worker-pool with bounded in-flight tasks; `--delete-all`/`--yes`/`--targets` flag surface; the basename-gate as a *minimum* safety floor (ours adds evidence + quarantine).

### node-prune — yarn-derived prune lists (Go, unmaintained-ish)

- **Scan**: [`internal/prune/prune.go`](https://github.com/tj/node-prune/blob/master/internal/prune/prune.go) — single-threaded `filepath.Walk` over `node_modules` (Go's `Walk` uses Lstat — does not follow symlinks). `Pruner.startN(runtime.NumCPU())` workers drain a `chan func()` so **deletions** run in a pool while the walk stays serial. Rule lists: `DefaultFiles` (~70 names: lint configs, `LICENSE`, `AUTHORS`, `tsconfig.json`, …, comment: "Copied from yarn (mostly)"), `DefaultDirectories` (`test/tests/docs/.github/coverage/…`), `DefaultExtensions` (`.md`, `.ts`, `.coffee`, `.tgz`, `.swp`); overridable via `WithFiles/WithDirectories/WithExtensions/WithGlobs/WithExceptions`.
- **Progress/delete**: final stats only (`files total/removed`, `size removed`, `duration`); `os.RemoveAll`/`os.Remove`, permanent, no trash, no dry-run. Repo last push 2022-11-30 (low activity); latest tag v1.2.0.
- **Borrow**: the yarn-derived "safe-to-prune *inside* node_modules" lists as a curated starting ruleset for a future "slim node_modules in place" mode (distinct from deleting whole trees); the channel-worker-pool idiom.

## Reusable code & licenses

| Repo | License (verified) | What to borrow | Where (file/module) |
|---|---|---|---|
| [Byron/dua-cli](https://github.com/Byron/dua-cli) | MIT | macOS `getattrlistbulk` walker; APFS clone/firmlink handling; work-stealing pool; staged mark→confirm delete; gitignore marking | `crates/dua-lib/src/{lib.rs,macos/mod.rs,macos/attributes.rs}`; `src/interactive/app/{handlers.rs,cleanup.rs,gitignore.rs}` |
| [Byron/trash-rs](https://github.com/Byron/trash-rs) (`trash` 5.2.6) | MIT | macOS Trash move (the reversible primitive our quarantine move complements) | `src/macos/mod.rs` — `NSFileManager::defaultManager()` + `trashItemAtURL` via `objc2_foundation` |
| [dundee/gdu](https://github.com/dundee/gdu) | MIT | `--max-age`/`--min-age` mtime scan filters; SQLite analysis persistence; constant-memory top-dir mode | `pkg/analyze/{parallel.go,dir_unix.go,sqlite.go}` |
| [bootandy/dust](https://github.com/bootandy/dust) | Apache-2.0 | `par_bridge()` walk idiom; `(dev,ino)` dedup; spinner+atomic progress | `src/{dir_walker.rs,progress.rs}` |
| [sharkdp/diskus](https://github.com/sharkdp/diskus) | MIT OR Apache-2.0 | `par_iter().for_each_with(channel)` + accumulator; 3×-cores heuristic; `UniqueID{nlink>1}` | `src/{walk.rs,unique_id.rs}` |
| [tbillington/kondo](https://github.com/tbillington/kondo) | MIT | 23-ecosystem artifact-dir table; marker-file project detection; `ignore::WalkBuilder` `last_modified` | `kondo-lib/src/lib.rs` |
| [holmgr/cargo-sweep](https://github.com/holmgr/cargo-sweep) | MIT | rustc-fingerprint→toolchain mapping; `--maxsize` goal shrinking | `src/fingerprint.rs` (⚠️ unmaintained) |
| [voidcosmos/npkill](https://github.com/voidcosmos/npkill) | MIT | profile target lists; bounded worker pool; flag surface (`--delete-all/--yes/--targets`) | `src/core/{constants/profiles.constants.ts,services/files/*}` |
| [tj/node-prune](https://github.com/tj/node-prune) | MIT | yarn-derived in-`node_modules` prune lists | `internal/prune/prune.go` |
| [Yorhel/ncdu](https://code.blicky.net/yorhel/ncdu/) | MIT | CACHEDIR.TAG honoring; scan/export decoupling (`-O` binary); shared/unique hardlink column | official site + [man page](https://dev.yorhel.nl/ncdu/man) (C source in repo, Zig 2.x branch) |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| [`trash`](https://crates.io/crates/trash) (5.2.6, MIT) | Move to macOS Trash | macOS backend = `NSFileManager.trashItemAtURL` (objc2); the reversible-delete primitive; pairs with our same-volume quarantine `renamex_np` | manual `objc2-foundation` calls |
| [`gix`](https://crates.io/crates/gix) (`excludes`) | gitignore-style excludes | dua-cli uses it for `--ignore-from` + TUI gitignore marking; full gitignore syntax | `ignore` crate's gitignore mode |
| [`ignore`](https://crates.io/crates/ignore) | gitignore-aware walking | `WalkBuilder` honors `.gitignore`, `follow_links`, `same_file_system` (kondo's `last_modified`) | `walkdir` (no gitignore) |
| [`crossbeam`](https://crates.io/crates/crossbeam) (`deque`) | work-stealing walker | dua's `Injector/Stealer/Worker` + `Parker/Unparker` pattern | `rayon` |
| [`rayon`](https://crates.io/crates/rayon) | simple parallel walk | `par_bridge()` over `read_dir` (dust) or `for_each_with(channel)` (diskus) | `crossbeam` |
| [`crossbeam-channel`](https://crates.io/crates/crossbeam-channel) | scan→accumulator pipe | diskus's unbounded `Message` channel; also our WAL/event plumbing | `std::sync::mpsc` |
| [`cargo_metadata`](https://crates.io/crates/cargo_metadata) | find Cargo workspaces/target | for `RebuildableFromSource` evidence on `target/` dirs | hand-rolled `Cargo.toml` parse |
| [`ratatui`](https://crates.io/crates/ratatui) 0.30 + [`crossterm`](https://crates.io/crates/crossterm) 0.29 | TUI | dua-cli's exact TUI stack; proven at dua's scale | `tview`-style (Go only) |
| [`nix`](https://crates.io/crates/nix) / `libc` | syscalls | `getattrlistbulk(2)`, `renamex_np(RENAME_EXCL)`; dua uses raw `libc::attrlist` structs | — |
| [`jwalk`](https://crates.io/crates/jwalk) | parallel walker (fallback) | **not used by current dua** (bespoke walker now); fine generic option | own crossbeam pool |

## Risks & open questions

1. **macOS trash is the weak spot of every surveyed tool** — gdu errors out on macOS; npkill/node-prune/kondo/cargo-sweep permanently unlink. Only dua-cli (via `trash`) trashes on macOS. Our quarantine-move + Trash two-layer reversibility is genuinely novel here — but `NSFileManager.trashItemAtURL` moves *across volumes* to `~/.Trash` (not same-volume), so for large trees our `renamex_np` quarantine must remain the fast path and Trash the fallback. Exact symbol: `-[NSFileManager trashItemAtURL:resultingItemURL:error:]`.
2. **`getattrlistbulk` licensing**: dua's macOS walker is MIT — clean to adapt — but it hand-rolls Darwin constants absent from `libc` (`SF_FIRMLINK = 0x0080_0000`, `EF_MAY_SHARE_BLOCKS = 0x1`, `ATTR_CMNEXT_CLONEID`, vnode types from `<sys/vnode.h>`). Verify each constant against XNU headers (`vfs_attrlist.c` `LIST_DIR_ATTRS`) when we vendor; dua's comment already documents the FTS-contract gate.
3. **Directory hardlinks / firmlinks**: ncdu double-counts them (documented BUGS). macOS `/Data` firmlinks make this real on Catalina+ volumes. dua tracks `SF_FIRMLINK`; we need an explicit policy (skip? count once? mark HandsOff?) — open.
4. **kondo/dua name-lists vs our evidence chains**: kondo's 23-ecosystem table and dua's 9-name bisect list are seed data, but both delete on **name match only** (npkill's gate is literally `basename ∈ targets`). Porting them into our `Evidence`-graded engine is the core differentiator; risk = over-trusting the ported lists.
5. **cargo-sweep unmaintained** (README banner) — its fingerprint hash logic is still the best reference for "which toolchain built this artifact", but `rustc_stable_hash` tracking must track Cargo's evolving hash (pre/post 1.85 split already exists). Open: is a maintained fork worth pinning, or reimplement the ~80-line matcher?
6. **npkill shells out to `rm -rf`** — a reminder that "permanent delete by default" is the ecosystem norm; our typed byte-count purge gate is the counter-example to demo. node-prune & npkill show low maintenance activity (2022-2024 last releases); treat their rule lists as historical snapshots, not living standards.

## Sources

1. https://dev.yorhel.nl/ncdu — ncdu official site: MIT license, versions 2.9.2 (Zig, 2025-10-24) / 1.22 (C LTS, 2025-03-05), 2.5 parallel scanning, 2.6 binary export · accessed 2026-08-30
2. https://dev.yorhel.nl/ncdu/man — Ncdu 2.9 Manual: `-t`, `-O`, `-L` symlink semantics, `H` flag, firmlink/directory-hardlink BUGS, `--delete-command`, `--exclude-caches`/CACHEDIR.TAG, `-r` read-only · accessed 2026-08-30
3. https://github.com/Byron/dua-cli/blob/main/LICENSE — dua-cli MIT license text ("Copyright (c) 2019 Sebastian Thiel") · accessed 2026-08-30
4. https://github.com/Byron/dua-cli/blob/master/Cargo.toml — dua-cli 2.43.1, `dua-core` 3.3.0, `trash = "5.2.0"` (feature `trash-move`, default), `gix` `excludes`, ratatui 0.30.2, crossterm 0.29 · accessed 2026-08-30
5. https://github.com/Byron/dua-cli/blob/master/crates/dua-lib/src/lib.rs — work-stealing walker doc: Injector/Stealer/Worker, Parker, `Order::{ParentFirst,Completion}`, symlinks never followed · accessed 2026-08-30
6. https://github.com/Byron/dua-cli/blob/master/crates/dua-lib/src/macos/mod.rs + .../attributes.rs — `getattrlistbulk(2)` walker, `ATTR_CMNEXT_CLONEID`, `EF_MAY_SHARE_BLOCKS = 0x1`, `SF_FIRMLINK = 0x0080_0000`, FTS-stat contract · accessed 2026-08-30
7. https://github.com/Byron/dua-cli/blob/master/README.md — `--deduplicate-apfs-clones` (opt-in, ~6% cost), `--ignore-from` gitignore syntax, interactive multi-stage delete safety note, config `delete_marked/trash_marked` disable · accessed 2026-08-30
8. https://github.com/Byron/dua-cli/blob/master/src/interactive/app/{handlers.rs,cleanup.rs,gitignore.rs,widgets/mark.rs} — `MarkMode::{Delete,Trash}`, `trash::delete()`, `CLEANUP_DIR_NAMES_SORTED_FOR_BISECT`, parallel deepest-first delete, `gix::ignore` marking · accessed 2026-08-30
9. https://github.com/Byron/trash-rs/blob/master/Cargo.toml + src/macos/mod.rs — `trash` 5.2.6 MIT; macOS backend `NSFileManager::defaultManager()` / `trashItemAtURL` via objc2 · accessed 2026-08-30
10. https://github.com/dundee/gdu/blob/master/LICENSE.md — gdu MIT ("Copyright 2020-2021 Daniel Milde") · accessed 2026-08-30
11. https://github.com/dundee/gdu — README: v5.37.0, `--max-age/--min-age/--since/--until`, `--db` SQLite/Badger, hardlinks counted once, `--no-delete`, web UI · accessed 2026-08-30
12. https://github.com/dundee/gdu/blob/master/pkg/remove/trash_darwin.go — "MoveItemToTrash is not supported on macOS… use permanent delete (d) or empty (e)" · accessed 2026-08-30
13. https://github.com/dundee/gdu/blob/master/pkg/analyze/{parallel.go,dir_unix.go} — `concurrencyLimit = 2×GOMAXPROCS`, `Nlink > 1 → ino`, `stat.Blocks × 512` · accessed 2026-08-30
14. https://github.com/bootandy/dust/blob/master/Cargo.toml — `du-dust` 1.2.5, `license = "Apache-2.0"` · accessed 2026-08-30
15. https://github.com/bootandy/dust/blob/master/src/{dir_walker.rs,progress.rs} — `par_bridge()`, `clean_inodes` `(dev,ino)` dedup, spinner thread · accessed 2026-08-30
16. https://github.com/sharkdp/diskus/blob/master/Cargo.toml + src/{walk.rs,unique_id.rs} — 0.9.0 `MIT OR Apache-2.0`; `3 × available_parallelism` (≤64); `UniqueID` when `nlink() > 1` · accessed 2026-08-30
17. https://github.com/tbillington/kondo/blob/master/{kondo/Cargo.toml,kondo-lib/Cargo.toml,kondo-lib/src/lib.rs} — 0.9.0 MIT; 23 `ProjectType`s; `PROJECT_*_DIRS`; `clean()` = `remove_dir_all`; `ignore`/`walkdir`; `ScanOptions` · accessed 2026-08-30
18. https://github.com/holmgr/cargo-sweep/blob/master/{Cargo.toml,README.md,src/fingerprint.rs} — 0.8.0 MIT; unmaintained banner; `StableSipHasher128`/legacy `SipHasher`; `--time/--installed/--toolchains/--maxsize/--recursive/--dry-run` · accessed 2026-08-30
19. https://github.com/voidcosmos/npkill/blob/master/{package.json,src/core/constants/profiles.constants.ts,src/core/services/files/*,src/utils/is-safe-to-delete.ts} — 0.12.2 MIT; worker pool `min(cores-1, 8)`; `MAX_PROCS = 100`; `execFile('rm', ['-rf', …])`; `isSafeToDelete` basename gate · accessed 2026-08-30
20. https://github.com/tj/node-prune/blob/master/{LICENSE,internal/prune/prune.go,main.go} — MIT ("2017 TJ Holowaychuk"); `filepath.Walk`; `DefaultFiles/Directories/Extensions` ("copied from yarn"); `os.RemoveAll`; latest tag v1.2.0 · accessed 2026-08-30
