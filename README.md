# CLIP STUDIO PAINT Format

[English](README.en.md)

CLIP STUDIO PAINT のファイル（`.clip` / `.cmc`）を読み・検証し・書き戻すための Rust ライブラリ `csp-format` と CLI `cspf`。
サブツール・オートアクション・カラーセット・グラデーションセット・素材は `csp-assets` で扱う。
形式は公開されていないため、実ファイルの観察から仕様を起こし、実ファイルでの検証で確かめている。

CELSYS の公式のものではなく、CELSYS の承認も受けていない。CLIP STUDIO PAINT は株式会社セルシスの商標。

- 要求仕様: [docs/requirements.md](docs/requirements.md)
- 仕様書: [docs/spec/](docs/spec/)（未解明の項目の一覧を含む）
- 検証の記録: [docs/validation/](docs/validation/)（実ファイル 719 件の集計: [V1-corpus.md](docs/validation/V1-corpus.md)、描画: [V3-render.md](docs/validation/V3-render.md)）
- Python の読み取り実装: [python/](python/README.md)（標準ライブラリだけ）
- 既存リポジトリ（name-studio / csp-name-splitter）からの移行: [docs/migration.md](docs/migration.md)

## できること

| 対象 | 読み取り | 書き戻し | 実ファイルでの確認 |
|---|---|---|---|
| コンテナ（CSFCHUNK のチャンク列） | ○ | ○ バイト一致 | 719 件 |
| 埋め込み SQLite（全テーブル）とスキーマ登録 | ○ | ○ | 719 件 |
| レイヤーツリー・キャンバス設定 | ○ | — | 27,986 レイヤー |
| ラスター画素（カラー / グレー / 2値 / マスク、ミップマップ） | ○ PNG へ | ○ CSP と同じ圧縮でバイト一致 | 186 万ブロック |
| ベクターデータ | ○ 点・矩形・色・太さ | ○ バイト一致（未解読部分も保持） | 117 万オブジェクト |
| テキスト（本文・属性） | ○ | ○ バイト一致 | 8,344 ブロック |
| 保存されたプレビュー | ○ PNG | — | 719 件 |
| .cmc（ページ一覧・作品設定） | ○ | ○ 並べ替え・追加・削除 | 42 件、CSP EX で開いて確認 |
| 合成描画（合成モード・クリッピング・マスク・効果・色調補正・グラデーション） | ○ PNG | — | 719 件をプレビューと比較（V3） |
| 編集（画素・名前・不透明度・合成モード・表示、レイヤーとフォルダーの追加・削除） | — | ○ | CSP で開いて確認（V6） |
| 新規作成（大きさ・解像度・用紙の色、作品と作品情報・ノンブル） | — | ○ | CSP で開いて確認（V6・V8） |
| レイヤーのファイル間コピー（ベクター・テキストを含む） | — | ○ | CSP EX で開いて確認（[移行の記録](docs/migration.md)） |
| アニメーション（タイムライン・セル・キーフレーム・2D カメラ） | ○ フレームごとの描画 | ○ フレームレート・再生範囲・セル | CSP の連番書き出しと比較（V6c） |
| サブツール・オートアクション・カラーセット・グラデーションセット・素材（`csp-assets`） | ○ | ○ バイト一致 | CSP EX で読み込んで確認（[W3](docs/assets/validation/W3-csp.md)） |

描画で近似のもの・描かないもの（ベクターの線の形、CSP が描いた画像のないテキスト、一部のレイヤー効果、3D）は [仕様書の未解明の項目](docs/spec/README.md#未解明の項目) にある。

## 使い方

最初の版（0.1.0）はまだ公開していない（crates.io・PyPI・GitHub のリリースはない）。それまでは git のコミットを指すか、手元でビルドする。

| 使い方 | 今の入れ方 | 0.1.0 の公開後 |
|---|---|---|
| Rust のライブラリ | `csp-format = { git = "https://github.com/nanikasheila/CLIP_STUDIO_PAINT_Format", rev = "コミット" }`（`csp-assets` も同じ） | `cargo add csp-format` / `cargo add csp-assets` |
| CLI | clone して `cargo build --release -p cspf`（または `cargo install --git https://github.com/nanikasheila/CLIP_STUDIO_PAINT_Format cspf`） | [リリース](https://github.com/nanikasheila/CLIP_STUDIO_PAINT_Format/releases)の実行ファイル（Windows / macOS / Linux）か `cargo install cspf` |
| Python（読み取りだけ） | `pip install "csp-format @ git+https://github.com/nanikasheila/CLIP_STUDIO_PAINT_Format@コミット#subdirectory=python"` | `pip install csp-format` |
| ほかの言語 | `cspf inspect` / `layers` / `anim` などの JSON 出力、`extract` / `render` の PNG 出力を使う |

Rust のビルドには C コンパイラが要る（SQLite と zlib を組み込む。CSP と同じ圧縮結果を出すには zlib の C 実装が要るため）。
最低 Rust 版は 1.88。

### 版の付け方

[Semantic Versioning](https://semver.org/lang/ja/) に従う。1.0 までは、互換性のない変更は 2 桁目（0.1 → 0.2）を上げる。
変更点は [CHANGELOG.md](CHANGELOG.md)。API 文書に出ていない項目（`#[doc(hidden)]`）は調査用で、互換性の対象外。
ファイルから読む値の列挙型は `#[non_exhaustive]` で、新しい値を足しても互換性は壊れない。

## CLI

```sh
cargo build --release -p cspf

cspf inspect FILE            # 構造の要約（JSON）。.cmc はページ一覧
cspf layers FILE             # レイヤー一覧
cspf schema FILE             # ファイルが持つスキーマ登録（ParamScheme）
cspf version FILE...         # 保存した CSP の世代と、古い版でロックされる機能
cspf check FILE...           # 自己検査（全項目）
cspf extract FILE OUT_DIR    # レイヤー画像とマスクを PNG で
cspf preview FILE OUT.png    # 保存されたプレビュー
cspf render FILE OUT.png     # レイヤーを合成した画像（既定はプレビューの大きさ）
cspf compare FILE...         # 描画とプレビューの PSNR
cspf roundtrip FILE OUT      # 無変更で保存してバイト一致を確かめる
cspf new OUT.clip --size 1000x1400 --dpi 350 [--image PNG]   # 新規作成
cspf new-work DIR NAME --trim 182x257 --dpi 600 --pages 8 --spreads --bind right --start left   # 作品（.cmc）の新規作成
cspf add-layer FILE PNG OUT [--name NAME] [--at X,Y]         # ラスターレイヤーを追加
cspf set FILE ID OUT --opacity 50 --blend 2                  # 属性の変更
cspf corpus ROOT OUT.jsonl [--render]   # フォルダー内の全ファイルを検査（再開可能）
```

すべてのコマンドは `cspf` を引数なしで実行すると表示される。

入力ファイルは読むだけで、変更しない。出力に入力と同じファイルを指定すると（相対・絶対パス、シンボリックリンク、ハードリンクのどれでも）書かずにエラーになる。
ライブラリの `ClipFile::save` は与えたパスに書くだけなので、入力を残すのは呼ぶ側の責任。

## ライブラリ

```rust
use csp_format::ClipFile;

let file = ClipFile::open("page0001.clip")?;
for (layer, depth) in file.layers.flatten() {
    println!("{}{} ({})", "  ".repeat(depth), layer.name, layer.kind.name());
    if let Some((image, (x, y))) = file.layer_pixels(layer)? {
        std::fs::write(format!("{}.png", layer.main_id), image.to_png()?)?;
    }
}
file.save("copy.clip")?; // 変更がなければ元とバイト一致
```

API 文書は `cargo doc --open -p csp-format`（crates.io に出した後は docs.rs）。例は
[crates/csp-format/examples/](crates/csp-format/examples/)（レイヤーの一覧と書き出し、描画、作品の新規作成）と
[crates/csp-assets/examples/](crates/csp-assets/examples/)（サブツールの一覧）。

## 構成

| パス | 内容 |
|---|---|
| `crates/csp-format/src/container.rs` | CSFCHUNK コンテナ |
| `crates/csp-format/src/database.rs` / `schema.rs` | 埋め込み SQLite、スキーマ登録 |
| `crates/csp-format/src/model.rs` | キャンバス・レイヤーツリー |
| `crates/csp-format/src/offscreen.rs` / `zlib.rs` | ラスター画素（Offscreen）、CSP と同じ設定の zlib |
| `crates/csp-format/src/vector.rs` / `text.rs` | ベクター・テキスト |
| `crates/csp-format/src/cmc.rs` | .cmc |
| `crates/csp-format/src/clip.rs` | ファイル全体（読み込み・変更の追跡・保存） |
| `crates/csp-format/src/render.rs` ほか | 合成描画（`material.rs` 画像素材、`tone.rs` トーン、`stroke.rs` ベクターの線、`gradient.rs`・`correction.rs`・`effect.rs`） |
| `crates/csp-format/src/edit.rs` / `create.rs` | 編集と新規作成 |
| `crates/csp-format/src/verify.rs` | 自己検査（コーパス検証と書き出し後の検査に使う） |
| `crates/csp-assets` | サブツール・オートアクション・カラーセット・グラデーションセット・ショートカット・素材（[docs/assets/](docs/assets/requirements.md)） |
| `crates/cspf` | CLI |
| `crates/csp-lab` | 調査と検証の道具（`examples/`、公開しない） |
| `python/csp_format` | Python の読み取り実装（標準ライブラリだけ） |
| `tools/corpus`、`tools/validation` | 標準ライブラリだけの Python 集計・照合ツール（構造だけを出力） |

## 実ファイルの扱い

作品ファイルはリポジトリに入れない（`.gitignore` で `.clip` / `.cmc` / `.sut` を除外）。
テストは観察した構造の最小構成を合成して使う。コーパス検証の結果は件数の集計だけを残す。

## 開発

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
python -m unittest discover -s python/tests
```

公開の手順は [docs/releasing.md](docs/releasing.md)。

## ライセンス

MIT（[LICENSE](LICENSE)）
