#!/usr/bin/env python3
"""Regenerate compact Plymouth assets from the approved 02 vector master.

Requires Python, pycairo, and Pillow only when editing the art. The ISO build
uses the committed PNGs and needs no render-time Python dependencies.
"""
from __future__ import annotations

import io
import math
import re
import xml.etree.ElementTree as ET
from pathlib import Path

import cairo
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "profile/airootfs/usr/share/plymouth/themes/02-turn-ripple"
WIDTH = 1280
CANVAS = (512, 448)
CYAN = (0, 186, 243)
TURN_FRAMES = 34


def path_to_cairo(ctx, path):
    tokens = re.findall(r"[MLCQVHZ]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?", path)
    position = 0
    while position < len(tokens):
        operation = tokens[position]
        position += 1
        if operation == "Z":
            ctx.close_path()
            continue
        count = {"M": 2, "L": 2, "C": 6, "Q": 4, "V": 1, "H": 1}[operation]
        arguments = [float(value) for value in tokens[position:position + count]]
        position += count
        if operation == "M":
            ctx.move_to(*arguments)
        elif operation == "L":
            ctx.line_to(*arguments)
        elif operation == "C":
            ctx.curve_to(*arguments)
        elif operation == "Q":
            x0, y0 = ctx.get_current_point()
            x1, y1, x2, y2 = arguments
            ctx.curve_to(x0 + 2 * (x1 - x0) / 3, y0 + 2 * (y1 - y0) / 3,
                         x2 + 2 * (x1 - x2) / 3, y2 + 2 * (y1 - y2) / 3, x2, y2)
        elif operation == "V":
            ctx.line_to(ctx.get_current_point()[0], arguments[0])
        elif operation == "H":
            ctx.line_to(arguments[0], ctx.get_current_point()[1])


def draw_tree(ctx, element):
    ctx.save()
    transform = element.get("transform", "")
    if transform:
        match = re.fullmatch(r"translate\(([-\d.]+)\s+([-\d.]+)\)", transform)
        if not match:
            raise ValueError(f"Unsupported vector transform: {transform}")
        ctx.translate(*map(float, match.groups()))
    if element.tag.split("}")[-1] == "path":
        ctx.new_path()
        path_to_cairo(ctx, element.get("d"))
        ctx.stroke()
    for child in element:
        draw_tree(ctx, child)
    ctx.restore()


def logo_image(master, width=WIDTH, canvas=CANVAS, supersample=3):
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32,
                                 canvas[0] * supersample, canvas[1] * supersample)
    ctx = cairo.Context(surface)
    ctx.scale(supersample, supersample)
    ctx.translate(canvas[0] / 2, canvas[1] / 2)
    ctx.scale(width * 0.2375 / 680, width * 0.2375 / 680)
    ctx.set_source_rgb(*(channel / 255 for channel in CYAN))
    ctx.set_line_width(32)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    ctx.set_line_join(cairo.LINE_JOIN_ROUND)
    draw_tree(ctx, master)
    buffer = io.BytesIO()
    surface.write_to_png(buffer)
    buffer.seek(0)
    return Image.open(buffer).convert("RGBA").resize(canvas, Image.Resampling.LANCZOS)


def project(asset, time):
    turn = min(1, max(0, (time - 0.08) / 1.02))
    turn = 1 - (1 - turn) ** 3
    angle = math.radians(88 * (1 - turn))
    scale = 0.94 + 0.06 * turn
    cx, cy = asset.width / 2, asset.height / 2
    a = math.cos(angle) * scale
    b = math.sin(angle) * scale / (600 * WIDTH / 736)
    d = a + b * cx
    coefficients = ((1 - cx * b) / d, 0, (cx * d - cx) / d,
                    -cy * b / d, (a / scale) / d, (cy * d - cy * a / scale) / d,
                    -b / d, 0)
    return asset.transform(CANVAS, Image.Transform.PERSPECTIVE, coefficients,
                           Image.Resampling.BICUBIC, fillcolor=(0, 0, 0, 0))


def main():
    master = ET.parse(Path(__file__).with_name("02-logo.svg")).getroot()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    logo = logo_image(master)
    # The static master is larger for a crisp long-boot holding state and HiDPI.
    logo_image(master, width=2560, canvas=(1024, 896)).save(OUTPUT / "logo.png", optimize=True)
    for index in range(TURN_FRAMES):
        project(logo, index / 30).save(OUTPUT / f"turn-{index}.png", optimize=True)
    supersample = 3
    ring = Image.new("RGBA", (512 * supersample, 512 * supersample), (0, 0, 0, 0))
    draw = ImageDraw.Draw(ring)
    draw.ellipse((6 * supersample, 6 * supersample, 506 * supersample, 506 * supersample),
                 outline=CYAN + (255,), width=2 * supersample)
    ring.resize((512, 512), Image.Resampling.LANCZOS).save(OUTPUT / "ripple.png", optimize=True)
    assets = list(OUTPUT.glob("*.png"))
    print(f"Generated {len(assets)} PNG assets, {sum(path.stat().st_size for path in assets):,} bytes")


if __name__ == "__main__":
    main()
