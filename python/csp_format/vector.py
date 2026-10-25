"""Vector data (``VectorObjectList.VectorData``, an external chunk; spec: docs/spec/05-vector.md).

Big endian::

    u32 0, u32 length of the objects
    object × n:
      header (header_size bytes)
        +0  u32 header_size (76, 88, 92, 96) +4  u32 72
        +8  u32 point_size (88, 104)       +12 u32 88
        +16 u32 point_count                +20 u32 flags
        +24 i32 bbox x0, y0, x1, y1 (layer pixels)
        +40 u32 main colour R, G, B        +52 u32 sub colour R, G, B
        +64 f64 opacity (1.0)
        +72 …  brush style ids (u32 × 1–3), the brush size (f64), u32
      point × point_count (point_size bytes)
        +0 f64 x  +8 f64 y  +16 i32 bbox x0, y0, x1, y1  +32 u32 flags
        +36 … f32 values (pressure, width ratio, …), +88 f64 x, y (104-byte points)

Coordinates are layer pixels; add ``LayerOffsetX/Y`` for the canvas.
"""
from __future__ import annotations

import struct
from dataclasses import dataclass

from .errors import CspFormatError

FLAG_CLOSED = 0x1000


@dataclass(frozen=True)
class Point:
    x: float
    y: float
    bbox: tuple[int, int, int, int]
    flags: int
    #: The f32 values after the flags (meaning partly unknown, see the spec).
    values: tuple[float, ...]

    @classmethod
    def parse(cls, raw: bytes) -> "Point":
        x, y = struct.unpack(">dd", raw[:16])
        bbox = struct.unpack(">4i", raw[16:32])
        (flags,) = struct.unpack(">I", raw[32:36])
        count = min(13, (len(raw) - 36) // 4)
        return cls(x, y, bbox, flags, struct.unpack(f">{count}f", raw[36:36 + 4 * count]))

    @property
    def pressure(self) -> float:
        """Value 0 (筆圧, 0–1)."""
        return self.values[0]


@dataclass(frozen=True)
class VectorObject:
    """One stroke or shape; ``header`` and every point keep their raw bytes."""

    header: bytes
    point_size: int
    raw_points: tuple[bytes, ...]

    def _u32(self, offset: int) -> int:
        return struct.unpack(">I", self.header[offset:offset + 4])[0]

    @property
    def flags(self) -> int:
        return self._u32(20)

    @property
    def is_closed(self) -> bool:
        return bool(self.flags & FLAG_CLOSED)

    @property
    def bbox(self) -> tuple[int, int, int, int]:
        return struct.unpack(">4i", self.header[24:40])

    @property
    def main_color(self) -> tuple[int, int, int]:
        """8-bit RGB (each channel is stored as 32 bits whose top byte is the 8-bit value)."""
        return tuple(self._u32(40 + 4 * i) >> 24 for i in range(3))  # type: ignore[return-value]

    @property
    def sub_color(self) -> tuple[int, int, int]:
        return tuple(self._u32(52 + 4 * i) >> 24 for i in range(3))  # type: ignore[return-value]

    @property
    def opacity(self) -> float:
        return struct.unpack(">d", self.header[64:72])[0]

    def _brush_size_offset(self) -> int | None:
        return len(self.header) - 12 if len(self.header) in (88, 92, 96) else None

    @property
    def brush_size(self) -> float | None:
        """Brush size in pixels (None for header layouts without one)."""
        offset = self._brush_size_offset()
        return None if offset is None else struct.unpack(">d", self.header[offset:offset + 8])[0]

    @property
    def brush_styles(self) -> list[int]:
        """``BrushStyle.MainId``s the header refers to."""
        end = min(self._brush_size_offset() or 76, len(self.header))
        return [self._u32(offset) for offset in range(72, end, 4)]

    @property
    def points(self) -> list[Point]:
        return [Point.parse(raw) for raw in self.raw_points]

    @property
    def coordinates(self) -> list[tuple[float, float]]:
        return [struct.unpack(">dd", raw[:16]) for raw in self.raw_points]  # type: ignore[misc]


def parse_vector_data(payload: bytes) -> list[VectorObject]:
    """All objects of one ``VectorData`` payload."""
    if len(payload) < 8:
        raise CspFormatError("VectorData is shorter than its header")
    _, length = struct.unpack(">II", payload[:8])
    end = 8 + length
    if end > len(payload):
        raise CspFormatError("VectorData length exceeds the payload")
    objects, position = [], 8
    while position < end:
        if position + 20 > end:
            raise CspFormatError(f"vector object at {position} is truncated")
        header_size = struct.unpack(">I", payload[position:position + 4])[0]
        point_size = struct.unpack(">I", payload[position + 8:position + 12])[0]
        point_count = struct.unpack(">I", payload[position + 16:position + 20])[0]
        if header_size < 24 or point_size < 16:
            raise CspFormatError(f"vector object at {position}: header {header_size} / point {point_size} bytes")
        points_start = position + header_size
        if points_start + point_count * point_size > end:
            raise CspFormatError(f"vector object at {position} overruns the data")
        header = payload[position:points_start]
        raw_points = tuple(payload[points_start + i * point_size:points_start + (i + 1) * point_size]
                           for i in range(point_count))
        objects.append(VectorObject(header, point_size, raw_points))
        position = points_start + point_count * point_size
    return objects
