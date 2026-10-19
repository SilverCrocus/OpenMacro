#!/usr/bin/env python3
"""Makes the tour's voiceover with ElevenLabs and times the video to it.

Reads voiceover.txt (one paragraph per scene), asks ElevenLabs for the
narration with character timings, saves assets/voiceover.mp3 and
voiceover-timing.json, then writes the cue times (when each line starts)
into index.html: the CUE object and every scene's data-start and
data-duration.

Needs ELEVENLABS_API_KEY in the environment or in ~/.openmacro-eval/keys.env.
Run from this folder: python3 make_voiceover.py
"""

import base64
import json
import os
import re
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
VOICE_ID = "cjVigY5qzO86Huf0OWal"  # Eric: smooth, trustworthy
MODEL = "eleven_multilingual_v2"
# Pause between scenes.
BREAK = '<break time="0.7s" />'
# Seconds a scene appears before its line (LEAD in index.html).
LEAD = 0.35
# Seconds the last scene stays after the narration ends.
TAIL = 1.8

# Each cue starts when its phrase is first spoken.
CUES = {
    "s2": "Instead of guessing",
    "s3": "Logging takes seconds",
    "s4": "Then watch",
    "s5": "No account",
    "s6": "Get OpenMacro",
    "c1": "Search fifty-eight",
    "c2": "scan a barcode",
    "c3": "snap a photo",
    "bubble": "Got something wrong",
    "l1": "No account",
    "l2": "No subscription",
    "l3": "No ads",
    "l4": "Your data stays",
    "badges": "every line of code",
}
SCENES = ["s1", "s2", "s3", "s4", "s5", "s6"]


def api_key() -> str:
    key = os.environ.get("ELEVENLABS_API_KEY")
    if key:
        return key
    keys = Path.home() / ".openmacro-eval" / "keys.env"
    for line in keys.read_text().splitlines():
        if line.startswith("ELEVENLABS_API_KEY="):
            return line.split("=", 1)[1].strip()
    raise SystemExit("Set ELEVENLABS_API_KEY")


def narrate(text: str) -> dict:
    request = urllib.request.Request(
        f"https://api.elevenlabs.io/v1/text-to-speech/{VOICE_ID}/with-timestamps"
        "?output_format=mp3_44100_128",
        data=json.dumps(
            {
                "text": text,
                "model_id": MODEL,
                "voice_settings": {"stability": 0.5, "similarity_boost": 0.75, "style": 0.15},
            }
        ).encode(),
        headers={"xi-api-key": api_key(), "content-type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def main() -> None:
    paragraphs = [p.strip() for p in (HERE / "voiceover.txt").read_text().split("\n\n") if p.strip()]
    reply = narrate(f" {BREAK} ".join(paragraphs))
    (HERE / "assets" / "voiceover.mp3").write_bytes(base64.b64decode(reply["audio_base64"]))

    alignment = reply["alignment"]
    spoken = "".join(alignment["characters"])
    starts = alignment["character_start_times_seconds"]
    ends = alignment["character_end_times_seconds"]
    (HERE / "voiceover-timing.json").write_text(json.dumps(alignment, indent=1))

    cue = {}
    for name, phrase in CUES.items():
        index = spoken.find(phrase)
        if index < 0:
            raise SystemExit(f"{phrase!r} isn't in the narration")
        cue[name] = round(starts[index], 2)
    cue["end"] = round(max(ends) + TAIL, 2)

    # Scene windows: each starts LEAD before its line and runs to the next.
    begin = {"s1": 0.0, **{s: max(0.0, cue[s] - LEAD) for s in SCENES[1:]}}
    window = {}
    for i, s in enumerate(SCENES):
        stop = begin[SCENES[i + 1]] if i + 1 < len(SCENES) else cue["end"]
        window[s] = (round(begin[s], 2), round(stop - begin[s], 2))

    html = (HERE / "index.html").read_text()
    html = re.sub(r"const CUE = \{.*?\};", "const CUE = " + json.dumps(cue, separators=(",", ":")) + ";", html)
    for s, (start, duration) in window.items():
        html = re.sub(
            rf'(<section id="{s}" class="clip") data-start="[^"]*" data-duration="[^"]*"',
            rf'\1 data-start="{start}" data-duration="{duration}"',
            html,
        )
    for element in ("root", "bg", "vo"):
        html = re.sub(
            rf'(id="{element}"[^>]*?data-duration=")[^"]*(")',
            rf'\g<1>{cue["end"]}\g<2>',
            html,
        )
    (HERE / "index.html").write_text(html)

    print(f"Narration: {max(ends):.1f} s; video: {cue['end']} s")
    for s, (start, duration) in window.items():
        print(f"  {s}: {start:5.2f} s for {duration:5.2f} s")


if __name__ == "__main__":
    main()
