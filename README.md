# CLIP STUDIO PAINT Format

CLIP STUDIO PAINT のファイル（`.clip` / `.cmc`）を読み・検証し・書き戻すための Rust ライブラリ `csp-format` と CLI `cspf`。
形式は公開されていないため、実ファイルの観察から仕様を起こし、実ファイルでの検証で確かめている。

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
| 新規作成（大きさ・解像度・用紙の色） | — | ○ | CSP で開いて確認（V6） |

描画で近似のもの・描かないもの（ベクターの線の形、CSP が描いた画像のないテキスト、一部のレイヤー効果、3D）は [仕様書の未解明の項目](docs/spec/README.md#未解明の項目) にある。

## CLI

```sh
cargo build --release -p cspf

cspf inspect FILE            # 構造の要約（JSON）。.cmc はページ一覧
cspf layers FILE             # レイヤー一覧
cspf schema FILE             # ファイルが持つスキーマ登録（ParamScheme）
cspf check FILE...           # 自己検査（全項目）
cspf extract FILE OUT_DIR    # レイヤー画像とマスクを PNG で
cspf preview FILE OUT.png    # 保存されたプレビュー
cspf render FILE OUT.png     # レイヤーを合成した画像（既定はプレビューの大きさ）
cspf compare FILE...         # 描画とプレビューの PSNR
cspf roundtrip FILE OUT      # 無変更で保存してバイト一致を確かめる
cspf new OUT.clip --size 1000x1400 --dpi 350 [--image PNG]   # 新規作成
cspf add-layer FILE PNG OUT [--name NAME] [--at X,Y]         # ラスターレイヤーを追加
cspf set FILE ID OUT --opacity 50 --blend 2                  # 属性の変更
cspf corpus ROOT OUT.jsonl [--render]   # フォルダー内の全ファイルを検査（再開可能）
```

すべてのコマンドは `cspf` を引数なしで実行すると表示される。

入力ファイルは読むだけで、変更しない。

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
| `crates/cspf` | CLI |
| `python/csp_format` | Python の読み取り実装（標準ライブラリだけ） |
| `tools/corpus`、`tools/validation` | 標準ライブラリだけの Python 集計・照合ツール（構造だけを出力） |

## 実ファイルの扱い

作品ファイルはリポジトリに入れない（`.gitignore` で `.clip` / `.cmc` / `.sut` を除外）。
テストは観察した構造の最小構成を合成して使う。コーパス検証の結果は件数の集計だけを残す。

## ライセンス

MIT
