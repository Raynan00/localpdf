"""Build the teaser soundtrack: music edit + synthesized sound effects.

Usage (from the teaser-25s folder):
    python3 audio/build_audio.py
Needs numpy, scipy and ffmpeg, plus the licensed music file at
assets/audio/music-full.mp3 (not committed: Pixabay's licence allows using
the track in a video but not redistributing the audio file itself).

Writes assets/audio/sfx.wav (effects only, original, safe to commit) and
assets/audio/mix.wav (music + effects, loudness-normalised; not committed).
"""
import subprocess
import numpy as np
from scipy.signal import butter, sosfilt, fftconvolve
from scipy.io import wavfile

SR = 48000
DUR = 25.0
N = int(SR * DUR)
rng = np.random.default_rng(20261008)  # fixed seed: identical output every build

# ---------------------------------------------------------------- music edit
EDIT_AT = 18.70          # video time of the splice (scene change into the labels)
B_OFFSET = 194.86        # song time = video time + offset after the splice
XFADE = 0.03

def load_music():
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", "assets/audio/music-full.mp3", "-f", "f32le",
         "-ac", "2", "-ar", str(SR), "-"], capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.float32).reshape(-1, 2).astype(np.float64)

def music_edit(m):
    out = np.zeros((N, 2))
    e = int(EDIT_AT * SR); x = int(XFADE * SR)
    out[:e + x] = m[:e + x]
    b0 = int((EDIT_AT + B_OFFSET) * SR)
    tail = m[b0: b0 + (N - e)]
    ramp = np.linspace(0, 1, x)[:, None]
    out[e:e + x] = out[e:e + x] * (1 - ramp) + tail[:x] * ramp
    out[e + x:e + len(tail)] = tail[x:]
    f = int(0.9 * SR)  # gentle tail-out on the end card
    out[-f:] *= np.linspace(1, 0, f)[:, None] ** 1.6
    return out

# ---------------------------------------------------------------- synthesis helpers
def env_exp(n, tau):
    return np.exp(-np.arange(n) / (tau * SR))

def band(x, lo, hi, order=2):
    return sosfilt(butter(order, [lo, hi], btype="band", fs=SR, output="sos"), x)

def hp(x, f): return sosfilt(butter(2, f, btype="high", fs=SR, output="sos"), x)
def lp(x, f): return sosfilt(butter(2, f, btype="low", fs=SR, output="sos"), x)

def place(bus, sig, t, gain_db=0.0, pan=0.0):
    """Add a mono signal to the stereo bus at time t with constant-power pan."""
    g = 10 ** (gain_db / 20)
    i = int(t * SR); sig = sig[: max(0, N - i)]
    l = np.cos((pan + 1) * np.pi / 4); r = np.sin((pan + 1) * np.pi / 4)
    bus[i:i + len(sig), 0] += sig * g * l * np.sqrt(2)
    bus[i:i + len(sig), 1] += sig * g * r * np.sqrt(2)

def click():
    n = int(0.06 * SR)
    t = np.arange(n) / SR
    transient = hp(rng.standard_normal(n), 3000) * env_exp(n, 0.0012)
    tone = np.sin(2 * np.pi * 3400 * t) * env_exp(n, 0.012) * 0.35
    body = np.sin(2 * np.pi * 190 * t) * env_exp(n, 0.009) * 0.5
    return (transient * 0.6 + tone + body)

def tick(freq=5200, tau=0.006):
    n = int(0.03 * SR); t = np.arange(n) / SR
    return np.sin(2 * np.pi * freq * t) * env_exp(n, tau) + hp(rng.standard_normal(n), 4000) * env_exp(n, 0.0008) * 0.3

def pop():
    n = int(0.16 * SR); t = np.arange(n) / SR
    f = 220 + 520 * np.exp(-t / 0.018)                 # pitch falls fast
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * env_exp(n, 0.045) + hp(rng.standard_normal(n), 2500) * env_exp(n, 0.002) * 0.25

def whoosh(dur, lo=300, hi=4200, peak=0.55):
    n = int(dur * SR); t = np.linspace(0, 1, n)
    noise = rng.standard_normal(n)
    out = np.zeros(n); blk = int(0.008 * SR)
    for s in range(0, n, blk):                       # swept band-pass, block-wise
        p = s / n
        c = lo * (hi / lo) ** np.sin(np.pi * min(p / peak, 1) * 0.5) if p < peak else hi * (lo / hi) ** ((p - peak) / (1 - peak))
        seg = noise[max(0, s - 2048): s + blk]
        y = band(seg, max(80, c * 0.55), min(20000, c * 1.6))
        out[s:s + blk] = y[-len(out[s:s + blk]):]
    shape = np.where(t < peak, (t / peak) ** 2.2, ((1 - t) / (1 - peak)) ** 1.4)
    return out * shape

def swell(dur):
    """Reverse-style riser that cuts to silence on its last sample."""
    n = int(dur * SR); t = np.linspace(0, 1, n)
    x = lp(hp(rng.standard_normal(n), 400), 6000) * t ** 3.2
    x += np.sin(2 * np.pi * 110 * np.arange(n) / SR) * t ** 4 * 0.25
    return x

def paper(dur=0.16):
    n = int(dur * SR)
    return band(rng.standard_normal(n), 1800, 7000) * np.sin(np.pi * np.linspace(0, 1, n)) ** 1.5

def sub_hit():
    n = int(1.8 * SR); t = np.arange(n) / SR
    f = 46 + 30 * np.exp(-t / 0.08)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * env_exp(n, 0.55) * (1 - np.exp(-t / 0.004))

def reverb(x, secs=0.9, wet=0.18):
    n = int(secs * SR)
    ir = rng.standard_normal((n, 2)) * env_exp(n, secs / 5)[:, None]
    ir = np.stack([lp(hp(ir[:, c], 300), 7000) for c in range(2)], 1)
    ir /= np.sqrt((ir ** 2).sum(0))
    tail = np.stack([fftconvolve(x[:, c], ir[:, c])[:N] for c in range(2)], 1)
    return x + tail * wet

# ---------------------------------------------------------------- the cue sheet (video seconds)
sfx = np.zeros((N, 2))
place(sfx, whoosh(0.85), 3.12, -15, pan=-0.05)        # photo flies into the browser
place(sfx, tick(2400, 0.01), 3.98, -22, pan=0.0)       # it lands in the drop zone
place(sfx, whoosh(0.45, 200, 1600, 0.4), 4.72, -20, pan=0.1)  # cookie panel slides up
place(sfx, swell(1.05), 6.12, -19)                     # riser into "It does not need to be."
place(sfx, click(), 11.50, -9, pan=-0.1)               # right-click on the photo (lands on the drop)
place(sfx, tick(6200, 0.004), 11.60, -24, pan=0.0)     # menu opens
place(sfx, tick(5600, 0.003), 12.08, -30, pan=0.05)    # hover moves
place(sfx, tick(5600, 0.003), 12.40, -30, pan=0.05)
place(sfx, tick(6200, 0.004), 12.60, -25, pan=0.15)    # submenu opens
place(sfx, tick(5000, 0.003), 13.13, -27, pan=0.2)     # Make PDF highlights
place(sfx, click(), 13.31, -8, pan=0.2)                # click Make PDF
place(sfx, pop(), 14.75, -13, pan=0.1)                 # scan.pdf lands
place(sfx, pop(), 15.82, -13, pan=0.25)                # contract-compressed.pdf lands
for k in range(6):                                     # size counts down
    place(sfx, tick(4200 - k * 250, 0.003), 16.07 + k * 0.08, -31, pan=0.25)
place(sfx, pop(), 16.88, -14, pan=0.5)                 # split stack appears
place(sfx, paper(), 17.22, -19, pan=0.4)               # pages fan apart
place(sfx, paper(0.12), 17.27, -21, pan=0.6)
place(sfx, sub_hit(), 21.94, -12)                      # weight under the wordmark, on the song's last hit
sfx = reverb(sfx)

wavfile.write("assets/audio/sfx.wav", SR, (np.clip(sfx, -1, 1) * 32767).astype(np.int16))

music = music_edit(load_music())
mix = music * 0.82 + sfx
peak = np.abs(mix).max()
wavfile.write("assets/audio/mix-pre.wav", SR, (mix / max(1, peak) * 0.98 * 32767).astype(np.int16))
subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", "assets/audio/mix-pre.wav", "-af",
                "loudnorm=I=-14:TP=-1.0:LRA=11", "-ar", str(SR), "assets/audio/mix.wav"], check=True)
print("wrote assets/audio/sfx.wav and assets/audio/mix.wav")
