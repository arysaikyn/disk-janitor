// S10Finale — logo-shrink-wordmark-lockup（发布会合影，Q8）
// 修正：环收束 clamp（不再无界放大）；元素全部落 lockup 槽位；b138 结构拍落位；字标 hold ≥45f。
import { AbsoluteFill, Img, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { T } from "../timeline";

const OUT_CUBIC = Easing.out(Easing.cubic);

type Flyer = {
  src: string | null;
  from: [number, number];
  to: [number, number];
  size: [number, number];
  delay: number;
  glyph?: string;
};

const FLYERS: Flyer[] = [
  { src: null, glyph: "纯缓存 13.6 GB", from: [-400, 200], to: [560, 320], size: [280, 56], delay: 8 },
  { src: null, glyph: "请勿动手", from: [2320, 320], to: [1080, 290], size: [200, 56], delay: 14 },
  { src: null, glyph: "1.21 GiB/s", from: [-360, 900], to: [600, 660], size: [260, 56], delay: 20 },
  { src: null, glyph: "19.3 GB", from: [2280, 860], to: [1060, 690], size: [220, 56], delay: 26 },
  { src: null, glyph: "72 h", from: [960, -160], to: [890, 170], size: [140, 56], delay: 32 },
  { src: null, glyph: "0 已销毁", from: [960, 1240], to: [850, 850], size: [200, 56], delay: 38 },
];

const ORB = { x: 864, y: 360, w: 192, h: 192 };
const BRAND = { y: 660 };

export const S10Finale: React.FC = () => {
  const lf = useCurrentFrame(); // 0–360

  const collapseAt = 240;
  const collapse = interpolate(lf, [collapseAt, collapseAt + 40], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.in(Easing.cubic) });
  const brake = interpolate(lf, [collapseAt + 40, collapseAt + 52], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const ringScale = collapse < 1 ? 1 + (1 - collapse) * 4.4 : 1 + Math.sin(Math.PI * brake) * 0.06;

  const brand = "还我磁盘";

  return (
    <AbsoluteFill style={{ background: `radial-gradient(1200px 700px at 50% 46%, #0d1420, ${T.ink})`, alignItems: "center", justifyContent: "center" }}>
      <div style={{ position: "absolute", inset: 0, background: "radial-gradient(900px 420px at 50% -80px, rgba(45,212,191,0.10), transparent 70%)" }} />

      {/* 合影飞入元素 */}
      {FLYERS.map((fl, i) => {
        const t = OUT_CUBIC(interpolate(lf, [fl.delay, fl.delay + 34], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
        const x = fl.from[0] + (fl.to[0] - fl.from[0]) * t;
        const y = fl.from[1] + (fl.to[1] - fl.from[1]) * t;
        return (
          <div key={i} style={{ position: "absolute", left: x, top: y, opacity: t, transform: `scale(${0.9 + 0.1 * t})` }}>
            {fl.src ? (
              <Img src={staticFile(fl.src)} style={{ width: fl.size[0], height: fl.size[1], borderRadius: 6, boxShadow: "0 8px 28px rgb(0 0 0 / 0.4)" }} />
            ) : (
              <div style={{ height: fl.size[1], padding: "0 24px", display: "flex", alignItems: "center", borderRadius: 10, background: T.surface, border: `1px solid ${T.hairline}`, fontFamily: T.mono, fontSize: 30, color: T.accent }}>
                {fl.glyph}
              </div>
            )}
          </div>
        );
      })}

      {/* teal 环 → orb（收束主角） */}
      <div style={{ position: "absolute", left: ORB.x, top: ORB.y, width: ORB.w, height: ORB.h, transform: `scale(${ringScale})` }}>
        {collapse < 1 ? (
          <div
            style={{
              position: "absolute",
              inset: 0,
              borderRadius: "50%",
              border: `${Math.max(2, 14 * (1 - collapse))}px solid ${T.accent}`,
              boxShadow: `0 0 ${40 + 120 * (1 - collapse)}px rgba(45,212,191,${0.25 + 0.4 * (1 - collapse)})`,
            }}
          />
        ) : null}
        <div
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: "50%",
            background: `radial-gradient(circle at 38% 32%, ${T.accentHover}, ${T.accentPressed})`,
            opacity: collapse,
            boxShadow: "0 0 60px rgba(45,212,191,0.5)",
          }}
        />
      </div>

      {/* 字标 lockup */}
      <div style={{ position: "absolute", left: 0, right: 0, top: BRAND.y, textAlign: "center", display: "flex", justifyContent: "center", gap: 4 }}>
        {brand.split("").map((ch, i) => {
          const t = OUT_CUBIC(interpolate(lf, [300 + i * 3, 314 + i * 3], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
          return (
            <span key={i} style={{ fontSize: 92, fontWeight: 700, color: T.text, opacity: t, transform: `translateY(${(1 - t) * 26}px)` }}>
              {ch}
            </span>
          );
        })}
      </div>
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: BRAND.y + 130,
          textAlign: "center",
          fontFamily: T.mono,
          fontSize: 30,
          color: T.mist,
          letterSpacing: "0.3em",
          opacity: interpolate(lf, [330, 348], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
        }}
      >
        DISK JANITOR
      </div>
    </AbsoluteFill>
  );
};
