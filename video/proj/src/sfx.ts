// sfx.ts — 声音设计：声明式钉帧表（S2/S3/S5）+ 同步修正（sound-design §4.6）
// 画面锁定后制作。PEAK_F 逐文件实测（48kHz 包络峰，2026-09-10）。
import { staticFile } from "remotion";
import { beatF, FPS } from "./timeline";

// 输出音轨偏移实测（2026-09-10，promo-v2 回测：0.1s 模板 + ±0.12s 窗 + 抛物线亚帧精化，
// 8 拍探针 + 3 slam 探针全部一致 -0.23f。注意：6s 宽窗互相关会锁到长特征读出 +1.27f 伪值，弃用。）
export const OUTPUT_AUDIO_OFFSET_F = -0.23;
const PEAK_F: Record<string, number> = {
  "sfx/whoosh-big.mp3": 21.4,
  "sfx/impact-cine-big.mp3": 65.2,
  "sfx/transition-snap.mp3": 3.5,
  "sfx/riser-cine.mp3": 31.3,
  "sfx/pop.mp3": 1.0,
  "sfx/bass-hit-futuristic.mp3": 6.2,
  "sfx/sparkle-wand.mp3": 5.0,
  "sfx/lock-digital.mp3": 6.2,
  "sfx/typewriter-hit-single.mp3": 5.1,
  "sfx/data-scan.mp3": 28.2,
  "sfx/sweep-fast.mp3": 13.5,
  "sfx/transition-soft.mp3": 13.0,
  "sfx/pen-write-paper.mp3": 15.4,
  "sfx/marker-pen-line.mp3": 4.1,
  "sfx/shimmer-sparkle-sweep.mp3": 29.4,
  "sfx/whoosh-swirl.mp3": 59.1,
  "sfx/switch-click-quick.mp3": 0.4,
};

// from = 目标峰值帧 − 音源峰值滞后 − 输出偏移（统一公式，绝不逐条手改）
const sfxFrom = (targetPeakF: number, src: string) =>
  Math.max(0, Math.round(targetPeakF - (PEAK_F[src] ?? 0) - OUTPUT_AUDIO_OFFSET_F));

export type Sfx = { from: number; src: string; volume: number; durationInFrames?: number };

// 钉帧表（全部 beatF 拍号锚点；b=beat）
const b = (n: number) => beatF(n);
export const SFX: Sfx[] = [
  // ── S0 品牌开场（b0–b8）────────────────────────────
  { from: sfxFrom(b(0) + 4, "sfx/pen-write-paper.mp3"), src: "sfx/pen-write-paper.mp3", volume: 0.5, durationInFrames: 40 }, // 准星描画
  { from: sfxFrom(b(2) + 6, "sfx/typewriter-hit-single.mp3"), src: "sfx/typewriter-hit-single.mp3", volume: 0.45 }, // 字标压印起
  { from: sfxFrom(b(4) + 2, "sfx/typewriter-hit-single.mp3"), src: "sfx/typewriter-hit-single.mp3", volume: 0.4 }, // 字标压印中
  { from: sfxFrom(b(7) + 2, "sfx/shimmer-sparkle-sweep.mp3"), src: "sfx/shimmer-sparkle-sweep.mp3", volume: 0.35 }, // 副标 typewriter 完成

  // ── S1 orb 主角立传（b8–b26）───────────────────────
  { from: sfxFrom(b(8) + 20, "sfx/whoosh-big.mp3"), src: "sfx/whoosh-big.mp3", volume: 0.5 }, // 相机推近（弹起）
  { from: sfxFrom(b(10) + 5, "sfx/shimmer-sparkle-sweep.mp3"), src: "sfx/shimmer-sparkle-sweep.mp3", volume: 0.3 }, // 轮廓光束 lap1
  { from: sfxFrom(b(18), "sfx/transition-snap.mp3"), src: "sfx/transition-snap.mp3", volume: 0.6 }, // ★slam1 b18 锁定脉冲（全片最响 SFX）
  { from: sfxFrom(b(20), "sfx/whoosh-swirl.mp3"), src: "sfx/whoosh-swirl.mp3", volume: 0.35 }, // 悬停注记浮现
  { from: sfxFrom(b(24) + 8, "sfx/transition-snap.mp3"), src: "sfx/transition-snap.mp3", volume: 0.4 }, // reseat 贴回

  // ── S2 扫描（b26–b44）─────────────────────────────
  { from: sfxFrom(b(26), "sfx/whoosh-big.mp3"), src: "sfx/whoosh-big.mp3", volume: 0.45 }, // 面板弹入 + 场景切入
  { from: sfxFrom(b(27) + 10, "sfx/data-scan.mp3"), src: "sfx/data-scan.mp3", volume: 0.4, durationInFrames: 90 }, // 扫描光带 5 趟（截 3s）

  // ── S3 呼吸字卡1（b44–b50）────────────────────────
  { from: sfxFrom(b(44), "sfx/transition-soft.mp3"), src: "sfx/transition-soft.mp3", volume: 0.4 }, // 字卡切入
  { from: sfxFrom(b(45) + 4, "sfx/marker-pen-line.mp3"), src: "sfx/marker-pen-line.mp3", volume: 0.35 }, // 下划线一笔

  // ── S4 证据审核（b50–b66）─────────────────────────
  { from: sfxFrom(b(50), "sfx/whoosh-big.mp3"), src: "sfx/whoosh-big.mp3", volume: 0.45 }, // 场景切入
  // 分级行逐行压入（pop 双样本交替不存在第二样本 → 音量阶梯递减 + 间隔加速）
  { from: sfxFrom(b(50) + 12, "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.4 },
  { from: sfxFrom(b(51) + 3, "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.36 },
  { from: sfxFrom(b(51) + 9, "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.32 },
  { from: sfxFrom(b(52), "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.28 },
  { from: sfxFrom(b(52) + 5, "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.25 },

  // ── S5 呼吸字卡2（b66–b72）────────────────────────
  { from: sfxFrom(b(66), "sfx/transition-soft.mp3"), src: "sfx/transition-soft.mp3", volume: 0.4 },
  { from: sfxFrom(b(67) + 4, "sfx/marker-pen-line.mp3"), src: "sfx/marker-pen-line.mp3", volume: 0.35 },

  // ── S6 18.9GB 里程表（b72–b84）────────────────────
  { from: sfxFrom(b(72), "sfx/sweep-fast.mp3"), src: "sfx/sweep-fast.mp3", volume: 0.4 }, // 数字登场
  { from: sfxFrom(b(73) + 6, "sfx/riser-cine.mp3"), src: "sfx/riser-cine.mp3", volume: 0.4, durationInFrames: 130 }, // 锁定前 riser 铺垫
  { from: sfxFrom(b(82), "sfx/bass-hit-futuristic.mp3"), src: "sfx/bass-hit-futuristic.mp3", volume: 0.55 }, // ★slam2 b82 锁定
  { from: sfxFrom(b(82) + 3, "sfx/sparkle-wand.mp3"), src: "sfx/sparkle-wand.mp3", volume: 0.35 }, // 锁定 sparkle 余韵

  // ── S7 purge 仪式（b84–b98，breakdown 段）─────────
  { from: sfxFrom(b(84), "sfx/transition-soft.mp3"), src: "sfx/transition-soft.mp3", volume: 0.4 }, // gate 切入
  { from: sfxFrom(b(87) + 2, "sfx/lock-digital.mp3"), src: "sfx/lock-digital.mp3", volume: 0.4 }, // 字节数敲定
  { from: sfxFrom(b(92) + 4, "sfx/switch-click-quick.mp3"), src: "sfx/switch-click-quick.mp3", volume: 0.35 }, // 输入框就位（真实开关拟音）

  // ── S8 呼吸字卡3（b98–b104）───────────────────────
  { from: sfxFrom(b(98), "sfx/transition-soft.mp3"), src: "sfx/transition-soft.mp3", volume: 0.4 },
  { from: sfxFrom(b(99) + 4, "sfx/marker-pen-line.mp3"), src: "sfx/marker-pen-line.mp3", volume: 0.35 },

  // ── S9 台账（b104–b122）───────────────────────────
  { from: sfxFrom(b(104), "sfx/sweep-fast.mp3"), src: "sfx/sweep-fast.mp3", volume: 0.4 }, // 扫描线启动

  // ── S10 发布会收尾（b122–b146）────────────────────
  { from: sfxFrom(b(122) + 2, "sfx/riser-cine.mp3"), src: "sfx/riser-cine.mp3", volume: 0.45, durationInFrames: 160 }, // 合影组装段 riser 铺垫
  // pop 五连发钉在飞入窗口内（每元素落位一响，音量阶梯递减）
  { from: sfxFrom(b(123), "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.4 },
  { from: sfxFrom(b(125), "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.36 },
  { from: sfxFrom(b(127), "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.32 },
  { from: sfxFrom(b(129), "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.28 },
  { from: sfxFrom(b(131), "sfx/pop.mp3"), src: "sfx/pop.mp3", volume: 0.25 },
  { from: sfxFrom(b(132), "sfx/whoosh-swirl.mp3"), src: "sfx/whoosh-swirl.mp3", volume: 0.3 }, // 元素汇聚 swirl

  // ── S10 发布会收尾（b122–b146）────────────────────
  { from: sfxFrom(b(138), "sfx/whoosh-big.mp3"), src: "sfx/whoosh-big.mp3", volume: 0.45 }, // 环收束
  { from: sfxFrom(b(140), "sfx/transition-snap.mp3"), src: "sfx/transition-snap.mp3", volume: 0.45 }, // orb 落位
  { from: sfxFrom(b(142) + 2, "sfx/shimmer-sparkle-sweep.mp3"), src: "sfx/shimmer-sparkle-sweep.mp3", volume: 0.3 }, // 字标滑入光韵

  // ── S11 终章（b146–b170）──────────────────────────
  { from: sfxFrom(b(146) + 4, "sfx/data-scan.mp3"), src: "sfx/data-scan.mp3", volume: 0.3, durationInFrames: 60 }, // 长句→颗粒爆散（故障质感）
  { from: sfxFrom(b(150) + 2, "sfx/shimmer-sparkle-sweep.mp3"), src: "sfx/shimmer-sparkle-sweep.mp3", volume: 0.3 }, // 颗粒凝聚
  { from: sfxFrom(b(164), "sfx/impact-cine-big.mp3"), src: "sfx/impact-cine-big.mp3", volume: 0.6, durationInFrames: 150 }, // ★slam3 b164 字标落定（全片最响时刻）
  { from: sfxFrom(b(164) + 4, "sfx/sparkle-wand.mp3"), src: "sfx/sparkle-wand.mp3", volume: 0.4 }, // slam 后 sparkle（riser→impact→sparkle 句式收口）
  { from: sfxFrom(b(168) + 2, "sfx/transition-soft.mp3"), src: "sfx/transition-soft.mp3", volume: 0.3 }, // CTA 淡入伴音
];

// 渲染层：Sequence 包裹（SFX 表是时间线级资产）
export const sfxSrc = (src: string) => staticFile(`audio/${src}`);
export const FPS_ = FPS;
