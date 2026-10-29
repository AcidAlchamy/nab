"""Renders Nab's icon from SVG at every size (crisp, not downscaled) and packs
res/nab.ico and res/nab-paused.ico. Needs Chrome (or Edge) and Pillow.

    python art/icons.py
"""
import pathlib, subprocess, tempfile
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Headless screenshots: Chrome first (Edge's headless mode stopped writing them on some PCs).
BROWSER = next(
    p for p in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ] if pathlib.Path(p).exists()
)
SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]


def brackets(l, t, r, b, arm, w, color):
    """Four capture corners whose outer edges sit on the box l, t, r, b."""
    h = w / 2
    L, T, R, B = l + h, t + h, r - h, b - h
    paths = [
        f"M{L} {T + arm} V{T} H{L + arm}",
        f"M{R - arm} {T} H{R} V{T + arm}",
        f"M{L} {B - arm} V{B} H{L + arm}",
        f"M{R - arm} {B} H{R} V{B - arm}",
    ]
    return "".join(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" stroke-linecap="square"/>' for d in paths)


def svg(px: int, paused: bool) -> str:
    small = px <= 24
    top, bottom = ("#5B5B6E", "#2E2E3A") if paused else ("#8B3DFF", "#FF2E7E")
    lime = "#B9B9C8" if paused else "#C6FF3A"
    paper = "#D9D9E3" if paused else "#FFFFFF"
    ink = "#140A2A"
    if small:
        win = (62, 74, 132, 108, 16, 30)  # x, y, w, h, radius, title bar
        corners = brackets(20, 20, 236, 236, 52, 30, lime)
        dots = ""
    else:
        win = (70, 80, 116, 92, 13, 24)
        corners = brackets(40, 50, 216, 202, 34, 15, lime)
        dots = "".join(
            f'<circle cx="{86 + i * 15}" cy="92" r="4.6" fill="{c}"/>'
            for i, c in enumerate(["#9A9AAE", "#C9C9D6", "#7C7C90"] if paused else ["#FF2E7E", "#C6FF3A", "#8B3DFF"])
        )
    x, y, w, h, rx, bar = win
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="{px}" height="{px}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>
    <radialGradient id="shine" cx=".28" cy=".18" r=".85"><stop offset="0" stop-color="#fff" stop-opacity=".32"/><stop offset=".6" stop-color="#fff" stop-opacity="0"/></radialGradient>
    <clipPath id="win"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}"/></clipPath>
  </defs>
  <rect x="6" y="6" width="244" height="244" rx="{50 if small else 58}" fill="url(#bg)"/>
  <rect x="6" y="6" width="244" height="244" rx="{50 if small else 58}" fill="url(#shine)"/>
  <rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{paper}"/>
  <rect x="{x}" y="{y}" width="{w}" height="{bar}" fill="{ink}" fill-opacity=".92" clip-path="url(#win)"/>
  {dots}
  {corners}
</svg>"""


def render(markup: str, px: int, out: pathlib.Path) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        page = pathlib.Path(tmp) / "icon.html"
        page.write_text(f'<html><body style="margin:0;background:transparent">{markup}</body></html>', encoding="utf-8")
        shot = pathlib.Path(tmp) / "shot.png"
        side = max(300, px + 20)
        subprocess.run([BROWSER, "--headless=new", f"--user-data-dir={pathlib.Path(tmp) / 'profile'}", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
                        "--default-background-color=00000000", f"--window-size={side},{side}",
                        f"--screenshot={shot}", page.as_uri()], check=True, capture_output=True, timeout=60)
        img = Image.open(shot).convert("RGBA").crop((0, 0, px, px))
        img.load()
    img.save(out)
    return img


def main():
    out = ROOT / "art" / "build"
    out.mkdir(parents=True, exist_ok=True)
    for paused, name in [(False, "nab"), (True, "nab-paused")]:
        imgs = [render(svg(s, paused), s, out / f"{name}-{s}.png") for s in SIZES]
        imgs[-1].save(ROOT / "res" / f"{name}.ico", sizes=[(s, s) for s in SIZES], append_images=imgs[:-1])
        (out / f"{name}.svg").write_text(svg(256, paused), encoding="utf-8")
    render(svg(512, False), 512, ROOT / "art" / "nab-icon-512.png")
    # Raw RGBA for the settings window's title-bar icon (no image decoder needed at runtime).
    (ROOT / "res" / "icon-64.rgba").write_bytes(Image.open(out / "nab-64.png").convert("RGBA").tobytes())
    print("icons written")


if __name__ == "__main__":
    main()
