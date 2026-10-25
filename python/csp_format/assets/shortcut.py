"""Shortcut keys (``Shortcut/default.khc`` and workspaces ``.spws``); see
``crates/csp-assets/src/shortcut.rs``.

``shortcutmenu`` (``workspaceshortcutmenu``): one row per command and key,
``modifier`` bits 1 Alt, 2 Shift, 4 Ctrl; ``workspaceshortcuttool``: a key
for a tool node UUID. Modifier key settings (``.tomd``): ``ModifyKeySetting``
rows with a ``SettingData`` BLOB, keys bits 1 Shift, 2 Ctrl, 4 Alt, 8 Space.
"""
from __future__ import annotations

import struct
from dataclasses import dataclass, field

from ..errors import CspFormatError
from .subtool import _open, _rows

ALT, SHIFT, CTRL = 1, 2, 4


@dataclass
class Shortcut:
    kind: str
    command: str
    key: str
    modifiers: int

    def label(self) -> str:
        names = [name for bit, name in ((CTRL, "Ctrl"), (SHIFT, "Shift"), (ALT, "Alt")) if self.modifiers & bit]
        return "+".join(names + [self.key])


@dataclass
class Shortcuts:
    commands: list[Shortcut] = field(default_factory=list)
    unassigned: list[str] = field(default_factory=list)
    tools: list[tuple[str, str]] = field(default_factory=list)

    def keys(self, command: str) -> list[str]:
        return [s.label() for s in self.commands if s.command == command]


def parse(data: bytes) -> Shortcuts:
    connection = _open(data)
    connection.text_factory = lambda b: b.decode("utf-8", "replace")
    tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
    menu = next((t for t in ("shortcutmenu", "workspaceshortcutmenu") if t in tables), None)
    if menu is None:
        raise CspFormatError("no shortcutmenu table (not a .khc or .spws file)")
    out = Shortcuts()
    for row in _rows(connection, menu):
        key = row.get("shortcut") or ""
        if key in ("", "NULL"):
            out.unassigned.append(row.get("menucommand") or "")
        else:
            out.commands.append(Shortcut(row.get("menucommandtype") or "", row.get("menucommand") or "", key,
                                         row.get("modifier") or 0))
    if "workspaceshortcuttool" in tables:
        for row in _rows(connection, "workspaceshortcuttool"):
            key = row.get("shortcut") or ""
            if key not in ("", "NULL"):
                out.tools.append(((row.get("uuid") or b"").hex(), key))
    connection.close()
    return out


MOD_SHIFT, MOD_CTRL, MOD_ALT, MOD_SPACE = 1, 2, 4, 8


@dataclass
class ModifierEntry:
    keys: int
    action: tuple  # ("tool", uuid hex) | ("operation", n) | ("other", action, payload hex)

    def label(self) -> str:
        names = [n for bit, n in ((MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_SPACE, "Space"))
                 if self.keys & bit]
        rest = self.keys & ~(MOD_SHIFT | MOD_CTRL | MOD_ALT | MOD_SPACE)
        return "+".join(names + ([f"0x{rest:x}"] if rest else []))


def setting_data(data: bytes) -> list[ModifierEntry]:
    """One ``ModifyKeySetting.SettingData`` BLOB (big endian)."""
    try:
        head, count = struct.unpack_from(">II", data, 0)
        if head != 8:
            raise CspFormatError("modifier keys: SettingData does not start with 8")
        at, out = 8, []
        for _ in range(count):
            size, keys, action = struct.unpack_from(">III", data, at)
            if size < 12 or at + size > len(data):
                raise CspFormatError(f"modifier keys: {size}-byte entry at {at:#x}")
            payload = data[at + 12:at + size]
            if action == 2 and len(payload) == 24 and payload[:8] == bytes(7) + b"\x10":
                out.append(ModifierEntry(keys, ("tool", payload[8:].hex())))
            elif action == 1 and len(payload) == 4:
                out.append(ModifierEntry(keys, ("operation", struct.unpack(">I", payload)[0])))
            else:
                out.append(ModifierEntry(keys, ("other", action, payload.hex())))
            at += size
    except struct.error as error:
        raise CspFormatError(f"modifier keys: {error}") from error
    if at != len(data):
        raise CspFormatError(f"modifier keys: {len(data) - at} bytes after the entries")
    return out


def modifier_keys(data: bytes) -> list[dict]:
    """The rules of a ``.tomd`` file: ops and entries (rows without data are left out)."""
    connection = _open(data)
    tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
    if "ModifyKeySetting" not in tables:
        raise CspFormatError("no ModifyKeySetting table (not a .tomd file)")
    rules = []
    for row in _rows(connection, "ModifyKeySetting"):
        if row.get("SettingData") is None:
            continue
        rules.append({"ops": (row.get("InputOperation") or 0, row.get("OutputOperation") or 0,
                              row.get("RangeOperation") or 0), "entries": setting_data(row["SettingData"])})
    connection.close()
    return rules
