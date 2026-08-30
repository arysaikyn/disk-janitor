# TUI Stack & Event-Loop Architecture (ratatui) — Research Note

Interactive file-review UI for Disk Janitor — stack choice (ratatui / crossterm / termion /
unicode-width), large-selectable-list & progress-bar patterns, resize handling, event loop with
background workers + channels + Ctrl-C-as-cancel; distilled from dua-cli, gdu, ncdu, gitui
· researched 2026-08-30

## Findings

### 1. Stack verdict: ratatui 0.30.2 + crossterm 0.29

ratatui 0.30.2 (crates.io; published 2026-06-19) is MIT-licensed [1]. It is a widget/render
library, not a framework: it draws to a diffed double buffer each `terminal.draw(...)` call and
deliberately does not own input, threads, or async — the app supplies the event loop [2].
Terminal control (raw mode, alternate screen, event capture) comes from a *backend* crate;
crossterm is the default and ratatui 0.30.2 hard-depends on crossterm ^0.29 (non-optional dep,
crates.io metadata) [1][2]. Both reference Rust apps in this lane run exactly this pair in
production: dua-cli 2.43.1 (ratatui 0.30.2, `default-features = false, features = ["crossterm"]`)
[8] and gitui 0.28.1 (crossterm 0.29, ratatui 0.30) [14].

Since 0.30 ratatui is workspace-split (ratatui-core 0.1.2, ratatui-widgets 0.3.2) [3][4]; the
umbrella `ratatui` crate re-exports everything — use the umbrella crate. Hard rule from the
official docs: never let two crossterm major versions into one dependency tree — separate major
versions "keep separate event queues (race conditions, lost events)" and "track raw mode
separately (raw mode may not be restored on exit)" [2]. Pin `crossterm = "0.29"` next to
`ratatui = "0.30"` and check transitive deps.

### 2. crossterm vs termion (events, raw mode, Ctrl-C)

| | crossterm 0.29.0 [5][6][7] | termion 4.0.6 [15] |
|---|---|---|
| Platforms | cross-platform (unix + windows) | Unix-only (Redox origin) |
| Raw mode | `enable_raw_mode()/disable_raw_mode()`; prior termios stored behind a mutex (`TERMINAL_MODE_PRIOR_RAW_MODE`) so calls are idempotent [7] | `stdout().into_raw_mode()` → `RawTerminal`, restores termios on `Drop` [15] |
| Events | blocking `read()`, timed `poll(Duration)`, `try_read()`; async `EventStream` behind opt-in `event-stream` feature [5] | pull `Iterator`s (`keys()`, `events()`) over the byte stream; no poll-with-timeout, no async variant [15] |
| Resize | SIGWINCH registered via signal-hook on its mio loop → `Event::Resize(w, h)` in the normal event stream [6] | none built-in; poll `terminal_size()` yourself |
| Raw-mode Ctrl-C | arrives as `KeyEvent` (raw mode cleared ISIG — see below) | same mechanism, arrives as key |
| Role in ratatui | default backend (`crossterm` feature) [2] | secondary backend (`termion` feature) [2] |

Key crossterm source facts (v0.29 master):
- Event source rule, verbatim from `event.rs`: "It's **not allowed** to call these functions from
  different threads or combine them with the `EventStream`. You're allowed to either use the
  `read` & `poll` functions on any, but same, thread or the `EventStream`" [5].
- Raw mode applies `cfmakeraw` + `tcsetattr(TCSANOW)` on the TTY fd [7]. macOS semantics
  (termios(4), verified): "INTR — Special character on input and is recognized if the ISIG flag is
  enabled. Generates a SIGINT signal … If ISIG is set, the INTR character is discarded when
  processed" [16]. `cfmakeraw` clears ISIG ⇒ in raw mode Ctrl-C is *not* a signal; it is read as
  a key event. dua-cli exploits this: its bindings are `quit = "q"`,
  `quit_immediately = "ctrl+c"`, `suspend = "ctrl+z"` — plain key handling, no SIGINT handler in
  the TUI [10].
- The only signal crossterm itself converts is SIGWINCH (registered via
  `signal_hook::consts::SIGWINCH`) [6].

termion is fine as a minimal Unix-only backend but crossterm wins for Disk Janitor: built-in
Resize events, poll-timeout, the async `EventStream` escape hatch, and ecosystem alignment (dua,
gitui, ratatui default) [2][5][6][15].

### 3. Long selectable lists/tables (virtual scrolling)

- The `StatefulWidget` pattern is the core primitive: `List`+`ListState`, `Table`+`TableState`,
  `Scrollbar`+`ScrollbarState`. Widget state holds the selected index plus a scroll *offset*
  remembered between draws, giving natural scrolling; the app mutates state (e.g.
  `state.select(Some(i))`) and renders via `Frame::render_stateful_widget` [3].
- Built-in virtualization: ratatui's List render loop iterates
  `.skip(state.offset).take(last_visible_index - first_visible_index)` — only the visible window
  is painted; `get_items_bounds()` recomputes the offset from the selection each frame [3]. A
  100k-candidate scan result is safe *provided* you rebuild `ListItem`s from your data slice per
  draw (cheap formatting, O(visible)) instead of pre-materializing 100k styled strings.
- dua-cli keeps its own data (petgraph tree) and windowed entries; after each traversal event it
  re-finds the previously selected entry **by name** so the cursor survives re-sorting/insertion
  (`update_state_during_traversal` in eventloop.rs) [9]. Adopt this: re-select candidates by
  stable key (path), never by index, when scan events reorder the list mid-scan.
- gitui renders large commit logs as virtualized lists driven by async notifications, with
  `any_work_pending()` gating redraw + spinner [13][14].

### 4. Live progress bars: indicatif vs ratatui-drawn

- indicatif 0.18.6 (MIT) [17] owns its output: it draws progress to a raw `Terminal` on
  stdout/stderr, with `MultiProgress` for concurrent bars. Inside a ratatui app (alternate screen,
  diffed buffer, own draw loop) the two fight over the terminal. None of the four studied tools
  use indicatif in their interactive mode.
- ratatui-drawn is the right call: `Gauge` / `LineGauge` are plain stateless `Widget`s — they fill
  the area proportionally to `percent()`/`ratio()` passed each frame, `use_unicode` gives
  eighth-cell precision [18]. Progress lives in app state; workers push updates over a channel;
  the loop redraws on each update or tick. gdu does exactly this with a hand-rolled
  `ProgressBar` tview primitive (mutex-guarded 0–100 int, `Draw()` renders under `RUnlock`)
  [11]; ncdu draws its scan-progress screen from the main loop, rate-limited by
  `config.update_delay` (default 100 ms; `-q/--slow-ui-updates` raises it to 2 s) [12].
- Keep indicatif only for the headless CLI subcommands (non-TUI mode), which Disk Janitor plans
  anyway (SPEC R2: "CLI subcommands as fallback").

### 5. Event-loop architecture (workers + channels + cancellation)

Patterns extracted from the four reference apps:

| App | Loop shape | Workers → UI channel | Ctrl-C / abort |
|---|---|---|---|
| dua-cli 2.43.1 [9][10] | one UI thread; `crossbeam::select!` over (a) bounded(32) input channel fed by a dedicated crossterm `read()` thread, (b) `BackgroundTraversal` event channel; blocking `recv()` when idle | traversal events integrated into the tree per event (`integrate_traversal_event`); selection re-found **by name** after each event | raw-mode key: `quit_immediately = "ctrl+c"`, `quit = "q"`; suspend via signal-hook SIGTSTP raise [10] |
| gitui 0.28.1 [13][14] | `crossbeam_channel::select!` (`select_event`) over 6 receivers: input, git-async, app-async, ticker (5 s), watcher, spinner (80 ms); redraw after every event | asyncgit spawns rayon jobs; on completion sends `AsyncGitNotification` variant; `any_work_pending()` gates spinner/state | input thread with fast(100 ms)/slow(10 s) poll; `aborted` AtomicBool if input thread dies; `is_quit()` checks `input.is_aborted()` |
| gdu v5.37.0 [11] | tview `app.Run()` owns the loop; OS signals (SIGINT et al.) converted to synthetic `tcell` key events and posted into the UI event queue (retry-ticker until accepted) | delete work via `deleteQueue` chan + worker goroutines; UI updated only via `app.QueueUpdateDraw` (thread-safe) | `handleSignalEvent`: if scanning → `cancelScan()` (sets flag, title "Stopping scan...", keeps partial results); else app.Stop() |
| ncdu 2.9.2 [12] | single main loop `while(true){ state-switch; handleEvent(block, force_draw) }`; redraw rate-limited by `update_delay` | parallel scan threads (config.threads, default CPU count) feed a shared sink; UI reads it on draw | 'q' during scan → confirm → abort; scan state machine (running/done/err) drives the progress screen |

Synthesis for Disk Janitor's `ui/` module (recommended shape — event-driven, no tokio; ratatui's
own FAQ says async is only worth it if the app already spawns many tasks, and both dua and gitui
stay on threads + crossbeam) [2][9][13]:

1. **UI thread** owns `Terminal<CrosstermBackend>` and ALL drawing. Main loop:
   `terminal.draw(|f| ui.render(f, &state))?` then `crossbeam::select!` over input_rx /
   worker_rx / tick_rx. Redraw **on every event** (draws are diffs, cheap) + 100 ms tick while a
   long task runs (ncdu/gdu cadence) [11][12][13].
2. **Input thread**: dedicated thread calling `crossterm::event::read()` in a loop, forwarding
   into a `crossbeam::channel::bounded(32)` channel (dua's exact pattern; bounded so input can't
   grow unbounded while the UI does synchronous work) [9].
3. **Worker threads** (scan/classify/purge): long tasks run on their own threads; they stream
   progress events (`Progress { done, total, bytes }`, `CandidateFound`, `Finished`) over an
   mpsc channel and poll an `Arc<AtomicBool>` cancel flag at safe points (per directory / per
   candidate). Never touch ratatui from a worker — send events, let the UI thread draw (gdu's
   QueueUpdateDraw discipline) [11].
4. **Ctrl-C = cancel current task, not quit**: with raw mode on, Ctrl-C arrives as
   `KeyCode::Char('c')` + `KeyModifiers::CONTROL` (macOS: `cfmakeraw` clears ISIG, so the INTR
   character no longer generates SIGINT — termios(4) [16]). Handler: if a long task is active →
   set cancel flag, show "Stopping…" state (gdu) [11]; double-Ctrl-C or Ctrl-C while idle → quit.
   As a belt-and-braces measure also catch real SIGINT/SIGTERM via signal-hook (e.g. sent by
   another process) and route them into the same cancel path (gdu posts them as synthetic keys)
   [11]. On any exit path, flush the WAL/ledger first so state stays crash-consistent (SPEC R4).
5. **Resize**: crossterm turns SIGWINCH into `Event::Resize(w, h)` in the normal stream [6];
   on receipt just fall through to `terminal.draw` — ratatui's Terminal auto-resizes its buffer
   per draw. ncdu's `KEY_RESIZE` → `updateSize()` → redraw is the same idea [12].
6. **State**: single `AppState` owned by the UI thread; workers only send messages (no shared
   mutable state beyond the cancel flag). This is dua's model (AppState owns navigation, scan,
   stats) and avoids gitui's `NotifiableMutex` complexity [9][14].

## Reusable code & licenses

Licenses verified against each repo on 2026-08-30 (not from memory).

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| ratatui/ratatui | MIT (LICENSE; crates.io) [1][2] | StatefulWidget pattern; Gauge/LineGauge; list offset windowing; `ratatui::init()/restore()` terminal setup; TestBackend for UI unit tests | ratatui-widgets/src/{list,gauge,table}; README quickstart |
| Byron/dua-cli | MIT (Cargo.toml `license = "MIT"`) [8] | Event-loop skeleton: input thread → bounded channel + `crossbeam::select!` over worker events; name-keyed selection restore during live scan; ctrl+c = quit_immediately keymap; terminal suspend/resume | src/interactive/app/{eventloop.rs,input.rs,terminal.rs}, src/config.rs |
| dundee/gdu | MIT (LICENSE.md; GitHub API `license.spdx_id = MIT`) [11] | SIGINT → synthetic key into UI queue (with retry); cancelScan() keeps partial results + "Stopping..." UX; background delete queue with workers; mutex-guarded progress primitive | tui/tui.go, tui/background.go, tui/progressbar.go |
| yorhel/ncdu (zig branch) | MIT (LICENSES/MIT.txt; SPDX headers) [12] | Rate-limited redraw (`update_delay` 100 ms default); scan-progress screen with abort-and-keep-results; per-state key dispatch | src/main.zig, src/sink.zig, src/ui.zig |
| gitui-org/gitui | MIT (LICENSE.md "MIT License, Copyright (c) 2025 gitui-org") [14] | Multi-source select loop with tick/spinner intervals; `any_work_pending()` gating; rayon job → notification-channel pattern | src/{gitui.rs,main.rs,input.rs}, asyncgit/src/status.rs |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| ratatui 0.30.2 | widgets + diffed rendering | MIT; umbrella crate re-exporting core/widgets; crossterm backend by default [1][2] | cursive (ncurses-based; heavier) |
| crossterm 0.29 | raw mode, alt screen, events, resize | MIT; MUST pin `"0.29"` to match ratatui's non-optional dep; two majors in one tree = lost events / broken raw-mode restore [1][2][6] | termion 4.0.6 (MIT, Unix-only; no poll/timeout, no Resize events) [15] |
| unicode-width 0.2.2 | correct cell widths for CJK/emoji filenames | MIT OR Apache-2.0 [20]; 还我磁盘 will render Chinese paths — column alignment and truncation need `UnicodeWidthStr::width()`; already what dua pulls under its tui feature (0.2) [8] | unicode-segmentation 1.13.3 (grapheme clustering; MIT OR Apache-2.0 [20]) — needed too if truncating mid-cluster |
| crossbeam-channel 0.5.16 | bounded mpsc + `select!` over multiple receivers | MIT OR Apache-2.0 [19]; the proven pattern in dua AND gitui; std mpsc has no select | tokio::sync::mpsc (drags in tokio for no benefit here) |
| signal-hook 0.4.4 | real SIGINT/SIGTERM/SIGTSTP routing | MIT OR Apache-2.0 [19]; crossterm already depends on it (0.3.17) for SIGWINCH [6]; use its mio integration or a simple flag thread | libc::sigaction directly (unsafe, easy to get wrong) |
| indicatif 0.18.6 | progress bars for non-TUI CLI mode only | MIT [17]; never inside the ratatui loop [17] | hand-rolled `\r`-updates |

## Risks & open questions

- crossterm 0.30 does not exist yet as a known target; when a new major lands, ratatui will move
  to it (non-optional dep) and every direct crossterm use in Disk Janitor must move in the same
  commit (separate event queues per major version — real race/lost-event risk) [1][2].
- Raw-mode key handling means Ctrl-C is only cancel-as-key while raw mode is *on*. If a panic
  (or external SIGTERM via `kill`) fires, the terminal can be left in raw mode; mitigation:
  panic hook that calls `ratatui::restore()` (official pattern in the ratatui quickstart; check
  examples/panic.rs when writing the code) + signal-hook fallback path [1][2][11].
- `KeyEventKind::Press` filtering: on macOS only Press events are generated (official FAQ), but
  keep the `kind == Press` guard so the code stays portable and avoids future double-fires [2].
- Bounded(32) input channel: with a busy UI thread, held keys (arrow repeats) can fill it; dua
  accepts `TrySendError::Full` drops for non-focus events — acceptable here, but decide
  deliberately whether key-repeat spam should coalesce [9].
- Terminal-notifier UX: dua emits desktop notifications on scan-finish only when the terminal is
  unfocused (TerminalFocus AtomicBool) — nice-to-have for long scans, not for M3 [9][10].
- Open: does the demo terminal (Tencent classroom env / iTerm2) support kitty keyboard protocol
  (CSI-u)? crossterm supports it (`KeyboardEnhancementFlags`) but it is optional; don't rely on
  repeat/release events in M3 [5].

## Sources

1. crates.io API — ratatui: https://crates.io/api/v1/crates/ratatui (max_version 0.30.2, version
   0.30.2 license "MIT", created 2026-06-19); also https://crates.io/crates/ratatui — accessed 2026-08-30
2. Ratatui official site & docs — README (quickstart `ratatui::init()`/`restore()`, license MIT,
   forked from tui-rs 2023): https://github.com/ratatui/ratatui/blob/main/README.md ; Backends
   (crossterm default, version-compatibility rules, ratatui 0.30.2 ↔ crossterm 0.29):
   https://ratatui.rs/concepts/backends/ ; FAQ (async guidance, KeyEventKind::Press on macOS):
   https://ratatui.rs/faq/ — accessed 2026-08-30
3. ratatui StatefulWidget docs: https://docs.rs/ratatui/latest/ratatui/widgets/trait.StatefulWidget.html
   ; List rendering source (offset windowing, `skip(offset).take(...)`):
   https://github.com/ratatui/ratatui/blob/main/ratatui-widgets/src/list/rendering.rs — accessed 2026-08-30
4. crates.io API — ratatui-core 0.1.2, ratatui-widgets 0.3.2:
   https://crates.io/api/v1/crates/ratatui-core , https://crates.io/api/v1/crates/ratatui-widgets — accessed 2026-08-30
5. crossterm event.rs (master, v0.29.0): https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs
   (read/poll/try_read docs, thread-safety rule, EventStream feature, KeyboardEnhancementFlags) — accessed 2026-08-30
6. crossterm Unix event source (SIGWINCH via signal-hook → Event::Resize):
   https://github.com/crossterm-rs/crossterm/blob/master/src/event/source/unix/mio.rs — accessed 2026-08-30
7. crossterm Cargo.toml (version 0.29.0, license MIT, features incl. event-stream):
   https://github.com/crossterm-rs/crossterm/blob/master/Cargo.toml ; raw mode
   (cfmakeraw/tcsetattr TCSANOW, idempotent guard):
   https://github.com/crossterm-rs/crossterm/blob/master/src/terminal/sys/unix.rs — accessed 2026-08-30
8. dua-cli Cargo.toml (v2.43.1, license MIT workspace; ratatui 0.30.2 + crossterm 0.29 +
   unicode-width 0.2 under tui-crossplatform):
   https://github.com/Byron/dua-cli/blob/master/Cargo.toml — accessed 2026-08-30
9. dua-cli event loop & input: https://github.com/Byron/dua-cli/blob/master/src/interactive/app/eventloop.rs
   (`crossbeam::select!` over events + `active_traversal.event_rx`, name-keyed selection restore);
   https://github.com/Byron/dua-cli/blob/master/src/interactive/app/input.rs
   (`input_channel`: thread + `bounded(32)`, TerminalFocus) — accessed 2026-08-30
10. dua-cli keybindings (`quit="q"`, `quit_immediately="ctrl+c"`, `suspend="ctrl+z"`):
    https://github.com/Byron/dua-cli/blob/master/src/config.rs — accessed 2026-08-30
11. gdu (dundee/gdu, v5.37.0 latest release; MIT per LICENSE.md + GitHub API): signal→key posting
    and cancelScan: https://github.com/dundee/gdu/blob/master/tui/tui.go ; delete workers:
    https://github.com/dundee/gdu/blob/master/tui/background.go ; progress primitive:
    https://github.com/dundee/gdu/blob/master/tui/progressbar.go — accessed 2026-08-30
12. ncdu 2.9.2 (code.blicky.net, yorhel/ncdu, zig branch; MIT per LICENSES/MIT.txt + SPDX
    headers; ncurses): main loop / handleEvent / update_delay 100 ms:
    https://code.blicky.net/yorhel/ncdu/raw/branch/zig/src/main.zig ; scan progress + abort:
    https://code.blicky.net/yorhel/ncdu/raw/branch/zig/src/sink.zig ; getch/KEY_RESIZE:
    https://code.blicky.net/yorhel/ncdu/raw/branch/zig/src/ui.zig — accessed 2026-08-30
13. gitui 0.28.1 Cargo.toml (license MIT; ratatui 0.30, crossterm 0.29):
    https://github.com/gitui-org/gitui/blob/master/Cargo.toml ; select_event over 6 channels +
    TICK_INTERVAL 5 s / SPINNER_INTERVAL 80 ms: https://github.com/gitui-org/gitui/blob/master/src/main.rs — accessed 2026-08-30
14. gitui main loop & input thread: https://github.com/gitui-org/gitui/blob/master/src/gitui.rs
    (run_main_loop, any_work_pending, draw per event);
    https://github.com/gitui-org/gitui/blob/master/src/input.rs (poll 100 ms/10 s, aborted flag);
    https://github.com/gitui-org/gitui/blob/master/asyncgit/src/status.rs (rayon spawn →
    notification sender); LICENSE: https://github.com/gitui-org/gitui/blob/master/LICENSE.md — accessed 2026-08-30
15. termion 4.0.6: Cargo.toml (license MIT) https://github.com/redox-os/termion/blob/master/Cargo.toml ;
    raw mode / RawTerminal drop-restore: https://github.com/redox-os/termion/blob/master/src/raw.rs ;
    iterator-based events: https://github.com/redox-os/termion/blob/master/src/event.rs — accessed 2026-08-30
16. macOS termios(4) man page (INTR/ISIG semantics; `cfmakeraw(3)`): `man 4 termios`, `man 3
    cfmakeraw` on Darwin 25.3.0 (Apple) — accessed 2026-08-30
17. crates.io API — indicatif: https://crates.io/api/v1/crates/indicatif (0.18.6, MIT) — accessed 2026-08-30
18. ratatui Gauge/LineGauge source (percent/ratio/use_unicode, stateless Widget):
    https://github.com/ratatui/ratatui/blob/main/ratatui-widgets/src/gauge.rs — accessed 2026-08-30
19. crates.io API — crossbeam-channel (0.5.16, MIT OR Apache-2.0):
    https://crates.io/api/v1/crates/crossbeam-channel ; signal-hook (0.4.4, MIT OR Apache-2.0):
    https://crates.io/api/v1/crates/signal-hook — accessed 2026-08-30
20. crates.io API — unicode-width (0.2.2, MIT OR Apache-2.0):
    https://crates.io/api/v1/crates/unicode-width ; unicode-segmentation (1.13.3, MIT OR
    Apache-2.0): https://crates.io/api/v1/crates/unicode-segmentation — accessed 2026-08-30
