# OpenMacro tour video

The 42-second tour on the project's front page, made with [HyperFrames](https://hyperframes.heygen.com): an HTML composition animated with GSAP and rendered to MP4.

| File | What it is |
|---|---|
| `index.html` | The composition: six scenes over the app's screenshots |
| `voiceover.txt` | The narration, one paragraph per scene |
| `make_voiceover.py` | Makes `assets/voiceover.mp3` with ElevenLabs (voice: Eric), then times the scenes and animations to it |
| `assets/` | Screenshots, the icon, the voiceover and the Inter font (SIL OFL) |
| `thumbnail.png` | A thumbnail with a play button, for sharing the video |

## Making it again

You need Node.js 22+ and FFmpeg.

1. **Change the narration (optional):** edit `voiceover.txt`, then run `python3 make_voiceover.py`. It needs `ELEVENLABS_API_KEY`, either in the environment or in `~/.openmacro-eval/keys.env`.
2. **Preview:** `npm run dev` opens it in the browser.
3. **Check:** `npm run check`.
4. **Render:** `npx hyperframes render --quality high --output renders/openmacro-tour.mp4`.

The copy in `docs/media` is re-encoded smaller, with the loudness normalised:

```bash
ffmpeg -i renders/openmacro-tour.mp4 -c:v libx264 -preset slow -crf 24 -pix_fmt yuv420p \
  -movflags +faststart -af loudnorm=I=-16:TP=-1.5:LRA=11 -c:a aac -b:a 128k \
  ../../docs/media/openmacro-tour.mp4
```

New screenshots go in `assets/`, with the same names, at 720 pixels wide.

The front page shows `docs/images/tour-preview.gif`, a silent 12-second loop of the logging scene, because images display everywhere: on the website, in the GitHub app, and for visitors who aren't signed in. It links to the full video.

```bash
ffmpeg -ss 11.8 -t 11.9 -i renders/openmacro-tour.mp4 -vf "fps=10,scale=720:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=64:stats_mode=full[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" ../../docs/images/tour-preview.gif
```
