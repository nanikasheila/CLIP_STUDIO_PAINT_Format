"""Animation: timelines, tracks, keys and their evaluation (R17; docs/spec/08-animation.md).

    from csp_format import read_clip
    from csp_format.animation import read_animation
    with read_clip("anim.clip") as clip:
        animation = read_animation(clip)            # None without timelines
        state = animation.frame_state(clip, animation.current, frame=0)
        state.cels        # {folder id: shown cel layer id or None}
        state.transforms  # {layer id: (affine, opacity 0-256 or None)}

The same rules as the Rust crate (``crates/csp-format/src/animation.rs``):
a frame ``f`` is at ``t = f * 60 / FrameRate`` ticks, each track's clip maps
``t`` to motion ticks ``m = MS + (t - TS) * (ME - MS) / (TE - TS)``, and keys
interpolate ``Constant`` (hold), ``Linear`` or ``Default`` (cubic Hermite
through the slopes, smoothstep when they are 0). Track keys are stored in
"cmt " documents (:func:`parse_document`), which :func:`document_bytes`
writes back byte for byte.
"""
from __future__ import annotations

import math
import struct
import zlib
from dataclasses import dataclass, field

from .clip import _PER_INCH, UNITS
from .errors import CspFormatError

TYPES_0100 = ("null", "Byte", "SByte", "UInt16", "Int16", "UInt32", "Int32", "Single", "Double", "String", "Float2",
              "Float3", "Quat", "Matrix44", "Single[]", "Byte[]", "Int32[]", "String[]", "Float2[]", "Float3[]",
              "Quat[]", "Matrix44[]")
TYPES_0110 = tuple({"Float2": "Double2", "Float3": "Double3", "Single[]": "Double[]", "Float2[]": "Double2[]",
                    "Float3[]": "Double3[]"}.get(t, t) for t in TYPES_0100)
# struct format of each value type (arrays: of one element).
_FORMATS = {"Byte": "B", "SByte": "b", "UInt16": "H", "Int16": "h", "UInt32": "I", "Int32": "i", "Single": "f",
            "Double": "d", "String": "I", "Float2": "2f", "Float3": "3f", "Double2": "2d", "Double3": "3d"}
TRACK_CELS, TRACK_KEYFRAMES, TRACK_PAPER, TRACK_CAMERA = 2000, 2001, 2003, 2005
LAYER_TYPE_LIGHT_TABLE = 10016


@dataclass
class Node:
    """A node of a "cmt " document. ``kind`` is the type name (``"Double[]"``, …)."""

    name: str
    kind: str = "null"
    value: object = None
    attributes: list[tuple[str, str]] = field(default_factory=list)
    children: list["Node"] = field(default_factory=list)

    def attribute(self, name: str) -> str | None:
        return next((v for k, v in self.attributes if k == name), None)

    def child(self, name: str) -> "Node | None":
        return next((c for c in self.children if c.name == name), None)

    def find(self, *path: str) -> "Node | None":
        node: Node | None = self
        for name in path:
            node = node.child(name) if node else None
        return node


def _types(version: str) -> tuple[str, ...]:
    return TYPES_0110 if version == "0110" else TYPES_0100


def parse_document(data: bytes) -> tuple[str, Node]:
    """``(version, root)`` of a "cmt " document (decompressed)."""
    if data[:4] != b"cmt " or data[8:12] != b"binc":
        raise CspFormatError("not a binary cmt document")
    version = data[4:8].decode("ascii", "replace")
    if version not in ("0100", "0110"):
        raise CspFormatError(f"unknown cmt version {version!r}")
    if struct.unpack_from("<I", data, 12)[0] != zlib.crc32(data[16:]) & 0xFFFFFFFF:
        raise CspFormatError("cmt checksum does not match")
    (count,), position = struct.unpack_from("<I", data, 16), 20
    strings = []
    for _ in range(count):
        length = data[position]
        strings.append(data[position + 1:position + 1 + length].decode("utf-8"))
        position += 1 + length
    types = _types(version)
    if tuple(strings[:22]) != types:
        raise CspFormatError("cmt string table does not start with the type names")

    def read(fmt: str) -> tuple:
        nonlocal position
        values = struct.unpack_from("<" + fmt, data, position)
        position += struct.calcsize("<" + fmt)
        return values

    def value(kind: str) -> object:
        if kind == "null":
            return None
        base, array = (kind[:-2], True) if kind.endswith("[]") else (kind, False)
        if base not in _FORMATS:
            raise CspFormatError(f"cmt value type {kind} is not supported")
        fmt = _FORMATS[base]
        one = (lambda: strings[read(fmt)[0]]) if base == "String" else (
            (lambda: read(fmt)[0]) if len(fmt) == 1 else (lambda: read(fmt)))
        if not array:
            return one()
        (n,) = read("I")
        if n * struct.calcsize("<" + fmt) > len(data) - position:
            raise CspFormatError("cmt array does not fit")
        return [one() for _ in range(n)]

    def node(depth: int) -> Node:
        if depth > 64:
            raise CspFormatError("cmt tree too deep")
        start = position
        offsets = read("3I") if version == "0110" else None
        name_index, type_index = read("2I")
        kind = types[type_index]
        result = Node(strings[name_index], kind, value(kind))
        if offsets and position != start + offsets[1]:
            raise CspFormatError(f"cmt node {result.name}: attribute offset")
        for _ in range(read("I")[0]):
            k, v = read("2I")
            result.attributes.append((strings[k], strings[v]))
        if offsets and position != start + offsets[2]:
            raise CspFormatError(f"cmt node {result.name}: child offset")
        result.children = [node(depth + 1) for _ in range(read("I")[0])]
        return result

    root = node(0)
    if position != len(data):
        raise CspFormatError("bytes after the cmt root node")
    return version, root


def document_bytes(version: str, root: Node) -> bytes:
    """Write a "cmt " document (string table in CSP's first-use order)."""
    types = _types(version)
    strings = list(types)
    index: dict[str, int] = {}

    def add(text: str) -> None:
        if text not in index and text not in strings[:22]:
            index[text] = len(strings)
            strings.append(text)

    def collect(node: Node) -> None:
        add(node.name)
        if node.kind == "String":
            add(node.value)  # type: ignore[arg-type]
        elif node.kind == "String[]":
            for text in node.value:  # type: ignore[union-attr]
                add(text)
        for k, v in node.attributes:
            add(k)
            add(v)
        for child in node.children:
            collect(child)

    collect(root)
    position_of = {text: i for i, text in reversed(list(enumerate(strings)))}

    def pack_value(node: Node) -> bytes:
        if node.kind == "null":
            return b""
        base, array = (node.kind[:-2], True) if node.kind.endswith("[]") else (node.kind, False)
        fmt = "<" + _FORMATS[base]

        def one(v: object) -> bytes:
            if base == "String":
                return struct.pack(fmt, position_of[v])  # type: ignore[index]
            return struct.pack(fmt, *(v if isinstance(v, (tuple, list)) else (v,)))

        if not array:
            return one(node.value)
        return struct.pack("<I", len(node.value)) + b"".join(one(v) for v in node.value)  # type: ignore

    def pack_node(node: Node) -> bytes:
        body = struct.pack("<2I", position_of[node.name], types.index(node.kind)) + pack_value(node)
        attributes = struct.pack("<I", len(node.attributes)) + b"".join(
            struct.pack("<2I", position_of[k], position_of[v]) for k, v in node.attributes)
        children = struct.pack("<I", len(node.children)) + b"".join(pack_node(c) for c in node.children)
        if version == "0110":
            header = struct.pack("<3I", 16, 12 + len(body), 12 + len(body) + len(attributes))
            return header + body + attributes + children
        return body + attributes + children

    table = struct.pack("<I", len(strings)) + b"".join(
        bytes([len(s.encode("utf-8"))]) + s.encode("utf-8") for s in strings)
    rest = table + pack_node(root)
    return b"cmt " + version.encode("ascii") + b"binc" + struct.pack("<I", zlib.crc32(rest) & 0xFFFFFFFF) + rest


def unpack_chunk(payload: bytes) -> bytes:
    """A mixer's external chunk payload: u64 BE rest, u32 LE zlib length, zlib."""
    if len(payload) < 12 or struct.unpack_from(">Q", payload)[0] != len(payload) - 8:
        raise CspFormatError("cmt chunk lengths do not match")
    return zlib.decompress(payload[12:])


# -- keys -------------------------------------------------------------------


@dataclass
class Key:
    frame: float
    value: float
    left_slope: float = 0.0
    right_slope: float = 0.0
    interp: str = "Default"
    tag: str | None = None


@dataclass
class Curve:
    kind: str
    axis: str | None
    keys: list[Key]

    def evaluate(self, m: float) -> tuple[float, int | None] | None:
        """``(value, index of the key in effect or None before the first)``."""
        if not self.keys:
            return None
        if m < self.keys[0].frame:
            return self.keys[0].value, None
        i = max(j for j, key in enumerate(self.keys) if key.frame <= m)
        key = self.keys[i]
        if i + 1 == len(self.keys):
            return key.value, i
        nxt = self.keys[i + 1]
        span = nxt.frame - key.frame
        if span <= 0 or key.interp == "Constant":
            return key.value, i
        u = (m - key.frame) / span
        if key.interp == "Default":
            h00, h10 = 2 * u ** 3 - 3 * u ** 2 + 1, u ** 3 - 2 * u ** 2 + u
            h01, h11 = -2 * u ** 3 + 3 * u ** 2, u ** 3 - u ** 2
            return (h00 * key.value + h10 * span * key.right_slope + h01 * nxt.value
                    + h11 * span * nxt.left_slope), i
        return key.value + (nxt.value - key.value) * u, i


@dataclass
class Clip:
    time: tuple[float, float]
    motion: tuple[float, float]
    params: dict[str, object]
    curves: list[Curve]

    @classmethod
    def from_root(cls, root: Node) -> "Clip":
        clip = root.find("General", "ActionNodeClip")
        if clip is None:
            raise CspFormatError("track mixer has no ActionNodeClip")

        def number(*path: str) -> float:
            node = clip.find(*path)
            return float(node.value) if node is not None and isinstance(node.value, (int, float)) else 0.0

        params: dict[str, object] = {}
        curves = []
        info = clip.child("AnimInfo")
        for node in info.children if info else []:
            if node.name == "Param":
                params[node.attribute("Name") or ""] = node.value
            elif node.name == "FCurve":
                get = {c.name: c.value for c in node.children}
                if not isinstance(get.get("Frame"), list) or not isinstance(get.get("Value"), list) \
                        or len(get["Frame"]) != len(get["Value"]):
                    raise CspFormatError(f"FCurve {node.attribute('Type')}: Frame and Value do not match")
                n = len(get["Frame"])

                def pick(name: str, i: int, default):
                    values = get.get(name)
                    return values[i] if isinstance(values, list) and i < len(values) else default

                curves.append(Curve(node.attribute("Type") or "", node.attribute("Axis"), [
                    Key(get["Frame"][i], get["Value"][i], pick("LeftSlope", i, 0.0), pick("RightSlope", i, 0.0),
                        pick("Interp", i, "Default"), pick("Tag", i, None)) for i in range(n)]))
        return cls((number("TimeClip", "Start"), number("TimeClip", "End")),
                   (number("MotionClip", "Start"), number("MotionClip", "End")), params, curves)

    def motion_at(self, t: float) -> float:
        (ts, te), (ms, me) = self.time, self.motion
        return ms if te == ts else ms + (t - ts) * (me - ms) / (te - ts)

    def curve(self, kind: str, axis: str | None = None) -> Curve | None:
        return next((c for c in self.curves if c.kind == kind and c.axis == axis), None)

    def value(self, kind: str, t: float, axis: str | None = None) -> float | None:
        curve = self.curve(kind, axis)
        found = curve.evaluate(self.motion_at(t)) if curve else None
        if found is not None:
            return found[0]
        param = self.params.get(kind)
        if isinstance(param, (tuple, list)):
            return float(param[0 if axis == "X" else 1])
        return float(param) if isinstance(param, (int, float)) else None

    def pair(self, kind: str, t: float, default: tuple[float, float]) -> tuple[float, float]:
        x, y = self.value(kind, t, "X"), self.value(kind, t, "Y")
        return (x, y) if x is not None and y is not None else default


# -- model ------------------------------------------------------------------


@dataclass
class Track:
    main_id: int
    timeline: int
    kind: int
    layer_uuid: str
    layer_id: int | None
    clip: Clip | None


@dataclass
class Label:
    main_id: int
    name: str
    frame: float
    kind: int
    length: float | None


@dataclass
class Timeline:
    main_id: int
    name: str
    frame_rate: float
    start_frame: float
    end_frame: float
    current_frame: float
    labels: list[Label]
    tracks: list[Track]

    def ticks(self, frame: float) -> float:
        return frame * 60.0 / max(self.frame_rate, 1e-9)


Affine = tuple[float, float, float, float, float, float]


def keyframe_affine(center, position, scale, rotation) -> Affine:
    """``p -> position + R(rotation) * diag(scale) * (p - center)``, rotation in degrees, clockwise."""
    s, c = math.sin(math.radians(rotation)), math.cos(math.radians(rotation))
    a, b, d, e = c * scale[0], -s * scale[1], s * scale[0], c * scale[1]
    return a, b, position[0] - a * center[0] - b * center[1], d, e, position[1] - d * center[0] - e * center[1]


@dataclass
class FrameState:
    hidden: set[int] = field(default_factory=set)
    #: layer id -> (original canvas position -> canvas position, opacity 0-256 or None)
    transforms: dict[int, tuple[Affine, int | None]] = field(default_factory=dict)
    #: camera folder id -> (output canvas position -> canvas position shown there)
    cameras: dict[int, Affine] = field(default_factory=dict)
    cels: dict[int, int | None] = field(default_factory=dict)


@dataclass
class Animation:
    timelines: list[Timeline]
    current: int
    #: reference frame (x, y, width, height) in canvas pixels
    frame: tuple[float, float, float, float]

    def frame_state(self, clip, index: int, frame: float) -> FrameState:
        timeline = self.timelines[index]
        t = timeline.ticks(frame)
        tracks = {track.layer_id: track for track in timeline.tracks if track.layer_id is not None}
        state = FrameState()
        fx, fy, fw, fh = self.frame
        for layer, _ in clip.root.walk():
            if layer.layer_type == LAYER_TYPE_LIGHT_TABLE:
                state.hidden.add(layer.main_id)
                continue
            row = clip.layer_row(layer.main_id)
            track = tracks.get(layer.main_id)
            is_folder = "AnimationFolder" in row.keys() and row["AnimationFolder"]
            if track is None or track.clip is None:
                if track is None and is_folder:
                    state.hidden.update(child.main_id for child in layer.children)
                    state.cels[layer.main_id] = None
                continue
            c = track.clip
            if track.kind == TRACK_CELS:
                curve = c.curve("ImageCelName")
                found = curve.evaluate(c.motion_at(t)) if curve else None
                name = curve.keys[found[1]].tag if found and found[1] is not None else None
                shown = next((ch.main_id for ch in reversed(layer.children) if ch.name == name), None)
                state.hidden.update(ch.main_id for ch in layer.children if ch.main_id != shown)
                state.cels[layer.main_id] = shown
            elif track.kind == TRACK_KEYFRAMES:
                if "TimeLineLayerKeyFrameEnabled" not in row.keys() or not row["TimeLineLayerKeyFrameEnabled"]:
                    continue
                if t < c.time[0] or t >= c.time[1]:
                    state.hidden.add(layer.main_id)
                    continue
                center = c.pair("ImageCenter", t, (0.0, 0.0))
                scale = c.pair("ImageAspectScale", t, (100.0, 100.0))
                affine = keyframe_affine(center, c.pair("ImagePosition", t, center),
                                         (scale[0] / 100, scale[1] / 100), c.value("ImageRotation", t) or 0.0)
                opacity = c.value("Opacity", t)
                state.transforms[layer.main_id] = (affine, None if opacity is None else _opacity(opacity))
            elif track.kind == TRACK_CAMERA:
                center = c.pair("ImageCenter", t, (fw / 2, fh / 2))
                position = c.pair("ImagePosition", t, (fx + center[0], fy + center[1]))
                scale = (c.value("ImageScale", t) or 100.0) / 100
                a, b, cc, d, e, f = keyframe_affine(center, position, (scale, scale), c.value("ImageRotation", t) or 0.0)
                state.cameras[layer.main_id] = (a, b, cc - a * fx - b * fy, d, e, f - d * fx - e * fy)
                opacity = c.value("Opacity", t)
                if opacity is not None:
                    state.transforms[layer.main_id] = ((1.0, 0.0, 0.0, 0.0, 1.0, 0.0), _opacity(opacity))
        return state


def _opacity(percent: float) -> int:
    """CSP applies a keyed opacity as ``round(v * 256 / 100) / 256``."""
    return max(0, min(256, math.floor(percent * 256 / 100 + 0.5)))


def _uuid(value) -> str:
    if isinstance(value, (bytes, bytearray)):
        return bytes(value).hex()
    return str(value or "").replace("-", "").lower()


def _frame(clip) -> tuple[float, float, float, float]:
    """The reference frame: the basic frame (基本枠) centred in the trim frame (仕上がり), in canvas pixels."""
    row = clip.database.execute("SELECT * FROM Canvas LIMIT 1").fetchone()
    per_inch = _PER_INCH.get(UNITS.get(row["CropFrameUnitKind"] if "CropFrameUnitKind" in row.keys() else 0, "px"))

    def px(column: str) -> float:
        value = float(row[column] or 0.0) if column in row.keys() else 0.0
        return value if per_inch is None else value * clip.canvas.resolution / per_inch

    cw, ch = float(clip.canvas.width_px), float(clip.canvas.height_px)
    crop = (px("CropFrameWidth"), px("CropFrameHeight"))
    if crop[0] <= 0 or crop[1] <= 0:
        return 0.0, 0.0, cw, ch
    inner = (px("CropFrameInnerWidth"), px("CropFrameInnerHeight"))
    w, h = inner if inner[0] > 0 and inner[1] > 0 else crop
    x = (cw - crop[0]) / 2 + px("CropFrameCropOffsetX") + (crop[0] - w) / 2 + px("CropFrameInnerOffsetX")
    y = (ch - crop[1]) / 2 + px("CropFrameCropOffsetY") + (crop[1] - h) / 2 + px("CropFrameInnerOffsetY")
    return x, y, w, h


def read_animation(clip) -> Animation | None:
    """The animation of an opened :class:`~csp_format.ClipFile`, None without timelines."""
    if not clip.has_table("TimeLine"):
        return None
    db = clip.database
    timelines_by_id = {row["MainId"]: row for row in db.execute("SELECT * FROM TimeLine")}
    if not timelines_by_id:
        return None
    tracks_by_id = {row["MainId"]: row for row in db.execute("SELECT * FROM Track")} if clip.has_table("Track") else {}
    labels_by_id = ({row["MainId"]: row for row in db.execute("SELECT * FROM TimeLineLabel")}
                    if clip.has_table("TimeLineLabel") else {})
    bank = db.execute("SELECT * FROM AnimationCutBank LIMIT 1").fetchone() if clip.has_table("AnimationCutBank") else None
    layers = {}
    for layer, _ in clip.root.walk():
        uuid = clip.layer_row(layer.main_id)["LayerUuid"]
        if uuid:
            layers[_uuid(uuid)] = layer.main_id

    def chain(first: int, rows: dict, next_column: str) -> list:
        out, seen = [], set()
        while first:
            if first in seen or first not in rows:
                raise CspFormatError(f"broken {next_column} list at {first}")
            seen.add(first)
            out.append(rows[first])
            first = rows[first][next_column] or 0
        return out

    def labels(first: int) -> list[Label]:
        return [Label(r["MainId"], r["LabelName"] or "", r["LabelFrame"] or 0.0, r["LabelType"] or 0,
                      r["LabelLength"] if "LabelLength" in r.keys() else None)
                for r in chain(first or 0, labels_by_id, "LabelNextIndex")]

    first = bank["FirstTimeLine"] if bank is not None and bank["FirstTimeLine"] else min(
        set(timelines_by_id) - {r["NextTimeLine"] for r in timelines_by_id.values()}, default=0)
    timelines = []
    for row in chain(first, timelines_by_id, "NextTimeLine"):
        tracks = []
        for track in chain(row["FirstTrack"] or 0, tracks_by_id, "TrackNextIndex"):
            mixer = None
            for column in ("TrackActionMixer2", "TrackActionMixer"):
                ident = track[column]
                payload = clip.container.external(ident.decode() if isinstance(ident, bytes) else ident) if ident else None
                if payload is not None:
                    mixer = Clip.from_root(parse_document(unpack_chunk(payload))[1])
                    break
            uuid = _uuid(track["LayerUuidWithTrack"])
            tracks.append(Track(track["MainId"], track["ItemId"], track["TrackKind"], uuid, layers.get(uuid), mixer))
        timelines.append(Timeline(row["MainId"], row["TimeLineName"] or "", row["FrameRate"] or 0.0,
                                  row["StartFrame"] or 0.0, row["EndFrame"] or 0.0, row["CurrentFrame"] or 0.0,
                                  labels(row["LabelFirstIndex"]), tracks))
    current = min(max(int(bank["CurrentIndex"] or 0) if bank is not None else 0, 0), len(timelines) - 1)
    return Animation(timelines, current, _frame(clip))
