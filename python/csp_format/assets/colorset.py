"""Colour sets (.cls, SLCC), little endian; see ``crates/csp-assets/src/colorset.rs``."""
from __future__ import annotations

import struct
from dataclasses import dataclass, field

from ..errors import CspFormatError


@dataclass
class Color:
    rgba: tuple[int, int, int, int]
    name: str | None = None
    flag: int = 0
    extra: bytes = b""


@dataclass
class ColorSet:
    name: str
    colors: list[Color] = field(default_factory=list)
    version: int = 0x0100
    format: int = 4
    sjis_name: bytes | None = None
    after_sjis: int = 0

    def to_json(self) -> dict:
        """The JSON of ``cspf asset colorset``."""
        return {
            "name": self.name,
            "version": self.version,
            "format": self.format,
            "colors": [{"rgba": list(c.rgba), "name": c.name} for c in self.colors],
        }


def parse(data: bytes) -> ColorSet:
    if not data.startswith(b"SLCC"):
        raise CspFormatError("not a colour set (no SLCC signature)")
    try:
        (version, block) = struct.unpack_from("<HI", data, 4)
        at = 10
        block_end = at + block
        (n,) = struct.unpack_from("<H", data, at)
        sjis = data[at + 2:at + 2 + n]
        at += 2 + n
        (after_sjis, n) = struct.unpack_from("<IH", data, at)
        name = data[at + 6:at + 6 + n].decode("utf-8")
        at += 6 + n
        if at != block_end:
            raise CspFormatError(f"colour set: name block ends at {at:#x}, not {block_end:#x}")
        (fmt, count, size) = struct.unpack_from("<III", data, at)
        at += 12
        if at + size != len(data):
            raise CspFormatError(f"colour set: list of {size} bytes, file has {len(data) - at}")
        colors = []
        for _ in range(count):
            (n,) = struct.unpack_from("<I", data, at)
            entry = data[at + 4:at + 4 + n]
            if n < 8 or len(entry) != n:
                raise CspFormatError(f"colour set: {n}-byte colour")
            at += 4 + n
            (flag,) = struct.unpack_from("<I", entry, 4)
            color_name, end = None, 8
            if flag == 1 and n >= 10:
                (length,) = struct.unpack_from("<H", entry, 8)
                if 10 + length <= n:
                    color_name = entry[10:10 + length].decode("utf-16-le", "replace")
                    end = 10 + length
            colors.append(Color(tuple(entry[:4]), color_name, flag, entry[end:]))
    except (struct.error, UnicodeDecodeError) as error:
        raise CspFormatError(f"colour set: {error}") from error
    if at != len(data):
        raise CspFormatError("colour set: colour list length does not match")
    return ColorSet(name, colors, version, fmt, sjis, after_sjis)


def to_bytes(colors: ColorSet) -> bytes:
    sjis = colors.sjis_name if colors.sjis_name is not None else colors.name.encode("cp932", "replace")
    utf8 = colors.name.encode("utf-8")
    block = struct.pack("<H", len(sjis)) + sjis + struct.pack("<IH", colors.after_sjis, len(utf8)) + utf8
    entries = bytearray()
    for color in colors.colors:
        entry = bytearray(color.rgba)
        if color.name is not None:
            units = color.name.encode("utf-16-le")
            entry += struct.pack("<IH", max(1, color.flag), len(units)) + units
        else:
            entry += struct.pack("<I", color.flag)
        entry += color.extra
        entries += struct.pack("<I", len(entry)) + entry
    head = b"SLCC" + struct.pack("<HI", colors.version, len(block)) + block
    return head + struct.pack("<III", colors.format, len(colors.colors), len(entries)) + bytes(entries)
