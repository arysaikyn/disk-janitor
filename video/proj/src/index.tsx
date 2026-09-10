import { registerRoot, Composition } from "remotion";
import { DiskJanitorPromo, FPS, WIDTH, HEIGHT, TOTAL_FRAMES } from "./DiskJanitorPromo";

registerRoot(() => (
  <>
    <Composition
      id="DiskJanitorPromo"
      component={DiskJanitorPromo}
      durationInFrames={TOTAL_FRAMES}
      fps={FPS}
      width={WIDTH}
      height={HEIGHT}
      defaultProps={{ bgm: true }}
    />
    <Composition
      id="DiskJanitorPromo916"
      component={DiskJanitorPromo}
      durationInFrames={TOTAL_FRAMES}
      fps={FPS}
      width={1080}
      height={1920}
      defaultProps={{ bgm: true, vertical: true }}
    />
  </>
));
