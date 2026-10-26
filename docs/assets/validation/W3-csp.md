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
| 11 | ブラシサイズの影響元設定: 筆圧の最小値 21、筆圧のグラフに点を 1 つ（約 50%, 79%） | `BrushSizeEffector` の最小値 [0] 0 → 21、筆圧のグラフに点 (0.509, 0.791) |
| 11 | 同: 傾き（オンのまま）の最大値 133 | ヘッダーの語 [10] 150 → 133（語 [10] は傾きの最大値 %） |
| 11 | 同: 速度 オン・最小値 7、ランダム オン・最小値 43 | 使う入力 0x30 → 0xF0、最小値 [2] 0 → 7、[3] 0 → 43。速度のグラフ（大きさ 0）に既定の (0, 1)–(1, 0) が入る |
| 12 | 2-ブラシ形状: オン、適用方法 スクリーン | `UseDualBrush` 0 → 1、`DualBrushCompositeMode` 1（乗算）→ 6。一覧は通常, 乗算, 加算(発光), 減算, 比較(暗), 比較(明), スクリーン, オーバーレイ, 覆い焼きカラー, 焼き込みカラー, 焼き込み(リニア), ハードミックス, 高さ(リニア) |
| 12 | 2-ブラシ形状: RGB値に適用 オン、2-ブラシサイズ 17、メインのブラシサイズに連動 オン、2-アンチエイリアス 強 | `ChangeRGBByDual` 0 → 1、`DualSize` 30 → 17、`SyncDualBrushSize` 0 → 1、`DualAntiAlias` 2 → 3 |
| 13 | 2-ブラシ先端: 硬さ 1 段目、厚さ 61、適用方向 垂直、向き 27 | `DualHardness` 100 → 1、`DualThickness` 100 → 61、`DualVerticalThicknes` 0 → 1、`DualRotation` 0 → 27 |
| 13 | 2-ブラシ先端: 2-ブラシ濃度 83、間隔によるブラシ濃度調整 オフ | `DualFlow` 100 → 83、`DualAdjustFlowByInterval` 1 → 0 |
| 14 | 2-散布効果: オン、粒子サイズ 9、ブラシサイズに連動 オン、粒子密度（3 段目）、散布偏向（2 段目）、粒子の向き 53 | `DualUseSpray` 0 → 1、`DualSpraySize` 2 → 9、`DualSpraySizeSyncBrushSize` 0 → 1、`DualSprayDensity` 1 → 4、`DualSprayBias` 0 → -50、`DualRotationInSpray` 0 → 53 |
| 14 | 2-ストローク: 間隔 狭い、連続吹き付け オン、比較(暗)で合成 オン | `DualAutoIntervalType` 2 → 3、`DualContinuousPlot` 0 → 1、`DualBlendPatternByDarken` と `DualBlendPatternByDarken2` がともに 0 → 1 |
| 15 | 2-紙質: インパスト、紙質濃度 44、濃度強調 オン、描点ごとに適用 オン | `DualTextureImage` が増える（素材の参照 1 件、種類 2）、`DualTextureDensity` 50 → 44、`DualTextureStressDensity` 0 → 1、`DualTextureForPlot` 0 → 1 |
| 15 | 2-紙質: 拡大率 70、回転角 15、明るさ -9、コントラスト 29、紙質適用方法 乗算 | `DualTextureScale` 100 → 70（デュアルブラシには `Scale2` がない）、`DualTextureRotate` 0 → 15、`DualTextureBrightness` 0 → -9、`DualTextureContrast` 0 → 29、`DualTextureCompositeMode` 0 → 1 |
| 16 | ブラシ濃度の影響元設定: 筆圧 オン・最小値 31、傾き オン・最小値 47・最大値 233、ランダム オン・最小値 59 | `BrushFlowEffector` の使う入力 0 → 0xB0、最小値 [0] 31・[1] 47・[3] 59、語 [10] 100 → 233。最大値は 100 未満にできない（77 を入れると 100） |
| 16 | 厚さの影響元設定: 速度 オン・最小値 13 | `BrushThicknessEffector` の使う入力 0 → 0x40、最小値 [2] 0 → 13 |
| 16 | 不透明度の影響元設定: 傾き オン・最小値 17 | `BrushOpacityEffector` の使う入力 0 → 0x20、最小値 [1] 100 → 17（最大値は 500 のまま）。不透明度は 0x100 を使える入力に持たない |
| 17 | 塗りつぶし: 隣接ピクセルをたどる オフ、色の誤差 23.4 | `FillContinuousPixel` 1 → 0、`FillColorMargin` 16.2 → 23.4 |
| 17 | 塗りつぶし: 領域拡縮 7、拡縮方法 1 番目のボタン、対称定規にスナップ オフ | `FillExpandLength` 4 → 7、`FillExpandType` 2 → 0、`EnableSnapSymmetry` 1 → 0 |
| 18 | 塗りつぶし: 隣接ピクセルをたどる オン、隙間閉じ 2 段目、細い領域にしみこむ オフ | `FillContinuousPixel` 0 → 1、`FillCloseGap` 10 → 3、`FillSoakIntoNarrowArea` 1 → 0 |
| 18 | 参照先: 複数参照の対象 3 番目（選択されたレイヤー）、参照しないレイヤーの 4 番目のボタン オン | `FillReferLayerType` 0 → 2、`FillNotReferCurrent` 0 → 1 |
| 18 | 参照先: 中心線も含める オフ、作画フレームを参照 オン | `FillIncludeVectorCenter` 1 → 0、`FillReferDrawFrame` 0 → 1 |
| 19 | 参照先: 参照しないレイヤーの 5 番目 オフ・6 番目 オン、ベクターの中心線で塗り止まる オフ、塗りを開始しない色 オン（黒のまま） | `FillNotReferPaper` 1 → 0、`FillNotReferLocked` 0 → 1、`FillReferVectorCenter` 1 → 0、`FillUseNotStartColor` 0 → 1 |
| 19 | 塗りつぶし: 隙間閉じ オフ、領域拡縮 オフ。アンチエイリアス オフ | `FillUseCloseGap` 1 → 0、`FillUseExpand` 1 → 0、`FillAntiAlias` 1 → 0 |
| 20 | 参照先: 複数参照の対象 4 番目（フォルダー内のレイヤー）、複数参照 オフ | `FillReferLayerType` 2 → 3、`FillReferMultiLayer` 1 → 0 |
| 21 | フォント: サイズ 21.5、太字と斜体、比率 横 87・縦 113、袋文字 オン、字間 1.5、文字詰め 23 | `TextFontSize` 16 → 21.5、`TextFontStyle` 0 → 3、`TextHorizontalRatio` 100 → 87、`TextVerticalRatio` 100 → 113、`TextOutlineEnable` 0 → 1、`TextWordSpace` 0 → 1.5、`TextTsume` 0 → 23 |
| 22 | 行間・揃え: 行間 137.5、行揃え 3 番目のボタン、フレームで折り返す オフ | `TextLineSpacePercent` 125 → 137.5、`TextAlignment` と `TextAlignmentJustify` 0 → 1、`TextWrapping` 1 → 0 |
| 23 | 縦書き、均等揃え 2 番目、フレームで折り返す オン、フレーム揃え 下、約物の字幅を半角にする オフ、背景 オン・不透明度 41、フチ オン・太さ 3 | `TextVertical` 0 → 1、`TextAlignment` と `TextAlignmentJustify` 1 → 5、`TextWrapping` 0 → 1、`TextTextAlignment` 0 → 2、`TextAlternateHalfMetrics` 1 → 0、`TextBackgroundEnabled` 0 → 1、`TextBackgroundAlpha` 100 → 41、`TextEdgeEnabled` 0 → 1、`TextEdgeWidth` 1 → 3 |
| 24 | 袋文字 太、カーニング メトリクス、アンチエイリアス オフ、自動縦中横 3 文字 | `TextOutline` 1 → 2、`TextKerningMethod` 0 → 1、`TextAntiAlias` 0 → 2、`TextTateChuYoko` 2 → 3 |
| 25 | ルビ: サイズ 63、位置 末尾揃え、位置の調整 2.5、親文字との距離 1.5、字間 自由・0.5。円形配置 オン・半径 77・反時計回り・均等配置 オン | `TextRubySize` 50 → 63、`TextRubyAlignment` 0 → 2、`TextRubyPosition` 0 → 2.5、`TextRubyDistance` 0 → 1.5、`TextRubyWordSpaceJustify` 4 → 0、`TextRubyWordSpace` 0 → 0.5、`TextLayoutCircularEnabled` 0 → 1、`TextLayoutCircularRadius` 100 → 77、`TextLayoutCircularDirection` 0 → 1、`TextLayoutCircularJustify` 0 → 1 |
| 26 | 操作: ドラッグ テキスト選択、選択の追加 1 番目。編集設定: テキストの色 2 番目、追加方法 常にレイヤーを作成。変形設定: リサイズハンドル オフ、変形方法 拡縮回転・平行ゆがみ、縦横比固定 オフ、回転角 33。補正: 表示メニューに従いスナップ オフ | `TextDragOperationType` 2 → 1、`TextSelectType` 1 → 0、`TextColorType` 0 → 1、`TextInsertType` 2 → 0、`TextEnableHandle` 1 → 0、`TextTransformType` 0 → 4、`TextFixAspect` 1 → 0、`TextRotation` 0 → 33、`ObjectUseSnap` 1 → 0 |
| 27 | 図形「長方形」: 図形 多角形・頂点数 7、角の丸さ オン・13・指定方法 長さ指定、線・塗り 1 番目、縦横指定 オン・横 2 | `FixedShapeKind` 0 → 2、`FixedShapeVertexNumber` 6 → 7、`FixedShapeUseRoundCorner` 0 → 1、`FixedShapeRoundRate` 10 → 13、`FixedShapeRoundValueType` 0 → 1、`ShapeStrokeAndFillType` 1 → 0、`FixedShapeUseAspect` 0 → 1、`FixedShapeWidthRate` 1 → 2 |
| 28 | 図形: 角の丸さ 25（長さ指定）、縦横指定 長さ指定・縦 150、線・塗り 3 番目、線の色 3 番目、塗りの色 1 番目。図形操作: 中央から開始、確定後に角度を調整、角度の刻み オン・30。補正: 表示メニューに従いスナップ オン | `FixedShapeRoundSize` 20 → 25、`FixedShapeAspectType` 0 → 1、`FixedShapeHeight` 100 → 150、`ShapeStrokeAndFillType` 0 → 2、`ShapeLineColorType` 0 → 2、`ShapeFillColorType` 1 → 0、`ShapeStartCenter` 0 → 1、`RotateShape` 0 → 1、`ShapeLockAngle` 0 → 1、`ShapeLockAngleStep` 45 → 30、`ShapeEnableSnap` 0 → 1 |
| 29 | 選択範囲「長方形選択」: 作成方法 4 番目、選択範囲と画像を連動 オン、図形 楕円、表示メニューに従いスナップ オン | `SelectCreateType` 1 → 3、`SelectMoveImage` 0 → 1、`FixedShapeKind` 0 → 1、`ShapeEnableSnap` 0 → 1 |
| 30 | グラデーション「昼空」: 形状 楕円、端の処理 3 番目、ディザリング オフ、角度の刻み オン・15 | `GradationShapeKind` 0 → 2、`GradationRepeatType` 0 → 2、`GradationDither` 1 → 0、`LineLockAngle` 0 → 1、`LineLockAngleStep` 45 → 15 |
| 31 | グラデーション: 反転、形状 円、端の処理 2 番目 | `GradationData` の点の並びと位置が左右反対になる（`GradationReverseSaved` は 1 のまま）、`GradationShapeKind` 2 → 1、`GradationRepeatType` 2 → 1 |
| 32 | フキダシ「角丸フキダシ」: 塗りの不透明度 64、追加方法 レイヤーを新規作成、トーン化 オン、描画位置のテキストに結合 オフ | `BalloonFillOpacity` 100 → 64、`BalloonInsertType` 1 → 0、`BalloonTone` 0 → 1、`BalloonUniteText` 1 → 0 |
| 33 | 流線「陰気」: 角度 -150、トーン化 オン、平行線定規を角度に用いる オフ、曲線化 オン。描画間隔: 線の間隔 17、乱れ 3 段目、まとまり オン・5、最大本数 150 | `ParallelStreamLineAngle` -180 → -150、`BalloonTone` 0 → 1（フキダシと同じ列）、`ParallelStreamUseRuler` 1 → 0、`StreamMakeCurve` 0 → 1、`ParallelStreamInterval` 12 → 17、`StreamIntervalDisturb` 10 → 50、`StreamUseGroup` 0 → 1、`StreamGroupSize` 3 → 5、`ParallelStreamMaxLineNumber` 200 → 150 |
| 34 | 流線の描画位置: 長さ 420、乱れ オン、コマ外まで延長 オフ、基準位置 中点、基準位置のずれ 35。連続曲線: 曲線 3 番目、線を閉じる オン、角度の刻み オン・20、作成途中の制御点の追加・削除 オフ | `StreamLineLength` 500 → 420、`StreamUseLineLengthDisturb` 0 → 1、`StreamExtendToFrame` 1 → 0、`ParallelStreamBasePointType` 2 → 1、`StreamStartGapDisturb` 50 → 35、`PolylineCurveKindWithoutCubic` 1 → 2、`PolylineClose` 0 → 1、`LineLockAngle` 0 → 1、`LineLockAngleStep` 45 → 20、`PolylineAddAndDeletePoint` 1 → 0 |
| 35 | 集中線「密な集中線」: 放射線定規を中心に用いる オフ、下地を塗りつぶす オン・塗りの不透明度 72、線の間隔 角度・3.5、基準位置 外側、ギザギザの数 40・高さ 250、基準位置をギザギザにする オフ | `ConcentrateStreamUseRuler` 1 → 0、`ConcentrateStreamUseFill` 0 → 1、`ConcentrateStreamFillOpacity` 100 → 72、`ConcentrateStreamIntervalType` 1 → 0、`ConcentrateStreamIntervalAngle` 2 → 3.5、`ConcentrateStreamBasePointType` 0 → 2、`StreamJagNumber` 36 → 40、`StreamJagLength` 300 → 250、`StreamUseJag` 1 → 0 |
| 36 | 操作「オブジェクト」: フォルダーを選択 オン、常に枠を表示 オン、選択の追加 3 番目、透明箇所の操作（別のレイヤーへの選択切り替え オフ、3D レイヤーの操作 オフ、平行線定規の方向指定 オン、ドラッグで範囲指定して選択 オン、すべてのレイヤーで有効 オン）、選択可能なオブジェクト（ベクター・テキスト・定規 オフ） | `ObjectSelectFolder` 0 → 1、`ObjectAlwaysShowFrame` 0 → 1、`ObjectSelectType` 0 → 2、`ObjectOperationAtBlank2` 0x111 → 0x10001018、`ObjectSelectable` 0x6FFFFFFF → 0x6FFFFEED（0x2・0x10・0x100 が消える） |
| 37 | スポイト「レイヤーから色を取得」: 参照先 3 番目・2 番目、周囲の色 オン・9、ペンでサークルを表示 オン、参照しないレイヤー 1 番目、続けて残りの 4 つ | `SpuitReferType` 0 → 2 → 1（ツールチップは 編集レイヤー・最も上のレイヤー・表示上のイメージ）、`SpuitUseAroundPixel` 0 → 1、`SpuitAroundPixel` 5 → 9、`SpuitShowColorCursorByPen` 0 → 1、`SpuitNotReferDraft` 0 → 1（1 番目だけ）、`SpuitNotReferText`・`SpuitNotReferBalloon`・`SpuitNotReferPaper`・`SpuitNotReferLocked` 0 → 1 |
| 38 | 線幅修正「線幅修正」: 処理を 1 つずつ選んで値を変える、線全体に処理 オフ、制御点を追加して修正 オフ → オン、滑らかにする範囲 5 段目 | `LineWidthOperation` 3 → 0 → 1 → 2 → 3 → 4、`LineWidthAddValue` 6.5 → 3.3、`LineWidthSubtractValue` 10 → 4.4、`LineWidthRemainCoreLine` 0 → 1、`LineWidthMultiplyValue` 1.2 → 1.7、`LineWidthDivideValue` 0.8 → 0.55、`LineWidthConstantValue` 20 → 7.7、`VectorWhole` 1 → 0、`VectorAddVertexByLineWidth` 1 → 0 → 1、`LineWidthSmoothLengthValue` 70 → 200 |
| 39 | ゆがみ「ゆがみ」: ゆがませ方 2・7・4 番目、強さ 33、硬さ 77、操作した箇所の画像のみを参照 オン、強さの影響元に速度を追加 | `LiquifyOperation` 0 → 1 → 6 → 3、`LiquifyStrength` 70 → 33、`LiquifyDensity` 50 → 77、`LiquifyReferDraggedArea` 0 → 1、`LiquifyStrengthEffector` の入力 16 → 80 と速度のグラフ |
| 40 | レイヤー移動「レイヤー移動」: クリック位置・選択範囲内・選択状態に変更・元画像を残す オン、移動対象 2・3・1 番目、選択状態に変更 オフ | `LayerMoveClicked`・`LayerMoveInSelectArea`・`LayerMoveSelectLayer`・`LayerMoveRemainOriginal` 0 → 1、`LayerMoveTarget` 0 → 1 → 2 → 0、`LayerMoveSelectLayer` 1 → 0（最後は単独） |
| 41 | コマ枠「コマフォルダー分割」: 分割形状と分割方法を 2 番目・3 番目・1 番目、間隔 33、環境設定の枠線間隔 オン → オフ、上下の間隔 99 | `CutFrameCurveKind` と `CutFrameCutFolderType` 0 → 1 → 2 → 0（2 つは同時に変えた）、`CutFrameHorizontalGap` 80 → 33、`CutFrameUsePreferenceGap` 0 → 1 → 0、`CutFrameVerticalGap` 180 → 99 |
| 42 | コマ枠「長方形コマ」: ラスターレイヤー オフ、続けて 追加方法 選択中のフォルダーに追加、コマの内側を塗りつぶす オフ、枠線を描画 オフ | `ComicFrameMakeBaseLayer` 1 → 0（単独）、`ComicFrameInsertType` 0 → 1、`ComicFrameMakeFillLayer` 1 → 0、`ComicFrameDrawLine` 1 → 0 |
| 43 | 定規「特殊定規」: 曲線 1 番目・3 番目、角度の刻み オン・30、編集レイヤーに作成 オフ、種類を 1 つずつ、同心円で 縦横比固定 オン・横 2・縦 3、確定後に角度を調整 オフ | `SpecialRulerCurveKind` 1 → 0 → 2、`SpecialRulerLockAngle` 0 → 1、`SpecialRulerLockAngleStep` 45 → 30、`RulerCurrentLayer` 1 → 0、`SpecialRulerKind` 放射曲線 2・同心円 3・平行線 0・ガイド 4・放射線 1・平行曲線 5・多重曲線 6、`SpecialRulerFixAspect` 0 → 1、`SpecialRulerWidthRate` 1 → 2、`SpecialRulerHeightRate` 1 → 3、`SpecialRulerRotate` 1 → 0 |
| 44 | 定規「対称定規」: 線の本数 6、線対称 オフ、角度の刻み オン | `SymmetryRulerLineNumber` 11 → 6、`SymmetryRulerLineSymmetry` 1 → 0、`SpecialRulerLockAngle` 0 → 1（特殊定規と同じ列） |
| 45 | 定規「パース定規」: 処理内容 無限遠にする・消失点の追加・ガイドの追加、透視図法を変更 オフ | `PerspectiveRulerOperation` 0 → 5 → 0 → 2、`PerspectiveRulerChangeType` 1 → 0 |
| 46 | フキダシ「フキダシしっぽ」: 曲げ方 スプライン・折れ線、しっぽの幅 35、しっぽの種類 丸 | `BalloonTailCurveKind` 0 → 2 → 1、`BalloonTailWidth` 20 → 35、`BalloonTailType` 0 → 1 |

2〜46 回目ではほかの列は変わらなかった。9・10 回目は 8 回目の書き出しを、11〜15 回目は 10 回目の書き出しを、16 回目は 15 回目の書き出しを読み込み直した一時ツールを使った（読み込み直しで変わったのは名前と ID だけ）。17〜20 回目は CSP の塗りつぶし「他レイヤーを参照」を、21〜26 回目はテキストツール「テキスト」を、27〜31 回目は図形「長方形」・選択範囲「長方形選択」・グラデーション「昼空」を、32〜36 回目はフキダシ「角丸フキダシ」・流線「陰気」・集中線「密な集中線」・操作「オブジェクト」を、37〜45 回目はスポイト「レイヤーから色を取得」・「線幅修正」・「ゆがみ」・「レイヤー移動」・「コマフォルダー分割」・「長方形コマ」・「特殊定規」・「対称定規」・「パース定規」を書き出して読み込み直した一時ツールで、読み込み直した直後の書き出しと比べた（21 回目以降は前の回の書き出しと比べる）。46 回目は「初期ツールを追加」で足した「フキダシしっぽ」を使い、最後に削除した。`SpecialRulerKind` の値は画面の一覧の並びと違う（一覧は 平行線・平行曲線・多重曲線・放射線・放射曲線・同心円・ガイド）。ゆがませ方の 3・5・6 番目とパース定規の処理内容 1・3・4 は値を見ておらず、ツールチップと一覧の並びから決めた。`SpuitNotReferBalloon`・`LiquifyAntiAlias`・`PerspectiveRulerAddFisheye` は手元の作品のサブツールには無い列で、表には入っていない（パース定規の 魚眼を追加する は画面で選べなかった）。テキストでは、フレームで折り返すをオフにすると均等揃えとフレーム揃えが使えなくなり、縦書きではスタイルのボタンが太字と斜体だけになる。影響元設定の画面の入力は筆圧・傾き（最小値と最大値）・速度・ランダムの 4 つだけで、入力ビット 0x100 に当たる項目は画面にない。入力をオンにすると、空だったグラフに既定の点が入る（筆圧 (0,0)–(1,1)、傾き (0,1)・(0.5,1)・(0.8,0)・(1,0)、速度 (0,1)–(1,0)）。1 回目は元のファイル（CSP の読み込み前）と比べたので、上の形式のそろえ直し
（`BrushPatternNameVersion` 0 → 1、`BrushPatternReverse` と `TextureScale` の削除、ヘッダー 48 バイト）も出た。結果は `tools/assets/subtool_columns.py` の `CONFIRMED` に入れ、
[`subtool-columns.tsv`](../spec/subtool-columns.tsv) では「確認済み」と示す（530 列のうち 266 列）。

## 残り

- W4 の続き（まだ確かめていないツール: 3D の操作、ライトテーブル、回転、コピースタンプ、ブラシの残りの項目など）。3D の操作はキャンバスと 3D 素材が要る
- 影響元設定の入力ビット 0x100（CSP の画面に出ないため、確かめる方法がまだない）
