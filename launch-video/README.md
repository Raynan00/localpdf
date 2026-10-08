# LocalPDF launch video

A 45-second 1080p launch video made with [HyperFrames](https://github.com/heygen-com/hyperframes) (HTML + GSAP, rendered to MP4).

```
npx hyperframes@0.8.140 preview                      # live preview in the browser
npx hyperframes@0.8.140 check                        # lint, layout and contrast checks
npx hyperframes@0.8.140 render -q high -o out/localpdf-launch.mp4
```

Everything the render needs is local: GSAP and the Inter fonts (SIL Open Font License, see `assets/fonts/LICENSE.txt`) live in `assets/`, so it renders offline and the same way every time.

Scenes, all in `index.html`: hook (0 s), the problem with upload sites (4.4 s), brand (9.4 s), one-click Compress demo (12.6 s), Windows and Mac right-click (20 s), Split dialog demo (26 s), all actions (32.4 s), privacy claims (36.8 s), call to action (40.6 s).
