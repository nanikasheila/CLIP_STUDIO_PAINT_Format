"""R18: what changed between two works (.cmc and their page files), for works made in CSP for the test.

    python tools/validation/cmc_diff.py A.cmc B.cmc [--pages]

Prints every ``Project`` and ``CanvasNode`` column whose value differs (the
nodes are matched by ``MainId``) and, with ``--pages``, the ``Canvas`` columns
of the page files (matched by position in the page list). Values are printed,
so use it on test works only, never on the user's works.
"""
from __future__ import annotations

import argparse
import sqlite3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))

from csp_format import read_clip, read_cmc  # noqa: E402


def short(value):
    if isinstance(value, bytes):
        return f"<{len(value)} bytes>" if len(value) > 24 else value.hex()
    return value


def rows(connection: sqlite3.Connection, table: str) -> dict:
    connection.row_factory = sqlite3.Row
    try:
        result = connection.execute(f'SELECT * FROM "{table}"').fetchall()
    except sqlite3.OperationalError:
        return {}
    # Tables without MainId (ElemScheme) are matched by position.
    return {(row["MainId"] if "MainId" in row.keys() else index): dict(row) for index, row in enumerate(result)}


def compare(name: str, a: dict, b: dict) -> None:
    for key in sorted(set(a) | set(b)):
        if key not in a or key not in b:
            print(f"{name} {key}: only in {'B' if key not in a else 'A'}")
            continue
        for column in sorted(set(a[key]) | set(b[key])):
            x, y = a[key].get(column, "<no column>"), b[key].get(column, "<no column>")
            if x != y:
                print(f"{name} {key}.{column}: {short(x)} -> {short(y)}")


def canvas(path: Path) -> dict:
    with read_clip(path) as clip:
        return {1: dict(clip.row("Canvas", 1) or {})} if clip.has_table("Canvas") else {}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("a", type=Path)
    parser.add_argument("b", type=Path)
    parser.add_argument("--pages", action="store_true")
    args = parser.parse_args()
    works = [read_cmc(path) for path in (args.a, args.b)]
    for table in ("Project", "CanvasNode", "ElemScheme"):
        compare(table, rows(works[0].database, table), rows(works[1].database, table))
    if args.pages:
        pages = [work.page_file_paths() for work in works]
        for index, (x, y) in enumerate(zip(*pages)):
            if x.is_file() and y.is_file():
                compare(f"page {index + 1} Canvas", canvas(x), canvas(y))
        if len(pages[0]) != len(pages[1]):
            print(f"pages: {len(pages[0])} -> {len(pages[1])}")


if __name__ == "__main__":
    main()
