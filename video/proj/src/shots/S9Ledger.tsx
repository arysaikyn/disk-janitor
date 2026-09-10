// S9Ledger — scanline-assemble-flyin：台账行扫描组装（真实像素裁带 + 扫描线）
import { AbsoluteFill, Img, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { PageCam } from "../PageCam";
import { T } from "../timeline";

const F0 = 1565;
const LIST = { x: 340, y: 150, w: 1240, h: 800 };

// 台账 5 行的真实 y（ledger-full.png：#3≈118 #4≈168 #5≈208 销毁≈278 恢复≈352）
const ROW_YS = [108, 158, 198, 268, 342];
export const S9Ledger: React.FC = () => {
  const lf = useCurrentFrame(); // 0–270
  const abs = F0 + lf;

  const camKeys = [
    { frame: F0, cx: 960, cy: 540, zoom: 0.96 },
    { frame: F0 + 36, cx: 960, cy: 520, zoom: 1.0 },
  ];

  // 扫描线匀速下行；行在过线时 clip-reveal
  const lineY = interpolate(lf, [20, 216], [90, 400], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.inOut(Easing.sin) });

  return (
    <AbsoluteFill style={{ background: T.ink }}>
      <PageCam src="textures/ledger-full.png" pageH={1080} keys={camKeys} frame={abs}>
        {/* 行 clip-reveal（扫描线以上揭示） */}
        {ROW_YS.map((ry, i) => {
          const delay = 20 + ((ry - 90) / 310) * 190;
          const t = Easing.out(Easing.cubic)(interpolate(lf, [delay, delay + 22], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
          return (
            <div key={i} style={{ position: "absolute", left: LIST.x, top: ry - 14, width: LIST.w, height: 110, overflow: "hidden" }}>
              <Img
                src={staticFile("textures/ledger-full.png")}
                style={{
                  position: "absolute",
                  left: -LIST.x,
                  top: -(ry - 14),
                  width: 1920,
                  height: 1080,
                  clipPath: `inset(0 0 ${(1 - t) * 100}% 0)`,
                  transform: `translateY(${(1 - t) * 14}px)`,
                }}
              />
            </div>
          );
        })}

        {/* 扫描光带（页面空间） */}
        <div
          style={{
            position: "absolute",
            left: LIST.x,
            top: lineY,
            width: LIST.w,
            height: 2,
            background: `linear-gradient(90deg, transparent, ${T.accent} 18%, ${T.accentHover} 82%, transparent)`,
            boxShadow: "0 0 14px rgba(45,212,191,0.6)",
            opacity: interpolate(lf, [16, 28, 202, 216], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
          }}
        />
      </PageCam>

      {/* 字幕 */}
      <div
        style={{
          position: "absolute",
          bottom: 84,
          left: 0,
          right: 0,
          textAlign: "center",
          fontSize: 54,
          fontWeight: 600,
          color: T.text,
          opacity: interpolate(lf, [190, 206], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
        }}
      >
        隔离 · 销毁 · 恢复 —— <span style={{ color: T.accent }}>每一笔都记在台账</span>
      </div>
    </AbsoluteFill>
  );
};
