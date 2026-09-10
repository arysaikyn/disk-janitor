// workbench.ts — 动效工作台拆解清单（references/workbench.md §2–3）
// SHOTS 是唯一事实源；字幕表/SFX 钉帧/转场点全部从 timeline/sfx 派生。
export { SHOTS, beatF, beatT, SLAMS, T, TOTAL_FRAMES } from "./timeline";
export { SFX } from "./sfx";

export const CAPTIONS = [
  { shot: "s0_open", line: "还我磁盘 / 你值得你的空间" },
  { shot: "s2_scan", line: "每秒 1.21 GiB · 只读元数据" },
  { shot: "s3_card1", line: "证据，而不是名字。" },
  { shot: "s4_evidence", line: "每一项判定，都附证据链" },
  { shot: "s5_card2", line: "删掉的，都能回来。" },
  { shot: "s6_onetap", line: "18.9 GB 可回收" },
  { shot: "s7_purge", line: "销毁需要逐位输入字节数确认" },
  { shot: "s8_card3", line: "每一笔，都有台账。" },
  { shot: "s9_ledger", line: "隔离 · 销毁 · 恢复 —— 每一笔都记在台账" },
  { shot: "s10_finale", line: "还我磁盘 / DISK JANITOR" },
  { shot: "s11_signoff", line: "你值得你的空间 / 源码 github.com/arysaikyn/disk-janitor" },
] as const;

export const UNITS = [
  { id: "s0", component: "S0Open", shot: "s0_open" },
  { id: "s1", component: "S1Hero", shot: "s1_hero" },
  { id: "s2", component: "S2Scan", shot: "s2_scan" },
  { id: "s3", component: "BreathCard", shot: "s3_card1", props: { line: "证据，而不是名字。" } },
  { id: "s4", component: "S4Evidence", shot: "s4_evidence" },
  { id: "s5", component: "BreathCard", shot: "s5_card2", props: { line: "删掉的，都能回来。" } },
  { id: "s6", component: "S6Onetap", shot: "s6_onetap" },
  { id: "s7", component: "S7Purge", shot: "s7_purge" },
  { id: "s8", component: "BreathCard", shot: "s8_card3", props: { line: "每一笔，都有台账。" } },
  { id: "s9", component: "S9Ledger", shot: "s9_ledger" },
  { id: "s10", component: "S10Finale", shot: "s10_finale" },
  { id: "s11", component: "S11Signoff", shot: "s11_signoff" },
] as const;
