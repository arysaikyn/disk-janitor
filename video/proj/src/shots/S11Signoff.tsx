// S11Signoff — grain-dissolve 适配：长句→颗粒爆散→凝聚「你值得你的空间」→slam b164→仓库 URL→hold
// 卡片: references/shots/outro/grain-dissolve.md · demo: demos/outro/grain-dissolve/GrainDissolve.tsx
// 铁律：种子逐帧换（seed=floor(t*46)）确定性；slam b164=局部 270；尾段静止 ≥45f；URL 是全片最不该小的一行。
import { AbsoluteFill, useCurrentFrame, interpolate, Easing, random } from "remotion";
import { T, SLAMS } from "../timeline";

const OUT_CUBIC = Easing.out(Easing.cubic);

const SENTENCE = "把你的磁盘还给你";
const WORDMARK = "你值得你的空间";
const REPO = "源码 · github.com/arysaikyn/disk-janitor";

// 颗粒场：80 粒，mulberry 经 random(seed) 派生（确定性）
const GRAINS = 80;

export const S11Signoff: React.FC = () => {
  const lf = useCurrentFrame(); // Sequence 局部 0–360
  const slam = SLAMS.signoff - 2195; // 270

  // 阶段1（0–60）：长句清晰可读
  // 阶段2（60–84）：feTurbulence 爆散（burst 0→1 outCubic）
  const burst = OUT_CUBIC(interpolate(lf, [60, 84], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const coag = OUT_CUBIC(interpolate(lf, [230, 268], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const sentenceOp = (1 - burst) * (lf < 84 ? 1 : 0);
  const wordOp = coag;
  const slamPulse = interpolate(lf, [slam, slam + 8], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const urlOp = interpolate(lf, [290, 310], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });

  const seedBase = Math.floor(lf * 1.53); // 逐帧换种子（46/30≈1.53）
  const grains = Array.from({ length: GRAINS }).map((_, i) => {
    const rx = random(`g${i}-x-${seedBase}`);
    const ry = random(`g${i}-y-${seedBase}`);
    // 爆散位：句区外扩；凝聚位：字标 bbox 内
    const scatter = { x: 960 + (rx - 0.5) * 2400 * burst, y: 540 + (ry - 0.5) * 1400 * burst };
    const target = { x: 960 + (rx - 0.5) * 900, y: 540 + (ry - 0.5) * 260 };
    const x = scatter.x + (target.x - scatter.x) * coag;
    const y = scatter.y + (target.y - scatter.y) * coag;
    const op = Math.max(burst * (1 - coag), coag * 0.85) * 0.8;
    return { x, y, op, s: 2 + rx * 3 };
  });

  return (
    <AbsoluteFill style={{ background: T.ink, alignItems: "center", justifyContent: "center" }}>
      {/* 长句（阶段1） */}
      <div
        style={{
          position: "absolute",
          fontSize: 84,
          fontWeight: 700,
          color: T.text,
          opacity: sentenceOp,
          filter: `blur(${burst * 6}px)`,
          transform: `scale(${1 + burst * 0.04})`,
        }}
      >
        {SENTENCE}
      </div>

      {/* SVG 颗粒（爆散→凝聚） */}
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0 }}>
        {grains.map((g, i) => (
          <circle key={i} cx={g.x} cy={g.y} r={g.s} fill={i % 7 === 0 ? T.accent : T.mist} opacity={g.op} />
        ))}
      </svg>

      {/* 字标（凝聚结果） */}
      <div
        style={{
          position: "absolute",
          display: "flex",
          gap: 4,
          opacity: wordOp,
          transform: `scale(${(0.94 + 0.06 * coag) * (1 + slamPulse * 0.035)})`,
          filter: slamPulse > 0 ? `brightness(${1 + slamPulse * 0.5})` : undefined,
        }}
      >
        {WORDMARK.split("").map((ch, i) => (
          <span
            key={i}
            style={{
              fontSize: 132,
              fontWeight: 700,
              color: i === 2 || i === 5 ? T.accent : T.text,
              textShadow: coag >= 1 ? "0 0 34px rgba(45,212,191,0.45)" : "none",
            }}
          >
            {ch}
          </span>
        ))}
      </div>

      {/* CTA：品牌句下 · 仓库 URL（全片最不该小的一行：56px ≥ 字幕线） */}
      <div
        style={{
          position: "absolute",
          bottom: 130,
          left: 0,
          right: 0,
          textAlign: "center",
          fontFamily: T.mono,
          fontSize: 44,
          color: T.mist,
          opacity: urlOp,
        }}
      >
        {REPO}
      </div>
      <div
        style={{
          position: "absolute",
          bottom: 74,
          left: 0,
          right: 0,
          textAlign: "center",
          fontSize: 34,
          color: T.mist,
          opacity: urlOp * 0.85,
          letterSpacing: "0.2em",
        }}
      >
        零遥测 · 无账号 · 扫描只读元数据
      </div>

      {/* slam 暗角脉冲（元素层） */}
      <div style={{ position: "absolute", inset: 0, background: `radial-gradient(circle at 50% 50%, rgba(45,212,191,${0.12 * slamPulse}), transparent 62%)`, pointerEvents: "none" }} />
    </AbsoluteFill>
  );
};
