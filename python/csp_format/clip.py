"""A .clip file: canvas, layer tree, pixels, vector and text data (read only).

The layer tree is stored as first-child / next-sibling links:
``Canvas.CanvasRootFolder`` is the root folder's ``Layer.MainId``, and
``LayerFirstChildIndex`` / ``LayerNextIndex`` hold ``MainId``s (0 = none).
Siblings are listed bottom to top (the paper first). See
``docs/spec/03-canvas-and-layers.md``. The rules here follow the Rust crate
(``crates/csp-format/src/model.rs`` and ``clip.rs``).
"""
from __future__ import annotations

import os
import sqlite3
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterator

from . import offscreen
from .container import Container
from .errors import CspFormatError
from .image import Image, decode_png
from .text import TextBlock, read_blocks
from .vector import VectorObject, parse_vector_data

OPACITY_FULL = 256
MAX_DEPTH = 256

LAYER_TYPE_RASTER = 1
LAYER_TYPE_ROOT = 256
LAYER_TYPE_STORY_INFO = 800
LAYER_TYPE_PAPER = 1584
LAYER_TYPE_CORRECTION = 4096

UNITS = {0: "px", 1: "cm", 2: "mm", 3: "inch", 5: "pt"}
_PER_INCH = {"cm": 2.54, "mm": 25.4, "inch": 1.0, "pt": 72.0}

FOLDER_KINDS = ("root", "folder", "frame_folder")


def to_pixels(value: float, unit: str, resolution: float) -> int:
    """Pixels of ``value`` in ``unit`` at ``resolution`` dpi, rounded half to even as CSP does."""
    per_inch = _PER_INCH.get(unit)
    return round(value if per_inch is None else value * resolution / per_inch)


def open_database(image: bytes) -> sqlite3.Connection:
    """Open an SQLite database image in memory (a temporary file before Python 3.11)."""
    if hasattr(sqlite3.Connection, "deserialize"):
        connection = sqlite3.connect(":memory:")
        connection.deserialize(image)
    else:  # pragma: no cover - Python < 3.11
        handle, name = tempfile.mkstemp(suffix=".sqlite")
        with os.fdopen(handle, "wb") as file:
            file.write(image)
        source = sqlite3.connect(name)
        connection = sqlite3.connect(":memory:")
        source.backup(connection)
        source.close()
        os.unlink(name)
    connection.row_factory = sqlite3.Row
    return connection


def _value(row: sqlite3.Row, column: str):
    return row[column] if column in row.keys() else None


def _int(row: sqlite3.Row, column: str) -> int:
    value = _value(row, column)
    try:
        return int(value) if value is not None else 0
    except (TypeError, ValueError):
        return 0


def _real(row: sqlite3.Row, column: str) -> float:
    value = _value(row, column)
    try:
        return float(value) if value is not None else 0.0
    except (TypeError, ValueError):
        return 0.0


def _truthy(row: sqlite3.Row, column: str) -> bool:
    value = _value(row, column)
    return bool(value) if value is not None else False


def _text(value) -> str | None:
    if isinstance(value, bytes):
        return value.decode("utf-8", "replace")
    return value if isinstance(value, str) else None


@dataclass(frozen=True)
class Canvas:
    """Settings of the canvas (the first ``Canvas`` row)."""

    main_id: int
    unit: str
    width: float
    height: float
    resolution: float
    width_px: int
    height_px: int
    root_folder: int
    channel_bytes: int
    double_page: bool
    #: Trim frame (仕上がり) of one page in pixels, when the canvas has one.
    crop_size_px: tuple[int, int] | None = None
    #: Basic frame (基本枠) in pixels and its offset from the trim frame's centre.
    basic_size_px: tuple[int, int] | None = None
    basic_offset_px: tuple[int, int] = (0, 0)

    @property
    def single_page_width_px(self) -> int:
        """Width of one page in pixels (half the canvas for a double page)."""
        return to_pixels(self.width / 2, self.unit, self.resolution) if self.double_page else self.width_px

    @classmethod
    def from_row(cls, row: sqlite3.Row) -> "Canvas":
        unit = UNITS.get(_int(row, "CanvasUnit"), "px")
        width, height, resolution = _real(row, "CanvasWidth"), _real(row, "CanvasHeight"), _real(row, "CanvasResolution")
        crop_unit = UNITS.get(_int(row, "CropFrameUnitKind"), "px")

        def size(w: str, h: str) -> tuple[int, int] | None:
            sw, sh = _real(row, w), _real(row, h)
            if sw <= 0 or sh <= 0:
                return None
            return to_pixels(sw, crop_unit, resolution), to_pixels(sh, crop_unit, resolution)

        return cls(
            main_id=_int(row, "MainId"),
            unit=unit,
            width=width,
            height=height,
            resolution=resolution,
            width_px=max(to_pixels(width, unit, resolution), 0),
            height_px=max(to_pixels(height, unit, resolution), 0),
            root_folder=_int(row, "CanvasRootFolder"),
            channel_bytes=_int(row, "CanvasChannelBytes"),
            double_page=_truthy(row, "CanvasDoublePage"),
            crop_size_px=size("CropFrameWidth", "CropFrameHeight"),
            basic_size_px=size("CropFrameInnerWidth", "CropFrameInnerHeight"),
            basic_offset_px=(to_pixels(_real(row, "CropFrameInnerOffsetX"), crop_unit, resolution),
                             to_pixels(_real(row, "CropFrameInnerOffsetY"), crop_unit, resolution)),
        )


def classify(row: sqlite3.Row, vector_layers: set[int]) -> str:
    """What a layer is. No single column holds it; the order of the checks matters."""
    layer_type, folder = _int(row, "LayerType"), _int(row, "LayerFolder")
    if layer_type == LAYER_TYPE_ROOT:
        return "root"
    if layer_type == LAYER_TYPE_PAPER:
        return "paper"
    if layer_type == LAYER_TYPE_STORY_INFO:
        return "story_info"
    if folder & 1:
        return "frame_folder" if _truthy(row, "ComicFrameLineMipmap") else "folder"
    if _value(row, "TextLayerString") is not None:
        return "text"
    if _truthy(row, "GradationFillInfo"):
        return "gradient"
    if _truthy(row, "FilterLayerInfo") or layer_type & LAYER_TYPE_CORRECTION:
        return "correction"
    if _truthy(row, "FirstLayerObject"):
        return "3d"
    if _truthy(row, "ResizableImageInfo"):
        return "image_material"
    if _int(row, "MainId") in vector_layers:
        return "vector"
    if layer_type & LAYER_TYPE_RASTER:
        return "raster"
    # A vector layer nothing was drawn on has no VectorObjectList row but the vector index columns.
    if layer_type == 0 and _value(row, "VectorNormalStrokeIndex") is not None:
        return "vector"
    return "other"


@dataclass(frozen=True)
class Layer:
    """One row of ``Layer`` with the commonly used columns, and its children (bottom to top)."""

    main_id: int
    name: str
    kind: str
    layer_type: int
    visible: bool
    #: ``LayerVisibility`` bit 1: the layer mask is applied.
    mask_enabled: bool
    #: 0.0–1.0 (``LayerOpacity`` / 256).
    opacity: float
    #: ``LayerComposite`` (blend mode code, see docs/spec/08-rendering.md).
    blend: int
    clipping: bool
    locked: bool
    folder_flags: int
    offset: tuple[int, int]
    render_offset: tuple[int, int]
    mask_offset: tuple[int, int]
    mask_render_offset: tuple[int, int]
    render_mipmap: int
    mask_mipmap: int
    uuid: str | None
    text: str | None
    children: tuple["Layer", ...] = field(default=(), repr=False)

    @property
    def is_folder(self) -> bool:
        return self.kind in FOLDER_KINDS

    @property
    def has_mask(self) -> bool:
        return self.mask_mipmap != 0

    @property
    def is_open(self) -> bool:
        """Folder shown expanded in the layer palette (``LayerFolder`` bit 4)."""
        return bool(self.folder_flags & 16)

    @property
    def pixel_origin(self) -> tuple[int, int]:
        """Canvas position of the stored pixels' top-left corner."""
        return self.offset[0] + self.render_offset[0], self.offset[1] + self.render_offset[1]

    @property
    def mask_origin(self) -> tuple[int, int]:
        """Canvas position of the mask's top-left corner (mask offsets are relative to the layer offset)."""
        return (self.offset[0] + self.mask_offset[0] + self.mask_render_offset[0],
                self.offset[1] + self.mask_offset[1] + self.mask_render_offset[1])

    def walk(self) -> Iterator[tuple["Layer", int]]:
        """``(layer, depth)`` depth first, parents before children, siblings bottom to top."""
        stack = [(self, 0)]
        while stack:
            layer, depth = stack.pop()
            yield layer, depth
            stack.extend((child, depth + 1) for child in reversed(layer.children))


def _layer(row: sqlite3.Row, kind: str, children: tuple[Layer, ...]) -> Layer:
    visibility = _int(row, "LayerVisibility")
    opacity = _value(row, "LayerOpacity")
    pair = lambda x, y: (_int(row, x), _int(row, y))  # noqa: E731
    return Layer(
        main_id=_int(row, "MainId"),
        name=_text(_value(row, "LayerName")) or "",
        kind=kind,
        layer_type=_int(row, "LayerType"),
        visible=bool(visibility & 1),
        mask_enabled=bool(visibility & 2),
        opacity=min(max(int(opacity) if opacity is not None else OPACITY_FULL, 0), OPACITY_FULL) / OPACITY_FULL,
        blend=_int(row, "LayerComposite"),
        clipping=_int(row, "LayerClip") != 0,
        locked=_int(row, "LayerLock") != 0,
        folder_flags=_int(row, "LayerFolder"),
        offset=pair("LayerOffsetX", "LayerOffsetY"),
        render_offset=pair("LayerRenderOffscrOffsetX", "LayerRenderOffscrOffsetY"),
        mask_offset=pair("LayerMaskOffsetX", "LayerMaskOffsetY"),
        mask_render_offset=pair("LayerMaskOffscrOffsetX", "LayerMaskOffscrOffsetY"),
        render_mipmap=_int(row, "LayerRenderMipmap"),
        mask_mipmap=_int(row, "LayerLayerMaskMipmap"),
        uuid=_text(_value(row, "LayerUuid")),
        text=_text(_value(row, "TextLayerString")),
        children=children,
    )


class ClipFile:
    """An opened .clip file. Nothing is written back; ``database`` is an in-memory copy."""

    def __init__(self, data: bytes, path: Path | None = None):
        self.path = path
        self.container = Container(data)
        self.database = open_database(self.container.sqlite())
        canvas = self.database.execute("SELECT * FROM Canvas LIMIT 1").fetchone()
        if canvas is None:
            raise CspFormatError("no Canvas row")
        self.canvas = Canvas.from_row(canvas)
        project = self.database.execute("SELECT * FROM Project LIMIT 1").fetchone()
        if project is None:
            raise CspFormatError("no Project row")
        self.project_name = _text(_value(project, "ProjectName"))
        self.internal_version = _text(_value(project, "ProjectInternalVersion"))
        self.root = self._read_tree(self.canvas.root_folder)

    @classmethod
    def open(cls, path: str | os.PathLike) -> "ClipFile":
        path = Path(path)
        return cls(path.read_bytes(), path)

    def close(self) -> None:
        self.database.close()

    def version_info(self):
        """Which CSP generation saved the file (:mod:`csp_format.version`)."""
        from .version import read_version_info
        return read_version_info(self.database, self.container.sqlite())

    def __enter__(self) -> "ClipFile":
        return self

    def __exit__(self, *_) -> None:
        self.close()

    # -- tables -----------------------------------------------------------

    def has_table(self, table: str) -> bool:
        return self.database.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?", (table,)).fetchone() is not None

    def row(self, table: str, main_id: int) -> sqlite3.Row | None:
        if not self.has_table(table):
            return None
        return self.database.execute(f'SELECT * FROM "{table}" WHERE MainId = ?', (main_id,)).fetchone()

    def layer_row(self, layer: Layer | int) -> sqlite3.Row:
        main_id = layer if isinstance(layer, int) else layer.main_id
        row = self.row("Layer", main_id)
        if row is None:
            raise CspFormatError(f"layer {main_id} does not exist")
        return row

    # -- layer tree -------------------------------------------------------

    def _read_tree(self, root_id: int) -> Layer:
        rows = {_int(row, "MainId"): row for row in self.database.execute("SELECT * FROM Layer")}
        vector_layers: set[int] = set()
        if self.has_table("VectorObjectList"):
            vector_layers = {int(r[0]) for r in self.database.execute("SELECT LayerId FROM VectorObjectList")}
        seen: set[int] = set()

        def build(main_id: int, depth: int) -> Layer:
            if depth > MAX_DEPTH:
                raise CspFormatError(f"layer tree deeper than {MAX_DEPTH}")
            row = rows.get(main_id)
            if row is None:
                raise CspFormatError(f"layer {main_id} does not exist")
            if main_id in seen:
                raise CspFormatError(f"layer {main_id} is linked twice (the tree has a cycle)")
            seen.add(main_id)
            children = []
            child = _int(row, "LayerFirstChildIndex")
            while child:
                children.append(build(child, depth + 1))
                child = _int(rows[child], "LayerNextIndex")
            return _layer(row, classify(row, vector_layers), tuple(children))

        root = build(root_id, 0)
        self.unreachable = sorted(set(rows) - seen)
        return root

    def iter_layers(self) -> list[Layer]:
        """All layers depth first, bottom to top (root excluded)."""
        return [layer for layer, depth in self.root.walk() if depth > 0]

    def find_layer(self, main_id: int) -> Layer:
        for layer, _ in self.root.walk():
            if layer.main_id == main_id:
                return layer
        raise CspFormatError(f"layer {main_id} not found")

    # -- pixels -----------------------------------------------------------

    def offscreen(self, main_id: int) -> tuple[bytes, bytes | None] | None:
        """``(Attribute, block data payload or None)`` of an ``Offscreen`` row."""
        row = self.row("Offscreen", main_id)
        if row is None:
            return None
        attribute = _value(row, "Attribute")
        if not isinstance(attribute, bytes):
            raise CspFormatError(f"Offscreen {main_id} has no Attribute")
        block_data = _text(_value(row, "BlockData"))
        return attribute, self.container.external(block_data) if block_data else None

    def mipmap_levels(self, mipmap_id: int) -> list[tuple[float, int]]:
        """``(ThisScale in percent, Offscreen id)`` of each level of a mipmap, full size first."""
        row = self.row("Mipmap", mipmap_id)
        if row is None:
            return []
        levels, next_id = [], _int(row, "BaseMipmapInfo")
        while next_id:
            if len(levels) > 64:
                raise CspFormatError(f"Mipmap {mipmap_id} has a looping level list")
            info = self.row("MipmapInfo", next_id)
            if info is None:
                raise CspFormatError(f"MipmapInfo {next_id} does not exist")
            levels.append((_real(info, "ThisScale"), _int(info, "Offscreen")))
            next_id = _int(info, "NextIndex")
        return levels

    def _base(self, mipmap_id: int) -> tuple[bytes, bytes | None] | None:
        levels = self.mipmap_levels(mipmap_id) if mipmap_id else []
        return self.offscreen(levels[0][1]) if levels else None

    def layer_pixels(self, layer: Layer) -> tuple[Image, tuple[int, int]] | None:
        """The stored pixels of a layer and the canvas position of their top-left.

        None when the layer stores none: CSP draws vector, text and similar
        layers itself when it opens the file (their stored render may be absent).
        """
        base = self._base(layer.render_mipmap)
        image = offscreen.decode(*base) if base else None
        return (image, layer.pixel_origin) if image is not None else None

    def layer_pixels_in_box(self, layer: Layer, box: tuple[int, int, int, int]) -> Image:
        """The layer's pixels inside a canvas box ``(x0, y0, x1, y1)``, transparent where none are stored."""
        x0, y0, x1, y1 = box
        base = self._base(layer.render_mipmap)
        ox, oy = layer.pixel_origin
        image = offscreen.decode(*base, box=(x0 - ox, y0 - oy, x1 - ox, y1 - oy)) if base else None
        return image if image is not None else Image.blank(max(x1 - x0, 0), max(y1 - y0, 0))

    def stored_pixel_boxes(self, layer: Layer) -> list[tuple[int, int, int, int]]:
        """Canvas boxes of the 256-pixel blocks that store data (cheap: nothing is decompressed)."""
        base = self._base(layer.render_mipmap)
        if not base:
            return []
        ox, oy = layer.pixel_origin
        return [(x0 + ox, y0 + oy, x1 + ox, y1 + oy) for x0, y0, x1, y1 in offscreen.stored_blocks(*base)]

    def layer_mask(self, layer: Layer) -> tuple[Image, tuple[int, int]] | None:
        """The layer mask as opaque gray (0 = hidden, 255 = shown) and its canvas position."""
        base = self._base(layer.mask_mipmap)
        image = offscreen.decode(*base, mask=True) if base else None
        return (image, layer.mask_origin) if image is not None else None

    def layer_thumbnail(self, layer: Layer) -> Image | None:
        """The layer palette thumbnail (``LayerRenderThumbnail`` → ``LayerThumbnail.ThumbnailOffscreen``)."""
        thumbnail_id = _int(self.layer_row(layer), "LayerRenderThumbnail")
        row = self.row("LayerThumbnail", thumbnail_id) if thumbnail_id else None
        if row is None:
            return None
        found = self.offscreen(_int(row, "ThumbnailOffscreen"))
        return offscreen.decode(*found) if found else None

    def preview_png(self) -> bytes | None:
        """The preview PNG CSP saved with the file (``CanvasPreview.ImageData``)."""
        if not self.has_table("CanvasPreview"):
            return None
        row = self.database.execute("SELECT ImageData FROM CanvasPreview LIMIT 1").fetchone()
        return bytes(row[0]) if row is not None and row[0] else None

    def preview(self) -> Image | None:
        data = self.preview_png()
        return decode_png(data) if data else None

    # -- vector and text --------------------------------------------------

    def vector_payloads(self, layer: Layer) -> list[bytes]:
        """The raw ``VectorData`` payloads of a layer (vector strokes, or a text layer's balloons)."""
        if not self.has_table("VectorObjectList"):
            return []
        payloads = []
        for row in self.database.execute(
                "SELECT VectorData FROM VectorObjectList WHERE LayerId = ? ORDER BY MainId", (layer.main_id,)):
            external_id = _text(row[0])
            payload = self.container.external(external_id) if external_id else None
            if payload:
                payloads.append(payload)
        return payloads

    def vector_objects(self, layer: Layer) -> list[VectorObject]:
        """The strokes and shapes of a layer (layer pixels: add ``layer.offset`` for the canvas)."""
        return [item for payload in self.vector_payloads(layer) for item in parse_vector_data(payload)]

    def text_blocks(self, layer: Layer) -> list[TextBlock]:
        """The text blocks of a text layer (several for balloons), with their attributes."""
        if layer.kind not in ("text", "story_info"):
            return []
        return read_blocks(self.layer_row(layer))


def read_clip(path: str | os.PathLike) -> ClipFile:
    """Open a .clip file (read only)."""
    return ClipFile.open(path)
