#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generate a placeholder ToxSocial app icon (1024x1024).

Style: rounded-square teal->purple gradient, a geometric fox face
(Tox mascot) drawn from node-dots and connecting lines, evoking a
decentralized social network. Replace with a real design later and
re-run `tauri icon` (see HANDOVER.md).

Usage:
    python scripts/make-icon.py [output.png]
"""

import sys
import math

from PIL import Image, ImageDraw, ImageFilter

SIZE = 1024


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def draw_gradient(draw, top, bottom, margin):
    for y in range(margin, SIZE - margin):
        t = (y - margin) / (SIZE - 2 * margin)
        draw.line([(margin, y), (SIZE - margin, y)], fill=lerp(top, bottom, t))


def rounded_rect_mask(size, radius):
    mask = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(mask)
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=255)
    return mask


def node_ring(draw, cx, cy, r, n, color, node_r=14, line=True):
    pts = []
    for i in range(n):
        ang = 2 * math.pi * i / n - math.pi / 2
        x = cx + r * math.cos(ang)
        y = cy + r * math.sin(ang)
        pts.append((x, y))
    if line:
        for i in range(n):
            a = pts[i]
            b = pts[(i + 1) % n]
            draw.line([a, b], fill=color, width=6)
    for (x, y) in pts:
        draw.ellipse(
            [x - node_r, y - node_r, x + node_r, y + node_r], fill=color
        )


def main(out_path: str) -> None:
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    margin = 48
    # Background: vertical teal -> purple gradient.
    draw_gradient(d, (45, 212, 191), (124, 58, 237), margin)

    # Subtle decorative node rings (decentralized network).
    node_ring(d, SIZE / 2, SIZE / 2, 330, 8, (255, 255, 255, 36), node_r=10)
    node_ring(d, SIZE / 2, SIZE / 2, 400, 6, (255, 255, 255, 22), node_r=8)

    # Fox face (geometric, centered).
    cx, cy = SIZE / 2, SIZE / 2 + 40
    orange = (255, 140, 56, 255)
    dark = (61, 40, 30, 255)
    white = (255, 250, 240, 255)

    # Ears (two triangles).
    ear_l = [(cx - 240, cy - 90), (cx - 130, cy - 330), (cx - 40, cy - 160)]
    ear_r = [(cx + 240, cy - 90), (cx + 130, cy - 330), (cx + 40, cy - 160)]
    d.polygon(ear_l, fill=orange)
    d.polygon(ear_r, fill=orange)
    # Inner ears.
    d.polygon(
        [(cx - 205, cy - 105), (cx - 140, cy - 265), (cx - 75, cy - 160)],
        fill=dark,
    )
    d.polygon(
        [(cx + 205, cy - 105), (cx + 140, cy - 265), (cx + 75, cy - 160)],
        fill=dark,
    )

    # Face.
    d.ellipse([cx - 240, cy - 140, cx + 240, cy + 220], fill=orange)

    # White cheeks (lower part).
    d.ellipse([cx - 210, cy - 30, cx + 210, cy + 205], fill=white)

    # Nose / mouth (dark triangle + line).
    d.polygon(
        [(cx - 45, cy + 55), (cx + 45, cy + 55), (cx, cy + 120)],
        fill=dark,
    )
    d.line([(cx, cy + 120), (cx, cy + 150)], fill=dark, width=14)
    d.line([(cx - 70, cy + 150), (cx + 70, cy + 150)], fill=dark, width=14)

    # Eyes (dark ovals with white glints).
    for ex in (cx - 110, cx + 110):
        d.ellipse([ex - 34, cy - 70, ex + 34, cy + 10], fill=dark)
        d.ellipse([ex - 10, cy - 56, ex + 8, cy - 38], fill=white)

    # Rounded-square mask + soft edges.
    img = img.crop((0, 0, SIZE, SIZE))
    mask = rounded_rect_mask(SIZE, 210)
    img.putalpha(mask)

    img.save(out_path, "PNG")
    print(f"icon written: {out_path}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "icon-master.png")
