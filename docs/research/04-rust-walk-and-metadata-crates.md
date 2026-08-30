# Rust FS Walking & Metadata Crates — Research Note

Lane `rust-walk-crates` · parallel walk + metadata collection for `scan/` (SPEC §4) · researched 2026-08-30

## Findings

### Crate landscape (versions verified on crates.io API, 2026-08-30)

| Crate | Ver (2026-08) | License | Threading model | Gitignore-aware | Symlinks | Mount-crossing | Error handling | API stability |
|---|---|---|---|---|---|---|---|---|
| walkdir | 2.5.0 (2024-03) | Unlicense/MIT | single-threaded `Iterator` | no (roll your own `filter_entry`) | `follow_links` off by default; `follow_root_links` **on** by default | `same_file_system(true)` opt-in; Unix+Windows only, else walk errors immediately | errors yielded as items; `walkdir::Error` carries depth, path, symlink-loop flag | very stable; no release since 2024-03 |
| jwalk | 0.9.0 (2026-08-05) | MIT | rayon (`Parallelism::{Serial, RayonDefaultPool{busy_timeout}, RayonExistingPool, RayonNewPool(n)}`) | **no** | `follow_links` off, `skip_hidden` **on** by default | **no option** (no device check anywhere in source) | errors as items; `process_read_dir` callback to filter/sort/skip | **README now says "Unmaintained … use dua-core"** — do not adopt |
| ignore | 0.4.33 (2026-08-04) | Unlicense OR MIT | `WalkParallel` (worker threads + channels, closure per entry) or sequential `Walk` | **yes**: `.ignore`, `.gitignore`, `.git/info/exclude`, global gitignore, `overrides`, `types` | `follow_links(false)` default | `same_file_system(true)` opt-in; per-entry device check via `MetadataExt::dev()` (Unix) / volume serial (Windows) | errors as items; `WalkState::{Continue, Skip, Quit}`; **Quit is asynchronous — more entries may arrive after it** | stable 0.4.x since 2018; maintained inside ripgrep |
| globset | 0.4.20 (2026-08-04) | Unlicense OR MIT | n/a (matcher) | n/a | n/a | n/a | builder returns `Result` | stable; powers `ignore` |
| fs-err | 3.3.1 (2026-07-03) | MIT OR Apache-2.0 | n/a | n/a | n/a | n/a | drop-in `std::fs` with op + path in message; optional `tokio`/`path_facts` features | stable 3.x |
| rayon | 1.12.0 | MIT OR Apache-2.0 | work-stealing global pool; `par_bridge()` pulls sequential items one at a time (documented bottleneck for IO iterators) | n/a | n/a | n/a | n/a | stable 1.x |
| num_cpus | 1.17.0 | MIT OR Apache-2.0 | n/a | — | — | — | — | stable; `std::thread::available_parallelism` covers most needs |
| dashmap | 6.2.1 | MIT | sharded concurrent map (`&self` mutation) | — | — | — | — | stable 6.x; MSRV 1.70 |
| sysinfo | 0.39.6 | MIT | n/a | — | — | n/a | — | 0.3x churns between minor versions |
| indicatif | 0.18.6 | MIT | draws to **stderr**, max 20 refreshes/s by default | — | — | — | — | 0.1x; minor-version churn |
| dua-core | 3.3.0 (2026-08-28) | MIT | own work-stealing pool (crossbeam-deque style; **no rayon**) | no (caller-side, see below) | never follows (symlinks reported only) | caller's `descend` predicate sees `Metadata::dev()` per entry | errors as `io::Result` items; drop iterator = stop + join workers | **new crate** (extracted from dua-cli Aug 2026, ~10k downloads) |
| crossbeam-channel | 0.5.16 | MIT OR Apache-2.0 | MPMC channels + `select!` | — | — | — | — | stable |

Sources: crates.io API per crate [1]; docs.rs pages [4][5][13][14][15]; source files cited below.

### dua-cli / dua-core: how the parallel walk works (source-verified)

- dua-cli 2.43.1 (MIT) ships its walker as the **dua-core** crate (`crates/dua-lib`, v3.3.0, `rust-version = "1.88"`, deps: `crossbeam` + `libc` on macOS + `windows-sys` — no rayon). jwalk's README redirects here.
- Pool: `crossbeam::deque::{Injector, Stealer, Worker}` + `Parker/Unparker`; root job in shared injector; workers run local LIFO or steal; each successful theft wakes one more idle worker; `stop: AtomicBool`; `ENTRY_CHUNK_SIZE = 4` batches metadata reads; **dropping the iterator stops and joins workers** (that is the cancellation primitive).
- **macOS fast path**: `getattrlistbulk(2)` enumerates a directory and returns per-entry metadata in one syscall per 64 KiB buffer (`DIRECTORY_BUFFER_BYTES`); no per-file `lstat`. Requested attribute bitmap (dua `macos/attributes.rs`): `ATTR_CMN_RETURNED_ATTRS, ATTR_CMN_NAME, ATTR_CMN_DEVID, ATTR_CMN_OBJTYPE, ATTR_CMN_CRTIME, ATTR_CMN_MODTIME, ATTR_CMN_CHGTIME, ATTR_CMN_ACCTIME, ATTR_CMN_OWNERID, ATTR_CMN_GRPID, ATTR_CMN_ACCESSMASK, ATTR_CMN_FLAGS, ATTR_CMN_FILEID, ATTR_FILE_LINKCOUNT, ATTR_FILE_ALLOCSIZE, ATTR_FILE_IOBLOCKSIZE, ATTR_FILE_DEVTYPE, ATTR_FILE_DATALENGTH, ATTR_FILE_DATAALLOCSIZE, ATTR_CMNEXT_CLONEID, ATTR_CMNEXT_EXT_FLAGS`; firmwares detected via `SF_FIRMLINK = 0x0080_0000`. `ATTR_CMNEXT_CLONEID` identifies APFS copy-on-write clones (dedup flag costs ~6%).
- Yield order: `Order::ParentFirst` (parent before descendants) or `Order::Completion` (batches as they finish); sibling order unspecified either way. APIs: `walk(root, threads, order, Options, descend) -> Walk: Iterator<Item = io::Result<Entry>>`, `walk_roots(...) -> RootWalk` yielding `(root_idx, RootEvent::{Entry, Finished})`; `Entry { depth, file_name, file_type, metadata: io::Result<Metadata>, parent_path: Arc<Path> }`; macOS `Metadata::{len, allocated_size, data_allocated_size, blocks, modified, dev, ino, nlink, clone_id}`.
- **Mount skipping is a per-entry `st_dev` compare, not a mount-table scan**: dua measures the root device with `crossdev::init` (= `path.metadata().dev()`, i.e. `std::os::unix::fs::MetadataExt::dev`) and the `descend` predicate rejects entries whose `metadata.dev()` differs (when `cross_filesystems == false`, i.e. `-x/--stay-on-filesystem`). `--ignore-dirs` are canonicalized and compared as paths (Linux defaults `/proc /dev /sys /run`; empty on macOS).
- **No gitignore in the walker**: dua-core has no ignore support. dua-cli applies patterns two ways: (a) `IgnorePatterns` built on `gix::ignore::Search` evaluated inside the `descend` predicate (`--ignore-from` files), and (b) interactively, post-hoc marking of entries via `gix` repo excludes (`interactive/app/gitignore.rs`). Negated patterns cannot re-include below an excluded dir (same as git).
- **Progress + interrupt pattern**: `BackgroundTraversal::start` spawns a `dua-fs-walk-dispatcher` thread; entries flow over a `crossbeam` bounded(100) channel as `TraversalEvent::{Entry, RootError, Finished}`; the TUI main loop is a `crossbeam::select!` over the crossterm input receiver and the scan-event receiver; redraws are gated by `Throttle` (background thread flips an `AtomicBool` every 100–250 ms). If the receiver is dropped (user quit), the sender errors and the walker thread returns — walking aborts without any cancel flag. Aggregate CLI mode throttles a transient count line to 100 ms with a 1 s initial sleep.
- Ctrl-C in the TUI is just a keybinding: `KeysConfig::default()` sets `quit_immediately = ["ctrl+c"]` (crossterm `KeyModifiers::CONTROL`), handled in the same select loop; scan state stays WAL-free but consistent because partial tree integration is already done per event.

### gdu (Go, MIT): the progress + cancel patterns worth copying

- Parallelism: one goroutine per subdirectory, bounded by a `chan struct{}` semaphore of `2*GOMAXPROCS` (`pkg/analyze/parallel.go`); `os.ReadDir` + per-entry `Info()`.
- Progress: atomic counters (`progressItemCount`, `progressTotalUsage`, current-path as `atomic.Value`); a producer goroutine ticks **50 ms**, non-blockingly pushing `CurrentProgress{ItemCount, TotalUsage, CurrentItemName}` into a buffered(1) channel; the UI goroutine ticks **100 ms**, reads `GetProgress()`, and does one `QueueUpdateDraw` (`tui/progress.go`). Decoupled ticker → snapshot → draw is the clean shape to port to ratatui.
- Ctrl-C semantics: SIGINT is intercepted (`signal.Notify`, converted to a synthetic tcell key event); `Cancel()` sets an atomic flag checked per directory and before scheduling queued dirs; in-flight ops finish; **partial results are kept** ("Press Ctrl+C to stop scanning and keep results"); cancel has race tests (`cancel_race_test.go`).
- Terminal-tab progress via OSC 9;4 escape sequence written to stderr (`tui/progress.go`).
- Mount handling: `--no-cross` does **not** compare devices during the walk; it pre-computes nested mount points from the mount table and appends them to `IgnoreDirs` (`cmd/gdu/app/app.go setNoCross`) — inferior to dua's per-entry `st_dev` check for removable volumes mounted mid-tree.
- **macOS dataless files (iCloud/OneDrive)**: `pkg/fs/dataless_darwin.go` calls `setiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_SCOPE_PROCESS, IOPOL_MATERIALIZE_DATALESS_FILES_OFF)` via `purego` dlopen of libSystem, citing Apple TN3150 and `/usr/bin/du`'s `vfs.nspace.prevent_materialization` sysctl. Constants verified in xnu `bsd/sys/resource.h`: `IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES = 3`, `IOPOL_SCOPE_PROCESS = 0`, `IOPOL_MATERIALIZE_DATALESS_FILES_OFF = 1`. Effect: scanning never triggers downloads; content reads of dataless files fail with `EDEADLK` (fine — metadata-only scan). Without it, walking a File Provider tree is orders of magnitude slower.

### Walk + metadata strategy for Disk Janitor's `scan/`

Our scan is **metadata-first, not content** (SPEC §4: size/mtime/flags), targets APFS + external volumes, must show live progress (R4) and skip mount points by default. Decision drivers:

1. **Parallel directory-level traversal is the win**; per-file stat is already covered by bulk enumeration. jwalk's benchmark on the Linux source tree (8 threads, iMac 2015): jwalk 54.6 ms unsorted vs ignore 70.8 ms; sorted+metadata 87 ms vs 122 ms ([2] — dated but directionally right). dua-core's `getattrlistbulk` path goes further on macOS: metadata arrives with the directory read.
2. **ignore::WalkParallel is the only maintained general-purpose parallel walker with gitignore + same_file_system built in**, but its parallel API is push-closure based (no streaming iterator): you implement a `run(|| |entry| -> WalkState)` callback and push results through your own channel. `WalkState::Quit` is asynchronous — plan a drain/flush phase.
3. **Cancelling = dropping the iterator** (walkdir, dua-core) or returning `WalkState::Quit` (ignore) or `false` from `descend` (dua-core). All join their workers on drop, so Ctrl-C handling is: flip an `AtomicBool`, drain-and-drop, workers join within one directory read.
4. Mount-point skipping: **compare `MetadataExt::dev()` per directory entry** (dua's approach; also what ignore's `same_file_system` does internally via `device_num` = `path.metadata().dev()`) rather than parsing the mount table. Edge case to test: **firmlinks** on macOS Catalina+ (`SF_FIRMLINK` flag in stat/attr flags; dua-core handles them in its macOS module) and `.Spotlight-V100`-style dirs on external drives.
5. **Sizes**: use allocated size (`ATTR_FILE_DATAALLOCSIZE` / `st_blocks * 512`) for "space you'd reclaim" and `len()`/`DATALENGTH` for apparent size — dua exposes both (`apparent_size` flag). For purge-confirm byte counts (SPEC: type exact byte count), allocated is what freeing actually yields.
6. Aggregate state: single-threaded aggregation over the event stream (HashMap<PathBuf, Node> + parent links) is what dua does (`petgraph::stable_graph::StableGraph`); DashMap is only needed if classifier threads mutate shared state concurrently — avoid; keep classify/scan boundary channel-based instead.

### Progress-bar strategy: indicatif vs custom channels

- indicatif 0.18.6 (MIT) draws to **stderr** at max 20 Hz by default; good fit for **CLI/`plan` subcommand** phases: `ProgressBar::new(len)` + `set_length` (unknown total → use a spinner or `inc_length` as entries stream in), `set_message`, `inc`, `println` for log lines above the bar, `abandon_with_message` on cancel. Unknown-total scans can't show percent — dua's CLI just throttles an entries-traversed counter, which is the honest pattern.
- For the **ratatui TUI**, indicatif fights the alternate screen (it assumes it owns stderr). Use the dua/gdu pattern instead: scanner thread pushes events into a `crossbeam::channel`; a background **ticker thread** (100–250 ms; dua uses 100 ms CLI / 250 ms TUI; gdu 50 ms producer + 100 ms UI tick) drains non-blockingly (`try_recv`) into an atomic snapshot struct (`AtomicU64` entries/bytes + `AtomicU64` elapsed); the ratatui draw loop renders the footer from the snapshot. This is exactly dua's `Footer` widget fed by `TraversalStats`.
- Byte-count "progress": show `bytes_seen` + entries, optionally percent against `Disk::total_space()` of the scanned volume (sysinfo) like gdu's disk progress bar — never against an unknown tree size.

### Recommended walker stack (concrete)

```
scan/
  dua-core 3.3.0            // parallel walk; macOS getattrlistbulk metadata in-bulk
     walk_roots(input, threads, Order::ParentFirst, Options{apfs_clone_metadata: false},
                descend: |_, entry| same_dev(entry) && !ignored(entry))
  crossbeam-channel         // forward TraversalEvent-ish batches to UI thread
  libc (0.2.189)            // setiopolicy_np via extern "C" (NOT exported by libc — see risks)
  std::os::unix::fs::MetadataExt::dev()  // root device + per-entry mount check
UI progress: crossbeam select! + Throttle-style atomic snapshot -> ratatui footer
CLI progress: indicatif 0.18.6 (stderr) for plan/apply phases
Cancellation: ctrl-c handler flips AtomicBool + drops RootWalk (joins workers);
              channel-send failure also aborts walker thread (dua pattern)
Volume info: sysinfo 0.39.6 Disks::new_with_refreshed_list() -> Disk::{mount_point,
             total_space, available_space, file_system, is_removable, is_read_only}
Error text:  fs-err 3.3.1 for ledger/WAL/manifest file IO (not for hot walk path)
```

Why dua-core over ignore::WalkParallel: metadata arrives with enumeration on macOS (no second syscall), per-entry `dev()` in `descend` gives default-off mount crossing, streaming iterator + drop-to-cancel matches R4, MIT license, and it's the extraction dua itself now recommends (jwalk is dead). Keep `ignore` + `globset` in reserve for gitignore-style **exclusion patterns** if we want `.gitignore`-aware "orphan node_modules" classification without pulling gix: `ignore::gitignore::GitignoreBuilder` is usable standalone. If dua-core proves too young/unstable by M1, the fallback is `ignore::WalkParallel` + our own channel + `std::thread::available_parallelism()`.

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| Byron/dua-cli (crates/dua-lib = dua-core) | MIT | whole walker: work-stealing pool, drop-to-cancel, macOS `getattrlistbulk` + APFS clone-id, `descend` device check, Throttle redraw gating | `crates/dua-lib/src/lib.rs`, `crates/dua-lib/src/macos/{mod,attributes}.rs`; CLI side `src/traverse.rs`, `src/common.rs`, `src/crossdev.rs` |
| Byron/dua-cli TUI | MIT | select! over input+scan channels, per-event tree integration, Footer progress widget, ctrl+c as `quit_immediately` binding | `src/interactive/app/eventloop.rs`, `src/interactive/app/input.rs`, `src/interactive/widgets/footer.rs`, `src/config.rs` |
| dundee/gdu | MIT | ticker→atomic-snapshot→QueueUpdateDraw progress loop; Ctrl-C keeps-partial-results semantics + `Cancel()` atomic checked per dir; OSC 9;4 terminal progress; dataless-file prevention | `tui/progress.go`, `pkg/analyze/analyzer.go`, `internal/tui/tui.go` (signal handling), `pkg/fs/dataless_darwin.go` |
| BurntSushi/ripgrep (crates/ignore) | Unlicense OR MIT | reference for per-entry device check (`device_num` = `MetadataExt::dev()`), `WalkParallel` worker/channel design, `WalkState::Quit` semantics | `crates/ignore/src/walk.rs` (L1463–1495, L1791–1792, L2164–2175) |
| GuillaumeGomez/sysinfo | MIT | macOS volume enumeration via `getfsstat(MNT_NOWAIT)` + CoreFoundation volume keys; available-space preference logic (`kCFURLVolumeAvailableCapacityForImportantUsageKey` first) | `src/unix/apple/disk.rs` (L246–252, L379–392, L512) |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| dua-core 3.3.0 | parallel walk + bulk macOS metadata | MIT; `getattrlistbulk` path; streaming iterator; drop-to-cancel; `descend` predicate for mounts & ignores; no rayon dep | ignore 0.4.33 `WalkParallel` |
| ignore 0.4.33 | gitignore/glob exclusion + fallback walker | only maintained walker with built-in gitignore + `same_file_system`; use `gitignore::GitignoreBuilder` standalone for classifer patterns | globset 0.4.20 raw |
| globset 0.4.20 | glob matching for user ignore globs | fast set matcher; shares engine with ignore | — |
| fs-err 3.3.1 | error context for non-hot-path file IO | drop-in `std::fs`; op + path in messages; MIT/Apache-2.0 | manual `map_err` |
| rayon 1.12.0 | parallel map for classify stage (`par_bridge` over scan events) | documented bottleneck warning for IO iterators — keep for CPU-bound classify only | std threads + channel |
| num_cpus 1.17.0 | worker count for dua-core `threads` arg | dua itself depends on it; or `std::thread::available_parallelism` (std) | std |
| dashmap 6.2.1 | only if classifier threads share maps | sharded `RwLock<HashMap>`; avoid by keeping scan→classify channel-based | `RwLock<HashMap>` |
| sysinfo 0.39.6 | volume list, capacity, removable/read-only flags | MIT; uses `getfsstat` + CFURL volume keys on macOS; `DiskRefreshKind` to trim cost | libc `statfs` manual |
| indicatif 0.18.6 | CLI progress bars (plan/apply/purge) | stderr, 20 Hz default, `abandon_with_message` for cancel; don't mix with ratatui alt-screen | custom ratatui footer |
| crossbeam-channel 0.5.16 | scan→UI event channel + select! loop | MPMC + select; the dua/gdu progress backbone | std mpsc (no select) |

## Risks & open questions

- **dua-core is 10 days old as a standalone crate** (first release Aug 2026, ~10k downloads, v3.3.0). Pin exact version; audit `crates/dua-lib/src/macos` unsafe code (raw `attrlist` buffers) before shipping. Fallback: ignore::WalkParallel.
- **libc does not export `setiopolicy_np`** (verified against docs.rs libc aarch64-apple-darwin index, 2026-08-30; it does export `getattrlistbulk`, `fgetattrlist`, `dlopen`, `dlsym`). We need `extern "C" { fn setiopolicy_np(iotype: c_int, scope: c_int, policy: c_int) -> c_int; }` linking against libSystem directly (no dlopen needed in Rust — unlike Go, plain linking suffices). Constants `IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES=3`, `IOPOL_SCOPE_PROCESS=0`, `IOPOL_MATERIALIZE_DATALESS_FILES_OFF=1` from xnu `bsd/sys/resource.h`; also `MNT_RDONLY` IS in libc (0x1) — sysinfo reads it from `f_flags`.
- **iCloud dataless files**: without `setiopolicy_np`, scanning `~/Library/Mobile Documents` triggers mass materialization (downloads). Decide: always call it at scan start (gdu's stance: safe, unprivileged, inert when no dataless files) or make it a flag. Note `EDEADLK` on content reads — classify must not open file contents.
- **APFS firmlinks**: `SF_FIRMLINK` (0x0080_0000) entries between `/System` and `/Users` — dua-core handles them; a naive `st_dev` check may not fire (firmlinks share device), but a naive no-check would re-walk both sides. Test on `/` root scan.
- **Scanner thread count vs UI latency**: dua caps channel at 100 events and throttles redraw to 250 ms — copying those exact numbers is the safest starting point; gdu's 50/100 ms tickers are the alternative.
- **`ignore`'s `WalkState::Quit` is async**: entries still arrive after quit; if we ever switch to ignore::WalkParallel, the cancel path needs an explicit drain/flush. dua-core's drop-join avoids this class of bug.

## Sources

1. crates.io API per-crate records (versions/licenses/publish dates, all fetched 2026-08-30): https://crates.io/api/v1/crates/walkdir · /jwalk · /ignore · /globset · /fs-err · /rayon · /num_cpus · /dashmap · /sysinfo · /indicatif · /crossbeam-channel · /dua-core · /dua-cli · /libc
2. jwalk README ("Unmaintained … use dua-core") + benchmarks: https://github.com/Byron/jwalk (README.md, benches/benchmarks.md) — accessed 2026-08-30
3. dua-core source (v3.3.0, tag on main): https://github.com/Byron/dua-cli/blob/main/crates/dua-lib/src/lib.rs and https://github.com/Byron/dua-cli/blob/main/crates/dua-lib/src/macos/mod.rs, .../macos/attributes.rs — accessed 2026-08-30
4. dua-cli traversal/progress wiring: https://github.com/Byron/dua-cli/blob/main/src/{traverse,common,options,main,crossdev,aggregate}.rs and https://github.com/Byron/dua-cli/blob/main/src/interactive/app/{eventloop,input,terminal,gitignore,widgets/footer}.rs, src/config.rs — accessed 2026-08-30
5. dua-cli/dua-core Cargo.toml + LICENSE (MIT): https://github.com/Byron/dua-cli/blob/main/Cargo.toml, https://github.com/Byron/dua-cli/blob/main/crates/dua-lib/Cargo.toml, https://github.com/Byron/dua-cli/blob/main/LICENSE — accessed 2026-08-30
6. gdu analyzer + progress: https://github.com/dundee/gdu/blob/master/pkg/analyze/{parallel,analyzer,file,dir_unix}.go, https://github.com/dundee/gdu/blob/master/tui/{progress,actions,tui}.go, https://github.com/dundee/gdu/blob/master/cmd/gdu/app/app.go — accessed 2026-08-30
7. gdu dataless handling: https://github.com/dundee/gdu/blob/master/pkg/fs/dataless_darwin.go — accessed 2026-08-30
8. gdu license: https://github.com/dundee/gdu/blob/master/LICENSE.md (MIT) — accessed 2026-08-30
9. Apple man page getattrlistbulk(2): https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/getattrlistbulk.2 — accessed 2026-08-30
10. xnu IOPOL constants: https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/resource.h (L570–602) — accessed 2026-08-30
11. libc Apple symbols: https://docs.rs/libc/latest/aarch64-apple-darwin/libc/index.html (getattrlistbulk, fgetattrlist, dlopen, dlsym, MNT_*; no setiopolicy_np) and https://github.com/rust-lang/libc/blob/main/src/unix/bsd/apple/mod.rs (L3955–4033, L4987) — accessed 2026-08-30
12. Apple TN3150 (dataless files, referenced by gdu): https://developer.apple.com/documentation/technotes/tn3150-getting-ready-for-data-less-files — accessed 2026-08-30
13. docs.rs WalkBuilder (ignore 0.4.33): https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html; WalkState: https://docs.rs/ignore/latest/ignore/enum.WalkState.html — accessed 2026-08-30
14. docs.rs WalkDir (walkdir 2.5.0): https://docs.rs/walkdir/latest/walkdir/struct.WalkDir.html, struct.Error.html — accessed 2026-08-30
15. docs.rs ProgressBar (indicatif 0.18.6): https://docs.rs/indicatif/latest/indicatif/struct.ProgressBar.html — accessed 2026-08-30
16. docs.rs Disk/Disks (sysinfo 0.39.6): https://docs.rs/sysinfo/latest/sysinfo/struct.Disk.html, struct.Disks.html — accessed 2026-08-30
17. rayon par_bridge semantics: https://docs.rs/rayon/latest/rayon/iter/trait.ParallelBridge.html — accessed 2026-08-30
18. sysinfo macOS disk source: https://github.com/GuillaumeGomez/sysinfo/blob/master/src/unix/apple/disk.rs — accessed 2026-08-30
19. fs-err Cargo.toml/README: https://github.com/andrewhickman/fs-err — accessed 2026-08-30
20. License files verified directly: walkdir LICENSE-MIT (BurntSushi/walkdir), dashmap LICENSE, indicatif LICENSE, sysinfo LICENSE, jwalk LICENSE, rayon LICENSE-{MIT,APACHE}, num_cpus LICENSE-*, libc LICENSE-MIT, ignore/globset Cargo.toml `license = "Unlicense OR MIT"`, crossbeam-channel Cargo.toml — all accessed 2026-08-30
