#!/usr/bin/env python3
"""Builds the Android launcher icons from crates/openmacro-app/android/icon.png.

Writes crates/openmacro-app/android/res/, which scripts/android-apk.sh copies
over the default icons in the project dx generates:
- an adaptive icon (Android 8+): the artwork scaled into the safe zone on a
  yellow foreground layer, over a matching background colour;
- square icons for older Android versions.

Needs Pillow (`pip install pillow`). Run from the repository root.
"""

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "crates/openmacro-app/android/icon.png"
RES = ROOT / "crates/openmacro-app/android/res"

# Background of the artwork, sampled from its edges.
BACKGROUND = (253, 230, 101)

# Launchers mask the 108 dp adaptive layer to about the middle 72 dp (a
# circle on some). The artwork reaches 96% of the way to the image's
# corners, so scaling it to two thirds keeps it all inside a circle mask.
ARTWORK_SCALE = 0.66

DENSITIES = {"mdpi": 1.0, "hdpi": 1.5, "xhdpi": 2.0, "xxhdpi": 3.0, "xxxhdpi": 4.0}


def foreground(source: Image.Image, size: int) -> Image.Image:
    layer = Image.new("RGB", (size, size), BACKGROUND)
    inner = round(size * ARTWORK_SCALE)
    art = source.resize((inner, inner), Image.LANCZOS)
    offset = (size - inner) // 2
    layer.paste(art, (offset, offset))
    return layer


def main() -> None:
    source = Image.open(SOURCE).convert("RGB")
    for name, scale in DENSITIES.items():
        folder = RES / f"mipmap-{name}"
        folder.mkdir(parents=True, exist_ok=True)
        foreground(source, round(108 * scale)).save(folder / "ic_launcher_foreground.png", optimize=True)
        size = round(48 * scale)
        source.resize((size, size), Image.LANCZOS).save(folder / "ic_launcher.webp", quality=95)

    anydpi = RES / "mipmap-anydpi-v26"
    anydpi.mkdir(parents=True, exist_ok=True)
    (anydpi / "ic_launcher.xml").write_text(
        """<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/openmacro_icon_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
"""
    )
    values = RES / "values"
    values.mkdir(parents=True, exist_ok=True)
    (values / "openmacro_icon.xml").write_text(
        """<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="openmacro_icon_background">#%02X%02X%02X</color>
</resources>
"""
        % BACKGROUND
    )


if __name__ == "__main__":
    main()
