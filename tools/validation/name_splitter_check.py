"""V7: can the Python reader replace csp-name-splitter's reader?

For each .clip, reads it with ``name_splitter.csp.clip_reader`` (needs
Pillow and a csp-name-splitter checkout) and with ``python/csp_format``,
and compares what name-splitter uses: canvas sizes and frames, the layer
tree (ids, names, visibility, opacity, offsets, masks), stored pixels,
vector payloads and text. Writes one JSON line per file with the kinds of
differences only (no names, text, paths or pixels).

    python tools/validation/name_splitter_check.py ROOT LIST.txt OUT.jsonl \
        --name-splitter PATH [--shard K/N] [--seconds S]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))

from csp_format import read_clip  # noqa: E402

# name-splitter's kinds; it calls the layers it does not know "other".
SPLITTER_KINDS = {"root", "paper", "story_info", "raster", "folder", "frame_folder", "text", "vector"}


def normalized(image):
    """RGBA with the colour of transparent pixels cleared (it is not meaningful)."""
    from PIL import Image
    alpha = image.getchannel("A")
    clear = Image.new("RGBA", image.size, (0, 0, 0, 0))
    return Image.composite(image, clear, alpha.point(lambda a: 255 if a else 0))


def same_pixels(ours, theirs) -> bool:
    from PIL import Image, ImageChops
    if ours is None or theirs is None:
        return ours is None and theirs is None
    mine = Image.frombytes("RGBA", (ours.width, ours.height), bytes(ours.rgba))
    if mine.size != theirs.size:
        return False
    return ImageChops.difference(normalized(mine), normalized(theirs.convert("RGBA"))).getbbox() is None


def compare(path: Path, clip_reader) -> tuple[list[str], dict]:
    found, notes = [], {}
    theirs = clip_reader.read_clip(path)
    with read_clip(path) as ours:
        a, b = ours.canvas, theirs.canvas
        for field in ("width_px", "height_px", "crop_size_px", "basic_size_px", "basic_offset_px"):
            if getattr(a, field) != getattr(b, field):
                found.append(f"canvas.{field}")
        if a.double_page != b.is_double_page or a.single_page_width_px != b.single_page_width_px:
            found.append("canvas.double_page")
        if ours.preview_png() != theirs.preview_png:
            found.append("preview")
        mine, other = ours.iter_layers(), theirs.iter_layers()
        if [l.main_id for l in mine] != [l.main_id for l in other]:
            return found + ["layer tree"], notes
        kinds: dict[str, int] = {}
        for x, y in zip(mine, other):
            if x.kind != y.kind:
                key = f"{y.kind}->{x.kind}"
                kinds[key] = kinds.get(key, 0) + 1
                if x.kind in SPLITTER_KINDS and y.kind != "other":
                    found.append(f"kind {key}")
            checks = {
                "name": x.name == y.name,
                "visible": x.visible == y.visible,
                "opacity": abs(x.opacity - y.opacity) < 1e-9 or y.opacity > 1,
                "offset": x.offset == (y.offset_x, y.offset_y) and x.pixel_origin == y.pixel_origin,
                "mask": x.mask_mipmap == y.mask_mipmap and x.has_mask == y.has_mask,
                "clipping": x.clipping == y.clipping and x.locked == y.locked,
                "blend": x.blend == y.composite,
            }
            found += [f"layer.{name}" for name, ok in checks.items() if not ok]
            if x.kind in ("vector", "text") and y.kind in ("vector", "text"):
                if ours.vector_payloads(x) != theirs.vector_payloads(y):
                    found.append("vector payloads")
            if x.kind == "text" and y.kind == "text":
                if (x.text or "") != (y.text or "") or len(ours.text_blocks(x)) != len(theirs.text_blocks(y)):
                    found.append("text")
            if x.render_mipmap:
                try:
                    theirs_pixels = theirs.render_layer_pixels(y)
                except Exception as error:  # noqa: BLE001 - name-splitter does not know every layout
                    notes.setdefault("splitter errors", []).append(type(error).__name__)
                    continue
                found_pixels = ours.layer_pixels(x)
                if not same_pixels(found_pixels[0] if found_pixels else None, theirs_pixels):
                    found.append(f"pixels ({x.kind})")
        notes["kinds"] = kinds
    return sorted(set(found)), notes


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("list", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--name-splitter", required=True, type=Path)
    parser.add_argument("--shard", default="0/1")
    parser.add_argument("--seconds", type=float, default=0)
    args = parser.parse_args()
    sys.path.insert(0, str(args.name_splitter))
    from name_splitter.csp import clip_reader
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
            begin = time.monotonic()
            try:
                found, notes = compare(args.root / relative, clip_reader)
                record = {"ok": not found, "differences": found, **notes}
            except Exception as error:  # noqa: BLE001 - reported per file
                record = {"ok": False, "differences": ["error"], "error": f"{type(error).__name__}: {error}"}
            out.write(json.dumps({"key": key, "seconds": round(time.monotonic() - begin, 2), **record}) + "\n")
            out.flush()


if __name__ == "__main__":
    main()
