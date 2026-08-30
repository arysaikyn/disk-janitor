# Disk Janitor — Architecture Decision Records

> ADR log. Each entry: Status / Context / Decision / Consequences.
> Citations: research notes `docs/research/NN-*.md` cited as `[NN §x]`, spec as `[SPEC §x]`;
> all notes verified 2026-08-30. Companion to `docs/DESIGN.md`, which details the designs
> these ADRs commit to. Every ADR below is Status: accepted.

## Index

- ADR-1 Walker: dua-core 3.3.0 (jwalk unmaintained; ignore kept for patterns + fallback)
- ADR-2 Quarantine primitive: same-volume `renamex_np(RENAME_EXCL)` into per-volume hidden roots
- ADR-3 fsync strategy: `F_FULLFSYNC` via std, dir-sync boundaries, sync per state transition
- ADR-4 Ledger/WAL format: framed JSONL + crc32 chain + flock, skip-and-continue recovery
- ADR-5 TUI stack: ratatui 0.30.2 + crossterm 0.29 pinned; threads + crossbeam; indicatif CLI-only
- ADR-6 Agent-loop HTTP stack: async-openai 0.41.3, sans-IO loop, reqwest rustls
- ADR-7 Privacy architecture: opaque IDs + serializer chokepoint + CI leak test
- ADR-8 Classification evidence model: evidence chains, 5 grades, CAS GC-only, orphan-first
- ADR-9 Token/cost accounting: per-call provider-reported usage, budget interrupt
- ADR-10 Grace window & purge confirmation: typed allocated byte count, 72 h restore

## ADR-1: Walker — dua-core 3.3.0 over jwalk and ignore

**Status:** accepted (2026-08-30)

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

- dua-core 3.3.0 pinned exact (`=3.3.0`) as the primary walker in `scan/`:
  `walk_roots(roots, threads, Order::ParentFirst, opts, descend)` [04][SPEC §4].
- Mount skipping is a per-entry `MetadataExt::dev()` compare inside the `descend` predicate —
  not mount-table parsing; firmlink-safe because firmlinked `/`, `/Users`, and
  `/System/Volumes/Data/...` share `st_dev` [04 §dua][05 §1].
- ignore 0.4.33 is kept only for `gitignore::GitignoreBuilder` exclusion patterns (classifier
  and policy globs), globset for user ignore globs, walkdir 2.5.0 as the serial fallback [04].
- Pre-planned fallback if dua-core churns before M1: `ignore::WalkParallel` + own channel +
  `std::thread::available_parallelism()`, with an explicit drain/flush cancel path [04].

**Consequences.**

- Metadata (mtime, BSD flags, nlink, allocsize, APFS clone-id) arrives with enumeration; the
  second-stat pass disappears; drop-the-iterator = cancel + join workers is exactly R4's
  interrupt primitive [04].
- dua-core is ~10 days old as a standalone crate: pin exact, audit the macOS unsafe
  (`attrlist` buffers) before shipping [04].
- While on dua-core, the async-Quit drain bug class never exists; it returns only if the
  ignore fallback is ever activated.

## ADR-2: Quarantine primitive — same-volume `renamex_np(RENAME_EXCL)` moves

**Status:** accepted (2026-08-30)

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
- Raw `libc` decls: nix 0.31.3 has no `renamex_np` (`RenameFlags` is Linux-only); constants
  missing from libc/nix (`RENAME_NOFOLLOW_ANY=0x10`, `SF_DATALESS`, `UF_DATAVAULT`, …) are
  defined locally [05 §reusable].

**Consequences.**

- `apply` frees zero bytes and never loses a file even on crash (rename crash-atomicity);
  quarantined trees lose nothing (xattrs/flags preserved) [05 §1][05 §6].
- Plan-time refusals pre-map the exact errno families apply would hit (`EXDEV`, `ENOTSUP`,
  `EDEADLK` for dataless, `EPERM` for immutable-flag entries) [05 §2][05 §5][05 §6].
- Restore never depends on Trash Put-Back metadata — the WAL owns the
  `(original, quarantine)` pairs [02 risks #5][05 §7].

## ADR-3: fsync strategy — `F_FULLFSYNC` via std, dir-sync boundaries, sync per state transition

**Status:** accepted (2026-08-30)

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

**Status:** accepted (2026-08-30)

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
- Single writer guarded by `flock(LOCK_EX | LOCK_NB)` on `~/.disk-janitor/janitor.lock` via
  fs4 (maintained rustix-based fork; nix `Flock` RAII as alternative) — second instance fails
  fast with a clear message [08 §6].
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

**Status:** accepted (2026-08-30)

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

## ADR-6: Agent-loop HTTP stack — async-openai 0.41.3, sans-IO loop, reqwest rustls

**Status:** accepted (2026-08-30)

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

- async-openai 0.41.3 with only the `chat-completion` (+ `middleware`) features; tokio
  `rt-multi-thread` confined to the agent thread — the only async runtime in the binary
  [07 §rec][06 §5].
- reqwest stays on its rustls default; `json` enabled explicitly and base-URL overrides
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
- Feature churn risk is pinned away (exact async-openai/reqwest pins; see crate-map §5);
  `byot` and other features remain uncompiled.

## ADR-7: Privacy architecture — opaque IDs + serializer chokepoint + CI leak test

**Status:** accepted (2026-08-30)

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

**Status:** accepted (2026-08-30)

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

## ADR-9: Token/cost accounting — per-call provider-reported usage, budget interrupt

**Status:** accepted (2026-08-30)

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
entirely. DeepSeek prices peak/off-peak separately. tiktoken-rs is accurate only for OpenAI
models — DeepSeek/Ollama use their own BPE, so local estimates are approximations at best.

**Decision.**

- Per-call `CompletionUsage` is captured on every model response (non-streaming loop per
  ADR-6, so the usage chunk cannot be lost mid-stream) and persisted as a `budget` ledger
  record; running cost = Σ over the session [07 §cost][08 §8].
- Pricing lives in `config.toml` as `[[model.prices]]` per 1M tokens
  (`prompt_miss`, `prompt_hit`, `completion`, reasoning billed at completion rate when
  listed); DeepSeek peak/off-peak is modeled as multiple `ModelPrice` entries keyed by model
  name [07 §cost][DESIGN §3.9].
- OpenRouter `usage.cost` (credits) is accepted when present as a cross-check, but our
  computed cost remains the displayed number for uniform accounting [07 §cost].
- Lost/absent usage falls back to a tiktoken estimate that is clearly marked "unverified" and
  **never billed** into the running cost [07 risks].
- tiktoken-rs is used for preflight only: "will this turn fit `config.ctx_len` + how much
  budget remains before sending", stopping *before* a `context_length_exceeded` 400 [07 §context].
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

**Status:** accepted (2026-08-30)

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

---

*ADRs 1–10 accepted 2026-08-30, derived from research notes [03]–[08] and the tool-landscape
note [02]; iterate alongside `docs/DESIGN.md` and `docs/SPEC.md` (course rule).*
