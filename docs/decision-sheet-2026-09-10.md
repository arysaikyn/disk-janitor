# Decision sheet — implementation session 2026-09-09/10

> Per session contract: zero owner checkpoints; genuinely ambiguous calls surfaced here
> instead of stopping. Each item below was decided unilaterally during M1–M5 to keep the
> tree green. SPEC/design-decisions won wherever they spoke; these are the gaps.

| # | Call made | Where | Why ambiguous | Suggested owner disposition |
|---|---|---|---|---|
| D-32? | **`Refusal::GradeNotAllowed` doubles as the policy-deny bucket** — deny-globs, target_bytes cap, max_entries all emit it (no `PolicyDeny`/`TargetReached` variant exists in the frozen §6 enum) | `src/plan.rs` decide() | Frozen `Refusal` enum has no policy-deny variant; lib.rs was off-limits to parallel units | Add `Refusal::PolicyDeny` (+ maybe `TargetReached`) in a §6 amendment so manifest consumers can distinguish grade-refusals from policy refusals |
| D-33? | **MTA semantics: 「别动我这两周在跑的项目」 protects project *activity* (ancestor `.git/HEAD` mtime), exempts machine-derived build outputs** (LockfilePresent/CasStore/BuildArtifactShape evidence) | `src/plan.rs` MTA block (77f0b97) | Literal reading (candidate mtime < 14 d) refuses *everything* — incl. the node_modules/_cacache/DerivedData the SPEC §4 data flow quarantines under this exact sentence | Confirm the data-flow reading; if literal protection is wanted, spec how 「腾出 25 GB」 ever frees anything |
| D-34? | **Quarantine root anchors on the topmost writable same-dev ancestor** when `<mnt>/.Trash-janitor` mkdir fails (SIP locks `/System/Volumes/Data` root) | `src/execute.rs quarantine_root_for_source` | SPEC §5.1 says `<mnt>/.Trash-janitor` per volume; on the boot volume that root is not mkdir-able as user. Fallback keeps same-volume atomicity + WAL co-location | Bless the fallback (e.g. `~/.Trash-janitor` or `/private/tmp/...` on boot volume) or spec a privileged installer that pre-creates the root |
| D-35? | **`VolumeInfo.read_only` = sysinfo `is_read_only()` AND writability probe** — sealed-system `/` reports RO while firmlinked writes land on Data (same st_dev) | `src/scan/platform/macos.rs volumes()` | statfs truth ≠ user-visible writability; plan/execute need the writable picture | Confirm; the probe file (`.janitor-ro-probe-<pid>`) is transient + removed |
| D-36? | **HTTP `ChatClient` impl deferred** — `AgentRun` is sans-IO with injected replies; async-openai/reqwest/tokio are in Part D §4 but were never added to Cargo.toml | `src/agent.rs` ChatClient trait | M4 exit 「腾出 25 GB under budget」 verified with scripted StubClient; live DeepSeek calls need the HTTP impl + a key (D-22 fallback = trajectory replay, so the demo never blocks) | Decide whether live-LLM is a 9.13 deliverable or the D-22 replay carries the demo |
| D-37? | **`dua-core` pinned `=3.3.0` though 3.3.1/4.0.0 exist** — SPEC Part D §5 pins exact; 4.0.0 released after the spec was written | `Cargo.toml` | ADR-1 says pin exact + watch for churn | Say the word and bump (4.0.0 API unknown, would need a probe) |
| D-38? | **GUI demo runs on `ipc.ts` synthetic fallback** (demo candidates/plan/ledger in the browser) — the real Tauri app uses identical serde shapes | `app/src/lib/ipc.ts` | D-21 automated capture + D-22 keyless fallback both assume a keyless demo surface | Fine for 9.10; for 9.13 consider a `--demo-data` flag on the Tauri build for reproducible capture |

## Non-decisions (verified correct, no owner input needed)
- WAL on the quarantine volume, crc32 recipe, skip-and-continue replay — SPEC §3.5/§5.2 as written.
- `renamex_np` canonical-parent resolution (ELOOP on `/var` symlinks) — §3.4 syscall table + empirical fix.
- Purge typed-byte gate at PURGE only, Esc-inert modal — D-27/§4.2 as written, AX-verified in the GUI.
- Keep-both restore collision default — D-26 as written (17647bb). All other milestones
  followed the SPEC verbatim.

*Every claim above traces to a commit: 9f4cb7c · 9fd33f1 · 17647bb · 97d704f · 8041142 · 3ed9545 · a671575 · 77f0b97 · 82c1649.*

## Dispositions (owner answers, 2026-09-10 — all implemented same day)

| # | Answer | Landed in |
|---|---|---|
| D-32 | **C** — PolicyDeny + TargetReached added to §6 Refusal enum; deny-globs emit PolicyDeny, target/max_entries caps emit TargetReached; plan tests updated | `src/lib.rs`, `src/plan.rs` |
| D-33 | **A** — activity semantics confirmed (.git/HEAD mtime judges; machine-derived outputs exempt) | `src/plan.rs` (no change) |
| D-34 | **A** — same-volume writable-ancestor fallback confirmed | `src/execute.rs` (no change) |
| D-35 | **A** — writability probe confirmed | `src/scan/platform/macos.rs` (no change) |
| D-36 | **B** — HTTP ChatClient landed behind `llm` feature: async-openai 0.41.3 non-streaming, async-openai's built-in retry layer (429/5xx + Retry-After), Ollama id-less → synthetic ids, `janitor agent` subcommand wired; 8 stub-server tests | `src/agent/http.rs`, `src/main.rs` |
| D-37 | **C** — dua-core bumped `=3.3.0` → `=4.0.0` (Options.skip_metadata + Entry.metadata: Option<_> adapted; SPEC §5 risk row amended); 91 tests green | `Cargo.toml`, `src/scan/platform/macos.rs` |
| D-38 | **C** — `--demo-data` Tauri switch: all 10 commands serve ipc.ts-mirrored synthetic payloads; ipc.ts probes `is_demo` once and routes to the same deterministic fallback | `src-tauri/src/{main,commands}.rs`, `app/src/lib/ipc.ts` |
