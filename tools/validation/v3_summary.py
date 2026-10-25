"""V3: summarise ``cspf corpus --render`` output (PSNR of our render against CSP's saved preview).

    python tools/validation/v3_summary.py OUT.jsonl...

The acceptance set of R10 is the files without vector, text, 3D or
correction layers. Prints counts and PSNR quantiles per group; no paths.
"""
from __future__ import annotations

import json
import statistics
import sys
from collections import Counter


def group(render: dict) -> str:
    if render.get("vector_layers") or render.get("text_layers"):
        return "vector/text"
    if render.get("correction_layers") or "3d" in render.get("without_pixels", {}):
        return "correction/3d"
    return "plain"


def quantiles(values: list[float]) -> str:
    if not values:
        return "-"
    values = sorted(values)
    pick = lambda q: values[min(int(q * len(values)), len(values) - 1)]  # noqa: E731
    return f"p10 {pick(0.1):.1f} / median {statistics.median(values):.1f} / p90 {pick(0.9):.1f}"


def main() -> None:
    rows = {}
    for path in sys.argv[1:]:
        for line in open(path, encoding="utf-8"):
            try:
                row = json.loads(line)
            except ValueError:
                continue
            rows[row["key"]] = row
    errors = Counter()
    groups: dict[str, list[float]] = {}
    no_preview = 0
    reasons = Counter()
    for row in rows.values():
        if not row["ok"]:
            errors[(row["report"].get("errors") or ["?"])[0].split(":")[0][:60]] += 1
            continue
        report = row["report"]
        if report.get("psnr") is None:
            no_preview += 1
            continue
        name = group(report["render"])
        psnr = min(report["psnr"], 99.0)
        groups.setdefault(name, []).append(psnr)
        if name == "plain" and psnr < 30:
            render = report["render"]
            for key in ("unknown_blend", "effects_skipped", "without_pixels"):
                for item in render.get(key, {}):
                    reasons[f"{key}:{item}"] += 1
            for key in ("gradients_approximate", "tones_approximate", "corrections_approximate", "material_layers"):
                if render.get(key):
                    reasons[key] += 1
    print(f"files {len(rows)}, errors {sum(errors.values())}, no preview {no_preview}")
    for message, count in errors.most_common():
        print(f"  error {count}: {message}")
    for name, values in sorted(groups.items()):
        good = sum(v >= 30 for v in values)
        print(f"{name}: {len(values)} files, >= 30 dB {good} ({100 * good / len(values):.1f}%), {quantiles(values)}")
    every = [v for values in groups.values() for v in values]
    print(f"all: {len(every)} files, >= 30 dB {sum(v >= 30 for v in every)} ({100 * sum(v >= 30 for v in every) / max(len(every), 1):.1f}%)")
    if reasons:
        print("plain files under 30 dB have:", dict(reasons.most_common()))


if __name__ == "__main__":
    main()
