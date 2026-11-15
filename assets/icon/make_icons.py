"""Draw the EasyPlots app icon and menu-bar (tray) icon.

Run: python3 assets/icon/make_icons.py
Then: npm --prefix app run tauri icon assets/icon/app-icon.png
"""
from pathlib import Path
from PIL import Image, ImageDraw

HERE = Path(__file__).parent
TRAY = HERE.parent.parent / "app" / "src-tauri" / "icons" / "tray.png"
# A rising, slightly wiggly curve through these points (0..1 coordinates, y up).
CURVE = [(0.0, 0.10), (0.18, 0.30), (0.34, 0.22), (0.52, 0.55), (0.70, 0.48), (1.0, 0.92)]


def app_icon(size=1024):
    s = 4  # draw 4x larger then shrink, for smooth edges
    big = size * s
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pad = int(big * 0.09)  # macOS icon grid leaves a margin round the tile
    d.rounded_rectangle([pad, pad, big - pad, big - pad], radius=int(big * 0.18), fill=(250, 250, 252, 255))
    # plot area
    x0, y0, x1, y1 = pad + big * 0.17, pad + big * 0.17, big - pad - big * 0.15, big - pad - big * 0.19
    axis = (60, 60, 67, 255)
    w = int(big * 0.022)
    d.line([(x0, y0), (x0, y1), (x1, y1)], fill=axis, width=w, joint="curve")
    pts = [(x0 + (x1 - x0) * (0.06 + 0.9 * u), y1 - (y1 - y0) * (0.06 + 0.86 * v)) for u, v in CURVE]
    d.line(pts, fill=(0, 114, 178, 255), width=int(big * 0.04), joint="curve")
    r = big * 0.03
    for x, y in pts[1:-1]:
        d.ellipse([x - r, y - r, x + r, y + r], fill=(230, 159, 0, 255))
    img.resize((size, size), Image.LANCZOS).save(HERE / "app-icon.png")


def tray_icon(size=44):
    # Template image: black on transparent; macOS recolours it for light/dark menu bars.
    s = 8
    big = size * s
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    m = big * 0.12
    x0, y0, x1, y1 = m, m, big - m, big - m
    d.line([(x0, y0), (x0, y1), (x1, y1)], fill=(0, 0, 0, 255), width=int(big * 0.08), joint="curve")
    pts = [(x0 + (x1 - x0) * (0.15 + 0.85 * u), y1 - (y1 - y0) * (0.15 + 0.8 * v)) for u, v in CURVE]
    d.line(pts, fill=(0, 0, 0, 255), width=int(big * 0.1), joint="curve")
    img.resize((size, size), Image.LANCZOS).save(TRAY)


if __name__ == "__main__":
    app_icon()
    tray_icon()
