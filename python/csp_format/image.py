"""Plain RGBA images and PNG reading/writing with the standard library only."""
from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass

from .errors import CspFormatError

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


@dataclass
class Image:
    """``width × height`` pixels, 4 bytes each (R, G, B, A, not premultiplied), row by row."""

    width: int
    height: int
    rgba: bytearray

    @classmethod
    def blank(cls, width: int, height: int) -> "Image":
        return cls(width, height, bytearray(width * height * 4))

    def pixel(self, x: int, y: int) -> tuple[int, int, int, int]:
        i = (y * self.width + x) * 4
        return tuple(self.rgba[i:i + 4])  # type: ignore[return-value]

    def to_png(self) -> bytes:
        """Encode as an 8-bit RGBA PNG."""
        row = self.width * 4
        raw = b"".join(b"\x00" + bytes(self.rgba[y * row:(y + 1) * row]) for y in range(self.height))
        header = struct.pack(">IIBBBBB", self.width, self.height, 8, 6, 0, 0, 0)
        return PNG_SIGNATURE + _chunk(b"IHDR", header) + _chunk(b"IDAT", zlib.compress(raw, 6)) + _chunk(b"IEND", b"")

    # Pillow interoperability without depending on it: Image.frombytes("RGBA", (w, h), bytes(image.rgba)).


def _chunk(tag: bytes, body: bytes) -> bytes:
    return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)


def decode_png(data: bytes) -> Image:
    """Decode an 8-bit, non-interlaced PNG (RGBA, RGB, gray or gray+alpha), as CSP writes its previews."""
    if data[:8] != PNG_SIGNATURE:
        raise CspFormatError("not a PNG")
    position, header, idat = 8, None, []
    while position + 8 <= len(data):
        (length,) = struct.unpack(">I", data[position:position + 4])
        tag = data[position + 4:position + 8]
        body = data[position + 8:position + 8 + length]
        if tag == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif tag == b"IDAT":
            idat.append(body)
        elif tag == b"IEND":
            break
        position += 12 + length
    if header is None:
        raise CspFormatError("PNG has no IHDR")
    width, height, depth, color, _, _, interlace = header
    channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(color)
    if depth != 8 or channels is None or interlace:
        raise CspFormatError(f"unsupported PNG (depth {depth}, color type {color}, interlace {interlace})")
    raw = zlib.decompress(b"".join(idat))
    stride = width * channels
    rows: list[bytearray] = []
    previous = bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind = raw[start]
        line = bytearray(raw[start + 1:start + 1 + stride])
        _unfilter(kind, line, previous, channels)
        rows.append(line)
        previous = line
    out = bytearray(width * height * 4)
    for y, line in enumerate(rows):
        base = y * width * 4
        if channels == 4:
            out[base:base + width * 4] = line
            continue
        target = memoryview(out)[base:base + width * 4]
        if channels == 3:
            target[0::4], target[1::4], target[2::4] = line[0::3], line[1::3], line[2::3]
            target[3::4] = b"\xff" * width
        elif channels == 2:
            target[0::4] = target[1::4] = target[2::4] = line[0::2]
            target[3::4] = line[1::2]
        else:
            target[0::4] = target[1::4] = target[2::4] = line
            target[3::4] = b"\xff" * width
    return Image(width, height, out)


def _unfilter(kind: int, line: bytearray, previous: bytearray, bpp: int) -> None:
    n = len(line)
    if kind == 0:
        return
    if kind == 1:
        for i in range(bpp, n):
            line[i] = (line[i] + line[i - bpp]) & 0xFF
    elif kind == 2:
        for i in range(n):
            line[i] = (line[i] + previous[i]) & 0xFF
    elif kind == 3:
        for i in range(n):
            left = line[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
    elif kind == 4:
        for i in range(n):
            a = line[i - bpp] if i >= bpp else 0
            b = previous[i]
            c = previous[i - bpp] if i >= bpp else 0
            p = a + b - c
            pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            predictor = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
            line[i] = (line[i] + predictor) & 0xFF
    else:
        raise CspFormatError(f"unknown PNG filter {kind}")
