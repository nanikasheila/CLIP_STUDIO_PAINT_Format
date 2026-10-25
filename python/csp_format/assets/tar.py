"""The ustar archives in .sut files and material folders; see ``crates/csp-assets/src/tar.rs``."""
from __future__ import annotations

from dataclasses import dataclass

from ..errors import CspFormatError


@dataclass
class Entry:
    name: str
    data: bytes


def _octal(field: bytes) -> int:
    text = field.split(b"\0", 1)[0].strip()
    try:
        return int(text, 8) if text else 0
    except ValueError as error:
        raise CspFormatError(f"tar: bad number {text!r}") from error


def read(data: bytes) -> list[Entry]:
    """Regular files in order."""
    entries, at = [], 0
    while at + 512 <= len(data):
        header = data[at:at + 512]
        if not any(header):
            break
        name = header[:100].split(b"\0", 1)[0].decode("utf-8", "replace")
        prefix = header[345:500].split(b"\0", 1)[0].decode("utf-8", "replace")
        if header[257:262] == b"ustar" and prefix:
            name = f"{prefix}/{name}"
        size = _octal(header[124:136])
        start = at + 512
        if start + size > len(data):
            raise CspFormatError(f"tar: {name} runs past the end")
        if header[156] in (0x30, 0):
            entries.append(Entry(name, data[start:start + size]))
        at = start + (size + 511) // 512 * 512
    return entries


def write(entries: list[Entry], mtime: int = 0) -> bytes:
    """CSP's layout: mode 0777, owner ``compress``, checksum as 7 digits and NUL."""
    out = bytearray()
    for entry in entries:
        name = entry.name.encode("utf-8")
        if len(name) > 99:
            raise CspFormatError(f"tar: name too long: {entry.name}")
        header = bytearray(512)
        header[:len(name)] = name
        header[100:108] = b"0000777\0"
        header[108:116] = b"0000000\0"
        header[116:124] = b"0000000\0"
        header[124:136] = b"%011o\0" % len(entry.data)
        header[136:148] = b"%011o\0" % mtime
        header[156] = 0x30
        header[257:263] = b"ustar\0"
        header[263:265] = b"00"
        header[265:273] = b"compress"
        header[148:156] = b" " * 8
        header[148:156] = b"%07o\0" % sum(header)
        out += header + entry.data
        out += bytes(-len(out) % 512)
    return bytes(out) + bytes(1024)
