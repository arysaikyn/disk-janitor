# Disk Janitor「还我磁盘」宣传片 — 交付包 (out/)

- **promo.mp4** — 主版（16:9 1920×1080@30fps，85.0s，含 BGM "Keep Walking" + 全 SFX）
- **promo-nobgm.mp4** — 无 BGM 版（保留 SFX，自配音乐用；同一时间线渲出）
- **promo-916.mp4** — 9:16 竖版合成（同时间线，非裁切）
- **poster.png** — 封面帧（S11 字标落定 f2500）
- **captions.txt** — 烧录字幕清单（全部真实 UI 文案，见 DESIGN-SPEC §4）
- **beat_data.json** — BGM 节奏分析审计链（120.00 BPM, t0=0.1712s, ±18ms）
- **REVIEW-v3.md / REVIEW-v4.md** — 独立终检报告（13 项验收 + 修复记录）

修复记录（v3 → v4）：
1. P0：S2/S7/S9 页顶浅色横条（PageCam 背景 #faf7f2 → #0a0e14）— 已修
2. P1：slam/动作节奏错位 — S6 滚轮锁定点→b82、S11 凝聚→b164、S1 贴回→b24+8、S10 pop 钉回飞入窗口 — 已修
3. P1：S11 末行「9.10 课堂见」杜撰+过小 → 替换 i18n 真文案「零遥测 · 无账号 · 扫描只读元数据」34px — 已修

待 owner：把 CTA 占位 URL（github.com/arysaikyn/disk-janitor）替换为清华 Git 实际地址后重渲（一条字符串 + npx remotion render）。
