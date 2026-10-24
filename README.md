# CLIP STUDIO PAINT Format

CLIP STUDIO PAINT のファイル（`.clip` / `.cmc`）を読み・検証し・書き戻すための Rust ライブラリ `csp-format` と CLI `cspf`。
形式は公開されていないため、実ファイルの観察から仕様を起こし、実ファイルでの検証で確かめている。

- 要求仕様: [docs/requirements.md](docs/requirements.md)
- 検証の記録: [docs/validation/](docs/validation/)（実ファイル 719 件の集計: [V1-corpus.md](docs/validation/V1-corpus.md)）

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
| .cmc（ページ一覧・作品設定） | ○ | ○ 並べ替え・追加・削除 | 42 件 |

作業中: 合成描画（R10）、スキーマ目録と仕様書（R3）、レイヤー編集・新規作成と CSP での確認（R12・R13・V6）。

## CLI

```sh
cargo build --release -p cspf

cspf inspect FILE            # 構造の要約（JSON）。.cmc はページ一覧
cspf layers FILE             # レイヤー一覧
cspf schema FILE             # ファイルが持つスキーマ登録（ParamScheme）
cspf check FILE...           # 自己検査（全項目）
cspf extract FILE OUT_DIR    # レイヤー画像とマスクを PNG で
cspf preview FILE OUT.png    # 保存されたプレビュー
cspf corpus ROOT OUT.jsonl   # フォルダー内の全ファイルを検査（再開可能）
```

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
| `crates/csp-format/src/verify.rs` | 自己検査（コーパス検証と書き出し後の検査に使う） |
| `crates/cspf` | CLI |
| `tools/corpus` | 標準ライブラリだけの Python 集計ツール（構造だけを出力） |

## 実ファイルの扱い

作品ファイルはリポジトリに入れない（`.gitignore` で `.clip` / `.cmc` / `.sut` を除外）。
テストは観察した構造の最小構成を合成して使う。コーパス検証の結果は件数の集計だけを残す。

## ライセンス

MIT
