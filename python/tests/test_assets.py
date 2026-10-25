"""Tests of the standard-library asset reader on synthetic data (no CSP files)."""
from __future__ import annotations

import sqlite3
import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from csp_format import CspFormatError  # noqa: E402
from csp_format.assets import c2f, colorset, material, serial, shortcut, subtool, tar  # noqa: E402


def action_set() -> serial.Document:
    item = serial.Object([("typename", serial.Value(serial.STRING, "typeActionItem"))], [
        ("actionItemEnable", serial.Value(serial.BOOL, b"\x01")),
        ("actionItemKind", serial.Value(serial.STRING, "layeractionrasterdetailnew")),
        ("offset", serial.Value(serial.INT, b"\xff\xff\xff\xf6")),
        ("scale", serial.Value(serial.REAL, 0.5)),
        ("data", serial.Value(serial.BYTES, b"\x00\x01")),
    ])
    items = serial.Array([("typename", serial.Value(serial.STRING, ""))], [item])
    root = serial.Object([("typename", serial.Value(serial.STRING, "typeActionSet"))], [
        ("actionSetName", serial.Value(serial.STRING, "テスト")), ("actionArray", items)])
    return serial.Document(root)


def points(*pairs) -> bytes:
    return struct.pack(">III", 12, len(pairs), 16) + b"".join(struct.pack(">dd", *p) for p in pairs)


def to_bytes(c: sqlite3.Connection) -> bytes:
    if hasattr(c, "serialize"):
        return c.serialize()
    import tempfile
    import os
    handle, name = tempfile.mkstemp()
    os.close(handle)
    disk = sqlite3.connect(name)
    c.backup(disk)
    disk.close()
    data = Path(name).read_bytes()
    os.unlink(name)
    return data


def tool_db() -> bytes:
    c = sqlite3.connect(":memory:")
    c.executescript(
        "CREATE TABLE Manager(_PW_ID INTEGER PRIMARY KEY, RootUuid BLOB, CommonVariantID INTEGER);"
        "CREATE TABLE Node(_PW_ID INTEGER PRIMARY KEY, NodeUuid BLOB, NodeName TEXT, NodeInputOp INTEGER,"
        " NodeOutputOp INTEGER, NodeVariantID INTEGER, NodeInitVariantID INTEGER, NodeFirstChildUuid BLOB,"
        " NodeNextUuid BLOB);"
        "CREATE TABLE Variant(_PW_ID INTEGER PRIMARY KEY, VariantID INTEGER, BrushSize REAL,"
        " BrushSizeEffector BLOB, BrushPatternImageArray BLOB, VariantShowParam BLOB);")
    c.execute("INSERT INTO Manager(RootUuid, CommonVariantID) VALUES (?, 0)", (b"\x01" * 16,))
    none = b"\0"
    for uuid, name, op, variant, child, nxt in [
        (b"\x01" * 16, "", 0, 0, b"\x02" * 16, none),
        (b"\x02" * 16, "ペン", 0, 0, b"\x03" * 16, none),
        (b"\x03" * 16, "G-ペン", 10, 1, none, b"\x04" * 16),
        (b"\x04" * 16, "消しゴム", 12, 2, none, none),
    ]:
        c.execute("INSERT INTO Node(NodeUuid, NodeName, NodeInputOp, NodeOutputOp, NodeVariantID, NodeInitVariantID,"
                  " NodeFirstChildUuid, NodeNextUuid) VALUES (?, ?, 1, ?, ?, 0, ?, ?)",
                  (uuid, name, op, variant, child, nxt))
    graph = points((0.0, 0.0), (1.0, 1.0))
    effector = struct.pack(">11I", 44, 0x1F0, 0x10, 20, 0, 0, 0, 0, len(graph), 0, 100) + graph
    path = ".:56:16:0a1b:data:material_0.layer\0".encode("utf-16-le")
    name = "丸".encode("utf-16-le")
    ref = struct.pack(">I", len(path)) + path + struct.pack(">I", 2) + struct.pack(">I", len(name)) + name
    ref += struct.pack(">I", 1)
    materials = struct.pack(">II", 8, 1) + struct.pack(">I", len(ref) + 4) + ref
    show = struct.pack(">III", 12, 2, 4) + struct.pack(">II", 1000, 1081)
    c.execute("INSERT INTO Variant(VariantID, BrushSize, BrushSizeEffector, BrushPatternImageArray, VariantShowParam)"
              " VALUES (1, 5.0, ?, ?, ?)", (effector, materials, show))
    c.execute("INSERT INTO Variant(VariantID, BrushSize) VALUES (2, 30.0)")
    return to_bytes(c)


class SerialTests(unittest.TestCase):
    def test_round_trips_bytes_and_json(self):
        data = serial.to_bytes(action_set())
        self.assertTrue(serial.looks_like(data))
        document = serial.parse(data)
        self.assertEqual(serial.to_bytes(document), data)
        json = serial.to_json(document)
        self.assertEqual(json["@typename"], "typeActionSet")
        item = json["actionArray"]["@array"][0]
        self.assertEqual(item["offset"], {"int": -10})
        self.assertEqual(item["actionItemEnable"], {"bool": True})
        self.assertEqual(serial.to_bytes(serial.from_json(json)), data)
        self.assertEqual(document.root.items("actionArray")[0].str("actionItemKind"), "layeractionrasterdetailnew")

    def test_keeps_trailing_bytes_and_rejects_cut_files(self):
        data = serial.to_bytes(action_set())
        self.assertEqual(serial.to_bytes(serial.parse(data + b"\0\x3f")), data + b"\0\x3f")
        with self.assertRaises(CspFormatError):
            serial.parse(data[:-3])


class ColorSetTests(unittest.TestCase):
    def test_round_trips(self):
        colors = colorset.ColorSet("肌", [colorset.Color((242, 211, 196, 255)), colorset.Color((128, 64, 32, 128), "影")])
        data = colorset.to_bytes(colors)
        parsed = colorset.parse(data)
        self.assertEqual(parsed.name, "肌")
        self.assertEqual(parsed.colors[1].name, "影")
        self.assertEqual(parsed.colors[1].rgba, (128, 64, 32, 128))
        self.assertEqual(colorset.to_bytes(parsed), data)
        with self.assertRaises(CspFormatError):
            colorset.parse(data[:-1])


class SubToolTests(unittest.TestCase):
    def test_tree_and_settings(self):
        db = subtool.SubToolDb(tool_db())
        self.assertEqual(db.kind, "toolDb")
        names = [(depth, db.nodes[i].name) for i, depth, _ in db.tree()]
        self.assertEqual(names, [(0, ""), (1, "ペン"), (2, "G-ペン"), (2, "消しゴム")])
        pens = db.sub_tools()
        self.assertEqual(pens[0][2], ["ペン"])
        self.assertEqual([db.nodes[i].kind for i, _, _ in pens], ["brush", "eraser"])
        settings = db.settings(1)
        self.assertEqual(settings["BrushSize"], 5.0)
        self.assertEqual(settings["BrushSizeEffector"]["minimum"], [20, 0, 0, 0])
        self.assertEqual(settings["BrushSizeEffector"]["pressure_graph"], [[0.0, 0.0], [1.0, 1.0]])
        self.assertEqual(settings["BrushPatternImageArray"]["materials"][0]["name"], "丸")
        self.assertEqual(settings["VariantShowParam"], [[1000], [1081]])

    def test_not_a_sub_tool_file(self):
        with self.assertRaises(CspFormatError):
            subtool.SubToolDb(b"SLCC")


class ShortcutTests(unittest.TestCase):
    def test_commands_keys_and_tools(self):
        c = sqlite3.connect(":memory:")
        c.executescript(
            "CREATE TABLE workspaceshortcutmenu(_PW_ID INTEGER PRIMARY KEY, menucommandtype TEXT, menucommand TEXT,"
            " shortcut TEXT, modifier INTEGER);"
            "CREATE TABLE workspaceshortcuttool(_PW_ID INTEGER PRIMARY KEY, uuid BLOB, shortcut TEXT);"
            "INSERT INTO workspaceshortcutmenu(menucommandtype, menucommand, shortcut, modifier) VALUES"
            " ('advancedcommand', 'filesaveas', 'S', 6), ('basiccommand', 'undo', 'Z', 4),"
            " ('basiccommand', 'undo', 'Z', 0), ('advancedcommand', 'filesaveaspng', 'NULL', 0);")
        c.execute("INSERT INTO workspaceshortcuttool(uuid, shortcut) VALUES (?, 'P')", (b"\x02" * 16,))
        keys = shortcut.parse(to_bytes(c))
        self.assertEqual(keys.keys("filesaveas"), ["Ctrl+Shift+S"])
        self.assertEqual(keys.keys("undo"), ["Ctrl+Z", "Z"])
        self.assertEqual(keys.unassigned, ["filesaveaspng"])
        self.assertEqual(keys.tools, [("02" * 16, "P")])
        with self.assertRaises(CspFormatError):
            shortcut.parse(tool_db())

    def test_modifier_keys(self):
        words = lambda *w: struct.pack(f">{len(w)}I", *w)  # noqa: E731
        common = words(8, 2, 36, 10, 2, 0, 16) + b"\x07" * 16 + words(16, 6, 64, 0)
        c = sqlite3.connect(":memory:")
        c.executescript("CREATE TABLE ModifyKeySetting(_PW_ID INTEGER PRIMARY KEY, InputOperation INTEGER,"
                        " OutputOperation INTEGER, RangeOperation INTEGER, SettingData BLOB);")
        c.execute("INSERT INTO ModifyKeySetting(InputOperation, OutputOperation, RangeOperation, SettingData)"
                  " VALUES (0, 0, 0, ?), (1, 10, 0, ?), (2, 2, 0, NULL)", (common, words(8, 1, 16, 1, 1, 100)))
        rules = shortcut.modifier_keys(to_bytes(c))
        self.assertEqual([r["ops"] for r in rules], [(0, 0, 0), (1, 10, 0)])
        first, second = rules[0]["entries"]
        self.assertEqual((first.label(), first.action), ("Ctrl+Space", ("tool", "07" * 16)))
        self.assertEqual((second.label(), second.action), ("Ctrl+Alt", ("other", 64, "00000000")))
        self.assertEqual(rules[1]["entries"][0].action, ("operation", 100))
        with self.assertRaises(CspFormatError):
            shortcut.setting_data(common[:-2])


class MaterialTests(unittest.TestCase):
    def test_catalog_and_info(self):
        xml = ('<?xml version="1.0"?><archive><version>2.0.8</version><catalog uuid="c"><type>material</type>'
               '<name>線 &amp; 点</name><groups><group><name></name><items><item uuid="i"><type ver="1.0.1">image'
               '</type><name>点線</name><data><fileref idref="d"/></data></item></items></group></groups>'
               '</catalog><files><file id="d" encrypt="true"><path>data/material_0.layer</path></file></files>'
               '</archive>').encode()
        found = material.catalog(xml)
        self.assertEqual(found["name"], "線 & 点")
        self.assertEqual(found["items"][0]["data"], "d")
        self.assertTrue(found["files"][0]["encrypted"])
        info = material.info_list(b'<infolist><version>2.0.0</version><info uuid="u"><datalist key="tag">'
                                  b'<data>a</data></datalist></info></infolist>')
        self.assertEqual(info["infos"][0]["data"], [["tag", ["a"]]])

    def test_install_map_cac_envelope_and_tar(self):
        pair = [".:15_brush_pattern:a.layer", ".:Paint001:u:data:material.layer"]
        data = struct.pack(">Q", 1) + b"".join(struct.pack(">I", len(t)) + t.encode() for t in pair)
        self.assertEqual(material.install_map(data), [tuple(pair)])
        cac = b"\x07\x001.0.1.6\0\0\x03\0" + "線".encode()
        self.assertTrue(material.looks_like_cac(cac))
        self.assertEqual(material.cac_strings(cac), ["1.0.1.6", "線"])
        import zlib
        envelope = c2f.SIGNATURE + struct.pack("<I4s", 0, b"HEAD") + struct.pack("<I", zlib.crc32(b"HEAD"))
        self.assertEqual(c2f.summary(envelope), {"chunks": ["HEAD"], "payload_bytes": 0, "crc_ok": True,
                                                 "encrypted": True})
        entries = [tar.Entry("catalog.zip", b"z" * 700), tar.Entry("data/material_0.layer", b"d")]
        self.assertEqual(tar.read(tar.write(entries, 1618959627)), entries)


if __name__ == "__main__":
    unittest.main()
