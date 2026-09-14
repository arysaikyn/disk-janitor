# Disk Janitor — Architecture Decision Records

> **Merged-docs map (2026-08-31):** `OVERVIEW.md`, `DESIGN.md`, `UX-SPEC.md`, `crate-map.md` were absorbed into `SPEC.md` (Parts A–D). Citations: 'SPEC §N' → Part A §N · 'DESIGN §N' → Part B §N · 'UX-SPEC §N' → Part C §N · 'crate-map §N' → Part D §N. The 08-31 grill register (D-01…D-22) is the Appendix below.
> ADR log. Each entry: Status / Context / Decision / Consequences.
> Citations: research notes `docs/research/NN-*.md` cited as `[NN §x]`, spec as `[SPEC §x]`;
> all notes verified 2026-08-30. Companion to `docs/SPEC.md` (Parts A–D), which details the designs
> these ADRs commit to. Status lines carry the owner's explicit verdicts from the 2026-08-30 review (upheld / amended / rejected / contested).

## Index

- ADR-1 Walker: dua-core (pinned `=4.0.0` since D-37, 2026-09-10) — the only walker in the tree; jwalk rejected as unmaintained; `globset` covers deny-glob patterns (the `ignore` crate is a research record only)
- ADR-2 Quarantine primitive: same-volume `renamex_np(RENAME_EXCL)` into per-volume hidden roots
- ADR-3 fsync strategy: `F_FULLFSYNC` via std, dir-sync boundaries, sync per state transition
- ADR-4 Ledger/WAL format: framed JSONL + crc32 chain + flock, skip-and-continue recovery
- ADR-5 TUI stack: ratatui 0.30.2 + crossterm 0.29 + indicatif — **shelved with the GUI pivot (ADR-18); never declared as dependencies**
- ADR-6 Agent-loop HTTP stack: async-openai 0.41.3, sans-IO loop, reqwest rustls (reqwest is **transitive only** — we never name it in source)
- ADR-7 Privacy architecture: opaque IDs + serializer chokepoint + CI leak test
- ADR-8 Classification evidence model: evidence chains, 5 grades, CAS GC-only, orphan-first
- ADR-9 Token/cost accounting: per-call provider-reported usage, budget interrupt
- ADR-10 Grace window & purge confirmation: typed allocated byte count, 72 h restore
- ADR-11 Platform strategy — native on macOS, Windows, Linux (`platform/` abstraction, macOS-first order, honest per-OS guarantee table)
- ADR-12 Windows durability & restore semantics — WAL journal as restore authority, FlushFileBuffers at state transitions
- ADR-13 Bilingual UI — CN primary + EN secondary catalog (svelte-i18n central catalog; CN for persuasion, EN for identifiers/terms) — **rendering amended 09-08 per D-31: single-language mode with a 中/EN switcher, no simultaneous dual labels**
- ADR-14 CLI surface contract — thin subset (scan --json, plan, restore, interactive purge) for CI/tests/agents; purge never gains a --yes bypass (P4)
- (ADR-15 number reserved — unused; ADR-16..18 continue the 2026-08-31 session series)
- ADR-16 Uninstall contract — restore-or-explicit-confirm every surviving quarantine across volumes before state removal; orphaned-quarantine warning at startup
- ADR-17 Data format versioning — ledger/WAL/manifest v1 frozen through the course; forward-compat replay (unknown skipped + counted, deny_unknown_fields OFF)
- ADR-18 GUI paradigm — Tauri + Svelte desktop App (GUI-first per D-10; Rust core unchanged; UX-SPEC.md governs UI; ADR-5 TUI shelved)

## ADR-1: Walker — dua-core over jwalk and ignore

**Status:** amended 2026-08-30 — Arys upheld the macOS dua-core fast path and confirmed the 3-platform amendment (ignore::WalkParallel baseline elsewhere, Walker trait seam).

**Context.** SPEC §4 requires a parallel, metadata-first walker (size/mtime/flags) with live
progress and interrupt (R4), staying on one filesystem by default. The researched field [04]:
walkdir 2.5.0 is serial-only; jwalk 0.9.0's README now states "Unmaintained … use dua-core"
and its source has no mount/device check, no gitignore, and `skip_hidden` on by default —
do not adopt even as fallback [04]; ignore 0.4.33 `WalkParallel` is the only maintained
general-purpose parallel walker with gitignore + `same_file_system`, but its API is
push-closure (no streaming iterator) and `WalkState::Quit` is asynchronous — entries still
arrive after quit, so cancellation needs an explicit drain/flush phase [04]. dua-core 3.3.0
(extracted from dua-cli, Aug 2026) gets macOS metadata with enumeration via
`getattrlistbulk(2)` — one syscall per 64 KiB buffer, no per-file `lstat` — runs its own
crossbeam work-stealing pool (no rayon), yields a streaming iterator, and joins its workers
when dropped [04].

**Decision.**

- dua-core pinned exact — `=3.3.0` at decision time, **`=4.0.0` since D-37 (2026-09-10)** — as the
  primary and only walker in `scan/`:
  `walk_roots(roots, threads, Order::ParentFirst, opts, descend)` [04][SPEC §4].
- Mount skipping is a per-entry `MetadataExt::dev()` compare inside the `descend` predicate —
  not mount-table parsing; firmlink-safe because firmlinked `/`, `/Users`, and
  `/System/Volumes/Data/...` share `st_dev` [04 §dua][05 §1].
- `globset` 0.4.20 does the actual policy deny-glob matching. `ignore` 0.4.33 (and `walkdir`
  2.5.0) are **research records only — never dependencies** (2026-09-12 audit); the survey
  proposals to use them for `gitignore::GitignoreBuilder` and as a serial fallback were never
  implemented [04].
- Pre-planned fallback if dua-core churns: `ignore::WalkParallel` + own channel +
  `std::thread::available_parallelism()`, with an explicit drain/flush cancel path — **not
  currently vendored**; adding it would reintroduce the `ignore` dependency [04].

**Consequences.**

- Metadata (mtime, BSD flags, nlink, allocsize, APFS clone-id) arrives with enumeration; the
  second-stat pass disappears; drop-the-iterator = cancel + join workers is exactly R4's
  interrupt primitive [04].
- dua-core is ~10 days old as a standalone crate: pin exact, audit the macOS unsafe
  (`attrlist` buffers) before shipping [04].
- While on dua-core, the async-Quit drain bug class never exists; it returns only if the
  ignore fallback is ever activated.

**Amended 2026-08-30 (3-platform):** dua-core remains the **macOS** fast path only — its
`getattrlistbulk` enumeration and APFS accounting are exactly as decided above. The portable
baseline on every OS is *planned* as `ignore` 0.4.33 `WalkParallel` (the only walker fully
portable today) with per-OS metadata fast paths feeding the same `DirStat` type: Linux `statx`
via raw `libc` FFI (`libc::statx`, `STATX_BTIME` for birth time; `st_dev`/`st_nlink`/`st_blocks`
via portable Unix `MetadataExt`) — **`rustix` is a research record, not a dependency**
(2026-09-12 audit); plus per-candidate mount resolution from `/proc/self/mountinfo`; Windows
`GetFileInformationByHandleEx(FileIdBothDirInfo)` bulk enumeration copied from dua-core's
windows module (measured "30× and more performance" since dua-cli 2.41), never descending
reparse points/junctions. The `Walker` trait is the seam — macOS = dua-core, baseline =
`ignore::WalkParallel`, fast paths behind the identical `DirStat` output contract [11 §4/§5/§7]
[09 §delta].

> **Status 2026-09-12 (audit):** the 3-platform paragraph above is the **design plan, not the
> shipped tree**. `ignore::WalkParallel` is not a dependency and no portable walker baseline
> exists in source; `scan/` is macOS-only behind the `Walker` seam. Adding the baseline means
> declaring `ignore` for the first time. Windows/Linux remain compile-checked, not
> behavior-tested, exactly as SPEC §8 records.

## ADR-2: Quarantine primitive — same-volume `renamex_np(RENAME_EXCL)` moves

**Status:** amended 2026-08-30 — Arys upheld the macOS renamex_np design and confirmed the per-OS AtomicMove amendment (Linux renameat2 with fs probe; Windows FileRenameInfoEx with runtime capability probe).

**Context.** SPEC §3.2 mandates `apply` = atomic same-volume move into an on-volume
quarantine dir, zero bytes freed, fully reversible. [05] resolves the mechanics empirically
on this machine (macOS 26, APFS): plain `rename(2)` guarantees an instance of `new` exists
even if the system crashes mid-move; `renamex_np(RENAME_EXCL)` additionally fails `EEXIST`
on collision, making the syscall itself the existence check (no TOCTOU vs plain rename);
`RENAME_NOFOLLOW_ANY` returns `ELOOP` if any path component is a symlink; `EXDEV` is raised
even across APFS volumes in one container (`/tmp` → `/System/Volumes/VM`), so "same volume"
means same `st_dev`; firmlinked System/Data paths share `st_dev` so firmlink traversal does
not break same-volume moves; xattrs and BSD flags survive the rename; exFAT/FAT sticks lack
`VOL_CAP_INT_RENAME_EXCL` (`ENOTSUP`). Apple's `.Trashes` is a root-owned ACL minefield with
undocumented Put-Back metadata [02 risks #5][05 §7].

**Decision.**

- Quarantine = per-volume hidden root `<mnt>/.Trash-janitor/` (mkdir 0700 + `chflags
  UF_HIDDEN`; the flag survives rename — never Apple's `.Trashes`), with per-operation
  subdirectory `<root>/<wal-op-id>/` so restore = reverse renames from the WAL and purge =
  recursive unlink of the op dir [05 §7].
- Move = `renamex_np(from, to, RENAME_EXCL | RENAME_NOFOLLOW_ANY)`; `EEXIST` ⇒ retry with a
  `-<n>` suffix; `ELOOP` ⇒ re-plan [05 §1][05 §7].
- Same-`st_dev` is enforced by construction (per-volume root) and re-verified at apply time
  (`statfs` both ends) as TOCTOU armor; cross-volume targets are refused at plan time with
  the non-atomic `copyfile(COPYFILE_MOVE | COPYFILE_ALL)` downgrade documented but requiring
  explicit consent [05 §2].
- Capability probe `VOL_CAP_INT_RENAME_EXCL` per volume is advisory only — trust errno
  (`EINVAL`/`ENOTSUP`) at call time, because the empirical `RENAME_SWAP` bit is clear while
  the call succeeds [05 §1].
- Raw `libc` decls: `nix` was surveyed as a candidate and rejected — it has no `renamex_np`
  (`RenameFlags` is Linux-only) and is not a dependency; constants missing from the `libc`
  crate (`RENAME_NOFOLLOW_ANY=0x10`, `SF_DATALESS`, `UF_DATAVAULT`, …) are defined locally
  [05 §reusable].

**Consequences.**

- `apply` frees zero bytes and never loses a file even on crash (rename crash-atomicity);
  quarantined trees lose nothing (xattrs/flags preserved) [05 §1][05 §6].
- Plan-time refusals pre-map the exact errno families apply would hit (`EXDEV`, `ENOTSUP`,
  `EDEADLK` for dataless, `EPERM` for immutable-flag entries) [05 §2][05 §5][05 §6].
- Restore never depends on Trash Put-Back metadata — the WAL owns the
  `(original, quarantine)` pairs [02 risks #5][05 §7].

**Amended 2026-08-30 (3-platform):** the same-volume atomic move becomes a per-OS
`AtomicMove` table behind the `platform/` trait (DESIGN §3.10), with the macOS row above as
the semantics gold standard [11 §1]:

- **Linux** — `renameat2(AT_FDCWD, src, AT_FDCWD, dst, RENAME_NOREPLACE)`: kernel ≥ 3.15,
  glibc wrapper ≥ 2.28 (libc 0.2.189 ships the constants; the `rustix::fs::renameat_with`
  candidate was surveyed but not adopted — raw `libc` covers it); `EEXIST` on target feeds the
  same `-<n>` retry loop. Filesystem support is a
  matrix (ext4 ≥ 3.15, btrfs/tmpfs ≥ 3.17, xfs ≥ 4.0, many more ≥ 4.9), probed per volume via
  `/proc/self/mountinfo` fstype — **NFS rejects any flags with `EINVAL`** in `nfs_rename()` and
  the man page warns the file may still have been renamed; on unsupported/NFS fs the fallback
  is `O_TMPFILE` + `linkat(AT_EMPTY_PATH)` staging (atomicity preserved) or refuse auto-action;
  `ENOSYS` (kernel < 3.15) and `EINVAL` (unsupported flag on this fs) at call time route to
  that same fallback/refuse path — trust errno over the probe, per the macOS advisory-probe
  precedent [11 §1][10 §0].
- **Windows** — open source with `DELETE` access
  (`FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`), then
  `SetFileInformationByHandle(FileRenameInfoEx, Flags=0)` (no-clobber = omit
  `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`; Win10 1607 headers, 1709+ for POSIX_SEMANTICS per
  rust-lang#123985; `[MS-FSCC]` states no minimum — runtime probe, never assumed). Residual
  weaker cases are documented, not hidden: restore's replace step is non-atomic w.r.t. open
  targets pre-1709 (`MoveFileExW(MOVEFILE_REPLACE_EXISTING)` fallback, sharing violations
  surface as errors), SMB shares have no POSIX rename semantics (treated as external
  volumes), and metadata durability rides NTFS journaling — **on Windows, restore therefore
  relies on the WAL journal, not OS Trash** (ADR-12) [11 §1/§2/risks][09 §delta].

## ADR-3: fsync strategy — `F_FULLFSYNC` via std, dir-sync boundaries, sync per state transition

**Status:** upheld — Arys, 2026-08-30.

**Context.** The WAL only makes operations crash-recoverable if its records actually reach
permanent storage. On Darwin this is subtle [08 §1–2][05 §4]: plain `fsync(2)` flushes host
buffers but the drive itself may delay, reorder, or never physically persist — Apple's man
page calls the loss scenario "easily reproduced with real world workloads" and directs
databases to `fcntl(F_FULLFSYNC)` (cmd 51), which drains the device queue as a barrier and is
implemented on HFS/FAT/UDF/APFS. Rust std already maps `File::sync_all()`/`sync_data()` to
`F_FULLFSYNC` on apple targets (PR #60121) — no extra crate for the data fsync [08 §2].
There is no std API for fsyncing a directory, but opening it `O_RDONLY|O_DIRECTORY` and
calling `sync_all()` works (SQLite's `openDirectory` does exactly this). Worst, even two
independent fullfsyncs do not order across files on APFS: SQLite 3.46.1 shipped a backport
inserting a directory sync *between* the DB-file and WAL-file fullfsyncs [08 §1]. `F_FULLFSYNC`
costs ≈ 4 ms vs ≈ 0.075 ms for plain fsync on Apple SSDs [08 §1], and `F_BARRIERFSYNC`
(cmd 85) orders prior I/O without a durability claim [05 §4].

**Decision.**

- WAL durability uses `F_FULLFSYNC` semantics exclusively, reached via std
  `sync_data()`/`sync_all()`; no plain `fsync` anywhere in the durability path [08 §2][05 §4].
- The rename-to-quarantine dance ends with `sync_all()` on the parent directory fd (open
  `O_RDONLY|O_DIRECTORY`) — directory-entry durability is the textbook POSIX gap, and on APFS
  a dir-sync closes it cheaply at WAL-commit frequency [08 §1][05 §4].
- Between two independent fullfsync'd steps (WAL append vs snapshot/manifest replace), a
  **parent-dir sync is inserted at the boundary**, mirroring the SQLite 3.46.1 fix [08 §1].
- Sync granularity: per **state transition** (intent synced before mutation; result recorded
  after; dir sync at the op-group boundary), not per record — amortizing the ~4 ms cost
  [08 §1][08 risks #1].
- `tempfile`'s `persist`/`persist_noclobber` provide atomicity only and do NOT fsync — we own
  sync(temp) + sync(parent) ourselves for WAL compaction/manifest writes [08 §3].

**Consequences.**

- Crash windows shrink to the documented set (before-wal-sync / after-wal-sync-before-rename /
  after-rename-before-result) and each is covered by the recovery algorithm (ADR-4).
- Write throughput is bounded by ~4 ms per transition, acceptable for user-visible operations
  (apply/purge), not for bulk scan — which writes only session-summary ledger records.
- `F_BARRIERFSYNC` stays unused for now: ordering-without-durability has no current call site.

## ADR-4: Ledger/WAL format — framed JSONL + CRC32 chain + flock, skip-and-continue recovery

**Status:** upheld — Arys, 2026-08-30.

**Context.** SPEC §4 gives `ledger/` a persistent JSONL 台账 (every verdict and operation,
cross-session memory) and §3.2 mandates WAL crash-recoverability. [08] surveyed the space:
sled is stalled at 1.0.0-alpha.124 since 2024-10 and overkill; SQLite's WAL and git's pack
formats are the reference patterns. A bare JSON line cannot distinguish a torn tail from a
corrupt middle; SQLite solves this with per-frame checksums plus a cumulative chain, and its
recovery is a single pass that stops at the first invalid checksum [08 §4–5]. Reads must use
`BufRead::read_until(b'\n')` — bare `lines()` silently merges a torn final line into a valid
one [08 §5]. macOS offers `flock(2)` (whole-file, open-file-description ownership, "more
efficient" per Apple's man page) plus advisory-only fcntl ranges; a second janitor instance
must fail fast. On external volumes, recovery is impossible unless the WAL lives on the same
volume as the quarantine it describes [08 risks #3].

**Decision.**

- Two files in `~/.disk-janitor/`: `ledger.jsonl` (permanent history) and `wal.jsonl`
  (crash journal, checkpointed); derived state never lives only in memory [08 §7][08 §8].
- Envelope per line: `{"v":1,"seq":42,"ts":…,"kind":…,"crc32":…,"prev_crc":…,"rec":{…}}` —
  CRC32 (`crc32fast`) over canonical `serde_json::to_vec(rec)` + seq, stored in-band;
  optional `prev_crc` chain (SQLite-style) detects reordering/loss of middle lines [08 §4].
- Record kinds: `scan_result`, `verdict`, `plan`, `apply`, `purge`, `restore`, `budget`
  (R5/R6) — every record self-contained for replay; the only cross-record reference is
  `undo_of` (a `seq`), so one corrupted line cannot poison later independent ops [08 §8].
- Single writer guarded by an exclusive non-blocking lock (`flock(2) LOCK_EX | LOCK_NB` on unix)
  on `~/.disk-janitor/janitor.lock` via **std `File::try_lock`** (stabilized Rust 1.89) — the
  `fs4` and `nix::fcntl::Flock` candidates were surveyed and **not adopted** (2026-09-12 audit);
  see `src/ledger.rs::open`. A second instance fails fast with a clear message [08 §6].
- Recovery: replay ledger (skip-and-continue, recording skipped offsets + seq gaps — an
  operations ledger, unlike SQLite's stop-at-first-invalid-frame); replay WAL tail — a record
  is committed iff its CRC validates; torn tail truncated to the last complete line and parked
  in `*.torn`; reconcile WAL intents against observed FS state (`quarantine_path` exists ⇒
  roll forward — apply/restore are re-runnable, rename fails benignly `ENOENT`; else drop the
  tail) [08 §5][08 risks #5].
- Per-volume WAL lives inside its quarantine (`<quarantine>/wal.jsonl`) — cross-volume crash
  recovery is otherwise impossible [08 risks #3].

**Consequences.**

- Checkpoint = replay valid records → temp file in the same dir → sync temp → sync parent →
  `rename` over the WAL (tempfile 3.27.0; we add the fsyncs per ADR-3) [08 §5].
- Torn tails and mid-corrupt lines degrade one operation, never the ledger; forensics survive
  in `*.torn` files.
- Ledger privacy asymmetry is decided here: `verdict`/`plan` store `path_hash` only; `apply`/
  `restore` store real paths because undo needs them — local-only, the LLM never reads it
  [08 risks #6][07 §privacy].

## ADR-5: TUI stack — ratatui + crossterm pinned, threads + crossbeam, indicatif CLI-only

**Status:** amended 2026-08-31 — Arys pivoted to GUI-first desktop App (Tauri + Svelte); TUI shelved (see ADR-18).

**Context.** R2 requires an interactive TUI with CLI subcommands as fallback; R4 requires
live progress and ctrl-c-safe interrupt. [06] distilled four reference TUIs (dua-cli, gitui,
gdu, ncdu): ratatui 0.30.2 is the widget/diffed-render library (MIT, umbrella re-exporting
ratatui-core/widgets since the 0.30 workspace split) with crossterm 0.29 as its
hard-dependent default backend; the official rule is that two crossterm major versions in one
tree keep separate event queues (races, lost events) and may not restore raw mode on exit —
so crossterm must be pinned to 0.29 next to ratatui 0.30. Raw mode applies `cfmakeraw`,
clearing ISIG: Ctrl-C arrives as a key event, not SIGINT [06 §2]. Both dua and gitui stay on
plain threads + crossbeam channels, and ratatui's own FAQ says async only pays off with many
concurrent tasks; tokio mpsc "drags in tokio for no benefit here" [06 §5]. indicatif 0.18.6
draws to its own stderr terminal at 20 Hz and assumes it owns the output — inside a
ratatui alternate-screen app the two fight over the terminal; none of the four studied tools
use it interactively [06 §4][04].

**Decision.**

- Stack: ratatui 0.30.2 + crossterm 0.29 (pinned pair, transitive deps checked in CI),
  unicode-width (+ unicode-segmentation) for CJK/emoji cell widths in path columns,
  signal-hook 0.4.4 for real SIGINT/SIGTERM/SIGTSTP routing as belt-and-braces [06 §1, §crates].
- Event-loop architecture (dua/gitui synthesis): UI thread owns `Terminal + AppState` and
  does `terminal.draw` then `crossbeam::select!` over input/scan/candidate/exec/agent/tick
  receivers, redrawing per event (diffs are cheap) with a 100 ms tick during long tasks; a
  dedicated input thread feeds `crossterm::event::read()` into a bounded(32) channel (dua's
  exact pattern; key-repeat `Full` drops are accepted); workers never touch ratatui — they
  send events (gdu's QueueUpdateDraw discipline) [06 §5].
- Ctrl-C = cancel current task, not quit (gdu semantics: partial results kept, "Stopping…"
  state); double-Ctrl-C or idle quits; a panic hook calls `ratatui::restore()`; every exit
  path flushes WAL/ledger first (R4 crash-consistency) [06 §2, §5].
- No tokio in the UI path — async is confined to the agent thread (ADR-6) [06 §5][07].
- indicatif 0.18.6 is CLI-mode-only (plan/apply/purge subcommands, stderr, 20 Hz,
  `abandon_with_message` on cancel), never inside the ratatui loop; in-TUI progress uses
  ratatui `Gauge`/`LineGauge` fed by the ticker→atomic-snapshot pattern [06 §4][04].
- Large candidate tables are virtualized `StatefulWidget`s with items rebuilt per draw
  (O(visible)); selection is re-found by stable path key when the list reorders mid-scan
  (dua's name-keyed restore) [06 §3].

**Consequences.**

- One event model everywhere: resize rides the normal crossterm stream (SIGWINCH via
  signal-hook), signals are converted into the same cancel path [06 §5].
- crossterm 0.30 does not exist yet; when a new major lands, ratatui moves (non-optional dep)
  and every direct crossterm use must move in the same commit [06 risks].
- `KeyEventKind::Press` filtering is kept even though macOS only emits Press (portability,
  no double-fires later) [06 risks].
- ratatui `TestBackend` gives snapshot tests per screen [06 §rec].

**Amended 2026-08-31 (GUI pivot):** Tauri 2.11.5 + wry 0.56.1 + tao 0.37.0 + Svelte 5.57
replace ratatui/crossterm as the UI layer — all Apache-2.0 OR MIT, verified in [12]; the
ratatui/crossterm decision above is retained as the record of the shelved TUI path. Scan/
apply progress reaches the frontend via `tauri::ipc::Channel` (not terminal redraws);
indicatif 0.18.6 stays CLI-only in the thin subset (ADR-14); window-vibrancy 0.8.0 supplies
the Apple-native material feel the owner demanded (D-10, D-11). The full pivot rationale
lives in ADR-18.

> **Status 2026-09-12 (audit):** every crate in this ADR's decision block — `ratatui`,
> `crossterm`, `unicode-width`, `unicode-segmentation`, `signal-hook`, `indicatif` — is a
> **research record, not a dependency**. None is declared in `Cargo.toml`, and no source file
> names them; the CLI reports progress through plain `eprintln`/stdout, and the GUI uses
> `tauri::ipc::Channel`. Kept as the record of the shelved TUI path and as the pinning rule if
> it is ever revived.

**Live topology (2026-09-13 — what actually ships).** The event model survived the pivot; the
terminal plumbing did not. Concretely:

- **Consumer:** the Tauri main process + Svelte webview (SPEC Part B §2.1). There is no UI
  thread owning `Terminal + AppState` and no `select!` — the webview has its own redraw
  scheduler and fan-in is `tauri::ipc::Channel<T>` (`ScanProgress`, `ExecProgress`,
  `AgentProgressMsg`).
- **No input thread and no tick thread.** Input arrives as `#[tauri::command]` invocations;
  progress cadence comes from the scan/exec emitters, not a 100 ms `AtomicBool` flip. The
  `Arc<AtomicBool>` cancel flag endures, now written by the `cancel_scan`/`cancel_agent`
  commands and polled by the walker (same `Arc`, `src-tauri/src/commands.rs`).
- **Core threads unchanged:** Scan (`scan::start → JoinHandle<ScanTotals>`, drop = cancel +
  join), the dua-core walker pool, Exec (sequential, `Sender<ExecEvent>`), Agent (own tokio rt).
  Classify is single-threaded and inline in the consumer drain loop — it was never a thread,
  and the TUI-era `cand_rx(1000)` channel does not exist.
- **What survived verbatim:** the `bounded(100)` scan-event cap, the no-tokio-outside-the-agent
  rule, workers-own-no-widgets discipline (now: workers never touch UI state), the D-15
  abort-and-keep-results cancel semantics, and `ratatui::restore()`'s replacement concern —
  every exit path still flushes WAL/ledger first (R4).
- **indicatif:** still CLI-only (ADR-14); the CLI prints plain lines and never declared it.

## ADR-6: Agent-loop HTTP stack — async-openai 0.41.3 (rustls TLS via its own reqwest), sans-IO loop

**Status:** upheld — Arys, 2026-08-30 (provider matrix: DeepSeek primary; others best-effort).

**Context.** SPEC §4's `agent/` is an OpenAI-compatible tool-calling loop that must also work
against DeepSeek, OpenRouter, Ollama, and vLLM (R3: endpoint/key/model configurable). [07]
verified the field: hand-rolling means reqwest + serde_json + an SSE layer (reqwest-eventsource
0.6.0, aging 2024-03, or eventsource-stream 0.2.3 — pick exactly one, never stack both) plus
re-implementing retry; async-openai 0.41.3 (MIT) already exposes typed tool-calling types,
`CompletionUsage` with cached/reasoning token details, the streaming usage chunk
(`stream_options.include_usage`), `OpenAIConfig::with_api_base` for all four endpoints, and a
built-in `SimpleRetryPolicy` (429/5xx + Retry-After, exponential 100 ms × 2^n capped 8 s).
reqwest 0.13 made rustls the default TLS (aws-lc-rs provider, roots via
`rustls-platform-verifier`) — no native-tls/OpenSSL anywhere. The `arguments` field of a tool
call is a JSON *string* the model may malform; the loop must repair/reject it. Provider quirks
are real: DeepSeek thinking mode requires echoing `reasoning_content` of all previous turns
(context/cost bloat), OpenRouter's final usage chunk deviates from the empty-`choices` shape,
older Ollama daemons emit id-less tool calls, and streaming can lose the final usage chunk.
For loop structure, rig's sans-IO steppable `AgentRun` (testable and serializable without a
network) matches the WAL/crash-recovery ethos, and swiftide's `StopReason` + control-tool
pattern gives the model an explicit terminal action [07 §rig].

**Decision.**

- async-openai 0.41.3 with only the `chat-completion` feature (`middleware` and `byot` remain
  uncompiled); tokio `rt-multi-thread` confined to the agent thread — the only async runtime
  in the binary [07 §rec][06 §5].
- `reqwest` is **not a direct dependency and is never named in source** — it arrives
  transitively through async-openai's default features (`default` → `rustls` → `reqwest`),
  which is also where the rustls/aws-lc-rs TLS provider comes from; base-URL overrides are
  tested at build time (0.13 broke rustls-default; `query`/`form` became opt-in) [07].
- Loop = sans-IO `AgentRun` state machine (rig-run pattern): `CallModel{turn, history}` /
  `CallTools{calls}` / `Done{reason}` steps; tools call local scan/classify/plan/execute
  in-process [07 §rig].
- Non-streaming `create_chat_completion` for the agent loop itself (`stream:false` — streams
  can drop the final usage chunk); SSE crates stay out of the dependency tree while
  async-openai parses streams itself [07 risks].
- Malformed `arguments` handled by validate/repair/skip/fail hooks with `tool_retry_limit=3`
  hashed per name+args (swiftide); Ollama id-less calls get a synthetic id (rig's minting);
  OpenRouter's final usage chunk parsed as an accounting frame, not a second turn; DeepSeek
  capped to non-thinking mode for agent loops [07 §providers, risks].
- Terminal control: `StopReason::{ModelStop, RequestedByTool, MaxTurns, TokenBudget,
  CostBudget, UserCancel, ToolFailLoop, ContextCap}` with `record_verdict` as the terminal
  tool (swiftide `Stop` pattern) [07 §budget].

**Consequences.**

- The run state is serializable/resumable — consistent with the ledger/WAL recovery story and
  testable against a stub OpenAI-compatible HTTP server [07 §rig][DESIGN §7].
- One HTTP stack serves both the agent and the NL-policy-compiler fallback (`policy/` reuses
  `ChatClient` — no second stack) [DESIGN §3.6].
- Feature churn risk is pinned away (exact `async-openai` pin; `reqwest` pinned only through
  `Cargo.lock` as a transitive dep; see crate-map §5); `byot` and other features remain
  uncompiled.

## ADR-7: Privacy architecture — opaque IDs + serializer chokepoint + CI leak test

**Status:** upheld — Arys, 2026-08-30 (fully opaque: no paths, no filenames).

**Context.** SPEC §3.3 sets the invariant: the LLM never sees file paths or names — only the
intent sentence (user-authored) and aggregate statistics. [07 §privacy] and the [02] tool
lessons justify the shape: a generic LLM agent that re-judges the same tree weekly is exactly
the failure the ledger replaces, and tool descriptions/names plus JSON-Schema `description`
fields are visible to the model *and* provider logs, so prompt discipline alone cannot hold
the invariant (a tool named "delete a node_modules dir" already leaks a name). The only
robust enforcement point is the single serializer through which every outbound message passes;
it is testable in CI. The ledger keeps the ID→path map locally, so undo needs no path to
cross outward.

**Decision.**

- Enforcement layers, strongest first [07 §privacy 1–5][DESIGN §5.4]:
  1. **Structural** — `scan`/`plan` tools return candidates as opaque IDs `C-001…` in
     deterministic size-desc order (stable across resumed runs); `apply`/`restore` accept
     ONLY ID lists; the ID→path map is resolved inside `execute/` after the decision and
     never crosses back to the model.
  2. **Serializer chokepoint** — one `serialize_tool_result` redacts every outbound payload;
     receipts echoed to the model carry only `{id, verdict, bytes, grade}`.
  3. **CI leak test** — `serde_json` of every outbound message must reject regexes for `/`,
     `~`, `/Users/`, `/Volumes/`, `\.(app|dmg|trash)$`; the build fails on any leak.
  4. **Tool-metadata hygiene** — tool names/descriptions/schema descriptions are path-free
     ("apply a reclaim verdict", not "delete a node_modules dir").
- The NL-policy LLM fallback sees only `AggregateStats { total_by_grade, count_by_grade,
  top_dir_shapes }` plus the verbatim intent sentence; output is JSON-schema'd, validated and
  clamped locally [07 §privacy 1,4].
- Context management truncates, never LLM-summarizes: tool results older than N turns become
  one-line ledger receipts — zero new tokens, no privacy exposure, reproducible [07 §context].
- Ledger asymmetry (documented, accepted): `apply`/`restore` records store real paths for
  undo; the ledger is local-only and the LLM never reads it [08 risks #6].

**Consequences.**

- Privacy is a property of the code, enforced mechanically (chokepoint + CI test), not of
  prompt wording or provider trust.
- Agent-facing payloads are bounded and shape-stable, which the CI test and the sans-IO
  snapshot round-trip both pin.
- The invariant holds regardless of which endpoint is configured — including local Ollama,
  where it still prevents accidental path exposure in stored prompts.

## ADR-8: Classification evidence model — evidence chains, 5 grades, CAS GC-only, orphan-first

**Status:** amended 2026-08-30 — Arys upheld the macOS evidence model and confirmed the per-OS taxonomy matrix (Windows [09], Linux [10]; platform-neutral core signals).

**Context.** SPEC §3.1 mandates evidence-driven reclaimability: classification from machine-
derived evidence, not directory names — name matching is precisely AppCleaner's false-positive
leftover mode [02 §4] and CleanMyMac's Safety Database is black-box and human-curated, so
judgment must be inspectable [02 §4 lessons]. [03 §0] enumerates the cross-cutting signals
(lockfile presence; sibling-manifest absence = orphan; mtime recency < 10 min; file locks via
flock markers or `lsof`-equivalents; `.git` ancestor; content-addressable-store shape;
hardlink/clone counts) and requires every verdict to carry ≥1 strong signal. Per-ecosystem
tables [03 §1–8] add the regen command and measured cost ("rebuildable ≈ 40 s"). Two hard
rules emerge: uv states it is "never safe to modify the cache directly" [03 §3] — CAS stores
must only ever be pruned wholesale via their own GC command; and npm `_cacache` mtimes update
on any install, so the hot-mtime rule must gate file-level deletion, not whole-store prune
commands [03 risks #7]. An orphan tree (no manifest in any ancestor) cannot be re-pinned from
a lockfile, so it is the strongest danger signal and is detected first.

**Decision.**

- Every `Candidate` carries `evidence: Vec<Evidence>{kind, detail, strength}` (Weak/Medium/
  Strong) plus a rebuild-cost grade `PureCache / RebuildableFromSource / Redownloadable /
  Expensive / HandsOff` and a `Regen{command, cost_secs, network}` when deterministic regen
  exists [03 §0][SPEC §3.1].
- Two-layer classify: (1) path-class rules recognize known junk shapes per ecosystem [03
  §1–8]; (2) evidence probes confirm, refute, or regrade — path-class alone never produces a
  verdict without ≥1 strong signal [03 §0].
- **GC-only rule for CAS stores** (`cacache`, pnpm store, uv buckets): never file-level
  deletion; propose `pnpm store prune` / `uv cache prune` / `npm cache` commands as
  `PruneCommand` entries with dry-run output captured into the manifest [03 §1, §3][DESIGN §3.3].
- **Orphan-first detection**: a missing manifest in any ancestor (`node_modules` w/o
  `package.json`, `.venv` w/o `pyproject.toml`) promotes the tree to a "project deleted?"
  prompt, never auto-delete — plan-level hard refusal `OrphanTree` [03 §0].
- `MtimeHot` (< 10 min subtree mtime) refuses auto-action; it gates file-level deletion but is
  waived for whole-store prune commands [03 §0][03 risks #7].
- `FileLock`, `Dataless` (`SF_DATALESS`), and flag-blocks (`UF_IMMUTABLE`, `SF_RESTRICTED`,
  `SF_NOUNLINK`, `UF_DATAVAULT`) ⇒ `HandsOff` with evidence; `com.apple.quarantine` xattr
  presence is `Redownloadable` evidence [03 §0][05 §5][05 §6].
- Byte honesty is part of the model: hardlinks (`nlink>1`) and APFS clones (`clone_id`) get a
  byte discount — deleting one of N links frees 0; sparse files (Docker.raw) report allocated,
  not apparent, size [03 §0, §6, risks #6][05 §6].
- Xcode DerivedData resolves `IDECustomDerivedDataLocation` before trusting the default path;
  Homebrew and Docker cleanups invoke `brew cleanup` / `docker system prune` rather than
  re-implementing their logic [03 §4–6].

**Consequences.**

- Verdicts are inspectable and diffable — the evidence chain ships in the plan manifest for
  review (Hazel's rules-as-artifact UX [02 §4]).
- Rule borrowing (cargo-sweep, kondo) stays read-only: cargo-sweep is unmaintained — borrow
  the rules, not the maintenance expectation [03 §2].
- Thresholds (`hot_mtime`, `ancestor_depth`, `min_bytes`) live in `config.toml` so SPEC §7's
  tuning questions are config changes, not code changes [DESIGN §3.6].

**Amended 2026-08-30 (3-platform):** the taxonomy becomes a per-OS evidence matrix — macOS
tables [03 §0–8] unchanged as the baseline, joined by Windows [09] and Linux [10]. The core
evidence signals (lockfile presence, sibling-manifest absence = orphan, mtime recency, file
locks, `.git` ancestor, CAS shape) are platform-neutral and carry over 1:1; only the read
substrate differs per OS (`lsof`-equivalent flock markers → Windows `CreateFileW`
sharing-violation probe; `st_nlink` → NTFS `nNumberOfLinks`; reparse points/junctions,
OneDrive placeholders, privilege boundaries, package-manager ownership, CACHEDIR.TAG are
per-OS additions) [09 §0][10 §0]. Path tables remain per-OS (`~/Library/Caches` ↔
`%LOCALAPPDATA%` ↔ XDG `.cache`); macOS mirrors map 1:1 onto the Linux/Windows equivalents
for pip/uv/npm/CAS stores. GC-only, orphan-first, and grading rules are unchanged on all
three [09 §delta][10 §delta].

## ADR-9: Token/cost accounting — per-call provider-reported usage, budget interrupt

**Status:** upheld — Arys, 2026-08-30 (warn 80%, hard stop 100% between turns).

**Context.** R6 requires per-call usage capture → running cost at configured prices →
interrupt at budget. [07 §cost] establishes the ground rules: provider-reported usage is the
only billing truth (DeepSeek's docs: "the actual number of tokens processed each time is based
on the model's return") — local tokenizers are never trusted for billing. Cost decomposes as
`(prompt − cached)·p_miss + cached·p_hit + completion·p_completion`, with reasoning tokens
already inside `completion_tokens` (never double-counted); DeepSeek reports cache hit/miss
directly (`prompt_cache_hit_tokens` / `prompt_cache_miss_tokens`), OpenRouter even reports
`usage.cost` in credits. Providers diverge in wire shape: OpenAI's final streaming chunk is
empty-`choices` with usage; OpenRouter's final usage chunk deviates (content-free delta
repeating `finish_reason`); Ollama maps `prompt_eval_count`/`eval_count` and has no
cached/reasoning breakdown; a stream interrupted mid-flight may lose the final usage chunk
entirely. DeepSeek prices peak/off-peak separately. `tiktoken-rs` is accurate only for OpenAI
models — DeepSeek/Ollama use their own BPE, so local estimates are approximations at best
(a field finding; the crate was never adopted — see the Decision block).

**Decision.**

- Per-call `CompletionUsage` is captured on every model response (non-streaming loop per
  ADR-6, so the usage chunk cannot be lost mid-stream) and persisted as a `budget` ledger
  record; running cost = Σ over the session [07 §cost][08 §8].
- Pricing lives in `config.toml` as a top-level `[[prices]]` array of tables per 1M tokens
  (`prompt_miss`, `prompt_hit`, `completion`, reasoning billed at completion rate when
  listed); DeepSeek peak/off-peak is modeled as multiple `ModelPrice` entries keyed by model
  name [07 §cost][DESIGN §3.9].
- OpenRouter `usage.cost` (credits) is accepted when present as a cross-check, but our
  computed cost remains the displayed number for uniform accounting [07 §cost].
- Lost/absent usage falls back to a chars/4 estimate (`cost::estimate_unverified`) that is
  clearly marked "unverified" and **never billed** into the running cost [07 risks]. The
  `tiktoken-rs` candidate was surveyed and **not adopted** (2026-09-12 audit) — it is accurate
  only for OpenAI's BPE anyway, never for DeepSeek/Ollama.
- The same chars/4 estimate is used for preflight ("will this turn fit `config.ctx_len` + how
  much budget remains before sending"), stopping *before* a `context_length_exceeded` 400
  [07 §context].
- Budget interrupt: three independent stop counters checked every turn — model-call budget
  (`max_turns`), Σ `total_tokens`, Σ cost ≥ `config.budget` — each surfacing a distinct
  `StopReason` (`MaxTurns` / `TokenBudget` / `CostBudget`); user Ctrl-C additionally cancels
  WAL-consistently [07 §budget][SPEC §5 R4].
- DeepSeek thinking mode is avoided in agent loops (reasoning echo bloat); if enabled anyway,
  `completion_tokens_details.reasoning_tokens` is tracked against the budget [07 risks].

**Consequences.**

- 开销明细表 (M5's expense sheet) is generated from ledger `budget` records — the audit trail
  is the same JSONL as everything else [DESIGN §8].
- Cost math is a pure function (`cost_of(usage, prices)`) — unit-testable, provider-quirk
  cases covered by the stub-server suite [07 §providers][DESIGN §7].
- Provider API divergence is confined to one adapter layer (usage parsing per endpoint);
  accounting and interrupt logic never see raw wire shapes.

## ADR-10: Grace window & purge confirmation — typed allocated byte count, 72 h restore

**Status:** upheld — Arys, 2026-08-30 (batch total, 72 h grace).

**Context.** The product claim [SPEC §1] is judgment + reversibility; `purge` is the only
irreversible step, so it needs a gate no other tool has. [02]'s tool survey shows why the
common gates fail: DaisyDisk's Collector staging ends in a 5 s countdown then **permanent**
unlink ("cannot be 'undeleted'") — a countdown is not an undo channel; CleanMyMac's
"Remove Securely" is *deliberately* irreversible — an anti-pattern; Trash-based tools (AppCleaner,
Hazel, Apple flows) survive only until the Trash is emptied and their Put-Back metadata format
is undocumented [02 §2, risks #5]; Hazel 6's "Revert Files" validates post-hoc undo demand,
and Hazel's Trash age/size policies mirror a grace window. Disk Drill's per-item
"recovery chance" framing shows the right vocabulary: state the rebuild cost ("rebuildable in
≈ 40 s") next to the risk. On the number to type: allocated size (`st_blocks`/`ATTR_FILE_
DATAALLOCSIZE`) is what purge actually frees — apparent size lies for pnpm stores, Docker.raw,
and APFS-compressed files [03 §6, risks #6][05 §6]. The 72 h grace figure is a starting
default, flagged as a SPEC §7 open question.

**Decision.**

- **Typed byte-count gate**: purge requires typing the exact allocated byte count of the
  operation (e.g. `5841673728` for 5.4 GiB); a mismatch refuses. The typed count is echoed
  into the `purge` WAL/ledger record as `bytes_freed` [DESIGN §3.4, §5.3][02 §2].
- The gate number is always **allocated** bytes — hardlink/clone-discounted per `(dev, ino)` /
  `clone_id` — never apparent size [03 §0][05 §6].
- **72 h grace window**: `apply` records `grace_expires_at = now + 72 h` in the undo record;
  `restore` reverse-renames from WAL `(original, quarantine)` pairs for entries not yet
  purged; recovery checks `grace_expires_at` — expired quarantines surface as purge
  candidates, never auto-purged [08 §8][DESIGN §5.2].
- Freed-space reports stay honest: if Time Machine local snapshots exist, freed bytes are
  quoted as "purgeable after snapshot expiry" [02 §3][05 §3].
- Gate ladder [DESIGN §5.3]: apply = soft gate (plan reviewed, reversible, Enter); purge =
  typed byte count; prune commands = dry-run preview first; hard-evidence refusals
  (`MtimeHot`/`FileLock`/`OrphanTree`/`Dataless`/flag-blocks) never enter the auto plan;
  `Expensive`/`HandsOff` grades require explicit per-entry opt-in; external/exFAT volumes
  refuse auto-apply with evidence.
- 72 h (and all thresholds) live in `config.toml` (`thresholds.grace_hours`) pending M5
  tuning — the SPEC §7 "is 72 h right?" question stays open by design [SPEC §7][DESIGN §3.6].

**Consequences.**

- Purge is the single irreversible step and it is impossible to hit by accident or through a
  visualization click (GrandPerspective's lesson: deletion must never be one-click on a map
  [02 §4]).
- `restore` after `purge` is undefined-by-construction for purged entries — the WAL only
  restores survivors; the grace window covers the regret case for everything else [DESIGN §4].
- The typed count doubles as an honesty check: it is computed from the same
  hardlink-discounted totals the plan manifest displayed, so a mismatch means the plan changed
  under the user.

## ADR-11: Platform strategy — native on macOS, Windows, Linux

**Status:** upheld — Arys, 2026-08-30 (native macOS+Windows+Linux confirmed; macOS-first order).

**Context.** The owner decided 2026-08-30 to make Disk Janitor native on all three desktop
OSes. The course context shaped it: the 9/1 comment deadline and 9/6 public demo run on this
Mac, and the rubric has no cross-platform requirement — so this is an owner-driven ambition,
not an imposed one, and it must not destabilize the demo lane. Research [09][10][11] established
the feasibility basis: the judgment/reversibility architecture (evidence engine, quarantine +
WAL restore, ledger, purge gate) carries over unchanged on all three — only the syscall
substrate differs [11 §8]. Windows and Linux taxonomies are canonical ([09][10] mirror [03]),
and every OS difference (atomic move, walker, durability, paths, dataless placeholders, trash)
has a verified primitive [11 §1–7].

**Decision.**

- **Native on macOS, Windows, Linux** — one Rust crate, per-OS impls behind a single
  abstraction layer: `platform/` per DESIGN §3.10, exposing the `AtomicMove` / `Walker` /
  `VolumeInfo` / `DatalessCheck` traits with `#[cfg]`-gated impls per OS (macOS `renamex_np`;
  Linux `renameat2` + fs probe; Windows `FileRenameInfoEx`) [SPEC §4][DESIGN §3.10][11 §7].
  No emulation layer, no Electron/Python substrate — syscall-native on each OS [11].
- **macOS-first implementation order** for the 9/1–9/6 deadlines: M1 lands the macOS
  scan/classify core and dua-core walker; the `platform/` traits are sketched at M1 so M2 can
  slot impls in without re-plumbing; Linux impls wire in at M2 (`renameat2`, mountinfo fstype
  probe, `fsync(dirfd)` pair), Windows at M2+ (`FileRenameInfoEx`, `FlushFileBuffers`,
  `FileIdBothDirInfo` bulk walk) [DESIGN §8][09][10][11].
- **Guarantee table per OS, stated honestly** (SPEC §8 / DESIGN §5.5): macOS full atomicity
  (`renamex_np(RENAME_EXCL)`); Linux atomic on ext4/btrfs/tmpfs/xfs and most fs ≥ 4.9 kernels,
  NFS refuses `RENAME_NOREPLACE` (probe per volume, fall back or refuse); Windows no-clobber
  apply is atomic via `FileRenameInfoEx` on Win10 ≥1607, restore replace is atomic on 1709+
  and degrades to non-atomic-w.r.t.-open-targets `MoveFileExW` before, with **no dir-fsync —
  NTFS journaling carries metadata durability (ADR-12)** [SPEC §8][11 §1/§2]. "What is NOT
  promised is identical atomicity on every platform" — the deltas are documented, not hidden
  [SPEC §8].

**Consequences.**

- Crate surface grows: per-OS cfg-gated additions (Windows `windows` 0.62.2 / `winreg` 0.56.0;
  Linux raw `libc` FFI — `renameat2`, `statx`, `syncfs`; portable `dirs`) — none affect the
  macOS build until M2 [DESIGN §2.4][11]. `rustix`, `same-file`, and `filetime` were surveyed
  as candidates and are **not dependencies** (2026-09-12 audit); Linux `statx`/`renameat2`
  were already done through raw `libc` with locally defined constants, so no new crate is
  needed there.
- CI becomes a matrix (macOS + Linux at minimum; Windows at M2+) — the demo Mac is unaffected
  either way.
- Taxonomy triples: [03] macOS is joined by [09] Windows and [10] Linux as per-OS rule tables
  behind the same evidence engine (ADR-8 amendment).
- The 9/1–9/6 demo is unaffected: everything demoed runs the macOS path, which remains exactly
  as designed in ADR-1/2/3.

**Amended 2026-08-31 (GUI pivot):** the platform strategy is unchanged by the pivot to a
GUI-first desktop App (ADR-18) — Tauri **is** the cross-platform UI: one Svelte frontend
over one Rust core, with per-OS native depth still coming from the `platform/` abstraction
(DESIGN §3.10). The `AtomicMove` / `Walker` / `VolumeInfo` traits and their per-OS impls
(ADR-1/ADR-2 amendments, ADR-12) are untouched; macOS-first order and the honest per-OS
guarantee table (SPEC §8) stand as written (D-11).

## ADR-12: Windows durability & restore semantics — WAL journal as restore authority

**Status:** upheld — Arys, 2026-08-30 (runtime probe + honest guarantee tiers, incl. pre-1709 MoveFileExW fallback with clear messaging).

**Context.** Windows lacks the primitives macOS's durability plan (ADR-3) leans on [11 §2]:
there is no `F_FULLFSYNC` equivalent — `FlushFileBuffers` writes "all the buffered information
for a specified file to the device" but its documented behavior is a cache→device write, not the
platter-barrier request `F_FULLFSYNC` makes (and it requires a `GENERIC_WRITE` handle);
`FILE_FLAG_WRITE_THROUGH` is the per-open alternative but "file system metadata is always
cached"; volume-wide `FlushFileBuffers` is admin-only; and there is **no directory-fsync API at
all** — the parent-dir-entry sync that closes POSIX's textbook gap (ADR-3) does not exist, with
metadata durability riding the NTFS journal instead (documented weaker than `F_FULLFSYNC`).
Separately, there was no POSIX rename before Win10 1709: `FileRenameInfoEx` appears in 1607
headers, but POSIX_SEMANTICS (atomic replace with open handles staying valid) is usable from
1709 per rust-lang#123985, while `[MS-FSCC]` states no minimum version — so the gate must be a
runtime probe, never an assumption [09 risks #1][11 §1]. OS-side restore is not a fallback
either: Recycle Bin restore is shell-mediated and our WAL already owns the
`(original, quarantine)` pairs (ADR-2) [11 §2/§6].

**Decision.**

- **The WAL journal is the restore authority on Windows, not OS Trash.** Recovery replays the
  recorded `(original_path, quarantine_path)` pairs exactly as on macOS; the shell Recycle Bin
  (`IFileOperation`) is at most a purge-time handoff surface and never part of the restore path
  [11 §2/§6][DESIGN §5.5].
- **`FlushFileBuffers` is called at the same WAL state transitions as macOS `F_FULLFSYNC`**
  (ADR-3): intent synced before mutation, result recorded after, `FILE_FLAG_WRITE_THROUGH` on
  WAL opens; the missing directory-entry flush is compensated by ordering — the quarantine move
  happens strictly after the intent record is flushed, so a crash between flush and move leaves
  the journal describing no mutation, and post-move re-verification at recovery reconciles
  observed state against recorded intents (ADR-4's roll-forward/drop-tail logic) [11 §2].
- **Document the residual cases honestly** [11 §2/risks]: (a) NTFS metadata durability is
  journal-backed, weaker than a platter barrier — the documented guarantee delta of §5.5; (b)
  restore's replace step is atomic (old handles valid) only on Win10 1709+ via
  `REPLACE_IF_EXISTS | POSIX_SEMANTICS`; on 1607–1708 it degrades to
  `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, non-atomic w.r.t. open targets
  (`ERROR_ACCESS_DENIED`/sharing violations) — restore correctness still holds because the WAL
  is written first, and only concurrent-reader races surface as errors [11 §1][DESIGN §5.5];
  (c) SMB shares have no POSIX rename semantics and are treated as external volumes [11 risks #2].

**Consequences.**

- Restore fidelity is identical across all three platforms because it is replay from the same
  journal, not OS Trash semantics — the platform difference confines itself to *when* a crash
  can lose the last in-flight mutation, never *whether* restore works [11 §2/§8].
- The purge gate (ADR-10) is unchanged: Windows already refuses auto-apply on external/network
  volumes, and the typed byte count is computed from the same hardlink-discounted
  (`nNumberOfLinks`) totals [09 §0].
- Windows support ships with the documented guarantee table (SPEC §8), not a silent promise of
  macOS-grade atomicity [SPEC §8][DESIGN §5.5].

## ADR-13: Bilingual UI — CN primary + EN secondary

**Status:** accepted 2026-08-31 (owner-confirmed); **amended 2026-09-08 (D-31)** —
rendering changed from simultaneous dual labels to single-language mode with a 中/EN
switcher; the catalog and CN/EN completeness requirements below stand unchanged.

**Context.** The owner chose bilingual labels with **Chinese primary, English secondary**
 (D-02, D-12). Chinese carries the persuasion — the trust-building copy, the refusal voice
 (P1: 拒绝+证据+替代), the brand invariants (恢复是权利, D-17) — while English serves
 identifiers and technical terms the audience already knows (`Quarantine`, `WAL`,
 `DerivedData`). Half-bilingual UIs fail by omission: a single untranslated error string
 breaks the promise at exactly the moment trust matters most.

**Decision.**

- **Central string catalog** via `svelte-i18n 4.0.1` [12] — no hardcoded user-facing
  strings in Svelte components; every key has CN + EN from day one.
- **CN for persuasion, EN for identifiers/terms** (D-12): prose, buttons, refusals,
  evidence labels are Chinese-primary with English secondary; command names, crate terms,
  file formats, and grade identifiers stay English.
- Every user-facing string is bilingual **from day one** — including errors, empty states,
  and the uninstall flow (ADR-16); a missing translation is a build/lint failure, not a
  runtime fallback to hardcoded text.
- The catalog lives with the frontend (crate-map §1g); UX-SPEC.md (ADR-18) owns the copy.

**Consequences.**

- Refusals, evidence chains, and the purge gate speak the user's language at the trust
  boundary (P1, P6) — the strings are part of the safety surface, reviewed like code.
- Adding a locale later is additive catalog work, not a refactor; the component convention
  (keys only) is already enforced.
- Ledger/WAL records stay locale-neutral: stored copy is identifiers + data, UI-localized
  at render (consistent with ADR-4's replay-first design).

**Amended 2026-09-08 (D-31).** After the interactive mockup exposed the cost of
simultaneous dual labels — 「中文 English」 pairs doubled the text on every control and
read as clutter ("text soup") against the clean reference (DaisyDisk) — the owner revised
the rendering rule: one locale renders at a time, 中文 by default, EN via a titlebar
switcher that swaps the whole UI. What survives unchanged: the central `svelte-i18n`
catalog, CN/EN completeness from day one (missing key = build failure), and CN for
persuasion / EN for identifiers. Simultaneous bilingual display was the wrong rendering
of a right requirement.

## ADR-14: CLI surface contract — thin subset beside a GUI-primary product

**Status:** accepted 2026-08-31 (owner-confirmed).

**Context.** The product is GUI-primary (D-10, ADR-18), but a CLI subset earns its keep as
infrastructure: CI harnesses, crash-injection tests, and agent plumbing (D-08). The risk is
scope creep — a full CLI becomes a second product to keep consistent with the GUI, and a
convenience flag on `purge` would betray P4.

**Decision.**

- The CLI is a **thin subset** (D-08): `scan --json` (machine-readable output for CI and
  agent tooling), `plan` (review a plan manifest), `restore` (WAL replay), and `interactive
  purge` (the ADR-10 typed-byte-count gate in the terminal). Nothing more.
- **`purge` NEVER gains a `--yes`/flag bypass** (P4) — the interactive gate is the product;
  any "scriptable purge" request is refused by design.
- CLI flags are plumbing, not surface: they exercise the same Rust core the GUI calls;
  behaviors diverge only in rendering, never in safety semantics.
- Packaging/distribution work (D-16) targets GUI installers; the CLI ships inside the same
  binary, not as a separate artifact.

**Consequences.**

- CI and crash-injection tests drive the product exactly as a user would (P7), through the
  same commands and gates — no test-only backdoors.
- The CLI stays reviewable in one screenful; feature parity questions are closed by policy,
  not renegotiated per milestone.
- AGENTS/agent integrations (D-08 plumbing use) depend only on the four commands, a stable
  contract smaller than the GUI's.

## ADR-16: Uninstall contract — restore-or-confirm before state removal

**Status:** accepted 2026-08-31 (owner-confirmed).

**Context.** The brand invariant is 恢复是权利 — restore is a right (D-17). Quarantine roots
live on user volumes (ADR-2), while app state lives in `~/.disk-janitor/` (ADR-4); deleting
the app without reconciling the two would strand quarantined files whose only undo metadata
is the WAL — the exact trap Apple's undocumented Trash Put-Back format creates [05 §7],
which this product exists to avoid.

**Decision.**

- **Uninstall requires restore-or-explicit-confirm of every surviving quarantine across all
  volumes** before any state removal (D-17): each per-volume `<mnt>/.Trash-janitor/` op dir
  is either restored via WAL replay or explicitly confirmed by the user; no default-confirm,
  no skip-all.
- Only after every quarantine is resolved is `~/.disk-janitor/` (ledger, WAL, lock) removed.
- **Orphaned-quarantine detection**: a volume `.Trash-janitor` dir with no matching WAL is
  warned at startup — the UI offers inspection and manual resolution; it never deletes
  silently (P5, P6).
- The uninstall flow is a settings-screen surface in the GUI spec (D-17), with the CLI
  (ADR-14) exposing the same contract non-interactively for tests.

**Consequences.**

- No Disk Janitor uninstall can leave files in limbo — the guarantee holds even when the
  WAL was lost (orphan path degrades to explicit manual confirm, never auto-delete).
- Startup gains a cheap per-volume existence probe; the cost is one `stat` per mounted
  volume's quarantine root.
- The contract is testable end-to-end: uninstall with survivors → refused; restore all →
  state removed clean.

## ADR-17: Data format versioning — v1 frozen, forward-compat replay

**Status:** accepted 2026-08-31 (owner-confirmed).

**Context.** The owner set the posture explicitly: "not just for a class — real product"
 (D-18). The ledger and WAL are append-only JSONL 台账 (ADR-4) and the plan manifest is the
review artifact (ADR-10); any format drift between sessions on the demo Mac — or across
machines via exports (D-19) — must degrade loudly, never fake success (P6), and the ledger
must never be silently deleted or rewritten (P5).

**Decision.**

- **Ledger / WAL / manifest format v1 is frozen through the course** (through 9/10).
- **Forward-compat replay** (D-18): unknown record kinds and unknown fields inside known
  kinds are skipped **and counted** — the recovery report surfaces `skipped_unknown=N`, never
  silently drops; `serde` `deny_unknown_fields` stays **OFF** on all envelope/record/manifest
  types.
- Schema evolution after 9/10: version-gated migration passes only — a reader seeing a higher
  `v` either migrates in a pass that is logged and reviewable, or refuses with an explicit
  message; never best-effort guesswork.

**Consequences.**

- Adding a record kind mid-course (e.g. `budget` gaining `model` per D-20) is an additive
  change old readers tolerate — no migration needed within v1.
- The skip-and-count contract doubles as the corruption story's honesty check: a replay that
  skips records reports them, consistent with ADR-4's operations-ledger stance.
- Real-product posture over courseware convenience: no throwaway "just reformat the ledger"
  escape hatch exists (D-18).

## ADR-18: GUI paradigm — Tauri + Svelte desktop App

**Status:** accepted 2026-08-31 (owner-confirmed).

**Context.** The owner's vision: an Apple-native-feeling desktop app, frictionless for
people who barely know what a filesystem is — a real product, not a class exercise (D-10).
The course's R2 in fact permits Web / 交互式终端 CLI / 桌面 App / 手机 App — any one (D-09);
the earlier "ratatui required" reading was a repo-doc choice, now retracted, and the agent's
prior "R2 conflict" claim was withdrawn. Research [12] verified the stack: Tauri 2.11.5
(Apache-2.0 OR MIT) with wry 0.56.1 / tao 0.37.0, Svelte 5.57, `tauri::ipc::Channel` for
progress streaming, window-vibrancy 0.8.0 for native material effects, and per-OS packaging
in the same note (D-11).

**Decision.**

- **GUI-first desktop App** (D-10): Tauri 2.11.5 + Svelte 5.57 [12] is the product surface;
  ADR-5's TUI is shelved.
- **Rust core unchanged** (D-11, R1): `scan/`/`classify/`/`plan/`/`execute/`/`agent/` and
  the `platform/` traits (ADR-11) are untouched; the GUI is a new frontend over the same
  commands the CLI thin subset (ADR-14) wraps.
- **UX-SPEC.md governs UI design** — layout, bilingual copy (ADR-13), first-run dashboard
  (D-05), settings/uninstall (D-17) are specified there, not improvised in components.
- Demo is a **product video + live preview** (D-21/D-22), not a terminal screencast.

**Consequences.**

- crate-map gains a frontend section (§1g per D-11): Tauri shell, Svelte app, svelte-i18n
  catalog (ADR-13), window-vibrancy plugin; packaging pipeline per [12] feeding D-16's
  installer plan.
- SIGINT-era questions — raw-mode keymaps, ctrl-c routing, minimum-terminal-size handling —
  are dissolved, not deferred: they have no GUI counterpart. Interrupt semantics move to the
  GUI's cancel affordances under the same WAL-consistent rules (R4, D-15).
- The agent loop (ADR-6) stays backend: the LLM proposes, the human confirms in the GUI
  (P2, D-01).

---

*ADRs 1–12 verdict-stamped 2026-08-30 (9 upheld, 3 amended — see footer below), derived from
research notes [02]–[11]; iterate alongside `docs/SPEC.md` (course rule). Later decisions:
D-32…D-38 (2026-09-10, refusal enum / dua-core 4.0.0 / HTTP ChatClient / demo-data switch)
are recorded in `docs/decision-sheet-2026-09-10.md`.*

---
*Verdicts recorded 2026-08-30 in a single review session: 9 upheld, 3 amended (ADR-1, ADR-2, ADR-8), 0 rejected, 0 contested. Amendments propagate into DESIGN §3.10, SPEC §7–8, crate-map §1f/§3 (landed same day).*
> Decision register D-01…D-31 (22 from the 08-31 grill; D-23…D-30 added by wave 2, 09-01; D-31 added 09-08 after the design-language v2 rework). **D-32…D-38 were raised 2026-09-10 and live in `docs/decision-sheet-2026-09-10.md` — this register stops at D-31 by design**;
*2026-08-31 session: GUI pivot (Tauri + Svelte per notes [12]/[13]) — ADR-5 amended, ADR-11
amended, new ADR-13/14/16/17/18 from 22 owner decisions (decision register, 3 grill rounds).
Statuses read "accepted (owner-confirmed)" pending the owner's line-by-line stamp pass over
those five texts; every substantive decision was owner-answered in the register.*


---

# Appendix — Decision register (2026-08-31 grill, owner-confirmed)

> Decision register D-01…D-31 (22 from the 08-31 grill; D-23…D-30 added by wave 2, 09-01; D-31 added 09-08 after the design-language v2 rework). D-32…D-38 are in `docs/decision-sheet-2026-09-10.md`;
> the 'docs to amend' column cites pre-merge file names — see the citation map at the top of this file.

Format: ID | decision | owner answer (verbatim or paraphrased) | docs to amend

## Round 1 (safety, identity, UX direction)
D-01 agent_authority | Agent is PROPOSE-ONLY: returns plan ID, human confirms in UI; LLM never executes quarantine moves | DESIGN §3.7 tools table + ADR-7 + new principle P2
D-02 ui_language | BILINGUAL support, Chinese primary + English secondary — **rendering revised 09-08 (D-31): one locale at a time via 中/EN switcher** | ADR-13 (as amended) + §3 conventions
D-03 error_voice | 拒绝+证据+替代: every refusal = verdict + evidence + safe alternative | new principle P1 + DESIGN §5 error catalog note
D-04 ui_paradigm | (see D-13) | —
D-05 first_run | Dashboard + hint line (no wizard state machine) | GUI spec §first-run
D-06 telemetry | ZERO telemetry, stated explicitly in README + DESIGN | SPEC §8/principles + README
D-07 product_license | No LICENSE file until after course grading (course owns submission) | README note; do NOT create LICENSE
D-08 cli_scripting | GUI-primary; thin CLI subset kept (scan --json / plan / restore / purge-interactive) for CI + agent plumbing; purge NEVER gets --yes | ADR-14 (CLI surface) + DESIGN §3.8-replacement

## Round 2 (paradigm pivot)
D-09 r2_correction | COURSE R2 ACTUALLY ALLOWS: Web / CLI 交互式终端 / 桌面 App / 手机 App (any one). Previous "ratatui required" was a repo-doc choice, NOT course mandate. Agent's earlier "R2 conflict" claim was wrong and retracted. | SPEC §5 R2 row rewrite
D-10 paradigm | GUI-FIRST desktop App (owner explicit: "Apple native feel, frictionless, usable by people who barely know what it is", "not going to be a CLI", "not just for a class — want a real product") | SPEC §1/§4/§5 + DESIGN §3.8 rewrite + ADR-5 amendment
D-11 gui_stack | Tauri + Web UI (owner confirmed recommendation). Rust core unchanged (R1). Research: docs/research/12-tauri-gui-stack.md — Tauri 2.11.5, Svelte 5.57, svelte-i18n, window-vibrancy 0.8.0, ipc::Channel for progress; packaging per OS in note 12 | ADR-5 amendment → "GUI stack: Tauri 2.11.5 + Svelte 5.57" + crate-map §1g
D-12 ui_language_detail | Bilingual = CN primary, EN secondary (owner picked option "Bilingual labels", round-2 confirmation question defaulted to CN-primary when picker cancelled) — **rendering revised 09-08 (D-31): switcher replaces inline dual labels** | ADR-13 (as amended)

## Round 3 (NFRs, operations, course artifacts)
D-13 perf_budget | Scan ≥1 GiB/s metadata-only on M1 Pro demo Mac; scan home dir <2 min; bench test in CI | DESIGN §7 + NFR table in DESIGN §1
D-14 memory_ceiling | <500 MB RSS with 1M synthetic entries fixture assertion | DESIGN §7 NFR
D-15 interrupt_semantics | scan/classify: interrupt = drop incomplete batches (fine); apply: WAL auto-resume next launch; purge: runs to completion once confirmed (no mid-purge stop) | DESIGN §5 + R4 acceptance + ADR-9-adjacent note
D-16 distribution | CI-built signed/notarized installers per OS (macOS dmg needs Developer ID $99/yr for notarization — else ad-hoc + right-click-open instructions; Windows msi/nsis signed; Linux AppImage) | ADR-14/15 (packaging) + DESIGN §8 M-note + note 12 packaging section
D-17 uninstall | Uninstall = restore-or-explicitly-confirm every surviving quarantine BEFORE removing state (brand invariant: 恢复是权利) | DESIGN §5 + new ADR-16 (uninstall contract) + GUI spec settings screen
D-18 format_versioning | Owner: "not just for a class — real product." Decision: product-grade policy = forward-compat replay (unknown kinds/fields skipped + counted, serde deny_unknown_fields OFF) + v1 frozen until post-course; migrations allowed only with version-gated migration passes after 9/10 | ADR-17 (data format versioning) + DESIGN §3.5 note
D-19 ai_log_workflow | Per-milestone export to docs/ai-logs/M<N>-<date>.md (excerpts + timestamps, API keys/privacy redacted) | new docs/ai-logs/ convention + README note
D-20 expense_sheet | Standard columns: date / model / calls / prompt+completion tokens / cost USD + CNY; ledger budget rec gains model field; janitor expense-report subcommand; scope = LLM API spend | DESIGN §3.5 ledger rec payload + ADR-9 note
D-21 demo_dataset | Demo = PRODUCT VIDEO (Screen Studio/Hyperframes capture) + live preview; dataset = synthetic fixture tree per SPEC §7 (never real user data); capture can be automated via browser/process automation — owner involvement optional, keyless fallback D-22 trajectory replay | DESIGN §8 M5 note + SPEC §7 fixtures
D-22 demo_llm | Product video + live preview after (owner explicit). Fallback chain: live DeepSeek → pre-recorded trajectory replay | DESIGN §8 M5 note

## Round 4 (research wave 09-01, owner-confirmed via decision sheet)
D-23 brand_identity | EN identity = COINAGE on the undo promise (还我磁盘 stays CN-primary brand; "Disk Janitor" demoted to internal/working title — live collision audit found 4+ shipping products using it, note 20 §1); **EN candidate picked 09-08: Giveback** (motif: restore arrow ↩ + byte counter, "the undo moment is the logo" — icon at docs/mockups/icon.svg) | SPEC Part C §1 + note 20 §3
D-24 one_tap_set | One-tap clean auto-marks PureCache + RebuildableFromSource, surfaces explicit confirm list 「N 项待确认 N awaiting confirm」 for everything else; enabled only after a scan exists | SPEC Part C §3.2 + note 15 §2/§6
D-25 vault_tab | 恢复站 Recover vault = TAB inside Ledger (台账|恢复站), not a separate screen; actionable subset of ledger (apply records in grace), grouped by plan session, countdown per row, 待销毁 pending section, WAL-replay banner | SPEC Part C §3.7 + note 17 §6
D-26 restore_collision | Restore collision triad default = RENAME (Keep-Both, reverse-rename to original-<n>); 跳过 skip and 覆盖 replace (typed confirm) available | SPEC Part C §4.4 + note 17 §5
D-27 purge_gate | Typed byte-count gate stays at PURGE (single final total for one-tap), never per-batch — signature moment not weakened | SPEC Part C §4.2 + note 15 risks #2
D-28 error_catalog | E-#### failure/error catalog (E1001–E3003, CN/EN P1 copy) is paste-ready appendix to SPEC Part C; source of truth note 18 | SPEC Part C §3.7 + note 18
D-29 countdown_clock | Vault countdown = ledger-recorded monotonic elapsed, never wall clock (clock jumps cannot coerce grace) | SPEC Part C §3.7 + note 17 risks #4
D-30 parity_copy | Demo/packaging parity claims use =/≈ delta language (note 19 §3), deltas in the same breath — never flat 3-platform claim | SPEC Part A §8 + note 19
D-31 ui_language_rendering | Owner design critique 09-08 ("text soup"): single-language mode — one locale rendered at a time, 中文 default, 中/EN titlebar switcher swaps the whole UI; catalog + CN/EN completeness unchanged from ADR-13 | ADR-13 amendment + SPEC Part C §3 conventions + §6 component inventory + mockup v2

## Extracted design principles (from audit, owner-endorsed by round choices)
P1 拒绝+证据+替代 — refuse with evidence, offer the safe alternative (D-03)
P2 LLM 建议代码决策 — the LLM advises; local code decides (D-01)
P3 离线是基线 — offline is the baseline; network is an enhancement
P4 不可逆无旁路 — no bypass for irreversible steps; purge has no --yes ever (D-08)
P5 台账即审计 — ledger is append-only, user-inspectable, never silently deleted (D-18 retention posture)
P6 诚实降级 — degrade loudly, never fake success (platform gaps, refusals, unreachable endpoints)
P7 演示可复现 — every public claim is generated from the ledger or a committed fixture, never hand-typed

## Not decided (explicitly deferred, do NOT decide unilaterally)
X-01 product LICENSE file — post-course (D-07)
X-02 demo dataset + video script — needs note 13 + owner review
X-03 presentation (9/10) structure — M5
