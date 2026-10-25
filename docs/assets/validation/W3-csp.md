# W3・W4: CSP での確認

CLIP STUDIO PAINT EX（Windows、2026-10-25）で、`csp-assets` が書いたファイルを読み込み、表示と書き出し直したファイルを確かめた。
テストファイルはユーザーの素材フォルダーの外に置き、作業中は他の書類を開いていない。

## W3: 書いたファイルの読み込み

| ファイル | 作り方 | CSP の結果 |
|---|---|---|
| `w3_edit.sut` | CSP が書き出した `.sut` に `sut-set --name "W3 編集" Opacity=60 BrushSize=50.0` | 読み込めた。ブラシサイズ 50、絵の具量 100、絵の具濃度 55（元の値）。CSP で書き出し直すと `Opacity` 60・`BrushSize` 50 のまま |
| `w3_gpen_extract.sut` | CSP 同梱の `EditImageTool.todb` から `sut-extract` で「Gペン」 | 読み込めた。サイズ 10、不透明度 100、アンチエイリアス「中」（`AntiAlias` 2） |
| `w3_zara_nomat.sut` | 同じく「ざらつきペン」（同梱素材なし） | 読み込めた。サイズ 12、不透明度 100、アンチエイリアス「弱」（`AntiAlias` 1） |
| `w3_actions.laf` | CSP が書き出した `.laf` を `dump` し、セット名「W3 アクション」、アクション 1 つ（「レイヤーを複製」をオフ、「不透明度の変更」を 47）にして `undump` | 「セットの読み込み」で新しいセットとして追加され、名前と項目は合っていたが、**オフにした項目がオンになった**（下記） |
| `w3_actions2.laf` | 直した書き込みで同じものを書き直し | オフの項目はオフのまま読み込めた。CSP で書き出し直すと、違いは `actionUuid`（振り直し）と `actionOpenArray`（パレットで開いた）だけ |
| `w3_kinds.laf` | サンプルにある 37 種類の項目を 1 つずつ並べた 1 アクション（すべてオフ） | 読み込めた。パレットに出た各項目の名前を [`action-kinds.tsv`](../spec/action-kinds.tsv) の `csp_name` に入れた（UUID の 2 つはプラグインのフィルター「放射ぼかし」「ガウスぼかし」） |
| `w3_gradients.sgs` | `gradient` で 1 セット 1 グラデーション | 読み込めない: CSP に `.sgs` をファイルから読み込む操作がない（下記） |
| 同梱の `default.sgs`（読むだけ） | `cspf asset gradients` の値を、「グラデーションの編集」で同じセットを選んだ表示と比べた | 「空」の「昼空」「青空」、「効果」の「描画色→無色」で、点の数・位置・不透明度・色の種類・混合率曲線の点・混色モードが一致。セットの並び（くすんだ陰影、空、効果、空の色グラデーションセット）も一致 |
| `w3_colors.cls` | `colorset-new` で 6 色（名前付き 4 色、名前なし 1 色、透明色 1 色） | 「カラーセットの読み込み」で新しいセット「W3 カラーセット」として追加され、6 色が並んだ。CSP の「カラーセットの書き出し」で書き出した `.cls` は元のファイルと**バイト単位で一致** |

わかったこと:

- **サブツールの名前は読み込んだファイルの名前になる**。`Node.NodeName`（「W3 編集」など）は使われない。書き出すときは CSP が `NodeName` にサブツール名を入れる。
  `sut-extract` で作った `.sut` は、表示したい名前をファイル名にする。
- CSP は読み込んだ `.sut` を今の形式にそろえる。書き出し直すと、影響元設定のヘッダーが 44 バイトから 48 バイト（速度のグラフの大きさ付き）になり、
  ファイルになかった列（`Dual*`、水彩境界、`BrushPatternNameVersion` など）が既定値で入る。`BrushPatternReverse` と `TextureScale` は消える。
- 素材の参照は、読み込み時にユーザーの素材フォルダーへ登録し直される（`.:56:16:<uuid>` が別の `.:<2 桁>:<2 桁>:<uuid>` に、同梱素材は `.:Install2:` 付きに）。
- **真偽は 1 バイト**。CSP は偽を `01 00` と書き、0 バイトの真偽を真と読む。`csp-assets` はほかの数と同じく最小の幅（偽は 0 バイト）で書いていたので、
  `set_enabled(false)` や JSON の `{"bool": false}` から書いた項目が CSP でオンになった。真偽はいつも 1 バイトで書くように直した（Rust・Python とも。
  これまでのサンプルはすべて 1 バイトで、読み書きのバイト一致は変わらない）。
- オートアクションの書き出し・読み込みはどちらも `.laf`（「オートアクションセットファイル」）。読み込むと新しいセットとして追加され、`actionUuid` は振り直される。
- グラデーションセットは、ツールプロパティ・サブツール詳細・「グラデーションの編集」のどこにも `.sgs` のファイル読み込みがない。
  「グラデーションの読み込み」が読むのは `.cgs` と `.grd` で、セットは素材として登録・追加する。設定フォルダーへ直接置く方法はユーザーの設定を変えるので試していない。
- CSP はページ（作品）を開いている間はサブツールを削除できない（「開いているページがあるため実行できません」）。カラーセットの削除は「カラーセットの編集」の「削除」で、確認なしに消える。試験で追加したサブツールとカラーセットは、確認後に CSP から削除した。

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
| 5 | 散布効果: オン | `BrushUseSpray` 0 → 1 |
| 5 | 散布効果: 粒子サイズ 7 | `BrushSpraySize` 2.0 → 7.0 |
| 5 | 散布効果: 粒子密度（バーで 2 段目） | `BrushSprayDensity` 5 → 2 |
| 5 | 散布効果: 散布偏向（バーで 4 段目） | `BrushSprayBias` 0 → 50 |
| 5 | 散布効果: 粒子の向き 41 | `BrushRotationInSpray` 0.0 → 41.0 |
| 6 | 紙質: なし → インパスト | `TextureImage` に素材の参照 1 件（種類 2）。素材のファイルも `.sut` に入り、37 KB → 590 KB |
| 7 | 紙質: 紙質濃度 37 | `TextureDensity` 50 → 37 |
| 7 | 紙質: 濃度反転 オン | `TextureReverseDensity` 0 → 1 |
| 7 | 紙質: 拡大率 150 | `TextureScale2` 100 → 150 |
| 7 | 紙質: 回転角 23 | `TextureRotate` 0 → 23 |
| 7 | 紙質: 明るさ 11 | `TextureBrightness` 0 → 11 |
| 7 | 紙質: コントラスト 19 | `TextureContrast` 0 → 19 |
| 7 | 紙質: 紙質適用方法 比較 | `TextureCompositeMode` 0 → 3（一覧の 4 番目。通常, 乗算, 減算, 比較, 輪郭, オーバーレイ, 覆い焼きカラー, 焼き込みカラー, ハードミックス, 高さ） |
| 7 | 紙質: 描点ごとに適用 オン | `TextureForPlot` 0 → 1 |
| 8 | 入り抜き: 影響先 ブラシサイズと厚さ | `BrushInOutTarget` の項目 1001 と 1051 の「有効」が 0 → 1（最小値は 0 のまま） |
| 8 | 入り抜き: 入り 13、抜き 31（長さ指定） | `BrushInLength` 20 → 13、`BrushOutLength` 20 → 31 |
| 8 | 入り抜き: 速度による入り抜き オフ | `BrushInOutBySpeed` 1 → 0 |
| 8 | 入り抜き: 指定方法 パーセント指定 | `BrushInOutType` 0 → 1。表示は `BrushInRatio` / `BrushOutRatio` の 30.0 |
| 9 | 水彩境界: オン、範囲 7 | `BrushUseWaterEdge` 0 → 1、`BrushWaterEdgeRadius` 3 → 7 |
| 9 | 水彩境界: 透明度影響（バーで 1 段目）、明度影響（バーで 4 段目） | `BrushWaterEdgeAlphaPower` 20 → 5、`BrushWaterEdgeValuePower` 0 → 70 |
| 9 | 水彩境界: ドラッグ後に処理 オン、ぼかし幅 5 | `BrushWaterEdgeAfterDrag` 0 → 1、`BrushWaterEdgeBlur` 0 → 5 |
| 9 | 水彩境界: フチの表現 ビビッド、強さ（2 段目）、鮮やかさ（5 段目） | `BrushWaterEdgeStyle` 0 → 1、`BrushWaterEdgeStrength` 100 → 40、`BrushWaterEdgeVividness` 50 → 100。ビビッドでは透明度影響・明度影響・ぼかし幅が無効になるが、値は残る |
| 10 | 色の変化: ブラシ先端色の変化 オン、色相 11、彩度 13、明度 17、サブ描画色混合率 19 | `BrushChangePatternColor` 0 → 1、`BrushHueChange`・`BrushSaturationChange`・`BrushValueChange`・`BrushSubColor` が同じ値 |
| 10 | 色の変化: ストロークごとのランダム変化 オン、色相 23、彩度 29、明度 31、サブ描画色混合率 37 | `BrushChangeStrokeColor` 0 → 1、`BrushStroke{Hue,Saturation,Value}Change`・`BrushStrokeSubColor` が同じ値 |
| 10 | 色の変化: 変化対象 サブ描画色 | `BrushChangeColorTarget` 1（メイン描画色）→ 2。一覧はメイン描画色とサブ描画色、メイン描画色、サブ描画色の順 |

2〜10 回目ではほかの列は変わらなかった。9 回目からは 8 回目の書き出しを読み込み直した一時ツールを使った（読み込み直しで変わったのは名前と ID だけ）。1 回目は元のファイル（CSP の読み込み前）と比べたので、上の形式のそろえ直し
（`BrushPatternNameVersion` 0 → 1、`BrushPatternReverse` と `TextureScale` の削除、ヘッダー 48 バイト）も出た。結果は `tools/assets/subtool_columns.py` の `CONFIRMED` に入れ、
[`subtool-columns.tsv`](../spec/subtool-columns.tsv) では「確認済み」と示す（530 列のうち 59 列）。

## 残り

- W4 の続き（デュアルブラシ、影響元設定と、ペン以外のツール）
