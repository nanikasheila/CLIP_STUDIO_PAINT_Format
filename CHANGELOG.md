# Changelog

All notable changes to the crates `csp-format`, `csp-assets`, `cspf` and the
Python package `csp-format`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, breaking changes
raise the minor version).

## [Unreleased]

### Changed

- The repository moved from `nanikasheila/CLIP_STUDIO_PAINT_Format` to
  `nanikasheila/csp-format`, so that its name matches the packages and does
  not carry the product name. GitHub forwards the old URLs. The repository
  and homepage links in the crate and PyPI metadata change with the next
  release; the 0.1.0 pages keep the old links, which still work.

## [0.1.0] - 2026-10-26

First release.

### csp-format

- Read `.clip`: the CSFCHUNK container, the embedded SQLite database and its
  schema registry, the canvas and layer tree, raster pixels (colour, gray,
  1-bit, masks, mipmaps), vector data, text layers, layer effects, tone,
  gradient, correction and image material layers, the stored preview, and
  animation (timelines, cels, keyframes, the 2D camera, onion skin).
- Read `.cmc`: the page list in reading order and the work settings.
- Write: lossless saving (an unchanged file is byte-identical), new canvases
  and works with story info and nombre, editing pixels and layer attributes,
  adding and removing layers and folders, copying layers between files,
  page list edits, animation timeline and cel edits.
- Render the layers into an image; check every part of a file
  (`verify::check_clip`, `verify::check_cmc`); tell which CSP generation
  saved a file (`version`).

### csp-assets

- Sub tools (`.sut`) and tool databases (`.todb`), auto actions (`.las`,
  `.laf`), colour sets (`.cls`), gradient sets (`.sgs`), shortcut and
  modifier key settings, material folders, the C2F envelope of encrypted
  material data (never decrypted).

### cspf

- Commands to inspect, check, extract, render, create and edit files, and
  `cspf asset` for the files of `csp-assets`.
- Inputs are never replaced: a command refuses an output that is one of its
  inputs, by any path, symbolic link or hard link.

### Python

- `csp_format`: a reader with the standard library only, matching the Rust
  crate; `csp_format.assets` for the files of `csp-assets`.
- The sdist carries the tests and their synthetic files.

[Unreleased]: https://github.com/nanikasheila/csp-format/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nanikasheila/csp-format/releases/tag/v0.1.0
