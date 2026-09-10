// S2Scan — scan-bracket-sweep（PageCam 承载 + 面板 bbox 括号/光带）
// 命门：文档全程静止；光带裁进同圆角层；拖尾方向随扫描方向翻转（top 与 gradient 同改）。
import { AbsoluteFill, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { PageCam } from "../PageCam";
import { T } from "../timeline";

const OUT_CUBIC = Easing.out(Easing.cubic);
const seg = (f: number, a: number, b: number) => Math.max(0, Math.min(1, (f - a) / (b - a)));

// 扫描面板 bbox（scan-full.png: 主面板区域，页面坐标）
const PANEL = { x: 320, y: 120, w: 1280, h: 830 };
const R = 16;
const F0 = 395;

export const S2Scan: React.FC = () => {
  const lf = useCurrentFrame(); // 0–270
  const abs = F0 + lf;

  // 相机：全页 → 缓推面板（中低能量镜，正视 Q6）
  const camKeys = [
    { frame: F0, cx: 960, cy: 540, zoom: 0.94 },
    { frame: F0 + 40, cx: 960, cy: 520, zoom: 1.0 },
  ];

  // 面板弹入（页面空间 overlay，静止文档本身不动：动的是入场）
  const pop = OUT_CUBIC(seg(lf, 0, 30));

  // 5 趟往复（局部 50→255），趟内 inOutSine
  const PASSES = 5;
  const sweepRaw = seg(lf, 50, 255);
  const pass = Math.min(Math.floor(sweepRaw * PASSES), PASSES - 1);
  const local = sweepRaw * PASSES - pass;
  const inOutSine = (t: number) => 0.5 - 0.5 * Math.cos(Math.PI * t);
  const lv = inOutSine(local);
  const dir = pass % 2 === 0 ? 1 : -1;
  const bandY = (dir === 1 ? lv * PANEL.h : PANEL.h - lv * PANEL.h);
  const bandOpacity = interpolate(lf, [50, 60, 246, 256], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });

  // 括号 stagger
  const arm = 34;
  const corners = [
    { x: 0, y: 0 }, { x: 1, y: 0 }, { x: 0, y: 1 }, { x: 1, y: 1 },
  ];

  return (
    <AbsoluteFill style={{ background: T.ink }}>
      <PageCam src="textures/scan-full.png" pageH={1080} keys={camKeys} frame={abs}>
        {/* 面板框：空 backplate 效果 = 底色圆角面板（弹出）+ 截图内容裁进同圆角 */}
        <div
          style={{
            position: "absolute",
            left: PANEL.x,
            top: PANEL.y,
            width: PANEL.w,
            height: PANEL.h,
            borderRadius: R,
            overflow: "hidden",
            transform: `scale(${0.86 + 0.14 * pop})`,
            transformOrigin: "center",
            opacity: Math.min(1, seg(lf, 0, 10) * 3),
            boxShadow: "0 4px 16px rgb(0 0 0 / 0.36), 0 16px 56px rgb(0 0 0 / 0.3)",
            background: T.surface,
          }}
        >
          <img
            src={staticFile("textures/scan-full.png")}
            style={{ position: "absolute", left: -PANEL.x, top: -PANEL.y, width: 1920, height: 1080 }}
          />
          {/* 光带 + 拖尾（裁进同圆角层；top 与 gradient 方向随 dir 同步翻转） */}
          <div style={{ position: "absolute", inset: 0, opacity: bandOpacity }}>
            <div
              style={{
                position: "absolute",
                left: 0,
                right: 0,
                top: dir === 1 ? bandY - 82 : bandY + 2.5,
                height: 82,
                background:
                  dir === 1
                    ? "linear-gradient(to bottom, rgba(45,212,191,0) 0%, rgba(45,212,191,0.5) 100%)"
                    : "linear-gradient(to top, rgba(45,212,191,0) 0%, rgba(45,212,191,0.5) 100%)",
              }}
            />
            <div style={{ position: "absolute", left: 0, right: 0, top: bandY, height: 2.5, background: T.accentHover, boxShadow: "0 0 12px rgba(45,212,191,0.8)" }} />
          </div>
        </div>

        {/* 四角 L 括号（页面空间） */}
        {corners.map((c, i) => {
          const p = OUT_CUBIC(seg(lf, 12 + i * 6.6, 12 + i * 6.6 + 16.5));
          if (p <= 0) return null;
          const off = (1 - p) * 8;
          const px = PANEL.x + (c.x ? PANEL.w : 0) + (c.x ? 10 : -10) + (c.x ? off : -off);
          const py = PANEL.y + (c.y ? PANEL.h : 0) + (c.y ? 10 : -10) + (c.y ? off : -off);
          const sx = c.x ? -1 : 1;
          const sy = c.y ? -1 : 1;
          return (
            <svg key={i} width={arm + 4} height={arm + 4} style={{ position: "absolute", left: px - (sx < 0 ? arm : 0), top: py - (sy < 0 ? arm : 0), overflow: "visible", opacity: p }}>
              <path d={`M ${sx < 0 ? 0 : arm} 0 L ${sx < 0 ? 0 : arm} 0`} stroke="none" />
              <path
                d={sx > 0 && sy > 0 ? `M 0 ${arm} L 0 0 L ${arm} 0` : sx < 0 && sy > 0 ? `M ${arm} ${arm} L ${arm} 0 L 0 0` : sx > 0 && sy < 0 ? `M 0 0 L 0 ${arm} L ${arm} ${arm}` : `M ${arm} 0 L ${arm} ${arm} L 0 ${arm}`}
                stroke={T.accent}
                strokeWidth={2}
                fill="none"
                transform={`translate(${sx < 0 ? 0 : 0}, 0)`}
              />
            </svg>
          );
        })}
      </PageCam>

      {/* 字幕（真实 UI 文案） */}
      <div style={{ position: "absolute", bottom: 86, left: 0, right: 0, textAlign: "center", opacity: interpolate(lf, [40, 55], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }}>
        <span style={{ fontSize: 56, fontWeight: 600, color: T.text }}>
          每秒 <span style={{ fontFamily: T.mono, color: T.accent }}>1.21 GiB</span> · 只读元数据
        </span>
      </div>
    </AbsoluteFill>
  );
};
