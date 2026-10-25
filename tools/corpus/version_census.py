#!/usr/bin/env python3
"""Record what tells CSP versions apart, for every CSP file under a folder (read only).

Why: CSP adds tables and columns as it grows, and it registers every table
     and column it knows in the file (`ParamScheme`, `ElemScheme`), even ones
     the file does not use. So the registration is a fingerprint of the CSP
     build that saved the file. Grouping the corpus by that fingerprint and
     ordering the groups by save date shows which columns each generation
     added, which values mark the version, and what a reader must tolerate.
How: Each .clip / .cmc is opened read only (the SQLite chunk of a .clip is
     copied to a temporary file). One JSON line per file is appended to the
     output; files already in it are skipped, so a run can be resumed.

     Only structure is written: names of tables and columns, schema rows,
     SQLite header fields, the version strings CSP writes and the numeric
     values of columns named like versions. No pixels, text, layer names or
     paths (files are identified by sha256 of the relative path, 16 hex).

Usage::

    python3 version_census.py ROOT OUT.jsonl [--seconds N] [--list FILES.txt]

Only the Python standard library is used (Python 3.8+).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import struct
import sys
import tempfile
import time

# Columns whose values identify a format version (not user content).
VERSION_COLUMN = re.compile(r"(Version|V\d{3}|Revision)")
MAX_VALUES = 12


def sqlite_header(raw: bytes) -> dict:
    """Fields of the 100-byte SQLite header that depend on the writer."""
    if len(raw) < 100 or not raw.startswith(b"SQLite format 3\x00"):
        return {"magic": raw[:16].hex()}
    (page_size,) = struct.unpack(">H", raw[16:18])
    return {
        "page_size": 65536 if page_size == 1 else page_size,
        "write_version": raw[18],
        "read_version": raw[19],
        "reserved": raw[20],
        "schema_format": struct.unpack(">I", raw[44:48])[0],
        "cache_size": struct.unpack(">I", raw[48:52])[0],
        "auto_vacuum": struct.unpack(">I", raw[52:56])[0],
        "encoding": struct.unpack(">I", raw[56:60])[0],
        "user_version": struct.unpack(">I", raw[60:64])[0],
        "incremental_vacuum": struct.unpack(">I", raw[64:68])[0],
        "application_id": struct.unpack(">I", raw[68:72])[0],
        "sqlite_version": struct.unpack(">I", raw[96:100])[0],
    }


def read_clip(path: str):
    """Return (container summary, SQLite bytes) of a .clip."""
    size = os.path.getsize(path)
    with open(path, "rb") as handle:
        header = handle.read(24)
        if header[:8] != b"CSFCHUNK":
            raise ValueError("not a CSFCHUNK file")
        declared, first = struct.unpack(">QQ", header[8:24])
        position = first
        tags = []
        head = None
        exta_head = None
        database = None
        while position + 16 <= size:
            handle.seek(position)
            raw = handle.read(16)
            tag = raw[:8].decode("latin-1")
            (length,) = struct.unpack(">Q", raw[8:16])
            if not tags or tags[-1][0] != tag:
                tags.append([tag, 1])
            else:
                tags[-1][1] += 1
            if tag == "CHNKHead":
                body = handle.read(min(length, 64))
                head = {"length": length, "u64": list(struct.unpack(">QQQ", body[:24])) if len(body) >= 24 else None}
            elif tag == "CHNKExta" and exta_head is None:
                body = handle.read(min(length, 128))
                (id_len,) = struct.unpack(">Q", body[:8])
                exta_head = {"id_len": id_len, "after_id": body[8 + id_len:8 + id_len + 16].hex()}
            elif tag == "CHNKSQLi":
                database = handle.read(length)
            position += 16 + length
            if tag == "CHNKFoot":
                break
    container = {"declared_ok": declared == size, "first": first, "sequence": tags, "head": head, "exta": exta_head}
    return container, database


def rows(connection, table):
    try:
        cursor = connection.execute(f'SELECT * FROM "{table}"')
    except sqlite3.Error:
        return None, []
    return [d[0] for d in cursor.description], cursor.fetchall()


def survey_database(path: str) -> dict:
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    connection.text_factory = lambda raw: raw.decode("utf-8", "replace")
    try:
        master = connection.execute("SELECT type, name, tbl_name FROM sqlite_master").fetchall()
        tables = {}
        indexes = []
        for kind, name, _table in master:
            if kind == "table":
                info = connection.execute(f'PRAGMA table_info("{name}")').fetchall()
                tables[name] = {"columns": [[c[1], c[2]] for c in info],
                                "rows": connection.execute(f'SELECT COUNT(*) FROM "{name}"').fetchone()[0]}
            elif kind == "index":
                indexes.append(name)
        out = {"tables": tables, "indexes": sorted(indexes)}

        columns, data = rows(connection, "ParamScheme")
        if columns is not None:
            keep = [c for c in columns if c != "_PW_ID"]
            index = [columns.index(c) for c in keep]
            out["param_columns"] = keep
            out["param"] = [[r[i] for i in index] for r in data]
        columns, data = rows(connection, "ElemScheme")
        if columns is not None:
            name_at, type_at = columns.index("TableName"), columns.index("ElemType")
            out["elem"] = sorted([r[name_at], r[type_at]] for r in data)
        columns, data = rows(connection, "ExternalTableAndColumnName")
        if columns is not None:
            out["external"] = sorted([list(r) for r in data])

        versions = {}
        for name, table in tables.items():
            wanted = [c for c, _t in table["columns"] if VERSION_COLUMN.search(c)]
            if not wanted:
                continue
            for column in wanted:
                values = {}
                try:
                    found = connection.execute(f'SELECT "{column}" FROM "{name}"').fetchall()
                except sqlite3.Error:
                    continue
                for (value,) in found:
                    if isinstance(value, bytes):
                        value = f"blob:{len(value)}"
                    elif isinstance(value, str) and len(value) > 32:
                        value = f"text:{len(value)}"
                    key = json.dumps(value)
                    if key in values or len(values) < MAX_VALUES:
                        values[key] = values.get(key, 0) + 1
                versions[f"{name}.{column}"] = values
        out["versions"] = versions
        return out
    finally:
        connection.close()


def survey(path: str) -> dict:
    ext = os.path.splitext(path)[1].lower()
    if ext == ".clip":
        container, database = read_clip(path)
        record = {"container": container}
        if database is None:
            record["error"] = "no CHNKSQLi"
            return record
    else:
        with open(path, "rb") as handle:
            database = handle.read()
        record = {}
    record["sqlite"] = sqlite_header(database[:100])
    record["sqlite_bytes"] = len(database)
    if ext == ".clip":
        with tempfile.NamedTemporaryFile(suffix=".sqlite", delete=False) as temporary:
            temporary.write(database)
            temp_path = temporary.name
        try:
            record["db"] = survey_database(temp_path)
        finally:
            os.unlink(temp_path)
    else:
        record["db"] = survey_database(path)
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root")
    parser.add_argument("out")
    parser.add_argument("--seconds", type=float, default=0, help="stop starting new files after this long")
    parser.add_argument("--list", help="file of relative paths to survey (default: walk ROOT)")
    args = parser.parse_args()
    done = set()
    if os.path.exists(args.out):
        with open(args.out, encoding="utf-8") as existing:
            for line in existing:
                try:
                    done.add(json.loads(line)["key"])
                except (ValueError, KeyError):
                    pass
    if args.list:
        with open(args.list, encoding="utf-8") as listing:
            relatives = [line.rstrip("\n") for line in listing if line.strip()]
    else:
        relatives = []
        for directory, _dirs, files in os.walk(args.root):
            for name in files:
                if name.lower().endswith((".clip", ".cmc")):
                    relatives.append(os.path.relpath(os.path.join(directory, name), args.root))
    relatives.sort()
    started = time.time()
    count = 0
    with open(args.out, "a", encoding="utf-8") as out:
        for relative in relatives:
            key = hashlib.sha256(relative.encode("utf-8")).hexdigest()[:16]
            if key in done:
                continue
            if args.seconds and time.time() - started > args.seconds:
                break
            path = os.path.join(args.root, relative)
            stat = os.stat(path)
            record = {"key": key, "ext": os.path.splitext(path)[1].lower(), "size": stat.st_size,
                      "mtime": time.strftime("%Y-%m-%d", time.localtime(stat.st_mtime))}
            try:
                record.update(survey(path))
            except Exception as error:  # keep surveying the rest
                record["error"] = f"{type(error).__name__}: {error}"
            out.write(json.dumps(record, ensure_ascii=False) + "\n")
            out.flush()
            done.add(key)
            count += 1
    remaining = sum(1 for r in relatives if hashlib.sha256(r.encode("utf-8")).hexdigest()[:16] not in done)
    print(f"surveyed {count} files, {remaining} remaining", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
