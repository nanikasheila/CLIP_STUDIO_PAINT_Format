"""Compare two V3 corpus runs file by file (keys and numbers only).

Usage: v3_delta.py OLD.jsonl[,OLD2.jsonl...] NEW.jsonl[,NEW2.jsonl...]

Later files in a list override earlier ones (for runs that re-rendered some
files). Files are grouped by the old run's render report: vector/text,
correction/3d, plain. Prints per group the count at 30 dB or more, the
medians and the spread of the change, and the files that got most worse.
"""
import json
import sys


def load(files):
    rows = {}
    for path in files:
        for line in open(path, encoding="utf-8"):
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            psnr = (row.get("report") or {}).get("psnr")
            if psnr is not None:
                rows[row["key"]] = (psnr, row)
    return rows


def group(row):
    render = row["report"].get("render") or {}
    without = render.get("without_pixels", {})
    if render.get("vector_layers", 0) or render.get("text_layers", 0) or without.get("vector") or without.get("text"):
        return "vector/text"
    if render.get("correction_layers", 0) or without.get("3d"):
        return "correction/3d"
    return "plain"


def median(values):
    return sorted(values)[len(values) // 2]


old = load(sys.argv[1].split(","))
new = load(sys.argv[2].split(","))
keys = [k for k in new if k in old]
print("compared", len(keys))
for name in ["plain", "correction/3d", "vector/text", "all"]:
    ks = [k for k in keys if name == "all" or group(old[k][1]) == name]
    if not ks:
        continue
    before = sum(old[k][0] >= 30 for k in ks)
    after = sum(new[k][0] >= 30 for k in ks)
    ds = sorted(new[k][0] - old[k][0] for k in ks)
    print(
        f"{name:14s} n={len(ks)} >=30: {before} -> {after}"
        f"  median {median([old[k][0] for k in ks]):.1f} -> {median([new[k][0] for k in ks]):.1f}"
        f"  delta min {ds[0]:.1f} p10 {ds[len(ds) // 10]:.1f} med {ds[len(ds) // 2]:.2f}"
        f" p90 {ds[9 * len(ds) // 10]:.1f} max {ds[-1]:.1f}"
        f"  worse>0.5: {sum(d < -0.5 for d in ds)} better>0.5: {sum(d > 0.5 for d in ds)}"
    )
worst = sorted(keys, key=lambda k: new[k][0] - old[k][0])[:8]
print("worst", [(k, round(old[k][0], 1), round(new[k][0], 1)) for k in worst])
