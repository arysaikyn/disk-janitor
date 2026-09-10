// S7Purge — PurgeGate 段：直接使用真实截图 modal（不做手搓复刻，Q1）；
// 动效 = PageCam 缓推 + 行带错峰高亮（真实像素裁带，同 S4 手法家族但梯度行高不同）。
import { AbsoluteFill, Img, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { PageCam } from "../PageCam";
import { T } from "../timeline";

const OUT_CUBIC = Easing.out(Easing.cubic);
const F0 = 1265;

// purgegate-full.png 中央 modal 的行带（页面坐标）：标题带 / 字节数带 / orb 带 / 输入带
const BANDS = [
  { y: 300, h: 60, label: "将永久销毁", color: "#f87171" },
  { y: 620, h: 50, label: "其余 72 h 内可恢复", color: "#2dd4bf" },
  { y: 700, h: 40, label: "我理解我正在销毁这些条目", color: "#fbbf24" },
  { y: 760, h: 50, label: "请输入字节数以确认", color: "#5eead4" },
];

export const S7Purge: React.FC = () => {
  const lf = useCurrentFrame(); // 0–210
  const abs = F0 + lf;

  const camKeys = [
    { frame: F0, cx: 960, cy: 540, zoom: 0.9 },
    { frame: F0 + 44, cx: 960, cy: 520, zoom: 1.02 },
  ];

  const byteT = interpolate(lf, [50, 88], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: OUT_CUBIC });

  return (
    <AbsoluteFill style={{ background: T.ink }}>
      <PageCam src="textures/purgegate-full.png" pageH={1080} keys={camKeys} frame={abs}>
        {/* 行带错峰高亮（真实像素裁带 + 色条 + 提亮） */}
        {BANDS.map((b, i) => {
          const t = OUT_CUBIC(interpolate(lf, [60 + i * 16, 82 + i * 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
          return (
            <div
              key={i}
              style={{
                position: "absolute",
                left: 700,
                top: b.y,
                width: 520,
                height: b.h,
                opacity: t,
                transform: `translateY(${(1 - t) * 10}px)`,
                borderRadius: 8,
                boxShadow: `inset 3px 0 0 ${b.color}`,
                background: "rgba(45,212,191,0.06)",
              }}
            >
              <Img
                src={staticFile("textures/purgegate-full.png")}
                style={{ position: "absolute", left: -700, top: -b.y, width: 1920, height: 1080, filter: "brightness(1.25)" }}
              />
            </div>
          );
        })}
      </PageCam>

      {/* 字幕（真实语义：逐位输入字节数） */}
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
          opacity: interpolate(lf, [150, 168], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
        }}
      >
        销毁需要<span style={{ fontFamily: T.mono, color: T.accent }}>逐位输入</span>字节数确认
        <span style={{ display: "none" }}>{byteT}</span>
      </div>
    </AbsoluteFill>
  );
};
