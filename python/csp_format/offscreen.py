"""Raster pixels: ``Offscreen.Attribute`` and its block data (spec: docs/spec/04-raster.md).

An offscreen is a grid of 256×256 blocks. The attribute (big endian) is
``u32 16, u32 × 3 section lengths``, then the sections ``Parameter``,
``InitColor`` and ``BlockSize``, each a UTF-16 name followed by ``u32``
values. A decompressed block is an alpha plane followed by an image plane:

| channel order | kind | planes |
|---|---|---|
| 0x21 | colour | alpha 256×256, then BGRA 256×256×4 (4th byte unused) |
| 0x11, alpha row 256 | gray | alpha, then gray |
| 0x11, alpha row 32 | monochrome | 1 bit/pixel alpha, then 1 bit/pixel |
| 0x01 | alpha only (masks) | one plane |

The payload is ``u64 length`` then one record per block (``u32 length,
"BlockDataBeginChunk", u32 index, u32 raw size, u32 256, u32 256, u32 has
data, [u32 4 + n, u32le n, n bytes of zlib], "BlockDataEndChunk"``) and
the tables ``BlockStatus`` / ``BlockCheckSum``.
"""
from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass

from .errors import CspFormatError
from .image import Image

BLOCK = 256
BEGIN = "BlockDataBeginChunk"
END = "BlockDataEndChunk"


def _name(data: bytes, offset: int) -> tuple[str, int]:
    (length,) = struct.unpack(">I", data[offset:offset + 4])
    end = offset + 4 + 2 * length
    if end > len(data):
        raise CspFormatError("name runs past the end")
    return data[offset + 4:end].decode("utf-16-be", "replace"), end


@dataclass(frozen=True)
class Parameter:
    width: int
    height: int
    blocks_x: int
    blocks_y: int
    channel_order: int
    image_channels: int
    image_offset: int
    image_row_bytes: int
    alpha_row_bytes: int

    @property
    def kind(self) -> str:
        if self.channel_order == 0x21 and self.image_channels == 4:
            return "color"
        if self.channel_order == 0x11 and self.image_channels == 1:
            return "monochrome" if self.alpha_row_bytes == BLOCK // 8 else "gray"
        if self.channel_order == 0x01 and self.image_channels == 0:
            return "alpha"
        return "unknown"

    @property
    def block_bytes(self) -> int:
        alpha = self.alpha_row_bytes * BLOCK
        if self.image_channels == 0:
            return alpha
        return max(self.image_offset, alpha) + self.image_row_bytes * BLOCK


@dataclass(frozen=True)
class Attribute:
    parameter: Parameter
    init_alpha: int | None  # alpha of blocks that are not stored, when InitColor says so

    @classmethod
    def parse(cls, data: bytes) -> "Attribute":
        if len(data) < 16 or struct.unpack(">I", data[:4])[0] != 16:
            raise CspFormatError("offscreen attribute does not start with 16")
        lengths = struct.unpack(">III", data[4:16])
        sections: dict[str, list[int]] = {}
        position = 16
        for length in lengths:
            start = position
            name, position = _name(data, position)
            end = start + length
            if end > len(data) or (end - position) % 4:
                raise CspFormatError(f"offscreen attribute section {name!r} has a bad length")
            sections[name] = list(struct.unpack(f">{(end - position) // 4}I", data[position:end]))
            position = end
        values = sections.get("Parameter")
        if not values or len(values) < 18:
            raise CspFormatError("offscreen attribute has no Parameter section")
        parameter = Parameter(
            width=values[0], height=values[1], blocks_x=values[2], blocks_y=values[3],
            channel_order=values[4], image_channels=values[6], image_offset=values[8],
            image_row_bytes=values[10], alpha_row_bytes=values[12],
        )
        if parameter.blocks_x != -(-parameter.width // BLOCK) or parameter.blocks_y != -(-parameter.height // BLOCK):
            raise CspFormatError("block grid does not fit the offscreen size")
        if values[14] != BLOCK or values[15] != BLOCK:
            raise CspFormatError(f"unsupported block size {values[14]}×{values[15]}")
        if parameter.width > 1 << 20 or parameter.height > 1 << 20:
            raise CspFormatError("offscreen too large")
        init = sections.get("InitColor", [])
        init_alpha = init[2] & 0xFF if len(init) > 2 and init[1] else None
        return cls(parameter, init_alpha)


def iter_blocks(payload: bytes, block_bytes: int, inflate=True):
    """Yield ``(block index, raw bytes or None)`` for every block record.

    ``inflate`` False (or a function of the index that returns False) skips
    decompressing: a stored block then yields its compressed bytes.
    """
    (length,) = struct.unpack(">Q", payload[:8])
    if length != len(payload) - 8:
        raise CspFormatError("block data length does not match")
    position = 8
    while position < len(payload):
        start = position
        try:
            name, _ = _name(payload, position + 4)
        except CspFormatError:
            name = ""
        if name != BEGIN:
            break  # the BlockStatus / BlockCheckSum tables follow the records
        (record_length,) = struct.unpack(">I", payload[position:position + 4])
        _, position = _name(payload, position + 4)
        index, raw_size, _, _, has_data = struct.unpack(">5I", payload[position:position + 20])
        position += 20
        raw = None
        if has_data == 1:
            stored, zlib_length = struct.unpack(">I", payload[position:position + 4])[0], struct.unpack(
                "<I", payload[position + 4:position + 8])[0]
            if stored != zlib_length + 4:
                raise CspFormatError(f"block {index}: stored length {stored} vs zlib length {zlib_length}")
            raw = payload[position + 8:position + 8 + zlib_length]
            if inflate is True or (callable(inflate) and inflate(index)):
                raw = zlib.decompress(raw)
                if len(raw) != raw_size or len(raw) < block_bytes:
                    raise CspFormatError(f"block {index} has {len(raw)} bytes, expected {raw_size}")
            position += 8 + zlib_length
        elif has_data != 0:
            raise CspFormatError(f"block {index}: unknown data flag {has_data}")
        end_name, position = _name(payload, position)
        if end_name != END or position - start != record_length:
            raise CspFormatError(f"block {index}: bad record")
        yield index, raw


def stored_blocks(attribute: bytes, payload: bytes | None) -> list[tuple[int, int, int, int]]:
    """Boxes ``(x0, y0, x1, y1)`` in offscreen pixels of the blocks that store data."""
    if payload is None:
        return []
    p = Attribute.parse(attribute).parameter
    boxes = []
    for index, raw in iter_blocks(payload, p.block_bytes, inflate=False):
        if raw is not None:
            left, top = (index % p.blocks_x) * BLOCK, (index // p.blocks_x) * BLOCK
            boxes.append((left, top, min(left + BLOCK, p.width), min(top + BLOCK, p.height)))
    return boxes


def decode(attribute: bytes, payload: bytes | None, mask: bool = False,
           box: tuple[int, int, int, int] | None = None) -> Image | None:
    """Decode an offscreen to RGBA (not premultiplied).

    With ``mask``, the first (alpha) plane as opaque gray, as masks are
    read. With ``box`` (``x0, y0, x1, y1`` in offscreen pixels, may reach
    outside), only that part, so a large sheet need not be decoded whole.
    Blocks that are not stored are transparent (or the init colour: white
    with its alpha). None when no block data is stored.
    """
    if payload is None:
        return None
    attr = Attribute.parse(attribute)
    p = attr.parameter
    kind = p.kind
    if kind == "unknown":
        raise CspFormatError(f"unknown pixel layout (channel order {p.channel_order:#x})")
    x0, y0, x1, y1 = box if box is not None else (0, 0, p.width, p.height)
    out_width, out_height = max(x1 - x0, 0), max(y1 - y0, 0)
    image = Image.blank(out_width, out_height)
    # The init colour covers the offscreen only, not the part of a box outside it.
    inside = (max(x0, 0), max(y0, 0), min(x1, p.width), min(y1, p.height))
    if attr.init_alpha is not None or mask:
        value = attr.init_alpha or 0
        fill = bytes([value, value, value, 255]) if mask else bytes([255, 255, 255, value])
        span = max(inside[2] - inside[0], 0)
        for y in range(inside[1], inside[3]):
            start = ((y - y0) * out_width + inside[0] - x0) * 4
            image.rgba[start:start + span * 4] = fill * span
    view = memoryview(image.rgba)
    count = 0
    for index, raw in iter_blocks(payload, p.block_bytes, inflate=lambda i: _overlaps(p, i, inside)):
        count += 1
        if raw is None:
            continue
        left, top = (index % p.blocks_x) * BLOCK, (index // p.blocks_x) * BLOCK
        bx0, by0 = max(left, inside[0]), max(top, inside[1])
        bx1, by1 = min(left + BLOCK, inside[2]), min(top + BLOCK, inside[3])
        width = bx1 - bx0
        if width <= 0 or by1 <= by0:
            continue
        cx = bx0 - left  # first column inside the block
        for y in range(by0 - top, by1 - top):
            alpha = _plane(raw, 0, p.alpha_row_bytes, y, cx, width)
            start = ((top + y - y0) * out_width + bx0 - x0) * 4
            target = view[start:start + width * 4]
            if mask:
                target[0::4] = target[1::4] = target[2::4] = alpha
                target[3::4] = b"\xff" * width
                continue
            if kind == "color":
                at = p.image_offset + y * p.image_row_bytes + cx * 4
                row = raw[at:at + width * 4]
                target[0::4], target[1::4], target[2::4] = row[2::4], row[1::4], row[0::4]
            elif kind == "alpha":
                target[0::4] = target[1::4] = target[2::4] = bytes(width)
            else:
                target[0::4] = target[1::4] = target[2::4] = _plane(raw, p.image_offset, p.image_row_bytes, y, cx, width)
            target[3::4] = alpha
    if count != p.blocks_x * p.blocks_y:
        raise CspFormatError(f"block data has {count} blocks, the grid {p.blocks_x * p.blocks_y}")
    return image


def _overlaps(p: Parameter, index: int, box: tuple[int, int, int, int]) -> bool:
    left, top = (index % p.blocks_x) * BLOCK, (index // p.blocks_x) * BLOCK
    return left < box[2] and left + BLOCK > box[0] and top < box[3] and top + BLOCK > box[1]


def _plane(raw: bytes, offset: int, row_bytes: int, y: int, x: int, width: int) -> bytes:
    """``width`` values of one plane row from column ``x``; 1-bit planes become 0/255."""
    start = offset + y * row_bytes
    if row_bytes != BLOCK // 8:
        return raw[start + x:start + x + width]
    row = raw[start:start + row_bytes]
    return bytes(255 if row[(x + i) >> 3] & (0x80 >> ((x + i) & 7)) else 0 for i in range(width))
