"""Material folders: catalog.xml, info.xml / layerData.xml, .cac caches and
``InstallPathToCatalogId.dat``; see ``crates/csp-assets/src/material.rs``.
Encrypted data files are summarised with :mod:`.c2f`, never decrypted.
"""
from __future__ import annotations

import struct
import xml.etree.ElementTree as ElementTree

from ..errors import CspFormatError


def _parse(xml: bytes) -> ElementTree.Element:
    try:
        return ElementTree.fromstring(xml)
    except ElementTree.ParseError as error:
        raise CspFormatError(f"XML: {error}") from error


def _text(element, name: str) -> str:
    child = element.find(name) if element is not None else None
    return (child.text or "") if child is not None else ""


def _fileref(element, name: str):
    ref = element.find(f"{name}/fileref")
    return ref.get("idref") if ref is not None else None


def catalog(xml: bytes) -> dict:
    """``catalog.xml`` as the JSON of ``cspf asset inspect``."""
    archive = _parse(xml)
    if archive.tag != "archive":
        raise CspFormatError(f"catalog.xml: root is <{archive.tag}>")
    cat = archive.find("catalog")
    if cat is None:
        raise CspFormatError("catalog.xml: no <catalog>")
    items: list[dict] = []

    def collect(groups, path):
        for group in groups.findall("group"):
            here = path + [_text(group, "name")]
            for item in group.findall("items/item"):
                kind = item.find("type")
                items.append({
                    "uuid": item.get("uuid", ""),
                    "kind": (kind.text or "") if kind is not None else "",
                    "kind_version": kind.get("ver", "") if kind is not None else "",
                    "name": _text(item, "name"),
                    "created": _text(item, "created_date"),
                    "modified": _text(item, "modified_date"),
                    "groups": [p for p in here if p],
                    "thumbnail": _fileref(item, "thumbnail"),
                    "data": _fileref(item, "data"),
                    "icedata": _fileref(item, "icedata"),
                })
            collect(group, here)

    groups = cat.find("groups")
    if groups is not None:
        collect(groups, [])
    tool = cat.find("tool")
    content = cat.find("content_id")
    return {
        "version": _text(archive, "version"),
        "uuid": cat.get("uuid", ""),
        "kind": _text(cat, "type"),
        "name": _text(cat, "name"),
        "tool": f"{tool.get('name', '')} {tool.get('version', '')}" if tool is not None else "",
        "content_id": (content.text or "") if content is not None else None,
        "items": items,
        "files": [
            {"id": f.get("id", ""), "path": _text(f, "path"), "mime": f.get("mime", ""),
             "encrypted": f.get("encrypt") == "true"}
            for f in archive.findall("files/file")
        ],
    }


def info_list(xml: bytes) -> dict:
    """``info.xml`` / ``icedata/layerData.xml``."""
    root = _parse(xml)
    if root.tag != "infolist":
        raise CspFormatError(f"info list: root is <{root.tag}>")
    return {
        "version": _text(root, "version"),
        "infos": [
            {"uuid": info.get("uuid", ""),
             "data": [[d.get("key", ""), [x.text or "" for x in d.findall("data")]] for d in info.findall("datalist")]}
            for info in root.findall("info")
        ],
    }


def install_map(data: bytes) -> list[tuple[str, str]]:
    """``InstallPathToCatalogId.dat``: (old material path, installed path)."""
    try:
        (count,) = struct.unpack_from(">Q", data, 0)
        at, out = 8, []
        for _ in range(count):
            pair = []
            for _ in range(2):
                (n,) = struct.unpack_from(">I", data, at)
                if at + 4 + n > len(data):
                    raise CspFormatError("InstallPathToCatalogId: cut short")
                pair.append(data[at + 4:at + 4 + n].decode("utf-8"))
                at += 4 + n
            out.append((pair[0], pair[1]))
    except (struct.error, UnicodeDecodeError) as error:
        raise CspFormatError(f"InstallPathToCatalogId: {error}") from error
    if at != len(data):
        raise CspFormatError("InstallPathToCatalogId: bytes after the entries")
    return out


def looks_like_cac(data: bytes) -> bool:
    return len(data) > 9 and data[1] == 0 and data[0] + 2 <= len(data) and data[2:].startswith(b"1.0.1.")


def cac_strings(data: bytes) -> list[str]:
    """Readable strings of a ``catalog*.cac`` cache (u16 LE length + UTF-8)."""
    out, at = [], 0
    while at + 2 <= len(data):
        n = data[at] | data[at + 1] << 8
        if n and at + 2 + n <= len(data):
            try:
                text = data[at + 2:at + 2 + n].decode("utf-8")
            except UnicodeDecodeError:
                text = None
            if text is not None and not any(ord(c) < 32 or 0x7F <= ord(c) < 0xA0 for c in text):
                out.append(text)
                at += 2 + n
                continue
        at += 1
    return out
