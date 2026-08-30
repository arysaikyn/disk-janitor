# macOS Cleanup-Tool Landscape & Safety Models — Research Note
Scope: what CleanMyMac, DaisyDisk, AppCleaner, OmniDiskSweeper, GrandPerspective, Pearcleaner, Hazel, Disk Drill and Apple's built-in Storage Management target; each tool's deletion/undo model and published rules DB; license & price; plus Apple platform facts on purgeable space and APFS snapshots. · researched 2026-08-30

## Findings

### 1. Landscape: targets, safety model, rules DB, license

| Tool (version, 2026-08) | Targets | Safety / undo model | Published rules DB | License & price |
|---|---|---|---|---|
| CleanMyMac 5.5.7 (MacPaw) | system junk, caches, logs, language packs, app leftovers, malware, optimization | "Move to Trash" default; also Remove Immediately / Remove Securely (shred); review list before acting; no undo once Trash emptied | Yes — proprietary **Safety Database** (since 2008): rules + exceptions, e.g. caches holding licenses, apps that must quit before cleanup. Not user-inspectable. [S1][S2] | Proprietary. ~US$39.95/yr (1 Mac); one-time purchase path exists (≈€79.95 listed). [S3] |
| DaisyDisk 4.10+ (Software Ambience) | sunburst disk map incl. hidden/purgeable/snapshot space | Collector staging → review → Delete → 5 s cancel countdown → **permanent unlink, NOT Trash** ("cannot be 'undeleted'"); refuses /System, /Library, home in Collector; purgeable force-purge by dragging it to Collector (standalone edition only) | No | Proprietary. US$9.99 one-time, ≤5 personal Macs; separate MAS edition with reduced deletion powers. [S4][S5][S6][S8] |
| AppCleaner 3.6.8 (FreeMacSoft, 2023-07-04) | app uninstall + related/orphaned files (prefs, caches, app support) | Checkbox review list; **moves app + related files to Trash** (Put Back possible); optional SmartDelete daemon intercepts Finder Trash-drag and offers related files; system apps not removable, related files searchable | No published DB; related files matched by bundle-id/name heuristics. [S9][S10] | Freeware, closed source. Free. |
| OmniDiskSweeper 1.16 (Omni Group; 2025-09-12 build) | file list largest→smallest | "Trash or open them" — no quarantine beyond Trash, no rules, no staging | No | Freeware (2012 freeware announcement), closed source; macOS 11+. [S11][S12][S13] |
| GrandPerspective 3.8.0 (Eriban, 2026-08-24) | treemap viz; delete from view; advanced filters; TM hard-link analysis; dataless/cloud-file awareness | Delete from view (exact destination/undo model undocumented on official site — gap, see §Risks); scan results savable/reloadable; filters persisted | No | **GPL-2.0** (LICENSE verified in source tree). Free on SourceForge; US$2.99 on MAS. [S14][S15] |
| Pearcleaner 5.4.3 (Alienator88; dev On Hold since end-2025) | app uninstall, orphaned-file search, dev-env cleanup, Homebrew manager, Sentinel monitor | Orphaned search with adjustable sensitivity + include/exclude dirs; **Sentinel** background Trash-event monitor (~2 MB RAM) offers related-file cleanup when an app is trashed; privileged helper for system folders | No vendor DB; heuristics in open source | **Apache-2.0 + Commons Clause v1.0** (LICENSE.md verified; monetization/"Sell" prohibited). Free. [S16][S17] |
| Hazel 6.1.x (Noodlesoft) | rule-based folder automation; Trash auto-clean (age/size); App Sweep on app deletion | Transparent user-authored rules: first-match order, per-rule enable/disable, run-status view, test/preview runs; actions → Move to Trash; Trash rules then delete **permanently** (time- or size-based); Hazel 6 adds a "Revert Files" undo feature | Rules are user-authored (no vendor DB); semantics documented in manual. [S18][S19][S21] | Proprietary. ≈US$42 single / US$65 family-5 [secondary source S22]; 14-day eval, then demo mode. macOS 13+. [S20] |
| Disk Drill 6.3.2326 (CleverFiles) | data recovery, not cleanup — included as the "undo-first" reference product | **Recovery Vault / Guaranteed Recovery**: pre-deletion metadata or byte-level retention so recovery is possible later; recovery-chance predictions (high/medium/low) shown per file | No | Proprietary. Basic free (scan + preview, 100 MB recovery); PRO US$89/yr or US$149 lifetime; MAS US$89.99. [S23][S24] |
| macOS Storage Management (built-in) | category browse; Store in iCloud; Optimize (watched Apple TV movies, old email attachments); Empty Trash after 30 days; Reduce Clutter (large/unused browse) | All deletions user-confirmed via standard flow; system may itself purge "safe caches and logs"; no evidence chains; "System Data" is an opaque aggregate category | Apple's purge rules are internal (CacheDelete plists, ~32 of them) — not published. [S26][S27] | Bundled with macOS. |

> ⚠ Spec correction: docs/SPEC.md's pain-point table claims DaisyDisk's undo "depends on Trash not being emptied" — the official guide states deletion is **permanent** and files "cannot be 'undeleted'" [S4]. The design doc must not rely on a Trash round-trip for DaisyDisk; it actually *lacks* our restore path.

### 2. Safety-model taxonomy (reversibility spectrum)

1. **No-op + system reclaim** (macOS purgeable): zero user risk; but only system-owned caches/snapshots are eligible [S5][S27].
2. **Metadata pre-capture** (Disk Drill Recovery Vault): reversible-by-reconstruction; must be enabled before loss; storage-hungry [S24].
3. **Trash** (AppCleaner, OmniDiskSweeper, Hazel, Apple's own flows): OS-provided Put Back; survives only until Trash emptied; Put-Back metadata format undocumented [S9][S11].
4. **Staged manifest + confirm** (DaisyDisk Collector): plan-like UX, but post-confirm action is permanent [S4].
5. **Same-volume quarantine move** (Disk Janitor's apply): atomic rename, zero bytes freed until purge — **no surveyed tool does this**; closest cousin is Apple's snapshot-then-delete and Disk Drill's pre-capture.
6. **Shred / secure-erase** (CleanMyMac "Remove Securely"): deliberately irreversible — anti-pattern for us; our purge confirm (typed byte count) replaces it.

### 3. Apple platform facts: purgeable space, APFS & Time Machine snapshots, "free space"

- **Purgeable space ≠ free space.** Finder/About-This-Mac "available" = free + purgeable. Purgeable consists mainly of local TM snapshots, plus caches, sleep image, swap, other temp files; macOS reclaims it instantly on demand and otherwise lets it grow to ~80% of disk capacity (Apple doc HT204015 as cited by DaisyDisk guide). [S5]
- **Who computes it:** the CacheDelete daemon, not APFS. Disk Utility/apps using `volumeAvailableCapacityForImportantUsageKey` read CacheDelete's totals; `statfs(2)`/`fstatfs(2)` (Foundation `NSFileSystemFreeSize`) report only raw FS free, **not** purgeable. [S26][S28][S29]
- **TM local snapshots:** created ~hourly, kept 24 h; one more snapshot kept after each successful backup until space needed; one more before each macOS update (10.13+). Space they use is counted as available. Manual removal supported only by toggling TM backups off/on (consumer path) — not documented as user-purgeable directly. [S7]
- **Command-line control (man pages):** `tmutil listlocalsnapshots <mount>` / `listlocalsnapshotdates` / `deletelocalsnapshots {mount|date}` / `thinlocalsnapshots <mount> [purge_amount] [urgency 1-4]` [S30]; `diskutil apfs listSnapshots [-plist] <vol>` and `diskutil apfs deleteSnapshot <vol> -uuid|-xid|-name` (root/ownership required). [S31]
- **Syscall-level facts (verified in man page):** `renamex_np(2)` with `RENAME_EXCL` fails `EEXIST` if destination exists (fs support via `getattrlist` cap `VOL_CAP_INT_RENAME_EXCL`); cross-device rename → `EXDEV`; rename touching dataless (iCloud-evicted) files with disallowed I/O policy → `EDEADLK`. These map exactly onto our same-volume quarantine + dataless-file risks. [S32]
- **API surface for tools:** `URL.resourceValues(forKeys:)` with `volumeAvailableCapacityKey` (raw), `volumeAvailableCapacityForImportantUsageKey` (free + purgeable "important"), `volumeAvailableCapacityForOpportunisticUsageKey` (adds opportunistic). Privacy: declare usage reason in `PrivacyInfo.xcprivacy` (fingerprinting vector). [S28][S33]
- Snapshot purging order under pressure: CacheDelete purges phased caches first; TM local snapshots last; only TM purges TM snapshots. [S27]
- Container arithmetic: APFS container free = capacity − Σ volumes; space can appear "used" by a snapshot even after file deletion until snapshot purged. [S34]

### 4. Design lessons: what each tool gets right / wrong about reversible deletion

- **CleanMyMac — right:** curated rule DB with per-app exceptions (caches-with-licenses, quit-before-clean) ≈ our Evidence model; review-before-act. **Wrong:** DB is black-box, human-curated, no machine evidence; "Remove Securely" is deliberately irreversible. Lesson: rules + evidence should be *inspectable* (our manifest), and grades like `PureCache` should be obvious.
- **DaisyDisk — right:** staging area (Collector) = our `plan` JSON preview; refuse-list for system dirs; treats purgeable/snapshots as first-class objects with explicit "hidden space" breakdown instead of hiding them. **Wrong:** the stage ends in permanent unlink — a 5 s countdown is not an undo channel. Lesson: keep the staging affordance, replace the countdown with quarantine + grace window.
- **AppCleaner — right:** relies on OS Trash = real Put-Back path; SmartDelete hooks the *existing* deletion gesture instead of adding a new one. **Wrong:** bundle-id/name heuristics produce false-positive leftovers with no evidence trail; one undo layer only (Trash). Lesson: name-based matching is exactly what our evidence engine must supersede — but its trash-interception UX is worth copying (our watcher could observe Trash events too).
- **OmniDiskSweeper — right:** zero-friction list+Trash. **Wrong:** nothing else — no memory, no judgment, no undo design. Lesson: fastest failure mode; proves judgment/reversibility is the actual product gap.
- **GrandPerspective — right:** persisted scan results (re-review without rescan), advanced filters, hard-link-aware TM analysis, cloud/dataless-file awareness. **Wrong:** deletion from view with an undocumented undo story. Lesson: scan-result persistence is cheap and valuable for our ledger UX; deletion must never be one-click on a visualization.
- **Pearcleaner — right:** open-source code worth reading for orphaned-file search heuristics and Sentinel's event-driven Trash monitoring; privacy-lean. **Wrong:** Commons Clause + development On Hold = do not vendor its code; still fine to read for behavior. Lesson: even a good open-source scanner can't promise its own future — design our ledger to be self-sufficient.
- **Hazel — right:** the only tool that treats *rules* as a first-class user-facing artifact (ordered, testable, previewable) — closest philosophical cousin to our policy→plan compiler; Trash age/size policies mirror our 72 h grace idea; "Revert Files" undo in Hazel 6 validates post-hoc undo demand. **Wrong:** rules are name/metadata-matchers, not build-system or rebuild-cost-aware. Lesson: borrow rule-preview UX; keep evidence-graded classification as our differentiator.
- **Disk Drill — right:** inverts the problem: capture before loss (Recovery Vault/Guaranteed Recovery), predicted recovery chance per item. **Wrong:** for cleanup that means paying storage to insure deletions, and only for things it saw deleted. Lesson: our WAL+quarantine is cheaper insurance (zero bytes until purge); per-item recovery-confidence framing ("rebuildable in ≈40 s") is a great UX vocabulary to imitate.
- **Apple Storage Management — right:** no-op-first philosophy (purgeable reclaim before user deletion), simple age-based Trash policy. **Wrong:** opaque "System Data" aggregate, no evidence, no cross-session memory. Lesson: our ledger (persistent JSONL) + evidence chains are the two features the OS has never offered; document purgeable clearly instead of burying it in a category.

## Reusable code & licenses

| Repo | License (verified) | What to borrow | Where (file/module) |
|---|---|---|---|
| alienator88/Pearcleaner v5.4.3 | Apache-2.0 **+ Commons Clause v1.0** — copy-with-care (no "Sell"; fine to read, not to vendor into a monetized product) | orphaned-file search heuristics; Trash-event monitor (Sentinel); privileged-helper install pattern | Sources/Sentinel, search logic |
| grandperspectiv (SourceForge) v3.8.0 | GPL-2.0 — copy-with-care (viral for any linked code) | scan-result persistence format; filter predicates; dataless/cloud-file handling | Objective-C sources in src tgz |
| popyapp/app_cleaner | open-source mirror of AppCleaner-style logic (verify per-file before use) | bundle-id-based related-file matching as a *baseline to beat* | repo root |
| rust-lang/libc 0.2.189 | MIT OR Apache-2.0 | `renamex_np`, `RENAME_EXCL`(0x4), `VOL_CAP_INT_RENAME_EXCL` bindings already present | src/unix/bsd/apple/mod.rs L3199-3200, L4634 |
| The Omni Group apps | closed freeware — reference behavior only | none (no source) | — |

## Recommended crates

| Crate | Purpose | Why / notes (version & license verified 2026-08-30 via crates.io) | Alternative |
|---|---|---|---|
| `libc` 0.2.189 | raw syscalls | `renamex_np` + `RENAME_EXCL` already exported for apple targets; MIT OR Apache-2.0 | `nix` 0.31.3 (MIT; safe wrappers) |
| `nix` 0.31.3 | safe syscall wrappers | covers rename-family as `nix::fcntl`/unistd wrappers; pair with libc for exotic flags | libc direct |
| `objc2-foundation` 0.3.2 | Foundation from Rust | `NSURLResourceKey` volume-capacity keys (`volumeAvailableCapacityForImportantUsageKey`) for correct free-space reporting; MIT | `sysinfo` for coarse stats |
| `trash` 5.2.6 | OS Trash ops | optional fallback path for `HandsOff`-adjacent items; MIT | manual `NSWorkspace` via objc2 |
| `notify` 8.2.0 | FS event watch | Trash/Sentinel-style interception, project-dir watching; **CC0-1.0 — copy-with-care per brief allowlist** (public-domain-style, generally OK but nonstandard) | `notify-debouncer-full` on top |
| `walkdir` 2.5.0 | parallel-friendly tree walk | metadata-first iteration; Unlicense/MIT | `jwalk` (parallel walker) |
| `rayon` 1.12.0 | parallelism | scan/classify fan-out; MIT OR Apache-2.0 | std threads |
| `sysinfo` 0.39.6 | disk/volume overview | quick capacity panels in TUI; MIT | objc2-foundation direct |
| `serde`/`serde_json` 1.0.229/1.0.151 | manifest + JSONL ledger | plan manifest, WAL, ledger records; MIT OR Apache-2.0 | — |

## Risks & open questions

1. **Purgeable/available reporting mismatch:** `statfs`/`NSFileSystemFreeSize` excludes purgeable; UI must choose one definition and label it. Which key do we surface in the TUI (`volumeAvailableCapacityKey` vs `ForImportantUsage`)? Note `PrivacyInfo.xcprivacy` declaration requirement. [S28][S26]
2. **Deleting files covered by a TM local snapshot doesn't free space** until the snapshot thins/purges; our "bytes freed" report must anticipate this (query `diskutil apfs listSnapshots`, maybe warn). [S7][S5]
3. **Dataless (iCloud-evicted) files:** `renamex_np` returns `EDEADLK` when touching dataless items under restrictive I/O policy; also SF sync placeholders. Scanner must never trigger materialization — needs a dataless-detection flag (`st_flags` UF_COMPRESSED? SF_UNSUPPORTED?) — unresolved; verify `getattrlist` `ATTR_FILE_DATALESS` (unconfirmed symbol). [S32]
4. **GrandPerspective's actual delete destination** (Trash vs unlink) is undocumented on official pages — verify in source before citing in DESIGN.md.
5. **Trash Put-Back metadata format is undocumented** — our restore must store original paths in the WAL ourselves, not rely on `.DS_Store`-style Trash metadata.
6. **Cross-volume quarantine:** `renamex_np` gives `EXDEV` across devices; quarantine dir must be created per-volume (SPEC §7 already flags this). Same applies to purge on external volumes.
7. Hazel price sourced from a secondary review (S22); Noodlesoft store page lists no numeric price — confirm $42/$65 before quoting in course docs.

## Sources

1. MacPaw — CleanMyMac Safety Database knowledgebase — https://macpaw.com/support/cleanmymac/knowledgebase/cleanmymac-safety — accessed 2026-08-30
2. MacPaw — Smart Scan / Safety Database — https://macpaw.com/cleanmymac-x/smart-cleanup — accessed 2026-08-30
3. MacPaw — CleanMyMac plans (US$39.95/yr 1 Mac; one-time option) — https://macpaw.com/store/cleanmymac — accessed 2026-08-30
4. DaisyDisk — User Guide: Deleting files (permanent, not Trash; 5 s cancel) — https://daisydiskapp.com/guide/3/en/DeletingFiles/ — accessed 2026-08-30
5. DaisyDisk — User Guide: Purgeable space (definition, 80% cap, force purge) — https://daisydiskapp.com/guide/4/en/PurgeableSpace/ — accessed 2026-08-30
6. DaisyDisk — purchase page (US$9.99) — https://daisydiskapp.com/ — accessed 2026-08-30
7. Apple Support — About Time Machine local snapshots (hourly/24 h, pre-update snapshots; counted as available) — https://support.apple.com/en-us/102154 — accessed 2026-08-30
8. DaisyDisk guide — Editions/hidden space & snapshot deletion restrictions (standalone vs MAS) — https://daisydiskapp.com/guide/4/en/Snapshots/ — accessed 2026-08-30
9. FreeMacSoft — AppCleaner page + release notes (3.6.8, 2023-07-04) — https://freemacsoft.net/appcleaner/ · https://freemacsoft.net/appcleaner/releasenotes.html — accessed 2026-08-30
10. Macworld — AppCleaner related-files/SmartDelete behavior — https://www.macworld.com/article/226619/remove-extra-application-files-with-appcleaner.html — accessed 2026-08-30
11. Omni Group — Omni Labs (OmniDiskSweeper "Trash or open") — https://www.omnigroup.com/more — accessed 2026-08-30
12. Omni Group — freeware announcement — https://www.omnigroup.com/blog/OmniWeb_OmniDazzle_OmniDiskSweeper_and_OmniObjectMeter_now_freeware — accessed 2026-08-30
13. Omni downloads index (1.16, 2025-09-12) — https://files.omnigroup.com/software/macOS/11/ — accessed 2026-08-30
14. GrandPerspective site (3.8.0; GPL; MAS US$2.99; features) — https://grandperspectiv.sourceforge.net/ — accessed 2026-08-30
15. GrandPerspective LICENSE (GPL-2.0 text) — https://sourceforge.net/p/grandperspectiv/source/ci/master/tree/LICENSE?format=raw — accessed 2026-08-30
16. Pearcleaner README (v5.4.3, On Hold, features, Sentinel) — https://github.com/alienator88/Pearcleaner/blob/main/README.md — accessed 2026-08-30
17. Pearcleaner LICENSE.md (Apache-2.0 + Commons Clause v1.0) — https://github.com/alienator88/Pearcleaner/blob/main/LICENSE.md — accessed 2026-08-30
18. Noodlesoft — Hazel overview / Taking Out the Trash (App Sweep) — https://www.noodlesoft.com/ — accessed 2026-08-30
19. Noodlesoft manual — rule logic & Trash auto-deletion (permanent; time/size) — https://www.noodlesoft.com/manual/hazel/hazel-basics/manage-your-trash/use-automatic-deletion/ — accessed 2026-08-30
20. Noodlesoft — Hazel 6.1 release notes (macOS 13+) — https://www.noodlesoft.com/kb/hazel-6-1-release-notes/ — accessed 2026-08-30
21. Noodlesoft — What's New in Hazel 6 (Revert Files) — https://www.noodlesoft.com/whats-new-in-hazel-6/ — accessed 2026-08-30
22. Bundl.run Hazel review (price US$42/$65; secondary source) — https://bundl.run/apps/hazel — accessed 2026-08-30
23. CleverFiles — Disk Drill editions/pricing (PRO US$89/yr, US$149 lifetime) — https://www.cleverfiles.com/help/what-are-the-differences-between-disk-drill-basic-pro-expert-and-enterprise/ — accessed 2026-08-30
24. CleverFiles — Recovery Vault / Guaranteed Recovery explained — https://www.cleverfiles.com/help/protect-data-disk-drill/ — accessed 2026-08-30
25. CleverFiles — Disk Drill macOS versions (latest 6.3.2326, macOS 10.15–26.x) — https://www.cleverfiles.com/download-mac-versions.html — accessed 2026-08-30
26. Eclectic Light — Where does macOS get its volume free space figures (CacheDelete; 32 plists; fstatfs gap) — https://eclecticlight.co/2023/04/27/where-does-macos-get-its-volume-free-space-figures-from/ — accessed 2026-08-30
27. Eclectic Light — Ventura space management: what gets purged and how (CacheDelete phases; TM snapshots last) — https://eclecticlight.co/2023/04/19/ventura-space-management-what-gets-purged-and-how/ — accessed 2026-08-30
28. Apple Developer — Checking Volume Storage Capacity (important vs opportunistic keys) — https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity — accessed 2026-08-30
29. Apple Developer — volumeAvailableCapacityForImportantUsageKey (URLResourceKey; PrivacyInfo.xcprivacy) — https://developer.apple.com/documentation/foundation/urlresourcekey/volumeavailablecapacityforimportantusagekey — accessed 2026-08-30
30. tmutil(8) man page — listlocalsnapshots/deletelocalsnapshots/thinlocalsnapshots — https://keith.github.io/xcode-man-pages/tmutil.8.html — accessed 2026-08-30
31. diskutil(8) man page — apfs listSnapshots / deleteSnapshot — https://keith.github.io/xcode-man-pages/diskutil.8.html — accessed 2026-08-30
32. rename(2)/renamex_np(2) man page — RENAME_EXCL/EEXIST, EXDEV, EDEADLK, VOL_CAP_INT_RENAME_EXCL — https://keith.github.io/xcode-man-pages/rename.2.html — accessed 2026-08-30
33. Foundation Release Notes macOS 10.13 (NSURL volume capacity keys introduced) — https://developer.apple.com/library/archive/releasenotes/Foundation/RN-Foundation/index.html — accessed 2026-08-30
34. Eclectic Light — The arithmetic of free space (container capacity − volumes) — https://eclecticlight.co/2026/08/21/the-arithmetic-of-free-space/ — accessed 2026-08-30
35. Apple Support — Optimize storage space on your Mac (Reduce Clutter; Empty Trash 30 days) — https://support.apple.com/guide/system-information/optimize-storage-space-sysp4ee93ca4/mac — accessed 2026-08-30
36. crates.io API — license/version fields for libc, nix, objc2-foundation, trash, notify, walkdir, rayon, sysinfo, serde, serde_json — https://crates.io/api/v1/crates/<name> — accessed 2026-08-30
