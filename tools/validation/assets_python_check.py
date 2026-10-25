"""W5: compare the Python asset reader (python/csp_format/assets) with the
Rust crate (crates/csp-assets) on every file under ROOT.

For each file, runs the ``asset_digest`` example and computes the same
digest in Python (sub tool trees and every decoded setting, serialised
documents, colour sets, material catalogues, envelopes, caches), then writes
one JSON line ``{"key", "ok", "differences"}``. Paths are reduced to a hash
so the output carries no names.

    cargo build --release --example asset_digest -p csp-assets
    python tools/validation/assets_python_check.py ROOT OUT.jsonl --digest target/release/examples/asset_digest
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))

from csp_format.assets import c2f, colorset, material, serial, subtool  # noqa: E402
from csp_format.errors import CspFormatError  # noqa: E402


def python_digest(name: str, data: bytes):
    if data.startswith(subtool.SQLITE):
        db = subtool.SubToolDb(data)
        tree = [[depth, path, db.nodes[i].name, db.nodes[i].kind, db.nodes[i].variant_id, db.nodes[i].init_variant_id]
                for i, depth, path in db.tree()]
        return {"kind": db.kind, "tree": tree, "settings": {str(k): db.settings(k) for k in sorted(db.variants)}}
    if serial.looks_like(data):
        return serial.to_json(serial.parse(data))
    if data.startswith(b"SLCC"):
        return colorset.parse(data).to_json()
    if data.lstrip(b"\xef\xbb\xbf").startswith(b"<?xml") and name == "catalog.xml":
        return material.catalog(data)
    if data.lstrip(b"\xef\xbb\xbf").startswith(b"<?xml") and name in ("info.xml", "layerData.xml"):
        return material.info_list(data)
    if data.startswith(c2f.SIGNATURE):
        return c2f.summary(data)
    if material.looks_like_cac(data):
        return material.cac_strings(data)
    return None


def differences(a, b, path="", out=None, limit=20):
    out = [] if out is None else out
    if len(out) >= limit:
        return out
    if isinstance(a, dict) and isinstance(b, dict):
        for key in sorted(set(a) | set(b)):
            if key not in a or key not in b:
                out.append(f"{path}/{key}: only in {'python' if key in a else 'rust'}")
            else:
                differences(a[key], b[key], f"{path}/{key}", out, limit)
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            out.append(f"{path}: {len(a)} vs {len(b)} items")
        for i, (x, y) in enumerate(zip(a, b)):
            differences(x, y, f"{path}[{i}]", out, limit)
    elif isinstance(a, float) or isinstance(b, float):
        if not (isinstance(a, (int, float)) and isinstance(b, (int, float)) and float(a) == float(b)):
            out.append(f"{path}: {a!r} vs {b!r}")
    elif a != b:
        out.append(f"{path}: {str(a)[:60]!r} vs {str(b)[:60]!r}")
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--digest", required=True)
    args = parser.parse_args()
    files = sorted(p for p in args.root.rglob("*") if p.is_file())
    failed = compared = 0
    with args.out.open("w", encoding="utf-8") as out:
        for path in files:
            data = path.read_bytes()
            try:
                mine = {"ok": True, "digest": python_digest(path.name, data)}
            except CspFormatError as error:
                mine = {"ok": False, "error": str(error)}
            if mine.get("digest") is None and mine["ok"]:
                continue
            rust = json.loads(subprocess.run([args.digest, str(path)], capture_output=True, check=True).stdout)
            if mine["ok"] != rust["ok"]:
                found = [f"python ok={mine['ok']}, rust ok={rust['ok']}"]
            else:
                found = differences(mine.get("digest"), rust.get("digest"))
            key = hashlib.sha256(str(path.relative_to(args.root)).encode()).hexdigest()[:16]
            out.write(json.dumps({"key": key, "ok": not found, "differences": found}) + "\n")
            compared += 1
            failed += bool(found)
    print(f"{compared} compared, {failed} differ")
    return 1 if failed or not compared else 0


if __name__ == "__main__":
    sys.exit(main())
