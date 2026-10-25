"""Census of sub tool settings columns (A5): for every column of
``Variant`` (and ``Node`` / ``Manager``) in the given tool databases, its
declared type, how many settings rows set it, for which tool kinds, and the
observed range. Text values are never printed (NA1); numbers and blob
layouts only.

    python tools/assets/subtool_census.py OUT.tsv FILE...
"""
from __future__ import annotations

import collections
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))

from csp_format.assets import subtool  # noqa: E402
from csp_format.errors import CspFormatError  # noqa: E402


def main() -> int:
    out, files = Path(sys.argv[1]), sys.argv[2:]
    stats: dict = {}
    for name in files:
        try:
            db = subtool.SubToolDb(Path(name).read_bytes())
        except CspFormatError:
            continue
        kinds = {}
        for node in db.nodes:
            if node.variant_id:
                kinds[node.variant_id] = node.kind
                kinds.setdefault(node.init_variant_id, node.kind)
        declared = {r[1]: r[2] for r in db.connection.execute("PRAGMA table_info(Variant)")}
        for variant_id, row in db.variants.items():
            kind = kinds.get(variant_id, "common" if variant_id == db.manager.get("CommonVariantID") else "?")
            for column, value in row.items():
                s = stats.setdefault(column, {"type": declared.get(column, ""), "rows": 0, "kinds": collections.Counter(),
                                              "min": None, "max": None, "values": collections.Counter(), "blob": collections.Counter()})
                if value is None:
                    continue
                s["rows"] += 1
                s["kinds"][kind] += 1
                if isinstance(value, (int, float)):
                    s["min"] = value if s["min"] is None else min(s["min"], value)
                    s["max"] = value if s["max"] is None else max(s["max"], value)
                    if len(s["values"]) <= 12:
                        s["values"][value] += 1
                elif isinstance(value, bytes):
                    layout = subtool.kind_of_column(column)
                    s["blob"][f"{layout}:{len(value)}" if layout == "unknown" else layout] += 1
                else:
                    s["values"]["<text>"] += 1
    with out.open("w", encoding="utf-8") as f:
        f.write("column\ttype\trows\tkinds\tmin\tmax\tvalues\tblob\n")
        for column in sorted(stats):
            s = stats[column]
            kinds = ",".join(f"{k}:{n}" for k, n in s["kinds"].most_common())
            values = "" if len(s["values"]) > 12 else ",".join(f"{v}" for v in sorted(s["values"], key=str))
            blob = ",".join(f"{k}:{n}" for k, n in s["blob"].most_common(3))
            f.write(f"{column}\t{s['type']}\t{s['rows']}\t{kinds}\t{s['min']}\t{s['max']}\t{values}\t{blob}\n")
    print(f"{len(stats)} columns")
    return 0


if __name__ == "__main__":
    sys.exit(main())
