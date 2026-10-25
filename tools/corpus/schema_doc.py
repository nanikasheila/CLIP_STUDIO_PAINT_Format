#!/usr/bin/env python3
"""Write the schema catalog (docs/spec/schema-catalog.md) from the corpus summaries (R3, V2).

Why: R3 asks for every column CSP registers (the union of `ParamScheme` over
     the corpus) with its type, link, how often it is used, the values seen
     and what it means. The first three come from the corpus; the meaning is
     written by hand in docs/spec/schema-semantics.tsv. This script joins
     them, so the catalog can be regenerated when either changes.
How: schema_catalog.py gives the registered columns, value_census.py the
     use and the numeric values, the TSV the meaning and its status, and the
     library source which columns the code reads or writes. Only numeric
     ranges and text lengths are printed; no text values (names) at all.

Usage::

    python3 schema_doc.py --catalog schema_catalog.json \\
        --census value_census.json --census value_census_cmc.json \\
        --semantics docs/spec/schema-semantics.tsv --source crates/csp-format/src \\
        --out docs/spec/schema-catalog.md
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import re
import sys

TYPES = {1: "整数", 2: "実数", 3: "文字列", 4: "BLOB"}
STATUSES = ("確認済み", "推測", "未解明")
# Tables of the core areas (R4–R10): every column must be 確認済み or 推測.
CORE = ("Project", "Canvas", "Layer", "Offscreen", "VOffscreen", "Mipmap", "MipmapInfo", "LayerThumbnail",
        "CanvasPreview", "VectorObjectList", "ExternalChunk", "CanvasNode")


class Semantics:
    def __init__(self, path: str):
        self.exact: dict[str, tuple[str, str]] = {}
        self.rules: list[tuple[re.Pattern, str, str]] = []
        with open(path, encoding="utf-8") as handle:
            for number, line in enumerate(handle, 1):
                line = line.rstrip("\n")
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) != 3 or parts[1] not in STATUSES:
                    raise SystemExit(f"{path}:{number}: expected key<TAB>status<TAB>meaning")
                key, status, meaning = parts
                if key.startswith("re:"):
                    self.rules.append((re.compile(key[3:]), status, meaning))
                elif key in self.exact:
                    raise SystemExit(f"{path}:{number}: {key} twice")
                else:
                    self.exact[key] = (status, meaning)
        self.used: set[str] = set()

    def table(self, table: str) -> tuple[str, str] | None:
        return self.exact.get(f"{table}.*")

    def column(self, table: str, column: str) -> tuple[str, str]:
        key = f"{table}.{column}"
        if key in self.exact:
            self.used.add(key)
            return self.exact[key]
        for pattern, status, meaning in self.rules:
            if pattern.search(key):
                self.used.add("re:" + pattern.pattern)
                return status, meaning
        return "未解明", ""


def source_names(root: str) -> set[str]:
    names: set[str] = set()
    for directory, _dirs, files in os.walk(root):
        for name in files:
            if name.endswith(".rs"):
                with open(os.path.join(directory, name), encoding="utf-8") as handle:
                    names.update(re.findall(r'"([A-Za-z_][A-Za-z0-9_]*)"', handle.read()))
    return names


def number(text: str) -> float | None:
    try:
        return float(text)
    except ValueError:
        return None


def short(value: float) -> str:
    if value == int(value) and abs(value) < 1e15:
        return str(int(value))
    return f"{value:.4g}"


def values_text(entry: dict) -> str:
    if not entry or not entry["not_null"]:
        return ""
    values = [v for v in (number(k) for k in entry["values"]) if v is not None]
    parts = []
    if values:
        low, high = entry.get("min", min(values)), entry.get("max", max(values))
        more = "+" if entry["more_values"] else ""
        count = f"{len(values)}{more} 種"
        parts.append(f"{short(low)}" if low == high else f"{short(low)}〜{short(high)}（{count}）")
    if entry["max_length"]:
        parts.append(f"長さ ≤ {entry['max_length']}")
    return "、".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--catalog", required=True)
    parser.add_argument("--census", action="append", default=[])
    parser.add_argument("--semantics", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--coverage", help="also write the coverage numbers as JSON")
    args = parser.parse_args()

    with open(args.catalog, encoding="utf-8") as handle:
        catalog = json.load(handle)
    census: dict[str, dict] = {}
    census_files: dict[str, int] = {}
    for path in args.census:
        with open(path, encoding="utf-8") as handle:
            data = json.load(handle)
        for key, entry in data["columns"].items():
            census[key] = entry
            census_files[key.split("|")[0]] = data["files"]
    semantics = Semantics(args.semantics)
    code = source_names(args.source)

    # (table, column) -> what is known, merged over .clip and .cmc.
    columns: dict[tuple[str, str], dict] = {}

    def slot(table: str, column: str) -> dict:
        return columns.setdefault((table, column), {
            "exts": set(), "types": collections.Counter(), "links": set(), "registered": 0, "census": {}})

    for key, entry in catalog["params"].items():
        ext, table, column, data_type, _flag, _owner, _lock, _locks, _alt, _alts, link = json.loads(key)
        item = slot(table, column)
        item["exts"].add(ext)
        item["types"][data_type] += entry["files"]
        if link:
            item["links"].add(link)
        item["registered"] += entry["files"]
    for key, entry in census.items():
        ext, table, column = key.split("|")
        item = slot(table, column)
        item["exts"].add(ext)
        item["census"][ext] = entry
    externals = {tuple(json.loads(key)[1:]) for key in catalog["externals"]}
    max_index: dict[str, int] = collections.defaultdict(int)
    for key, entry in catalog["elems"].items():
        _ext, table, _kind = json.loads(key)
        max_index[table] = max(max_index[table], entry["max_index"])

    tables: dict[str, list[str]] = collections.defaultdict(list)
    for table, column in columns:
        tables[table].append(column)

    counts = collections.Counter()
    core_counts = collections.Counter()
    table_rows: dict[str, list[str]] = {}
    for table in sorted(tables):
        rows = []
        for column in sorted(tables[table], key=lambda c: (c not in ("_PW_ID", "MainId"), c.lower())):
            item = columns[(table, column)]
            status, meaning = semantics.column(table, column)
            counts[status] += 1
            if table in CORE:
                core_counts[status] += 1
            type_text = "、".join(TYPES.get(t, str(t)) for t, _ in item["types"].most_common()) or "未登録"
            link = "、".join(sorted(item["links"]))
            if (table, column) in externals:
                link = (link + "、" if link else "") + "外部チャンク"
            use = []
            value = []
            for ext in sorted(item["census"]):
                entry = item["census"][ext]
                use.append(f"{ext} {entry['files']}/{entry['files_set']}")
                text = values_text(entry)
                if text:
                    value.append(text if len(item["census"]) == 1 else f"{ext}: {text}")
            rows.append("| {} | {} | {} | {} | {} | {} | {} | {} |".format(
                column, type_text, link, "<br>".join(use) or "—", "<br>".join(value),
                "○" if column in code else "", status, meaning.replace("|", "\\|")))
        table_rows[table] = rows

    total = sum(counts.values())
    core_total = sum(core_counts.values())
    out = []
    out.append("# スキーマ目録\n")
    out.append("> この文書は `tools/corpus/schema_doc.py` が生成する。意味は `docs/spec/schema-semantics.tsv` を直して再生成する。\n")
    out.append(f"コーパスの `.clip` {census_files.get('.clip', 0)} 件・`.cmc` {census_files.get('.cmc', 0)} 件の "
               f"`ParamScheme`（CSP が登録している全列）と、実際の表の列の和集合。{len(tables)} 表、{total} 列。\n")
    out.append("## 網羅率（V2）\n")
    out.append("| 範囲 | 列 | 確認済み | 推測 | 未解明 |")
    out.append("|---|---|---|---|---|")
    out.append(f"| コア（{'、'.join(CORE)}） | {core_total} | " +
               " | ".join(str(core_counts[s]) for s in STATUSES) + " |")
    out.append(f"| すべて | {total} | " + " | ".join(str(counts[s]) for s in STATUSES) + " |\n")
    out.append("## 見方\n")
    out.append("- 型: `ParamScheme.DataType`（1 整数、2 実数、3 文字列、4 BLOB）。SQLite の列の型とは違うことがある。"
               "未登録は `ParamScheme` になく表にだけある列（`_PW_ID`、`MainId` など）")
    out.append("- リンク: `ParamScheme.LinkTable`（この列の値がその表の `MainId`）。外部チャンク: "
               "`ExternalTableAndColumnName` にあり、値が外部チャンクの ID")
    out.append("- ファイル: 拡張子ごとに「表にこの列があるファイル数 / 値（NULL・0・空以外）を持つファイル数」")
    out.append("- 値: 数値の最小〜最大と種類の数（40 種まで数え、超えると +）。文字列・BLOB は最大の長さだけ（中身は載せない）")
    out.append("- コード: ライブラリ（`crates/csp-format/src`）が名前で読み書きしている列")
    out.append("- 状態: 確認済み（ライブラリが使い、コーパス・CSP・テストで確かめた）、推測（名前と値から）、未解明\n")
    for table in sorted(tables):
        out.append(f"## {table}\n")
        described = semantics.table(table)
        notes = []
        if described:
            notes.append(described[1])
        if max_index.get(table):
            notes.append(f"`ElemScheme.MaxIndex` は最大 {max_index[table]}")
        if table in CORE:
            notes.append("コア領域")
        if notes:
            out.append("。".join(notes) + "。\n")
        out.append("| 列 | 型 | リンク | ファイル | 値 | コード | 状態 | 意味 |")
        out.append("|---|---|---|---|---|---|---|---|")
        out.extend(table_rows[table])
        out.append("")
    with open(args.out, "w", encoding="utf-8") as handle:
        handle.write("\n".join(out))

    unused = [key for key in semantics.exact if not key.endswith(".*") and key not in semantics.used]
    unused += [f"re:{p.pattern}" for p, _s, _m in semantics.rules if f"re:{p.pattern}" not in semantics.used]
    for key in unused:
        print(f"semantics entry matches nothing: {key}", file=sys.stderr)
    summary = {"tables": len(tables), "columns": total, "status": dict(counts),
               "core_columns": core_total, "core_status": dict(core_counts)}
    print(json.dumps(summary, ensure_ascii=False))
    if args.coverage:
        with open(args.coverage, "w", encoding="utf-8") as handle:
            json.dump(summary, handle, ensure_ascii=False, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
