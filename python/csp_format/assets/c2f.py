"""The chunk envelope of CSP's encrypted material data (``\\x89C2F``).

Only the envelope is read; the payload is encrypted by CELSYS and is not
decrypted (``docs/assets/requirements.md``, C5).
"""
from __future__ import annotations

import struct
import zlib

from ..errors import CspFormatError

SIGNATURE = b"\x89C2F\r\n\x1a\n"


def chunks(data: bytes) -> list[tuple[str, bytes, bool]]:
    """(type, data, CRC matches) per chunk."""
    if not data.startswith(SIGNATURE):
        raise CspFormatError("not a C2F file")
    out, at = [], len(SIGNATURE)
    while at < len(data):
        if at + 8 > len(data):
            raise CspFormatError(f"C2F: chunk header cut at {at:#x}")
        length, kind = struct.unpack_from("<I4s", data, at)
        end = at + 8 + length
        if end + 4 > len(data):
            raise CspFormatError(f"C2F: chunk at {at:#x} runs past the end")
        body = data[at + 8:end]
        (stored,) = struct.unpack_from("<I", data, end)
        out.append((kind.decode("latin-1"), body, stored == zlib.crc32(kind + body) & 0xFFFFFFFF))
        at = end + 4
    return out


def summary(data: bytes) -> dict:
    found = chunks(data)
    return {
        "chunks": [kind for kind, _, _ in found],
        "payload_bytes": sum(len(body) for kind, body, _ in found if kind == "dATA"),
        "crc_ok": all(ok for _, _, ok in found),
        "encrypted": True,
    }
