// S4Evidence — list-stack-press：审核分级行逐行压入（行素材 = 需要你判断 2 行 + 可安全回收摘要行）
// 行 cutout 坐标来自 layout.json review.rowCutouts（真实 bbox），素材裁自 review-full.png 全页纹理。
import { AbsoluteFill, Img, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { T } from "../timeline";

const OUT_CUBIC = Easing.out(Easing.cubic);
const F0 = 755;

// 5 行：可安全回收摘要行（y≈268）+ 判断区 2 行（y≈475/510）——统一取自页面纹理的行带
const ROWS = [
  { y: 262, h: 44, label: "纯缓存 · 4 候选", color: "#34d399" },
  { y: 462, h: 44, label: "代价高 · 4.9 GB", color: "#fbbf24" },
  { y: 498, h: 44, label: "请勿动手 · 正在写入", color: "#f87171" },
  { y: 330, h: 30, label: "可重建 · DerivedData 6.8 GB", color: "#2dd4bf" },
  { y: 364, h: 30, label: "可重下载 · old-installers 1.5 GB", color: "#60a5fa" },
];

export const S4Evidence: React.FC = () => {
  const lf = useCurrentFrame(); // 0–240

  return (
    <AbsoluteFill style={{ background: T.ink }}>
      {/* 页面底（已含展开态） */}
      <Img src={staticFile("textures/review-full.png")} style={{ position: "absolute", width: 1920, height: 1080, opacity: 0.88 }} />

      {/* 行带高亮压入：从页面纹理裁出的行带 + grade 色条 + 轻投影（真实像素，非重绘） */}
      {ROWS.map((r, i) => {
        const t = OUT_CUBIC(interpolate(lf, [10 + i * 18, 36 + i * 18], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              left: 200,
              top: r.y,
              width: 1520,
              height: r.h,
              opacity: t,
              transform: `translateY(${(1 - t) * -26}px)`,
              borderRadius: 8,
              overflow: "hidden",
              boxShadow: `0 2px 8px rgb(0 0 0 / 0.32), inset 3px 0 0 ${r.color}`,
              background: "#111721cc",
            }}
          >
            <Img
              src={staticFile("textures/review-full.png")}
              style={{ position: "absolute", left: -200, top: -r.y, width: 1920, height: 1080 }}
            />
          </div>
        );
      })}

      {/* 字幕（真实 UI 语义） */}
      <div
        style={{
          position: "absolute",
          bottom: 84,
          left: 0,
          right: 0,
          textAlign: "center",
          fontSize: 56,
          fontWeight: 600,
          color: T.text,
          opacity: interpolate(lf, [110, 126], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
        }}
      >
        每一项判定，都附<span style={{ color: T.accent }}>证据链</span>
      </div>
    </AbsoluteFill>
  );
};
