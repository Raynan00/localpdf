# LocalPDF teaser (25 s)

Composition is 1920×1080 at 60 fps; the delivery render is 4K.

```
npx hyperframes@0.8.140 render -q high -f 60 --resolution 4k -o out/localpdf-teaser-25s-4k60.mp4
npx hyperframes@0.8.140 render -q high -f 60 -o out/localpdf-teaser-25s-1080p60.mp4   # faster, 1080p
```

`--resolution 4k` renders the same layout at 2× pixel density, so type and vector shapes stay sharp.

Look and motion:
- Background: #141413 with an #E08A5D dot field (denser at the bottom) drifting in slow parallax, a faint warm glow, a vignette and fine film grain. The grain loop is finite (50 × 0.5 s) so frames seek deterministically.
- Camera: a slow drift across the whole film, plus a push toward the context menu during the right-click.
- Motion: `expo`/`power4` settles and critically damped springs (no bounces), per-word blur-in for text, scenes recede with a defocus instead of cutting. Shutter motion blur (HyperFrames registry `motion-blur` component, inlined at the bottom of `index.html`) on the cursor and the photo.
- Menu: Windows 11-style translucent panel; one hover pill glides between rows; the submenu wipes open; "Make PDF" fills amber.

`assets/scan.jpg` is a generated stand-in for a phone photo of a desk (1600×1200); replace it with a real 4:3 photo of the same name. `assets/dots.svg` is generated with a fixed seed.

Beats: desktop photo (0 s), converter site and cookie banner (3 s), "It does not need to be." (7 s), right-click in a generic Desktop folder window with a taskbar, labelled "On your desktop · Windows & Mac" (10 s), three results landing in the same folder (14.5 s), Photos / Contracts / Scans (18.5 s), wordmark (22 s).

## Soundtrack

`audio/build_audio.py` builds the soundtrack: an edit of "Product Launch Review" by apalonbeats (Pixabay) plus original synthesized sound effects, mixed to -14 LUFS.

- The video opens on the song's drop (song 0:11.5), so the beat runs from the first frame and the song's grid lines up with the video's (accents every 0.36 s from 0). At 18.62 s, under the scene change, it splices to the song's final bars, so its last hit lands on the wordmark (22.23 s). The music dips 5 dB under "It does not need to be."
- Clicks, menu ticks, result pops and the size count-down sit on the song's 166.7 BPM beat grid.
- The music file is not committed: Pixabay's licence allows it inside the video but not redistribution of the audio file. To rebuild, download the track to `assets/audio/music-full.mp3` and run `python3 audio/build_audio.py` (needs numpy, scipy, ffmpeg). `assets/audio/sfx.wav` (effects only) is committed.
