# Why DaisyDisk Feels Good — Interaction-Design Principle Extraction — Research Note

Scope: why DaisyDisk's UX reads as high-craft ("Apple-design quality, premium feel" [5]) — the
interaction mechanics behind the felt quality — extracted as testable principles and mapped to
Disk Janitor as **adopt / adapt / already-have / refuse**. Complements [02] (safety model, §2/§4)
and [13 §A/§C/§G] (positioning & marketing): those notes documented *what* DaisyDisk does; this
one asks *why it works* and what we take from it. · researched 2026-09-07; primary sources
verified at source on that date.

> **Method note.** DaisyDisk is proprietary — no public source exists (vendor's only OSS
> artifact is DFeedback, MIT [6]). Principles are inferred from observable behavior (official
> specs/guides, demo, corpus-verified marketing) + independent review signals, never from code.
> Each principle's evidence is marked **verified** (vendor page) or **inference** (analytic).

## Findings

### P1 — Spatial orientation beats reading (verified behavior + review)

- The workflow their own guide teaches: "Look at the disk map and try to locate unusually large
  folders. Then drill down and find out what actually takes so much disk space" [3]. The map
  **is** the interface: orientation, selection, and navigation all happen on it; there is no
  file-list first pass.
- Felt quality, independent: "circular sunburst visualization, Apple-design quality, smooth zoom
  animations, polished, premium feel… repeatedly highlighted as the most beautiful and
  best-designed option" [5].
- Mechanics (inference): area ∝ size is **pre-attentive** — the eye catches the byte hogs with
  zero reading or sorting; the disk becomes a *place* with stable geometry, so re-scans and
  drill-downs keep spatial context; zoom replaces list-paging.
- Boundary: the map answers *where*, never *why* — DaisyDisk's documented gap [13 §G], the exact
  lane our evidence engine owns [SPEC Part A §3].

### P2 — Performance as theater (verified)

- Speed is their #1 claim everywhere: home speed chart (Finder 6 min vs DaisyDisk 15 s) [13 §A/§C];
  specs benchmark "outperforms any existing disk scanning software by 6–32 times", APFS SSD
  12–20 s, "almost instant and doesn't cause distraction" [1].
- The scan→sunburst spin-down in the demo makes speed *felt*, not merely claimed [13 §A].
- Takeaway: the animation is the claim; the numbers make it honest.

### P3 — Embodied staging: the Collector (verified)

- Staging is **direct manipulation**: tear petals off and drop them (plus context menu and ⌘⌫);
  "the files remain intact until you click the Delete button"; the Collector expands into a
  sidebar-like inspector where items can be previewed and **dragged back out**; × removes an
  entry [2].
- Safety is enforced **at the gesture**, not in dialogs: "the Collector will not accept" `/System`,
  `/Library`, the home folder; consolidated (semi-transparent) petals must be expanded before
  removal [2][02 §4].
- Boundary: staging is reversible, deletion is not — 5 s countdown then **permanent unlink**,
  "cannot be 'undeleted'" [2][02 §2]. Half the principle is honest; half is fear theater.

### P4 — Honesty as craft (verified)

- Specs Safety section: "safety stoppers prevent the user from accidentally deleting
  system-critical components", "No automated 'cleaning' is performed", "The user makes the
  informed decision" [1] — deliberately positioned as *analyzer*, not cleaner [13 §C].
- Privacy: "scans only file metadata — name and size", "does not transmit any data outside of
  your computer", no analytics [1].
- Permanent deletion disclosed plainly; rollback outsourced: "make sure your *Time Machine is
  always up and running*" [4].
- The formula (inference): disclose everything, claim less, decide nothing for the user —
  and community trust follows, unlike the incumbent fear playbook [13 §B/§H].

### P5 — Restraint (verified)

- One-time $9.99, "not a subscription!" [13 §A]; no badge counters, no threat framing anywhere
  on the site (contrast CleanMyMac's "329K threats removed" [13 §B]); calm dark navy [13 §A].
- The guide is deliberately conservative: "only delete the files that you can recognize", use
  each app's own UI for its libraries, "ask us if unsure" [4].
- Takeaway: a cleaner that refuses to be aggressive reads as trustworthy — restraint *is* the
  brand.

### P6 — First-class hidden space (verified)

- Hidden space is decomposed into **named causes, each with its own solution**: permissions →
  scan-as-admin / full-disk-access; purgeable space + snapshots → dedicated delete paths; About
  This Mac's "Other" traced to the two above [1]. Purgeable itself is draggable to the Collector
  [3][02 §3].
- Refuses the black-box aggregate (contrast macOS built-in "System Data" [02 §1]).

## Mapping to Disk Janitor

| Principle | Verdict | How (design candidates — note only, not yet spec'd per owner call) |
|---|---|---|
| P1 spatial orientation | **ADAPT** (biggest open question) | Review gains a spatial layer: size-scaled treemap/sunburst **colored by grade**, drill-down = navigation, evidence panel on selection. Map answers *where*, evidence answers *why* — the combination DaisyDisk structurally can't do [13 §G] |
| P2 performance theater | ALREADY-HAVE + ADAPT | scan ring + live rate exist (Part C §3.3); ADAPT = publish real benchmark numbers (dua-core walker) wherever we claim speed — never vague "fast" |
| P3 embodied staging | ADAPT | plan manifest stays (Hazel pattern, Part C §3.5); ADAPT = tactile staging: drag-to-plan gesture, staging totals with 「释放 freed 0 字节」 visible *during* staging, staged items always draggable out |
| P4 honesty as craft | ALREADY-HAVE | anti-fear voice table (Part C §1), zero telemetry on screen, 恢复是权利; our step beyond: every disclosure is backed by *architecture* (quarantine+WAL), not just copy |
| P5 restraint | ALREADY-HAVE | calm voice, no counters, no daemon (Part A §8); if we ever charge: lifetime reads as trust [13 §I] |
| P6 hidden space | ADAPT | Scan/Dashboard shows named-cause hidden-space breakdown per volume (permissions-gated / purgeable / snapshots / dataless) with per-cause evidence-or-refusal — never an aggregate |

## Refusals (with reason)

- **5 s countdown-to-unlink**: fear theater posing as undo — no restore channel exists [02 §2].
  Replaced by quarantine + typed byte-count gate [ADR-10].
- **"Make sure Time Machine is always running"**: rollback outsourced to the user [4]. Replaced
  by same-volume quarantine + WAL restore within 72 h — the undo moment no competitor shows on
  camera [13 §G1].
- **"The user makes the informed decision" as the entire safety model**: disclose-and-walk-away
  [1]. Replaced by machine-derived evidence chains + a propose-only, path-blind agent
  [SPEC Part A §3][ADR-7].

## Risks & open questions

1. **Felt-quality evidence is thin**: vendor copy + one independent comparison [5] + corpus
   community signals [13 §B]. Mining App Store reviews would strengthen P1/P5 — queued, not
   blocking; do not ship a claim like "reviewers call it the most beautiful analyzer" on one
   source.
2. P1 is the only principle that would change product **scope** (a map layer in Review) — it is
   deliberately left unspec'd here; the Review-screen decision stays an owner call.
3. P3's tactile-staging delta interacts with the plan-manifest contract (Part C §3.5) — any
   adoption must keep the manifest as the review artifact, not replace it with a canvas.

## Sources

1. DaisyDisk Tech Specs — speed benchmark (6–32×, APFS SSD 12–20 s), hidden-space decomposition,
   Safety, Privacy — https://daisydiskapp.com/specs/ — accessed 2026-09-07
2. DaisyDisk User Guide: Deleting files — Collector mechanics, gesture-level refusals, 5 s
   cancel, permanent unlink — https://daisydiskapp.com/guide/3/en/DeletingFiles/ — accessed 2026-09-07
3. DaisyDisk User Guide: Locating space wasters — map→locate→drill-down workflow; purgeable to
   Collector — https://daisydiskapp.com/guide/4/en/LocatingSpaceWasters — accessed 2026-09-07
4. DaisyDisk User Guide: What is safe to delete? — recognize-only rule, app-UI guidance,
   Time Machine as rollback — https://daisydiskapp.com/guide/what-to-delete — accessed 2026-09-07
5. Anhphong — Best Disk Analyzer for Mac (2026): DaisyDisk vs Storage Peek vs Free — independent
   design-quality signal — https://anhphong.dev/blog/best-disk-space-analyzer-mac/ — accessed 2026-09-07
6. DaisyDisk blog: Sharing code — DFeedback MIT release, the extent of their open-sourcing —
   https://daisydiskapp.com/blog/sharing-code/ — accessed 2026-09-07 (via search summary)
7. Corpus cross-refs: [02] `docs/research/02-macos-cleanup-landscape.md` (§2 taxonomy, §3 purgeable
   facts, §4 lessons) · [13] `docs/research/13-disk-tool-product-presentation.md` (§A positioning,
   §C DaisyDisk, §G evidence gap, §H voice, §I pricing) · [SPEC Part A §3/§8] · [ADR-7][ADR-10]
