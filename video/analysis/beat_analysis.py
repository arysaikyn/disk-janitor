# Beat analysis per video-shotcraft references/music-beat-sync.md
# Grid-fit BPM/phase + tri-band onset detection + RMS structure + hit pool.
import json
import os

import librosa
import numpy as np
from scipy.signal import butter, sosfilt

SRC = os.environ.get("BGM", "/Users/aaikyn1/Downloads/Keep Walking.mp3")
OUT = os.path.dirname(os.path.abspath(__file__))

y, sr = librosa.load(SRC, sr=None, mono=True)
dur = len(y) / sr

# --- §1 grid fit ---
tempo, beats = librosa.beat.beat_track(y=y, sr=sr, tightness=400, units="time")
i = np.arange(len(beats))
A = np.vstack([i, np.ones_like(i)]).T
(T, t0), *_ = np.linalg.lstsq(A, beats, rcond=None)
bpm = 60.0 / T
residual = beats - (t0 + i * T)
print(f"beat_track tempo={float(np.ravel(tempo)[0]):.2f}  grid-fit BPM={bpm:.2f}  t0={t0:.4f}s  T={T:.5f}s  resid±{np.abs(residual).max()*1000:.0f}ms")

# half/double ambiguity: score 0.5x/1x/2x candidates by kick coverage
perc = librosa.effects.hpss(y)[1]
kick_env_full, times = None, None

def band_env(yy, lo, hi):
    sos = butter(4, [lo, hi], btype="band", fs=sr, output="sos")
    env = librosa.onset.onset_strength(y=sosfilt(sos, yy), sr=sr)
    return env, librosa.times_like(env, sr=sr)

kick_env, times = band_env(perc, 40, 160)
snare_env, _ = band_env(perc, 150, 3000)
hat_env, _ = band_env(perc, 6000, 14000)

def grid_score(period, phase):
    n = np.arange(int((dur - phase) / period))
    ts = phase + n * period
    hits = 0
    for t in ts:
        idx = np.argmin(np.abs(times - t))
        if abs(times[idx] - t) < 0.05 and kick_env[idx] > np.percentile(kick_env, 40):
            hits += 1
    return hits / max(len(ts), 1)

cands = {}
for name, period, phase in [
    ("0.5x", T * 2, t0 % (T * 2)),
    ("1x", T, t0),
    ("2x", T / 2, t0),
]:
    cands[name] = grid_score(period, phase)
print("grid candidates kick-coverage:", {k: f"{v:.2f}" for k, v in cands.items()})

n_beats = int((dur - t0) / T)
beats_t = [round(t0 + n * T, 4) for n in range(n_beats + 1)]

# --- §2a hit pool ---
hits = []
for n in range(n_beats):
    t = t0 + n * T
    idx = np.argmin(np.abs(times - t))
    if abs(times[idx] - t) < 0.06:
        hits.append({"n": n, "t": round(t, 4), "k": float(kick_env[idx]), "s": float(snare_env[idx]), "h": float(hat_env[idx])})
kicks = sorted(hits, key=lambda x: -x["k"])

# --- §2b RMS structure ---
rms = librosa.feature.rms(y=y, frame_length=2048, hop_length=512)[0]
r_times = librosa.times_like(rms, sr=sr)
# coarse sections: 4-beat window mean energy, then threshold segmentation
win = 4 * T
frames_per = max(int(win / (512 / sr)), 1)
seg_e = [float(rms[j:j + frames_per].mean()) for j in range(0, len(rms), frames_per)]
e_max = max(seg_e)
sections = []
state = None
for j, e in enumerate(seg_e):
    lvl = "high" if e > 0.6 * e_max else ("mid" if e > 0.35 * e_max else "low")
    if lvl != state:
        sections.append({"t": round(j * win, 2), "level": lvl})
        state = lvl

data = {
    "bpm": round(float(bpm), 3),
    "t0": round(float(t0), 4),
    "T": round(float(T), 5),
    "beat_track_tempo": round(float(np.ravel(tempo)[0]), 2),
    "duration": round(dur, 3),
    "grid_candidates": cands,
    "n_beats": n_beats,
    "beats": beats_t,
    "hits": hits,
    "top_kicks": kicks[:12],
    "sections": sections,
    "rms_head": [round(float(v), 4) for v in rms[::8]],
    "sr": sr,
}
with open(f"{OUT}/beat_data.json", "w") as f:
    json.dump(data, f, ensure_ascii=False)
print(f"n_beats={n_beats}  sections={len(sections)}  hits={len(hits)}  saved beat_data.json")
print("top kicks:", [(h["n"], round(h["k"], 1)) for h in kicks[:8]])
print("sections:", sections[:14])
