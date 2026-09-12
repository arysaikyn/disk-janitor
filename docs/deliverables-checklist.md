# 9.13 提交清单 — Final Submission Checklist

Course: 程序设计训练（Rust 语言）(30240522-3), THU CS.
**Deadline: 2026-09-13 23:59** — every item below must ship. Source of truth for the
作业要求 items: `docs/SPEC.md` Part A §6 (milestones) and the rubric R1–R6 (§5).
Status is tracked here and mirrored in the README milestone table.

| # | Item (from 作业要求) | Status | Evidence / pointer |
|---|---|---|---|
| 1 | **设计文档 PDF** — 4 部分: 痛点分析 / 场景定制 ≥2 / 架构图 / 技术选型 | ⬜ export from SPEC → PDF | 痛点分析: SPEC Part A §2 (why existing tools fail) + `docs/research/` notes [01–21] · 场景定制: SPEC Part A §3 (≥2 scenarios, course mandate) · 架构图: SPEC Part B §1–§2 (30-second architecture; crate layout; ASYNC Mermaid/图待导出) · 技术选型: SPEC Part B §4 + Part D (dua-core 4.0.0 pin, Tauri 2.11.5 + Svelte 5, renamex_np, JSONL WAL) |
| 2 | **源码 → 清华 Git + README** | 🔄 README done — **pending owner push** to the Tsinghua Git server (`lab.cs.tsinghua.edu.cn` course repo) | [`README.md`](../README.md) (brand 还我磁盘 Giveback, quickstart, gates, milestone table); repo branch `agent/repo-scaffold`, 115 tests green (125 with `--features llm`); **no git remote configured yet** |
| 3 | **AI 对话历史原始记录** | 🔄 excerpts landed per milestone — raw transcripts live in harness session history | [`docs/ai-logs/M1.md`](ai-logs/M1.md) … [`M5.md`](ai-logs/M5.md) (per-milestone: date range, key decisions, failures→fixes, test counts; 摘录 excerpts, privacy-redacted) |
| 4 | **开销明细表 (Excel, 按阶段: 人时/calls/tokens/花费/模型/工具)** | 🔄 drafted — export to Excel format pending | [`docs/expense-sheet.md`](expense-sheet.md) (D-20 table format, USD/CNY, per-phase; see also `cost/` ledger rows in code, commit a671575) |

## How to close each item

1. **设计文档 PDF**: assemble the four sections from the SPEC pointers above into one
   document, export to PDF, commit under `docs/` (or a `submission/` folder) and
   reference it here.
2. **源码**: owner pushes `agent/repo-scaffold` to the 清华 Git course repo; confirm
   README renders, tests pass (`cargo fmt --check && cargo clippy -D warnings && cargo test`).
3. **AI logs**: each `docs/ai-logs/M*.md` already summarizes its milestone; if graders
   require raw transcripts, export the full harness session history and archive it
   (pointer location TBD — keep the per-milestone files as the readable index).
4. **开销明细表**: complete `docs/expense-sheet.md` with per-phase 人时/calls/tokens/
   花费/模型/工具, then export to the required Excel format.

## Participation milestones

- ✅ 8.30 选题帖 · ✅ 9.1 ≥3 选题评论 (vault 09-01: "选题 posted late + ≥3 peer comments done")
- ⚠️ **9.6 公开展示帖 (设计摘要 + 项目链接)** — no recorded evidence the post is live; **verify**
- ⚠️ **9.8 ≥3 作品试用反馈 (5 pts)** — deadline passed 09-08 23:59 with no recorded evidence of
  posting; **verify or accept the loss** (5 pts of 100)
- ✅ 9.10 课堂展示 (5' + 2' Q&A, 25 pts) — delivered 09-10; deck committed at
  `docs/presentation/ai-experience.{html,pdf,png}`
