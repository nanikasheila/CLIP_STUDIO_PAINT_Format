#!/usr/bin/env python3
"""Survey the structure of every CSP file under a folder (read only).

Why: The format has no public specification. What the files actually hold
     (chunks, tables, columns, value ranges, blob layouts) is learnt from
     many real files saved by different CSP versions, so every claim in the
     specification can be checked against the whole corpus.
How: Each .clip / .cmc / .sut is opened read only. For a .clip the chunk
     container is walked with seeks (pixel chunks are not read), the SQLite
     chunk is copied to a temporary file and inspected. One JSON line per
     file is appended to the output; files already in the output are
     skipped, so an interrupted run can be resumed.

     Artwork never leaves the machine: no pixels, no text strings and no
     layer names are recorded, only structure (names of tables/columns,
     counts, numeric ranges, blob sizes and the first bytes of blobs).

Usage::

    python3 census.py ROOT OUT.jsonl [--limit N]

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

EXTERNAL_ID = re.compile(rb"^extrnlid[0-9A-Fa-f]{32}$")
# Text columns whose values are format settings, not user content.
TEXT_WHITELIST = re.compile(r"(Version|Format|Kind|Unit|Type|Scheme|TableName|ColumnName|Name)$")
TEXT_BLACKLIST = re.compile(r"(LayerName|TextLayerString|StoryName|AuthorName|Title|Subtitle|FileName|LinkPath|"
                            r"ComicStory|Comment|Memo|Path)")
MAX_DISTINCT = 40
PREFIX = 16


def walk_chunks(handle, file_size):
    """Return (header, chunks) of a CSF container. Chunk = dict(tag, pos, length, head)."""
    handle.seek(0)
    header = handle.read(24)
    if len(header) < 24 or header[:8] != b"CSFCHUNK":
        raise ValueError("not a CSFCHUNK file")
    declared_size, first = struct.unpack(">QQ", header[8:24])
    chunks = []
    position = first
    while position + 16 <= file_size:
        handle.seek(position)
        raw = handle.read(16)
        tag = raw[:8].decode("latin-1")
        (length,) = struct.unpack(">Q", raw[8:16])
        head = handle.read(min(length, 128))
        chunks.append({"tag": tag, "pos": position, "length": length, "head": head})
        position += 16 + length
        if tag == "CHNKFoot":
            break
    return {"declared_size": declared_size, "first": first, "end": position}, chunks


def blob_signature(value: bytes) -> str:
    return value[:PREFIX].hex()


class ColumnStats:
    def __init__(self):
        self.types = {}
        self.ints = {}
        self.int_min = None
        self.int_max = None
        self.real_min = None
        self.real_max = None
        self.blob_len_min = None
        self.blob_len_max = None
        self.blob_prefix = {}
        self.text_len_max = 0
        self.text_values = {}
        self.external = 0

    def add(self, value, keep_text: bool):
        kind = type(value).__name__
        self.types[kind] = self.types.get(kind, 0) + 1
        if isinstance(value, int):
            self.int_min = value if self.int_min is None else min(self.int_min, value)
            self.int_max = value if self.int_max is None else max(self.int_max, value)
            if len(self.ints) < MAX_DISTINCT or value in self.ints:
                self.ints[value] = self.ints.get(value, 0) + 1
            else:
                self.ints["_other"] = self.ints.get("_other", 0) + 1
        elif isinstance(value, float):
            self.real_min = value if self.real_min is None else min(self.real_min, value)
            self.real_max = value if self.real_max is None else max(self.real_max, value)
        elif isinstance(value, bytes):
            if EXTERNAL_ID.match(value):
                self.external += 1
            n = len(value)
            self.blob_len_min = n if self.blob_len_min is None else min(self.blob_len_min, n)
            self.blob_len_max = n if self.blob_len_max is None else max(self.blob_len_max, n)
            sig = blob_signature(value)
            if len(self.blob_prefix) < MAX_DISTINCT or sig in self.blob_prefix:
                self.blob_prefix[sig] = self.blob_prefix.get(sig, 0) + 1
        elif isinstance(value, str):
            if EXTERNAL_ID.match(value.encode("latin-1", "replace")):
                self.external += 1
            self.text_len_max = max(self.text_len_max, len(value))
            if keep_text and len(value) <= 64:
                if len(self.text_values) < MAX_DISTINCT or value in self.text_values:
                    self.text_values[value] = self.text_values.get(value, 0) + 1

    def to_json(self):
        out = {"types": self.types}
        if self.ints:
            out["int"] = {"min": self.int_min, "max": self.int_max, "values": {str(k): v for k, v in self.ints.items()}}
        if self.real_min is not None:
            out["real"] = {"min": self.real_min, "max": self.real_max}
        if self.blob_len_min is not None:
            out["blob"] = {"min": self.blob_len_min, "max": self.blob_len_max, "prefix": self.blob_prefix}
        if self.text_len_max:
            out["text"] = {"max_len": self.text_len_max}
            if self.text_values:
                out["text"]["values"] = self.text_values
        if self.external:
            out["external"] = self.external
        return out


def survey_database(path: str, external_heads: dict):
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    connection.text_factory = lambda raw: raw.decode("utf-8", "replace")
    try:
        master = connection.execute("SELECT type, name, tbl_name, sql FROM sqlite_master").fetchall()
        tables = {}
        objects = []
        external_columns = {}
        for kind, name, tbl_name, sql in master:
            objects.append({"type": kind, "name": name, "table": tbl_name, "sql": sql})
            if kind != "table":
                continue
            columns = connection.execute(f'PRAGMA table_info("{name}")').fetchall()
            column_names = [c[1] for c in columns]
            stats = [ColumnStats() for _ in column_names]
            keep = [bool(TEXT_WHITELIST.search(c)) and not TEXT_BLACKLIST.search(c) for c in column_names]
            rows = 0
            for row in connection.execute(f'SELECT * FROM "{name}"'):
                rows += 1
                for index, value in enumerate(row):
                    if value is None:
                        stats[index].types["NoneType"] = stats[index].types.get("NoneType", 0) + 1
                        continue
                    stats[index].add(value, keep[index])
                    raw = value.encode("latin-1", "replace") if isinstance(value, str) else value
                    if isinstance(raw, bytes) and EXTERNAL_ID.match(raw):
                        head = external_heads.get(raw.decode("latin-1"))
                        key = f"{name}.{column_names[index]}"
                        entry = external_columns.setdefault(key, {"refs": 0, "present": 0, "prefix": {}})
                        entry["refs"] += 1
                        if head is not None:
                            entry["present"] += 1
                            sig = head.hex()
                            if len(entry["prefix"]) < MAX_DISTINCT or sig in entry["prefix"]:
                                entry["prefix"][sig] = entry["prefix"].get(sig, 0) + 1
            storage = {}
            for c in column_names:
                kinds = connection.execute(f'SELECT typeof("{c}"), count(*) FROM "{name}" GROUP BY 1').fetchall()
                storage[c] = {k: n for k, n in kinds}
            tables[name] = {
                "columns": [[c[1], c[2]] for c in columns],
                "rows": rows,
                "stats": {column_names[i]: stats[i].to_json() for i in range(len(column_names))},
                "storage": storage,
            }
        return {"objects": objects, "tables": tables, "external_columns": external_columns}
    finally:
        connection.close()


def survey_clip(path: str):
    size = os.path.getsize(path)
    with open(path, "rb") as handle:
        header, chunks = walk_chunks(handle, size)
        tags = {}
        sequence = []
        external_heads = {}
        exta_lengths = []
        head_body = None
        sqlite_chunk = None
        for chunk in chunks:
            tags[chunk["tag"]] = tags.get(chunk["tag"], 0) + 1
            if not sequence or sequence[-1][0] != chunk["tag"]:
                sequence.append([chunk["tag"], 1])
            else:
                sequence[-1][1] += 1
            if chunk["tag"] == "CHNKHead":
                head_body = chunk["head"][:chunk["length"]]
            elif chunk["tag"] == "CHNKExta":
                body = chunk["head"]
                (id_len,) = struct.unpack(">Q", body[:8])
                ext_id = body[8:8 + id_len].decode("latin-1")
                external_heads[ext_id] = body[8 + id_len:8 + id_len + 24]
                exta_lengths.append(chunk["length"])
            elif chunk["tag"] == "CHNKSQLi":
                sqlite_chunk = chunk
        record = {
            "header": {"declared_size": header["declared_size"], "size_matches": header["declared_size"] == size,
                       "first": header["first"], "end_matches": header["end"] == size},
            "chunks": tags,
            "sequence": sequence,
            "head": _head_summary(head_body),
            "exta": {"count": len(exta_lengths), "bytes": sum(exta_lengths),
                     "max": max(exta_lengths) if exta_lengths else 0},
        }
        if sqlite_chunk is None:
            record["error"] = "no CHNKSQLi"
            return record
        handle.seek(sqlite_chunk["pos"] + 16)
        database = handle.read(sqlite_chunk["length"])
    record["sqlite_bytes"] = len(database)
    record["sqlite_header"] = database[:16].decode("latin-1")
    with tempfile.NamedTemporaryFile(suffix=".sqlite", delete=False) as temporary:
        temporary.write(database)
        temp_path = temporary.name
    try:
        record["db"] = survey_database(temp_path, external_heads)
        record["db"]["chunk_order"] = _offset_check(temp_path, chunks)
    finally:
        os.unlink(temp_path)
    return record


def _head_summary(body):
    if body is None:
        return None
    out = {"length": len(body)}
    if len(body) >= 24:
        out["u64"] = list(struct.unpack(">QQQ", body[:24]))
    return out


def _offset_check(path, chunks):
    """Compare ExternalChunk.Offset with the actual chunk positions."""
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        try:
            rows = connection.execute("SELECT * FROM ExternalChunk").fetchall()
            columns = [d[0] for d in connection.execute("SELECT * FROM ExternalChunk LIMIT 0").description]
        except sqlite3.Error:
            return None
        positions = {c["pos"] for c in chunks if c["tag"] == "CHNKExta"}
        try:
            offset_index = columns.index("Offset")
        except ValueError:
            return {"columns": columns}
        hits = sum(1 for r in rows if r[offset_index] in positions)
        return {"columns": columns, "rows": len(rows), "offset_hits": hits, "exta_chunks": len(positions)}
    finally:
        connection.close()


def survey_sqlite_file(path: str):
    with open(path, "rb") as handle:
        magic = handle.read(16)
    record = {"sqlite_header": magic.decode("latin-1")}
    if not magic.startswith(b"SQLite format 3"):
        record["error"] = "not sqlite"
        return record
    record["db"] = survey_database(path, {})
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root")
    parser.add_argument("out")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--seconds", type=float, default=0, help="stop starting new files after this long")
    args = parser.parse_args()
    done = set()
    if os.path.exists(args.out):
        with open(args.out, encoding="utf-8") as existing:
            for line in existing:
                try:
                    done.add(json.loads(line)["key"])
                except (ValueError, KeyError):
                    pass
    paths = []
    for directory, _dirs, files in os.walk(args.root):
        for name in files:
            lower = name.lower()
            if lower.endswith((".clip", ".cmc", ".sut")):
                paths.append(os.path.join(directory, name))
    paths.sort()
    count = 0
    run_started = time.time()
    with open(args.out, "a", encoding="utf-8") as out:
        for path in paths:
            relative = os.path.relpath(path, args.root)
            key = hashlib.sha1(relative.encode("utf-8")).hexdigest()[:16]
            if key in done:
                continue
            if args.seconds and time.time() - run_started > args.seconds:
                break
            started = time.time()
            stat = os.stat(path)
            record = {"key": key, "path": relative, "ext": os.path.splitext(path)[1].lower(), "size": stat.st_size,
                      "mtime": time.strftime("%Y-%m-%d", time.localtime(stat.st_mtime))}
            try:
                if record["ext"] == ".clip":
                    record.update(survey_clip(path))
                else:
                    record.update(survey_sqlite_file(path))
            except Exception as error:  # keep surveying the rest
                record["error"] = f"{type(error).__name__}: {error}"
            record["seconds"] = round(time.time() - started, 3)
            out.write(json.dumps(record, ensure_ascii=False) + "\n")
            out.flush()
            count += 1
            if args.limit and count >= args.limit:
                break
    remaining = sum(1 for p in paths if hashlib.sha1(os.path.relpath(p, args.root).encode("utf-8")).hexdigest()[:16]
                    not in done) - count
    print(f"surveyed {count} files, {len(done)} were already done, {remaining} remaining", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
