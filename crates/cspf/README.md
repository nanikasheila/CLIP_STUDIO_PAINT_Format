# cspf

Command line tool for CLIP STUDIO PAINT files, built on
[`csp-format`](https://crates.io/crates/csp-format) and
[`csp-assets`](https://crates.io/crates/csp-assets).

```sh
cargo install cspf      # or download a binary from the GitHub releases

cspf inspect FILE            # structure summary as JSON (.clip / .cmc)
cspf layers FILE             # layer tree
cspf extract FILE OUT_DIR    # stored layer pixels and masks as PNG
cspf render FILE OUT.png     # composite the layers
cspf check FILE...           # self-check of every part
cspf new-work DIR NAME --pages 8 --spreads --bind right   # a new comic work
cspf asset help              # sub tools, auto actions, colour / gradient sets, materials
```

`cspf` without arguments lists every command. Input files are only read;
output goes to the paths given. JSON output is for people and scripts to
read; its fields may change between minor versions until 1.0.

Not made or endorsed by CELSYS. CLIP STUDIO PAINT is a trademark of CELSYS, Inc.

## License

MIT
