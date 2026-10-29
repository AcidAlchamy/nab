"""A made-up desktop for Nab's README pictures: wallpaper, a taskbar, a browser
and a notes window. Nothing on it comes from a real screen.

    python art/mock.py   ->  art/build/mock-desktop.png (1600x900)
                             art/build/mock-window.png  (the notes window alone)
Then `NAB_DEMO=art/build cargo test demo -- --ignored` draws Nab's picker and
card over it with the app's own code.
"""
import math, pathlib
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent
OUT = ROOT / "build"
W, H = 1600, 900
F = "C:/Windows/Fonts/"


def font(name, size):
    return ImageFont.truetype(F + name, size)


UI, UI_B, UI_SB, MONO = (lambda s: font("segoeui.ttf", s)), (lambda s: font("segoeuib.ttf", s)), (lambda s: font("seguisb.ttf", s)), (lambda s: font("consola.ttf", s))


def wallpaper():
    img = Image.new("RGB", (W, H))
    px = img.load()
    for y in range(H):
        for x in range(W):
            t = y / H
            r, g, b = 13 + 12 * t, 11 + 4 * t, 20 + 28 * t
            # two soft glows: violet top-left, pink bottom-right
            d1 = math.hypot(x - 260, y - 120) / 700
            d2 = math.hypot(x - 1400, y - 820) / 650
            k1, k2 = max(0, 1 - d1) ** 2, max(0, 1 - d2) ** 2
            r += 139 * 0.35 * k1 + 255 * 0.28 * k2
            g += 61 * 0.35 * k1 + 46 * 0.28 * k2
            b += 255 * 0.35 * k1 + 126 * 0.28 * k2
            px[x, y] = (int(min(255, r)), int(min(255, g)), int(min(255, b)))
    d = ImageDraw.Draw(img, "RGBA")
    for x in range(0, W, 40):
        d.line([(x, 0), (x, H)], fill=(255, 255, 255, 7))
    for y in range(0, H, 40):
        d.line([(0, y), (W, y)], fill=(255, 255, 255, 7))
    return img.convert("RGBA")


def shadowed(base, box, radius=10):
    x0, y0, x1, y1 = box
    sh = Image.new("RGBA", base.size, (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle((x0 + 4, y0 + 12, x1 + 4, y1 + 16), radius + 6, fill=(0, 0, 0, 150))
    base.alpha_composite(sh.filter(ImageFilter.GaussianBlur(18)))


def window(size, title, dark=True):
    w, h = size
    win = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(win)
    bg = (24, 21, 33, 255) if dark else (245, 244, 248, 255)
    d.rounded_rectangle((0, 0, w - 1, h - 1), 8, fill=bg, outline=(58, 52, 78, 255))
    d.rectangle((1, 1, w - 2, 34), fill=(18, 15, 26, 255) if dark else (232, 230, 238, 255))
    d.text((14, 8), title, font=UI_SB(13), fill=(220, 216, 232) if dark else (40, 36, 52))
    for i, c in enumerate([(120, 116, 136), (120, 116, 136), (120, 116, 136)]):
        x = w - 110 + i * 36
        if i == 0:
            d.line([(x, 18), (x + 10, 18)], fill=c, width=1)
        elif i == 1:
            d.rectangle((x, 13, x + 9, 22), outline=c)
        else:
            d.line([(x, 13), (x + 9, 22)], fill=c); d.line([(x + 9, 13), (x, 22)], fill=c)
    return win, d


def browser():
    win, d = window((860, 540), "Donationware - Acid Alchamy", dark=True)
    d.rounded_rectangle((14, 44, 846, 74), 15, fill=(32, 28, 44, 255))
    d.text((34, 50), "acidalchamy.com/donationware", font=UI(14), fill=(170, 164, 190))
    d.text((40, 104), "// ACID ALCHAMY LAB", font=MONO(13), fill=(198, 255, 58))
    d.text((38, 124), "Donationware", font=UI_B(44), fill=(240, 238, 246))
    d.text((40, 184), "Small tools, built properly, given away.", font=UI(17), fill=(170, 164, 190))
    for i, (a, b) in enumerate([((139, 61, 255), (255, 46, 126)), ((46, 230, 255), (139, 61, 255)), ((60, 56, 76), (60, 56, 76))]):
        x = 40 + i * 190
        box = Image.new("RGBA", (160, 210))
        bd = ImageDraw.Draw(box)
        for y in range(210):
            t = y / 210
            bd.line([(0, y), (160, y)], fill=tuple(int(a[k] + (b[k] - a[k]) * t) for k in range(3)) + (255,))
        mask = Image.new("L", (160, 210), 0)
        ImageDraw.Draw(mask).rounded_rectangle((0, 0, 159, 209), 10, fill=255)
        win.paste(box, (x, 240), mask)
        d.text((x + 14, 256), ["SHOOK", "NAB", "NEXT"][i], font=UI_B(22), fill=(255, 255, 255))
    return win


def notes():
    win, d = window((820, 500), "ideas.md - Notes", dark=True)
    lines = [
        ("# Nab", (198, 255, 58), True),
        ("", None, False),
        ("One key grabs the window you're in.", (226, 222, 238), False),
        ("", None, False),
        ("- Ctrl+Shift+Q: the window, edge to edge", (170, 164, 190), False),
        ("- Ctrl+Shift+A: drag an area, or click a window", (170, 164, 190), False),
        ("- copied, saved, named after the app", (170, 164, 190), False),
        ("- a folder per project, if you like", (170, 164, 190), False),
        ("", None, False),
        ("## Next", (255, 46, 126), True),
        ("- box art", (170, 164, 190), False),
        ("- tell the stream", (170, 164, 190), False),
    ]
    for i, (text, color, bold) in enumerate(lines):
        y = 56 + i * 30
        d.text((18, y), f"{i + 1:>2}", font=MONO(15), fill=(84, 78, 104))
        if text:
            d.text((62, y), text, font=(UI_B(19) if bold else MONO(16)), fill=color)
    d.rectangle((1, 466, 818, 498), fill=(18, 15, 26, 255))
    d.text((16, 474), "Ln 3, Col 36    UTF-8    Markdown", font=UI(12), fill=(120, 116, 136))
    return win


def taskbar(img):
    d = ImageDraw.Draw(img, "RGBA")
    d.rectangle((0, H - 46, W, H), fill=(14, 12, 22, 232))
    d.line([(0, H - 46), (W, H - 46)], fill=(255, 255, 255, 18))
    icons = [(139, 61, 255), (46, 230, 255), (255, 46, 126), (198, 255, 58), (255, 176, 32)]
    x0 = W // 2 - len(icons) * 22
    for i, c in enumerate(icons):
        x = x0 + i * 44
        d.rounded_rectangle((x, H - 37, x + 28, H - 9), 7, fill=c + (230,))
    d.text((W - 92, H - 38), "9:14 PM", font=UI(13), fill=(220, 216, 232))
    d.text((W - 92, H - 22), "10/29/2026", font=UI(12), fill=(160, 154, 180))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    img = wallpaper()
    b = browser()
    shadowed(img, (90, 70, 90 + b.width, 70 + b.height))
    img.alpha_composite(b, (90, 70))
    n = notes()
    nx, ny = 640, 250
    shadowed(img, (nx, ny, nx + n.width, ny + n.height))
    img.alpha_composite(n, (nx, ny))
    taskbar(img)
    img.convert("RGB").save(OUT / "mock-desktop.png")
    # The notes window as Nab would nab it: its own pixels, rounded corners and all.
    img.crop((nx, ny, nx + n.width, ny + n.height)).save(OUT / "mock-window.png")
    (OUT / "mock-window.txt").write_text(f"{nx} {ny} {n.width} {n.height}")
    print("mock written", img.size)


if __name__ == "__main__":
    main()
