"""Text layers: ``TextLayerString`` (UTF-8) and its attributes (spec: docs/spec/06-text.md).

``TextLayerAttributes`` is a little-endian list of records ``u32 id, u32
length, bytes``. A layer with several text blocks (balloons) keeps them in
``TextLayerStringArray`` / ``TextLayerAttributesArray``: ``u32le length`` +
string / attribute list per block.
"""
from __future__ import annotations

import struct
from dataclasses import dataclass

from .errors import CspFormatError

RUN_FONT_SIZE = 11  # character runs with a font size
FONT_SIZE = 32  # font size × 100 in points (u32)
BBOX = 42  # text block bounding box, layer pixels (i32 × 4)
RENDER_OFFSCREEN = 50  # offscreen CSP rendered the text into (0 = render again)
FONT = 57  # u16 ?, u16 length, UTF-8 name, u16 length, PostScript name
BLOCK_SIZE = 63  # text block width and height in pixels (i32 × 2)
QUAD = 64  # four corners in 1/100 px relative to the anchor (i32 × 8)


@dataclass(frozen=True)
class Font:
    lead: int
    name: str
    postscript_name: str


@dataclass(frozen=True)
class Attributes:
    """A parsed attribute list: ``(id, bytes)`` records in file order."""

    records: tuple[tuple[int, bytes], ...] = ()

    @classmethod
    def parse(cls, blob: bytes) -> "Attributes":
        records, position = [], 0
        while position < len(blob):
            if position + 8 > len(blob):
                raise CspFormatError("text attribute record header is truncated")
            record_id, length = struct.unpack("<II", blob[position:position + 8])
            position += 8
            if length > len(blob) - position:
                raise CspFormatError(f"text attribute {record_id} says {length} bytes, {len(blob) - position} remain")
            records.append((record_id, blob[position:position + length]))
            position += length
        return cls(tuple(records))

    def get(self, record_id: int) -> bytes | None:
        return next((data for rid, data in self.records if rid == record_id), None)

    def _i32s(self, record_id: int) -> list[int] | None:
        data = self.get(record_id)
        return None if data is None else list(struct.unpack(f"<{len(data) // 4}i", data[:len(data) // 4 * 4]))

    @property
    def font_size(self) -> float | None:
        """Font size in points."""
        values = self._i32s(FONT_SIZE)
        return values[0] / 100 if values else None

    @property
    def bbox(self) -> tuple[int, int, int, int] | None:
        values = self._i32s(BBOX)
        return tuple(values) if values and len(values) == 4 else None  # type: ignore[return-value]

    @property
    def block_size(self) -> tuple[int, int] | None:
        values = self._i32s(BLOCK_SIZE)
        return tuple(values) if values and len(values) == 2 else None  # type: ignore[return-value]

    @property
    def quad(self) -> tuple[int, ...] | None:
        values = self._i32s(QUAD)
        return tuple(values) if values and len(values) == 8 else None

    @property
    def render_offscreen(self) -> int | None:
        values = self._i32s(RENDER_OFFSCREEN)
        return values[0] & 0xFFFFFFFF if values else None

    @property
    def font(self) -> Font | None:
        data = self.get(FONT)
        try:
            lead, length = struct.unpack("<HH", data[:4])  # type: ignore[index]
            name = data[4:4 + length]
            (ps_length,) = struct.unpack("<H", data[4 + length:6 + length])
            ps_name = data[6 + length:6 + length + ps_length]
        except (TypeError, struct.error):
            return None
        if len(name) != length or len(ps_name) != ps_length:
            return None
        return Font(lead, name.decode("utf-8", "replace"), ps_name.decode("utf-8", "replace"))


def split_array(blob: bytes) -> list[bytes]:
    """Split a ``…Array`` column: ``u32le length`` + bytes, repeated."""
    parts, position = [], 0
    while position < len(blob):
        if position + 4 > len(blob):
            raise CspFormatError("array entry length is truncated")
        (length,) = struct.unpack("<I", blob[position:position + 4])
        position += 4
        if length > len(blob) - position:
            raise CspFormatError(f"array entry of {length} bytes, {len(blob) - position} remain")
        parts.append(blob[position:position + length])
        position += length
    return parts


@dataclass(frozen=True)
class TextBlock:
    text: str
    attributes: Attributes


def _blob(row, column: str) -> bytes | None:
    if column not in row.keys():
        return None
    value = row[column]
    if isinstance(value, str):
        return value.encode("utf-8")
    return bytes(value) if isinstance(value, (bytes, bytearray, memoryview)) else None


def read_blocks(row) -> list[TextBlock]:
    """The text blocks of a ``Layer`` row: the single string and attributes
    first, then one block per entry of the arrays (the arrays hold the other
    blocks, not a copy of the first)."""
    blocks = []
    text, attrs = _blob(row, "TextLayerString"), _blob(row, "TextLayerAttributes")
    if text is not None or attrs is not None:
        blocks.append(TextBlock((text or b"").decode("utf-8", "replace"), Attributes.parse(attrs) if attrs else Attributes()))
    strings = _blob(row, "TextLayerStringArray")
    attributes = _blob(row, "TextLayerAttributesArray")
    strings = split_array(strings) if strings is not None else []
    attributes = split_array(attributes) if attributes is not None else []
    for index in range(max(len(strings), len(attributes))):
        string = strings[index] if index < len(strings) else b""
        blob = attributes[index] if index < len(attributes) else b""
        blocks.append(TextBlock(string.decode("utf-8", "replace"), Attributes.parse(blob)))
    return blocks
