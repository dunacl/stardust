#!/usr/bin/env python3
"""Makes the macOS app icon from the square master.

macOS does not round an app's icon: the image must carry the shape. This
puts the master in Apple's 824-point rounded square on a transparent
1024-point canvas. Standard library only.

Usage: sips -z 824 824 assets/icon/icon-1024.png --out /tmp/824.png
       scripts/macos-icon.py /tmp/824.png assets/icon/icon-macos.png
"""
import math
import struct
import sys
import zlib

CANVAS = 1024
RADIUS = 185


def chunks(data):
    at = 8
    while at < len(data):
        length, kind = struct.unpack(">I4s", data[at : at + 8])
        yield kind, data[at + 8 : at + 8 + length]
        at += 12 + length


def read_png(path):
    """Decodes an 8-bit, non-interlaced RGB or RGBA PNG to rows of bytes."""
    with open(path, "rb") as file:
        data = file.read()
    idat = b""
    for kind, body in chunks(data):
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or colour not in (2, 6) or interlace:
                sys.exit("macos-icon: need an 8-bit, non-interlaced RGB or RGBA PNG")
            channels = 3 if colour == 2 else 4
        elif kind == b"IDAT":
            idat += body
    raw = zlib.decompress(idat)
    stride = width * channels
    rows, previous, at = [], bytearray(stride), 0
    for _ in range(height):
        kind = raw[at]
        row = bytearray(raw[at + 1 : at + 1 + stride])
        at += 1 + stride
        for i in range(stride):
            left = row[i - channels] if i >= channels else 0
            up = previous[i]
            corner = previous[i - channels] if i >= channels else 0
            if kind == 1:
                row[i] = (row[i] + left) & 255
            elif kind == 2:
                row[i] = (row[i] + up) & 255
            elif kind == 3:
                row[i] = (row[i] + (left + up) // 2) & 255
            elif kind == 4:
                p = left + up - corner
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - corner)
                best = left if pa <= pb and pa <= pc else up if pb <= pc else corner
                row[i] = (row[i] + best) & 255
        rows.append(row)
        previous = row
    return width, height, channels, rows


def coverage(x, y, size):
    """How much of the pixel at (x, y) lies inside the rounded square."""
    half = size / 2
    dx = max(abs(x + 0.5 - half) - (half - RADIUS), 0.0)
    dy = max(abs(y + 0.5 - half) - (half - RADIUS), 0.0)
    return min(max(RADIUS + 0.5 - math.hypot(dx, dy), 0.0), 1.0)


def write_png(path, rows):
    def chunk(kind, body):
        crc = zlib.crc32(kind + body)
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)

    header = struct.pack(">IIBBBBB", CANVAS, CANVAS, 8, 6, 0, 0, 0)
    body = zlib.compress(b"".join(b"\0" + bytes(row) for row in rows), 9)
    with open(path, "wb") as file:
        file.write(b"\x89PNG\r\n\x1a\n")
        file.write(chunk(b"IHDR", header) + chunk(b"IDAT", body) + chunk(b"IEND", b""))


def main():
    width, height, channels, source = read_png(sys.argv[1])
    if width != height or width > CANVAS:
        sys.exit("macos-icon: need a square image no wider than the canvas")
    margin = (CANVAS - width) // 2
    rows = [bytearray(CANVAS * 4) for _ in range(CANVAS)]
    for y in range(height):
        out, row = rows[y + margin], source[y]
        for x in range(width):
            at = (x + margin) * 4
            out[at : at + 3] = row[x * channels : x * channels + 3]
            out[at + 3] = round(255 * coverage(x, y, width))
    write_png(sys.argv[2], rows)


main()
