# 9.13 提交清单 — Final Submission Checklist

Course: 程序设计训练（Rust 语言）(30240522-3), THU CS.
**Deadline: 2026-09-13 23:59** — every item below must ship. Source of truth for the
作业要求 items: `docs/SPEC.md` Part A §6 (milestones) and the rubric R1–R6 (§5).
Status is tracked here and mirrored in the README milestone table.

| # | Item (from 作业要求) | Status | Evidence / pointer |
|---|---|---|---|
| 1 | **设计文档 PDF** — 4 部分: 痛点分析 / 场景定制 ≥2 / 架构图 / 技术选型 | ⬜ export from SPEC → PDF | 痛点分析: SPEC Part A §2 (why existing tools fail) + `docs/research/` notes [01–33] · 场景定制: SPEC Part A §3 (≥2 scenarios, course mandate) · 架构图: SPEC Part B §1–§2 (30-second architecture; crate layout; ASYNC Mermaid/图待导出) · 技术选型: SPEC Part B §4 + Part D (dua-core 4.0.0 pin, Tauri 2.11.5 + Svelte 5, renamex_np, JSONL WAL) + research notes [22–33], each ending in a `## Port backlog` naming the borrowed mechanism and its destination symbol |
| 2 | **源码 → 清华 Git + README** | 🔄 README done — **pending owner push** to the Tsinghua Git server (`lab.cs.tsinghua.edu.cn` course repo) | [`README.md`](../README.md) (brand 还我磁盘 Giveback, quickstart, gates, milestone table); repo branch `agent/repo-scaffold`, 122 tests green (132 with `--features llm`); **no git remote configured yet** |
| 3 | **AI 对话历史原始记录** | 🔄 excerpts landed per milestone — raw transcripts live in harness session history | [`docs/ai-logs/M1.md`](ai-logs/M1.md) … [`M5.md`](ai-logs/M5.md) (per-milestone: date range, key decisions, failures→fixes, test counts; 摘录 excerpts, privacy-redacted) |
| 4 | **开销明细表 (Excel, 按阶段: 人时/calls/tokens/花费/模型/工具)** | ✅ **exported — Excel format shipped** | [`docs/expense-sheet.xlsx`](expense-sheet.xlsx) (the graded Excel deliverable: 分阶段 / 逐日 / 工具 / 方法 sheets) + [`docs/expense-sheet.md`](expense-sheet.md) (same tables, readable). Both emitted by `scripts/gen-expense-xlsx.py` from the mined omp session transcripts — generated, never hand-typed, so md and xlsx cannot drift |

## How to close each item

1. **设计文档 PDF**: assemble the four sections from the SPEC pointers above into one
   document, export to PDF, commit under `docs/` (or a `submission/` folder) and
   reference it here.
2. **源码**: owner pushes `agent/repo-scaffold` to the 清华 Git course repo; confirm
   README renders, tests pass (`cargo fmt --check && cargo clippy -D warnings && cargo test`).
3. **AI logs**: each `docs/ai-logs/M*.md` already summarizes its milestone; if graders
   require raw transcripts, export the full harness session history and archive it
   (pointer location TBD — keep the per-milestone files as the readable index).
4. **开销明细表**: done. `python3 scripts/gen-expense-xlsx.py` regenerates both
   `docs/expense-sheet.xlsx` and the marked block in `docs/expense-sheet.md` from the
   session transcripts — re-run it on 09-13 if more sessions land before submission, and
   commit the regenerated pair (`docs: re-mine 开销明细表`). Never hand-edit either
   artifact: the drift that made this table wrong the first time came from hand edits.

## Participation milestones

- ✅ 8.30 选题帖 · ✅ 9.1 ≥3 选题评论 (vault 09-01: "选题 posted late + ≥3 peer comments done")
- ⚠️ **9.6 公开展示帖 (设计摘要 + 项目链接)** — **status UNKNOWN; the repo cannot close this.**
  `~/vault/05-Journal/synthesis/night-think/2026-W37.md:20` records the 09-06 status as
  UNKNOWN; `~/vault/Courses/Rust/Agent-Project.md:24` carries the 09-06 row with no ✅; a
  vault-wide search for `公开展示|展示帖` returns only calendar/plan rows — **no artifact,
  screenshot, or receipt exists**. Needs Arys to confirm the post is live on the course forum.
- ⚠️ **9.8 ≥3 作品试用反馈 (5 pts)** — deadline passed 09-08 23:59; `night-think/2026-W37.md:34`
  and `:101` show **zero vault evidence through 09-10**. Same position: no evidence either
  way. Confirm on the forum before 9.13; if not posted, accept the 5-pt loss honestly —
  do NOT record a ✅ that the vault does not support.
- ✅ 9.10 课堂展示 (5' + 2' Q&A, 25 pts) — delivered 09-10; deck committed at
  `docs/presentation/ai-experience.{html,pdf,png}`
- ⬜ **9.13 23:59 final submission** — 设计文档 PDF + 源码 push + AI logs + this sheet.
  Items 2/3/4's remaining owner actions are listed in the table above.
