"""V7: compare the Python reader (python/csp_format) with the Rust crate on many files.

For each .clip, runs ``layer_digest`` (crates/csp-format/examples) and
computes the same digest with the Python reader, then writes one JSON line:
``{"key", "ok", "seconds", "differences": [...]}``. Only structure and
CRC-32s are compared or written: no names, text, paths or pixels.

    python tools/validation/python_reader_check.py ROOT LIST.txt OUT.jsonl \
        --digest target/release/examples/layer_digest [--shard K/N] [--seconds S]

Resumable: files already in OUT.jsonl are skipped.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))

from csp_format import read_clip  # noqa: E402


def crc(data: bytes) -> int:
    return zlib.crc32(data) & 0xFFFFFFFF


def image(found):
    if found is None:
        return None
    picture, (x, y) = found
    return [picture.width, picture.height, x, y, crc(picture.rgba)]


def digest(path: Path) -> dict:
    with read_clip(path) as clip:
        layers = []
        for layer, depth in clip.root.walk():
            thumbnail = clip.layer_thumbnail(layer)
            vector = clip.vector_objects(layer) if clip.has_table("VectorObjectList") else []
            layers.append({
                "id": layer.main_id,
                "depth": depth,
                "kind": layer.kind,
                "name": crc(layer.name.encode()),
                "visible": layer.visible,
                "opacity": layer.opacity,
                "pixels": image(clip.layer_pixels(layer)),
                "mask": image(clip.layer_mask(layer)),
                "thumbnail": None if thumbnail is None else crc(thumbnail.rgba),
                "vector": [len(vector), sum(len(item.raw_points) for item in vector)],
                "text": [crc(block.text.encode()) for block in clip.text_blocks(layer)],
            })
        preview = clip.preview()
        return {"canvas": [clip.canvas.width_px, clip.canvas.height_px],
                "preview": None if preview is None else crc(preview.rgba), "layers": layers}


def differences(rust: dict, python: dict) -> list[str]:
    found = []
    if rust.get("canvas") != python["canvas"]:
        found.append("canvas")
    if rust.get("preview") != python["preview"]:
        found.append("preview")
    rust_layers, python_layers = rust.get("layers", []), python["layers"]
    if [l["id"] for l in rust_layers] != [l["id"] for l in python_layers]:
        return found + ["layer tree"]
    for a, b in zip(rust_layers, python_layers):
        for field in a:
            if a[field] != b.get(field):
                found.append(f"{field} ({a['kind']})")
    return sorted(set(found))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("list", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--digest", required=True)
    parser.add_argument("--shard", default="0/1")
    parser.add_argument("--seconds", type=float, default=0)
    args = parser.parse_args()
    k, n = map(int, args.shard.split("/"))
    done = set()
    if args.out.exists():
        for line in args.out.read_text(encoding="utf-8").splitlines():
            try:
                done.add(json.loads(line)["key"])
            except (ValueError, KeyError):
                pass
    files = sorted((line.strip().removeprefix("./") for line in args.list.read_text(encoding="utf-8").splitlines()
                    if line.strip().endswith(".clip")), key=lambda s: s.encode())
    started = time.monotonic()
    with args.out.open("a", encoding="utf-8") as out:
        for index, relative in enumerate(files):
            key = hashlib.sha256(relative.encode()).hexdigest()[:16]
            if index % n != k or key in done:
                continue
            if args.seconds and time.monotonic() - started > args.seconds:
                break
            path = args.root / relative
            begin = time.monotonic()
            rust_line = subprocess.run([args.digest, str(path)], capture_output=True, text=True).stdout
            rust = json.loads(rust_line) if rust_line.strip() else {"error": "no output"}
            try:
                python = digest(path)
                error = None
            except Exception as exception:  # noqa: BLE001 - reported, not hidden
                python, error = None, f"{type(exception).__name__}: {exception}"
            if "error" in rust or error:
                both_failed = "error" in rust and error is not None
                record = {"ok": both_failed, "differences": ["rust error" if "error" in rust else "python error"],
                          "errors": [rust.get("error"), error]}
            else:
                found = differences(rust, python)
                record = {"ok": not found, "differences": found}
            out.write(json.dumps({"key": key, "seconds": round(time.monotonic() - begin, 2),
                                  "layers": len(rust.get("layers", [])), **record}) + "\n")
            out.flush()


if __name__ == "__main__":
    main()
