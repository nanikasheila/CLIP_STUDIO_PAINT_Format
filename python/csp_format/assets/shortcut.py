"""Shortcut keys (``Shortcut/default.khc`` and workspaces ``.spws``); see
``crates/csp-assets/src/shortcut.rs``.

``shortcutmenu`` (``workspaceshortcutmenu``): one row per command and key,
``modifier`` bits 1 Alt, 2 Shift, 4 Ctrl; ``workspaceshortcuttool``: a key
for a tool node UUID.
"""
from __future__ import annotations

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
