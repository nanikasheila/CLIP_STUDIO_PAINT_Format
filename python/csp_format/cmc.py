"""Multi-page works (.cmc): a plain SQLite file next to the page files (spec: docs/spec/07-cmc.md).

``Project`` holds the work settings and ``CanvasNode`` the page list::

    root node (Type 0): FirstChildIndex → first page, SelectedIndex → selected page
    page node (Type 2): NextIndex → next page (0 ends the list),
                        LinkPath ".:pageNNNN.clip" (relative to the .cmc folder),
                        PageFlag 1 = two-page spread

The page order is the order of the linked list, not of the file names.
"""
from __future__ import annotations

import os
import sqlite3
from dataclasses import dataclass
from pathlib import Path

from .clip import ClipFile, _int, _real, _text, _truthy, _value, open_database
from .errors import CspFormatError
from .image import Image

SQLITE_MAGIC = b"SQLite format 3\x00"
NODE_ROOT = 0
NODE_PAGE = 2


@dataclass(frozen=True)
class CanvasNode:
    main_id: int
    kind: int
    next: int
    first_child: int
    selected: int
    spread: bool
    link_path: str | None
    memo: str

    @property
    def relative_path(self) -> Path | None:
        """The page file relative to the .cmc folder (``page0001.clip``)."""
        if not self.link_path:
            return None
        parts = [part for part in self.link_path.split(":") if part not in ("", ".")]
        return Path(*parts) if parts else None


@dataclass(frozen=True)
class PageSlot:
    """Where a page sits in the work (as ``work::PageSlot`` in Rust)."""

    #: Pages before this one, counting a spread as two.
    position: int
    spread: bool
    #: The page (a spread: its first page) is a left-hand page.
    left: bool


@dataclass(frozen=True)
class CmcProject:
    """Work settings from ``Project`` (the common subset; ``CmcFile.database`` has the rest)."""

    version: str | None
    root_node: int
    story_name: str
    author_name: str
    #: ``ComicBindPosition``: 1 right binding, 0 left binding.
    bind_position: int
    #: ``ComicStartPage``: 0 starts on the left, 1 on the right.
    start_page: int
    cover: bool
    #: ``DefaultPageUnit`` (0 = px, 2 = mm in the samples).
    page_unit: int
    page_width: float
    page_height: float
    page_resolution: float
    double_page: bool
    template_name: str
    max_canvas_file_index: int


class CmcFile:
    """An opened .cmc (read only)."""

    def __init__(self, data: bytes, path: Path | None = None):
        if not data.startswith(SQLITE_MAGIC):
            raise CspFormatError("not a .cmc (SQLite) file")
        self.path = path
        self._data = data
        self.database = open_database(data)
        row = self.database.execute("SELECT * FROM Project LIMIT 1").fetchone()
        if row is None:
            raise CspFormatError(".cmc has no Project row")
        self.project = CmcProject(
            version=_text(_value(row, "ProjectInternalVersion")),
            root_node=_int(row, "ProjectRootCanvasNode"),
            story_name=_text(_value(row, "ComicStoryStoryName")) or "",
            author_name=_text(_value(row, "ComicStoryAuthorName")) or "",
            bind_position=_int(row, "ComicBindPosition"),
            start_page=_int(row, "ComicStartPage"),
            cover=_truthy(row, "ComicCoverExist"),
            page_unit=_int(row, "DefaultPageUnit"),
            page_width=_real(row, "DefaultPageWidth"),
            page_height=_real(row, "DefaultPageHeight"),
            page_resolution=_real(row, "DefaultPageResolution"),
            double_page=_truthy(row, "DefaultPageDoublePage"),
            template_name=_text(_value(row, "DefaultPageTemplateName")) or "",
            max_canvas_file_index=_int(row, "MaxCanvasFileIndex"),
        )
        self.nodes = {
            _int(r, "MainId"): CanvasNode(
                main_id=_int(r, "MainId"), kind=_int(r, "Type"), next=_int(r, "NextIndex"),
                first_child=_int(r, "FirstChildIndex"), selected=_int(r, "SelectedIndex"),
                spread=bool(_int(r, "PageFlag") & 1), link_path=_text(_value(r, "LinkPath")),
                memo=_text(_value(r, "Memo")) or "",
            )
            for r in self.database.execute("SELECT * FROM CanvasNode")
        }

    @classmethod
    def open(cls, path: str | os.PathLike) -> "CmcFile":
        path = Path(path)
        return cls(path.read_bytes(), path)

    def version_info(self):
        """Which CSP generation saved the work file (:mod:`csp_format.version`)."""
        from .version import read_version_info
        return read_version_info(self.database, self._data)

    def close(self) -> None:
        self.database.close()

    def __enter__(self) -> "CmcFile":
        return self

    def __exit__(self, *_) -> None:
        self.close()

    def pages(self) -> list[CanvasNode]:
        """The page nodes in page order."""
        root = self.nodes.get(self.project.root_node)
        if root is None:
            raise CspFormatError(f"root canvas node {self.project.root_node} does not exist")
        pages, seen, node_id = [], set(), root.first_child
        while node_id:
            if node_id in seen:
                raise CspFormatError(f"page list loops at canvas node {node_id}")
            seen.add(node_id)
            node = self.nodes.get(node_id)
            if node is None:
                raise CspFormatError(f"page list points to missing canvas node {node_id}")
            pages.append(node)
            node_id = node.next
        return pages

    def positions(self) -> list[PageSlot]:
        """Each page's position and side, counted from the page list as CSP
        does (the page files' own ``ComicPageIndex``/``ComicIsLeftPage`` are
        caches CSP refreshes only when it opens the page)."""
        start_right = self.project.start_page == 1
        slots, position = [], 0
        for node in self.pages():
            slots.append(PageSlot(position, node.spread, (position % 2 == 0) != start_right))
            position += 2 if node.spread else 1
        return slots

    def _folder(self, folder: str | os.PathLike | None) -> Path:
        if folder is not None:
            return Path(folder)
        if self.path is None:
            raise CspFormatError("the .cmc was read from bytes: pass the folder of its pages")
        return self.path.parent

    def page_file_paths(self, folder: str | os.PathLike | None = None) -> list[Path]:
        """Each page's .clip path (by default relative to the .cmc's folder)."""
        base = self._folder(folder)
        return [base / node.relative_path for node in self.pages() if node.relative_path is not None]

    def page_thumbnail(self, node: CanvasNode, folder: str | os.PathLike | None = None) -> Image | None:
        """The thumbnail CSP shows in the page list: the page file's preview
        (``CanvasNode.CanvasThumbnail`` is unset in every .cmc seen)."""
        if node.relative_path is None:
            return None
        path = self._folder(folder) / node.relative_path
        if not path.is_file():
            return None
        with ClipFile.open(path) as page:
            return page.preview()


def read_cmc(path: str | os.PathLike) -> CmcFile:
    """Open a .cmc file (read only)."""
    return CmcFile.open(path)
