# V7 実装間の比較（Python の読み取り実装）

`python/csp_format`（標準ライブラリだけの読み取り実装、R16）を、Rust の `csp-format` と、置き換え元の csp-name-splitter の読み取り（`name_splitter.csp`）と比べた結果。
対象はユーザーの PC にある実ファイルで、元ファイルは読み取りだけ（制約 C2）。載せるのは件数だけで、画素・文字・レイヤー名・パスは含まない（N6）。

## 実施条件

| 項目 | 値 |
|---|---|
| 実施日 | 2026-10-25 |
| 対象 | V1 と同じ `.clip` 719 件（27,986 レイヤー） |
| 実行環境 | ユーザー PC 上の Linux VM、Python 3.10、2 並列 |
| Python と Rust | `tools/validation/python_reader_check.py ROOT LIST OUT --digest layer_digest [--shard K/N]` |
| Python と name_splitter | `tools/validation/name_splitter_check.py ROOT LIST OUT --name-splitter PATH [--shard K/N]`（Pillow が要る） |

## 1. Python と Rust

両方の実装で、ファイルごとに同じ要約を作って比べた（Rust は `crates/csp-format/examples/layer_digest.rs`）。

- キャンバスの大きさ、保存されたプレビューの画素の CRC-32
- レイヤーごとに: ID と木の深さ（並び順を含む）、種類、名前の CRC、表示、不透明度、
  保存された画素（大きさ・位置・RGBA の CRC）、マスク（灰色の RGBA の CRC）、サムネイルの CRC、ベクターのオブジェクト数と点の数、テキストのブロックごとの本文の CRC

**719 件すべてで一致（不一致 0、どちらかが読めないファイル 0）。** 27,986 レイヤーの画素・マスク・サムネイルが、両方の実装でバイト単位で同じになった。

| 項目 | 値 |
|---|---|
| 一致 | 719 / 719（テキストの読み方を直した後、テキストだけを比べ直して 719 / 719） |
| 1 件あたりの時間（両方の実装と比較の合計） | 中央値 3.4 秒、最大 122 秒、合計 9,718 秒 |

時間の大半は、キャンバス全体の大きさの画像を作ることと、比較のための CRC の計算。

## 2. Python と name_splitter

csp-name-splitter が使う項目（キャンバスの大きさと枠、レイヤーの木・名前・表示・不透明度・位置・マスク・クリッピング・合成モード、
保存された画素、ベクターのデータ、テキスト）を、標本（`--shard 0/16`、パスの並びで 16 件おきに選んだ 24 件）で比べた。
name_splitter の読み取りは Pillow でキャンバス全体の画像を作るので遅く、メモリーも多く使う（2 並列ではメモリー不足で止まった）。

| 結果 | 件数 | 内容 |
|---|---|---|
| 全項目一致 | 21 | |
| 用紙の画素が違う | 2 | csp_format は白、name_splitter は透明。用紙のオフスクリーンは画素のブロックを持たず、`InitColor`（白）で埋める（04 章）。csp_format が正しい（V3 で描画がプレビューと一致） |
| name_splitter が読めない | 1 | `VectorObjectList` の表がないファイルで、name_splitter の `vector_payloads` が SQL のエラーになる。csp_format は表がなければ空を返す |

種別は、name_splitter が `other` とするレイヤーを csp_format が細かく分ける（グラデーション 16、色調補正 14、何も描いていないベクター 4、画像素材 1）。
name_splitter が知っている種別（ラスター・フォルダー・テキスト・ベクターなど）どうしで違ったものは 0。

### 見つかった誤り（直した）

最初の比較で、文字のまとまりが複数あるテキストレイヤーの「まとまりの数」が 1 つ違った。調べると、1 つ目のまとまりは `TextLayerString` /
`TextLayerAttributes` にあり、配列（`…Array`）は 2 つ目以降を持つ（コーパスの配列を持つ 133 レイヤーすべてで、1 つ目の書式は配列のどれとも違い、
自分の外接矩形と描いた画像を持つ）。name_splitter の数え方が正しく、csp_format と Rust の `csp-format` は 1 つ目を読み落としていた。
両方を直し（`text.rs` / `text.py` の `read_blocks`）、合成したファイルのテストを足した。直した後、テキストだけを比べる `--text-only` で 719 件を
もう一度比べ、Python と Rust はすべて一致した。描画への影響は [V3](V3-render.md) 4 章。

## 再現の手順

```sh
cargo build --release -p csp-format --example layer_digest
python tools/validation/python_reader_check.py "$ROOT" LIST.txt out/v7_0.jsonl --digest target/release/examples/layer_digest --shard 0/2 &
python tools/validation/python_reader_check.py "$ROOT" LIST.txt out/v7_1.jsonl --digest target/release/examples/layer_digest --shard 1/2 &
wait
grep -c '"ok": true' out/v7_?.jsonl
```

どちらの道具も途中から再開できる（同じ出力に追記し、済んだファイルを飛ばす）。CI では合成したファイル（`python/tests/data/layers.clip`）で同じ比較を行う。
