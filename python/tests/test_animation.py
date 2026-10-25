"""Tests of the animation reader (R17).

``data/anim.clip`` was made with the Rust crate (no artwork):
``cargo test -p csp-format -- --ignored write_python_fixture``. It has an
animation folder with cels "A" and "B", one 24 fps timeline and a cel track
showing A from frame 0 and B from frame 3 (motion ticks 0 and 7.5).
"""
from __future__ import annotations

import struct
import sys
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from csp_format import CspFormatError, read_clip  # noqa: E402
from csp_format.animation import (  # noqa: E402
    TRACK_CELS, Clip, Curve, Key, Node, document_bytes, keyframe_affine, parse_document, read_animation,
    unpack_chunk,
)

DATA = Path(__file__).parent / "data"


def mixer(version: str = "0110") -> Node:
    numbers = "Double[]" if version == "0110" else "Single[]"
    clip = Node("ActionNodeClip", attributes=[("Name", "")], children=[
        Node("TimeClip", children=[Node("Start", "Double", 0.0), Node("End", "Double", 90.0)]),
        Node("MotionClip", children=[Node("Start", "Double", 0.0), Node("End", "Double", 45.0)]),
        Node("AnimInfo", children=[
            Node("Param", "Int32", 0, [("Name", "Opacity")]),
            Node("FCurve", attributes=[("Type", "Opacity")], children=[
                Node("Frame", numbers, [0.0, 10.0, 20.0]),
                Node("Value", numbers, [0.0, 100.0, 50.0]),
                Node("Interp", "String[]", ["Linear", "Default"]),
            ]),
        ]),
    ])
    return Node("celsysdocument", attributes=[("version", "1.0.0")], children=[Node("General", children=[clip])])


class DocumentTest(unittest.TestCase):
    def test_round_trip_both_versions(self):
        for version in ("0100", "0110"):
            data = document_bytes(version, mixer(version))
            self.assertEqual(data[:12], b"cmt " + version.encode() + b"binc")
            again_version, root = parse_document(data)
            self.assertEqual(again_version, version)
            self.assertEqual(document_bytes(version, root), data)
            self.assertEqual(root.find("General", "ActionNodeClip", "AnimInfo", "FCurve", "Interp").value,
                             ["Linear", "Default"])

    def test_node_header_of_0110(self):
        data = document_bytes("0110", Node("a", "Int32", 7))
        # string table: 22 type names + "a"; the root node starts after it.
        self.assertEqual(struct.unpack_from("<4I", data, len(data) - 32), (16, 24, 28, 22))

    def test_checksum_and_chunk(self):
        data = bytearray(document_bytes("0100", mixer("0100")))
        data[-1] ^= 1
        with self.assertRaises(CspFormatError):
            parse_document(bytes(data))
        compressed = zlib.compress(bytes(data))
        payload = struct.pack(">Q", 4 + len(compressed)) + struct.pack("<I", len(compressed)) + compressed
        self.assertEqual(unpack_chunk(payload), bytes(data))


class CurveTest(unittest.TestCase):
    def test_interpolations(self):
        clip = Clip.from_root(mixer())
        curve = clip.curve("Opacity")
        self.assertEqual(curve.evaluate(-1), (0.0, None))
        self.assertEqual(curve.evaluate(5), (50.0, 0))           # Linear
        self.assertEqual(curve.evaluate(15)[0], 75.0)           # Default, zero slopes: smoothstep(0.5)
        self.assertAlmostEqual(curve.evaluate(12.5)[0], 100 - 50 * 0.15625)
        self.assertEqual(curve.evaluate(99), (50.0, 2))
        constant = Curve("ImageCelName", None, [Key(0, 0, interp="Constant"), Key(10, 1, interp="Constant")])
        self.assertEqual(constant.evaluate(9.9), (0, 0))

    def test_clip_maps_timeline_to_motion_ticks(self):
        clip = Clip.from_root(mixer())
        self.assertEqual(clip.motion_at(30), 15)
        self.assertEqual(clip.value("Opacity", 10), 50.0)
        self.assertIsNone(clip.value("ImageRotation", 10))

    def test_keyframe_affine(self):
        a, b, c, d, e, f = keyframe_affine((10, 10), (50, 20), (2, 2), 90)
        x, y = 11, 10   # one pixel right of the centre: rotated clockwise to below it, twice as far
        self.assertAlmostEqual(a * x + b * y + c, 50)
        self.assertAlmostEqual(d * x + e * y + f, 22)


class AnimationFileTest(unittest.TestCase):
    def test_cels_by_frame(self):
        with read_clip(DATA / "anim.clip") as clip:
            animation = read_animation(clip)
            self.assertEqual(len(animation.timelines), 1)
            timeline = animation.timelines[0]
            self.assertEqual((timeline.frame_rate, timeline.end_frame), (24.0, 6.0))
            (track,) = timeline.tracks
            folder = next(layer for layer, _ in clip.root.walk() if layer.name == "anim")
            cels = {child.name: child.main_id for child in folder.children}
            self.assertEqual((track.kind, track.layer_id), (TRACK_CELS, folder.main_id))
            for frame, name in ((0, "A"), (2, "A"), (3, "B"), (5, "B")):
                state = animation.frame_state(clip, 0, frame)
                self.assertEqual(state.cels[folder.main_id], cels[name], frame)
                self.assertEqual(state.hidden, set(cels.values()) - {cels[name]})

    def test_no_timeline(self):
        with read_clip(DATA / "layers.clip") as clip:
            self.assertIsNone(read_animation(clip))


if __name__ == "__main__":
    unittest.main()
