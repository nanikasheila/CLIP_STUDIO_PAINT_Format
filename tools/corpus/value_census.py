#!/usr/bin/env python3
"""Count which database columns real files actually use, and with which values.

Why: `ParamScheme` lists every column CSP knows, but a column that no file
     sets cannot be specified from the corpus. This census tells the used
     columns from the unused ones (coverage gaps), so the missing features
     can be made on purpose in CSP and analysed.
How: For every table and column of each file's SQLite chunk: the rows, the
     rows that are not NULL, the rows that are not NULL / 0 / '' / empty
     blob, and for INTEGER / REAL values the distinct values (up to a limit,
     with counts). TEXT and BLOB values are never recorded, only their
     lengths, so no names or artwork leave the machine. The minimum and
     maximum of the numbers are kept beyond the distinct-value limit.

Usage::

    python3 value_census.py ROOT OUT.json [--list PATHS.txt] [--limit N]
"""
from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from schema_catalog import sqlite_bytes  # noqa: E402

DISTINCT_LIMIT = 40
SKIP_TABLES = {"ParamScheme", "ElemScheme", "sqlite_sequence", "ExternalTableAndColumnName"}


def census_file(path: str, columns: dict) -> None:
    data = sqlite_bytes(path)
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as handle:
        handle.write(data)
        name = handle.name
    try:
        db = sqlite3.connect(f"file:{name}?mode=ro", uri=True)
        ext = os.path.splitext(path)[1].lower()
        tables = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")]
        for table in tables:
            if table in SKIP_TABLES:
                continue
            names = [row[1] for row in db.execute(f'PRAGMA table_info("{table}")')]
            rows = db.execute(f'SELECT * FROM "{table}"').fetchall()
            for index, column in enumerate(names):
                entry = columns.setdefault(f"{ext}|{table}|{column}", {
                    "files": 0, "files_set": 0, "rows": 0, "not_null": 0, "set": 0,
                    "values": {}, "more_values": False, "max_length": 0,
                })
                entry["files"] += 1
                any_set = False
                for row in rows:
                    value = row[index]
                    entry["rows"] += 1
                    if value is None:
                        continue
                    entry["not_null"] += 1
                    if isinstance(value, (bytes, str)):
                        entry["max_length"] = max(entry["max_length"], len(value))
                        is_set = len(value) > 0
                    else:
                        is_set = value != 0
                        entry["min"] = value if entry.get("min") is None else min(entry["min"], value)
                        entry["max"] = value if entry.get("max") is None else max(entry["max"], value)
                        key = repr(value)
                        if key in entry["values"]:
                            entry["values"][key] += 1
                        elif len(entry["values"]) < DISTINCT_LIMIT:
                            entry["values"][key] = 1
                        else:
                            entry["more_values"] = True
                    if is_set:
                        entry["set"] += 1
                        any_set = True
                entry["files_set"] += any_set
        db.close()
    finally:
        os.unlink(name)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root")
    parser.add_argument("out")
    parser.add_argument("--list", help="paths relative to ROOT, one per line")
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()
    if args.list:
        with open(args.list, encoding="utf-8") as handle:
            paths = [os.path.join(args.root, line.strip()) for line in handle if line.strip()]
    else:
        paths = []
        for directory, _, names in os.walk(args.root):
            paths += [os.path.join(directory, n) for n in names if n.lower().endswith((".clip", ".cmc"))]
    paths.sort()
    if args.limit:
        paths = paths[: args.limit]
    columns: dict = {}
    errors = 0
    for number, path in enumerate(paths, 1):
        try:
            census_file(path, columns)
        except Exception as error:  # a broken file is counted, not fatal
            errors += 1
            print(f"{path}: {error}", file=sys.stderr)
        if number % 50 == 0:
            print(f"{number}/{len(paths)}", file=sys.stderr)
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump({"files": len(paths), "errors": errors, "columns": columns}, handle, ensure_ascii=False, indent=0)
    return 0


if __name__ == "__main__":
    sys.exit(main())
