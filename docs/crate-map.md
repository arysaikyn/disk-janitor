# Disk Janitor — Crate Map (Dependency Plan)

Verified 2026-08-30. Every version/license claim cites a research note:
[04] `docs/research/04-rust-walk-and-metadata-crates.md` · [05] `05-macos-fs-edge-cases.md` ·
[06] `06-tui-stack-ratatui.md` · [07] `07-llm-agent-loop-rust.md` · [08] `08-ledger-wal-crash-safety.md`.
[01] (`01-oss-disk-analyzers.md`) and [02] (`02-macos-cleanup-landscape.md`) are cited only for
supplementary facts (dust config precedent; Commons Clause license caveat; serde licensing).
Owning modules (SPEC layout): scan / classify / plan / execute / ledger / policy / agent / ui / config.

## 1. Crates table

### 1a. Scan — walking + metadata

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| dua-core | 3.3.0 (2026-08-28) [04] | MIT [04] | PRIMARY parallel walker: crossbeam work-stealing pool (no rayon), macOS `getattrlistbulk` bulk metadata, streaming iterator, drop-to-cancel, `descend` predicate per-entry `st_dev` mount skip [04] | — | scan | ignore 0.4.33 `WalkParallel` [04] |
| ignore | 0.4.33 (2026-08-04) [04] | Unlicense OR MIT [04] | gitignore/glob exclusion (`gitignore::GitignoreBuilder` usable standalone) + fallback parallel walker; only maintained walker with built-in gitignore + `same_file_system` [04] | — | scan, classify, policy | globset raw [04] |
| globset | 0.4.20 (2026-08-04) [04] | Unlicense OR MIT [04] | fast glob-set matching for user ignore/policy globs; shares engine with `ignore` [04] | — | classify, policy | — |
| walkdir | 2.5.0 [04] | Unlicense/MIT [04] | serial ordered traversal base; fallback for sorted/serial scans; stable (no release since 2024-03) [04][05] | — | scan | fs + recursion [05] |
| jwalk | 0.9.0 (2026-08-05) [04] | MIT [04] | REJECTED — README: "Unmaintained … use dua-core"; no device check in source, no gitignore, `skip_hidden` on by default [04] | — | — (none) | dua-core [04] |
| crossbeam-channel | 0.5.16 [04] | MIT OR Apache-2.0 [04] | scan→ui event channel + `select!` loop; the dua/gdu progress backbone [04][06] | — | scan, ui | std mpsc (no select) [06]; tokio mpsc (drags tokio in for no benefit) [06] |
| num_cpus | 1.17.0 [04] | MIT OR Apache-2.0 [04] | worker count for dua-core `threads`; dua itself depends on it; `std::thread::available_parallelism` covers most needs [04] | — | scan | std [04] |
| sysinfo | 0.39.6 [04] | MIT [04] | volume list, capacity, removable/read-only flags (`Disks::new_with_refreshed_list`, `DiskRefreshKind` to trim cost) [04] | — | scan, ui | libc `statfs` manual [04] |
| rayon | 1.12.0 [04] | MIT OR Apache-2.0 [04] | parallel map for CPU-bound classify only; `par_bridge()` is a documented bottleneck for IO iterators [04] | — | classify | std threads + channel [04] |
| dashmap | 6.2.1 [04] | MIT [04] | ONLY if classifier threads share maps; avoid by keeping scan→classify channel-based (MSRV 1.70) [04] | — | classify | `RwLock<HashMap>` [04] |

### 1b. macOS syscall / execute / ledger stack

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| libc | 0.2.189 [04][05] (0.2.187 stable line [08]) | MIT OR Apache-2.0 [05][08] | raw FFI: `renamex_np`/`renameatx_np`, `clonefile`, `getattrlist(bulk)`, xattr family, `F_FULLFSYNC=51`, `F_BARRIERFSYNC=85`, `RENAME_EXCL=4`; MISSING consts to define locally: `SF_DATALESS`, `UF_DATAVAULT`, `SF_FIRMLINK`, `RENAME_NOFOLLOW_ANY`, `RENAME_RESOLVE_BENEATH`, `RENAME_SECLUDE`, `IOPOL_*` [05]; `setiopolicy_np` NOT exported — own `extern "C"` block [04] | — | execute, scan, ledger | hand-written `extern "C"` blocks [05] |
| nix | 0.31.3 [05][08] | MIT [05][08] | typed errno + `fstatat`+`AtFlags`, `fcntl(F_FULLFSYNC)`/`F_BARRIERFSYNC`/`F_GETPATH(_NOFIRMLINK)` (apple targets), `FileFlag` bitflags (lacks `SF_DATALESS`/`UF_DATAVAULT`/`SF_FIRMLINK`), `Flock` RAII; `RenameFlags` is Linux-only; NO `renamex_np` [05][08] | `fs` needed for fcntl enums [05] | execute, ledger | rustix [05] |
| rustix | 1.1.4 [05] | Apache-2.0 OR MIT OR Apache-2.0-WITH-LLVM-exception [05] | safe fd/path APIs + full `fs::xattr` module; `Arg` path trait avoids UB on interior NULs; no macOS rename-with-flags [05] | — | execute | xattr crate [05] |
| xattr | 1.6.1 [05] | MIT OR Apache-2.0 [05] | minimal cross-platform xattr get/set/list/remove; fine if rustix feels heavy [05] | — | execute | rustix `fs::xattr` [05] |
| tempfile | 3.27.0 (2026-03-11) [08] | MIT OR Apache-2.0 [08] | atomic temp file + rename for WAL compaction/snapshot replace; `persist_noclobber` atomic on Apple via `renameat_with(NOREPLACE)` since 3.18.0; docs: does NOT fsync for you [08] | — | ledger, execute | hand-rolled libc `renamex_np` [08] |
| crc32fast | 1.5.1 (2026-08-22) [08] | MIT OR Apache-2.0 [08] | per-record CRC32 framing of JSONL ledger/WAL records; SIMD + streaming `Hasher` [08] | — | ledger | `crc` crate (more variants, slower) [08] |
| fs4 | 1.1.0 [08] | MIT OR Apache-2.0 [08] | advisory `flock` wrapper (rustix-based); preferred over fs2; sync feature MSRV 1.75 [08] | sync MSRV 1.75 [08] | ledger | fs2 0.4.3 [08]; `nix::fcntl::Flock` [08] |
| fs2 | 0.4.3 [08] | MIT OR Apache-2.0 [08] | advisory lock reference / unmaintained-ish predecessor; keep as documented fallback [08] | — | ledger | fs4 [08] |
| fs-err | 3.3.1 (2026-07-03) [04][08] | MIT OR Apache-2.0 [04][08] | drop-in `std::fs` with op + path in error messages — for ledger/WAL/manifest IO, NOT the hot walk path [04][08] | optional `tokio`/`path_facts` features [04] | ledger, execute, config | manual `map_err` [04] |
| trash | 5.2.6 [05] | MIT [05] | NOT for quarantine moves (Finder-Trash semantics, not our WAL); optional interop to show where Apple trash lives [05] | — | execute (optional) | none (skip) [05] |

### 1c. TUI stack (feature `tui`)

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| ratatui | 0.30.2 (2026-06-19) [06] | MIT [06] | widget/diffed-render library; umbrella crate re-exporting ratatui-core 0.1.2 + ratatui-widgets 0.3.2 (workspace-split since 0.30) [06] | `crossterm` default backend [06] | ui | cursive (ncurses; heavier) [06] |
| crossterm | 0.29.0 [06] | MIT [06] | raw mode, alt screen, events, SIGWINCH→Resize; MUST pin "0.29" — ratatui 0.30 hard-depends on it (non-optional); two majors in one tree = lost events / broken raw-mode restore [06] | `event-stream` opt-in (not needed; threads+channels) [06] | ui | termion 4.0.6 (MIT, Unix-only; no poll/timeout, no Resize events) [06] |
| unicode-width | 0.2.2 [06] | MIT OR Apache-2.0 [06] | correct cell widths for CJK/emoji filenames (Chinese paths render); column alignment + truncation via `UnicodeWidthStr::width()`; dua pulls 0.2 under its tui feature [06] | — | ui | unicode-segmentation 1.13.3 (grapheme clustering; needed too if truncating mid-cluster) [06] |
| signal-hook | 0.4.4 [06] | MIT OR Apache-2.0 [06] | real SIGINT/SIGTERM/SIGTSTP routing (belt-and-braces cancel path); crossterm itself depends on 0.3.17 for SIGWINCH [06] | mio integration optional [06] | ui, execute | `libc::sigaction` directly (unsafe, easy to get wrong) [06] |
| indicatif | 0.18.6 [04][06] | MIT [04][06] | progress bars for NON-TUI CLI mode only (plan/apply/purge subcommands); draws to stderr, max 20 refreshes/s, `abandon_with_message` on cancel; NEVER inside the ratatui loop [04][06] | — | ui (cli mode), plan | hand-rolled `\r`-updates [06]; ratatui `Gauge`/`LineGauge` in TUI [06] |

### 1d. Agent / LLM stack (feature `llm`)

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| reqwest | 0.13.4 (2026-05) [07] | MIT OR Apache-2.0 [07] | HTTP client; rustls DEFAULT since 0.13 (aws-lc-rs provider, roots via `rustls-platform-verifier`); built-in `ClientBuilder::retry(policy)` since 0.12.23; no native-tls/OpenSSL anywhere [07] | default = rustls; `query`/`form` now opt-in; enable `json` explicitly (verify at build time); `rustls-no-provider` if ring provider wanted [07] | agent | hyper + hyper-rustls [07] |
| async-openai | 0.41.3 (2026-07-31) [07] | MIT [07] | typed chat-completions client: tool-calling types, `CompletionUsage` (+ cached/reasoning token details), streaming usage chunk via `stream_options.include_usage`, `OpenAIConfig::with_api_base` (DeepSeek/OpenRouter/Ollama/vLLM), built-in `SimpleRetryPolicy` (429/5xx + Retry-After); default `rustls` [07] | `default = ["rustls"]`; fine-grained: `chat-completion`, `middleware` (on tower), `byot` — compile in only chat-completions to keep binary lean [07] | agent | hand-roll reqwest+serde_json [07] |
| reqwest-eventsource | 0.6.0 [07] | MIT OR Apache-2.0 [07] | SSE with auto-reconnect — only if hand-rolling chat JSON without async-openai; last release 2024-03, stable but aging [07] | — | agent (alternative path) | eventsource-stream + manual reconnect [07] |
| eventsource-stream | 0.2.3 [07] | MIT OR Apache-2.0 [07] | pure SSE Stream adapter used by async-openai internally; if using async-openai you never touch SSE crates; pick ONE of the two, never stack both [07] | — | agent (alternative path) | reqwest-eventsource [07] |
| tiktoken-rs | 0.12.0 [07] | MIT [07] | OpenAI-model token preflight estimates ONLY (`o200k_base`, `o200k_harmony`, `cl100k_base`); never for billing — DeepSeek/Ollama use own BPE, estimates labeled approximate [07] | per-encoding features [07] | agent | approx char/4 heuristic [07] |
| backon | 1.6.0 [07] | Apache-2.0 [07] | retry wrapper for tool calls/connect errors; `ExponentialBuilder`/`ConstantBuilder`/`FibonacciBuilder`, `.when()`/`.notify()`, Retry-After support [07] | — | agent | tower::retry [07] |
| tower | 0.5.3 [07] | MIT [07] | retry/budget layer primitives (async-openai middleware is built on it); pull in only if not using backon [07] | — | agent | backon [07] |
| tokio | 1.53.1 [07] | MIT OR Apache-2.0 [07] | async runtime for the agent loop only (reqwest/async-openai need it); need only `rt-multi-thread` + `macros` + `sync`; NO tokio in scan/ui/ledger paths [07] | `rt-multi-thread`, `macros`, `sync` — skip full/default [07] | agent | smol [07] |
| serde_json | 1.0.151 [08] | MIT OR Apache-2.0 [08] | wire-format (de)serialization + JSONL ledger records; canonical `to_vec` for CRC input; tool `arguments` is a JSON string [07][08] | — | agent, ledger, config | — |
| serde | 1.0.229 [08] | MIT OR Apache-2.0 [08] | derive for ledger/plan/config types (MIT OR Apache-2.0 per crates.io) [02][08] | `derive` | agent, ledger, config | — |

### 1e. Config / CLI / errors / misc

| Crate | Ver (2026-08) | License | Purpose / verdict | Feature flags | Module | Alternatives |
|---|---|---|---|---|---|---|
| toml | — [INFERENCE: not in notes; pinned at write time] | — [INFERENCE] | config file parsing (dua reads config; dust ships `config.toml` precedent [01]) | `parse`/`display` as needed | config | serde + `toml_edit` (format-preserving) |
| anyhow | — [INFERENCE: not in notes] | — [INFERENCE] | application-level error context at CLI/plan boundaries | — | plan, config | hand-rolled error enums |
| thiserror | — [INFERENCE: not in notes] | — [INFERENCE] | typed error enums for library-ish modules (scan/classify/ledger) | — | scan, classify, ledger | manual `impl Error` |
| clap | — [INFERENCE: not in notes] | — [INFERENCE] | CLI subcommand parsing (scan/plan/apply/purge/restore + TUI) | `derive` | config | argh |
| chrono | — [INFERENCE: not in notes] | — [INFERENCE] | RFC3339 `ts` in ledger/WAL record envelopes [08] | `serde` | ledger, config | time crate |
| time | — | — | alternative to chrono for RFC3339 timestamps [08]; pick one | — | — (alt) | chrono |
| uuid | — [INFERENCE: not in notes] | — [INFERENCE] | WAL op-id / session-id / quarantine `<wal-op-id>/` subdir naming [05][08] | `v4` | ledger, execute | ulid |

> Crate rows marked [INFERENCE] are NOT version/license-verified in notes 04–08 — the notes were
> verified on 2026-08-30 for the crates listed above; for these, check crates.io at Cargo.toml
> authoring time and pin what it reports. No version/license claim here is fabricated.

## 2. Pitfalls table

| Pitfall | Detail | Source |
|---|---|---|
| jwalk is unmaintained | jwalk 0.9.0 README states "Unmaintained … use dua-core"; also no mount/device check anywhere in source, no gitignore, `skip_hidden` on by default — do NOT adopt even as fallback | [04] |
| dua-core is 10 days old | first standalone release Aug 2026, ~10k downloads; pin exact version; audit `crates/dua-lib/src/macos` unsafe code (raw `attrlist` buffers) before shipping; fallback = ignore::WalkParallel | [04] |
| indicatif never in TUI | indicatif draws to its own Terminal on stderr at 20 Hz and assumes it owns the output — inside a ratatui alternate-screen app the two fight over the terminal; none of the four studied TUI tools use it interactively; use ratatui `Gauge`/`LineGauge` + ticker-thread snapshot pattern instead; indicatif only in headless CLI subcommands | [04][06] |
| tokio for no benefit | dua AND gitui both stay on threads + crossbeam; ratatui FAQ says async is only worth it if the app already spawns many tasks; tokio::sync::mpsc "drags in tokio for no benefit here" — tokio ONLY behind the `llm` feature for reqwest/async-openai | [06][07] |
| Commons Clause license caveat | Pearcleaner 5.4.3 is Apache-2.0 + Commons Clause v1.0 — "Sell" prohibited; fine to READ for heuristics, do NOT vendor code into a monetized product | [02] |
| GPL copy-with-care | GrandPerspective GPL-2.0 (viral for linked code — reference only), git GPL-2.0-only (design reference only), xnu APSL-2.0 (reference only, never paste code) | [02][05][08] |
| ignore::WalkState::Quit is async | more entries may arrive after Quit; a switch to ignore::WalkParallel needs an explicit drain/flush phase; dua-core's drop-join avoids this bug class | [04] |
| crossterm major-version mixing | never let two crossterm major versions into one tree — separate event queues (races, lost events), raw mode tracked separately (may not restore on exit); pin 0.29 next to ratatui 0.30 and check transitive deps | [06] |
| two SSE layers | reqwest-eventsource and eventsource-stream are alternatives — choose one, do not stack both; if using async-openai you never touch SSE crates directly | [07] |
| tempfile gives atomicity, not durability | docs caveat: persist/persist_noclobber do NOT fsync file contents or the containing directory — sync(temp) + sync(parent dir) yourself (F_FULLFSYNC on Apple is std `sync_all()` since PR #60121) | [08] |
| crates.io API notes | versions/licenses above were pulled per-crate from the crates.io API on 2026-08-30 [04][07][08]; sysinfo 0.3x and indicatif 0.1x churn between minor versions — pin tightly; dua-core 3.3.0 published 2026-08-28 (check for newer before pinning); libc 1.0.0-alpha.4 exists but the stable line is 0.2.x — stay on 0.2; sled 1.0.0-alpha.124 stalled since 2024-10 | [04][07][08] |
| fs2 vs fs4 | fs4 is the maintained rustix-based fork (sync feature MSRV 1.75); fs2 0.4.3 is the older original — prefer fs4; both advisory-only locks | [08] |
| libc missing macOS symbols | `setiopolicy_np` NOT exported by libc (own `extern "C"` block, plain linking vs libSystem suffices); nix `FileFlag` lacks `SF_DATALESS`/`UF_DATAVAULT`/`SF_FIRMLINK`; nix has NO `renamex_np` (`RenameFlags` is Linux-only) — raw libc it is | [04][05] |
| rustix license triple | rustix is Apache-2.0 OR MIT OR Apache-2.0-WITH-LLVM-exception — unusual triple; fine for use, attribute correctly if vendoring | [05] |
| num_cpus vs std | `std::thread::available_parallelism` covers most needs; num_cpus only if you mirror dua's exact `threads` plumbing | [04] |
| signal-hook transitive version | we list signal-hook 0.4.4 while crossterm 0.29 pulls signal-hook 0.3.17 transitively — two minor lines in one tree. Harmless (they don't share types across our boundary; we use only signal-hook's own flag/mio surface), but check `cargo tree -d` at pin time and prefer routing through crossterm's own SIGWINCH handling where possible | [06] |

## 3. Proposed [dependencies] block

```toml
[dependencies]
# --- core (default feature: headless scan/classify/plan/execute/ledger) ---
dua-core = "=3.3.0"                      # [04] pin exact: 10-day-old standalone crate
ignore = "0.4.33"                        # [04] gitignore builder + fallback walker
globset = "0.4.20"                       # [04] policy/ignore glob matching
walkdir = "2.5.0"                        # [04] serial fallback traversal
crossbeam-channel = "0.5.16"             # [04][06] scan->ui events + select! loop
num_cpus = "1.17.0"                      # [04] worker counts (or std available_parallelism)
sysinfo = "0.39.6"                       # [04] volume list/capacity/removable flags
rayon = "1.12.0"                         # [04] CPU-bound classify parallel map ONLY
dashmap = { version = "6.2.1", optional = true }  # [04] only if classifier threads share maps
libc = "0.2.189"                         # [04][05][08] FFI: renamex_np, clonefile, F_FULLFSYNC...
nix = { version = "0.31.3", features = ["fs"] }   # [05][08] fcntl/fstatat/flock typed APIs
rustix = { version = "1.1.4", features = ["fs"] } # [05] xattr module
tempfile = "3.27.0"                      # [08] atomic persist for WAL compaction
crc32fast = "1.5.1"                      # [08] ledger/WAL per-record CRC
fs4 = "1.1.0"                            # [08] advisory flock (sync feature MSRV 1.75)
fs-err = "3.3.1"                         # [04][08] io error context on ledger/config IO
serde = { version = "1.0.229", features = ["derive"] } # [08]
serde_json = "1.0.151"                   # [08] JSONL ledger + CRC canonical bytes
toml = "0.9"                             # config parsing [INFERENCE — verify on crates.io]
thiserror = "2"                          # typed module errors [INFERENCE — verify]
anyhow = "1"                             # CLI/plan boundary errors [INFERENCE — verify]
clap = { version = "4", features = ["derive"] }   # subcommands [INFERENCE — verify]
chrono = { version = "0.4", features = ["serde"] }# RFC3339 ts [INFERENCE — verify]
uuid = { version = "1", features = ["v4"] }       # WAL op ids [INFERENCE — verify]

# --- TUI (feature "tui") ---
ratatui = { version = "0.30.2", optional = true } # [06]
crossterm = { version = "0.29", optional = true } # [06] MUST match ratatui's hard dep
unicode-width = { version = "0.2.2", optional = true } # [06] CJK/emoji cell widths
signal-hook = { version = "0.4.4", optional = true }   # [06] SIGINT/SIGTERM/SIGTSTP
indicatif = { version = "0.18.6", optional = true }    # [04][06] headless CLI progress ONLY

# --- LLM agent loop (feature "llm") ---
tokio = { version = "1.53.1", optional = true, features = ["rt-multi-thread", "macros", "sync"] } # [07]
reqwest = { version = "0.13.4", optional = true, default-features = true, features = ["json"] }   # [07] rustls default
async-openai = { version = "0.41.3", optional = true, features = ["chat-completion", "middleware"] } # [07]
reqwest-eventsource = { version = "0.6.0", optional = true }  # [07] only if hand-rolling SSE (pick ONE)
eventsource-stream = { version = "0.2.3", optional = true }   # [07] alt to reqwest-eventsource (pick ONE)
tiktoken-rs = { version = "0.12.0", optional = true } # [07] preflight estimates, never billing
backon = { version = "1.6.0", optional = true }       # [07] tool-call retry
cargo_metadata = "0.23.1"                # [DESIGN §3.2] resolve toolchain/target layout for classify evidence
regex = "1.13.1"                         # [DESIGN §3.6] policy compiler token patterns
dirs = "6.0.0"                           # [DESIGN §3.9] XDG/home dir resolution for config + caches
secrecy = "0.10"                         # [DESIGN §3.9] API-key type (no accidental Debug/Display leaks)
same-file = "1.0.6"                      # [DESIGN §2.4] st_dev/mntonname dev-identity checks
unicode-segmentation = "1.13.3"          # [06] grapheme-safe truncation for CJK/emoji filenames
```

Notes: `dashmap` stays optional until classifier threads provably need shared maps [04];
`reqwest-eventsource` vs `eventsource-stream` — enable exactly one [07]; keep SSE crates disabled
while async-openai parses streams itself [07].

## 4. Feature-gating strategy

```toml
[features]
default = []            # headless core: scan/classify/plan/execute/ledger/policy/config
tui = ["dep:ratatui", "dep:crossterm", "dep:unicode-width", "dep:signal-hook", "dep:indicatif"]
llm = ["dep:tokio", "dep:reqwest", "dep:async-openai", "dep:tiktoken-rs", "dep:backon"]
```

- **default = headless core.** scan → dua-core walk + crossbeam events; classify → rayon CPU pool;
  plan/execute → nix/libc quarantine moves; ledger → JSONL + crc32fast + fs4 flock + tempfile
  compaction; policy/config → globset + toml + clap. Zero async runtime, zero terminal control.
  CLI progress without TUI comes via `indicatif` (kept in the `tui` feature so the pure-headless
  build stays lean; move to `default` if headless builds still need progress bars).
- **`tui` feature**: ratatui 0.30.2 + crossterm 0.29 pinned together [06]; unicode-width for CJK
  filenames [06]; signal-hook for external SIGINT/SIGTERM routing [06]; NO tokio in this path [06].
- **`llm` feature**: the only place tokio lives [06][07]; reqwest (rustls default) + async-openai
  with only `chat-completion` + `middleware` features to keep the binary lean [07]; tiktoken-rs for
  preflight token estimates (never billing) [07]; backon for tool-call retry [07]. SSE crates
  (reqwest-eventsource OR eventsource-stream) stay OUT of the feature until hand-rolling is chosen
  — async-openai already parses streams [07].
- Both features compose: `tui` + `llm` = the interactive agent UI; the privacy chokepoint (tool
  serializer) lives in `agent/` regardless of features [07].
- CI should build `--no-default-features`, default, `--features tui`, `--features llm`, and both,
  so crossterm/ratatui version drift is caught at the edges [06].

## 5. Version risk register (churn before 2026-09-08)

| Dependency | Risk | Mitigation |
|---|---|---|
| dua-core 3.3.0 | 10-day-old standalone crate (2026-08-28, ~10k downloads) — most likely to churn before 2026-09-08 | Pin exact `=3.3.0` [04]; watch crates.io for 3.3.1+; audit macOS unsafe code; fallback = ignore::WalkParallel + own channel + `std::thread::available_parallelism()` [04] |
| ratatui 0.30.2 / crossterm 0.29 | crossterm 0.30 does not exist yet as a known target; when a new major lands, ratatui moves (non-optional dep) and every direct crossterm use must move in the same commit (separate event queues per major = race/lost-event risk) | Pin `crossterm = "0.29"` next to `ratatui = "0.30"`; check transitive deps in CI [06] |
| reqwest 0.13.x | 0.13.0 broke with rustls-default (crypto provider aws-lc-rs); `query`/`form` became opt-in; `json` default-on "verify at build time" | Pin `0.13.4`; enable `json` explicitly; test base-URL overrides (DeepSeek/OpenRouter/Ollama/vLLM) at build time [07] |
| async-openai 0.41.3 | fine-grained feature churn (`chat-completion`, `middleware`, `byot`); 0.41.3 released 2026-07-31 | Pin exact; compile only chat-completions [07] |
| sysinfo 0.39.6 | 0.3x "churns between minor versions" | Pin exact; `DiskRefreshKind` to trim cost [04] |
| indicatif 0.18.6 | 0.1x minor-version churn | Pin exact [04] |
| libc 0.2.x | 1.0.0-alpha.4 pre-releases exist; stable line is 0.2.x | Stay on `0.2.x` (0.2.189); do NOT adopt 1.0-alpha [04][05][08] |
| tokio 1.53.1 | minor-version drift normal | `1.53.1` with minimal features [07] |
| fs4 1.1.0 | fork-of-fs2 lineage; sync feature MSRV 1.75 | Pin 1.1.0; verify MSRV fits toolchain [08] |
| nix 0.31.3 | 0.x line; API surface (fcntl enums, Flock) stable-ish but 0.x | Pin 0.31.3 [05][08] |
| tempfile 3.27.0 | persist semantics changed at 3.18.0 (renameat_with on Apple) | Pin ≥3.27.0; re-verify docs caveat (no fsync) on upgrade [08] |
| General pinning advice | Any crate could publish tomorrow | For young/churning crates (dua-core, sysinfo, indicatif, reqwest, async-openai) pin EXACT versions in Cargo.toml and let only deliberate upgrades move them; for ultra-stable lines (serde 1.x, libc 0.2.x, crossbeam-channel 0.5.x, globset 0.4.x, walkdir 2.x) caret ranges are acceptable [04][07][08]. Commit Cargo.lock; re-verify pinned versions against crates.io API the day Cargo.toml lands (all claims here verified 2026-08-30 [04][05][06][07][08]) |
