# csp-format

[日本語](README.md)

Rust libraries and a command line tool to read, verify and write CLIP STUDIO
PAINT files: `csp-format` for canvases (`.clip`) and multi-page works
(`.cmc`), `csp-assets` for sub tools, auto actions, colour and gradient sets
and materials, and the CLI `cspf`. A Python reader that needs only the
standard library is in [python/](python/README.md).

The format is not published. The specification was written from observing
real files, and every rule was checked against real files: 719 works saved
by CSP versions from 2017 to 2026, and files made in CLIP STUDIO PAINT EX for
each feature.

Not made or endorsed by CELSYS. CLIP STUDIO PAINT is a trademark of CELSYS, Inc.

- Requirements: [docs/requirements.md](docs/requirements.md)
- Specification: [docs/spec/](docs/spec/) (with the list of what is not yet understood)
- Validation records: [docs/validation/](docs/validation/) (the 719-file corpus: [V1](docs/validation/V1-corpus.md), rendering: [V3](docs/validation/V3-render.md))
- Other CSP files: [docs/assets/](docs/assets/requirements.md)

The documents are in Japanese; the API documentation (rustdoc) and the code
comments are in English.

## What it does

| Part | Read | Write | Checked against |
|---|---|---|---|
| Container (CSFCHUNK chunks) | yes | yes, byte-identical | 719 files |
| Embedded SQLite (every table) and the schema registry | yes | yes | 719 files |
| Layer tree, canvas settings | yes | — | 27,986 layers |
| Raster pixels (colour, gray, 1-bit, masks, mipmaps) | yes, to PNG | yes, compressed exactly as CSP | 1.86 million blocks |
| Vector data | points, boxes, colours, widths | byte-identical (undecoded parts kept) | 1.17 million objects |
| Text (strings and attributes) | yes | byte-identical | 8,344 blocks |
| Stored preview | PNG | — | 719 files |
| .cmc (page list, work settings) | yes | reorder, add, remove | 42 works, opened in CSP EX |
| Rendering (blend modes, clipping, masks, effects, corrections, gradients) | PNG | — | 719 files against their previews |
| Editing (pixels, name, opacity, blend mode, visibility, adding and removing layers and folders) | — | yes | opened in CSP |
| New canvases and works (with story info and nombre) | — | yes | opened in CSP |
| Copying layers between files (vector and text included) | — | yes | opened in CSP EX |
| Animation (timelines, cels, keyframes, 2D camera) | per-frame rendering | frame rate, range, cels | CSP's image-sequence export |
| Sub tools, auto actions, colour / gradient sets, materials (`csp-assets`) | yes | byte-identical | loaded in CSP EX |

What is approximate or not drawn (vector line shapes, text CSP did not
render, some layer effects, 3D) is listed in [the specification](docs/spec/README.md).

## Getting it

| Use | How |
|---|---|
| Rust library | `cargo add csp-format` (and `cargo add csp-assets`) |
| CLI | a binary from the [releases](https://github.com/nanikasheila/csp-format/releases) (Windows, macOS, Linux) or `cargo install cspf` |
| Python (reading) | `pip install csp-format` |
| Other languages | the JSON of `cspf inspect` / `layers` / `anim` and the PNGs of `extract` / `render` |

For the latest main: `csp-format = { git = "https://github.com/nanikasheila/csp-format" }`, or clone and
`cargo build --release -p cspf`.

Building needs a C compiler: SQLite and zlib are compiled in (the reference
C zlib is needed to reproduce CSP's compressed bytes). Minimum Rust: 1.88.

### Versions

[Semantic Versioning](https://semver.org/). Before 1.0, breaking changes
raise the minor version (0.1 → 0.2). Changes are in [CHANGELOG.md](CHANGELOG.md).
Items hidden from the API documentation (`#[doc(hidden)]`) serve the
investigation tools and are not covered. Enums of values read from files are
`#[non_exhaustive]`, so new values are not breaking changes.

## Library

```rust
use csp_format::ClipFile;

let file = ClipFile::open("page0001.clip")?;
for (layer, depth) in file.layers.flatten() {
    println!("{}{} ({})", "  ".repeat(depth), layer.name, layer.kind.name());
    if let Some((image, (x, y))) = file.layer_pixels(layer)? {
        std::fs::write(format!("{}.png", layer.main_id), image.to_png()?)?;
    }
}
file.save("copy.clip")?; // byte-identical while nothing was changed
```

API documentation: `cargo doc --open -p csp-format` (docs.rs once published).
Examples: [crates/csp-format/examples/](crates/csp-format/examples/) (listing and
exporting layers, rendering, making a work) and
[crates/csp-assets/examples/](crates/csp-assets/examples/) (listing sub tools).

## CLI

```sh
cspf inspect FILE            # structure summary (JSON); the page list of a .cmc
cspf layers FILE             # layer tree
cspf version FILE...         # which CSP generation saved it, what older versions lock
cspf check FILE...           # self-check of every part
cspf extract FILE OUT_DIR    # layer pixels and masks as PNG
cspf render FILE OUT.png     # composite the layers (default: the preview's size)
cspf new OUT.clip --size 1000x1400 --dpi 350 [--image PNG]
cspf new-work DIR NAME --trim 182x257 --dpi 600 --pages 8 --spreads --bind right --start left
cspf add-layer FILE PNG OUT [--name NAME] [--at X,Y]
cspf set FILE ID OUT --opacity 50 --blend 2
cspf frames FILE OUT_DIR     # animation frames as PNG
cspf asset help              # sub tools, auto actions, colour / gradient sets, materials
```

`cspf` without arguments lists every command. Input files are only read:
an output that is the same file as an input (by any path, symbolic link or
hard link) is refused before anything is written. The library's
`ClipFile::save` writes wherever it is told, so keeping inputs is the
caller's job.

## Layout

| Path | Contents |
|---|---|
| `crates/csp-format` | canvases and works: container, database, layers, pixels, vectors, text, .cmc, rendering, editing, self-check |
| `crates/csp-assets` | sub tools, auto actions, colour / gradient sets, shortcuts, materials |
| `crates/cspf` | CLI |
| `crates/csp-lab` | investigation and validation tools (not published) |
| `python/csp_format` | Python reader (standard library only) |
| `tools/` | standard-library Python tools for corpus statistics and cross-checks (they print structure only) |

## Real files

Artwork never goes into the repository (`.gitignore` excludes `.clip`,
`.cmc`, `.sut`). Tests use synthetic files built from the observed
structure; corpus runs keep only counts.

## Development

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
python -m unittest discover -s python/tests
```

Releasing: [docs/releasing.md](docs/releasing.md).

## License

MIT ([LICENSE](LICENSE))
