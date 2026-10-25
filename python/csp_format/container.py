"""The CSFCHUNK container of a .clip file (spec: docs/spec/01-container.md).

A .clip is a chunked container: a 24-byte header, then chunks of
``8-byte tag, u64 length, body`` (big endian). ``CHNKSQLi`` holds the
SQLite database and every ``CHNKExta`` one external chunk (pixel blocks,
vector data) as ``u64 40, 40-byte id, payload``.
"""
from __future__ import annotations

import struct
from dataclasses import dataclass

from .errors import CspFormatError

MAGIC = b"CSFCHUNK"
HEADER_SIZE = 24
CHUNK_HEADER_SIZE = 16
EXTERNAL_ID_LENGTH = 40


@dataclass(frozen=True)
class Chunk:
    """One chunk: its tag and where its body lies in the file."""

    tag: bytes
    offset: int
    body_start: int
    body_end: int


class Container:
    """A parsed container. The bytes are kept; nothing is copied until asked."""

    def __init__(self, data: bytes):
        if len(data) < HEADER_SIZE or data[:8] != MAGIC:
            raise CspFormatError("not a CLIP STUDIO file (no CSFCHUNK header)")
        declared, first = struct.unpack(">QQ", data[8:24])
        if declared != len(data):
            raise CspFormatError(f"header says {declared} bytes, the file has {len(data)}")
        self.data = data
        self.chunks: list[Chunk] = []
        self._externals: dict[str, tuple[int, int]] = {}
        position = first
        while position < len(data):
            if position + CHUNK_HEADER_SIZE > len(data):
                raise CspFormatError(f"truncated chunk header at {position}")
            tag = data[position:position + 8]
            if not tag.startswith(b"CHNK"):
                raise CspFormatError(f"unexpected chunk tag {tag!r} at {position}")
            (length,) = struct.unpack(">Q", data[position + 8:position + 16])
            start = position + CHUNK_HEADER_SIZE
            end = start + length
            if end > len(data):
                raise CspFormatError(f"chunk {tag!r} at {position} runs past the end")
            self.chunks.append(Chunk(tag, position, start, end))
            if tag == b"CHNKExta":
                (id_length,) = struct.unpack(">Q", data[start:start + 8])
                if id_length != EXTERNAL_ID_LENGTH or start + 8 + id_length > end:
                    raise CspFormatError(f"external chunk at {position} has an id of {id_length} bytes")
                external_id = data[start + 8:start + 8 + id_length].decode("ascii")
                self._externals[external_id] = (start + 8 + id_length, end)
            if tag == b"CHNKFoot":
                break
            position = end

    def sqlite(self) -> bytes:
        """The embedded SQLite database image."""
        for chunk in self.chunks:
            if chunk.tag == b"CHNKSQLi":
                return self.data[chunk.body_start:chunk.body_end]
        raise CspFormatError("no SQLite chunk")

    def external(self, external_id: str) -> bytes | None:
        """Payload of an external chunk, or None when the id is not stored."""
        span = self._externals.get(external_id)
        return None if span is None else self.data[span[0]:span[1]]

    def external_ids(self) -> list[str]:
        return list(self._externals)
