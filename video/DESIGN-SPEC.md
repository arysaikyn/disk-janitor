# Disk Janitor「还我磁盘」— 设计 spec 与最终分镜（制作放行文档）

> 模式：共同创作（co-creation）。Phase A–D 逐 gate 由 owner 确认。
> 帧率 30fps · 1920×1080 · 总长 85.0s（2550f）· BGM 85.5s 尾部自然衰减。

## 0. 产品简报（Phase A 已确认）

| 项目 | 判断 | 依据 |
|---|---|---|
| 定位 | 磁盘清理桌面 App——「还你空间」不是「删文件」 | SPEC Part A; UI 文案「可回收」「0 已销毁 · 72h 宽限」 |
| 受众 | CS 学生（16:9 课堂展示） | Layer 0 owner 指定 |
| 核心卖点 | 证据化判断 + 72h 可逆 + 诚实字节 | 五安全不变量 |
| 必须展示 | ① orb/仪表盘 ② 扫描 ③ 证据审核 ④ 一键清理 ⑤ purge 仪式 ⑥ 台账 | 五屏+不变量 |
| 数据口径 | :1420 demo 合成数据（虚构造盘 1000GB/612.4GB/18.9GB），无隐私风险，可入片 | D-38 --demo-data |
| One Thing | **你值得你的空间** | Layer 0 |
| CTA | 品牌句 + 仓库 URL（`git.tsinghua.edu.cn/rust-course/2026/agent/agent-aes25`，已替换占位） | Phase A gate |

## 1. 音乐结构表（beat_data.json 审计链）

- **网格**：BPM=120.00（beat_track 120.19 → 最小二乘拟合 120.00），t0=0.1712s，T=0.50001s
- **验收**：残差 ±18ms ≈ 半帧；候选覆盖 0.5x=0.88 / **1x=0.90（胜）** / 2x=0.80
- **30fps 换算**：1 拍 = 15.0002f ≈ 15f 整（120BPM 的整数倍性质，量化误差 ≤0.01f）
- **结构**（RMS 4 拍窗）：mid 0–10s · high 10–36 · dip 36–40 · high 40–50 · **mid 50–58（天然 breakdown = 品牌呼吸位）** · high 58–84 · fade 84–85.5
- **最强 hit 候选池**：b18(k2.9) · b82(k2.5) · b92(k2.3) · b68/70/76/78/80(k2.0–2.2) · b159(k1.5, 79.7s 收尾最强)

## 2. 视觉方向（Phase B 已确认）：精密仪器 · 呼吸核心

Tokens 原样取自 `app/src/app.css`：ink `#0a0e14` / surface `#111721` / lift `#16202c` /
text `#e8edf4` / mist `#8b98a8` / **accent `#2dd4bf`**（hover #5eead4）/ grade 五色
`#34d399 #2dd4bf #60a5fa #fbbf24 #f87171` / danger `#f87171` / amber `#fbbf24` /
hairline `rgba(139,152,168,.14)`；圆角 6/10/16 concentric；字体 SF Pro Text + PingFang SC
（sans）/ SF Mono（数字主角，tabular-nums）；分层阴影代边框。

**运动人格**：专业信赖骨架 —— 入场 ~21f、`cubic-bezier(0.2,0,0,1)`（=产品 token 同族）、
过冲 ≤1.0、squash 0；hold 节奏向平静关怀偏移（呼吸位 ≥42f）。三个词：**精确、沉稳、释然**。

## 3. 功能→镜头映射（Phase C 已确认）

| 功能 | 首选镜头卡 | 备选 | 页面状态 | 手法名额 |
|---|---|---|---|---|
| 品牌开场 | brand-ink-open | letterspace-materialize | 手搓（品牌段，准许） | 墨线描画（1次） |
| orb 主角立传 | spotlight-hero-card | — | Dashboard 有数态截图 | 聚光+推近+悬浮弧（1次） |
| 实时扫描 | scan-bracket-sweep | scanline-assemble-flyin | Scan 进行中截图 | 往复光带（1次） |
| 可回收数字 | odometer-digit-roll | counter-confetti | Review 头部大数字 4x | 里程表滚轮（1次） |
| 证据审核 | list-stack-press | scanline-annotate-focus | Review 展开态截图 | 逐行压层（1次） |
| 一键清理 | counter-confetti | particle-celebrate-hits | Review 18.9GB | 计数+纸屑（1次） |
| purge 仪式 | value-stagger-gradient* | gauge-readout-moves | PurgeGate 截图+手搓 | 渐变错峰（1次） |
| 台账/恢复 | scanline-assemble-flyin | list-reveal | Ledger 记录态截图 | 扫描线组装（1次） |
| 收尾组装 | logo-shrink-wordmark-lockup + grain-dissolve | ui-to-brand-morph | 手搓（品牌段，准许） | 环收束+颗粒聚合（1次） |

*value-stagger-gradient 用于 PurgeGate 的分级错峰揭示（骨架段稀疏→文字段密集的单调爬升）。

## 4. 最终分镜（Phase D 已确认 · 11 镜 2550f）

时间线全部 `beatF(n)`（=15n+5f）；slam 名额 3：b18(275) · b82(1235) · b164(2465)。
呼吸字卡 3 张（3s 各 90f），高/低交替，信息密度最高镜头（s7 purge）置于峰值段前。

| # | beatF | from–to (帧) | 时长 | 镜头卡 | 主动作 | 素材来源 | 字幕（真实 UI 文案） | 转场/SFX 意图 |
|---|---|---|---|---|---|---|---|---|
| S0 | b0–b8 | 5–125 | 4.0s | brand-ink-open | 准星描画→「还我磁盘」压印→「你值得你的空间」副标 | 手搓（品牌段） | 还我磁盘 / 你值得你的空间 | 淡切 / pen-write, whoosh-swirl |
| S1 | b8–b26 | 125–395 | 9.0s | spotlight-hero-card | 聚光游走→斜侧推近 orb 卡→悬浮 3s→贴回 | Dashboard 截图 2x + orb 元素 4x | （无字幕，纯质感镜） | **slam b18** / whoosh-big, sparkle, transition-snap |
| S2 | b26–b44 | 395–665 | 9.0s | scan-bracket-sweep | 文档（扫描面板）弹入→括号落位→光带 5 趟往复 | Scan 进行中截图 2x | 扫描 / 已扫描 · 速率 /s | whoosh 大区运镜 / data-scan ×5 趟 |
| S3 | b44–b50 | 665–755 | 3.0s | 呼吸字卡 1 | 「证据，而不是名字。」墨底字卡 | 手搓字卡 | 证据，而不是名字。 | 硬切 / typewriter-hit-single |
| S4 | b50–b66 | 755–995 | 8.0s | list-stack-press | Review 展开态逐行压入（纯缓存→可重建→代价高→请勿动手） | Review 截图 2x + 行元素 | 纯缓存 / 可重建 / 代价高 / 请勿动手 | 硬切 / row 逐行 pop（双样本交替递减） |
| S5 | b66–b72 | 995–1085 | 3.0s | 呼吸字卡 2 | 「删掉的，都能回来。」 | 手搓字卡 | 删掉的，都能回来。 | 硬切 / transition-soft |
| S6 | b72–b84 | 1085–1265 | 6.0s | odometer-digit-roll | 18.9 GB 里程表逐位滚锁（**slam b82=1235**）+18.9GB counter-confetti 纸屑合体 | Review 大数字 4x 截图 | 18.9 GB / 可回收 | **slam b82** / riser, impact-cine-big, sparkle |
| S7 | b84–b98 | 1265–1475 | 7.0s | value-stagger-gradient | PurgeGate：「这一步不可撤销」→分级行错峰爬升→字节数输入锁定 | PurgeGate 截图 2x | 清理隔离区 / 这一步不可撤销 / 输入字节数 | 淡切 / lock-digital, key 拟音 |
| S8 | b98–b104 | 1475–1565 | 3.0s | 呼吸字卡 3 | 「每一笔，都有台账。」 | 手搓字卡 | 每一笔，都有台账。 | 硬切 / marker-pen-line |
| S9 | b104–b122 | 1565–1835 | 9.0s | scanline-assemble-flyin | 台账记录态：扫描线扫过→隔离/销毁/恢复行逐条组装 | Ledger 截图 2x | 台账 / 隔离 · 销毁 · 恢复 / 72h 宽限 | whoosh / sweep-fast, row pop 阶梯 |
| S10 | b122–b146 | 1835–2195 | 12.0s | logo-shrink-wordmark-lockup | 已展示功能代表元素（orb/扫描行/证据点/数字/台账行）四方向飞入→teal 环收束成 orb→字标 lockup | 手搓组装 + 元素 cutout | 还我磁盘 | b159（79.7s 全曲尾段最强 kick）= 环收束落位 |
| S11 | b146–b170 | 2195–2555 | 12.0s | grain-dissolve | 长句→颗粒爆散→凝聚「你值得你的空间」发光字标→**slam b164(2465)**→仓库 URL + hold ≥1s→随 BGM fade | 手搓（品牌段） | 你值得你的空间 / 源码 git.tsinghua.edu.cn/rust-course/2026/agent/agent-aes25 | **slam b164** / impact loudest, sparkle-wand, 声音随 fade |

**段落能量核对**：S0 低→S1 中（质感最慢镜）→S2 中→S3 低→S4 中高→S5 低→S6 高→S7 中
（breakdown 段天然呼吸）→S8 低→S9 中高→S10 峰→S11 收。高/低无三连；手法 9 种各 1 次；
hold 预算：S0 字标落定 hold 30f、S1 悬停+归位 ≥90f、S6 锁定后 hold 45f、S11 静止 ≥45f。

## 5. 素材采集清单（Phase E 输入）

`:1420` demo 数据已冻结（虚构）。采集 2x 全页 + 元素级 + layout.json：
1. Dashboard 有数态（含 orb 元素 4x 特写）
2. Scan 进行中态（速率曲线可见）
3. Review 展开态（分级行完整）+ 头部 18.9GB 4x
4. PurgeGate 输入态
5. Ledger 记录态（需造 3 条记录：隔离/销毁/恢复）

## 6. 工程约束

- webpack（**禁止 Config.setRspack(true)**，本机静默崩）
- 禁 Date.now()/Math.random()；mulberry32 固定种子
- `src/timeline.ts`：SOURCE_BEAT0=0.1712、BEAT_INT=0.50001、beatF(n)=round((t0+n·T)·30)
- OUTPUT_AUDIO_OFFSET_SEC 渲后实测（sound-design §4.6）
- 终渲双版：BGM 版 + 无 BGM 版（inputProp `bgm`）
