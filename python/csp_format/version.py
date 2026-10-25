"""Which CSP version saved a file (the Python side of ``crates/csp-format/src/version.rs``).

CSP stores no version number of its own. Every release registers the columns
it knows in ``ParamScheme`` and labels are never removed, so the registered
labels tell the newest CSP generation that saved the file. CSP up to 2.x
appended new labels after the old rows; CSP 3.0 and newer rewrite a .clip's
registry but keep retired labels. See ``docs/spec/09-versions.md``.
"""
from __future__ import annotations

import re
import sqlite3
from dataclasses import dataclass

# (estimated CSP versions, first month in the corpus, marker labels), newest first.
GENERATIONS: tuple[tuple[str, str, tuple[tuple[str, str], ...]], ...] = (
    ("5.1", "2026-10", (("Layer", "Has1_0_19"), ("BrushStyle", "WaterEdgeStyle"))),
    ("5.0", "2026-05", (("Layer", "Has1_0_18"), ("BrushStyle", "BlurKind"))),
    ("4.2", "2025-12", (("Layer", "Has1_0_15"), ("Canvas", "TimeLapseCodec"))),
    ("3.0-4.1", "2024-12", (("Layer", "Has1_0_12"), ("Canvas", "CanvasWorkTime"))),
    ("2.1-2.x", "2024-01", (("Layer", "Has1_0_8"), ("BrushStyle", "BrushColorMixingMode"))),
    ("2.0", "2023-04", (("Layer", "WebCooperationLayerEditable"),)),
    ("1.12-1.13", "2022-10", (("Layer", "Has1_0_7"), ("CameraNodeInfo", "CameraNodeUUID"))),
    ("1.11-1.12", "2022-02", (("Layer", "HasPrimitive"), ("PrimitiveInfo", "PrimitiveUUID"))),
    ("1.10.10-1.11", "2021-06", (("BrushStyle", "DualStyleIndex"),)),
    ("1.10.5-1.10.9", "2020-12", (("Canvas", "TimeLapseManagerIndex"),)),
    ("1.8-1.10.3", "2020-10", (("Layer", "TextLayerVersion"),)),
)

# Labels newer CSP no longer registers, with the first generation that stopped.
RETIRED_LABELS: tuple[tuple[str, str, str], ...] = (("CameraInfo", "CameraZoomWithDolly", "3.0-4.1"),)

_LEVEL = re.compile(r"^(?:LayerEffect)?Has1_0_(\d+)$")


@dataclass(frozen=True)
class VersionInfo:
    saved_by: str | None          # newest generation registered (the last saver, at least)
    created_by: str | None        # generation of the first block (None when a rewrite lost it)
    resaved: bool                 # appended rows, or a label the saver had retired
    format_level: int | None      # highest N of the registered Layer.Has1_0_N
    locking_features: tuple[str, ...]  # set columns an older CSP cannot use
    needs_format_level: int | None     # highest N among set Has1_0_N columns
    sqlite_version: int | None


def generation_of(labels: set[tuple[str, str]]) -> str | None:
    for csp, _first, markers in GENERATIONS:
        if all(marker in labels for marker in markers):
            return csp
    return None


def _age(csp: str) -> int:
    return next(i for i, (name, _first, _markers) in enumerate(GENERATIONS) if name == csp)


def _retired_label_kept(labels: set[tuple[str, str]], saved_by: str | None) -> bool:
    if saved_by is None:
        return False
    return any((table, label) in labels and _age(saved_by) <= _age(retired_in)
               for table, label, retired_in in RETIRED_LABELS)


def _level(column: str) -> int | None:
    found = _LEVEL.match(column)
    return int(found.group(1)) if found else None


def read_version_info(database: sqlite3.Connection, image: bytes | None = None) -> VersionInfo:
    rows = database.execute("SELECT * FROM ParamScheme ORDER BY rowid").fetchall()
    names = [d[0] for d in database.execute("SELECT * FROM ParamScheme LIMIT 0").description]
    index = {name: i for i, name in enumerate(names)}

    def field(row, name, default=None):
        return row[index[name]] if name in index else default

    params = [(field(r, "TableName") or "", field(r, "LabelName") or "", field(r, "Flag") or 0,
               field(r, "LockType") or 0, field(r, "LinkTable") or "") for r in rows]
    labels = {(table, label) for table, label, *_ in params}

    blocks: list[set[tuple[str, str]]] = [set()]
    seen: set[str] = set()
    last = None
    for table, label, *_ in params:
        if table != last:
            if table in seen:
                blocks.append(set())
                seen = set()
            seen.add(table)
        blocks[-1].add((table, label))
        last = table
    blocks = [block for block in blocks if block]

    levels = [_level(label) for table, label, *_ in params if table == "Layer"]
    format_level = max((level for level in levels if level is not None), default=None)

    locking: list[str] = []
    needs = None
    for table, label, flag, lock_type, link in params:
        if lock_type != 4 or flag != 0 or link:
            continue
        columns = {row[1] for row in database.execute(f'PRAGMA table_info("{table}")')}
        if label not in columns:
            continue
        count = database.execute(
            f'SELECT COUNT(*) FROM "{table}" WHERE "{label}" IS NOT NULL AND "{label}" != 0').fetchone()[0]
        if count:
            locking.append(f"{table}.{label}")
            level = _level(label)
            if level is not None:
                needs = level if needs is None else max(needs, level)

    sqlite_version = None
    if image is not None and len(image) >= 100 and image.startswith(b"SQLite format 3\x00"):
        sqlite_version = int.from_bytes(image[96:100], "big")
    saved_by = generation_of(labels)
    created_by = generation_of(blocks[0]) if blocks else None
    resaved = len(blocks) > 1
    if _retired_label_kept(labels, saved_by):
        resaved = True
        if created_by == saved_by:
            created_by = None
    return VersionInfo(
        saved_by=saved_by,
        created_by=created_by,
        resaved=resaved,
        format_level=format_level,
        locking_features=tuple(locking),
        needs_format_level=needs,
        sqlite_version=sqlite_version,
    )
