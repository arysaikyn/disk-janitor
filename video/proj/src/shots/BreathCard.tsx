// BreathCard — 呼吸字卡（3s，落定静止 ≥42f；手搓品牌段准许）
// 文案 = 产品真实语义（C2 具体）：证据/可逆/台账
import { AbsoluteFill, useCurrentFrame, interpolate, Easing } from "remotion";
import { T } from "../timeline";

const OUT_CUBIC = Easing.out(Easing.cubic);

export const BreathCard: React.FC<{ line: string }> = ({ line }) => {
  const f = useCurrentFrame();
  const chars = line.split("");
  // 单词压入：0–24f 逐字 blur+scale 收敛（一次收敛，无二次冲击）
  // 落定后静止到 90f（3s 镜头内 hold ≥42f）
  const underline = OUT_CUBIC(interpolate(f, [30, 44], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));

  return (
    <AbsoluteFill style={{ background: T.ink, alignItems: "center", justifyContent: "center" }}>
      <div style={{ display: "flex", alignItems: "baseline" }}>
        {chars.map((ch, i) => {
          const t = OUT_CUBIC(interpolate(f, [i * 2.2, i * 2.2 + 12], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
          return (
            <span
              key={i}
              style={{
                fontSize: 88,
                fontWeight: 700,
                color: T.text,
                letterSpacing: "0.04em",
                opacity: t,
                filter: `blur(${(1 - t) * 8}px)`,
                transform: `translateY(${(1 - t) * 18}px)`,
              }}
            >
              {ch}
            </span>
          );
        })}
      </div>
      {/* teal 下划线：一笔点睛（marker-underline 思路，非重复手法主角） */}
      <div
        style={{
          marginTop: 26,
          height: 5,
          width: `${line.length * 88 * underline * 0.62}px`,
          maxWidth: `${line.length * 88 * 0.62}px`,
          borderRadius: 3,
          background: `linear-gradient(90deg, ${T.accent}, ${T.accentHover})`,
          boxShadow: "0 0 18px rgba(45,212,191,0.4)",
        }}
      />
    </AbsoluteFill>
  );
};
