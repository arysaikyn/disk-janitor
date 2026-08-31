# Disk-Tool Product Presentation — Research Note

How disk-cleanup tools present themselves as **products** — landing structure, hero claims,
demo videos, safety marketing, pricing — as inputs for Disk Janitor's product video and
landing positioning (GUI-first Tauri app; bilingual CN-primary/EN-secondary; zero telemetry;
agent is propose-only). Researched 2026-08-31. Versions verified at source on that date.

## Findings

### A. Positioning map — the 10-second pitch each tool leads with

| Tool | 10-second pitch (as led with, official site) | Visual language | Pricing model | What their demo shows first |
|---|---|---|---|---|
| CleanMyMac 5.5.7 (MacPaw) | "Smart & effortless Mac care" — clean, maintain, protect; "trusted by millions" [1] | One big button ("Five routines. One Smart Care."); animated clouds/flowers; awards strip; scale counters | Subscription, "Starting at $3.33/month" [1] | One-click Smart Care sweep (overview MP4 + YouTube) [1] |
| DaisyDisk 4.34.2 | "Find it. Clean it." — "the most popular disk space **analyzer**" (avoids "cleaner") [2] | Sunburst interactive map, drag-to-Collector; dark navy; speed-comparison chart | One-time $9.99 lifetime, "not a subscription!" [3] | Scan → sunburst spin-down → drag junk into Collector (YouTube "See it in action") [2] |
| AppCleaner 3.6.8 (FreeMacSoft) | "a small application which allows you to thoroughly uninstall unwanted apps" [6] | One screenshot + one text box; drag-app-onto-window | Free + PayPal donate [6] | No video at all — page is a single screenshot |
| Pearcleaner 5.4.3 | "A free, source-available and fair-code licensed Mac app cleaner" [7] | SwiftUI list/grid + badges; Sentinel trash-watch | Free (Apache-2.0 + Commons Clause — no selling) [8] | README screenshots only; no produced video |
| Sensei (Cindori) | "The All-in-One Mac System Performance Monitoring Tool" — monitor first, clean second [9] | Dashboard/gauge screenshots, press-logo strip, PH 4.8/5 badge | $29/yr subscription OR $59 one-time (both ≤3 Macs) [10] | Status-bar monitor tour, then Clean/Uninstaller [9] |
| Stats | "macOS system monitor in your menu bar" | Menu-bar widget screenshots | Free, MIT [11] | README screenshot pair; no video |
| DevCleaner 1.9.0 | "Your Mac is hoarding **23.7 GB** of build artifacts." | Menu-bar card: per-category sizes + SAFE/WARNING/DANGER chips; live byte counter | Free; Pro $29 lifetime or $19.99/yr [16] | "From the first scan to the safe-only clean — the whole idea in twenty-two seconds" [16] |
| Screen Studio | "Professional screen recorder for macOS — professional animations by default" [20] | Auto-zoom, smooth-cursor recordings (the *style* to imitate) | One-time license ≈$89/1 Mac (mirror page; verify live) [26] | Auto-zoomed recording of the app itself |

### B. CleanMyMac (MacPaw) — the incumbent playbook and its failure modes

- **Landing structure**: hero claim → trust-badge strip ("29 million downloads · Notarized
  by Apple · Red Dot 2021 · UX Design Awards · Featured in 50+ tech blogs") → one-button
  "Smart Care" (five routines in one) → feature triads (clean / protect / performance) →
  scale-number social proof ("35M GB of junk cleaned monthly · 329K threats removed in one
  month · 5.5 GB of junk cleaned on the first scan") → testimonials → pricing footer [1].
- **Fear/scale framing is on the page itself**: "Neutralize threats before they do any
  harm", monthly threat counters, expectation-setting ("5.5 GB on your first scan") [1].
- **Criticism is consistent across independent sources**: Macworld found "marketing hype
  not always backed by engineering results" [21]; a 2026 review documents free-trial
  scare tactics, "misleading one-time purchase wording" (one-time ≠ next major version),
  and confusing cancellation [22]; Apple Community threads show durable user distrust of
  cleaner apps ("fake alerts", "not needed on macOS") [23].
- **Trust-mechanics lesson**: "Notarized by Apple" is an automated malware scan, **not**
  Apple endorsement — community repeatedly calls this out [23]. Disk Janitor must say
  "Developer ID signed + notarized" (like DevCleaner does [16]) and never imply endorsement.
- **Pricing framing**: subscription monthly-equivalent on the landing ($3.33/mo); version
  5.5.7, 22 Jul 2026 [1]. Subscription fatigue is measurable: DevCleaner's worst in-app
  review is literally "I really don't like the subscription thing" — and they added a
  lifetime tier in response [16].

### C. DaisyDisk — beauty + speed + honesty as positioning

- **Hero**: "Find it. Clean it." — deliberately positions as *analyzer*, not cleaner, so
  the safety burden shifts to the user ("the user makes the informed decision") [2][4].
- **The famous interactive preview**: sunburst map + drag-to-Collector. Their site's
  demo is one YouTube video ("See it in action") embedded in a MacBook mockup [2].
- **Speed as a marketing chart**: "macOS Finder 6 min · average 3 min · fastest 1 min ·
  DaisyDisk 15 sec"; specs page claims 6–32× faster scanning, APFS SSD 12–20 s [2][4].
- **Stated safety model (specs page)**: "Safety stoppers prevent the user from accidentally
  deleting system-critical components. **No automated 'cleaning' is performed**"; privacy:
  scans only metadata (name+size), no analytics, nothing leaves the machine [4]. Their
  guide concedes the judgment gap: "only delete the files that you can recognize… make
  sure your Time Machine is always up and running" — i.e. rollback is outsourced to the
  user's backup, not the product [5].
- **Pricing page copy worth copying**: "one-time purchase, and not a subscription!"
  lifetime license, 5 personal Macs, 30-day money-back, FastSpring checkout [3].

### D. Open-source / indie positioning (Pearcleaner, AppCleaner, Stats)

- **AppCleaner** is the floor of the market: one screenshot, one paragraph, versioned
  downloads for old macOS (3.6.8 "for Mojave up to Tahoe" as of 2026-08) [6]. No claims
  about judgment, no undo, no video. Proves a free tool can win on simplicity alone —
  and leaves the entire *trust* layer unowned.
- **Pearcleaner** leads with license-as-positioning: "free, source-available and
  fair-code licensed… with privacy in mind" [7]. License is Apache-2.0 **with Commons
  Clause** (selling prohibited) — copy-with-care for our university repo; do not vendor
  code from it [8]. Notable ops lesson: its README warns that pearcleaner.com is a fake
  scam site pushing malware — a real risk for any popular cleaner, and a reason our
  official domain/download must be singular and verifiable [7]. Development on hold since
  end of 2025 (no Mac access) [7].
- **Stats** (41.5k stars, MIT) wins trust by stating exactly which network calls it makes
  (update check + own IP API) and "does not collect any telemetry or analytics" [11].
  Zero-telemetry products can still update-check: publishing the exact endpoints is the
  trust pattern. It also documents a macOS 26 privacy gate (System Settings → Menu Bar)
  that silently hides menu-bar items — a demo-day failure mode to pre-empt [11].

### E. AI-era entrants (2025–2026) — what the new wave leads with

- **DevCleaner** (menu bar, 23 toolchains incl. Claude/Cursor/Ollama caches): leads with
  a *specific number on the hero* ("Your Mac is hoarding 23.7 GB of build artifacts"),
  then safety-model-as-UI: SAFE (deleted by default: DerivedData, Gradle, pip) / WARNING
  (your call: old simulators, .xcarchives, AVDs — "never pre-selected") / DANGER (hands
  off: Android SDK, iOS Device Support — "shows you their size, asks twice, never touches
  them on its own") [16]. Product Hunt: #12 on launch day; reception praised
  "developer-explained information on what you're deleting"; one-star review punished the
  subscription [16][17].
- **FilesMagicAI** (Jun 2026 PH): AI agent organizer + cleaner, on-device renaming, free
  cleaning tier — 11 upvotes, #44 daily; no feature. Modest reception without a hook [18].
- **Drezze** (Jul 2026 PH): "private, local-first… moves approved items to Trash so
  they're recoverable… no accounts, ads, telemetry"; personal files never preselected [19].
- **PoofMac**: local LLM (Ollama, 14–35B models) classifies what is junk; local-first
  pitch [24]. **DevBox Cleaner**: treemap + quick actions [25]. **DuckDisk**: table-first
  "WizTree-style" analysis — the counter-trend to pretty sunbursts [27].
- **Pattern**: every trusted 2026 entrant leads with *privacy + recoverability*, not AI.
  AI is a feature inside the product, never the hero claim. "Trash = recoverable" is the
  dominant trust metaphor (weakest form of our quarantine+ledger design).

### F. Product-video conventions (Screen Studio style + dev tools)

- **Length budgets** (2026 consensus across demo-video guides): homepage/top-of-funnel
  hero video 60–90 s; pricing/mid-funnel 2–4 min; feature deep-dive 4–8 min; social cuts
  15–60 s. ~90 s is the hard ceiling for a cold audience; the demo section is ~60% of
  runtime [28][29][30].
- **First 5 seconds decide**: viewers drop at 15–20 s if it opens with a logo or feature
  list; open with the *problem dramatized* (disk-full dialog, Xcode 100 GB cache), one
  problem, one outcome [28][30][31]. DevCleaner's 22-second launch clip is the minimal
  viable shape: scan → risk chips → one-click clean → bytes-freed counter [16].
- **Screen Studio conventions** (what dev-tool videos now look like): auto-zoom to cursor
  actions, smooth cursor, soft window background, dark/light-matched backdrop — polish
  comes from the recorder, not from After Effects [20][26]. Also the accepted structure:
  problem (0–3 s) → solution reveal → 1–3 feature proofs → benefit → single CTA [29].
- **HyperFrames** (Apache-2.0, npm `hyperframes` 0.8.20, 43k stars) is the scriptable
  alternative: HTML/CSS → deterministic MP4 via headless Chrome+FFmpeg; ships a
  product-launch workflow whose review checklist is directly reusable: "Every product
  claim is current · shown states are realistic · interface remains readable · opening
  proves the product quickly · ending asks for one clear action" [12][13][14]. Captures
  the *real* app, not mockups ("the screens are captured from huly.io, not mocked —
  which is the point") [14]. Remotion 4.0.519 is the React alternative but its license is
  custom (company-size gated) — flagged copy-with-care [15].

### G. Extraction — what a trustworthy-safety-first product video MUST show

1. **The undo moment is the climax, not a footnote.** No competitor owns it on video:
   DaisyDisk outsources rollback to Time Machine [5]; DevCleaner's video ends at bytes
   freed [16]; Drezze's "recoverable via Trash" is text, not shown [19]. Our 72 h restore
   (quarantine move → restore) is the differentiator — show a purge, then *restore the
   folder back to its original path* on camera.
2. **Zero-bytes-untouched framing**: show "apply" freeing **0 bytes** and the ledger
   recording the move; then the typed byte-count confirm as purge. The ledger is visual
   proof no data was destroyed before the user typed the number.
3. **Evidence, not verdicts**: click one file → its Evidence panel (lockfile present;
   sibling package.json absent; mtime < 10 min ⇒ refuse). The video must show a file the
   tool *refuses* to act on — the refusal IS the trust demo (cf. DevCleaner's "never
   pre-selected" WARNING tier [16], Sensei testimonial "unlike other apps that
   automatically delete system files" [9]).
4. **Show the agent as propose-only**: agent proposes a plan manifest → human approves.
   Never show auto-execution; it maps to the fear every review thread expresses [21][23].
5. **Specific numbers on the hero** (DevCleaner's "23.7 GB" pattern [16]) — for us:
   "19.3 GB recoverable · 14.2 GB auto-safe · 5.1 GB needs your call · 0 files destroyed
   until you confirm".
6. **Privacy stated on screen**: "no telemetry · no account · your files never leave the
   Mac · the LLM never sees file names" — mirrors Stats' endpoint disclosure [11] and
   DaisyDisk's metadata-only promise [4].
7. **First 5 s = the pain**, not the logo: disk-full warning or a sunburst-equivalent
   (our treemap) filling with red "hands-off" sectors [28][30].
8. **Structure to shoot (90 s)**: 0–5 s pain → 5–15 s scan + live counters → 15–45 s
   evidence review + refusal moment → 45–60 s quarantine (0 bytes freed) → 60–72 s typed
   purge → 72–85 s **restore moment** → 85–90 s one CTA (download). Cut a 22 s social
   version (DevCleaner shape) and a 60 s homepage hero.

### H. Patterns to avoid (fear-based marketing and trust traps)

- Threat counters and fear-of-invisible-harm framing ("329K threats removed") [1] —
  invites the "scareware in pink" critique [21][22][23].
- RAM "optimization"/"boost" claims on Apple Silicon — repeatedly called snake oil
  [22][23]; we clean bytes, never claim speed miracles.
- "Notarized by Apple" as endorsement shorthand [1][23] — say "signed + notarized" only.
- Subscription-first pricing on the landing ($3.33/mo) [1]; one-time ≠ current-version
  -only sneaky renewals [22]. If we ever charge: DaisyDisk-style lifetime copy [3] or
  DevCleaner's dual free/lifetime [16].
- Pre-selected deletions of anything not provably regenerable (the exact inverse of our
  policy engine; DANGER tier must never be pre-selected [16]).
- Fake download sites: register/hold our official domain early (Pearcleaner precedent) [7].

### I. How an evidence-driven tool can *visually demonstrate judgment*

- **Risk-tier color language is now table stakes** (SAFE/WARNING/DANGER chips, DevCleaner
  [16]); ours maps to the 5 rebuild-cost grades — show grade as a colored ring/badge on
  every row, with HandsOff visually "locked".
- **Evidence-on-demand**: DaisyDisk shows *where* space is (sunburst) but never *why
  something is junk* [2][4][5]; DevCleaner annotates categories with one-line reasons
  ("regenerate automatically on your next build… costs you one slower compile") [16].
  The next level — per-file machine-derived evidence (lockfile, mtime, sibling analysis)
  — is unowned in any competitor's UI and is our video's center shot.
- **Show the counterfactual cost**: "delete DerivedData → next build +40 s" is the
  DevCleaner reason-string pattern [16]; our grades can carry the same cost line
  (RebuildableFromSource ⇒ "rebuild via lockfile", Expensive ⇒ "hours to regenerate").
- **Refusal as UI**: mtime < 10 min ⇒ row shows "actively written — refused" with the
  timestamp as evidence. Showing a refusal is more persuasive than any badge.

## Platform delta vs macOS (GUI pivot note)

- All positioning above is macOS-source; the *claims* (quarantine, ledger, restore)
  carry to Windows/Linux only as far as the primitives note (11-cross-platform-
  primitives.md) allows. Windows rename-by-handle and Linux `renameat2` give the same
  atomic-move story; the video/landing claims must be gated per-OS exactly like the
  implementation (state the guarantee the platform actually provides — weaker Windows
  guarantee → show the weaker line in the video, or scope the claim to macOS).
- Bilingual CN-primary landing: no major competitor ships real Simplified-Chinese
  product copy (CleanMyMac/DaisyDisk localize the app, their sites are EN-first);
  原生中文官网 is an open lane. Sensei/Stats community-translate the app only [9][11].
- Themes: dark+light following system appearance is table stakes (DaisyDisk dark navy
  site, Sensei dark hero); demo video should show both themes switching (macOS 26
  appearance APIs), and the menu-bar privacy gate (Stats issue [11]) applies to our
  macOS build's demo environment.

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| heygen-com/hyperframes (v0.8.20 npm) | Apache-2.0 [12][13] | Deterministic HTML→MP4 pipeline; product-launch video checklist; caption/catalog blocks for the CN/EN dual-cut video | `docs/`-adjacent tooling, not app code |
| exelban/stats | MIT [11] | Zero-telemetry disclosure wording; menu-bar behavior notes | landing copy, policy docs |
| alienator88/Pearcleaner | Apache-2.0 + Commons Clause [8] | **Positioning only** ("free, source-available, privacy in mind"); no code (no-sell clause) | none |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| (none needed for this lane) | Product presentation is not an app dependency | Video/landing work happens outside the Rust core | — |

## Recommended packages

| Package | Version (2026-08-31, npm registry) | Purpose | License | Why / notes |
|---|---|---|---|---|
| hyperframes | 0.8.20 [13] | Scriptable HTML→MP4 product video (deterministic; CI-able; agent skills included) | Apache-2.0 | Preferred for CN/EN dual-cut videos from one source; captures the real app via URL/screens |
| remotion | 4.0.519 [15] | React-based programmatic video | Custom ("SEE LICENSE IN LICENSE.md") — company-size gating; copy-with-care | Only if React ergonomics preferred; license review required before any commercial use |

(Note: Screen Studio is a closed-source macOS app, ≈$89 one-time per mirror page —
buy/verify at screen.studio before citing price publicly [20][26].)

## Risks & open questions

- Screen Studio's exact current price could not be loaded at source (screen.studio
  unreachable from this network during research); mirror page says $89/1-device one-time
  — verify before purchase [20][26].
- "Notarized by Apple" boundaries: fine to state notarization, never imply endorsement
  [23]; wording needs a legal-ish review for both EN and CN copy.
- Subscription-vs-lifetime is a product decision, not a research one; evidence here says
  lifetime/one-time reads as trust in this market (DaisyDisk [3], DevCleaner lifetime
  tier added after subscription backlash [16]).
- Fake-site/scam clone risk applies to any popular cleaner (Pearcleaner precedent [7]) —
  domain strategy is unsolved.
- Do the weaker Windows/Linux guarantees (vs macOS `renamex_np(RENAME_EXCL)`) change the
  video's undo-moment claim per-platform, or do we scope the claim to macOS in v1?
  Depends on lane 11's final primitive matrix.
- Bilingual landing: CN-primary claims need CN-native legal/trust phrasing for
  "restore within 72 h" and "notarized" — not a translation of the EN legal line.

## Sources

1. CleanMyMac landing page — https://macpaw.com/cleanmymac (accessed 2026-08-31)
2. DaisyDisk home (hero, demo video, speed chart, media quotes) — https://daisydiskapp.com/ (accessed 2026-08-31)
3. DaisyDisk pricing & licensing — https://daisydiskapp.com/support/pricing (accessed 2026-08-31)
4. DaisyDisk tech specs (safety, privacy, hidden space) — https://daisydiskapp.com/specs/ (accessed 2026-08-31)
5. DaisyDisk guide: What is safe to delete? — https://daisydiskapp.com/guide/what-to-delete (accessed 2026-08-31)
6. AppCleaner (FreeMacSoft) — https://freemacsoft.net/appcleaner/ (accessed 2026-08-31)
7. Pearcleaner README (status, positioning, fake-site PSA) — https://github.com/alienator88/Pearcleaner (accessed 2026-08-31)
8. Pearcleaner LICENSE.md (Apache-2.0 + Commons Clause) — https://github.com/alienator88/Pearcleaner/blob/main/LICENSE.md (accessed 2026-08-31)
9. Sensei product page (Cindori) — https://cindori.com/sensei (accessed 2026-08-31)
10. Sensei store (pricing tiers) — https://cindori.com/store/sensei (accessed 2026-08-31)
11. Stats README (telemetry disclosure, macOS 26 menu-bar gate, license) — https://github.com/exelban/stats (accessed 2026-08-31)
12. HyperFrames repo (Apache-2.0, skills, CLI) — https://github.com/heygen-com/hyperframes (accessed 2026-08-31)
13. npm registry: hyperframes 0.8.20, license Apache-2.0 — https://registry.npmjs.org/hyperframes/latest (accessed 2026-08-31, API with UA)
14. HyperFrames product-launch video guide — https://github.com/heygen-com/hyperframes/blob/68205dbb/docs/guides/product-launch-video.mdx (accessed 2026-08-31)
15. npm registry: remotion 4.0.519, custom license — https://registry.npmjs.org/remotion/latest (accessed 2026-08-31, API with UA)
16. DevCleaner official site (hero number, risk model, pricing, FAQ, 22-s demo) — https://devcleaner.app/ (accessed 2026-08-31)
17. DevCleaner Product Hunt launch — https://www.producthunt.com/products/devcleaner-reclaim-gbs-from-dev-tools (accessed 2026-08-31 via search summary)
18. FilesMagicAI PH overview — https://hunted.space/product/files-magic-ai (accessed 2026-08-31 via search summary)
19. Drezze Disk Cleaner PH dashboard — https://www.hunted.space/dashboard/drezze-disk-cleaner (accessed 2026-08-31 via search summary)
20. Screen Studio (positioning, features) — https://screen.studio/ (per search snapshot; site unreachable directly 2026-08-31)
21. Macworld: CleanMyMac virus-removal review — https://www.macworld.com/article/2560649/cleanmymac-offers-virus-removal-but-is-it-any-good.html (accessed 2026-08-31 via search summary)
22. MacPaw Review 2026 (trial scare tactics, renewal wording) — https://couponsscout.com/reviews/macpaw-review-scam-truth/ (accessed 2026-08-31 via search summary)
23. Apple Community / MacRumors threads on cleaner distrust — https://discussions.apple.com/thread/255249088 ; https://forums.macrumors.com/threads/is-cleanmymac-actually-legit-or-snake-oil.2476091/ (accessed 2026-08-31 via search summary)
24. PoofMac (local-LLM cleaner) — https://github.com/lesteroliver911/poofmac (accessed 2026-08-31 via search summary)
25. DevBox Cleaner PH — https://webmail.hunted.space/dashboard/devbox-cleaner-for-macos (accessed 2026-08-31 via search summary)
26. Screen Studio pricing mirror (unofficial; verify) — https://screen-studio.framer.website/ (accessed 2026-08-31 via search summary)
27. DuckDisk PH (table-first counter-trend) — https://www.producthunt.com/products/duckdisk (accessed 2026-08-31 via search summary)
28. Demo video length data 2026 (Rimo) — https://www.rimodreamlabs.ai/blog/how-long-should-a-product-demo-video-be (accessed 2026-08-31 via search summary)
29. Demo script template (DemoPolish) — https://demopolish.com/blog/how-to-write-demo-video-script/ (accessed 2026-08-31 via search summary)
30. Drop-off / 8-second rule (DEV + Versely) — https://dev.to/cvchelles/the-8-second-rule-why-most-saas-demos-lose-viewers-instantly-2elg ; https://www.versely.studio/blog/product-demos-that-dont-bore (accessed 2026-08-31 via search summary)
31. Launch video length 2026 (Clickstrike) — https://clickstrike.com/blog/product-launch-video-length/ (accessed 2026-08-31 via search summary)
