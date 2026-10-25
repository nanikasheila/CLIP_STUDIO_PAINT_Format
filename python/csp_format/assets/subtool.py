"""Sub tool databases (.sut, EditImageTool.todb, .bfps): SQLite with
``Manager`` / ``Node`` / ``Variant`` [/ ``MaterialFile``].

See ``crates/csp-assets/src/subtool`` and ``docs/assets/requirements.md``
(A3, A4). The database is read from bytes (``sqlite3`` deserialize when
available, else a temporary copy), so a file CSP has open is never opened
in place. Binary columns decode to the same JSON as ``cspf asset settings``.
"""
from __future__ import annotations

import os
import sqlite3
import struct
import tempfile
from dataclasses import dataclass
from pathlib import Path

from ..errors import CspFormatError
from . import tar

SQLITE = b"SQLite format 3\0"


def _word(data: bytes, at: int) -> int:
    if at + 4 > len(data):
        raise CspFormatError(f"sub tool blob: 4 bytes needed at {at:#x} of {len(data)}")
    return struct.unpack_from(">I", data, at)[0]


def _signed(word: int) -> int:
    return word - (1 << 32) if word >= 1 << 31 else word


def records(data: bytes) -> tuple[int, list[bytes]]:
    """``u32 12, u32 count, u32 size``, records → (size, records)."""
    header, count, size = _word(data, 0), _word(data, 4), _word(data, 8)
    if header != 12 or size == 0:
        raise CspFormatError(f"records: header {header}, record size {size}")
    if len(data) != 12 + count * size:
        raise CspFormatError(f"records: {count} x {size} bytes, blob has {len(data)}")
    return size, [data[12 + i * size:12 + (i + 1) * size] for i in range(count)]


def records_json(size: int, rows: list[bytes], as_points: bool = True):
    if size == 16 and as_points:
        return [list(struct.unpack_from(">dd", r)) for r in rows]
    if size in (4, 8, 12):
        return [list(struct.unpack(f">{size // 4}I", r)) for r in rows]
    return {"recordSize": size, "hex": [r.hex() for r in rows]}


def effector(data: bytes) -> dict:
    """影響元設定: header (40/44/48 bytes) and up to three graphs."""
    header = _word(data, 0)
    if header not in (40, 44, 48):
        raise CspFormatError(f"effector: header of {header} bytes")
    w = [_word(data, 4 * i) for i in range(header // 4)]
    at = header
    graphs = []
    for size in (w[8], w[9], w[11] if header >= 48 else 0):
        if size == 0:
            graphs.append(None)
            continue
        if at + size > len(data):
            raise CspFormatError("effector: graph runs past the end")
        graphs.append(records_json(*records(data[at:at + size])))
        at += size
    if at != len(data):
        raise CspFormatError(f"effector: {len(data) - at} bytes after the graphs")
    return {
        "header_size": header,
        "offered": w[1],
        "inputs": w[2],
        "minimum": [_signed(x) for x in w[3:7]],
        "word7": w[7],
        "word10": w[10] if header >= 44 else None,
        "pressure_graph": graphs[0],
        "tilt_graph": graphs[1],
        "velocity_graph": graphs[2],
    }


def material_list(data: bytes) -> dict:
    """``*PatternImageArray`` / ``*TextureImage``: material references."""
    header, count = _word(data, 0), _word(data, 4)
    if header != 8:
        raise CspFormatError(f"material list: header {header}")
    at, materials = 8, []
    for _ in range(count):
        size = _word(data, at)
        item = data[at + 4:at + size]
        if at + size > len(data) or size < 4:
            raise CspFormatError("material list: item runs past the end")
        materials.append(_material_ref(item))
        at += size
    if at != len(data):
        raise CspFormatError("material list: bytes after the last item")
    return {"materials": materials}


def _material_ref(item: bytes) -> dict:
    at = 0

    def string():
        nonlocal at
        if at >= len(item):
            return None
        n = _word(item, at)
        if at + 4 + n > len(item):
            raise CspFormatError("material reference: text runs past the end")
        text = item[at + 4:at + 4 + n].decode("utf-16-le", "replace")
        at += 4 + n
        return text

    def number():
        nonlocal at
        if at >= len(item):
            return None
        value = _word(item, at)
        at += 4
        return value

    path = string() or ""
    kind = number()
    name = string()
    flag = number()
    install = string()
    return {"path": path, "kind": kind, "name": name, "flag": flag, "install_path": install}


def gradation(data: bytes) -> dict:
    """``GradationData``; see ``crates/csp-assets/src/gradient.rs``."""
    header, stop_size, count, word3 = (_word(data, 4 * i) for i in range(4))
    if not 16 <= header <= 64 or stop_size < 28 or count > 4096:
        raise CspFormatError(f"GradationData: header {header}, {count} stops of {stop_size}")
    mixing = [_signed(_word(data, 16)), _signed(_word(data, 20))] if header >= 24 else None
    curve_at = header + count * stop_size
    stops = []
    for i in range(count):
        at = header + i * stop_size
        rgba = [_word(data, at + 4 * k) for k in range(4)]
        flags, position, points = _word(data, at + 16), _word(data, at + 20), _word(data, at + 24)
        curve = []
        if flags & 0x10:
            for p in range(points):
                if curve_at + 16 * p + 16 > len(data):
                    raise CspFormatError("GradationData: curves run past the end")
                curve.append(list(struct.unpack_from(">dd", data, curve_at + 16 * p)))
            curve_at += 16 * points
        elif points:
            raise CspFormatError("GradationData: curve points without flag 0x10")
        stops.append({"rgba": rgba, "flags": flags, "position": position, "curve": curve})
    if curve_at > len(data):
        raise CspFormatError("GradationData: curves run past the end")
    return {"header_size": header, "stop_size": stop_size, "word3": word3, "mixing": mixing, "stops": stops}


def kind_of_column(column: str) -> str:
    if column.endswith("Effector"):
        return "effector"
    if column.endswith("ImageArray") or column.endswith("TextureImage"):
        return "materials"
    if column == "GradationData":
        return "gradation"
    if column in ("VariantShowParam", "BrushInOutTarget", "IncludeFillColor", "ContourFillBlendGraph", "PressureGraph") \
            or column.endswith("FixedPointArray"):
        return "records"
    return "unknown"


def blob_json(column: str, data: bytes):
    """A binary column as the JSON of ``cspf asset settings``."""
    kind = kind_of_column(column)
    try:
        if kind == "records":
            return records_json(*records(data), as_points=column != "IncludeFillColor")
        if kind == "effector":
            return effector(data)
        if kind == "materials":
            return material_list(data)
        if kind == "gradation":
            return gradation(data)
    except CspFormatError:
        pass
    if kind == "unknown" and column.endswith("Uuid"):
        return data.hex()
    return {"hex": data.hex()}


def row_json(row: dict) -> dict:
    out = {}
    for column, value in row.items():
        if value is None:
            continue
        out[column] = blob_json(column, value) if isinstance(value, bytes) else value
    return out


# NodeInputOp / NodeOutputOp → kind, as ``ToolKind::of`` in the Rust crate.
_OUTPUT_KINDS = {
    10: "brush", 11: "brush", 14: "brush", 12: "eraser", 13: "blend", 20: "selection",
    31: "fill", 50: "fill", 51: "fill", 52: "gradient", 54: "gradient", 30: "shape", 32: "shape", 33: "shape",
    70: "text", 61: "balloon", 62: "balloon", 60: "frame", 63: "frame", 64: "effectLines", 65: "effectLines",
    53: "vectorEdit", 80: "vectorEdit", 81: "vectorEdit", 82: "vectorEdit", 83: "vectorEdit", 84: "vectorEdit",
    85: "vectorEdit", 90: "ruler", 91: "ruler", 92: "ruler", 93: "ruler", 1: "view", 2: "view", 3: "view",
    40: "operation", 42: "operation", 45: "operation", 100: "operation", 110: "operation", 41: "eyedropper",
    120: "liquify",
}


@dataclass
class Node:
    row: dict
    uuid: bytes
    name: str
    input_op: int
    output_op: int
    variant_id: int
    init_variant_id: int
    first_child: bytes | None
    next: bytes | None

    @property
    def kind(self) -> str:
        if self.variant_id == 0 or (self.input_op, self.output_op) == (0, 0):
            return "folder"
        return _OUTPUT_KINDS.get(self.output_op, "other")


def _none(uuid) -> bool:
    return uuid is None or all(b == 0 for b in uuid)


def _open(data: bytes) -> sqlite3.Connection:
    if not data.startswith(SQLITE):
        raise CspFormatError("not a sub tool file (not SQLite)")
    connection = sqlite3.connect(":memory:")
    if hasattr(connection, "deserialize"):
        connection.deserialize(data)
        return connection
    connection.close()
    handle, name = tempfile.mkstemp(suffix=".sqlite")
    with os.fdopen(handle, "wb") as out:
        out.write(data)
    disk = sqlite3.connect(name)
    memory = sqlite3.connect(":memory:")
    disk.backup(memory)
    disk.close()
    os.unlink(name)
    return memory


def _rows(connection: sqlite3.Connection, table: str) -> list[dict]:
    cursor = connection.execute(f'SELECT * FROM "{table}" ORDER BY rowid')
    names = [d[0] for d in cursor.description]
    return [dict(zip(names, row)) for row in cursor.fetchall()]


class SubToolDb:
    """A sub tool file read into memory."""

    def __init__(self, data: bytes):
        self.connection = _open(data)
        self.connection.text_factory = lambda b: b.decode("utf-8", "replace")
        tables = {r[0] for r in self.connection.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
        for table in ("Manager", "Node", "Variant"):
            if table not in tables:
                raise CspFormatError(f"not a sub tool file: no {table} table")
        self.kind = "subTool" if "MaterialFile" in tables else "toolDb"
        managers = _rows(self.connection, "Manager")
        if not managers:
            raise CspFormatError("Manager has no row")
        self.manager = managers[0]
        self.nodes = [
            Node(
                row=r, uuid=r.get("NodeUuid") or b"", name=r.get("NodeName") or "",
                input_op=r.get("NodeInputOp") or 0, output_op=r.get("NodeOutputOp") or 0,
                variant_id=r.get("NodeVariantID") or 0, init_variant_id=r.get("NodeInitVariantID") or 0,
                first_child=None if _none(r.get("NodeFirstChildUuid")) else r["NodeFirstChildUuid"],
                next=None if _none(r.get("NodeNextUuid")) else r["NodeNextUuid"],
            )
            for r in _rows(self.connection, "Node")
        ]
        self.variants = {r.get("VariantID") or 0: r for r in _rows(self.connection, "Variant")}
        self.materials = _rows(self.connection, "MaterialFile") if self.kind == "subTool" else []

    @classmethod
    def open(cls, path) -> "SubToolDb":
        return cls(Path(path).read_bytes())

    def tree(self) -> list[tuple[int, int, list[str]]]:
        """(node index, depth, names above) in display order; same walk as the Rust crate."""
        index = {n.uuid: i for i, n in enumerate(self.nodes)}
        root = self.manager.get("RootUuid")
        if _none(root) or root not in index:
            return [(i, 0, []) for i in range(len(self.nodes))]
        out, seen, stack = [], set(), [(root, 0, [])]
        while stack:
            uuid, depth, path = stack.pop()
            i = index.get(uuid)
            if i is None or i in seen:
                continue
            seen.add(i)
            node = self.nodes[i]
            if node.next is not None:
                stack.append((node.next, depth, path))
            if node.first_child is not None:
                child_path = path if depth == 0 and not node.name else path + [node.name]
                stack.append((node.first_child, depth + 1, child_path))
            out.append((i, depth, path))
        return out

    def sub_tools(self) -> list[tuple[int, int, list[str]]]:
        return [e for e in self.tree() if self.nodes[e[0]].variant_id != 0]

    def settings(self, variant_id: int) -> dict | None:
        row = self.variants.get(variant_id)
        return None if row is None else row_json(row)

    def material_files(self, index: int) -> list[tar.Entry]:
        return tar.read(self.materials[index].get("FileData") or b"")
