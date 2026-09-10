// DiskJanitorPromo — 主合成：11 镜串联 + BGM 包裹（inputProp bgm 解耦）
import { AbsoluteFill, Audio, Sequence, staticFile } from "remotion";
import { SHOTS, T, TOTAL_FRAMES, FPS, WIDTH, HEIGHT } from "./timeline";
import { SFX, sfxSrc } from "./sfx";
export { FPS, WIDTH, HEIGHT, TOTAL_FRAMES };
import { S0Open } from "./shots/S0Open";
import { S1Hero } from "./shots/S1Hero";
import { S2Scan } from "./shots/S2Scan";
import { S4Evidence } from "./shots/S4Evidence";
import { S6Onetap } from "./shots/S6Onetap";
import { S7Purge } from "./shots/S7Purge";
import { S9Ledger } from "./shots/S9Ledger";
import { S10Finale } from "./shots/S10Finale";
import { S11Signoff } from "./shots/S11Signoff";
import { BreathCard } from "./shots/BreathCard";

type PromoProps = { bgm: boolean; vertical?: boolean };

const CARDS = [
  { shot: "s3_card1", line: "证据，而不是名字。" },
  { shot: "s5_card2", line: "删掉的，都能回来。" },
  { shot: "s8_card3", line: "每一笔，都有台账。" },
] as const;

export const DiskJanitorPromo: React.FC<PromoProps> = ({ bgm }) => {
  return (
    <AbsoluteFill style={{ background: T.ink, fontFamily: T.sans }}>
      <Sequence from={SHOTS.s0_open.from} durationInFrames={SHOTS.s0_open.to - SHOTS.s0_open.from}>
        <S0Open />
      </Sequence>
      <Sequence from={SHOTS.s1_hero.from} durationInFrames={SHOTS.s1_hero.to - SHOTS.s1_hero.from} name="s1_hero">
        <S1Hero />
      </Sequence>
      <Sequence from={SHOTS.s2_scan.from} durationInFrames={SHOTS.s2_scan.to - SHOTS.s2_scan.from} name="s2_scan">
        <S2Scan />
      </Sequence>
      <Sequence from={SHOTS.s3_card1.from} durationInFrames={SHOTS.s3_card1.to - SHOTS.s3_card1.from}>
        <BreathCard line={CARDS[0].line} />
      </Sequence>
      <Sequence from={SHOTS.s4_evidence.from} durationInFrames={SHOTS.s4_evidence.to - SHOTS.s4_evidence.from} name="s4_evidence">
        <S4Evidence />
      </Sequence>
      <Sequence from={SHOTS.s5_card2.from} durationInFrames={SHOTS.s5_card2.to - SHOTS.s5_card2.from}>
        <BreathCard line={CARDS[1].line} />
      </Sequence>
      <Sequence from={SHOTS.s6_onetap.from} durationInFrames={SHOTS.s6_onetap.to - SHOTS.s6_onetap.from} name="s6_onetap">
        <S6Onetap />
      </Sequence>
      <Sequence from={SHOTS.s7_purge.from} durationInFrames={SHOTS.s7_purge.to - SHOTS.s7_purge.from} name="s7_purge">
        <S7Purge />
      </Sequence>
      <Sequence from={SHOTS.s8_card3.from} durationInFrames={SHOTS.s8_card3.to - SHOTS.s8_card3.from}>
        <BreathCard line={CARDS[2].line} />
      </Sequence>
      <Sequence from={SHOTS.s9_ledger.from} durationInFrames={SHOTS.s9_ledger.to - SHOTS.s9_ledger.from} name="s9_ledger">
        <S9Ledger />
      </Sequence>
      <Sequence from={SHOTS.s10_finale.from} durationInFrames={SHOTS.s10_finale.to - SHOTS.s10_finale.from} name="s10_finale">
        <S10Finale />
      </Sequence>
      <Sequence from={SHOTS.s11_signoff.from} durationInFrames={SHOTS.s11_signoff.to - SHOTS.s11_signoff.from} name="s11_signoff">
        <S11Signoff />
      </Sequence>
      {bgm ? <Audio src={staticFile("audio/bgm.mp3")} volume={(f) => (f < 15 ? f / 15 : f > TOTAL_FRAMES - 45 ? Math.max((TOTAL_FRAMES - f) / 45, 0) : 1)} /> : null}
      {SFX.map((s, i) => (
        <Sequence key={i} from={s.from} durationInFrames={s.durationInFrames ?? 90}>
          <Audio src={sfxSrc(s.src)} volume={s.volume} />
        </Sequence>
      ))}
    </AbsoluteFill>
  );
};
