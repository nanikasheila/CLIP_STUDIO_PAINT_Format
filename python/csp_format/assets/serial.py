"""CSP's typed serialisation (.laf / .las auto actions, .sgs gradient sets).

Same layout and JSON form as ``crates/csp-assets/src/serial.rs``::

    object = fields(head) fields(body)        head holds "typename"
    fields = count, (key, tag, value) * count
    number = u8 n, n bytes big endian (minimal width)
    0x06 string, 0x07 bytes, 0x11 bool (always 1 byte), 0x13 int (4 bytes when negative),
    0x14 uint, 0x15 real (f64), 0x48 array (u32 length, fields, count,
    (tag value) * count), 0x49 object (u32 length, object)

Values are kept as :class:`Value` (tag and raw bytes) so that writing gives
the original bytes; :func:`to_json` gives the JSON form the Rust crate uses.
"""
from __future__ import annotations

import struct
from dataclasses import dataclass, field

from ..errors import CspFormatError

STRING, BYTES, BOOL, INT, UINT, REAL, ARRAY, OBJECT = 0x06, 0x07, 0x11, 0x13, 0x14, 0x15, 0x48, 0x49
SIGNATURE = b"\x01\x01\x01\x08typename"


def looks_like(data: bytes) -> bool:
    return data.startswith(SIGNATURE)


@dataclass
class Value:
    """A scalar: ``tag`` and ``raw`` (string: str, bytes: bytes, numbers: their stored bytes, real: float)."""

    tag: int
    raw: object

    def as_int(self) -> int:
        if self.tag == REAL:
            return int(self.raw)
        unsigned = int.from_bytes(self.raw, "big")
        if self.tag == INT and len(self.raw) == 4 and unsigned >= 1 << 31:
            return unsigned - (1 << 32)
        return unsigned


@dataclass
class Array:
    head: list
    items: list
    tag: int = ARRAY


@dataclass
class Object:
    head: list = field(default_factory=list)
    body: list = field(default_factory=list)
    tag: int = OBJECT

    @property
    def typename(self) -> str:
        for key, value in self.head:
            if key == "typename" and isinstance(value, Value) and value.tag == STRING:
                return value.raw
        return ""

    def get(self, key: str):
        for k, value in self.body:
            if k == key:
                return value
        return None

    def str(self, key: str) -> str | None:
        value = self.get(key)
        return value.raw if isinstance(value, Value) and value.tag == STRING else None

    def int(self, key: str) -> int | None:
        value = self.get(key)
        return value.as_int() if isinstance(value, Value) and value.tag in (BOOL, INT, UINT, REAL) else None

    def items(self, key: str) -> list:
        value = self.get(key)
        return value.items if isinstance(value, Array) else []


@dataclass
class Document:
    root: Object
    trailing: bytes = b""


class _Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.at = 0

    def take(self, n: int) -> bytes:
        if self.at + n > len(self.data):
            raise CspFormatError(f"serialisation: {n} bytes needed at {self.at:#x}")
        out = self.data[self.at:self.at + n]
        self.at += n
        return out

    def raw_number(self) -> bytes:
        n = self.take(1)[0]
        if n > 8:
            raise CspFormatError(f"serialisation: {n}-byte number at {self.at - 1:#x}")
        return self.take(n)

    def number(self) -> int:
        return int.from_bytes(self.raw_number(), "big")

    def text(self) -> str:
        try:
            return self.take(self.number()).decode("utf-8")
        except UnicodeDecodeError as error:
            raise CspFormatError(f"serialisation: text at {self.at:#x}") from error

    def fields(self, depth: int) -> list:
        out = []
        for _ in range(self.number()):
            key = self.text()
            tag = self.take(1)[0]
            out.append((key, self.value(tag, depth)))
        return out

    def object(self, depth: int) -> Object:
        head = self.fields(depth)
        return Object(head, self.fields(depth))

    def sized(self) -> int:
        (length,) = struct.unpack(">I", self.take(4))
        end = self.at + length
        if end > len(self.data):
            raise CspFormatError(f"serialisation: {length}-byte block at {self.at:#x} runs past the end")
        return end

    def value(self, tag: int, depth: int):
        if depth > 64:
            raise CspFormatError("serialisation: nested too deeply")
        if tag == STRING:
            return Value(tag, self.text())
        if tag == BYTES:
            return Value(tag, self.take(self.number()))
        if tag in (BOOL, INT, UINT):
            return Value(tag, self.raw_number())
        if tag == REAL:
            raw = self.raw_number()
            if len(raw) != 8:
                raise CspFormatError(f"serialisation: {len(raw)}-byte real")
            return Value(tag, struct.unpack(">d", raw)[0])
        if tag in (ARRAY, OBJECT):
            end = self.sized()
            if tag == ARRAY:
                head = self.fields(depth + 1)
                items = []
                for _ in range(self.number()):
                    items.append(self.value(self.take(1)[0], depth + 1))
                result = Array(head, items)
            else:
                result = self.object(depth + 1)
            if self.at != end:
                raise CspFormatError(f"serialisation: block ends at {end:#x}, content at {self.at:#x}")
            return result
        raise CspFormatError(f"serialisation: unknown tag {tag:#04x} at {self.at - 1:#x}")


def parse(data: bytes) -> Document:
    reader = _Reader(data)
    root = reader.object(0)
    return Document(root, data[reader.at:])


def _minimal(value: int) -> bytes:
    return value.to_bytes((value.bit_length() + 7) // 8, "big")


def _number(out: bytearray, value: int) -> None:
    raw = _minimal(value)
    out.append(len(raw))
    out += raw


def _fields(out: bytearray, fields: list) -> None:
    _number(out, len(fields))
    for key, value in fields:
        encoded = key.encode("utf-8")
        _number(out, len(encoded))
        out += encoded
        out.append(value.tag)
        _value(out, value)


def _sized(out: bytearray, body: bytes) -> None:
    out += struct.pack(">I", len(body))
    out += body


def _value(out: bytearray, value) -> None:
    if isinstance(value, Object):
        inner = bytearray()
        _fields(inner, value.head)
        _fields(inner, value.body)
        _sized(out, bytes(inner))
    elif isinstance(value, Array):
        inner = bytearray()
        _fields(inner, value.head)
        _number(inner, len(value.items))
        for item in value.items:
            inner.append(item.tag)
            _value(inner, item)
        _sized(out, bytes(inner))
    elif value.tag == STRING:
        encoded = value.raw.encode("utf-8")
        _number(out, len(encoded))
        out += encoded
    elif value.tag == BYTES:
        _number(out, len(value.raw))
        out += value.raw
    elif value.tag == REAL:
        out.append(8)
        out += struct.pack(">d", value.raw)
    else:
        out.append(len(value.raw))
        out += value.raw


def to_bytes(document: Document) -> bytes:
    out = bytearray()
    _fields(out, document.root.head)
    _fields(out, document.root.body)
    return bytes(out) + document.trailing


def _int_raw(value: int) -> bytes:
    return (value & 0xFFFFFFFF).to_bytes(4, "big") if value < 0 else _minimal(value)


def _head_json(out: dict, head: list) -> None:
    if len(head) == 1 and head[0][0] == "typename" and isinstance(head[0][1], Value) and head[0][1].tag == STRING:
        if head[0][1].raw:
            out["@typename"] = head[0][1].raw
    else:
        out["@head"] = {key: _value_json(value) for key, value in head}


def _object_json(obj: Object) -> dict:
    out: dict = {}
    _head_json(out, obj.head)
    for key, value in obj.body:
        out[key] = _value_json(value)
    return out


def _value_json(value):
    if isinstance(value, Object):
        return _object_json(value)
    if isinstance(value, Array):
        out: dict = {}
        _head_json(out, value.head)
        out["@array"] = [_value_json(item) for item in value.items]
        return out
    if value.tag == STRING:
        return value.raw
    if value.tag == BYTES:
        return {"hex": value.raw.hex()}
    if value.tag == REAL:
        return {"real": value.raw}
    number = value.as_int()
    names = {BOOL: "bool", INT: "int", UINT: "uint"}
    out = {names.get(value.tag, f"tag{value.tag:02x}"): (number != 0) if value.tag == BOOL else number}
    canonical = value.raw == (_int_raw(number) if value.tag == INT else (bytes([number]) if number in (0, 1) else None) if value.tag == BOOL else _minimal(number))
    if not canonical or (value.tag == BOOL and number > 1):
        out["@raw"] = value.raw.hex()
    return out


def to_json(document: Document) -> dict:
    """The JSON form of ``cspf asset dump``."""
    out = _object_json(document.root)
    if document.trailing:
        out["@trailing"] = document.trailing.hex()
    return out


def _head_from_json(obj: dict) -> list:
    if isinstance(obj.get("@head"), dict):
        return [(k, _value_from_json(v)) for k, v in obj["@head"].items()]
    return [("typename", Value(STRING, obj.get("@typename", "")))]


def _value_from_json(json):
    if isinstance(json, str):
        return Value(STRING, json)
    if not isinstance(json, dict):
        raise CspFormatError(f"serialisation JSON: cannot read {json!r}")
    if isinstance(json.get("@array"), list):
        return Array(_head_from_json(json), [_value_from_json(item) for item in json["@array"]])
    raw = bytes.fromhex(json["@raw"]) if isinstance(json.get("@raw"), str) else None
    for key, tag in (("bool", BOOL), ("int", INT), ("uint", UINT)):
        if key in json:
            number = int(json[key])
            if raw is None:
                raw = _int_raw(number) if tag == INT else bytes([number != 0]) if tag == BOOL else _minimal(number)
            return Value(tag, raw)
    if "real" in json:
        return Value(REAL, float(json["real"]))
    if isinstance(json.get("hex"), str):
        return Value(BYTES, bytes.fromhex(json["hex"]))
    for key, number in json.items():
        if key.startswith("tag"):
            tag = int(key[3:], 16)
            return Value(tag, raw if raw is not None else _minimal(int(number)))
    return _object_from_json(json)


def _object_from_json(json: dict) -> Object:
    body = [(k, _value_from_json(v)) for k, v in json.items() if not k.startswith("@")]
    return Object(_head_from_json(json), body)


def from_json(json: dict) -> Document:
    trailing = bytes.fromhex(json["@trailing"]) if isinstance(json.get("@trailing"), str) else b""
    return Document(_object_from_json(json), trailing)
