# 既存リポジトリからの移行

name-studio（Rust の `clip-core`）と csp-name-splitter（Python の `name_splitter/csp`）は、
それぞれ CSP 形式の読み書きを自前で持っている。どちらも csp-name-splitter の Python 実装が起点で、
このリポジトリの `csp-format`（Rust）と `csp_format`（Python）はその規則を実ファイル 719 件で確かめ、広げたものにあたる。

この文書は、形式の部分をこのリポジトリへ寄せ、アプリの部分（ネームの分割・配置・書き出しの流れ）を各リポジトリに残すための手順をまとめる。

## 1. 方針

| 層 | 置き場所 | 例 |
|---|---|---|
| 形式（読む・検査する・書き戻す） | このリポジトリ | コンテナ、DB、オフスクリーン、ベクター・テキストの解読、.cmc のページ一覧、自己検査 |
| アプリの規則 | 各リポジトリ | コマ枠からのページ検出、配置計画、ネームの分割、台詞の更新、書き出しの記録 |

一度に置き換えず、読み取りから順に差し替える。差し替えた箇所は、同じファイルで旧実装と新実装の結果を比べてから旧実装を消す（このリポジトリの `tools/validation/name_splitter_check.py` がその比較の例）。

## 2. 依存の追加

Rust（name-studio）:

```toml
# Cargo.toml（workspace.dependencies か crates/clip-core）
csp-format = { git = "https://github.com/nanikasheila/CLIP_STUDIO_PAINT_Format", rev = "<main のコミット>" }
```

両者とも `rusqlite 0.40`（`bundled`, `serialize`）と `png 0.18` を使うので、依存の重複は増えない。
`flate2` は csp-format が C の zlib（`libz-sys`、static）を使う。CSP と同じ圧縮結果でブロックをバイト一致で書き戻すためで、ビルドに C コンパイラが要る。

Python（csp-name-splitter）:

```sh
pip install "csp-format @ git+https://github.com/nanikasheila/CLIP_STUDIO_PAINT_Format#subdirectory=python"
```

`pyproject.toml` の `dependencies` に同じ指定を書く。`csp_format` は標準ライブラリだけで動く（Python 3.10 以上）。Pillow は不要になるが、
アプリ側の画像処理に使っているなら残す。

## 3. name-studio（clip-core → csp-format）

### 3.1 読み取り

| clip-core | csp-format | 注意 |
|---|---|---|
| `ClipDocument::read(path)` | `ClipFile::open(path)` | 保存（`to_bytes` / `save`）まで同じ型で扱える |
| `ClipDocument.canvas: ClipCanvas` | `ClipFile.canvas: Canvas` | 単位は `Unit` 列挙型。仕上がり枠・基本枠は `canvas.row` の `CropFrame*` 列（§3.4） |
| `ClipDocument::layers()` | `file.layers.flatten()` | `(layer, depth)` の組。根を含む |
| `find_layer(id)` | `file.layers.get(id)` | |
| `layer_row(id)` / `database()` | `layer.row` / `file.database()` | `Row` は列名で引ける（`int`, `real`, `text`, `blob`, `is_set`, `truthy`） |
| `ClipLayer.kind: LayerKind`（9 種） | `Layer.kind: LayerKind`（13 種） | §3.3 |
| `ClipLayer.opacity`（0.0–1.0） | `layer.opacity_fraction()` | `layer.opacity` は 0–256 の生の値 |
| `ClipLayer.composite` | `layer.blend` | 合成モードの番号。`render::BlendMode::from_code` で名前に |
| `pixel_origin()` | `layer.pixel_origin()` | 同じ式 |
| （なし） | `layer.mask_origin()` | マスクの位置は `LayerMaskOffset` も足す（clip-core は足していない） |
| `render_layer_pixels(layer)` | `file.layer_pixels(layer)` | `(RgbaImage, origin)` を返す |
| `render_layer_scaled(layer, n)` | `file.mipmap_base(id)?.to_rgba_halved(n)` | 半分にする回数で指定 |
| `layer_pixels_in_box` / `occupied_pixel_boxes` | `file.layer_pixels_in_box(layer, box)` / `file.stored_pixel_boxes(layer)` | 箱が重なるブロックだけを展開する。端のブロックはオフスクリーンの大きさで切る（旧実装は 256 の倍数） |
| （なし） | `file.layer_mask(layer)`, `file.layer_thumbnail(layer)` | |
| `vector_payloads` / `vector_objects` | `VectorObjectList` の `VectorData` を `file.external(id)` → `VectorData::parse` | 未解読部分を含めて全バイトを保持し、`to_bytes` で書き戻せる |
| `text_blocks` / `text_strings` | `text::read_blocks(&layer.row)` | 本文と `Attributes`（`font_size`, `bbox`, `font`, `quad` …） |
| `CmcProject::read` / `page_file_paths` | `CmcFile::open` / `page_paths(folder)` | ページ順は `CanvasNode` の連結リスト順 |
| `inspect_file` | `cspf inspect` と同じ JSON（`cspf` の `inspect`） | |
| `RgbaImage` | `csp_format::RgbaImage` | `pixels` は RGBA 非乗算。`resize_area`, `halve`, `paste`, `draw_over`, `to_png` |

### 3.2 書き込み

| clip-core | csp-format | 注意 |
|---|---|---|
| `ClipEditor::open(template)` | `ClipFile::open(template)` | 開いたファイルをそのまま編集して `save` |
| `renew_ids()` | `file.renew_file_id()`（レイヤーの UUID は `edit::new_uuid`） | 別名で保存したファイルには新しいファイル ID が要る（同じ ID だと CSP が開いている元のファイルの表示を使い回す） |
| `update_layer(id, values)` | `set_layer_value` / `set_layer_name` / `set_layer_opacity` / `set_layer_blend` / `set_layer_visible` / `set_layer_clipping` | |
| `set_layer_image(id, image, pos)` | `file.set_layer_pixels(id, &image, pos)` | ミップマップとサムネイルも作り直す |
| `add_layer(...)` | `file.add_layer(parent, NewLayer::Raster \| Folder, name)` / `add_raster_layer` | |
| `clear_layer_image` | `set_layer_pixels` に透明画像 | |
| （なし） | `file.remove_layer(id)`, `file.update_preview()` | |
| `assemble_container` | `ContainerWriter`（`ClipFile::to_bytes` が使う） | |
| `encode_offscreen_image` / `image_to_blocks` | `offscreen::encode` | CSP と同じ zlib で、元のブロックとバイト一致 |
| `transform_vector_data` | `VectorObject::transform(scale, dx, dy)` | 点・外接矩形・ブラシサイズ・104 バイト点の制御点も動かす |
| `transform_text_attributes` | `Attributes::transform` | |
| `write_cmc(template, pages, ...)` | `CmcFile::add_page` / `reorder` / `remove_page` / `save` | ひな形からの新規作成は csp-format にまだない（§5） |
| `verify::check_clip` | `verify::check_clip(data, options)` / `cspf check` | コンテナ・DB・画素の再符号化・ベクター・テキスト・参照まで調べる |
| `verify::verify_work` | `verify::check_cmc(data, Some(folder))` | |
| `RowCopier`（他ファイルの行の複製）、`merge_schema` | なし | §5 |
| `NewVectorLine` / `build_vector_data` / `NewFrame` / `FrameTemplate` | なし（`VectorObject` を組み立てて `VectorData::to_bytes`） | §5 |
| `NewText` / `retarget_*` / `text_color` | `Attributes::set` / `set_bbox` / `clear_render_offscreen` | 本文を変えたら `clear_render_offscreen` で CSP に描き直させる |
| `subtool::read_sub_tools`（.sut） | なし | ブラシの設定は .clip 内の `BrushStyle` を `stroke` が読む。.sut はアプリ側に残す |

### 3.3 レイヤー種別の違い

csp-format の判定順は `03-canvas-and-layers.md` の通りで、clip-core と次の点が違う。

- clip-core は `LayerType == 1` を最初にラスターと判定する。csp-format は先にフォルダー・テキスト・グラデーション・色調補正・3D・画像素材・ベクターを調べ、最後に `LayerType & 1` でラスターとする。
- clip-core の `other` は csp-format では `gradient`, `correction`, `image_material`, `3d` に分かれ、何も描いていないベクターレイヤー（`LayerType` 0、`VectorNormalStrokeIndex` あり）は `vector` になる。
- `match` を書いている箇所は、`other` 扱いだった種別をどう扱うか決めてから移す。ネームの分割で「画素として運ぶ」対象は `layer_pixels` が `Some` を返すレイヤーと考えるのが安全（グラデーション・画像素材も描画済みの画素を持つ）。

csp-name-splitter の読み取りと種別が違ったレイヤーの数（標本 24 件）は [V7-python.md](validation/V7-python.md) 2 章にある。

### 3.4 取りこぼしていた形式

csp-format に移すと次が読める。clip-core（と元の Python 実装）は対応していない。

- 2 値（モノクロ）レイヤー: 1 ビット/画素の平面（`alpha_row_bytes` 32）。clip-core は 1 バイト/画素として読むので画素が崩れる
- `InitColor`: 保存されていないブロックを白で埋めるオフスクリーン（用紙・マスクに多い）。clip-core では透明になる
- レイヤーマスク: 読み出しと位置（`LayerMaskOffset` も足す）
- 色調補正・グラデーション・画像素材・3D の区別（§3.3）、レイヤーのサムネイル

ファイル ID の更新（`renew_ids`）と `ExternalID` を TEXT で入れる処理は、clip-core・name_splitter とも既に同じ扱いになっている。

## 4. csp-name-splitter（name_splitter/csp → csp_format）

### 4.1 読み取り

| name_splitter.csp | csp_format | 注意 |
|---|---|---|
| `read_clip(path)` → `ClipDocument` | `read_clip(path)` → `ClipFile` | `with` で閉じる（メモリー上の DB を持つ） |
| `doc.canvas`（`ClipCanvas`） | `clip.canvas`（`Canvas`） | `width_px`, `height_px`, `crop_size_px`, `basic_size_px`, `basic_offset_px`, `single_page_width_px` は同名。`is_double_page` → `double_page`、`unit` は `"px"` などの文字列で同じ |
| `doc.root` / `iter_layers()` / `find_layer(id)` | `clip.root` / `clip.iter_layers()` / `clip.find_layer(id)` | 同じ順（深さ優先、下から上） |
| `ClipLayer.offset_x/y`, `render_offset_x/y` | `layer.offset`, `layer.render_offset`（組） | `pixel_origin` は同じ |
| `ClipLayer.composite` | `layer.blend` | |
| `render_layer_pixels(layer)` → PIL か None | `clip.layer_pixels(layer)` → `(Image, origin)` か None | PIL へは `Image.frombytes("RGBA", (w, h), bytes(image.rgba))` |
| `layer_pixels_in_box(layer, box)` | `clip.layer_pixels_in_box(layer, box)` | 同じ意味（キャンバス座標、外は透明） |
| `occupied_pixel_boxes(layer)` | `clip.stored_pixel_boxes(layer)` | 端のブロックはオフスクリーンの大きさで切る（旧実装は 256 の倍数） |
| `layer_pixel_parameters(layer)` | `offscreen.Attribute.parse(attr).parameter` | |
| `vector_payloads` / `vector_objects` | 同名 | `VectorObject` は全バイトを保持。座標は `coordinates`、点の値は `points` |
| `text_blocks(layer)` → `TextAttributes` | `clip.text_blocks(layer)` → `TextBlock(text, attributes)` | 本文と属性が組になる |
| `preview_png` | `clip.preview_png()` / `clip.preview()` | |
| `read_cmc(path)` / `page_file_paths()` | 同名 | `pages()` は `CanvasNode` の列。`memo`, `spread`（PageFlag）, `relative_path` |
| `CmcProject.default_page` | `work.project`（`page_width` など） | |

### 4.2 書き込み

`clip_editor.py`, `page_writer.py`, `cmc_writer.py`, `offscreen_writer.py` は Python 版を作っていない。
書き込みはこのリポジトリの `cspf`（`new`, `add-layer`, `add-folder`, `set`, `remove-layer`）を呼ぶか、Rust の csp-format を使う。
読み取りを `csp_format` に替え、書き込みは当面いまの `clip_editor.py` などを残す、という順で移せる（両者は同じ DB の行を扱うので、`clip.database` の内容はそのまま使える）。

## 5. このリポジトリに足すと移行が楽になるもの

移行の途中で必要になったら、ここに足してから各リポジトリで使う。

- Rust: 他のファイルからレイヤーを行ごと複製する機能（clip-core の `RowCopier` / `merge_schema`）
- Rust: 線・コマ枠のベクターデータを組み立てる関数（clip-core の `build_vector_data` / `build_frame_data`）
- Rust: ひな形からの .cmc 新規作成（clip-core の `write_cmc`）
- Python: 書き込み

## 6. 確かめ方

1. 置き換えの前後で同じファイルを読み、結果を比べる。Python は `tools/validation/name_splitter_check.py`、Rust は `crates/csp-format/examples/layer_digest.rs` の出力と旧実装の出力を比べる。
2. 書き込みは `cspf check` で自己検査し、CSP で開いて確かめる（V6 の手順）。
3. 実ファイルでの比較結果は [V7-python.md](validation/V7-python.md)。
