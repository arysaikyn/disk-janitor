// timeline.ts — 唯一事实源：拍号网格 + 镜头表（DESIGN-SPEC.md §4）
// 分析真值与渲染偏移分开记账（music-beat-sync §5b）。
export const FPS = 30;
export const WIDTH = 1920;
export const HEIGHT = 1080;
export const SOURCE_BEAT0 = 0.1712; // beat_data.json t0，永不掺渲染偏移
export const BEAT_INT = 0.50001; // T
export const OUTPUT_AUDIO_OFFSET_SEC = 0; // 渲后回测填实测值（sound-design §4.6）

export const beatT = (n: number) => SOURCE_BEAT0 + OUTPUT_AUDIO_OFFSET_SEC + n * BEAT_INT;
export const beatF = (n: number) => Math.round(beatT(n) * FPS);
export const TOTAL_FRAMES = 2550; // 85.0s；BGM 85.5s 尾部自然衰减

// 镜头表（帧边界全部 beatF(整数拍)）
export const SHOTS = {
  s0_open: { from: beatF(0), to: beatF(8) }, // 5–125
  s1_hero: { from: beatF(8), to: beatF(26) }, // 125–395
  s2_scan: { from: beatF(26), to: beatF(44) }, // 395–665
  s3_card1: { from: beatF(44), to: beatF(50) }, // 665–755
  s4_evidence: { from: beatF(50), to: beatF(66) }, // 755–995
  s5_card2: { from: beatF(66), to: beatF(72) }, // 995–1085
  s6_onetap: { from: beatF(72), to: beatF(84) }, // 1085–1265
  s7_purge: { from: beatF(84), to: beatF(98) }, // 1265–1475
  s8_card3: { from: beatF(98), to: beatF(104) }, // 1475–1565
  s9_ledger: { from: beatF(104), to: beatF(122) }, // 1565–1835
  s10_finale: { from: beatF(122), to: beatF(146) }, // 1835–2195
  s11_signoff: { from: beatF(146), to: beatF(170) }, // 2195–2555
} as const;

export const localBeat = (shot: { from: number }, n: number) => beatF(n) - shot.from;

// slam 名额（全片 3 处，R4）：b18 / b82 / b164
export const SLAMS = {
  hero: beatF(18), // 275 — orb 悬停锁定脉冲
  onetap: beatF(82), // 1235 — 18.9GB 计数到位
  signoff: beatF(164), // 2465 — 字标落定
};

// 设计 tokens（app/src/app.css 原样）
export const T = {
  ink: "#0a0e14",
  surface: "#111721",
  lift: "#16202c",
  text: "#e8edf4",
  mist: "#8b98a8",
  accent: "#2dd4bf",
  accentHover: "#5eead4",
  accentPressed: "#14b8a6",
  danger: "#f87171",
  amber: "#fbbf24",
  gradePure: "#34d399",
  gradeRebuild: "#2dd4bf",
  gradeRedl: "#60a5fa",
  gradeExpensive: "#fbbf24",
  gradeHands: "#f87171",
  hairline: "rgba(139, 152, 168, 0.14)",
  sans: '-apple-system, "SF Pro Text", "PingFang SC", system-ui, sans-serif',
  mono: '"SF Mono", ui-monospace, "JetBrains Mono", monospace',
} as const;

// 运动人格「精密仪器·呼吸核心」：入场统一 Easing.out(Easing.cubic)（≈bezier(0.2,0,0,1)），
// 入场时长 ~21f，过冲 ≤1.0、无 squash；hold ≥42f（平静关怀偏移）。落地弹的场合 y1>1 判例优先。
