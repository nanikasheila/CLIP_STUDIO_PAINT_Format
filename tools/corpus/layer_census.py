#!/usr/bin/env python3
"""Count which layer features occur together, over many .clip files (read only).

Why: Layer kinds are not stored as one code. `LayerType`, `LayerFolder` and
     which optional columns are filled (text, vector, gradient, filter,
     3D, …) together tell what a layer is. Counting the combinations over
     the corpus shows every kind CSP writes, so the classifier can be
     checked against all of them.
How: For every Layer row a feature tuple is built (codes and booleans only;
     no names, text or pixels) and counted. The pixel layout of the render
     and mask offscreens is read from `Offscreen.Attribute`.

Usage::

    python3 layer_census.py ROOT OUT.json [--seconds N]
"""
from __future__ import annotations

import argparse
import json
import os
import sqlite3
import struct
import sys
import tempfile
import time

from schema_catalog import sqlite_bytes


def pixel_layout(attribute: bytes):
    if not attribute or len(attribute) < 40:
        return None
    _, length = struct.unpack(">II", attribute[:8])
    name_length = struct.unpack(">I", attribute[16:20])[0]
    start = 20 + 2 * name_length
    values = struct.unpack(">%dI" % ((16 + length - start) // 4), attribute[start:16 + length])
    # channel order, image channels, alpha row bytes
    return f"{values[4]:#x}/{values[6]}/{values[12]}"


def offscreen_of_mipmap(connection, mipmap_id):
    if not mipmap_id:
        return None
    row = connection.execute(
        "SELECT o.Attribute FROM Mipmap m JOIN MipmapInfo i ON i.MainId = m.BaseMipmapInfo "
        "JOIN Offscreen o ON o.MainId = i.Offscreen WHERE m.MainId = ?", (mipmap_id,)).fetchone()
    return pixel_layout(row[0]) if row else "missing"


FEATURES = ["LayerType", "LayerFolder", "LayerComposite", "LayerClip", "LayerVisibility", "LayerMasking",
            "LayerLock", "TextLayerType", "VectorNormalType", "DrawToRenderOffscreenType", "SpecialRenderType",
            "MaterialContentType", "OutputAttribute", "ComicStoryInfoType", "LayerColorTypeIndex",
            "DrawColorEnable", "EffectRenderType"]
PRESENCE = ["TextLayerString", "GradationFillInfo", "FilterLayerInfo", "LayerEffectInfo", "MonochromeFillInfo",
            "ResizableImageInfo", "ComicFrameLineMipmap", "FirstLayerObject", "StreamLineIndex",
            "SpecialRulerManager", "LightTableInfo", "VectorNormalStrokeIndex", "VectorNormalBalloonIndex",
            "VectorNormalFillIndex"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root")
    parser.add_argument("out")
    args = parser.parse_args()
    combos: dict = {}
    files = 0
    for directory, _dirs, names in os.walk(args.root):
        for name in names:
            if not name.lower().endswith(".clip"):
                continue
            path = os.path.join(directory, name)
            try:
                data = sqlite_bytes(path)
            except Exception as error:
                print(f"{name}: {error}", file=sys.stderr)
                continue
            with tempfile.NamedTemporaryFile(suffix=".sqlite", delete=False) as temporary:
                temporary.write(data)
            try:
                connection = sqlite3.connect(temporary.name)
                connection.row_factory = sqlite3.Row
                columns = {c[1] for c in connection.execute("PRAGMA table_info(Layer)")}
                vector_layers = set()
                if connection.execute("SELECT 1 FROM sqlite_master WHERE name='VectorObjectList'").fetchone():
                    vector_layers = {r[0] for r in connection.execute("SELECT LayerId FROM VectorObjectList")}
                for row in connection.execute("SELECT * FROM Layer"):
                    key = {}
                    for column in FEATURES:
                        key[column] = row[column] if column in columns else "-"
                    for column in PRESENCE:
                        value = row[column] if column in columns else None
                        key["has_" + column] = bool(value)
                    key["in_VectorObjectList"] = row["MainId"] in vector_layers
                    key["render"] = offscreen_of_mipmap(connection, row["LayerRenderMipmap"])
                    key["mask"] = offscreen_of_mipmap(connection, row["LayerLayerMaskMipmap"])
                    text = json.dumps(key, sort_keys=False)
                    combos[text] = combos.get(text, 0) + 1
                connection.close()
                files += 1
            finally:
                os.unlink(temporary.name)
    rows = sorted(({"count": n, **json.loads(k)} for k, n in combos.items()), key=lambda r: -r["count"])
    with open(args.out, "w", encoding="utf-8") as out:
        json.dump({"files": files, "combinations": rows}, out, ensure_ascii=False, indent=0)
    print(f"{files} files, {len(rows)} combinations", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
