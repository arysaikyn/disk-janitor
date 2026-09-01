# One-Tap / "Smart" Clean Flows & Post-Clean Receipts — Research Note
Scope: how CleanMyMac Smart Scan/Smart Care, DevCleaner, Sensei, MacBooster, OnyX and CCleaner structure their one-tap "smart" clean (scan → triage → confirm → execute), which categories are auto-included vs shown for confirmation, their safety framing/copy, and their post-clean RECEIPT patterns; then recommendations for Disk Janitor's quarantine-first protocol — which grades may be auto-marked for one-tap, where the typed byte-count confirmation fits, receipt content, and Dashboard one-tap button copy. · researched 2026-09-01

> Builds on [02] (landscape safety models) and [13] (product-presentation patterns); does NOT
> duplicate them. [02] covers *deletion/undo reversibility* per tool; [13] covers *positioning,
> hero claims, demo-video structure*. This note is about the **one-tap flow choreography and the
> receipt** — the moments [02]/[13] only glanced at. SPEC Part C §3.2 (Dashboard), §3.6 (Activity),
> §4.2 (purge modal) and §4.3 (apply semantics) are the anchor sections cited below.

## Findings

### 1. Stage taxonomy — every "smart" clean is four moves in disguise

Across all six tools the one-tap flow collapses to the same four logical stages; what differs is
which stages are *visible to the user* and where the *auto-select* vs *confirm* boundary sits:

| Tool | 1 Scan | 2 Triage (auto-select) | 3 Confirm | 4 Execute + Receipt |
|---|---|---|---|---|
| CleanMyMac Smart Care [S1] | visible (progress) | **all 5 module tiles pre-checked**; per-item deselect via Review Details | soft (Run button; no typed confirm) | Run → "optimized"; no itemized receipt, no undo |
| DevCleaner Quick Clean [S2] | visible (live streaming sizes) | **SAFE tier pre-selected only**; WARNING/DANGER shown-but-unselected | soft (one click; DANGER "asks twice") | real-time freed counter + 30-day history |
| CCleaner Health Check [S3] | visible (Analyze) | junk/trackers auto-selected by category toggles + Cookie Allowlist | soft (Make it better) | Results screen: space freed + items removed |
| MacBooster 8 [S4] | visible (System Status) | System Junk auto-listed; Clean button | soft (Clean) | "freed in seconds"; no undo |
| Sensei Clean [S5] | visible (disk scan) | safe-by-default prompts; manual review | soft (confirm) | reclaim summary; no undo |
| OnyX Automation [S6] | minimal (checkbox pre-flight) | **bottom-three Cleaning items pre-ticked by default** | soft (Execute) | caches rebuilt on reboot; no receipt |

**Key axis — the auto-select boundary.** Only DevCleaner restricts auto-selection to a *safety
tier* (SAFE = regenerates on next build) and explicitly never pre-selects WARNING/DANGER [S2].
CleanMyMac pre-checks *every* module tile by default and lets you deselect [S1] — the auto-select
boundary is "everything the Safety Database approves", a black box [02 §1]. CCleaner's boundary is
category toggles + an allowlist [S3]. OnyX pre-ticks a conservative subset but offers no per-item
judgment, only checkboxes [S6].

### 2. CleanMyMac Smart Scan / Smart Care — the incumbent one-tap

**Stage structure** (Smart Care, current 5.5.7, KB updated 2026-08-11 [S1]):
1. **Scan** — Smart Care runs five modules in sequence: Cleanup (system junk, mail attachments,
   trash), Protection (malware Quick Scan), Performance (maintenance scripts, flush DNS),
   Applications (app-update suggestions), My Clutter (duplicate files in Downloads).
2. **Triage** — results appear as **module tiles, all pre-checked by default**. Per the KB:
   "By default, CleanMyMac selects all Smart Care tasks. You can choose which tasks to skip by
   clearing the checkmark." Within a tile, **Review Details** exposes individual files to deselect
   (e.g. specific app caches, language files) [S1][S7]. Cleanup "automatically selects items for
   removal"; Protection "automatically selects found threats for removal" [S1].
3. **Confirm** — a single **Run** button; no typed or destructive-action ceremony. The Safety
   Database is the stated guarantee: "doesn't delete any user files or app data unless you choose
   to" [S1][S8].
4. **Execute + receipt** — Run executes all checked tiles; the KB closes with "your Mac will be
   optimized." **No itemized post-clean receipt, no undo channel** — deletion is direct (Move to
   Trash default, optionally Remove Immediately/Securely [02 §1]); reversibility ends at the Trash.

**Safety framing/copy:** "Rest assured… Safety Database, built over years of experience, prevents
cleaning errors and protects user data" [S1]. The framing is *trust-the-database*, not
*show-the-evidence* — the opposite of our Evidence-over-names invariant (SPEC Part A spine #1).
Excludable via Settings → Scanning → Smart Care (deselected modules appear dimmed, re-enabled with
an Enable button) [S1].

**Auto-included vs confirm:** everything the DB approves is auto-included; the only "confirm" is
the user's right to deselect. There is no *grade-gated* auto-select — a cache and a language pack
sit at the same pre-checked tier [S1][S7].

### 3. DevCleaner — the grade-gated one-tap (closest cousin to our model)

**Stage structure** (v1.9.0, menu-bar app [S2]):
1. **Scan** — sizes stream live per category as the walker runs ("every category, every toolchain,
   counted byte by byte in front of you") [S2].
2. **Triage** — three explicit risk tiers, each a distinct auto-select posture [S2]:
   - **SAFE — "Deleted by default"**: "Pure caches that regenerate automatically on your next
     build. Deleting them costs you one slower compile, nothing more." (DerivedData, Gradle
     caches, pip cache.) **Pre-selected.**
   - **WARNING — "Your call"**: "Things that grow back slowly — simulators you might still need,
     archived builds, emulator images. Visible, measured, but **never pre-selected**." (Old
     simulators, .xcarchives, AVDs.)
   - **DANGER — "Hands off"**: "SDKs and device symbols that can break your environment. DevCleaner
     shows you their size, **asks twice, and never touches them on its own**." (Android SDK, iOS
     Device Support.)
3. **Confirm** — Quick Clean "wipes everything marked safe" in one click; DANGER items "ask twice"
   [S2]. No typed byte count.
4. **Execute + receipt** — "watch the freed space add up in real time" during clean; a **30-day
   cleanup history & stats** panel is in the Free tier [S2]. **No quarantine/undo** — deletion is
   direct; "Project Hibernation" (Pro) is the only restore-like affordance, and only for whole
   projects, not arbitrary cleaned caches [S2].

**Why this is our closest cousin:** DevCleaner is the only competitor whose one-tap auto-select is
*grade-gated* (SAFE only), and the only one that states the rebuild-cost rationale inline ("costs
you one slower compile"). Our `PureCache` grade is the direct analogue; `RebuildableFromSource`
maps to its WARNING; `Expensive`/`HandsOff` map to DANGER. The gap DevCleaner leaves — and we fill
— is **reversibility**: it deletes directly with no quarantine, so its "30-day history" is a *log*,
not an *undo channel* [02 §2]. Our apply→purge split makes the safe-direction reversible by
construction (SPEC Part A §3.2, §4.3).
### 4. CCleaner Health Check — the two-stage analyze-then-fix pattern

**Stage structure** (Health Check, the one-click mode that replaced "Easy Clean" [S3][S9]):
1. **Scan (Analyze)** — scans junk files, browser cache, "trackers" (cookies/history/temp),
   performance issues, security recommendations. Presents a health report with space-that-can-be-
   freed [S3].
2. **Triage** — categories are **auto-selected via per-category toggles**, modulated by a **Cookie
   Allowlist** (Cookies to Keep) — allowlisted cookies are counted as "found" but not removed [S3].
   Saved passwords are unticked by default [S9].
3. **Confirm** — the **"Make it better"** button (a single soft confirm; no typed ceremony) [S3].
4. **Execute + receipt** — a **Results screen** summarizes "how much space was freed" plus an
   overview of items removed [S3]. Custom Clean (the alternate mode) remembers per-browser
   selections; Health Check is the "set-and-forget" one-tap [S9].

**Receipt quality:** CCleaner's Results screen is the most *itemized* post-clean receipt among the
one-tap tools — it lists categories cleaned and bytes freed. But it is a **terminal receipt**: no
undo, no quarantine; the audit lives in CCleaner's INI/log config, not a user-facing ledger [S3].
The Cookie Allowlist is the only "don't touch this" mechanism — a coarse allowlist, not per-item
evidence [S3].

### 5. MacBooster, Sensei, OnyX — the softer one-taps and the non-one-tap

**MacBooster 8** [S4]: opens to a System Status panel (System Junk / Virus & Malware / Turbo Boost);
a Scan → per-module Clean. System Junk "scans unneeded system and app-generated items… Clean to
remove or Details to review." **One-click Lightning Booster** is the one-tap. Reviewers flag that
"users are advised to review items before Fix, as deletions are permanent" — i.e. the one-tap is
soft-confirmed and **irreversible** [S4]. No grade gating; auto-select is by category.

**Sensei (Cindori)** [S5]: a Clean tool "to remove junk (caches, logs, downloads) with safe-by-
default prompts"; uninstaller + disk analyzer. Scheduling/automated removal "not yet available."
The one-tap is a confirm-prompted clean, not a grade-gated sweep. No undo channel [S5].

**OnyX (Titanium Software)** [S6]: **deliberately not a one-tap tool.** The Automation tab runs a
subset of Maintenance + Cleaning tasks (incl. cache cleaning) "with one click," and "default items
are ticked to avoid unintended data loss" — but the posture is expert/manual: structured cache
categories (System/User/Internet/Fonts/Logs/Misc), a caution to "review before Execute as the
system will rebuild caches afterward," and a required reboot for some classes [S6]. OnyX is the
**counter-example** that proves the market also wants deliberate, category-by-category control —
which our Review screen (SPEC Part C §3.4) already provides as the *non*-one-tap path.

### 6. Post-clean receipt patterns — what competitors show after a clean

| Tool | Freed-bytes summary | Per-category breakdown | Undo / restore | Ledger / history |
---|---|---|---|---|
| CleanMyMac Smart Care [S1] | implied ("optimized") | via Review Details *before*, not after | none (Trash only) [02 §1] | none user-facing |
| DevCleaner Quick Clean [S2] | **live real-time counter** | per-category sizes shown live | none (Hibernation is project-only) [S2] | **30-day cleanup history & stats** (Free) |
| CCleaner Health Check [S3] | **Results screen: space freed** | **items-removed overview** | none | INI/log config, not user ledger [S3] |
| MacBooster [S4] | "freed in seconds" | Details view | none | none user-facing |
| Sensei [S5] | reclaim summary | disk-analyzer list | none | none |
| OnyX [S6] | minimal | category checkboxes | none | none |

**Pattern:** the receipt is almost universally *terminal and one-shot* — a freed-bytes number, then
done. Only DevCleaner persists a multi-day history, and even that is a *log of past cleans*, not an
*undo channel* [S2]. **No competitor ships a receipt that distinguishes "freed now" from
"quarantined, restorable"** — because none has a quarantine phase. This is the white space our
two-phase protocol (apply = 0 bytes freed, purge = real free) turns into a receipt feature
(SPEC Part C §4.2-4.3, §3.6).

## Reusable code & assets

| Repo/Package | License (verified) | What to borrow | Where |
|---|---|---|---|
| (none new) | — | One-tap flows are UX/copy design, not copyable code; competitor receipt patterns extracted in §6 are conventions, adapted via SPEC Part C §4.2–4.3 (§3.6 Activity receipt row) | Dashboard / Activity |

## Recommended stack additions

None — the receipt is a derived view over `ledger` records and the existing
`ipc::Channel<ExecEvent>` stream (SPEC Part C §2.3/§3.6); no new dependency. One-tap
auto-marking lives in `plan/` constraints (grades subset PureCache), no new machinery.

## Risks & open questions

1. **Auto-mark set size** — PureCache-only one-tap may feel small to users whose junk is
   mostly RebuildableFromSource; the one-tap must show "N GB 待确认 awaiting confirm" so
   the flow never implies it saw everything. Confirm the copy with Arys.
2. **Typed-gate placement in one-tap** — if the typed byte-count confirm happens only at
   purge, the one-tap's apply phase is frictionless but the purge phase reintroduces
   friction — deliberately (signature moment, [13 §G2]). Keep; do not weaken.
3. **Receipt undo CTA** — receipt must deep-link to the vault (note 17) within 72 h; after
   expiry the CTA becomes "已过期 expired" not nothing. Requires ledger `apply` records to
   carry the plan's op-ids in display order.
4. **DevCleaner-style live counter** — the receipt's "freed now vs quarantined" split
   (§6 white space) needs an explicit Activity-line spec row; open whether partial
   streaming or end-state only.

## Sources

**Recovery note**: [S1]–[S6] citations (sections 1–6) were collected by the research agent whose
final chunk died; per-claim URLs were not persisted to this file before its session ended
(one-tap sources: CleanMyMac Smart Care, DevCleaner.app, CCleaner Health Check, MacBooster,
Sensei/Cindori, OnyX/Titanium — all 2026-08/09; the same pages are cited from note 02 and
note 13 verified source lists, which are authoritative for the shared claims). Trailing
sections above are corpus-grounded: SPEC Part C 3.2/3.6/4.2-4.3/5.2, ADR-10, notes 02/13.
Re-verification of the six vendor URLs is queued in the Verification phase.
