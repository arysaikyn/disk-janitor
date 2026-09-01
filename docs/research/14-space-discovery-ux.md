# Space-Discovery & Visualization UX — Research Note

How disk analyzers let users *explore* half a million entries — treemap/sunburst
interaction patterns, drill-down, aggregation, performance tricks, and how selection maps
to cleanup actions — as inputs for Disk Janitor's Scan (§3.3) / Review (§3.4) screens and
a Svelte-ecosystem treemap of aggregated DirStat data. Researched 2026-09-01.
Versions/licenses verified against the npm registry on 2026-09-01.

## Findings

### A. DaisyDisk — sunburst + drill-down (the pattern everyone copies)

- **Visual**: colored petals = folders, grey petals = files; petal width ∝ size; very
  small items are grouped into semi-transparent aggregated petals [1].
- **Drill-down**: click a folder petal → a *new* sunburst rooted at that folder
  ("recursive exploration"); click the very **center to go up** to the parent [1][2].
- **Breadcrumbs**: a breadcrumb control above the map tracks the path and enables
  jumping up; hover reveals the item in the sidebar (map ↔ list sync) [1][3].
- **Multi-surface navigation**: map, sidebar list, keyboard shortcuts, and multi-touch
  gestures all stay synchronized — three coordinated components, one selection [1].
- **Aggregation thresholds**: dense areas are thresholded — petals below a minimum
  angular size collapse into a grouped "other" petal; the design retrospective
  documents fixed ring counts (5 main + 5 outer) and hue-inheritance (child petals
  inherit parent hue, shifted) to keep the map readable at depth [3][4].
- **Selection → deletion**: petals can be "torn off" and dragged to a **Collector**;
  deletion happens later, from the Collector — visual staging before action [3].
  ⚠ Our two-phase design deliberately replaces Collector-permanence with quarantine.
- **Keyboard/preview**: space-bar Quick Look on the selected item [2].

### B. WizTree — cushion treemap + zoom, MFT-speed scan

- **MFT-first scanning** (NTFS Master File Table read) gives full-drive scans in
  seconds — the speed baseline every Windows tool is judged against [5][6].
- **Treemap**: cushion style ("Visual Treemap"); file/folder names drawn on large
  items only, name display toggleable [5].
- **Zoom**: double-click a file/folder to zoom in; F10/F11 (numpad +/−) zoom
  in/out; ".." and "\" buttons unzoom/reset; context-menu zoom entries [5].
- **Performance honesty**: the changelog itself says the treemap gets slow with
  extremely large file counts and recommends **turning the treemap off (F9)** —
  the map is a view over the data, never the data model [5].
- **Byte honesty**: treemap can render *allocated* (Size vs Allocated on disk)
  rather than logical size; free space can be drawn as an explicit region [7].

### C. TreeSize — list-first with treemap as secondary view

- TreeSize (JAM Software) leads with a **sorted tree-list** (folder sizes, expandable)
  and offers a treemap as an *additional* view — the table remains the workhorse;
  treemap is for spotting hogs at a glance [8][9].
- Column-style tools (TreeSize, OmniDiskSweeper, DuckDisk — note 13 §E "table-first
  counter-trend") prove that many users, especially developers, prefer precise
  tabular drill-down over pretty maps; the map is the *discovery* layer, the list is
  the *decision* layer [9][13 §E].

### D. GrandPerspective — detail levels, masks vs filters

 macOS treemap utility with three **view detail levels**: Files →
 Packages & Files → Packages & Folders (folders collapsed to single rectangles) —
 an explicit aggregation dial the user controls [10][11].
 **Masks vs filters** is its key conceptual contribution: a **mask** hides items
 that fail it but they *still occupy space* in the view (layout unchanged); a
 **filter** omits items entirely and produces a *new* view. "Rescan with mask as
 filter" bridges the two; filtered scans skip unneeded folders and speed scanning [12].
 **Color mappings** are pluggable (extension, file type, top folder, level,
 timestamps, "Nothing") — color encodes a *chosen dimension*, never hardcoded [11].

### E. ncdu / baobab — keyboard list drill-down vs ring chart

 **ncdu**: curses list, j/k navigate, `l`/Enter open dir, `h`/left go to parent
 (two-key breadcrumb), `d` delete with confirmation (default on), `a` toggle
 disk-usage vs apparent size, `u` shared/unique size (hardlink honesty), sorts by
 name/size/items/mtime. No graphics at all — proof that drill-down + typed actions
 alone can carry the whole workflow for experts [13][14].
 **baobab (GNOME)**: default **ring chart** with hover details; one-click switch
 to treemap; clicking a ring segment or box sets it as the new chart root
 (click-to-drill with implicit re-rooting); deletion is delegated to the file
 manager (Trash), not done in-chart [15][16].

### F. OmniDiskSweeper & Disk Drill — the minimal ends of the spectrum

 **OmniDiskSweeper**: Finder-column list sorted by size descending, navigate +
 Delete; no map whatsoever [17][18]. **Disk Drill Clean Up**: color-coded size bars
 (red→white, largest first) + drill-down list + type filters + multi-select →
 "Move to Trash or Delete permanently" [19].
 Both confirm the same shape: **a sorted list IS the discovery UI for a huge
 fraction of users**; maps accelerate orientation but never replace lists.

### G. Interaction patterns for 500k+ entries (synthesis)

 | Pattern | Who uses it | Lesson for us |
 |---|---|---|
 | Re-root on drill (click = new root) | DaisyDisk, baobab, WizTree | cheap: re-layout one subtree, not the whole tree |
 | Breadcrumb trail + center/`h` to go up | DaisyDisk, ncdu | keep path visible; up-navigation must be one action |
 | Aggregate below a size/angle threshold | DaisyDisk petals, GrandPerspective detail levels | collapse sub-threshold dirs into one "smaller items" block |
 | Map ↔ list synchronized selection | DaisyDisk sidebar | clicking a block highlights/scrolls the matching Review row |
 | Staged selection (Collector / checkboxes) before action | DaisyDisk Collector, Disk Drill | our Review checkbox column; map marks feed the same selection set |
 | Mask (layout-stable hide) vs filter (re-view) | GrandPerspective | filtering must not re-hide refusals (SPEC Part C §3.4 already pins this) |
 | LOD: names only on large blocks; F9-kill switch | WizTree, DaisyDisk | map can be degraded/hidden; the table is always authoritative |
 | Allocated-bytes rendering + free-space region | WizTree | matches our P6 honest-bytes invariant |

### H. Performance techniques for large hierarchies

 - **SVG/Canvas threshold**: peer-reviewed comparison shows FPS degrades around
   ~8–10k rendered elements for both SVG and Canvas; WebGL scales further but text
   rendering dominates cost — keep text minimal at high node counts [20][21].
 - **Angular/size filtering**: the canonical D3 sunburst optimization is dropping
   nodes below a minimum angle (`dx > 0.005` rad) before rendering — cuts element
   count by orders of magnitude with no visible loss [22].
 - **Aggregate-then-render**: layout is O(n) but re-layout on every update is the
   killer; render an *aggregated* tree (dirs with ≥X MB or top-N children), never
   raw 500k leaves [23][24].
 - **Windowing the list side**: Review's table stays virtualized (O(visible))
   per SPEC Part C §3.4; the map side needs the same discipline (one root level +
   one expanded level at a time).
 - **Conclusion for our scale**: 500k raw entries never enter the renderer. The
   Rust core already emits aggregated `DirStat` trees; the Svelte side should
   render ≤ ~2–5k visible blocks per frame (threshold-aggregated), in Canvas or
   SVG-with-LOD — comfortably inside the 10k element budget [20][22].

### I. Selection → cleanup-action mapping (survey)

 | Tool | Selection UI | Action mapping |
 |---|---|---|
 | DaisyDisk | tear-off petals → Collector | delete from Collector (permanent) [3] |
 | Disk Drill | multi-select rows + type filters | Move to Trash / Delete permanently, with confirm [19] |
 | ncdu | cursor row | `d` delete, confirmation default-on [13] |
 | OmniDiskSweeper | column navigation | per-item Delete key (dangerous; no undo) [17] |
 | WizTree | checkboxes in tree/treemap context menu | send to Recycle Bin / delete, export CSV [5] |
 | Disk Janitor (spec) | checkbox rows, refusals disabled | plan → apply (quarantine, 0 bytes) → typed-byte purge → restore ≤72 h |

 The survey's universal gap: **nobody maps a selection to a *graded* action**.
 Ours is unique: selection units are graded candidates, Expensive/HandsOff rows are
 opt-in/locked, and every selection lands in a reviewable manifest — the map's role
 is to make selection *fast*, never to bypass the evidence panel.

### J. Svelte-ecosystem libraries for a DirStat treemap (verified)

 | Library | Version (2026-09-01) | License | Treemap? | Sunburst? | Render | Notes |
 |---|---|---|---|---|---|---|
 | **d3-hierarchy** | 3.1.2 | ISC [23] | ✅ (squarify, binary, dice/slice) | ✅ (partition) | layout only | pure layout: compute `hierarchy()`+`treemap()`/`partition()`, render ourselves in Svelte — leanest, no markup of its own |
 | **layerchart** | 2.3.1 | MIT [24] | ✅ `<Treemap>` | ✅ `<Partition>` | SVG **and Canvas** modes | Svelte 5 native; composable, layered; takes a D3 hierarchy node; weekly ~226k dl; best "batteries" option for map+list sync |
 | **matterviz** | 0.7.0 | MIT [25] | ✅ `<Treemap>` | ✅ `<Sunburst>` | SVG | Svelte; zoom/hover/click built in; `sunburst_from_paths` + `max_depth` for progressive reveal; "Other"-grouping threshold built in; domain-named (materials science) but generic |

 d3-hierarchy does only layout math; d3-selection/d3-transition/d3-zoom (all ISC,
 3.x) add DOM/motion only if Layer Cake/Layerchart aren't doing it. Svelte 5.57.0
 itself is MIT [26].

### K. Recommended interaction model for Scan (§3.3) and Review (§3.4)

 **Scan view (§3.3) — discovery mode.**
 1. Show a **treemap of aggregated DirStat** during/after scan: top-level dir
    blocks, size-proportional; small dirs collapse into one "smaller items"
    block (DaisyDisk/WizTree aggregation pattern, §A/§B).
 2. **Click-to-drill = re-root** (not expand-in-place): clicking a block sets
    it as the new treemap root, the block's children laid out; breadcrumb shows
    the path; center/`Esc` goes up (DaisyDisk/baobab pattern).
 3. **Grade-color overlay** on each block (SPEC Part C §3.1): block border/fill
    tinted by the dominant grade of candidates it contains; HandsOff dirs get a
    🔒 ring; "no recoverable candidates here" blocks stay neutral grey — color
    means *safety*, not category.
 4. **Map ↔ list sync**: clicking a block scrolls/selects the matching row in a
    mini candidate strip; selecting a Review row highlights its block.
 5. **Performance contract**: render only the current root + one expanded level,
    threshold-aggregated to ≤ ~3k blocks; degrade to a "scan progress" ring
    during live scanning (existing §3.3 ring) rather than a half-drawn map.

 **Review view (§3.4) — decision mode.**
 - The Review screen stays **list-first** per its wireframe (the evidence panel
   is the heart); the treemap is an *optional inset* or a togglable split, never
   required. This matches TreeSize/OmniDiskSweeper evidence that lists are the
   decision layer.
 - **Selection is shared**: clicking a treemap block checks its candidate rows
   in the Review table (same selection set, SPEC §3.4 checkbox model);
   HandsOff/actively-written blocks render locked and refuse the click.
 - **Breadcrumb + filter bar** reuse the Review `[筛选 Filter ▾][排序 Sort ▾]`
   controls; filters never re-hide refusals (GrandPerspective mask discipline).

### L. Aggregation thresholds (recommended defaults, tunable)

 | Threshold | Default | Why |
 |---|---|---|
 | Minimum block size to render | 0.1% of current root size | DaisyDisk petal threshold; below this → "smaller items" block |
 | Max blocks per level | ~1500 | keeps element count < 3k across 2 levels, inside the ~10k SVG budget [20] |
 | Max drill depth | 6 | matches DaisyDisk's 5 main + outer rings; deeper = breadcrumb-only |
 | Snapshot-held / free space | separate neutral block, not colored by grade | mirrors WizTree free-space region [7] |

## Reusable code & assets

 | Repo/Package | License (verified) | What to borrow | Where |
 |---|---|---|---|
 | d3/d3-hierarchy (npm `d3-hierarchy` 3.1.2) | ISC [23] | `hierarchy()` + `treemap()` / `partition()` layout math; squarify tile fn | layout engine under our treemap |
 | techniq/layerchart (npm `layerchart` 2.3.1) | MIT [24] | `<Treemap>`/`<Partition>` Svelte components; SVG+Canvas modes; D3-hierarchy input | Svelte-native map components |
 | janosh/matterviz (npm `matterviz` 0.7.0) | MIT [25] | `sunburst_from_paths`, `max_depth` progressive reveal, "Other"-grouping threshold, zoom/hover/click handlers | reference for click-to-drill + threshold aggregation |
 | Layer Cake (layercake.graphics) | MIT | headless Svelte layered chart (SVG/Canvas/WebGL, shared coords) | alternative if Layerchart opinionatedness conflicts |

## Recommended stack additions

 | Package | Version (2026-09) | License | Purpose | Why / notes |
 |---|---|---|---|---|
 | d3-hierarchy | 3.1.2 | ISC [23] | treemap/sunburst layout for aggregated DirStat | leanest; layout only, no DOM — Svelte owns rendering |
 | layerchart | 2.3.1 | MIT [24] | Svelte-native Treemap/Partition components, SVG+Canvas | one-step map+list sync; Svelte 5 ready; primary recommendation |
 | matterviz | 0.7.0 | MIT [25] | reference Treemap/Sunburst with drill-down + threshold grouping | optional; smaller install if only patterns borrowed |

## Risks & open questions

 - Canvas-vs-SVG choice inside Layerchart depends on measured block counts from M1
   real scans (target fixture: 500k entries); if >~3k blocks rendered, prefer
   Canvas mode to stay in the ~10k element budget [20].
 - layerchart is a broad library (~226k dl/week) — pulling only the Treemap
   component may bloat bundle; verify tree-shaking on Tauri webview build, or
   fall back to d3-hierarchy + custom Svelte Canvas renderer.
 - matterviz is materials-science-named but generic; consider whether the
   domain naming is acceptable for a cleanup-tool dependency (cosmetic only).
 - Drill-down as "re-root" vs "expand-in-place": both are valid; re-root is
   simpler and matches DaisyDisk/baobab, but expand-in-place (GrandPerspective
   detail levels) preserves context — an M3 UX decision, not research.
 - Should the treemap appear on the Dashboard (§3.2) as a mini-map? The
   wireframe shows a used/free bar, not a map; adding a small treemap could
   aid discovery but risks competing with the scan CTA — owner UX call.
 - No surveyed tool renders grade/safety color on the map — ours would be
   novel; verify colorblind-safe palette interaction with §3.1/§5.5 at M3.

## Sources

 1. DaisyDisk User Guide — Understanding the disk map — https://daisydiskapp.com/guide/2/en/UnderstandingSunburst/ (accessed 2026-09-01)
 2. DaisyDisk — Wikipedia — https://en.wikipedia.org/wiki/DaisyDisk (accessed 2026-09-01)
 3. T. Brizitsky, "The making of DaisyDisk. Retrospective." — https://tbrizitsky.medium.com/the-making-of-daisydisk-retrospective-706714b4a5e9 (accessed 2026-09-01)
 4. DaisyDisk — A brief history of sunburst visualization — https://daisydiskapp.com/guide/2/en/SunburstHistory/ (accessed 2026-09-01)
 5. WizTree — What's new? — https://wiztree.co.uk/whats-new/ (accessed 2026-09-01)
 6. WizTree — About (MFT scanning) — https://www.diskanalyzer.com/about (accessed 2026-09-01)
 7. WizTree FAQ (Size vs Allocated, free space in treemap) — https://diskanalyzer.com/faq (accessed 2026-09-01)
 8. TreeSize — JAM Software — https://www.jam-software.com/treesize_free (accessed 2026-09-01)
 9. TreeSize feature overview — https://www.jam-software.com/treesize_free/features (accessed 2026-09-01)
 10. GrandPerspective — Display tab — https://grandperspectiv.sourceforge.net/HelpDocumentation/ViewDisplayPanel.html (accessed 2026-09-01)
 11. GrandPerspective — main page — https://grandperspectiv.sourceforge.net/ (accessed 2026-09-01)
 12. GrandPerspective — Filters and masks — https://grandperspectiv.sourceforge.net/HelpDocumentation/MasksAndFilters.html (accessed 2026-09-01)
 13. ncdu(1) — Arch manpage — https://man.archlinux.org/man/ncdu.1 (accessed 2026-09-01)
 14. ncdu — homepage — https://dev.yorhel.nl/ncdu (accessed 2026-09-01)
 15. GNOME Baobab — Different chart views — https://help.gnome.org/baobab/pref-view-chart.html (accessed 2026-09-01)
 16. GNOME Baobab — help index — https://help.gnome.org/baobab/ (accessed 2026-09-01)
 17. OmniDiskSweeper — The Omni Group (1.10) — https://www.omnigroup.com/blog/omnidisksweeper-1.10 (accessed 2026-09-01)
 18. OmniDiskSweeper review (list-based UI) — https://robservatory.com/macos-app-omnidisksweeper/ (accessed 2026-09-01)
 19. Disk Drill — disk space visualization (Clean Up module) — https://www.cleverfiles.com/help/how-do-i-use-the-disk-space-visualization-feature/ (accessed 2026-09-01)
 20. Horak et al., "Comparing Rendering Performance of SVG, Canvas, and WebGL" (VGTC poster, TU Dresden) — https://www.mt.inf.tu-dresden.de/cnt/uploads/Horak-2018-Graph-Performance-Poster.pdf (accessed 2026-09-01)
 21. Horak et al., full VGTC paper — https://www.imld.de/cnt/uploads/Horak-2018-Graph-Performance.pdf (accessed 2026-09-01)
 22. StackOverflow: Sunburst D3 with 10,000 items (angular threshold `dx > 0.005`) — https://stackoverflow.com/questions/38997492/sunburst-d3-with-10-000-items (accessed 2026-09-01)
 23. npm registry: d3-hierarchy 3.1.2, ISC — https://registry.npmjs.org/d3-hierarchy/latest (accessed 2026-09-01, API)
 24. npm registry: layerchart 2.3.1, MIT — https://registry.npmjs.org/layerchart/latest (accessed 2026-09-01, API)
 25. npm registry: matterviz 0.7.0, MIT — https://registry.npmjs.org/matterviz/latest (accessed 2026-09-01, API)
 26. npm registry: svelte 5.57.0, MIT — https://registry.npmjs.org/svelte/latest (accessed 2026-09-01, API)
