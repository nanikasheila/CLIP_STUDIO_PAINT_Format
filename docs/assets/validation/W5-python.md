# W5 Python と Rust の照合（2026-10-25）

標準ライブラリだけの Python 版（`python/csp_format/assets`）と Rust 版（`crates/csp-assets`）が、同じファイルから
同じものを読むかを `tools/validation/assets_python_check.py` で照合しました。比べるのは次のとおり（`examples/asset_digest.rs` と同じ形）。

- サブツール DB: ツリー（深さ・上位の名前・名前・種類・設定 ID・初期設定 ID）と、全設定行の全列（バイナリ列は解読した形）
- 型付き直列化: `cspf asset dump` の JSON の全体
- カラーセット: 名前・版・形式・全色（RGBA と名前）
- 素材: `catalog.xml`、`info.xml` / `layerData.xml`、C2F の外枠、`.cac` の文字列

## 結果

| 対象 | 照合したファイル | 違い |
|---|---:|---:|
| CSP 同梱の既定データ | 41 | 0 |
| 素材フォルダー | 32 | 0 |
| ユーザー設定 | 85 | 0 |
| 合成ファイル（CI で毎回） | 4 | 0 |

既定のツール DB 1 件だけで、ツリー 255 ノード・設定 406 行・値 47,881 個が一致しました。照合が違いを見逃さないことは、
値をわざと変えて検出されることで確かめています。サブツール DB でない SQLite（各種設定）は、両方が「サブツール DB ではない」と
判定することを照合しています（中身の照合は A2 の目録で行う）。

## 再現の手順

```sh
cargo build --release --example asset_digest -p csp-assets
python tools/validation/assets_python_check.py <フォルダー> w5.jsonl --digest target/release/examples/asset_digest
```

出力の各行はパスのハッシュと違いの一覧だけで、名前や値は含みません。
