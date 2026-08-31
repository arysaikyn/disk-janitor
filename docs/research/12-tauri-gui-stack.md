# Tauri 2.x GUI Stack (Rust core + Web UI) — Research Note

Scope: Tauri 2.x IPC/security/packaging/theming/i18n stack for the Disk Janitor 「还我磁盘」 GUI-first desktop app · researched 2026-08-31

## Findings

### Core stack & versions (verified at crates.io / npm registry 2026-08-31)

Tauri 2 is the Rust-centric desktop framework: a Rust process hosts a system webview
(macOS: WKWebView, Windows: WebView2/Edge-Chromium, Linux: WebKitGTK) and exposes a
permissioned IPC bridge to a web UI. All `tauri`-family crates are `Apache-2.0 OR MIT`
(license field read from crates.io API with a custom User-Agent, not from memory).

- Rust host: `tauri` 2.11.5 (stable, newest), `tauri-build` 2.6.3, CLI `@tauri-apps/cli` 2.11.4.
- Webview/window layers: `wry` 0.56.1 (`Apache-2.0 OR MIT`), `tao` 0.37.0 (`Apache-2.0`).
- Toolchain 1.97.1 (project standard) satisfies the plugins' floor of ≥1.77.2 [INFERENCE from plugin docs: "requires a Rust version of at least 1.77.2"].

### IPC model: commands + events + channels

Three primitives (official docs, /develop/calling-rust & /develop/calling-frontend):

| Primitive | Direction | Typing | Use for Disk Janitor |
|---|---|---|---|
| `#[tauri::command]` | UI → Rust | typed via serde; async supported; args camelCase; `Result<T,E>` rejects the JS promise | every state change: `start_scan`, `approve_plan`, `apply`, `purge`, `restore` |
| Event system | Rust → UI | payload is a JSON string, not type-safe, no capability control; async listeners may run out of order under rapid emission | low-frequency notifications: "purge finished", config changed |
| `tauri::ipc::Channel<T>` | Rust → UI | typed (`Channel<DownloadEvent>`-style enum), ordered, fast — "used internally for streaming operations such as download progress, child process output" | scan/classify progress streams |

Key verified details:

- Large binary payloads should not be JSON-serialized: commands can return
  `tauri::ipc::Response` to skip JSON for raw bytes (e.g. previewing a file's first bytes).
- The docs explicitly warn: "if a listener is async and the event emitter sends multiple
  events in rapid succession, the listeners may process events out of order. For ordered,
  high-throughput data delivery, consider using Channels instead of the event system."
- Frontend invokes via `invoke('cmd', {args})` from `@tauri-apps/api/core`; frontend receives
  channels via `new Channel<T>()` + `onmessage` (verified API shape in calling-frontend docs).
- Progress streaming decision: **Channel, not polling.** The scan thread holds a `Channel`
  obtained as a command parameter and pushes `ScanProgress { dirs_walked, bytes_seen,
  current_root, items_classified }` batches (throttled, e.g. ≥100 ms per emit). Channels are
  ordered and cheap; polling would add latency and wasted IPC. Global events remain useful for
  cross-window notices ("scan finished on volume X" to all windows). A task queue in managed
  state (`tauri::State`) gives the Rust side a handle to cancel a running scan (interrupt).

### Frontend framework fit: Svelte 5 (chosen)

Choices considered: Svelte 5 and React 19. Pick: **Svelte 5** (runes).

1. Size & startup: Svelte compiles away; no virtual DOM diffing at runtime. For a scan
   UI that appends thousands of list rows per second this is a real, measurable advantage
   over React's reconciler [INFERENCE: no apples-to-apples benchmark cited; qualitative].
2. Reactivity is built-in (runes: `$state`, `$derived`, `$effect`) — no state library
   (Redux/Zustand) needed for this app's single-page shape; a `Progress` class with `$state`
   fields wired to a Tauri Channel is sufficient.
3. Migration cost from the shelved ratatui plan is zero either way; the team has no React
   investment.
4. React stays the fallback if component-ecosystem needs arise (e.g. a complex treemap
   widget from npm); Svelte can interop via `svelte:component` wrappers, and the Tauri API
   layer (`@tauri-apps/api`) is framework-agnostic.
5. Accessibility: Svelte's compiler emits a11y warnings at compile time
   (`a11y_click_events_have_key_events`, `a11y_label_has_associated_control`, etc.,
   svelte.dev/docs/svelte/compiler-warnings), turning "remember aria-labels" into CI-visible
   errors — a cheap a11y baseline for a non-technical-audience product.

Versions (npm dist-tags, 2026-08-31): `svelte` 5.57.0, `@sveltejs/vite-plugin-svelte` 7.3.0,
`vite` 8.2.2, `typescript` 7.0.2 (note: TS major line 7 now on npm), `svelte-check` 4.7.6.

### State management for long-running scans

- Rust side: scans run on background threads/tasks; the Rust core owns the source of truth
  (WAL + JSONL ledger per SPEC). Managed state via `tauri::Builder::manage(...)` exposes
  `State<ScanRegistry>` to commands; commands take `AppHandle` for emitting/cancelling.
- Frontend side: a single Svelte store-object (`$state` class) per concern — `scanState`,
  `planState`, `ledgerState` — updated only from Channel `onmessage` handlers. Never poll;
  the Channel *is* the state feed. Snapshot commands (`get_ledger_tail`) serve initial mount,
  then channels take over (command for bootstrap + channel for deltas).
- Multi-window: events (`emit_to`) target specific windows; `emit` broadcasts. Window labels
  ("main", "purge-confirm") map to capabilities.

### File-system access: plugin-fs vs Rust command boundary

`tauri-plugin-fs` (2.5.2) gives JS direct fs calls but its security model is a
capability/permission *scope* system: "This module prevents path traversal… Paths accessed
with this API must be either relative to one of the base directories or created with the
path API" (official fs plugin doc). Scoped reads/writes to `$APPCONFIG` etc. are fine, but:

**Decision: all scan/classify/plan/apply/purge I/O goes through Rust commands.** Reasons:

1. The product's correctness lives in the Rust core (`renamex_np(RENAME_EXCL)` quarantine
   moves, WAL, typed byte-count purge). A JS-side fs layer would fork that logic.
2. Capabilities are coarse path scopes; a disk cleaner legitimately touches the *entire
   user-chosen volume*. Granting a webview wildcard fs-write scope is exactly the blast
   radius Tauri's capability system exists to avoid; routing through named commands keeps
   the audit surface at ~10 command names instead of unrestricted fs verbs.
3. Evidence-based grading needs syscalls + metadata the plugin doesn't expose.

Use plugin-fs (or plugin-dialog) only for what it's good at: native open/save dialogs and
reading app config; everything else is a command. Capabilities config lives in
`src-tauri/capabilities/*.json` with per-window `permissions` arrays (verified schema in
/security/capabilities: `core:window:default`, `core:event:default`, `fs:default`, …);
permissions are deny-by-default ("By default, all plugin commands are blocked").

### Packaging & notarization (macOS) / signing (Windows/Linux)

| Platform | Format | Pipeline (official docs) |
|---|---|---|
| macOS | `.app` + `.dmg` | `tauri build --bundles dmg`; sign via `bundle > macOS > signingIdentity` (Developer ID Application) or `APPLE_SIGNING_IDENTITY` env; notarize with App Store Connect API key (`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`) or Apple ID (`APPLE_ID`, `APPLE_PASSWORD`); stapling automatic, `--skip-stapling` for first pass. Ad-hoc signing = pseudo-identity `"-"`, required baseline on Apple Silicon for any locally-built app, but **insufficient for distribution**: Gatekeeper on downloaded (quarantined) apps demands a Developer ID signature + notarization; ad-hoc apps get the "damaged" dialog |
| Windows | `.msi` (WiX) + `.nsis` | sign via `bundle > windows > {certificateThumbprint, digestAlgorithm, timestampUrl}` (OV cert in cert store) or Azure Key Vault signing; no cert = SmartScreen warning on browser downloads |
| Linux | `.deb` / `.appimage` (+ rpm/snap/flatpak/aur) | `tauri build --bundles deb,appimage`; Linux signing documented but optional for a course project |

Course-reality note: a paid Apple Developer account (US$99/yr) is the only path to
notarized DMGs; otherwise distribute unsigned for local course use (ad-hoc, right-click
open) and document Gatekeeper limits. `tauri-apps/tauri-action@v0` GitHub Action builds all
three platforms from one matrix (verified example in signing docs).

### Auto-update

`tauri-plugin-updater` 2.11.0, all three desktop platforms full support (plugin doc table).
Not skippable parts: updates are **signature-verified** — `tauri signer generate` keypair,
pubkey in `tauri.conf.json > plugins > updater > pubkey`, artifacts signed at build with
`TAURI_SIGNING_PRIVATE_KEY` (env, not `.env`), `bundle > createUpdaterArtifacts: true`.
Endpoints are static JSON (`github releases latest.json`) or a dynamic server with
`{{target}}/{{arch}}/{{current_version}}` templating; TLS enforced in production.
Windows `installMode: "passive"` recommended (progress bar, no interaction).
macOS/Linux updater replaces the bundle; on macOS the app must be signed for updates to
behave under Gatekeeper [INFERENCE: not stated in updater doc; flagged as open question].

### Multi-window, tray, menus

- **Multi-window**: `WebviewWindowBuilder` in Rust or `app > windows` array in config;
  each window has a `label`. `tauri-plugin-window-state` 2.4.1 persists geometry.
  For the purge-confirmation sub-dialog and an About window this is first-class.
- **Tray/menubar**: core `tray-icon` Cargo feature (`tauri = { features = ["tray-icon"] }`),
  JS/Rust API `TrayIcon.new` with menu, tooltip, click/hover events (verified /learn/system-tray).
  `tauri-plugin-positioner` 2.3.4 positions tray-anchored popover windows; menu-bar-only
  utility mode = tray icon + `show: false` window. Disk Janitor's background "watch + restore
  window" mode maps onto this directly.
- **Native menus**: `Menu`/`Submenu`/`PredefinedMenuItem` (copy/paste/undo handled by OS),
  `CheckMenuItem` for language switching with accelerators; on macOS top-level items must be
  grouped under submenus and the first submenu lands under the app menu (verified
  /learn/window-menu). Bilingual menu labels set per locale from Rust or JS.

### System-appearance (dark/light) + theming

- Config: `windows[].theme` — "The initial window theme. Defaults to the system theme. Only
  implemented on Windows and macOS 10.14+" (config reference). Omit it to follow the system.
- Runtime change: Rust `WindowEvent::ThemeChanged` — "Only delivered if the window `theme`
  is `None`" (docs.rs tauri 2.11.5). Forward it to the UI via an event/channel.
- CSS: `@media (prefers-color-scheme: dark)` works inside the webview for automatic
  switching without any IPC; Tauri's ThemeChanged is needed only for menu/tray chrome sync
  or forcing one theme. Approach: CSS variables for dark + light palettes, system default,
  in-app override writes `localStorage` + sets an explicit `theme` on the window.
- CN/EN typography: keep `font-family: -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif`
  for Apple-native CJK rendering [INFERENCE: font stack is a convention, not a sourced claim].

### i18n: CN-primary + EN-secondary

- **Chosen: `svelte-i18n` 4.0.1 (MIT)** — message-format based, tiny, `$_('key')` calls,
  `init({ fallbackLocale })`; loads locales from JSON at build time. Fits the small,
  two-locale surface (zh-CN primary, en fallback).
- Alternative: `@inlang/paraglide-js` 2.25.0 (MIT) — compile-time, type-safe, tree-shaken,
  zero-runtime; better if the string table grows or translation CI is wanted.
- react-i18next was evaluated and rejected: no React in the stack.
- Strings live in `src/lib/i18n/zh-CN.json` + `en.json`; language switch also swaps native
  menu labels and tray tooltip via the menu APIs above.

### What "Apple-native feel" concretely requires

1. **Native window chrome**: `titleBarStyle: "Transparent"` / `"Overlay"` + `hiddenTitle:
   true` (config reference; macOS-specific) keeps real traffic lights while content extends
   under the titlebar; `trafficLightPosition` fine-tunes placement (requires Overlay +
   decorations true). Documented pattern: build window in `setup()`, then set
   `NSWindow.setBackgroundColor` via `objc2-app-kit` (official window-customization recipe).
2. **Vibrancy/transparency**: `window-vibrancy` 0.8.0 crate (`apply_vibrancy` with
   `NSVisualEffectMaterial`, macOS 10.10+; `apply_liquid_glass` for macOS 26+; Windows
   Mica/Acrylic). Requires `"transparent": true` **and** `macOSPrivateApi: true` — and the
   config reference warns private APIs block **App Store** acceptance (irrelevant for
   direct-DMG distribution). Linux: unsupported (compositor-controlled).
3. **Native menus**: real macOS menu bar with standard `PredefinedMenuItem`s, not an HTML
   imitation (see menus section).
4. **Native dialogs**: `tauri-plugin-dialog` 2.7.3 for file pickers (sheet-style on macOS).
5. **Single-instance + activation**: `tauri-plugin-single-instance` 2.4.4.
6. **Store-grade polish**: follow `prefers-color-scheme`, SF Symbols-style iconography,
   key-equivalent accelerators on menu items. None of these need private APIs except
   vibrancy, which is App-Store-safe but uses non-public NSVisualEffectView configuration
   paths [INFERENCE: apply_vibrancy wraps NSVisualEffectView; private-API requirement is
   only for window `transparent: true`].

### Accessibility basics

- Svelte compiler a11y warnings (compiler-warnings doc): `a11y_click_events_have_key_events`,
  `a11y_no_noninteractive_element_interactions`, `a11y_label_has_associated_control`, etc.
  Treat warnings as errors in CI; suppress only with `<!-- svelte-ignore code (reason) -->`.
- WebKit (macOS WKWebView) and WebView2 both map DOM accessibility to the platform tree
  (AXTree on macOS, UIA on Windows), so semantic HTML + ARIA carries through to
  VoiceOver/Narrator [INFERENCE: general webview behavior, not individually sourced].
- Tauri-side: keyboard shortcuts via menu accelerators (native, localized) and
  `tauri-plugin-global-shortcut` only if truly global hotkeys are needed (skip by default).

### vs iced / egui (the rejected Rust-native alternative)

| | Tauri 2.11 + Svelte | iced 0.14 (MIT) | egui 0.36.1 (MIT OR Apache-2.0) |
|---|---|---|---|
| Look & feel | system webview + CSS; vibrancy/mica/native menus/dialogs | custom rasterized widgets; not OS-styled | immediate-mode; distinctly non-native |
| Bilingual text | full Unicode/CJK via webview fonts + i18n libs | CJK shaping via custom text stack, font fallback manual [INFERENCE] | same category [INFERENCE] |
| Packaging | native bundle/sign/notarize pipeline built-in | raw binary, bundling DIY | raw binary, bundling DIY |
| Update story | signed auto-updater plugin | DIY | DIY |
| Maturity for product UI | production apps at scale (e.g. 1Password's CN CLI UIs, IntelliJ Rust plugin UIs use Tauri) [INFERENCE — not verified; excluded from Sources] | desktop-capable, smaller widget ecosystem | excellent for internal tools, weak for "consumer app" polish |
| Fit for Disk Janitor | matches "frictionless, Apple-native, non-technical users" goal | would re-create every native affordance by hand | same |

Verdict: for the stated goals (Apple-native feel, non-technical users, real product),
Tauri's native-chrome + packaging + updater pipeline is decisive; iced/egui remain valid
for future embedded "pure Rust" utilities (e.g. a tiny CLI previewer) but not the GUI.

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| tauri-apps/tauri | Apache-2.0 OR MIT | the whole shell; `examples/` for multiwindow | src-tauri/* |
| tauri-apps/plugins-workspace | Apache-2.0 OR MIT (per plugin crate; licenses verified individually on crates.io) | fs, dialog, updater, window-state, single-instance, opener, log, notification, positioner plugins | src-tauri/Cargo.toml |
| tauri-apps/window-vibrancy | Apache-2.0 OR MIT | vibrancy/liquid-glass/mica apply helpers | src-tauri/src/window_fx.rs (new) |
| tauri-apps/tauri-action | MIT | CI matrix for 3-OS build/sign/upload | .github/workflows/release.yml |

## Recommended crates

| Crate | Version (2026-08-31) | Purpose | Notes / Alternative |
|---|---|---|---|
| tauri | 2.11.5 | app shell, commands/channels/events, tray feature flag `tray-icon` | Apache-2.0 OR MIT |
| tauri-build | 2.6.3 | build script, capability schema gen | with @tauri-apps/cli 2.11.4 |
| tauri-plugin-fs | 2.5.2 | scoped app-config fs for JS (dialogs + config only) | main I/O stays in commands |
| tauri-plugin-dialog | 2.7.3 | native open/save/message dialogs | |
| tauri-plugin-updater | 2.11.0 | signed auto-updates, all 3 desktop OS | mandatory key mgmt |
| tauri-plugin-process | 2.3.1 | restart/relaunch after update | pairs with updater |
| tauri-plugin-opener | 2.5.5 | open quarantine folder / ledger file in Finder/Explorer | replaces deprecated shell-open |
| tauri-plugin-log | 2.9.1 | unified logging (no telemetry) | |
| tauri-plugin-single-instance | 2.4.4 | one instance + focus existing window | |
| tauri-plugin-window-state | 2.4.1 | remember window geometry | |
| tauri-plugin-positioner | 2.3.4 | tray-anchored window placement | for menubar mode |
| tauri-plugin-notification | 2.4.0 | system notifications (scan finished) | optional |
| window-vibrancy | 0.8.0 | macOS vibrancy / macOS 26 liquid glass / Win mica-acrylic | cfg-gate per OS |
| tauri-plugin-decorum | 1.1.1 (MIT) | extra macOS window-chrome helpers (inset traffic lights etc.) | optional, copy-with-care NOT needed: MIT, but it's a 3rd-party crate — pin & review |
| objc2-app-kit | 0.3.2 (documented in official recipe) | NSWindow background color for transparent titlebar | macOS only |

All licenses above were read from crates.io API metadata (`license` field of the version
object) with a project User-Agent on 2026-08-31; none is non-permissive.

## Recommended packages (frontend)

| Package | Version | Purpose | License (npm metadata) |
|---|---|---|---|
| @tauri-apps/api | 2.11.1 | invoke/Channel/event/window/menu/tray | Apache-2.0 OR MIT |
| @tauri-apps/cli | 2.11.4 | dev/build/bundle CLI | Apache-2.0 OR MIT |
| @tauri-apps/plugin-fs / -updater / -dialog / -opener | 2.5.2 / 2.11.0 / 2.7.3 / 2.5.5 | JS side of plugins | MIT OR Apache-2.0 |
| svelte | 5.57.0 | UI framework (runes) | MIT |
| @sveltejs/vite-plugin-svelte | 7.3.0 | Vite integration | MIT |
| vite | 8.2.2 | bundler/dev server | MIT |
| typescript | 7.0.2 | type checking | Apache-2.0 |
| svelte-check | 4.7.6 | template type/a11y checking | MIT |
| svelte-i18n | 4.0.1 | zh-CN/en messages | MIT |

## Risks & open questions

1. **WebView version fragmentation.** Windows WebView2 ships via Evergreen runtime; Linux
   WebKitGTK 4.1 quality varies by distro (Ubuntu/Debian/Fedora installs differ per wry
   README). Test matrix on all three OSes is mandatory before "works" claims.
2. **macOS notarization needs a paid Apple Developer account** (US$99/yr). Without it,
   distribution is ad-hoc signed only; Gatekeeper will block downloaded builds. Decide
   before release whether the course project funds this.
3. **`macOSPrivateApi: true` + `transparent: true`** are required for vibrancy and block
   App Store submission (config reference). Direct-DMG distribution is unaffected, but it
   forecloses a future store route without a non-vibrancy build variant.
4. **Updater on macOS without proper signing** [INFERENCE: updater doc does not state the
   interplay]: replacing a signed .app via updater while the app itself is only ad-hoc
   signed may trip Gatekeeper; needs an experiment before shipping auto-update.
5. **Channel payload schema drift** between Rust enums and TS types is untyped at rest
   (serde → TS hand-maintained). Mitigation: `specta`/`tauri-specta` (not version-verified
   in this lane) or codegen tests.
6. **Linux tray icon theme compliance** (AppIndicator APIs) historically flaky across
   desktop environments; verify on GNOME/KDE before promising a menubar mode on Linux.
7. Svelte 5 runes + Tauri channel lifecycle: listeners must be torn down on component
   destroy (official docs warn of leaks/duplicate handlers); audit every `listen()`.

## Platform delta vs macOS (GUI layer)

- **Carries over unchanged**: Rust core + all commands/channels; Svelte UI; i18n; tray,
  menus, multi-window; updater plugin; capability model; window-state.
- **Re-implemented per OS**: vibrancy = `apply_vibrancy` (macOS) vs `apply_mica`/`apply_acrylic`
  (Windows 11/10) vs none (Linux); sign pipeline (Developer ID + notarization vs
  signtool/thumbprint vs none); installer (dmg vs msi/nsis vs deb/appimage); CJK font stack
  (PingFang SC vs Microsoft YaHei vs Noto Sans CJK [INFERENCE: font availability per OS]).
- **Impossible / degraded**: Linux compositor-controlled blur (no vibrancy API); macOS
  App Store path if private APIs enabled; Linux auto-update for deb installs is AppImage
  only in updater flow (deb users update via package manager).

## Sources

1. https://v2.tauri.app/develop/calling-rust/ — commands, args, errors, ipc::Response, events — accessed 2026-08-31
2. https://v2.tauri.app/develop/calling-frontend/ — event ordering caveat, Channels (ordered, typed, streaming) — accessed 2026-08-31
3. https://v2.tauri.app/security/capabilities/ — capability files, permissions, remote access, boundaries — accessed 2026-08-31
4. https://v2.tauri.app/plugin/file-system/ — fs plugin scopes, path-traversal prevention, platform notes — accessed 2026-08-31
5. https://v2.tauri.app/distribute/sign/macos/ — Developer ID, notarization env vars, ad-hoc `-` — accessed 2026-08-31
6. https://v2.tauri.app/distribute/sign/windows/ — OV cert thumbprint/timestamp, Azure Key Vault — accessed 2026-08-31
7. https://v2.tauri.app/plugin/updater/ — pubkey/signing, static JSON endpoints, installMode — accessed 2026-08-31
8. https://v2.tauri.app/distribute/ — bundle formats per platform, build/bundle split — accessed 2026-08-31
9. https://v2.tauri.app/learn/window-customization/ — decorations, transparent titlebar recipe, objc2-app-kit NSWindow — accessed 2026-08-31
10. https://v2.tauri.app/learn/system-tray/ — TrayIcon, menus, events — accessed 2026-08-31
11. https://v2.tauri.app/learn/window-menu/ — Menu/Submenu/CheckMenuItem, macOS submenu rules — accessed 2026-08-31
12. https://v2.tauri.app/reference/config/ — windows[].theme/hiddenTitle/titleBarStyle/trafficLightPosition/transparent, macOSPrivateApi warning — accessed 2026-08-31
13. https://docs.rs/tauri/latest/tauri/enum.WindowEvent.html — `ThemeChanged` (only when `theme` is `None`), v2.11.5 — accessed 2026-08-31
14. https://raw.githubusercontent.com/tauri-apps/wry/dev/README.md — per-OS web engines, WebKitGTK deps — accessed 2026-08-31
15. https://raw.githubusercontent.com/tauri-apps/window-vibrancy/dev/README.md — apply_vibrancy/mica/liquid_glass matrix, transparent+private-api requirements — accessed 2026-08-31
16. https://svelte.dev/docs/svelte/compiler-warnings — a11y warning codes — accessed 2026-08-31
17. crates.io API v1 `/crates/{name}/versions` (User-Agent: disk-janitor-research/0.1) — tauri 2.11.5, plugins, wry 0.56.1, tao 0.37.0, window-vibrancy 0.8.0, tauri-plugin-decorum 1.1.1, iced 0.14.0, egui 0.36.1 + license fields — accessed 2026-08-31
18. npm registry `https://registry.npmjs.org/{pkg}` — @tauri-apps/api 2.11.1, cli 2.11.4, svelte 5.57.0, vite 8.2.2, typescript 7.0.2, svelte-check 4.7.6, svelte-i18n 4.0.1, @inlang/paraglide-js 2.25.0 + license fields — accessed 2026-08-31
