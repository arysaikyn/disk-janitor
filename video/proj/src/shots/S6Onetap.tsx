// S6Onetap — odometer-digit-roll：18.9 里程表滚锁（slam b82=局部150）
// 修正：整数位 1/8/9 三位滚轮 + '.' 常驻 + GB 单位淡入；残影速度门控；锁定加深脉冲 8f。
import { AbsoluteFill, Img, useCurrentFrame, interpolate, Easing, staticFile } from "remotion";
import { T, SLAMS } from "../timeline";

const E_OUT = Easing.out(Easing.cubic);
const E_BACK = Easing.out(Easing.back(1.4));

const TARGET = "18.9";
const LINE_H = 210;
const GLIDE_START = 40; // 滚动开始（留足镜头进入呼吸）
const SLIDE_START = 120; // 第 1 位开始减速（锁点=slam b82=局部150：最后位 120+2*9=138 起 6f 弹回=144…最终位锁定贴合 slam）
const SPIN = 10; // 整圈数

export const S6Onetap: React.FC = () => {
  const lf = useCurrentFrame(); // Sequence 局部 0–180
  const lockFrame = SLAMS.onetap - 1085; // 150

  // 位 i（数字位索引：只数数字，'.' 常驻）
  const digitIndex = (i: number) => i; // 1,8,9 → i 0,1,2
  // 里程表：条带 0..9 重复两圈；位 i 停在 SPIN + d_i（rowIdx=SPIN+d_i 的行落在查看窗）
  const digitPos = (i: number): number => {
    const start = GLIDE_START + i * 9;
    const stopStart = SLIDE_START + i * 9;
    const settle = stopStart + 6;
    const target = SPIN + Number(TARGET.replace(".", "")[i]);
    if (lf < start) return 0;
    if (lf < stopStart) {
      const t = (lf - start) / (stopStart - start);
      return E_OUT(t) * target;
    }
    if (lf < settle) {
      const t = (lf - stopStart) / (settle - stopStart);
      return target + 0.5 - E_BACK(t) * 0.5; // +0.5 过冲 → 弹回
    }
    return target;
  };
  const digitSpeed = (i: number) => Math.abs(digitPos(i) - (lf > 0 ? digitPosAt(i, lf - 1) : 0)) > 0.02;

  const slamPulse = interpolate(lf, [lockFrame, lockFrame + 8], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const scale = 1 + 0.035 * slamPulse;

  return (
    <AbsoluteFill style={{ background: T.ink, alignItems: "center", justifyContent: "center" }}>
      <Img src={staticFile("textures/review-full.png")} style={{ position: "absolute", width: 1920, height: 1080, opacity: interpolate(lf, [0, 24], [0.85, 0.1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }} />

      <div style={{ transform: `scale(${scale})`, display: "flex", alignItems: "flex-end", gap: 6 }}>
        {TARGET.split("").map((ch, i) => {
          if (ch === ".") {
            return (
              <span key={i} style={{ fontFamily: T.mono, fontSize: 190, fontWeight: 800, color: T.accent, lineHeight: `${LINE_H}px`, paddingBottom: 8, opacity: interpolate(lf, [GLIDE_START, GLIDE_START + 12], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }}>
                .
              </span>
            );
          }
          const di = digitIndex(TARGET.slice(0, i).replace(/\./g, "").length);
          const pos = digitPos(di);
          const speed = digitSpeed(di);
          const locked = lf >= SLIDE_START + di * 9 + 6;
          return (
            <span key={i} style={{ display: "inline-block", height: LINE_H, overflow: "hidden", position: "relative", width: 112 }}>
              {Array.from({ length: 20 }).map((_, rowIdx) => {
                const y = (rowIdx - pos) * LINE_H;
                if (y < -LINE_H || y > LINE_H * 2) return null;
                const n = rowIdx % 10;
                return (
                  <span
                    key={rowIdx}
                    style={{
                      position: "absolute",
                      left: 0,
                      right: 0,
                      top: y,
                      height: LINE_H,
                      textAlign: "center",
                      lineHeight: `${LINE_H}px`,
                      fontFamily: T.mono,
                      fontSize: 190,
                      fontWeight: 800,
                      fontVariantNumeric: "tabular-nums",
                      color: locked && n === Number(ch) ? mix(T.accent, "#e8edf4", slamPulse) : T.accent,
                    }}
                  >
                    {n}
                  </span>
                );
              })}
            </span>
          );
        })}
        <span style={{ fontFamily: T.mono, fontSize: 190, fontWeight: 800, color: T.text, lineHeight: `${LINE_H}px`, marginLeft: 22, opacity: interpolate(lf, [80, 96], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }}>
          GB
        </span>
      </div>

      {/* 标签条：锁定后淡入 */}
      <div
        style={{
          position: "absolute",
          bottom: 200,
          fontSize: 44,
          color: T.mist,
          opacity: interpolate(lf, [lockFrame + 6, lockFrame + 20], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
          letterSpacing: "0.18em",
        }}
      >
        可回收 · 全部附证据
      </div>

      {/* 锁定脉冲（元素层） */}
      <div style={{ position: "absolute", inset: 0, background: `radial-gradient(circle at 50% 46%, rgba(45,212,191,${0.14 * slamPulse}), transparent 60%)`, pointerEvents: "none" }} />
    </AbsoluteFill>
  );
}

function digitPosAt(i: number, f: number): number {
  const start = GLIDE_START + i * 9;
  const stopStart = SLIDE_START + i * 9;
  const target = SPIN + Number(TARGET.replace(".", "")[i]);
  if (f < start) return 0;
  if (f < stopStart) return E_OUT((f - start) / (stopStart - start)) * target;
  return target;
}

function mix(a: string, b: string, t: number): string {
  const pa = hex(a);
  const pb = hex(b);
  const c = pa.map((v, k) => Math.round(v + (pb[k] - v) * t));
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

function hex(h: string): [number, number, number] {
  const m = h.replace("#", "");
  return [parseInt(m.slice(0, 2), 16), parseInt(m.slice(2, 4), 16), parseInt(m.slice(4, 6), 16)];
}
