"""Builds the README pictures in docs/img from the demo renders.

    python art/icons.py && python art/mock.py
    NAB_DEMO=art/build cargo test demo -- --ignored     (the picker, the card, a finished nab)
    python art/readme.py

Everything shown is made up (art/mock.py) or drawn by Nab's own code; no real
screen is ever captured for these.
"""
import pathlib
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent
BUILD = ROOT / "build"
IMG = ROOT.parent / "docs" / "img"
F = "C:/Windows/Fonts/"
BG = (13, 11, 20)
ACID, PINK, VIOLET, TEXT, MUTED = (198, 255, 58), (255, 46, 126), (139, 61, 255), (238, 236, 244), (163, 157, 182)


def font(name, size):
    return ImageFont.truetype(F + name, size)


def glow(img, at, radius, color, strength):
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ImageDraw.Draw(layer).ellipse((at[0] - radius, at[1] - radius, at[0] + radius, at[1] + radius), fill=color + (strength,))
    img.alpha_composite(layer.filter(ImageFilter.GaussianBlur(radius * 0.6)))


def banner():
    w, h = 1280, 400
    img = Image.new("RGBA", (w, h), BG + (255,))
    glow(img, (180, 200), 220, VIOLET, 120)
    glow(img, (1120, 330), 200, PINK, 90)
    grid = Image.new("RGBA", img.size, (0, 0, 0, 0))
    gd = ImageDraw.Draw(grid)
    for x in range(0, w, 32):
        gd.line([(x, 0), (x, h)], fill=(255, 255, 255, 9))
    for y in range(0, h, 32):
        gd.line([(0, y), (w, y)], fill=(255, 255, 255, 9))
    img.alpha_composite(grid)  # drawing on RGBA replaces pixels; compositing blends
    d = ImageDraw.Draw(img)
    icon = Image.open(ROOT / "nab-icon-512.png").convert("RGBA").resize((200, 200), Image.LANCZOS)
    img.alpha_composite(icon, (64, 100))
    d.text((300, 92), "Nab", font=font("segoeuib.ttf", 112), fill=TEXT)
    d.text((306, 236), "One key grabs the window you're in.", font=font("seguisb.ttf", 30), fill=ACID)
    d.text((306, 282), "Copied, saved and named, in the folder you picked.", font=font("segoeui.ttf", 21), fill=MUTED)
    d.text((306, 314), "Free donationware for Windows 10 & 11  \u00b7  built in Rust", font=font("segoeui.ttf", 21), fill=MUTED)
    card = Image.open(BUILD / "card.png").convert("RGBA")
    card = card.resize((card.width * 46 // 100, card.height * 46 // 100), Image.LANCZOS)
    shadow = Image.new("RGBA", img.size, (0, 0, 0, 0))
    cx, cy = w - card.width - 40, h - card.height - 34
    ImageDraw.Draw(shadow).rounded_rectangle((cx + 6, cy + 14, cx + card.width + 6, cy + card.height + 14), 14, fill=(0, 0, 0, 170))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)))
    img.alpha_composite(card, (cx, cy))
    rule = Image.new("RGBA", (w, 3))
    for x in range(w):
        t = x / (w - 1)
        a, b, k = (VIOLET, PINK, t / 0.6) if t < 0.6 else (PINK, ACID, (t - 0.6) / 0.4)
        ImageDraw.Draw(rule).line([(x, 0), (x, 2)], fill=tuple(int(a[i] + (b[i] - a[i]) * k) for i in range(3)) + (255,))
    img.alpha_composite(rule, (0, h - 3))
    img.convert("RGB").save(IMG / "banner.png", optimize=True)


def checker(size, cell=8):
    img = Image.new("RGBA", size, (96, 92, 112, 255))
    d = ImageDraw.Draw(img)
    for y in range(0, size[1], cell):
        for x in range(0, size[0], cell):
            if (x // cell + y // cell) % 2:
                d.rectangle((x, y, x + cell - 1, y + cell - 1), fill=(66, 62, 82, 255))
    return img


def corners():
    """Before and after: a window's top-left corner, ten times bigger. "Before"
    is the same window over a bright wallpaper, the way a plain rectangle grab
    keeps whatever was behind the rounded corner."""
    done = Image.open(BUILD / "result.png").convert("RGBA")
    raw = Image.new("RGBA", done.size)
    rd = ImageDraw.Draw(raw)
    for y in range(done.height):
        t = y / done.height
        rd.line([(0, y), (done.width, y)], fill=(int(255 - 116 * t), int(46 + 15 * t), int(126 + 129 * t), 255))
    raw.alpha_composite(done)
    zoom, src = 10, 26
    w, h = 2 * src * zoom + 120, src * zoom + 112
    img = Image.new("RGBA", (w, h), BG + (255,))
    d = ImageDraw.Draw(img)
    for i, (shot, title, note) in enumerate([(raw, "A plain rectangle grab", "the wallpaper shows in the corner"), (done, "Nab", "a clean, transparent corner")]):
        x = 40 + i * (src * zoom + 40)
        tile = checker((src * zoom, src * zoom), 20)
        big = shot.crop((0, 0, src, src)).resize((src * zoom, src * zoom), Image.NEAREST)
        tile.alpha_composite(big)
        img.alpha_composite(tile, (x, 80))
        d.rectangle((x - 1, 79, x + src * zoom, 80 + src * zoom), outline=(58, 52, 78))
        d.text((x, 12), title, font=font("segoeuib.ttf", 22), fill=ACID if i else MUTED)
        d.text((x, 44), note, font=font("segoeui.ttf", 16), fill=MUTED)
    img.convert("RGB").save(IMG / "corners.png", optimize=True)


def main():
    IMG.mkdir(parents=True, exist_ok=True)
    banner()
    corners()
    Image.open(BUILD / "picker-window.png").convert("RGB").save(IMG / "picker.png", optimize=True)
    Image.open(BUILD / "picker-drag.png").convert("RGB").crop((60, 240, 880, 720)).save(IMG / "area.png", optimize=True)
    Image.open(BUILD / "card.png").save(IMG / "card.png", optimize=True)
    Image.open(BUILD / "result.png").save(IMG / "result.png", optimize=True)
    for name in ["settings-top.png"]:
        if (BUILD / name).exists():
            Image.open(BUILD / name).convert("RGB").save(IMG / "settings.png", optimize=True)
    print("docs/img written")


if __name__ == "__main__":
    main()
