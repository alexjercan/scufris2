#!/usr/bin/env python3
"""Generate the provisional Scufris iOS application icon."""

import struct
import zlib
from pathlib import Path

SIZE = 1024
OUTPUT = (
    Path(__file__).resolve().parent.parent
    / "Sources"
    / "Assets.xcassets"
    / "AppIcon.appiconset"
    / "AppIcon.png"
)


def inside_rounded_rectangle(
    x: int, y: int, left: int, top: int, right: int, bottom: int, radius: int
) -> bool:
    nearest_x = min(max(x, left + radius), right - radius)
    nearest_y = min(max(y, top + radius), bottom - radius)
    dx = x - nearest_x
    dy = y - nearest_y
    return dx * dx + dy * dy <= radius * radius


def pixel(x: int, y: int) -> tuple[int, int, int]:
    gradient = (x + y) / (2 * (SIZE - 1))
    red = round(20 + 35 * gradient)
    green = round(19 + 38 * gradient)
    blue = round(64 + 80 * gradient)

    center_x = 512
    center_y = 470
    glow_distance = ((x - center_x) ** 2 + (y - center_y) ** 2) ** 0.5
    glow = max(0.0, 1.0 - glow_distance / 560)
    red = min(255, round(red + 15 * glow))
    green = min(255, round(green + 44 * glow))
    blue = min(255, round(blue + 58 * glow))

    in_bubble = inside_rounded_rectangle(x, y, 210, 250, 814, 700, 145)
    in_tail = y >= 620 and y <= 796 and x >= 520 and x <= 735 and x + y <= 1390

    bar_width = 58
    bar_centers = (382, 512, 642)
    bar_heights = (130, 240, 170)
    for bar_center, bar_height in zip(bar_centers, bar_heights, strict=True):
        if inside_rounded_rectangle(
            x,
            y,
            bar_center - bar_width // 2,
            475 - bar_height // 2,
            bar_center + bar_width // 2,
            475 + bar_height // 2,
            bar_width // 2,
        ):
            return (42, 45, 105)

    if in_bubble or in_tail:
        return (224, 248, 255)

    return (red, green, blue)


def png_chunk(kind: bytes, data: bytes) -> bytes:
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


def main() -> None:
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    rows = bytearray()
    for y in range(SIZE):
        rows.append(0)
        for x in range(SIZE):
            rows.extend(pixel(x, y))

    png = bytearray(b"\x89PNG\r\n\x1a\n")
    png.extend(png_chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 2, 0, 0, 0)))
    png.extend(png_chunk(b"IDAT", zlib.compress(bytes(rows), level=9)))
    png.extend(png_chunk(b"IEND", b""))
    OUTPUT.write_bytes(png)


if __name__ == "__main__":
    main()
