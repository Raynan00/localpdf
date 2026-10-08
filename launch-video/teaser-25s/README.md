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

Beats: desktop photo (0 s), converter site and cookie banner (3 s), "It does not need to be." (7 s), right-click menu (10 s), three results (14.5 s), Photos / Contracts / Scans (18.5 s), wordmark (22 s).
