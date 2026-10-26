# csp_format (Python)

Read CLIP STUDIO PAINT files (`.clip`, `.cmc`) and its sub tools, auto
actions, colour and gradient sets and materials (`csp_format.assets`) with
the Python standard library only. It follows the same rules as the Rust
crate `csp-format`, and both give the same results on 719 real files. For
writing and rendering, use the Rust crates or the `cspf` command. Not made
or endorsed by CELSYS. Documentation: [GitHub](https://github.com/nanikasheila/csp-format);
the [specification](https://github.com/nanikasheila/csp-format/blob/main/docs/spec/README.md) lists what is not yet understood.

CLIP STUDIO PAINT のファイル（`.clip` / `.cmc`）と、サブツール・オートアクション・カラーセット・グラデーションセット・素材（`csp_format.assets`）を Python の標準ライブラリだけで読む実装。
規則は Rust の `csp-format` と同じで、実ファイル 719 件で両者の結果が一致することを確かめている（[V7](https://github.com/nanikasheila/csp-format/blob/main/docs/validation/V7-python.md)、assets は [W5](https://github.com/nanikasheila/csp-format/blob/main/docs/assets/validation/W5-python.md)）。
書き込みと合成描画は Rust 側（`cspf`）を使う。CELSYS の公式のものではない。
仕様と未解明の項目は [仕様書](https://github.com/nanikasheila/csp-format/blob/main/docs/spec/README.md#未解明の項目)。

```sh
pip install csp-format
```

```python
from csp_format import read_animation, read_clip, read_cmc

with read_clip("page.clip") as clip:
    print(clip.canvas.width_px, clip.canvas.height_px)
    for layer, depth in clip.root.walk():
        print("  " * depth, layer.kind, layer.name)
        found = clip.layer_pixels(layer)          # (Image, (x, y)) か None
        if found:
            image, origin = found
            open(f"{layer.main_id}.png", "wb").write(image.to_png())
        for item in clip.vector_objects(layer):   # 線・図形（レイヤー座標）
            print(item.brush_size, len(item.raw_points))
        for block in clip.text_blocks(layer):     # テキスト（吹き出しごと）
            print(block.text, block.attributes.font_size)

with read_clip("anim.clip") as clip:
    animation = read_animation(clip)              # タイムラインがなければ None
    timeline = animation.timelines[animation.current]
    state = animation.frame_state(clip, animation.current, frame=0)   # フレームは 0 始まり
    print(timeline.name, timeline.frame_rate, state.cels)            # フォルダー → 表示するセル

with read_cmc("work.cmc") as work:
    for path in work.page_file_paths():
        print(path)
```

| 機能 | API |
|---|---|
| キャンバス設定（px、仕上がり枠・基本枠） | `clip.canvas` |
| レイヤーツリーと種別（13 種） | `clip.root`, `clip.iter_layers()`, `clip.find_layer(id)` |
| 画素（カラー・グレー・2 値・初期色） | `clip.layer_pixels(layer)`, `clip.layer_pixels_in_box(layer, box)`, `clip.stored_pixel_boxes(layer)` |
| マスク・サムネイル・プレビュー | `clip.layer_mask(layer)`, `clip.layer_thumbnail(layer)`, `clip.preview()` |
| ベクター・テキスト | `clip.vector_payloads(layer)`, `clip.vector_objects(layer)`, `clip.text_blocks(layer)` |
| 任意のテーブル | `clip.database`（メモリー上の複製）, `clip.row(table, id)` |
| .cmc のページ順・ページのサムネイル | `work.pages()`, `work.page_file_paths()`, `work.page_thumbnail(node)` |
| アニメーション（タイムライン・セル・キーフレーム・2D カメラ） | `read_animation(clip)`, `animation.frame_state(clip, index, frame)`（[08 章](https://github.com/nanikasheila/csp-format/blob/main/docs/spec/08-animation.md)） |
| トラックのキー（"cmt " 文書の読み書き） | `csp_format.animation.parse_document`, `document_bytes`, `unpack_chunk` |
| サブツール・オートアクション・カラーセット・グラデーションセット・ショートカット・素材 | `csp_format.assets`（`subtool`, `serial`, `colorset`, `shortcut`, `material`, `c2f`, `tar`） |

画像は `Image(width, height, rgba)`（RGBA、非乗算）。Pillow へは
`PIL.Image.frombytes("RGBA", (image.width, image.height), bytes(image.rgba))`。

テスト: リポジトリでは `python -m unittest discover -s python/tests`、sdist を展開した場所では `python -m unittest discover -s tests`。
