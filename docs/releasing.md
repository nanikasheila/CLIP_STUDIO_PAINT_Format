# 公開の手順

対象は Rust の `csp-format`・`csp-assets`・`cspf`（crates.io）と Python の `csp-format`（PyPI）。
`csp-lab` は公開しない（`publish = false`）。

## 1. 版を決めて書き換える

[Semantic Versioning](https://semver.org/lang/ja/) に従う。1.0 までは、互換性のない変更（公開している型・関数の削除や引数の変更）は 2 桁目を上げる（0.1.x → 0.2.0）。
API 文書に出ていない項目（`#[doc(hidden)]`）の変更は互換性の対象外。

書き換える場所:

- `Cargo.toml` の `[workspace.package] version` と `[workspace.dependencies]` の `version`（同じ値）
- `python/pyproject.toml` の `version` と `python/csp_format/__init__.py` の `__version__`
- `CHANGELOG.md`: `[Unreleased]` の内容を新しい版の節に移し、日付と末尾のリンクを書く

## 2. 確かめる

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p csp-format -p csp-assets
cargo package --workspace --exclude csp-lab   # 公開されるファイルだけでビルドできるか
python -m unittest discover -s python/tests
```

CI（`.github/workflows/ci.yml`）も同じことを Linux・Windows・macOS と最低 Rust 版（1.88）で確かめる。

## 3. タグを打つ

main にマージしてから:

```sh
git tag v0.1.0
git push origin v0.1.0
```

`.github/workflows/release.yml` が GitHub のリリースを作り、`cspf` の実行ファイル（Linux x86_64 静的リンク、Windows x86_64、macOS arm64 / x86_64）と Python のパッケージ、`SHA256SUMS.txt` を付ける。

## 4. crates.io に出す（取り消せない）

crates.io に出した版は消せない（`cargo yank` で新しく使われないようにできるだけ）。
[crates.io](https://crates.io/) に GitHub アカウントでログインし、API トークンを作って `cargo login` してから、依存の順に出す:

```sh
cargo publish -p csp-format
cargo publish -p csp-assets
cargo publish -p cspf
```

docs.rs の API 文書は公開後に自動で作られる。

## 5. PyPI に出す（取り消せない）

PyPI も同じ版を上書きできない。[PyPI](https://pypi.org/) のアカウントと API トークンを用意して:

```sh
python -m pip install --upgrade build twine
python -m build python --outdir dist
python -m twine upload dist/*
```

リリースに付いた `csp_format-*.whl` / `.tar.gz` をそのまま上げてもよい。

## 公開後

README の「使い方」の git 指定（crates.io / PyPI に出すまでの書き方）を消す。
