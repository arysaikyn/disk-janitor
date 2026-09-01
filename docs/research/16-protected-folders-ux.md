# Protected Folders & Volume Protection UX — Research Note

Scope: how disk tools implement "never touch this folder/volume" protection (default-protected sets, where exclusion controls live, how protect status is shown during review, conflict handling, cross-session persistence), and what Disk Janitor should adopt — default protection list per OS, UI entry points, storage mapping, Agent interaction. References [02] (landscape) and [13] (product presentation); does not duplicate them. · researched 2026-09-01

## Findings

### 1. Two distinct concepts competitors conflate: "ignore" (scan-skip) vs "protect" (scan-allowed, never-act)

Across the surveyed tools, exclusion controls serve two different purposes that are rarely separated in the UI:

- **Scan-skip ignore** — the path is never enumerated, so its bytes are not counted and it never appears in results. Time Machine exclusions [S4][S5], CleanMyMac's Ignore List [S1][S2], Pearcleaner's exclude dirs [S11], and ripgrep's `.gitignore`/`.ignore`/`.rgignore` [S7] all behave this way for the user-added entries.
- **Hardcoded refuse** — the path IS scanned/counted (so the user sees the bytes), but the tool refuses to act on it. DaisyDisk's Collector refusing `/System`, `/Library`, and the home folder root is the purest example: those petals render but cannot enter the Collector [S3][02 §1]. CleanMyMac's Safety Database hard-excludes `/System/Library` and `/Library` from cleanup even when the rest of the tree is scanned [S6][02 §1].

Disk Janitor should keep these **two separate concepts** (see §6): an `ignore_globs` scan-skip list (already in SPEC Part C §3.8 settings) and a `protected` set that is scanned, counted, graded, but refused at plan time. Conflating them — as CleanMyMac does by routing everything through one Ignore List — means a user who "ignores" a folder loses visibility of its size, which breaks our byte-honesty invariant.

### 2. DaisyDisk — hardcoded refuse-list, no user exclusion controls at all

DaisyDisk has **no user-facing ignore/exclusion list** [S3]. Protection is entirely hardcoded:

- The Collector refuses `/System`, `/Library`, and the current user's home folder root — they cannot be dragged in [S3]. This is a refuse-at-act gate, not a scan-skip: those subtrees are still scanned and displayed.
- "Semi-transparent (consolidated) petals" — aggregated subtrees — cannot be removed wholesale; the user must expand and select individual files/folders before the Collector accepts them [S3]. This is an implicit descendant-protection: the parent is protected until decomposed.
- The only "control" over scope the user has is **permission elevation**, not exclusion: "Scan as Administrator" (standalone edition only; MAS edition sandboxed out) reveals more hidden space [S8]. Granting Full Disk Access in System Settings ▸ Privacy & Security is the consumer path to see restricted dirs [S9].
- No persistent exclusion config; no cross-session ignore memory. The refuse-list is baked into the binary.

Lesson for us: a hardcoded default-protected set is defensible and even desirable for system dirs (users should not have to opt out of `/System`). But DaisyDisk offers **zero user agency** over the protected set — you cannot say "also protect my ~/vault". Our product must let users extend the default set (§6).

### 3. CleanMyMac — Ignore List: multi-module, right-click-to-add, Settings-managed, persistent

CleanMyMac has the richest exclusion UX among cleaners [S1][S2][S6]:

- **Per-module Ignore Lists**, not one global list: separate ignore lists for System Junk, Mail Attachments, Malware Removal, Uninstaller, Updater, Duplicates, Similar Images, Downloads [S2]. An exclusion in one module does not affect another.
- **Two entry points to add** an exclusion:
  1. Inline, from the review list — open Review Details on the scan summary, control-click an item, choose "Add to Ignore List" [S1]. This is a **row-level action**, the pattern our Review screen should copy.
  2. From Settings — CleanMyMac ▸ Settings ▸ Ignore List tab, select a module, click Add, pick items [S2]. This is the **Settings-sheet list** pattern.
- **Management**: Settings ▸ Ignore List tab lists current exclusions per module; select an item ▸ Exclude (−) to remove it, after which it reappears in future scans [S1].
- **Effect**: excluded items are removed from scan results entirely (scan-skip semantics for user-added items) [S1]. "Changes take effect after a new scan" [S2] — no live re-filter.
- **Cross-session persistence**: the Ignore List persists across launches (stored in CleanMyMac's settings) [S2].
- **Hardcoded baseline** layered under the user list: the proprietary Safety Database (curated since 2008) hard-excludes critical system locations such as `/System/Library` and `/Library`, plus per-app exceptions (caches holding licenses, apps that must quit before cleanup) — not user-inspectable [S6][02 §1].
- **CLI parity**: `cleanmymac-cli` exposes `ignore add <path>`, `ignore list`, `ignore remove <path>` [S10] — a clean programmatic surface matching the GUI.

Lesson: the right-click-from-review + Settings-list duality is the strongest pattern. Per-module scoping is overkill for us (we have one cleanup domain), but the **default-baked + user-extended** layering and the **CLI↔GUI parity** are directly adoptable.

### 4. AppCleaner — protection is app-level, not folder-level

AppCleaner's protection model is narrower than folder exclusion: it protects *apps*, not arbitrary paths [S12][S13]:

- Preferences ▸ General: checkboxes for "Protect default OS X apps" and "Protect launched (running) apps", plus a user "protected list" of additional apps [S13]. Disabling "Protect default OS X Apps" unlocks system-app removal (e.g. GarageBand) [S12].
- System apps are non-removable by default; the protection is a refuse-at-act gate, not a scan-skip (AppCleaner still lists related files for protected apps, searchable but not deletable) [02 §1].
- SmartDelete (a daemon) intercepts Finder Trash-drags and offers related-file cleanup — protection extends to the interception path too [02 §1].

Lesson: "protect running/active things" is a valuable default-protection rule our classifier should adopt — a file locked by a running process (`FileLock` evidence) or under an active app container is `HandsOff` by default, mirroring AppCleaner's "Protect launched apps." But AppCleaner has no concept of arbitrary folder protection, so it is not a model for our Settings-list UX.
### 5. Hazel — rules-as-artifact, not exclusion-as-setting

Hazel's model is conceptually orthogonal to a protection list: it has *rules* (conditions → actions) attached to watched folders, not an exclusion list [S14][S15]. But it informs our protection UX in two ways:

- **Preview before act**: "Preview Rule" (⌘⌥P) tests a rule against a chosen item without running actions; each condition shows a green-check (matches) or red-X (does-not-match), live-updating as you edit [S15]. This is the closest cousin to our evidence-panel "what would happen to this row" affordance — protection status should be **inspectable per-row**, not just a global setting.
- **First-match, ordered, enable/disable per-rule** [S14]: rules are a first-class, previewable, toggleable artifact [02 §4]. Our `protected` set should inherit this inspectability — a protected ancestor shows up as evidence on every descendant candidate, not as an invisible global filter.
- Subfolder processing is opt-in via a "Go into subfolders" rule [S16] — Hazel does NOT recurse by default. Lesson for scan-skip: when a user adds an ignore glob, we should be explicit about whether it stops the whole subtree (it does — globset/ignore semantics) and show that in the UI.

Hazel has no hardcoded system-dir protection; it trusts the user entirely because its domain (Desktop/Downloads automation) rarely touches system dirs. Not a model for defaults, but a model for **preview-driven protection display**.

### 6. Time Machine — the canonical "exclusion list" UX, three storage layers

Apple's Time Machine Options panel is the reference for a clean, single-list exclusion UX [S4][S5]:

- **Entry point**: System Settings ▸ General ▸ Time Machine ▸ Options. A single "Exclude from Backups" list; add by + (native picker) or drag items in; remove by selecting ▸ − [S4].
- **No inline/right-click add**: exclusions are managed *only* from this panel, not from a Finder context menu. (Contrast CleanMyMac, which adds the right-click-from-review path.) Lesson: for a system-level, low-frequency setting, the dedicated panel suffices; for an in-flow decision (reviewing a candidate), inline add is better — we want both.
- **Three storage layers**, each with different move-semantics [S5][S17]:
  - `tmutil addexclusion <path>` (no flag) — writes an **xattr** (`com.apple.metadata:com_apple_backup_excludeItem`) on the item; the exclusion **follows the item if it moves** (sticky). [S5]
  - `tmutil addexclusion -p <path>` (sudo) — **path-based**; exclusion stays at the path, so a moved item is no longer excluded. [S5]
  - `tmutil addexclusion -v <volume>` (sudo) — **volume-by-UUID**; if the volume is erased and its UUID changes, the exclusion is lost. [S5]
  - `tmutil isexcluded <path>` queries status [S5].
- Excluded items may still appear in local snapshots [S4] — an honesty caveat Apple surfaces; we should surface the analogous caveat (a protected ancestor does not prevent the OS/CacheDelete from touching those bytes).

Lesson: the **path-vs-sticky distinction matters**. For our `protected` set, path-based (the protection is on the path, not the inode) is the safer default — if a user protects `~/vault`, moving `~/vault` away should NOT silently unprotect it... but on move, the path no longer exists, so the glob simply stops matching. We should document this: protection is path-glob-based; renamed/moved items are protected only if they still match a glob. Sticky (xattr-style) protection is out of scope for v1.

### 7. Developer-tool patterns: ripgrep's layered ignore precedence

ripgrep's ignore system [S7][S18] is the most principled "protection" model for *scan-skip* and informs our `ignore_globs` semantics:

- **Layered precedence** (highest → lowest): `--ignore-file` (CLI-supplied) > global gitignore (`$XDG_CONFIG_HOME/git/ignore`) > `.git/info/exclude` > `.gitignore` > `.ignore` > `.rgignore` [S7]. Higher layers override lower: a `!pattern` (un-ignore / re-include) in `.rgignore` can re-include something `.gitignore` excluded [S18].
- **Why the layering exists**: different authors own different layers — the repo (`.gitignore`), the tool user (`.rgignore`), the global user config. This maps directly onto our ownership layers: **product defaults** (system-dir protection, baked in) > **user global config** (`config.toml` protected list) > **per-scan session overrides**.
- **Negation re-includes**: `!pattern` re-includes below an excluded ancestor [S18] — but gitignore's rule is that **a negation cannot re-include below an excluded *directory*** (only below excluded *files*), because the directory is never descended into. Our `ignore_globs` (powered by globset/ignore [04]) inherits this exact semantics — worth documenting because it's the answer to the conflict-handling question (§9).
- `.rgignore` was later deprecated in favor of `.ignore` so multiple tools (ripgrep, fd) share one format [S19] — a unification lesson: our protection list should be one canonical source (config.toml), not duplicated across GUI state and CLI flags.

### 8. Where exclusion controls live — pattern synthesis across tools

| Tool | Default-protected set | Scan-skip (ignore) entry point | Protect (refuse-at-act) entry point | Persistence |
|---|---|---|---|---|
| DaisyDisk [S3] | /System, /Library, home root (hardcoded) | none | hardcoded refuse at Collector | baked in (no user state) |
| CleanMyMac [S1][S2][S6] | Safety DB (/System/Library, /Library, per-app) | Settings ▸ Ignore List + right-click from Review | hardcoded Safety DB | per-module, cross-session |
| AppCleaner [S12][S13] | default OS X apps, running apps | none (app-level only) | Preferences ▸ General checkboxes + protected list | cross-session |
| Hazel [S14][S15] | none | none (rules, not excludes) | none (rules) | rule file |
| Time Machine [S4][S5] | system files/apps (install-time) | Options panel +/− drag | n/a | xattr / path / UUID |
| Pearcleaner [S11] | none | Settings ▸ Folders include/exclude + per-item toggles | n/a | cross-session |
| ripgrep [S7] | none | .gitignore/.ignore/.rgignore files + --ignore-file | n/a | files on disk |

Synthesis: the **two-entry-point pattern** (Settings-sheet list + inline row action) appears only in CleanMyMac; everyone else picks one or neither. The **default-protected + user-extended** layering (CleanMyMac, Time Machine) is the norm for system-facing tools. Developer tools (ripgrep) push everything to files-on-disk with precedence layers.
### 9. Conflict handling: protected ancestor vs candidate descendant

This is the hard case the brief asks about: a user protects `~/Projects`, but `~/Projects/foo/node_modules` is a `RebuildableFromSource` candidate. What wins?

- **gitignore/ripgrep answer** [S7][S18]: once a *directory* is excluded, the walker never descends into it, so descendant candidates are never seen — and a `!` negation cannot re-include below an excluded *directory* (only below excluded files). This is the scan-skip semantics; it is **total**: protected ancestor ⇒ all descendants invisible.
- **CleanMyMac answer** [S1]: Ignore List entries are scan-skip; adding a folder removes it and all contents from results. No per-descendant override in the UI — you'd remove the whole exclusion.
- **DaisyDisk answer** [S3]: refuse-at-act. The protected ancestor (e.g. home root) is scanned and shown; only the *root* is un-Collector-able, but expanded children ARE individually selectable. So descendants are NOT protected by ancestor protection — you must protect each leaf. This is the opposite extreme.
- **Time Machine** [S4]: path-based exclusion is subtree-total (excluded folder + all contents); xattr-sticky is per-item (an excluded folder's children are excluded too because the folder xattr gates descent).

Recommendation for us: **`ignore_globs` (scan-skip) is subtree-total** (matches globset/ignore [04] — a directory glob stops descent, negation can't re-include below it). **`protected` (refuse-at-act) is also subtree-total by default**, BUT unlike DaisyDisk we surface it differently: a candidate under a protected ancestor renders as a **RefusalRow** (disabled, never hidden — per SPEC Part C §5.2 / RefusalRow.svelte [13 §I]) with the protected ancestor named in the evidence chain. This keeps byte honesty (the user sees those bytes are protected, not absent) while preventing action. A user who wants to act on a specific descendant must remove the ancestor protection or add a per-path un-protect — we do NOT support per-descendant overrides in v1 (too surprising; matches gitignore's no-re-include-below-dir rule).

### 10. How protect status is displayed during review

Competitor patterns for showing protection on a candidate row:

- **DaisyDisk**: protected petals are simply non-draggable into the Collector; no badge or lock glyph — the affordance is *missing* (you try and it refuses) [S3]. Poor discoverability.
- **CleanMyMac**: excluded items are simply *absent* from the review list (scan-skip) [S1]; the only signal they were ever there is the Ignore List in Settings. No inline badge.
- **Hazel**: Rule Status window shows which files match which rules (green/red per condition) [S15] — closest to a per-row evidence display.
- **Pearcleaner**: badges (list/grid views) [S11] — visual grade/category markers per row.

Our product already specifies the right primitive: `RefusalRow.svelte` renders a ✕ row in P1 shape (verdict + 证据 evidence + 替代 alternative), disabled, never hidden [SPEC Part C §5.2][13 §I]. For protected candidates we recommend:
- A **🔒 lock badge** (bilingual `保护 Protected`) on any candidate under a protected ancestor, rendered as a RefusalRow, not an actionable row.
- The evidence chain names the **matching protected glob** (e.g. `protected glob matched: ~/Projects/**`) so the user can see *why* it's protected and where to adjust it.
- The 替代 (alternative) offers a deep-link to Settings ▸ 保护列表 (the protection list), since the resolution is to adjust the list, not to force-delete.

## Reusable code & assets

| Repo/Package | License (verified) | What to borrow | Where |
|---|---|---|---|
| BurntSushi/ripgrep (`ignore` + `globset` crates) | Unlicense OR MIT [04] | layered ignore precedence model; `GitignoreBuilder` for matching user protected globs with negation semantics; the no-re-include-below-excluded-dir rule as the documented conflict-resolution contract | `crates/ignore` (already a SPEC dep [04]) |
| MacPaw/cleanmymac-cli | (license not stated on repo) | `ignore add/list/remove` CLI surface shape — a clean programmatic parity for our CLI; behavior reference only, do not vendor | repo root |
| Apple `tmutil` | (system binary) | the three-layer storage model (sticky-xattr / path / volume-UUID) as the design vocabulary for "how a protection persists across moves" | man page |
| alienator88/Pearcleaner | Apache-2.0 + Commons Clause v1.0 [02 §reusable] | per-item toggle UI for exclude dirs (issue #422) [S11]; reference for "tree-like checkbox" exclude selection — read, do not vendor | Sources, Settings UI |

## Recommended stack additions

| Package | Version (2026-09) | License | Purpose | Why / notes |
|---|---|---|---|---|
| `ignore` (crate) | 0.4.33 [04] | Unlicense OR MIT [04] | `GitignoreBuilder` to match user `protected` globs with the same precedence + negation semantics as ripgrep; already a SPEC dependency [SPEC Part D §1] | reuse, no new dep — the protected list reuses the ignore-engine we already ship for scan-skip |
| `globset` (crate) | 0.4.20 [04] | Unlicense OR MIT [04] | raw glob-set matching for the default-protected set (compiled once at startup); already a SPEC dependency | reuse, no new dep |

No new dependencies are required: protection matching rides entirely on the `ignore`/`globset` pair SPEC Part D already pins for scan-skip and policy globs [04].

## Risks & open questions

1. **Path-based vs sticky protection**: our `protected` set is path-glob-based; a renamed/moved protected item is protected only if it still matches a glob. Time Machine's xattr-sticky model [S5] is more robust but requires writing xattrs on user files — out of scope for v1, but document the limitation in Settings.
2. **Protected ancestor hiding recoverable space**: if a user over-protects (e.g. protects `~/Library` wholesale), the Review will fill with RefusalRows and the "可回收" headline number stays honest but low — risk of user confusion ("why can't I reclaim anything?"). Mitigation: the Dashboard should show a "受保护 protected: N GB" figure alongside 可回收, so protection is visible at a glance, not buried.
3. **Default set for Linux/Windows**: this note researched the macOS default-protected set. The cross-platform defaults (/usr, /opt, /boot, C:\Windows, etc.) need a per-OS pass — partially covered by [10 §HandsOff] and [11] but not yet a consolidated protected-defaults table. Open.
4. **Agent propositions on protected paths**: the SPEC requires the path-blind agent to never see paths [SPEC Part B §3.7]; a protected-path proposition must be refused server-side with P1 (拒绝+证据+替代) before reaching the user. The refusal evidence is "protected glob matched" — but the glob is path data. Resolution: the *fact* of protection (boolean + glob-pattern-shape, not the raw path) can be surfaced to the agent as a refusal reason without leaking the path. Needs an explicit chokepoint test (mirror of the existing path-leak test [07]).
5. **Volume-level toggle**: the brief asks for a volume-level protection toggle (protect an entire external volume). globset handles this (`/Volumes/MyDrive/**`) but the UX — a per-volume switch in Scan view or Settings — is not yet specified in SPEC Part C §3.8. Recommend adding a "保护此卷 protect this volume" toggle to VolumeCard.svelte.
6. **Negation / un-protect**: we do not support per-descendant un-protect in v1 (§9). A power user wanting to protect `~/Projects` but still clean `~/Projects/old/node_modules` has no path. Document as a known limitation; consider a future `!`-glob extension via globset (which supports negation [04]).

## Sources

1. MacPaw — CleanMyMac X: System Junk How to Use (right-click Add to Ignore List; Settings ▸ Ignore List ▸ Exclude) — https://macpaw.com/support/cleanmymac-x/knowledgebase/system-junk — accessed 2026-09-01
2. MacPaw — CleanMyMac safety and reliability (per-module Ignore Lists: System Junk, Mail Attachments, Malware Removal, Uninstaller, Updater, Duplicates, Similar Images, Downloads) — https://macpaw.com/support/cleanmymac/knowledgebase/cleanmymac-safety — accessed 2026-09-01
3. DaisyDisk — User Guide: Deleting files (Collector refuses /System, /Library, home root; semi-transparent petals must be expanded; deletion permanent) — https://daisydiskapp.com/guide/2/en/DeletingFiles — accessed 2026-09-01
4. Apple Support — Exclude files from a Time Machine backup on Mac (Options panel; +/drag; excluded items may appear in snapshots) — https://support.apple.com/en-hk/guide/mac-help/mh15622/mac — accessed 2026-09-01
5. Eclectic Light — Knowing what not to back up, and how (tmutil addexclusion xattr-sticky vs -p path vs -v UUID; isexcluded) — https://eclecticlight.co/2021/08/03/knowing-what-not-to-back-up-and-how/ — accessed 2026-09-01
6. MacPaw — CleanMyMac Safety Database (hard-excludes /System/Library, /Library; curated since 2008; not user-inspectable) — https://macpaw.com/support/cleanmymac-x/knowledgebase/safety-database — accessed 2026-09-01
7. BurntSushi/ripgrep GUIDE.md (layered ignore precedence: .rgignore > .ignore > .gitignore; global gitignore; .git/info/exclude) — https://github.com/BurntSushi/ripgrep/blob/HEAD/GUIDE.md — accessed 2026-09-01
8. DaisyDisk — User Guide: Local APFS snapshots (hidden space; admin scan; standalone vs MAS) — https://daisydiskapp.com/guide/4/en/Snapshots/ — accessed 2026-09-01
9. Swirlzcupcakes — What can I delete from DaisyDisk? (Full Disk Access in System Settings ▸ Privacy & Security to reveal restricted dirs) — http://swirlzcupcakes.com/samples/what-can-i-delete-from-daisydisk/ — accessed 2026-09-01
10. MacPaw — CleanMyMac 3 User Guide PDF (Ignore List: add from Review Details or Preferences; file extensions, system apps; changes take effect after new scan) — https://cms-assets-proxy.macpaw.com/0911b417-25c5-4f52-831e-b18058c76f66.pdf — accessed 2026-09-01
11. alienator88/Pearcleaner issue #422 — Exclude folders from scan regardless of scan level (per-item toggles; tree-like UI; closed Nov 2025) — https://github.com/alienator88/Pearcleaner/issues/422 — accessed 2026-09-01
12. Apple StackExchange — How to Uninstall System Apps by AppCleaner? (Preferences ▸ General ▸ uncheck "protect default OSX Apps") — https://apple.stackexchange.com/questions/232717/how-to-uninstall-system-apps-by-appcleaner — accessed 2026-09-01
13. yama-mac — How to use Mac free app "AppCleaner" (protect default OS X apps / running apps / protected list; SmartDelete tab) — https://yama-mac.com/en/appcleaner/ — accessed 2026-09-01
14. Noodlesoft — The Main Hazel Window (folders list, rules list, rule editor, Preview Rule, Run Rules Now) — https://www.noodlesoft.com/manual/hazel/hazel-basics/main-hazel-window/ — accessed 2026-09-01
15. Noodlesoft — Preview a Rule (⌘⌥P; green-check/red-X per condition; live; works on disabled rules) — https://www.noodlesoft.com/manual/hazel/work-with-folders-rules/create-edit-rules/preview-a-rule/ — accessed 2026-09-01
16. Noodlesoft — Processing Subfolders (Go into subfolders rule; top-level-only by default) — https://www.noodlesoft.com/manual/hazel/advanced-topics/processing-subfolders/ — accessed 2026-09-01
17. alexwlchan — Exclude files from Time Machine with tmutil addexclusion (sticky location-independent vs path-based) — https://alexwlchan.net/notes/2024/exclude-files-from-time-machine-with-tmutil/ — accessed 2026-09-01
18. ripgrep rg(1) man page (precedence: --ignore-file > global gitignore > .git/info/exclude > .gitignore > .ignore > .rgignore; negation cannot re-include below excluded dir) — https://man.archlinux.org/man/extra/ripgrep/rg.1.en — accessed 2026-09-01
19. BurntSushi/ripgrep issue #673 — Why was .rgignore deprecated in favor of .ignore? (unify across tools) — https://github.com/BurntSushi/ripgrep/issues/673 — accessed 2026-09-01
