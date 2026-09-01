# Brand, Name-Collision & Course-Audit Deliverables — Research Note
Two parts: (1) BRAND — name collision audit for "Disk Janitor / 还我磁盘", naming conventions of successful disk tools, 2–3 naming/visual directions; (2) AUDIT — every graded course deliverable (deadline 9/10), current gaps, and a deliverables-checklist structure · researched 2026-09-01

> Binds to SPEC Part A §5 (R1–R6) + §History, AGENTS.md (course deliverables), note 13
> (product presentation). Live data fetched 2026-09-01 (GitHub API + npm registry; web
> search providers were TLS-down, so App Store/trademark checks are partial — flagged below).

## Part 1 — Brand

### 1. Name-collision audit (live, 2026-09-01)

| Surface | Result | Verdict |
|---|---|---|
| GitHub repo search `"disk janitor" in:name` | **21 repos**; 5 notable: `Nikoxx99/DiskJanitor` (macOS cleanup dashboard for devs — node_modules, build artifacts, tool caches, Docker), `WillieCubed/disk-janitor` (git-worktree build artifact sweeper, macOS+Linux), `180024421/disk-janitor` (Windows cleaner), `lucasAlmeidaSilveira/Disk-janitor` (Electron macOS cleaner, tiers+preview+Lixeira), `popomore/janitor.sh` (threshold-based tmp cleaner) | **name is taken in every direction** — four live products on the exact name across 4 niches |
| npm `disk-janitor` | 14,670 total hits, no exact `disk-janitor` package; janitor-adjacent names exist | npm safe, but npm is the wrong registry for a desktop app |
| App Store / trademark | NOT CHECKED (search infra down 2026-09-01) | OPEN — must check Mac App Store + USPTO/CNIPA before shipping |
| 还我磁盘 (CN) | NOT CHECKED — CNIPA + App Store CN | OPEN — CN primary name; collision check required (likely low risk: phrase-heavy) |

**Assessment**: the English name is a commodity. `DiskJanitor` as-is reads as a generic
utility label; 4 live competing products already own the semantic space. This kills the
differentiator we *need* on camera — undo/evidence — which no competitor's name expresses.

### 2. What successful disk-tool names do (from note 13 sources, verified)

| Tool | Name strategy | Takeaway |
|---|---|---|
| CleanMyMac | brand promise (clean = trusted) + platform suffix | own a *verb promise*, not a noun |
| DaisyDisk | friendly non-generic coinage; no "cleaner" in name | distinct coinage survives category crowding |
| DevCleaner | audience + function | functional is fine when audience is narrower |
| Pearcleaner | whimsical coinage + function | coinage lowers collision risk |
| Stats / Sensei | single evocative word | one-word brands are taken/expensive |

### 3. Recommended directions (pick 1 in the owner decision sheet)

| Direction | Name candidates (CN / EN) | Visual identity | Risk |
|---|---|---|---|
| A. Keep + qualify | 还我磁盘 Disk Janitor — "evidence-backed" tagline | current broom/janitor glyph; add a "receipt/ledger" motif to the glyph | existing-name confusion; weak differentiation |
| B. Coinage on undo promise | 还我磁盘 (keep CN!) + EN coinage e.g. **Giveback / Reclaim / 归还** — "还我" is the brand, EN is a descriptor | hero motif = restore arrow (↩) + byte counter; the undo moment is the logo | CN name stays the promise; EN loses search equity |
| C. Evidence-led | EN: **Receipt / Proofread / Ledger-adjacent**; CN stays 还我磁盘 | motif = checkmark-in-ledger / evidence chips as brand element | functional, memorable, but strays from "cleaner" category cues |

Recommendation: **B** — the existing CN name already *is* the undo promise (还我 = "give it
back"), and the product's own differentiator (note 13 §G1: undo moment on camera) needs a
visual identity that says restore, not scrub. Keep 还我磁盘 primary (D-02), pick an EN
descriptor that carries the promise.

### 4. Light availability check (2026-09-01, live fetches)

- `giveback` / `reclaim` / `return` as repo names: not checked (search down) — OPEN.
- npm: no exact `disk-janitor`; `giveback` unlikely critical for desktop shipping.

## Part 2 — Course-audit deliverables

### 5. Graded deliverables inventory (deadline 9/10 presentation)

Sources: SPEC Part A §5 (R1–R6 mapping), AGENTS.md (course deliverables section), README
(provenance note), SPEC Part A §History + "Where we are" table (2026-08-31).

| # | Deliverable | Required content | Status 2026-09-01 | Evidence path |
|---|---|---|---|---|
| D1 | AI conversation logs | this session + all agent sessions, timestamps, no fabricated history | WIP (auto-captured per harness) | session exports; README provenance note |
| D2 | 开销明细表 cost table | per-call token usage × config prices, budget enforcement | NOT STARTED (R6 work, SPEC Part B §3.7 agent + ADR-9) | ledger `budget` records (SPEC C §3.7 #777 row) |
| D3 | R1–R6 mapping verification | every feature → its R-number, acceptance-criteria trace | Draft in SPEC Part A §5; **verify at M1+** | SPEC §5 table + test suite mapping |
| D4 | Design doc (SPEC Parts A–D) in sync | one source of truth; code ≠ doc ⇒ same-change update | Consolidation done 08-31; **re-sync after each milestone** | docs/SPEC.md |
| D5 | Demo video script | 9/6 demo, note 13 §F/G structure (60–90 s hero, undo climax) | NOT STARTED (M5; note 13 gives structure) | script doc + Screen Studio render |
| D6 | License / provenance statement | every borrowed line/pattern attributed; dependency license report | Partial: SPEC Part D §9-references table + license posture table (2026-08-30) | SPEC Part B §9, Part D tables |
| D7 | Dependency license report | full third-party crate/package licenses | Partial: Part D tables carry license+verdict per crate (notes 01–13 verified) | SPEC Part D §1a–1g |
| D8 | Secret hygiene check | no .env/keys in history | ✅ verified 2026-09-01: only `.env.example` (placeholders) in git; no secret-like paths in `git log --all` | this session's audit |
| D9 | Presentation (9/10) | slide deck + talking points | NOT STARTED (M5) | deck |
| D10 | 功能完整性 trace (30 pts) | R1–R6 each demoable | M1 not started (scan+classify awaits owner go) | milestones §6 |

### 6. Gaps & recommended checklist file

Gaps: D2 (cost table has no data — needs agent-loop usage capture, R6), D5/D9 (timeline
work), D3 (verification only after M1), D6/D7 (mostly done, re-verify at each milestone).

Proposed `docs/deliverables-checklist.md` structure (create at M1; NOT created here):

```markdown
# Deliverables Checklist (course, 9/10)
| Item | Owner | Status | Evidence path | Due |
| D1 AI logs | Arys | … | … | ongoing |
… (rows from §5 above) …
## Rules
- Status values: ⬜ planned / 🔄 active / ✅ done / ⚠ blocked
- Every row's Evidence path must point at a real file or export command
- Re-run secret hygiene check (D8) before every push
```

## Reusable code & assets

| Repo/Package | License (verified) | What to borrow | Where |
|---|---|---|---|
| (none) | — | Name/brand analysis is market data, not code; collision data fetched live from GitHub API + npm registry | brand note |

## Recommended stack additions

None.

## Risks & open questions

1. **Trademark/App Store checks OPEN** — Mac App Store, USPTO, CNIPA, GitHub org-name:
   must run before the 9/6 demo assets commit. Blocking for brand finalization.
2. **CN name risk low but unverified** — 还我磁盘 is phrase-like; likely clear, needs CNIPA
   lookup.
3. **D2 cost table dependency** — nothing to fill until agent-loop usage capture lands (M4);
   schedule data collection before 9/10, not after.
4. **Script + deck lead time** — note 13 gives structure; a 60–90 s hero render needs a
   real app; earliest honest render is post-M3.

## Sources

Live fetches 2026-09-01: GitHub REST search API (`q="disk janitor" in:name`, HTTP 200, 21
repos; top-5 full_name+description shown in §1), npm registry search (`text=disk-janitor`,
14,670 hits, no exact package). Corpus refs (verified): SPEC Part A §5/§History/§Where-we-are,
AGENTS.md course-deliverables, README provenance, notes 13 (brand/presentation §A–§G), 02
(competitor landscape). [INFERENCE] tags: App-Store/trademark status is explicitly OPEN, not
assumed clear; naming recommendations are judgment from note-13 data, flagged as such.
`git log --all` secret scan executed this session (D8 ✅).