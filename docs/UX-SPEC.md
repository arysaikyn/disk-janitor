# Disk Janitor — GUI Design Spec (UX-SPEC)

> Status: **accepted 2026-08-31** (owner-confirmed GUI pivot, decision register D-09–D-12).
> This document **replaces `DESIGN.md` §3.8** (ratatui TUI) as the UI authority; the Rust
> core (scan/classify/plan/execute/ledger/agent/policy/config, DESIGN §3.1–3.7, §3.9–3.10)
> is unchanged. Stack: Tauri 2.11.5 + wry 0.56.1 + Svelte 5.57 (all Apache-2.0 OR MIT,
> verified 2026-08-31) [12]. Citations: decision register `D-NN`; research notes
> [12] = `docs/research/12-tauri-gui-stack.md`, [13] = `docs/research/13-disk-tool-product-presentation.md`;
> core behavior cites `DESIGN.md` §/ADR. Companion docs: DESIGN, SPEC, design-decisions (ADR-18).

## 1. Product identity

**Name.** 「还我磁盘」 Disk Janitor — Chinese primary, English secondary everywhere in the
UI (D-02, D-12). The name is the promise: 还我 = "give it back" — 删除是操作，恢复是权利
(deletion is an operation, restoration is a right) [DESIGN §1].

**Positioning.** An evidence-driven, reversibility-first disk cleaner with an Apple-native
feel, usable by people who barely know what it is (D-10). It answers the question `du`
can't: not "where are the bytes?" but "which of these are safe to reclaim, why, and how do
I get them back if I'm wrong?" [SPEC §1]. Three pillars drive every screen:

| Pillar | What it means | Where the UI proves it |
|---|---|---|
| 证据驱动 Evidence-driven | every verdict carries inspectable machine-derived evidence, never name matching [DESIGN §1] | Evidence panel on every candidate (§3.4); refusal rows show the timestamp/lock as proof [13 §I] |
| 可恢复优先 Reversibility-first | `apply` frees 0 bytes; `purge` is the only irreversible step, gated by a typed byte count [DESIGN §5.1] | Ledger + Restore are first-class (§3.7, §4.3); 72 h grace countdown [ADR-10] |
| Apple 原生质感 Apple-native feel | frictionless, native chrome/menus/dialogs [12] | Titlebar, vibrancy, native dialogs per §5.5; system dialogs for dangerous ops (§7) |

**10-second pitch.**

- CN: 「看清哪些能删、为什么能删——删错了，一键拿回来。扫描只读元数据，文件不出你的 Mac。」
- EN: "See what's safe to reclaim and why — and take it back with one click if you're
  wrong. Scans read metadata only; your files never leave the Mac."

**Tone: calm, trustworthy — the anti-fear cleaner.** Competitor research [13 §B, §H]
shows the incumbent playbook (threat counters, "Neutralize threats before they do any
harm") reliably earns community distrust; every trusted 2026 entrant instead leads with
privacy + recoverability [13 §E]. Disk Janitor's voice rules:

| Say ✓ | Never say ✗ | Source |
|---|---|---|
| Specific honest numbers: 「19.3 GB 可回收 · 0 文件被销毁，直到你确认」 "19.3 GB recoverable · 0 files destroyed until you confirm" | Threat counters, "329K threats removed", fear of invisible harm | [13 §G5][13 §H] |
| 「Developer ID 签名并公证」 "Developer ID signed + notarized" | "Notarized by Apple" as endorsement shorthand | [13 §H][D-16] |
| 「我们清理字节」 "We clean bytes." | RAM boost / performance-miracle claims | [13 §H] |
| 「这一步不可撤销」 "This step cannot be undone." — stated plainly at the purge gate | Hiding the safety model; countdown-to-permanent-unlink endings (DaisyDisk Collector) | [13 §C] |
| 「零遥测 · 无账号」 "Zero telemetry · no account" stated on screen | Silent telemetry; undisclosed network calls | [13 §D][D-06] |

Refusals are voiced, never hidden: every refusal = verdict + evidence + safe alternative
(P1, D-03) — showing a refusal on screen is more persuasive than any badge [13 §I].

## 2. Information architecture

### 2.1 Window structure

| Surface | Decision | Rationale |
|---|---|---|
| Main window (label `main`) | **one** main window: left sidebar + content + collapsible right agent dock | a single focused flow; no window juggling for non-technical users [D-10] |
| Settings | **sheet inside the main window** (⌘,), not a separate window | one addressable surface; R3 config fields are few [DESIGN §3.9] |
| Purge confirm | **in-window modal sheet**; outside-click and Esc are inert (§4.2) | the signature moment must not be escapable by accident [P4] |
| Native dialogs | folder pickers + destructive-op confirms via `tauri-plugin-dialog` (sheet-style on macOS) [12] | dangerous ops use system dialogs, not HTML imitations [12] |
| Tray icon | **not in v1** — nothing runs in background by design (no daemon, SPEC §8) | a tray with no real behavior is noise; revisit only with a real background feature |
| About / privacy | panel inside Settings incl. the zero-telemetry statement [D-06] | privacy stated on screen [13 §G6] |

Window chrome per the Apple-native recipe [12]: `titleBarStyle: "Transparent"` +
`hiddenTitle: true` (real traffic lights, content extends under the titlebar);
vibrancy via `window-vibrancy` 0.8.0 with `transparent: true` + `macOSPrivateApi: true`
(fine for direct-DMG distribution; only forecloses the App Store route) [12]; geometry
persisted by the window-state plugin [12]. A second launch focuses the existing window
(single-instance plugin [12]; the core's `flock` already refuses a second writer,
DESIGN §3.5).

### 2.2 Screens

| Screen | Purpose | Entry |
|---|---|---|
| 仪表盘 Dashboard | volume truth, last-scan summary, one primary action | default on launch; first-run state per D-05 |
| 扫描 Scan view | volume/folder selection, live progress ring, cancel | Dashboard action |
| 审核 Review — **the heart** | evidence inspection; choose what enters the plan | after scan; Dashboard summary |
| 计划 Plan review | the manifest: quarantine / prune / refusals → apply | Review → 生成计划 |
| 活动 Activity | live apply/purge/restore progress + WAL-resume notices | auto during ops; from Ledger |
| 台账 Ledger | audit trail; restore affordance within grace | sidebar |
| 助手 Agent panel | propose-only chat: plan cards + evidence; human confirms [D-01] | global right dock (⌘J), any screen |
| 设置 Settings | scan/policy defaults, agent config (R3), language, appearance, uninstall | sheet |

Navigation model: the sidebar holds 4 persistent items (Dashboard / Review / Ledger /
Settings); Scan view, Plan review, and Activity are contextual stages; the Agent panel is
a global right dock. Esc always backs out of a *soft* stage (scan setup, plan review) and
never out of the purge gate (§4.2). Multi-window notices (e.g. "purge finished") use
`emit_to` broadcast events, not channels [12].

### 2.3 Data flow into the UI

Bootstrap-then-stream [12]: on mount, a snapshot command seeds state (`get_volumes`,
`get_ledger_tail`, `get_plan`), then a typed `tauri::ipc::Channel<T>` delivers ordered
deltas — never polling [12]. All scan/classify/plan/apply/purge I/O goes through ~10
named Rust commands; the webview never receives raw fs verbs, and the Tauri capability
file stays deny-by-default [12].

## 3. Screen-by-screen wireframes

Conventions for every wireframe below:

- Chrome is shared: transparent titlebar with real traffic lights, vibrancy backdrop,
  4-item sidebar, collapsible right agent dock (⌘J) [12] (§2.1–2.2). The sidebar is
  drawn in full once (§3.2) and abbreviated `(sidebar)` elsewhere.
- **Bilingual label contract (D-02, D-12, ADR-13):** every control renders 中文 primary +
  English secondary — 「开始扫描 Start scan」. Dense rows may drop the EN tail when
  width-bound; a screen never mixes CN-only and EN-only full labels. i18n keys carry
  both strings; composition is the component's job (§6, ADR-13).
- Wireframes pin content, hierarchy, and states — not pixels. Component mapping: §6.

### 3.1 Grade color language

Risk-tier color on every candidate row, plan entry, and ledger record is table stakes
[13 §I] (DevCleaner's SAFE/WARNING/DANGER chips [13 §A]); ours maps the five rebuild-cost
grades [ADR-8][DESIGN §6]. Every color pairs with a glyph + word (colorblind-safe,
theme-following per §5.5):

| Grade | Badge (bilingual) | Color | Plan policy |
|---|---|---|---|
| PureCache | ● 纯缓存 Pure cache | green | default allow |
| RebuildableFromSource | ● 可重建 Rebuildable | teal | default allow; regen cost shown |
| Redownloadable | ● 可重下载 Redownloadable | blue | default allow; network cost stated |
| Expensive | ⚠ 代价高 Expensive | amber | per-entry opt-in only [DESIGN §5.3] |
| HandsOff | 🔒 请勿动手 Hands off | red | never auto-plan; size shown, nothing more |

Refusals reuse the glyphs: ⚠/🔒 + refusal reason + evidence — never a bare strikethrough
[13 §I]. Color means exactly one thing on a row — "how wrong could this go" — bytes and
paths stay neutral.

### 3.2 仪表盘 Dashboard

```
┌──────────────────────────────────────────────────────────────────────┐
│ 仪表盘 Dashboard                                        ⌘J 助手 Agent │
├──────────────┬───────────────────────────────────────────────────────┤
│ 仪表盘       │  Macintosh HD · APFS · 可写 writable                  │
│ Dashboard    │                                                       │
│ 审核 Review  │  已用 Used               612.4 GB / 1 TB              │
│ 台账 Ledger  │  ███████████████████████████░░░░░░░░░░░               │
│ 设置         │  真实可用 Free (raw)      387.6 GB                    │
│ Settings     │  快照锁定 Snapshot-held  13.6 GB ¹                    │
│              │                                                       │
│ ──────────   │  上次扫描 Last scan    今天 14:32 · 1,204,553 files   │
│ (⌘J) 助手    │  可回收 Recoverable    19.3 GB · 214 项 candidates    │
│ Agent        │  已销毁 Destroyed      0 — 直到你确认 until confirm  │
│              │                                                       │
│              │  ┌──────────────────────────────┐                     │
│              │  │  开始扫描 Start scan          │  ← 唯一主操作 CTA   │
│              │  └──────────────────────────────┘                     │
│              │                                                       │
│              │  ¹ Time Machine 本地快照锁定的空间，过期后释放         │
│              │    freed after snapshot expiry [DESIGN §3.1]          │
└──────────────┴───────────────────────────────────────────────────────┘
```

Entry: default at launch; the scan button is the only CTA (D-05 — dashboard + hint line,
no wizard state machine). Volume truth shows raw free and snapshot-held space as two
labeled numbers [DESIGN §3.1]. 「0 已销毁」 is the anti-fear hero line [13 §G5].

Empty state (first run, D-05):

```
│           还没有扫描记录 No scans yet.                              │
│   扫描只读元数据，文件不会离开你的 Mac。                             │
│   Scans read metadata only — nothing leaves this Mac.               │
│           [ 开始第一次扫描 Start first scan ]                       │
```

Error state: unreadable volume → refusal banner in P1 format (§5.2) — 判定 verdict +
errno evidence + alternative (reselect root / replug / scan another volume).

### 3.3 扫描 Scan view

```
┌──────────────────────────────────────────────────────────────────────┐
│ 扫描 Scan · ~/ (Macintosh HD)                     [更改目标 Change]   │
├──────────────┬───────────────────────────────────────────────────────┤
│ (sidebar)    │                     ╭───────────╮                     │
│              │                   ╱               ╲                   │
│              │                  │     63 %        │                  │
│              │                  │  812,443 项     │  1.21 GiB/s [D-13]│
│              │                   ╲  当前 current ╱                  │
│              │                     ╰───────────╯  ~/Library/Develop… │
│              │                                                       │
│              │  候选 Candidates  214 · 19.1 GB    根错误 Root errs 2 │
│              │                                                       │
│              │  [ 取消 Cancel ]   中断 = 丢弃未完成批次，保留已完成   │
│              │                    interrupt drops incomplete [D-15] │
└──────────────┴───────────────────────────────────────────────────────┘
```

- Progress ring fed by a typed `Channel<ScanProgress>` (≥100 ms throttle) — never
  polling [12] (§5.3).
- Cancel per D-15: incomplete batches are dropped, completed classifications kept; the
  banner states exactly that — no fake "scan complete" (P6).
- Root errors stream inline as refusal rows; the scan continues (skip-and-continue
  posture, [ADR-4]).

### 3.4 审核 Review — the heart

Left = virtualized candidate table; right = evidence panel of the selected row. Every
row carries its grade badge (§3.1) and allocated bytes; every verdict is inspectable —
this panel is where 证据驱动 Evidence-driven stops being a slogan [13 §I].

```
┌──────────────────────────────────────────────────────────────────────┐
│ 审核 Review · 214 候选 candidates · 19.3 GB   [筛选 Filter ▾][排序 Sort ▾] │
├──────────────┬───────────────────────────────┬───────────────────────┤
│ (sidebar)    │ ✓  ~/code/api/node_modules    │ 证据链 Evidence chain │
│              │   ● 可重建 Rebuildable        │ ① 锁文件 lockfile:    │
│              │     2.1 GB   npm ci ≈ 40 s    │    package-lock.json  │
│              │                               │    newer than dir     │
│              │ ✓  ~/Library/Developer/…      │ ② 祖先 .git 活跃      │
│              │   DerivedData                 │    HEAD 3 h ago       │
│              │   ● 可重建 Rebuildable        │ ③ mtime 冷却 cool     │
│              │     6.8 GB   rebuild ≈ 90 s   │    6 d 14 h           │
│              │                               │ 重建 Rebuild:         │
│              │ ✓  ~/.npm/_cacache            │ `npm ci` ≈ 40 s       │
│              │   ● 纯缓存 Pure cache (CAS)   │ 网络 network: 不需要  │
│              │     3.2 GB                    │                       │
│              │ ✕  ~/code/api/node_modules/…  │                       │
│              │   ⚠ 正在写入 actively written │ 判定 REFUSED 拒绝     │
│              │     mtime 4 min ago          │ [03 §0][DESIGN §5.3] │
│              │                               │                       │
│              │ [ 214 项中 197 已选 197/214 ]  │                       │
│              │                               │                       │
│              │ [ 生成计划 Build plan ]       │ [在访达中显示 Reveal] │
└──────────────┴───────────────────────────────┴───────────────────────┘
```

| Rule | Spec |
|---|---|
| Selection | checkbox per row; refusals (✕) render **disabled**, not hidden — the refusal is the trust demo [13 §G3][13 §I]; reason row is tappable → evidence panel shows the exact signal (mtime 4 min ago) |
| Evidence panel | the candidate's `evidence: Vec<Evidence>` chain [DESIGN §6], numbered ①②③, each line = kind + human detail + strength; regen command + measured cost + network flag rendered verbatim [ADR-8] |
| Grades Expensive / HandsOff | ⚠/🔒 rows are listed but locked: 「显示大小，绝不自行处理」 shows-you-size-never-touches posture [13 §A DevCleaner]; opt-in is an explicit per-row toggle with a confirm chip [DESIGN §5.3] |
| Byte honesty | allocated bytes only (hardlink/clone-discounted); apparent size on hover/详情 [DESIGN §6 invariants] |
| Virtualization | windowed list, O(visible) rebuilds; selection re-found by stable path key when live data reorders rows [DESIGN §3.8][12] |
| Sort/filter | grade, size, mtime, path — filters never re-hide refusals (they pin trust) |

Empty state (scan done, nothing found):

```
│        扫描完成 — 没有值得回收的东西。Scan complete — nothing worth     │
│        reclaiming. 这本身就是好消息。That is good news.               │
```

Error state: evidence probe fails (permission denied) → row shows ⚠ 证据不可读 evidence
unreadable + errno, row stays ⚠-refused (never silent, never auto-included) (P1, P6).

### 3.5 计划 Plan review

The manifest is the artifact [02 §4 Hazel][DESIGN §3.3]: terraform-plan style, refusals
listed beside actions, totals hardlink-discounted, snapshot caveat attached.

```
│ 计划 Plan · plan-20260831-1432.json          [查看 JSON View manifest] │
│                                                                       │
│  操作 Actions                                                         │
│   11 × 隔离 Quarantine    5.4 GiB   （进入隔离区，0 字节释放          │
│                                        0 bytes freed [DESIGN §5.1]） │
│    2 × 执行清理命令 Prune    1.6 GiB   docker system prune / brew     │
│                                        cleanup --prune=90d            │
│   3 × 拒绝 Refused        —       ⚠ 孤儿树 orphan ×1 · ⚠ mtime ×2    │
│                                                                       │
│  合计 Total (去重 dedup)         19.3 GB · 72 h 宽限 grace [ADR-10]  │
│  快照注意 Snapshot note: 13.6 GB 在快照过期后才真正释放               │
│           freed only after TM snapshot expiry [DESIGN §3.1]           │
│                                                                       │
│  [应用 — 隔离 Apply — quarantine]   软确认 soft gate：可直接 Enter    │
└───────────────────────────────────────────────────────────────────────┘
```

Apply gate is soft (reversible): plan reviewed → Enter → quarantine moves begin (§4.3).
Purge is NOT offered here — it lives behind the applied-quarantine state (§4.2), so the
irreversible step can never be one click from a review list [02 §4 GrandPerspective].

### 3.6 活动 Activity (apply / purge / restore progress)

Contextual stage auto-entered during operations; every row streams over the exec channel,
strictly sequential, WAL-ordered [DESIGN §3.4].

```
┌──────────────────────────────────────────────────────────────────────┐
│ 活动 Activity · 隔离中 quarantining · plan-20260831-1432              │
├──────────────┬───────────────────────────────────────────────────────┤
│ (sidebar)    │ ✓ node_modules (api)      2.1 GB   隔离 quarantined   │
│              │ ✓ DerivedData             6.8 GB   隔离 quarantined   │
│              │ ⟳ _cacache                3.2 GB   移动中 moving…     │
│              │ ○ docker system prune     —       等待 pending       │
│              │                                                       │
│              │ 3/13 · 已移动 moved 12.1 GB · 释放 freed 0 字节 bytes │
│              │                                                       │
│              │ [ 取消 Cancel ]  中断安全：WAL 已记录，重启自动续跑   │
│              │                  WAL-recorded, auto-resumes [D-15]   │
└──────────────┴───────────────────────────────────────────────────────┘
```

- 「释放 freed 0 字节」 runs live on every apply — the reversibility claim, made
  numeric on screen [13 §G2].
- Interrupt (D-15): apply = cancel between entries, WAL auto-resumes next launch; a
  resumed launch shows a WAL-resume notice row first (「已恢复中断的操作 resumed
  interrupted operation」) — never a silent catch-up (P6).

### 3.7 台账 Ledger

Append-only audit trail, user-inspectable, never silently deleted (P5) — rendered from
ledger records, not a shadow state [DESIGN §3.5].

```
┌──────────────────────────────────────────────────────────────────────┐
│ 台账 Ledger · 1,204 条 records · [筛选 kind ▾] [导出 Export JSONL]   │
├──────────────┬───────────────────────────────────────────────────────┤
│ (sidebar)    │ #842  restore  ✓ 已恢复 restored                      │
│              │       DerivedData → ~/Library/Developer/…             │
│              │       undoes #815 · 宽限 grace 余 69 h                │
│              │ #815  apply    ✓ 隔离 quarantined  2.1 GB             │
│              │ #812  plan     plan-20260831-1432 (11 隔离 · 3 拒绝)  │
│              │ #801  purge    🔒 已销毁 destroyed 5.4 GiB            │
│              │       字节数已确认 byte count confirmed: 5841673728   │
│              │ #777  budget   deepseek-chat · 12 calls · $0.0031     │
│              │                                                       │
│              │ [ 恢复… Restore… ]  ← 选中 apply 记录时可用           │
└──────────────┴───────────────────────────────────────────────────────┘
```

- Restore affordance lives here within the grace window: select an `apply` record →
  恢复 Restore (§4.4). Expired-grace survivors surface as purge candidates with the
  countdown shown, never auto-purged [ADR-10].
- `purge` rows keep the typed byte count visible forever — the audit is the proof
  (P5, P7). Every public claim is generated from this ledger, never hand-typed (P7).
- Export = raw JSONL copy (same bytes the product replays); no telemetry channel
  exists to send it anywhere (D-06).

### 3.8 设置 Settings (sheet, ⌘,)

```
┌──────────────────────────────────────────────────────────────────────┐
│ 设置 Settings                                                   ⌘,   │
│                                                                       │
│  扫描 Scan                                                            │
│    默认根 roots            [~/ ] [更改 Change…] (native picker [12])  │
│    忽略 glob 忽略 globs    [node_modules 除外除外 …]                  │
│    阈值 thresholds         mtime 热区 10 min · 最小 50 MB  [高级 Adv] │
│                            (thresholds live in config.toml [DESIGN §3.9]) │
│                                                                       │
│  助手 Agent (R3)                                                      │
│    endpoint   [https://api.deepseek.com/v1 ]   模型 model [deepseek-chat ] │
│    API key    [•••••••••••• ]  (secrecy 存储; 永不写入日志 [DESIGN §3.9]) │
│    预算 budget [ $2.00 ] [ 200k tok ] [ 40 turns ]  到达即中断        │
│                    interrupt at cap [ADR-9][DESIGN §3.7]              │
│                                                                       │
│  语言 Language          ( ) 中文 primary  ( ) English primary         │
│                          switch applies live; menus re-label [12]     │
│  外观 Appearance        ( ) 跟随系统 Follow system  ( ) 浅 Light      │
│                          ( ) 深 Dark                                  │
│                                                                       │
│  隐私 Privacy                                                          │
│    ✓ 零遥测 Zero telemetry — 无账号 no account · 网络仅限 LLM 请求   │
│      network is LLM calls only (D-06)[13 §G6]                         │
│                                                                       │
│  卸载 Uninstall…          ← restore-or-confirm contract (ADR-16)      │
│         [取消 Cancel]                                  [保存 Save]    │
└───────────────────────────────────────────────────────────────────────┘
```

Uninstall row (D-17, ADR-16): opens the restore-or-confirm dialog — list every
surviving quarantine with sizes; user restores or explicitly confirms each; only when
the list is empty does the app remove `~/.disk-janitor/` state. The brand invariant
恢复是权利 is enforced at uninstall time, not just in marketing copy.

### 3.9 助手 Agent panel (right dock, ⌘J)

Propose-only by construction (D-01, P2): the agent returns plan IDs and verdict cards;
every card's action button routes through the same human confirm as any other row — the
LLM never executes quarantine moves [ADR-7][DESIGN §3.7].

```
┌───────────────────────────────┐
│ 助手 Agent            [折叠 ×] │
├───────────────────────────────┤
│ 你 You: 腾出 25 GB，别动我这两 │
│ 周在跑的项目                   │
│                               │
│ 助手 Agent:                   │
│ ┌─ 计划卡片 plan card ───────┐│
│ │ plan-20260831-1501         ││
│ │ 11 隔离 · 2 prune · 3 拒绝 ││
│ │ 19.3 GB (去重 dedup)       ││
│ │ 预览见计划页 see Plan page ││
│ │ [在计划中打开 Open in Plan]││ ← routes to §3.5; no
│ └───────────────────────────┘│   exec button exists
│ 拒绝 refused: 2 项 mtime 热  │
│ 证据 evidence: 正在写入…      │
│ 替代 alternative: 稍后重扫    │
│                               │
│ 本轮 turn: 3 · tokens 4,102   │
│ 成本 cost $0.0009 [ADR-9]    │
└───────────────────────────────┘
```

- Cards cite evidence IDs, never paths — the agent-side payload is opaque-ID only;
  the chokepoint guarantees it [ADR-7 §structural]. 「在计划中打开」 is the only CTA:
  approval happens in Plan review, on the human's screen.
- Cost footer per turn; budget interrupt surfaces as a distinct stop-reason banner
  (「已到预算上限 budget reached」) with the count of un-run turns [ADR-9].
- Trajectory history reloads from session snapshots (`RunSnapshot`), matching the
  ledger's reproducibility posture (P7) [DESIGN §3.7].

## 4. Key flows

### 4.1 First scan + TCC pre-flight

The first-run state is a dashboard + hint line, not a wizard (D-05). But `~/Library`,
`~/.npm`, external volumes may trip macOS TCC (privacy prompts); the app pre-flights
readability so the user never discovers a half-scanned tree after the fact.

```
[开始扫描 Start scan]
   │
   ├─ native folder picker (tauri-plugin-dialog, sheet-style) [12]
   │
   ├─ pre-flight probe: metadata-only getattrlist on each root's top level
   │     ├─ all readable → scan starts (ring, §3.3)
   │     └─ EPERM on root(s) → TCC pre-flight banner, scan does NOT half-run:
   │
│    ┌──────────────────────────────────────────────────────────────┐
│    │ ⚠ macOS 需要授权 macOS needs permission for ~/Library        │
│    │ 证据 evidence: probe returned EPERM (系统设置系统设置▸        │
│    │   隐私与安全性▸文件与文件夹 Privacy & Security ▸ Files)      │
│    │ 替代 alternative: [打开系统设置 Open Settings] (deep link)   │
│    │   或从该根中排除 or exclude this root and continue           │
│    │   [排除并继续 Exclude & continue] [重试 Retry]               │
│    └──────────────────────────────────────────────────────────────┘
```

Refusal voice = P1 exactly: verdict (cannot read) + evidence (errno + the exact
Settings pane) + safe alternative (deep link or exclude-and-continue) (D-03). The
banner is generated from the probe result, never from a guess (P7).

### 4.2 Review → plan → apply → purge — the typed byte-count modal (signature moment)

The purge gate is the product's signature moment [13 §G2]: the one irreversible step,
made deliberate. No other tool gates on the user typing the exact byte count
[ADR-10][DESIGN §5.3].

```
(after apply completes: 「已隔离 quarantined 5.4 GiB · 释放 freed 0 字节」)
   │
   ├─ 台账 Ledger / Dashboard: [清理隔离区 Purge… ] on the op
   │
   ├─ modal sheet (in-window, §2.1 — outside-click and Esc are INERT [P4])
   │
│    ┌────────────────────────────────────────────────────────────────┐
│    │        清理隔离区 Purge quarantine — 这一步不可撤销             │
│    │                    This step cannot be undone. [13 §C]         │
│    │                                                                │
│    │  将永久销毁 permanently destroy:                                │
│    │   11 entries · 5,841,673,728 bytes allocated (5.4 GiB)         │
│    │   = 同一个数字 the same hardlink-discounted number the plan     │
│    │     displayed [ADR-10 honesty check]                           │
│    │                                                                │
│    │  其余 72 h 内可恢复 everything else restorable within grace    │
│    │                                                                │
│    │  请输入字节数以确认 type the byte count to confirm:             │
│    │  [ 5,841,673_         ]   ← 精确匹配 required，否则拒绝        │
│    │                                                                │
│    │  [取消 Cancel]              [销毁 Destroy] (disabled until      │
│    │                                          exact match)          │
│    └────────────────────────────────────────────────────────────────┘
   │
   ├─ mismatch → shake + 「字节数不匹配 count mismatch — 已拒绝 refused」
   │    (typed attempt never logged anywhere — it is not ledger data)
   ├─ exact match → purge runs to completion (D-15: no mid-purge stop,
   │    progress reads-only); typed count echoed into the `purge` ledger
   │    record as bytes_freed [DESIGN §3.5]
   └─ done: 「已释放 freed 5.4 GiB · 已销毁 destroyed 11 entries」
        + TM-snapshot pinning caveat if snapshots exist [DESIGN §3.1]
```

Design rules of the moment:

- The number typed is **allocated** bytes, hardlink/clone-discounted — computed from
  the same totals the plan showed, so a mismatch means the plan changed under the user
  [ADR-10].
- Esc / outside-click inert (§2.1): the gate must not be escapable by accident — but
  equally must not trap: Cancel works, always; only the *confirm* is deliberate.
- 销毁 Destroy renders as red + 🔒-adjacent styling (grade language §3.1), and the
  modal states the plain truth 「这一步不可撤销」 [13 §C] — no countdown-to-unlink
  theater (DaisyDisk Collector anti-pattern [02 §2]).
- Purge never reachable from Review/Plan directly (§3.5): only from an *applied*
  quarantine state, so every pre-purge step is reversible by construction.

### 4.3 Apply (quarantine) semantics

Soft gate → sequential WAL-ordered moves → live Activity (§3.6). Interrupt = cancel
between entries; WAL auto-resumes next launch (D-15). Zero bytes freed throughout —
the Activity header keeps 「释放 freed 0 字节」 on screen for the whole operation
[13 §G2][DESIGN §5.1].

### 4.4 Restore — the undo moment

The moment no competitor shows on camera [13 §G1] — and the brand's proof: 删除是操作，
恢复是权利 [DESIGN §1]. Entry: Ledger `apply` record → [恢复 Restore] (§3.7), or the
resumption notice after an interrupted apply.

```
│ [恢复 Restore] → confirm sheet:                                       │
│                                                                       │
│   恢复 DerivedData (6.8 GB) → ~/Library/Developer/Xcode/…             │
│   reverse renames from the WAL pair · undoes #815 [DESIGN §3.4]       │
│   宽限 grace: 剩 remaining 69 h of 72 h [ADR-10]                      │
│                                                                       │
│   [取消 Cancel]                          [恢复 Restore] (soft gate)   │
│                                                                       │
│ → done banner: 「已恢复 restored · 回到原位 back where it was」        │
│   ledger `restore` rec with undoes:<apply seq> [DESIGN §3.5]          │
```

- Restore is a *soft* gate — it is the safe direction; friction belongs to destruction,
  not to recovery (P4's corollary).
- After restore, the original path is re-verified (target exists? `EEXIST` → offer
  rename-or-skip with evidence, P1) — never overwrite silently (P6).
- Purged entries: restore is impossible and the Ledger says so plainly (「已销毁，无法
 恢复 destroyed — not restorable」) — honesty over false comfort (P6). The 72 h grace
  countdown is always visible on restorable rows [ADR-10].

## 5. Interaction rules

### 5.1 Propose-only agent (D-01, P2)

| Rule | Spec |
|---|---|
| Output shape | plan ID + verdict cards + refusal explanations (§3.9). The agent's only mutation of UI state is *suggesting*; plan approval, apply, purge, restore confirmations are human-only actions on Rust-side commands |
| No exec path | the agent dock renders no destructive button; 「在计划中打开 Open in Plan」 is its only CTA. Enforcement is architectural (tools accept/return IDs; execution lives in `execute/`), not a hidden button [ADR-7][DESIGN §3.7] |
| Privacy | payloads opaque-ID only; the serializer chokepoint + CI leak test guarantee no path ever reaches the model [ADR-7] |
| Budget | cost/token/turn counters visible per turn (§3.9 footer); interrupt = distinct banner naming the stop reason [ADR-9] |
| Failure | unreachable endpoint / malformed tool output → the dock states it (「助手不可用 agent unreachable — 本地流程不受影响 local flow unaffected」) and the product remains fully usable offline (P3, P6) |

### 5.2 Refusal format — 拒绝 + 证据 + 替代 (D-03, P1)

Every refusal — classifier, policy, volume, TCC, agent — renders in one fixed shape:

> **判定 verdict** — what cannot happen and to what.
> **证据 evidence** — the machine-derived fact (errno, timestamp, lock, missing
> manifest), tappable to the evidence panel.
> **替代 alternative** — the safe path forward (exclude root, run the regen command,
> wait for mtime to cool, open Settings).

The three parts are never omitted, never reordered. Rationale: showing a refusal with
its evidence is more persuasive than any trust badge [13 §I], and unvoiced refusals are
exactly the "black-box judgment" the product exists to replace [02 §4].

| Refusal source | Example render |
|---|---|
| MtimeHot | ⚠ 拒绝自动处理 refuse auto-action · 证据: mtime 4 min ago（正在写入） · 替代: 稍后重扫 rescan later |
| OrphanTree | ⚠ 项目已删除？ project deleted? · 证据: no package.json in any ancestor · 替代: 手动确认后单独处理 review manually |
| TCC / EPERM | ⚠ 无法读取 cannot read ~/Library · 证据: probe EPERM · 替代: [打开系统设置] / 排除该根 |
| exFAT / cross-volume | ⚠ 不支持原子隔离 atomic quarantine unsupported · 证据: `VOL_CAP_INT_RENAME_EXCL` missing / EXDEV · 替代: 只读分析 analyze read-only |
| Budget | 已到预算上限 budget reached · 证据: $2.00 / 200k tok / 40 turns 中已触发 · 替代: 提高上限或继续本地流程 raise cap or continue locally |

### 5.3 Progress — `ipc::Channel`, never polling (D-15 substrate, [12])

- Scan/classify: typed `Channel<ScanProgress>` batches, ≥100 ms throttle, ordered
  delivery — the channel *is* the state feed; bootstrap snapshot command seeds, channel
  takes over (§2.3) [12].
- Apply/purge/restore: exec events stream over the same channel pattern; rows are
  WAL-ordered because the executor is [DESIGN §3.4].
- Events (`emit_to`) only for low-frequency cross-window notices — never progress
  (out-of-order risk documented in [12]) [12].
- Cancel: a task handle in managed state (`tauri::State`) owns cancellation; the UI
  cancel button flips it, the core decides safe stop points per D-15.

### 5.4 Interrupt semantics (D-15)

| Operation | Interrupt = | UI behavior |
|---|---|---|
| scan / classify | drop incomplete batches | cancel banner; completed classifications kept; never reports "complete" (P6) |
| apply | cancel between entries; WAL auto-resume next launch | Activity shows 「已取消，重启后继续 cancelled, resumes next launch」 |
| purge | **none** — runs to completion once confirmed | progress becomes read-only; Cancel absent (the modal warned; D-15) |
| restore | cancel between entries; WAL re-runnable | same as apply |

The purge row is the point: a confirmed irreversible op does not negotiate. Every other
operation is resumable because the WAL already owns its state [DESIGN §5.2].

### 5.5 Theme + appearance

- Default: follow system (`windows[].theme` omitted; `prefers-color-scheme` CSS drives
  the webview; `ThemeChanged` forwarded only for menu/tray chrome sync) [12].
- In-app override (§3.8): 浅 Light / 深 Dark / 跟随系统 Follow system; override writes
  `localStorage` + sets the window theme explicitly [12].
- Grade colors (§3.1) are CSS variables per theme — meaning never rides on hue alone
  (glyph + word always present).
- Vibrancy backdrop (§2.1) on macOS; Windows Mica/Acrylic; Linux none (compositor
  rules) — degrade loudly where unsupported, never fake the effect (P6) [12].

## 6. Component inventory

Svelte 5 (runes) components; stores are `$state` class instances fed only by Channel
`onmessage` handlers [12]. Every text-bearing component takes bilingual label pairs
(`{zh, en}`), composition per §3 conventions (D-02, D-12; ADR-13).

| Component | Purpose | Notes |
|---|---|---|
| `AppShell.svelte` | sidebar + content + agent dock host | window chrome, keyboard map (§2.1–2.2) |
| `Sidebar.svelte` | 4 persistent nav items + agent toggle | bilingually labeled, active state |
| `GradeBadge.svelte` | ●/⚠/🔒 grade chip (§3.1) | CSS-variable colors, glyph+word always |
| `BilingualLabel.svelte` | 「中文 English」 pair | dense mode drops EN tail (§3 conventions) |
| `VolumeCard.svelte` | used/free/snapshot-held bars (§3.2) | two labeled free numbers [DESIGN §3.1] |
| `ScanProgressRing.svelte` | ring + rate + current root (§3.3) | Channel-fed; no polling |
| `CandidateTable.svelte` | virtualized rows w/ checkboxes (§3.4) | O(visible); path-key selection restore [DESIGN §3.8] |
| `EvidencePanel.svelte` | numbered evidence chain + regen line (§3.4) | renders `Vec<Evidence>` verbatim [ADR-8] |
| `RefusalRow.svelte` | ✕ row in P1 format (§5.2) | disabled, never hidden [13 §I] |
| `PlanManifest.svelte` | actions/refusals/totals/snapshot note (§3.5) | mirrors manifest JSON; 查看JSON opens raw |
| `ActivityList.svelte` | per-entry exec rows + freed-0 header (§3.6) | WAL-resume notice row support |
| `LedgerView.svelte` | record stream + restore affordance (§3.7) | raw JSONL export |
| `PurgeGateModal.svelte` | typed byte-count sheet (§4.2) | Esc/outside inert; match gate [ADR-10] |
| `RestoreSheet.svelte` | restore confirm + grace countdown (§4.4) | soft gate |
| `AgentDock.svelte` | chat + plan cards + cost footer (§3.9) | propose-only CTAs [D-01] |
| `SettingsSheet.svelte` | scan/agent/language/appearance/uninstall (§3.8) | native dialog for roots picker [12] |
| `UninstallDialog.svelte` | restore-or-confirm quarantine list (§3.8) | ADR-16 contract |
| `TccBanner.svelte` | permission refusal banner (§4.1) | P1 shape; deep link |
| `stores/scanState.ts` etc. | `$state` stores per concern | updated only in `onmessage` [12] |
| `i18n/{zh-CN,en}.json` | string tables | `svelte-i18n`, zh-CN primary, en fallback [12] |

State discipline: stores never poll; bootstrap-then-stream (§2.3) is the only data path;
every component's listeners torn down on destroy (channel-lifecycle leak warning [12]).

## 7. Competitor-learned patterns (note 13)

What the product takes from [13]'s survey — and what it deliberately refuses:

| From | Pattern | Our use |
|---|---|---|
| DevCleaner [13 §A] | risk-tier chips (SAFE/WARNING/DANGER) | grade color language (§3.1); DANGER-tier never pre-selected posture [13 §H] |
| DevCleaner [13 §A] | specific hero number ("23.7 GB") | Dashboard's 「19.3 GB 可回收」+「0 已销毁」 (§3.2) [13 §G5] |
| DaisyDisk [13 §C] | beauty + speed honesty; metadata-only privacy promise | scan ring with rate (§3.3); 「文件不出你的 Mac」 everywhere (§3.2, §3.8) |
| DaisyDisk [13 §C] | **rejected:** Collector ends in 5 s countdown → permanent unlink | typed byte-count gate instead (§4.2) [ADR-10] |
| Hazel [13 §C via 02] | rules/manifest as first-class artifact | plan manifest page (§3.5) |
| Stats [13 §D] | telemetry disclosure builds trust | zero-telemetry stated on screen (§3.8) [D-06] |
| Drezze [13 §E] | "moves to Trash so it's recoverable" as trust metaphor | superseded: quarantine + WAL restore, stronger than Trash (§4.4) |
| CleanMyMac [13 §B] | **rejected:** threat counters, fear framing, "Notarized by Apple" endorsement shorthand | anti-fear voice table (§1); "Developer ID 签名并公证" wording only [D-16] |
| — [13 §F] | Screen Studio-style demo conventions | 90 s structure with restore as climax (§4.4); demo planning deferred (X-02, D-21) |

Open lane noted by [13]: no major competitor ships real Simplified-Chinese product copy —
CN-primary bilingual UI (ADR-13) is a differentiator, not just accessibility [13 §delta].
