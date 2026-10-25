"""Make docs/assets/spec/sqlite-tables.tsv (A2): every table and column of
CSP's SQLite settings files, with the storage types and value ranges seen.

    python tools/assets/sqlite_census.py ROOT OUT.tsv --cspf target/release/cspf

Each SQLite file under ROOT is named by ``cspf asset kind`` (its format
pattern, e.g. ``.khc`` or ``canvasviewsettings``) and read from a copy in
memory. Sub tool files (.sut, .todb, .bfps) are left out: their columns are
in subtool-columns.tsv. Only numbers are reported as values (min and max);
text and BLOB columns get their length range, so the table carries no
paths, names or other contents of the user's files.
"""
from __future__ import annotations

import argparse
import sqlite3
import subprocess
import sys
from pathlib import Path

SUB_TOOL_FORMATS = {".sut", ".todb", ".bfps"}


def read_db(data: bytes) -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    if hasattr(connection, "deserialize"):
        connection.deserialize(data)
        return connection
    import os
    import tempfile
    handle, name = tempfile.mkstemp()
    os.write(handle, data)
    os.close(handle)
    disk = sqlite3.connect(f"file:{name}?mode=ro", uri=True)
    disk.backup(connection)
    disk.close()
    os.unlink(name)
    return connection


def quote(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


class Column:
    def __init__(self, declared: str):
        self.declared = declared
        self.files = 0
        self.rows = 0
        self.nulls = 0
        self.types: set[str] = set()
        self.low = self.high = None
        self.short = self.long = None

    def add(self, value) -> None:
        self.rows += 1
        if value is None:
            self.nulls += 1
            self.types.add("null")
            return
        if isinstance(value, (int, float)):
            self.types.add("integer" if isinstance(value, int) else "real")
            self.low = value if self.low is None else min(self.low, value)
            self.high = value if self.high is None else max(self.high, value)
            return
        self.types.add("text" if isinstance(value, str) else "blob")
        n = len(value.encode("utf-8")) if isinstance(value, str) else len(value)
        self.short = n if self.short is None else min(self.short, n)
        self.long = n if self.long is None else max(self.long, n)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--cspf", required=True)
    args = parser.parse_args()
    files = sorted(p for p in args.root.rglob("*") if p.is_file() and p.read_bytes()[:16] == b"SQLite format 3\0")
    kinds = {}
    for i in range(0, len(files), 200):
        batch = files[i:i + 200]
        out = subprocess.run([args.cspf, "asset", "kind", *map(str, batch)], capture_output=True, text=True, check=True)
        for path, line in zip(batch, out.stdout.splitlines()):
            kinds[path] = line.split("\t")[1]
    census: dict[tuple[str, str, str], Column] = {}
    formats: dict[str, int] = {}
    for path in files:
        fmt = kinds[path]
        if fmt in SUB_TOOL_FORMATS:
            continue
        formats[fmt] = formats.get(fmt, 0) + 1
        db = read_db(path.read_bytes())
        tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")]
        for table in tables:
            if table == "sqlite_sequence":
                continue
            columns = [(r[1], r[2]) for r in db.execute(f"PRAGMA table_info({quote(table)})")]
            for name, declared in columns:
                census.setdefault((fmt, table, name), Column(declared)).files += 1
            for row in db.execute(f"SELECT * FROM {quote(table)}"):
                for (name, _), value in zip(columns, row):
                    census[(fmt, table, name)].add(value)
        db.close()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", encoding="utf-8", newline="") as f:
        f.write("format\ttable\tcolumn\tdeclared\tseen\tfiles\trows\tnulls\tmin\tmax\tbytes\n")
        for (fmt, table, name), c in sorted(census.items()):
            seen = ",".join(sorted(c.types))
            size = "" if c.short is None else f"{c.short}–{c.long}" if c.short != c.long else str(c.short)
            low = "" if c.low is None else repr(c.low)
            high = "" if c.high is None else repr(c.high)
            f.write("\t".join([fmt, table, name, c.declared, seen, str(c.files), str(c.rows), str(c.nulls),
                               low, high, size]) + "\n")
    print(f"{sum(formats.values())} files, {len(formats)} formats, {len(census)} columns")
    return 0


if __name__ == "__main__":
    sys.exit(main())
