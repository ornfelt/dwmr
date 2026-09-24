#!/usr/bin/env python3
"""Generate dwmr.png, the dwmr logo, in the blocky style of dwm's dwm.png.

The logo is drawn on a grid of square cells (16px each by default, like
dwm.png). Only the Python standard library is used.

Examples:
    tools/logo.py                      # transparent background, writes dwmr.png
    tools/logo.py --opaque             # white background, like dwm.png
    tools/logo.py --fg '#ffffff' -o /tmp/dwmr-dark.png
    tools/logo.py --rgap 1             # r one full cell away from "dwm"
"""

import argparse
import os
import struct
import sys
import zlib

# '#' is a foreground cell. d, w and m share their vertical strokes like in
# dwm.png; the r stands apart so the word reads as "dwmr".
LOGO = [
    "...#............",
    "...#............",
    "####.#.#####.###",
    "#..#.#.#.#.#.#..",
    "########.#.#.#..",
]

# Column of LOGO that separates the r from "dwm". It is drawn narrower than a
# full cell (see --rgap) so the r sits a bit closer to the rest of the word.
RGAP_COLUMN = 12


def parse_color(s):
    """Parse "#rrggbb" into an (r, g, b) tuple."""
    h = s.lstrip("#")
    if len(h) != 6:
        raise argparse.ArgumentTypeError(f"invalid colour '{s}', expected #rrggbb")
    try:
        return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid colour '{s}', expected #rrggbb")


def png_chunk(kind, data):
    chunk = kind + data
    return struct.pack(">I", len(data)) + chunk + struct.pack(">I", zlib.crc32(chunk) & 0xffffffff)


def column_widths(rows, unit, rgap):
    """Return the pixel width of each grid column."""
    widths = [unit] * len(rows[0])
    widths[RGAP_COLUMN] = max(1, round(unit * rgap))
    return widths


def render(rows, widths, unit, fg, bg, transparent):
    """Return the PNG file bytes for the grid in rows."""
    width = sum(widths)
    height = len(rows) * unit
    if transparent:
        fgpx = bytes(fg) + b"\xff"
        bgpx = bytes(bg) + b"\x00"
        colortype = 6  # RGBA
    else:
        fgpx = bytes(fg)
        bgpx = bytes(bg)
        colortype = 2  # RGB

    raw = bytearray()
    for row in rows:
        line = b"\x00" + b"".join((fgpx if cell == "#" else bgpx) * w for cell, w in zip(row, widths))
        raw += line * unit

    ihdr = struct.pack(">IIBBBBB", width, height, 8, colortype, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n"
            + png_chunk(b"IHDR", ihdr)
            + png_chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + png_chunk(b"IEND", b""))


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ap = argparse.ArgumentParser(description="Generate the dwmr logo (dwmr.png).")
    ap.add_argument("-o", "--output", default=os.path.join(root, "dwmr.png"),
                    help="output file (default: dwmr.png in the repository root)")
    bgmode = ap.add_mutually_exclusive_group()
    bgmode.add_argument("--transparent", dest="transparent", action="store_true", default=True,
                        help="transparent background (default)")
    bgmode.add_argument("--opaque", dest="transparent", action="store_false",
                        help="fill the background with --bg")
    ap.add_argument("--fg", type=parse_color, default=parse_color("#000000"),
                    help="letter colour (default: #000000)")
    ap.add_argument("--bg", type=parse_color, default=parse_color("#ffffff"),
                    help="background colour with --opaque (default: #ffffff)")
    ap.add_argument("--unit", type=int, default=16,
                    help="size of one grid cell in pixels (default: 16, like dwm.png)")
    ap.add_argument("--rgap", type=float, default=0.5,
                    help="gap between \"dwm\" and the r, as a fraction of a cell (0 < rgap <= 1, default: 0.5)")
    args = ap.parse_args()

    if args.unit < 1:
        ap.error("--unit must be at least 1")
    if not 0 < args.rgap <= 1:
        ap.error("--rgap must be greater than 0 and at most 1")

    widths = column_widths(LOGO, args.unit, args.rgap)
    data = render(LOGO, widths, args.unit, args.fg, args.bg, args.transparent)
    with open(args.output, "wb") as f:
        f.write(data)
    print(f"wrote {args.output} ({sum(widths)}x{len(LOGO) * args.unit}, "
          f"{'transparent' if args.transparent else 'opaque'} background)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
