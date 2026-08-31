# Decision Register — 2026-08-31 grill session (owner-confirmed)

Format: ID | decision | owner answer (verbatim or paraphrased) | docs to amend

## Round 1 (safety, identity, UX direction)
D-01 agent_authority | Agent is PROPOSE-ONLY: returns plan ID, human confirms in UI; LLM never executes quarantine moves | DESIGN §3.7 tools table + ADR-7 + new principle P2
D-02 ui_language | BILINGUAL labels, Chinese primary + English secondary | DESIGN §3.8-replacement (GUI spec) + new ADR-13 (i18n)
D-03 error_voice | 拒绝+证据+替代: every refusal = verdict + evidence + safe alternative | new principle P1 + DESIGN §5 error catalog note
D-04 ui_paradigm | (see D-13) | —
D-05 first_run | Dashboard + hint line (no wizard state machine) | GUI spec §first-run
D-06 telemetry | ZERO telemetry, stated explicitly in README + DESIGN | SPEC §8/principles + README
D-07 product_license | No LICENSE file until after course grading (course owns submission) | README note; do NOT create LICENSE
D-08 cli_scripting | GUI-primary; thin CLI subset kept (scan --json / plan / restore / purge-interactive) for CI + agent plumbing; purge NEVER gets --yes | ADR-14 (CLI surface) + DESIGN §3.8-replacement

## Round 2 (paradigm pivot)
D-09 r2_correction | COURSE R2 ACTUALLY ALLOWS: Web / CLI 交互式终端 / 桌面 App / 手机 App (any one). Previous "ratatui required" was a repo-doc choice, NOT course mandate. Agent's earlier "R2 conflict" claim was wrong and retracted. | SPEC §5 R2 row rewrite
D-10 paradigm | GUI-FIRST desktop App (owner explicit: "Apple native feel, frictionless, usable by people who barely know what it is", "not going to be a CLI", "not just for a class — want a real product") | SPEC §1/§4/§5 + DESIGN §3.8 rewrite + ADR-5 amendment
D-11 gui_stack | Tauri + Web UI (owner confirmed recommendation). Rust core unchanged (R1). Research: docs/research/12-tauri-gui-stack.md — Tauri 2.11.5, Svelte 5.57, svelte-i18n, window-vibrancy 0.8.0, ipc::Channel for progress; packaging per OS in note 12 | ADR-5 amendment → "GUI stack: Tauri 2.11.5 + Svelte 5.57" + crate-map §1g
D-12 ui_language_detail | Bilingual = CN primary, EN secondary (owner picked option "Bilingual labels", round-2 confirmation question defaulted to CN-primary when picker cancelled) | ADR-13

## Round 3 (NFRs, operations, course artifacts)
D-13 perf_budget | Scan ≥1 GiB/s metadata-only on M1 Pro demo Mac; scan home dir <2 min; bench test in CI | DESIGN §7 + NFR table in DESIGN §1
D-14 memory_ceiling | <500 MB RSS with 1M synthetic entries fixture assertion | DESIGN §7 NFR
D-15 interrupt_semantics | scan/classify: interrupt = drop incomplete batches (fine); apply: WAL auto-resume next launch; purge: runs to completion once confirmed (no mid-purge stop) | DESIGN §5 + R4 acceptance + ADR-9-adjacent note
D-16 distribution | CI-built signed/notarized installers per OS (macOS dmg needs Developer ID $99/yr for notarization — else ad-hoc + right-click-open instructions; Windows msi/nsis signed; Linux AppImage) | ADR-14/15 (packaging) + DESIGN §8 M-note + note 12 packaging section
D-17 uninstall | Uninstall = restore-or-explicitly-confirm every surviving quarantine BEFORE removing state (brand invariant: 恢复是权利) | DESIGN §5 + new ADR-16 (uninstall contract) + GUI spec settings screen
D-18 format_versioning | Owner: "not just for a class — real product." Decision: product-grade policy = forward-compat replay (unknown kinds/fields skipped + counted, serde deny_unknown_fields OFF) + v1 frozen until post-course; migrations allowed only with version-gated migration passes after 9/10 | ADR-17 (data format versioning) + DESIGN §3.5 note
D-19 ai_log_workflow | Per-milestone export to docs/ai-logs/M<N>-<date>.md (excerpts + timestamps, API keys/privacy redacted) | new docs/ai-logs/ convention + README note
D-20 expense_sheet | Standard columns: date / model / calls / prompt+completion tokens / cost USD + CNY; ledger budget rec gains model field; janitor expense-report subcommand; scope = LLM API spend | DESIGN §3.5 ledger rec payload + ADR-9 note
D-21 demo_dataset | (pending owner after competitor-presentation research — note 13) demo will be PRODUCT VIDEO (Screen Studio/Hyperframes) + live preview; dataset decision deferred to video-script step | DESIGN §8 M5 note
D-22 demo_llm | Product video + live preview after (owner explicit). Fallback chain: live DeepSeek → pre-recorded trajectory replay | DESIGN §8 M5 note

## Extracted design principles (from audit, owner-endorsed by round choices)
P1 拒绝+证据+替代 — refuse with evidence, offer the safe alternative (D-03)
P2 LLM 建议代码决策 — the LLM advises; local code decides (D-01)
P3 离线是基线 — offline is the baseline; network is an enhancement
P4 不可逆无旁路 — no bypass for irreversible steps; purge has no --yes ever (D-08)
P5 台账即审计 — ledger is append-only, user-inspectable, never silently deleted (D-18 retention posture)
P6 诚实降级 — degrade loudly, never fake success (platform gaps, refusals, unreachable endpoints)
P7 演示可复现 — every public claim is generated from the ledger or a committed fixture, never hand-typed

## Not decided (explicitly deferred, do NOT decide unilaterally)
X-01 product LICENSE file — post-course (D-07)
X-02 demo dataset + video script — needs note 13 + owner review
X-03 presentation (9/10) structure — M5
