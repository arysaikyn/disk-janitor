// S1Hero — spotlight-hero-card，PageCam 承载（Q2 栅格化链路：CSS zoom 布局级放大）
// 卡片: references/shots/opening/spotlight-hero-card.md · demo: demos/opening/spotlight-hero-card/SpotlightHeroCard.tsx
// 相机站（绝对帧）：125 全页 → 152 推进至 orb 左侧特写（rotY 34 主导，Q6）→ 200 reseat → 215+ 全锁死
import { AbsoluteFill, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { PageCam } from "../PageCam";
import { T, SLAMS } from "../timeline";

const ORB = { x: 228, y: 197, w: 96, h: 96 };
const ORB_CX = ORB.x + ORB.w / 2; // 276
const ORB_CY = ORB.y + ORB.h / 2; // 245
const F0 = 125;

// 聚光灯中间站（页面坐标）
const STATIONS = [
  { x: 1250, y: 300 },
  { x: 520, y: 700 },
  { x: 1400, y: 760 },
  { x: 700, y: 240 },
];

export const S1Hero: React.FC = () => {
  const lf = useCurrentFrame(); // Sequence 内 = 局部 0–270
  const abs = F0 + lf;

  // 悬停/reseat 逻辑（页面空间，作用于 orb 覆盖层）
  const liftT = interpolate(lf, [62, 74], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) });
  const reseatT = interpolate(lf, [230, 248], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) });
  const bob = lf > 74 && lf < 230 ? Math.sin(((lf - 74) / 39) * Math.PI * 2) * 4 : 0;
  const lift = (lf > 62 ? liftT : 0) * (1 - reseatT);

  const lockBeatLocal = SLAMS.hero - F0; // 150
  const lockPulse = 1 + 0.06 * Math.max(0, 1 - Math.abs(lf - lockBeatLocal) / 8);

  // 聚光灯插值（8→62f 锁定 orb 心）
  const spotProg = interpolate(lf, [8, 62], [0, STATIONS.length - 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const si = Math.min(Math.floor(spotProg), STATIONS.length - 2);
  const mixv = spotProg - si;
  const sx = STATIONS[si].x + (STATIONS[si + 1].x - STATIONS[si].x) * mixv;
  const sy = STATIONS[si].y + (STATIONS[si + 1].y - STATIONS[si].y) * mixv;
  const lx = sx + (ORB_CX - sx) * interpolate(lf, [48, 62], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const ly = sy + (ORB_CY - sy) * interpolate(lf, [48, 62], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const poolR = interpolate(lf, [8, 62], [620, 360], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const vign = interpolate(lf, [8, 62], [0.16, 0.44], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });

  // 轮廓光束（主角一次，两圈快慢）
  const lap1 = interpolate(lf, [27, 41], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const lap2 = interpolate(lf, [47, 67], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });

  const camKeys = [
    { frame: 125, cx: 960, cy: 460, zoom: 0.92 },
    { frame: 152, cx: ORB_CX - 30, cy: ORB_CY + 6, zoom: 3.0, rotX: 7, rotY: 32, persp: 1200 },
    { frame: 218, cx: ORB_CX - 30, cy: ORB_CY + 6, zoom: 3.0, rotX: 7, rotY: 32, persp: 1200 },
    { frame: 260, cx: 960, cy: 460, zoom: 0.95 },
  ];

  return (
    <AbsoluteFill style={{ background: T.ink }}>
      <PageCam
        src="textures/dashboard-full.png"
        pageH={1080}
        keys={camKeys}
        frame={abs}
        ease={Easing.bezier(0.33, 0, 0.15, 1)}
      >
        {/* orb 4x 原位覆盖（Q2 高清链路）+ 悬浮抬升（页面空间） */}
        <img
          src={staticFile("textures/dashboard-orb.png")}
          style={{
            position: "absolute",
            left: ORB.x,
            top: ORB.y - lift * 110 - bob,
            width: ORB.w,
            height: ORB.h,
            transform: `scale(${lockPulse})`,
            transformOrigin: "center",
            borderRadius: "50%",
            boxShadow: lift > 0.1 ? `0 ${8 * lift}px ${24 * lift}px rgba(0,0,0,0.5), 0 ${46 * lift}px ${90 * lift}px rgba(45,212,191,${0.12 * lift})` : undefined,
            opacity: lift > 0.05 ? 1 : interpolate(lf, [46, 52], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
          }}
        />
        {/* 原位补丁（orb 飞起后页面露底） */}
        {lift > 0.05 ? (
          <div style={{ position: "absolute", left: ORB.x, top: ORB.y, width: ORB.w, height: ORB.h, borderRadius: "50%", background: "#0a0e14", boxShadow: `inset 0 0 0 2px rgba(45,212,191,${0.4 * lift})` }} />
        ) : null}
      </PageCam>

      {/* 轮廓光束（屏幕空间 SVG，跟随特写构图） */}
      <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
        {lap1 > 0 && lap1 < 1 ? <CircleDash t={lap1} cx={742} cy={470} r={86} color={T.accentHover} width={5} opacity={0.9} /> : null}
        {lap2 > 0 && lap2 < 1 ? <CircleDash t={lap2} cx={742} cy={470} r={102} color={T.accent} width={3} opacity={0.55} /> : null}
      </svg>

      {/* 聚光灯光池 + vignette（屏幕空间，中心跟随推进构图） */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          background: `radial-gradient(circle ${poolR}px at ${interpolate(lf, [0, 62], [960, 742], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px ${interpolate(lf, [0, 62], [540, 470], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}px, rgba(45,212,191,0.10), rgba(10,14,20,${vign}) 72%)`,
          pointerEvents: "none",
        }}
      />

      {/* 3D 悬空注记（C3 同相机透视；跟随 orb 特写屏幕位） */}
      {lift > 0.05 && reseatT < 0.5 ? (
        <div
          style={{
            position: "absolute",
            left: interpolate(lf, [62, 74], [480, 640], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.out(Easing.cubic) }),
            top: 120,
            opacity: Math.min(lift * 2, 1) * (1 - reseatT * 2),
          }}
        >
          <div style={{ fontSize: 58, fontWeight: 700, color: T.text, lineHeight: 1.35 }}>
            呼吸的<span style={{ background: "linear-gradient(transparent 62%, oklch(88% 0.095 165) 62%)" }}>可回收空间</span>
          </div>
          <div style={{ fontFamily: T.mono, fontSize: 34, color: T.mist, marginTop: 10 }}>18.9 GB · 0 已销毁</div>
        </div>
      ) : null}
    </AbsoluteFill>
  );
};

const CircleDash: React.FC<{ t: number; cx: number; cy: number; r: number; color: string; width: number; opacity: number }> = ({ t, cx, cy, r, color, width, opacity }) => {
  const circ = 2 * Math.PI * r;
  return (
    <circle
      cx={cx}
      cy={cy}
      r={r}
      fill="none"
      stroke={color}
      strokeWidth={width}
      strokeDasharray={`${t * circ} ${circ}`}
      transform={`rotate(-90 ${cx} ${cy})`}
      opacity={opacity}
    />
  );
};
