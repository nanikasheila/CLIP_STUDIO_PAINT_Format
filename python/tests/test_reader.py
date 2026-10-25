"""Tests of the standard-library reader against a synthetic file.

``data/layers.clip`` was made with the Rust tool (no artwork):
    cspf new f1.clip --size 300x260 --dpi 350 --image a.png
    cspf add-folder f1.clip f2.clip --name group
    cspf add-layer f2.clip b.png f3.clip --name inner --at 250,240 --parent 5
    cspf set f3.clip 4 layers.clip --opacity 50 --name "layer 1"
with ``a.png`` = :func:`disc` and ``b.png`` 40×30 of (255, 0, 0, 200).
"""
from __future__ import annotations

import sqlite3
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from csp_format import CspFormatError, Image, decode_png, parse_vector_data, read_clip, read_cmc  # noqa: E402
from csp_format import offscreen  # noqa: E402
from csp_format.text import Attributes, read_blocks, split_array  # noqa: E402

DATA = Path(__file__).parent / "data"
FIXTURE = DATA / "layers.clip"


def disc(x: int, y: int) -> tuple[int, int, int, int]:
    """Pixel of the image the fixture's first raster layer was made from."""
    if (x - 150) ** 2 + (y - 130) ** 2 < 100 ** 2:
        return x % 256, y % 256, (x + y) % 256, 255 if x < 200 else 128
    return 0, 0, 0, 0


def visible(pixel: tuple[int, int, int, int]) -> tuple[int, int, int, int]:
    """Colour does not matter where a pixel is transparent."""
    return pixel if pixel[3] else (0, 0, 0, 0)


class ClipTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.clip = read_clip(FIXTURE)

    @classmethod
    def tearDownClass(cls):
        cls.clip.close()

    def test_canvas(self):
        canvas = self.clip.canvas
        self.assertEqual((canvas.width_px, canvas.height_px, canvas.resolution, canvas.unit), (300, 260, 350.0, "px"))
        self.assertFalse(canvas.double_page)

    def test_layer_tree(self):
        tree = [(layer.name, layer.kind, depth) for layer, depth in self.clip.root.walk()]
        self.assertEqual(tree, [("", "root", 0), ("用紙", "paper", 1), ("layer 1", "raster", 1),
                                ("group", "folder", 1), ("inner", "raster", 2)])
        layer = self.clip.find_layer(4)
        self.assertEqual(layer.opacity, 0.5)
        self.assertTrue(layer.visible)
        self.assertEqual([l.main_id for l in self.clip.iter_layers()], [3, 4, 5, 6])
        self.assertEqual(self.clip.unreachable, [])
        with self.assertRaises(CspFormatError):
            self.clip.find_layer(99)

    def test_raster_pixels(self):
        image, origin = self.clip.layer_pixels(self.clip.find_layer(4))
        self.assertEqual((image.width, image.height, origin), (300, 260, (0, 0)))
        for y in range(0, 260, 7):
            for x in range(0, 300, 3):
                self.assertEqual(visible(image.pixel(x, y)), disc(x, y), (x, y))

    def test_layer_placed_off_the_corner(self):
        image, origin = self.clip.layer_pixels(self.clip.find_layer(6))
        ox, oy = origin
        for y in range(image.height):
            for x in range(image.width):
                cx, cy = x + ox, y + oy
                expected = (255, 0, 0, 200) if 250 <= cx < 290 and 240 <= cy < 270 else (0, 0, 0, 0)
                self.assertEqual(visible(image.pixel(x, y)), expected, (cx, cy))

    def test_pixels_in_box(self):
        layer = self.clip.find_layer(4)
        part = self.clip.layer_pixels_in_box(layer, (240, 100, 330, 140))
        self.assertEqual((part.width, part.height), (90, 40))
        for y in range(40):
            for x in range(90):
                expected = disc(240 + x, 100 + y) if 240 + x < 300 else (0, 0, 0, 0)
                self.assertEqual(visible(part.pixel(x, y)), expected, (x, y))
        boxes = self.clip.stored_pixel_boxes(layer)
        self.assertTrue(boxes)
        self.assertTrue(all(0 <= x0 < x1 <= 300 and 0 <= y0 < y1 <= 260 for x0, y0, x1, y1 in boxes))

    def test_nothing_where_nothing_is_stored(self):
        folder = self.clip.find_layer(5)
        self.assertIsNone(self.clip.layer_pixels(folder))
        self.assertIsNone(self.clip.layer_mask(folder))
        self.assertEqual(self.clip.vector_objects(folder), [])
        self.assertEqual(self.clip.text_blocks(folder), [])

    def test_preview_and_thumbnails(self):
        preview = self.clip.preview()
        self.assertIsNotNone(preview)
        self.assertGreater(preview.width, 0)
        thumbnail = self.clip.layer_thumbnail(self.clip.find_layer(4))
        self.assertIsNotNone(thumbnail)
        self.assertEqual(decode_png(thumbnail.to_png()), thumbnail)


def attribute(width, height, channel_order, image_channels, image_offset, image_row, alpha_row, init=None):
    """An ``Offscreen.Attribute`` as CSP lays it out."""
    def section(name, values):
        body = struct.pack(">I", len(name)) + name.encode("utf-16-be") + struct.pack(f">{len(values)}I", *values)
        return body
    blocks = (-(-width // 256), -(-height // 256))
    parameter = [width, height, *blocks, channel_order, 1, image_channels, image_channels + 1, image_offset,
                 image_channels, image_row, 1, alpha_row, 0, 256, 256, 0, 0]
    sections = [section("Parameter", parameter),
                section("InitColor", [20, 1 if init is not None else 0, init or 0, 0, 4]),
                section("BlockSize", [12, blocks[0] * blocks[1], 4])]
    return struct.pack(">IIII", 16, *map(len, sections)) + b"".join(sections)


def block_data(raws):
    def name(text):
        return struct.pack(">I", len(text)) + text.encode("utf-16-be")
    records = b""
    for index, raw in enumerate(raws):
        body = name("BlockDataBeginChunk") + struct.pack(">5I", index, len(raw or b""), 256, 256, int(raw is not None))
        if raw is not None:
            packed = zlib.compress(raw)
            body += struct.pack(">I", len(packed) + 4) + struct.pack("<I", len(packed)) + packed
        body += name("BlockDataEndChunk")
        records += struct.pack(">I", len(body) + 4) + body
    return struct.pack(">Q", len(records)) + records


class OffscreenTest(unittest.TestCase):
    def test_monochrome_bits(self):
        alpha = bytearray(32 * 256)
        image = bytearray(32 * 256)
        alpha[0] = 0b1010_0000  # pixels 0 and 2 drawn
        image[0] = 0b1000_0000  # pixel 0 white, pixel 2 black
        attr = attribute(10, 5, 0x11, 1, 32 * 256, 32, 32)
        decoded = offscreen.decode(attr, block_data([bytes(alpha) + bytes(image)]))
        self.assertEqual([decoded.pixel(x, 0) for x in range(3)], [(255, 255, 255, 255), (0, 0, 0, 0), (0, 0, 0, 255)])

    def test_mask_with_init_colour(self):
        attr = attribute(300, 10, 0x01, 0, 0, 0, 256, init=255)
        raw = bytearray(256 * 256)
        raw[0] = 7
        decoded = offscreen.decode(attr, block_data([bytes(raw), None]), mask=True)
        self.assertEqual(decoded.pixel(0, 0), (7, 7, 7, 255))
        self.assertEqual(decoded.pixel(1, 0), (0, 0, 0, 255))  # stored block: its own values
        self.assertEqual(decoded.pixel(299, 9), (255, 255, 255, 255))  # not stored: the init colour

    def test_block_count_must_match_the_grid(self):
        attr = attribute(300, 10, 0x01, 0, 0, 0, 256)
        with self.assertRaises(CspFormatError):
            offscreen.decode(attr, block_data([None]))

    def test_no_block_data(self):
        self.assertIsNone(offscreen.decode(attribute(5, 5, 0x01, 0, 0, 0, 256), None))


def vector_object(points):
    header = bytearray(88)
    struct.pack_into(">5I", header, 0, 88, 72, 88, 88, len(points))
    struct.pack_into(">I", header, 20, 0x1000)
    struct.pack_into(">4i", header, 24, 1, 2, 30, 40)
    struct.pack_into(">3I", header, 40, 0xFF000000, 0x80000000, 0)
    struct.pack_into(">d", header, 64, 1.0)
    struct.pack_into(">I", header, 72, 9)
    struct.pack_into(">d", header, 76, 10.0)
    raw_points = b""
    for x, y, pressure in points:
        point = bytearray(88)
        struct.pack_into(">dd", point, 0, x, y)
        struct.pack_into(">f", point, 36, pressure)
        raw_points += bytes(point)
    return bytes(header) + raw_points


class VectorTextTest(unittest.TestCase):
    def test_vector_data(self):
        body = vector_object([(1.0, 2.0, 0.5), (3.5, 4.5, 1.0)])
        (item,) = parse_vector_data(struct.pack(">II", 0, len(body)) + body)
        self.assertEqual(item.coordinates, [(1.0, 2.0), (3.5, 4.5)])
        self.assertEqual((item.brush_size, item.brush_styles, item.bbox), (10.0, [9], (1, 2, 30, 40)))
        self.assertEqual(item.main_color, (255, 128, 0))
        self.assertTrue(item.is_closed)
        self.assertEqual(item.points[0].pressure, 0.5)
        with self.assertRaises(CspFormatError):
            parse_vector_data(struct.pack(">II", 0, len(body)) + body[:-3])

    def test_text_blocks(self):
        font = struct.pack("<HH", 1, 3) + b"abc" + struct.pack("<H", 2) + b"PS"
        records = [(32, struct.pack("<i", 600)), (42, struct.pack("<4i", 1, 2, 30, 40)), (57, font), (99, b"\1\2")]
        blob = b"".join(struct.pack("<II", rid, len(data)) + data for rid, data in records)
        attributes = Attributes.parse(blob)
        self.assertEqual((attributes.font_size, attributes.bbox), (6.0, (1, 2, 30, 40)))
        self.assertEqual((attributes.font.name, attributes.font.postscript_name), ("abc", "PS"))
        with self.assertRaises(CspFormatError):
            Attributes.parse(blob[:-1])
        array = b"".join(struct.pack("<I", len(part)) + part for part in (blob, b""))
        self.assertEqual(split_array(array), [blob, b""])
        connection = sqlite3.connect(":memory:")
        connection.row_factory = sqlite3.Row
        row = connection.execute("SELECT ? AS TextLayerString, ? AS TextLayerAttributes",
                                 ("台詞".encode(), blob)).fetchone()
        (block,) = read_blocks(row)
        self.assertEqual((block.text, block.attributes.font_size), ("台詞", 6.0))
        # The arrays hold the other blocks (balloons), after the single one.
        row = connection.execute(
            "SELECT ? AS TextLayerString, ? AS TextLayerAttributes, ? AS TextLayerStringArray, "
            "? AS TextLayerAttributesArray",
            (b"one", blob, struct.pack("<I", 3) + b"two", struct.pack("<I", 0))).fetchone()
        blocks = read_blocks(row)
        self.assertEqual([b.text for b in blocks], ["one", "two"])
        self.assertEqual((blocks[0].attributes.font_size, blocks[1].attributes.records), (6.0, ()))


class CmcTest(unittest.TestCase):
    def test_pages_in_list_order(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            (folder / "page0002.clip").write_bytes(FIXTURE.read_bytes())
            connection = sqlite3.connect(folder / "work.cmc")
            connection.executescript("""
                CREATE TABLE Project(MainId INTEGER, ProjectInternalVersion TEXT, ProjectRootCanvasNode INTEGER,
                                     ComicStartPage INTEGER, MaxCanvasFileIndex INTEGER);
                INSERT INTO Project VALUES (1, '1.1.0', 1, 1, 2);
                CREATE TABLE CanvasNode(MainId INTEGER, Type INTEGER, NextIndex INTEGER, FirstChildIndex INTEGER,
                                        SelectedIndex INTEGER, PageFlag INTEGER, LinkPath TEXT);
                INSERT INTO CanvasNode VALUES (1, 0, 0, 3, 3, 0, NULL), (2, 2, 0, 0, 0, 1, '.:page0001.clip'),
                                              (3, 2, 2, 0, 0, 0, '.:page0002.clip');
            """)
            connection.commit()
            connection.close()
            with read_cmc(folder / "work.cmc") as work:
                self.assertEqual([page.main_id for page in work.pages()], [3, 2])
                self.assertEqual([p.name for p in work.page_file_paths()], ["page0002.clip", "page0001.clip"])
                self.assertTrue(work.pages()[1].spread)
                first, second = work.pages()
                self.assertIsInstance(work.page_thumbnail(first), Image)
                self.assertIsNone(work.page_thumbnail(second))  # its file is missing

    def test_not_a_cmc(self):
        with self.assertRaises(CspFormatError):
            from csp_format import CmcFile
            CmcFile(b"CSFCHUNK")


if __name__ == "__main__":
    unittest.main()
