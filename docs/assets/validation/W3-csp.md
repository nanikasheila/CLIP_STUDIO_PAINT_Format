# W3・W4: CSP での確認

CLIP STUDIO PAINT EX（Windows、2026-10-25）で、`csp-assets` が書いたファイルを読み込み、表示と書き出し直したファイルを確かめた。
テストファイルはユーザーの素材フォルダーの外に置き、作業中は他の書類を開いていない。

## W3: 書いたファイルの読み込み

| ファイル | 作り方 | CSP の結果 |
|---|---|---|
| `w3_edit.sut` | CSP が書き出した `.sut` に `sut-set --name "W3 編集" Opacity=60 BrushSize=50.0` | 読み込めた。ブラシサイズ 50、絵の具量 100、絵の具濃度 55（元の値）。CSP で書き出し直すと `Opacity` 60・`BrushSize` 50 のまま |
| `w3_gpen_extract.sut` | CSP 同梱の `EditImageTool.todb` から `sut-extract` で「Gペン」 | 読み込めた。サイズ 10、不透明度 100、アンチエイリアス「中」（`AntiAlias` 2） |
| `w3_zara_nomat.sut` | 同じく「ざらつきペン」（同梱素材なし） | 読み込めた。サイズ 12、不透明度 100、アンチエイリアス「弱」（`AntiAlias` 1） |
| `w3_colors.cls` | `colorset-new` で 6 色（名前付き 4 色、名前なし 1 色、透明色 1 色） | 「カラーセットの読み込み」で新しいセット「W3 カラーセット」として追加され、6 色が並んだ。CSP の「カラーセットの書き出し」で書き出した `.cls` は元のファイルと**バイト単位で一致** |

わかったこと:

- **サブツールの名前は読み込んだファイルの名前になる**。`Node.NodeName`（「W3 編集」など）は使われない。書き出すときは CSP が `NodeName` にサブツール名を入れる。
  `sut-extract` で作った `.sut` は、表示したい名前をファイル名にする。
- CSP は読み込んだ `.sut` を今の形式にそろえる。書き出し直すと、影響元設定のヘッダーが 44 バイトから 48 バイト（速度のグラフの大きさ付き）になり、
  ファイルになかった列（`Dual*`、水彩境界、`BrushPatternNameVersion` など）が既定値で入る。`BrushPatternReverse` と `TextureScale` は消える。
- 素材の参照は、読み込み時にユーザーの素材フォルダーへ登録し直される（`.:56:16:<uuid>` が別の `.:<2 桁>:<2 桁>:<uuid>` に、同梱素材は `.:Install2:` 付きに）。
- オートアクション（`.las`）とグラデーションセット（`.sgs`）の読み込みは未確認。

## W4: 1 項目ずつ変えた差分

読み込んだ `w3_gpen_extract` をサブツール詳細で変え、そのつど「ツールの書き出し」で `.sut` にして、前の書き出しと設定を比べた（`cspf asset settings`）。
1 回に変えたのは、値の重ならない数値の項目と、オン・オフの項目 1 つ。

| 回 | CSP で変えた項目 | 変わった列 |
|---|---|---|
| 1 | ブラシサイズ: 画面上のサイズで指定 オン | `BrushSizeSyncViewScale` 0 → 1 |
| 1 | インク: 不透明度 37 | `Opacity` 100 → 37 |
| 1 | インク: 合成モード ハードミックス | `CompositeMode` 0 → 20（`.clip` の合成モード番号と同じ） |
| 2 | 補正: 角をとがらせる オン | `BrushSharpenCorner` 0 → 1 |
| 2 | 補正: 手ブレ補正 23 | `FlickerReduction` 6 → 23 |
| 2 | 補正: 補正タイプ 右のボタン | `FlickerReductionBySpeedType` 1 → 0 |
| 3 | 補正: ベクター吸着 オン | `BrushUseVectorMagnet` 0 → 1 |
| 3 | ブラシ先端: 硬さ 2 段目 | `BrushHardness` 100 → 30 |
| 3 | ブラシ先端: 厚さ 71 | `BrushThickness` 100 → 71 |
| 3 | ブラシ先端: 適用方向 水平 | `BrushVerticalThicknes` 1 → 0 |
| 3 | ブラシ先端: 向き 33 | `BrushRotation` 0 → 33 |
| 3 | ブラシ先端: ブラシ濃度 88 | `BrushFlow` 100 → 88 |
| 4 | ストローク: 間隔 固定 | `BrushAutoIntervalType` 2（普通）→ 0（固定） |
| 4 | ストローク: 連続吹き付け オン | `BrushContinuousPlot` 0 → 1 |

2〜4 回目ではほかの列は変わらなかった。1 回目は元のファイル（CSP の読み込み前）と比べたので、上の形式のそろえ直し
（`BrushPatternNameVersion` 0 → 1、`BrushPatternReverse` と `TextureScale` の削除、ヘッダー 48 バイト）も出た。結果は `tools/assets/subtool_columns.py` の `CONFIRMED` に入れ、
[`subtool-columns.tsv`](../spec/subtool-columns.tsv) では「確認済み」と示す（530 列のうち 18 列）。

## 残り

- `.las` と `.sgs` を CSP で読み込む確認
- W4 の続き（入り抜き、散布効果、紙質、水彩境界、色の変化と、ペン以外のツール）
