"""Draws the screen-shifter icon and writes every PNG the package and tray need.

Standard library only: the design is a handful of (rounded) rectangles, rendered with
exact edge coverage and supersampled corners. This script is the icon's source of truth;
its outputs are committed, so building never needs Python.

    python tools/make_icons.py
"""

import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

BEZEL = (0x4B, 0x55, 0x63)  # mid gray: must read on dark and light taskbars and Start
BEZEL_LIGHT = (0x6B, 0x72, 0x80)  # small sizes
SCREEN = (0x00, 0x78, 0xD7)  # the panel's accent blue
SHADOW = (0x00, 0x5A, 0x9E)
SHEET = (0xE5, 0xE7, 0xEB)
BAR = (0x9C, 0xA3, 0xAF)
BAR2 = (0xB8, 0xBE, 0xC7)

# (x, y, w, h, radius, color) on a 128-unit canvas, painted in order.
# "Sheet breaks out": the previous screen slides out past the monitor's left edge.
FULL = [
    (30, 18, 94, 70, 6, BEZEL),
    (36, 24, 82, 58, 0, SCREEN),
    (68, 88, 16, 12, 0, BEZEL),
    (52, 100, 48, 7, 3, BEZEL),
    (14, 28, 62, 50, 4, BAR),  # outline: the sheet must not melt into a light background
    (16, 30, 58, 46, 3, SHEET),
    (74, 32, 5, 44, 0, SHADOW),
    (22, 37, 24, 15, 2, BAR),
    (26, 56, 30, 14, 2, BAR2),
    (6, 40, 7, 3, 1.5, BAR),
    (4, 52, 9, 3, 1.5, BAR),
    (7, 64, 6, 3, 1.5, BAR),
]

# 16-24 px: one 128-unit step is 8 units per pixel at 16 px, so shapes snap to it.
SIMPLE = [
    (40, 16, 88, 88, 12, BEZEL_LIGHT),
    (48, 24, 72, 72, 0, SCREEN),
    (0, 32, 72, 56, 6, BEZEL_LIGHT),  # outline
    (8, 40, 64, 40, 2, SHEET),
    (72, 32, 8, 56, 0, SHADOW),
    (16, 52, 32, 8, 2, BAR),
]


def coverage(px, py, x0, y0, x1, y1, r):
    """Fraction of pixel [px,px+1)x[py,py+1) inside the rounded rect."""
    ox = min(px + 1, x1) - max(px, x0)
    oy = min(py + 1, y1) - max(py, y0)
    if ox <= 0 or oy <= 0:
        return 0.0
    if r > 0:
        # Pixel touching a corner square: supersample against the corner circle.
        cx = x0 + r if px < x0 + r else (x1 - r if px + 1 > x1 - r else None)
        cy = y0 + r if py < y0 + r else (y1 - r if py + 1 > y1 - r else None)
        if cx is not None and cy is not None:
            n, hit = 6, 0
            for i in range(n):
                sx = px + (i + 0.5) / n
                for j in range(n):
                    sy = py + (j + 0.5) / n
                    if x0 <= sx < x1 and y0 <= sy < y1:
                        inside_x = (sx >= x0 + r) and (sx <= x1 - r)
                        inside_y = (sy >= y0 + r) and (sy <= y1 - r)
                        if inside_x or inside_y or (sx - cx) ** 2 + (sy - cy) ** 2 <= r * r:
                            hit += 1
            return hit / (n * n)
    return ox * oy


def render(shapes, size, scale=1.0):
    """RGBA bytes; the 128-unit design scaled by `scale` and centered."""
    k = size / 128 * scale
    off = size * (1 - scale) / 2
    px = [[0.0, 0.0, 0.0, 0.0] for _ in range(size * size)]  # premultiplied
    for x, y, w, h, r, (cr, cg, cb) in shapes:
        x0, y0 = x * k + off, y * k + off
        x1, y1, rr = x0 + w * k, y0 + h * k, r * k
        for py in range(max(0, int(y0)), min(size, int(y1) + 1)):
            for qx in range(max(0, int(x0)), min(size, int(x1) + 1)):
                a = coverage(qx, py, x0, y0, x1, y1, rr)
                if a <= 0:
                    continue
                p = px[py * size + qx]
                p[0] = cr / 255 * a + p[0] * (1 - a)
                p[1] = cg / 255 * a + p[1] * (1 - a)
                p[2] = cb / 255 * a + p[2] * (1 - a)
                p[3] = a + p[3] * (1 - a)
    out = bytearray()
    for y in range(size):
        out.append(0)  # PNG filter: none
        for r, g, b, a in px[y * size:(y + 1) * size]:
            if a > 0:
                out += bytes((round(r / a * 255), round(g / a * 255), round(b / a * 255), round(a * 255)))
            else:
                out += b"\0\0\0\0"
    return bytes(out)


def png(size, rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def write(path, size, shapes=None, scale=1.0):
    shapes = shapes or (SIMPLE if size <= 24 else FULL)
    full = os.path.join(ROOT, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "wb") as f:
        f.write(png(size, render(shapes, size, scale)))
    print(path)


def main():
    a = "packaging/Assets"
    for s in (16, 24, 32, 48, 256):
        write(f"{a}/Square44x44Logo.targetsize-{s}.png", s)
        write(f"{a}/Square44x44Logo.targetsize-{s}_altform-unplated.png", s)
    write(f"{a}/Square44x44Logo.scale-100.png", 44)
    write(f"{a}/Square44x44Logo.scale-200.png", 88)
    write(f"{a}/Square150x150Logo.scale-100.png", 150, scale=0.66)
    write(f"{a}/Square150x150Logo.scale-200.png", 300, scale=0.66)
    write(f"{a}/StoreLogo.png", 50)
    for s in (16, 20, 24, 32):
        write(f"assets/tray-{s}.png", s)


if __name__ == "__main__":
    main()
