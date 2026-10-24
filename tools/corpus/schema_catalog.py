#!/usr/bin/env python3
"""Collect CSP's own schema registry (ParamScheme / ElemScheme) from many files.

Why: Every .clip / .cmc carries `ParamScheme`, CSP's list of every table and
     column it knows (with a data type code, flags and the table an id
     column links to), even for tables the file does not use. The union over
     a corpus is the full list of fields the specification has to cover.
How: The SQLite chunk of each file is read (pixels are skipped), and each
     distinct ParamScheme / ElemScheme / ExternalTableAndColumnName row is
     counted, with the first and last modification date it was seen in.
     Only schema rows are read; no artwork leaves the machine.

Usage::

    python3 schema_catalog.py ROOT OUT.json
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

PARAM_COLUMNS = ("TableName", "LabelName", "DataType", "Flag", "OwnerType", "LockType", "LockSpecified",
                 "AlternativeLockType", "AlternativeLockSpecified", "LinkTable")


def sqlite_bytes(path: str) -> bytes:
    with open(path, "rb") as handle:
        magic = handle.read(16)
        if magic.startswith(b"SQLite format 3"):
            handle.seek(0)
            return handle.read()
        if not magic.startswith(b"CSFCHUNK"):
            raise ValueError("unknown file")
        size = os.path.getsize(path)
        position = 24
        while position + 16 <= size:
            handle.seek(position)
            raw = handle.read(16)
            (length,) = struct.unpack(">Q", raw[8:16])
            if raw[:8] == b"CHNKSQLi":
                return handle.read(length)
            position += 16 + length
    raise ValueError("no CHNKSQLi")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root")
    parser.add_argument("out")
    args = parser.parse_args()
    params: dict = {}
    elems: dict = {}
    externals: dict = {}
    files = 0
    for directory, _dirs, names in os.walk(args.root):
        for name in names:
            ext = os.path.splitext(name)[1].lower()
            if ext not in (".clip", ".cmc"):
                continue
            path = os.path.join(directory, name)
            date = time.strftime("%Y-%m-%d", time.localtime(os.path.getmtime(path)))
            try:
                data = sqlite_bytes(path)
            except Exception as error:  # keep going
                print(f"{name}: {error}", file=sys.stderr)
                continue
            with tempfile.NamedTemporaryFile(suffix=".sqlite", delete=False) as temporary:
                temporary.write(data)
            try:
                connection = sqlite3.connect(temporary.name)
                connection.text_factory = lambda raw: raw.decode("utf-8", "replace")
                tables = {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
                present = set()
                for table in tables:
                    for column in connection.execute(f'PRAGMA table_info("{table}")'):
                        present.add((table, column[1]))
                columns = [c[1] for c in connection.execute("PRAGMA table_info(ParamScheme)")]
                for row in connection.execute("SELECT * FROM ParamScheme"):
                    values = dict(zip(columns, row))
                    key = json.dumps([ext] + [values.get(c) for c in PARAM_COLUMNS], ensure_ascii=False)
                    entry = params.setdefault(key, {"files": 0, "first": date, "last": date, "in_table": 0})
                    entry["files"] += 1
                    entry["first"] = min(entry["first"], date)
                    entry["last"] = max(entry["last"], date)
                    if (values.get("TableName"), values.get("LabelName")) in present:
                        entry["in_table"] += 1
                for table_name, elem_type, max_index in connection.execute(
                        "SELECT TableName, ElemType, MaxIndex FROM ElemScheme"):
                    key = json.dumps([ext, table_name, elem_type], ensure_ascii=False)
                    entry = elems.setdefault(key, {"files": 0, "max_index": 0, "first": date, "last": date})
                    entry["files"] += 1
                    entry["max_index"] = max(entry["max_index"], max_index or 0)
                    entry["first"] = min(entry["first"], date)
                    entry["last"] = max(entry["last"], date)
                if "ExternalTableAndColumnName" in tables:
                    for table_name, column_name in connection.execute(
                            "SELECT TableName, ColumnName FROM ExternalTableAndColumnName"):
                        key = json.dumps([ext, table_name, column_name], ensure_ascii=False)
                        externals[key] = externals.get(key, 0) + 1
                connection.close()
                files += 1
            finally:
                os.unlink(temporary.name)
    with open(args.out, "w", encoding="utf-8") as out:
        json.dump({"files": files, "params": params, "elems": elems, "externals": externals}, out,
                  ensure_ascii=False, indent=0)
    print(f"{files} files, {len(params)} param rows, {len(elems)} elem rows", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
