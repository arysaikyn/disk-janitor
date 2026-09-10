// S0Open — brand-ink-open 适配：准星描画 →「还我磁盘」逐字压印 → 副标 → hold 30f
// 卡片: references/shots/opening/brand-ink-open.md · demo: demos/typography/brand-ink-open/BrandInkOpen.tsx
import { AbsoluteFill, useCurrentFrame, interpolate } from "remotion";
import { T } from "../timeline";

const OUT_CUBIC = (x: number) => 1 - Math.pow(1 - x, 3);

export const S0Open: React.FC = () => {
  const f = useCurrentFrame();
  // 准星: 0–26f 描画（竖→横），28–40f 淡出
  const crossV = OUT_CUBIC(interpolate(f, [2, 14], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const crossH = OUT_CUBIC(interpolate(f, [10, 24], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const crossFade = interpolate(f, [46, 58], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  // 字标逐字压印: 34f 起，每字 4f，压印=blur 6→0 + scale 1.06→1 + opacity
  const brand = "还我磁盘";
  const brandDone = 34 + brand.length * 4;
  // 副标: 字标落定后 46f 起 typewriter
  const sub = "你值得你的空间";
  const subStart = brandDone + 12; // 92
  // 落定 hold: 全部文本在 f≈120 就位 → 到 125 切镜，副标出现后静止 ≥30f 由下一镜边界保证
  const subChars = Math.floor(OUT_CUBIC(interpolate(f, [subStart, subStart + 18], [0, sub.length], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })));

  return (
    <AbsoluteFill style={{ background: T.ink, alignItems: "center", justifyContent: "center" }}>
      {/* 准星 SVG（十字 + 双圈，hairline 墨线） */}
      <svg
        width={560}
        height={560}
        viewBox="0 0 560 560"
        style={{ position: "absolute", opacity: crossFade }}
      >
        <circle cx={280} cy={280} r={210} fill="none" stroke={T.hairline} strokeWidth={1.5} />
        <circle cx={280} cy={280} r={150} fill="none" stroke={T.hairline} strokeWidth={1} />
        <line x1={280} y1={40} x2={280} y2={40 + 480 * crossV} stroke={T.accent} strokeWidth={1.5} opacity={0.7} />
        <line x1={40} y1={280} x2={40 + 480 * crossH} y2={280} stroke={T.accent} strokeWidth={1.5} opacity={0.7} />
      </svg>
      {/* 字标 */}
      <div style={{ display: "flex", gap: 6 }}>
        {brand.split("").map((ch, i) => {
          const t = interpolate(f, [34 + i * 4, 34 + i * 4 + 10], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: OUT_CUBIC });
          return (
            <span
              key={i}
              style={{
                fontSize: 148,
                fontWeight: 700,
                color: T.text,
                letterSpacing: "0.06em",
                opacity: t,
                filter: `blur(${(1 - t) * 6}px)`,
                transform: `scale(${1 + (1 - t) * 0.06})`,
              }}
            >
              {ch}
            </span>
          );
        })}
      </div>
      {/* 副标 typewriter（mono mist，字距拉开） */}
      <div
        style={{
          marginTop: 34,
          fontFamily: T.mono,
          fontSize: 44,
          color: T.accent,
          letterSpacing: "0.42em",
          minHeight: 60,
          textShadow: "0 0 24px rgba(45,212,191,0.35)",
        }}
      >
        {sub.slice(0, subChars)}
        <span style={{ opacity: f % 30 < 15 && subChars < sub.length ? 0.9 : 0 }}>▍</span>
      </div>
      {/* 底部微标签：产品定位锚 */}
      <div style={{ position: "absolute", bottom: 90, fontSize: 26, color: T.mist, letterSpacing: "0.3em", opacity: interpolate(f, [100, 116], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }}>
        还我磁盘 · DISK JANITOR
      </div>
    </AbsoluteFill>
  );
};
