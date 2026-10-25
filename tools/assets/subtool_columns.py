"""Make docs/assets/spec/subtool-columns.tsv (A5): every Variant column with
its meaning (the item in CSP's tool property / sub tool detail palettes),
how sure that is, and what the samples hold (from subtool_census.py).

    python tools/assets/subtool_columns.py CENSUS.tsv docs/assets/spec/subtool-columns.tsv

Meanings come from LABELS below (one entry per base name) and the naming
rules of CSP's columns: ``Dual*`` is the same item of the dual brush (2つ目のブラシ先端),
``*Effector`` its 影響元設定, ``*Unit`` the unit of the value before it
(0 px, 2 mm, see the spec), ``Use*`` the check box that turns it on.
CONFIRMED lists the items checked in CSP (W3/W4); everything else is
"推測" (inferred from the name and the palettes) unless marked "未解明".
"""
from __future__ import annotations

import csv
import sys
from pathlib import Path

# base name → (palette group, label as CSP shows it, note on values)
LABELS = {
    # 共通
    "Opacity": ("不透明度", "不透明度", "0–100 %"),
    "AntiAlias": ("アンチエイリアス", "アンチエイリアス", "0 なし, 1 弱, 2 中, 3 強"),
    "CompositeMode": ("インク", "合成モード", "CSP のレイヤー合成モード番号（.clip の LayerComposite と同じ表）"),
    "FlickerReduction": ("補正", "手ブレ補正", "0–100"),
    "FlickerReductionBySpeed": ("補正", "速度による手ブレ補正", "0/1"),
    "FlickerReductionBySpeedType": ("補正", "補正タイプ（速度による手ブレ補正）", "1 左のボタン（既定）, 0 右のボタン"),
    "Stickness": ("補正", "ポストスムージング", "0–10"),
    "VariantID": ("（内部）", "設定行の ID", "Node.NodeVariantID / NodeInitVariantID から参照"),
    "_PW_ID": ("（内部）", "行番号", ""),
    "VariantShowParam": ("（内部）", "ツールプロパティに表示する項目", "項目 ID の列、subtool-items.tsv"),
    "VariantShowSeparator": ("（内部）", "ツールプロパティの区切り線", ""),
    # ブラシサイズ
    "BrushSize": ("ブラシサイズ", "ブラシサイズ", "半径ではなく直径の値（px は BrushSizeUnit 0）"),
    "BrushSizeSyncViewScale": ("ブラシサイズ", "画面上のサイズで指定", "0/1"),
    "BrushAtLeast1Pixel": ("ブラシサイズ", "最小値を 1 ピクセルにする", "0/1"),
    "SyncDualBrushSize": ("2-ブラシ形状", "メインのブラシサイズに連動", "0/1"),
    # インク
    "BrushFlow": ("インク", "ブラシ濃度", "1–100 %"),
    "BrushOpacityEffector": ("インク", "不透明度の影響元設定", ""),
    "BrushAdjustFlowByInterval": ("ブラシ先端", "間隔によるブラシ濃度調整", "0/1"),
    "BrushColorMixingMode": ("インク", "混色モード", "0 RGB, 1 知覚的（推測）"),
    "BrushLMSLinearity": ("インク", "知覚的混色の線形性", "未解明"),
    "BrushQuality": ("インク", "描画品質", "未解明"),
    # ブラシ先端
    "BrushHardness": ("ブラシ先端", "硬さ", "1–100（円形の先端）"),
    "BrushThickness": ("ブラシ先端", "厚さ", "%"),
    "BrushVerticalThicknes": ("ブラシ先端", "厚さの適用方向", "0 水平, 1 垂直"),
    "BrushRotation": ("ブラシ先端", "ブラシ先端の向き", "度"),
    "BrushRotationEffector": ("ブラシ先端", "向きの影響元", "ビット: 筆圧・傾き・進行方向・ランダムなど（.clip 側の RotationEffector と同じ）"),
    "BrushRotationRandomScale": ("ブラシ先端", "向きのランダムの幅", "%"),
    "BrushUsePatternImage": ("ブラシ先端", "先端形状: 素材", "0 円形, 1 素材"),
    "BrushPatternImageArray": ("ブラシ先端", "先端の素材", "素材の参照の列"),
    "BrushPatternNameVersion": ("ブラシ先端", "素材名の形式の版", "未解明"),
    "BrushPatternOrderType": ("ブラシ先端", "素材の順序（複数の素材）", "0 繰り返し, 1 往復, 2 ランダム, 3 筆圧（推測）"),
    "BrushPatternOrderType2": ("ブラシ先端", "素材の順序（新しい形式）", ""),
    "BrushPatternReverse": ("ブラシ先端", "素材の反転（旧形式）", ""),
    "BrushPatternReverseHorizontal": ("ブラシ先端", "左右反転", "0 なし, 2 ランダム, 3 進行方向（推測）"),
    "BrushPatternReverseVertical": ("ブラシ先端", "上下反転", ""),
    "BrushBlendPatternByDarken": ("ストローク", "比較(暗)で合成", "0/1（2-ストロークの同じ項目から推測）"),
    "BrushRibbon": ("ストローク", "リボン", "0/1"),
    "BrushInterval": ("ストローク", "間隔", "% (先端の直径に対する)"),
    "BrushAutoIntervalType": ("ストローク", "間隔", "0 固定, 1 広い（推測）, 2 普通, 3 狭い"),
    "BrushContinuousPlot": ("ストローク", "連続吹き付け", "0/1"),
    # 紙質
    "TextureImage": ("紙質", "紙質の素材", "素材の参照の列"),
    "TextureDensity": ("紙質", "紙質濃度", "%"),
    "TextureCompositeMode": ("紙質", "紙質適用方法", "CSP の並び順 0 通常, 1 乗算, 2 減算, 3 比較, 4 輪郭, 5 オーバーレイ, 6 覆い焼きカラー, 7 焼き込みカラー, 8 ハードミックス, 9 高さ（0、1、3 を確認）"),
    "TextureReverseDensity": ("紙質", "濃度反転", "0/1"),
    "TextureStressDensity": ("紙質", "濃度強調", "0/1"),
    "TextureScale": ("紙質", "拡大率（旧形式）", "%"),
    "TextureScale2": ("紙質", "拡大率", "%"),
    "DualTextureScale": ("紙質（デュアルブラシ）", "拡大率", "%（デュアルブラシには Scale2 の列がなく、この列が今の拡大率）"),
    "TextureRotate": ("紙質", "回転角", "度"),
    "TextureBrightness": ("紙質", "明るさ", "-100–100"),
    "TextureContrast": ("紙質", "コントラスト", "-100–100"),
    "TextureForPlot": ("紙質", "描点ごとに適用", "0/1"),
    # 水彩
    "BrushUseWaterColor": ("水彩", "絵の具量・絵の具濃度を使う", "0/1"),
    "BrushUseWaterColor2": ("水彩", "混色（新しい形式）", "0/1"),
    "BrushWaterColor": ("水彩", "混色の方式", "0 ブレンド, 1 絵の具量, 2 ?（推測）"),
    "BrushMixColor": ("水彩", "絵の具量", "%"),
    "BrushMixAlpha": ("水彩", "絵の具濃度", "%"),
    "BrushMixColorExtension": ("水彩", "色延び", "%"),
    "BrushBlur": ("水彩", "ぼかし幅", ""),
    "BrushBlurLinkSize": ("水彩", "ぼかし幅をブラシサイズに連動", "0/1"),
    "BrushBlurKind": ("水彩", "ぼかしの種類", ""),
    "BrushSubColor": ("色の変化", "サブ描画色混合率（ブラシ先端色の変化）", "%"),
    "BrushUseWaterEdge": ("水彩境界", "水彩境界", "0/1"),
    "BrushWaterEdgeRadius": ("水彩境界", "水彩境界（範囲）", ""),
    "BrushWaterEdgeAlphaPower": ("水彩境界", "透明度影響", "0–100（バー）"),
    "BrushWaterEdgeValuePower": ("水彩境界", "明度影響", "0–100（バー）"),
    "BrushWaterEdgeBlur": ("水彩境界", "ぼかし幅", ""),
    "BrushWaterEdgeAfterDrag": ("水彩境界", "ドラッグ後に処理", "0/1"),
    "BrushWaterEdgeStrength": ("水彩境界", "強さ（フチの表現 ビビッド）", "0–100（バー）"),
    "BrushWaterEdgeStyle": ("水彩境界", "フチの表現", "0 通常, 1 ビビッド"),
    "BrushWaterEdgeVividness": ("水彩境界", "鮮やかさ（フチの表現 ビビッド）", "0–100（バー）"),
    # 色の変化
    "BrushChangePatternColor": ("色の変化", "ブラシ先端色の変化", "0/1"),
    "BrushChangeStrokeColor": ("色の変化", "ストロークごとのランダム変化", "0/1"),
    "BrushChangeColorTarget": ("色の変化", "変化対象", "0 メイン描画色とサブ描画色, 1 メイン描画色, 2 サブ描画色（0 は並び順から推測）"),
    "BrushHueChange": ("色の変化", "色相（ブラシ先端色の変化）", ""),
    "BrushSaturationChange": ("色の変化", "彩度（ブラシ先端色の変化）", ""),
    "BrushValueChange": ("色の変化", "明度（ブラシ先端色の変化）", ""),
    "BrushStrokeHueChange": ("色の変化", "色相（ストロークごとのランダム変化）", ""),
    "BrushStrokeSaturationChange": ("色の変化", "彩度（ストロークごとのランダム変化）", ""),
    "BrushStrokeValueChange": ("色の変化", "明度（ストロークごとのランダム変化）", ""),
    "BrushStrokeSubColor": ("色の変化", "サブ描画色混合率（ストロークごとのランダム変化）", "%"),
    "ChangeRGBByDual": ("2-ブラシ形状", "RGB値に適用", "0/1"),
    # 散布
    "BrushUseSpray": ("散布効果", "散布効果", "0/1"),
    "BrushSpraySize": ("散布効果", "粒子サイズ", ""),
    "BrushSpraySizeSyncBrushSize": ("散布効果", "粒子サイズをブラシサイズに連動", "0/1"),
    "BrushSprayDensity": ("散布効果", "粒子密度", "1–16"),
    "BrushSprayBias": ("散布効果", "散布偏向", "-100–100"),
    "BrushSprayUseFixedPoint": ("散布効果", "固定の散布位置を使う", "0/1"),
    "BrushSprayFixedPointArray": ("散布効果", "固定の散布位置", "点の列（標本では空）"),
    "BrushRotationInSpray": ("散布効果", "粒子の向き", "度"),
    "BrushRotationEffectorInSpray": ("散布効果", "粒子の向きの影響元", "ビット"),
    "BrushRotationRandomInSpray": ("散布効果", "粒子の向きのランダムの幅", "%"),
    # 入り抜き
    "BrushUseIn": ("入り抜き", "入り", "0/1"),
    "BrushUseOut": ("入り抜き", "抜き", "0/1"),
    "BrushInLength": ("入り抜き", "入り（長さ指定）", ""),
    "BrushOutLength": ("入り抜き", "抜き（長さ指定）", ""),
    "BrushInRatio": ("入り抜き", "入り（パーセント指定）", "%"),
    "BrushOutRatio": ("入り抜き", "抜き（パーセント指定）", "%"),
    "BrushInOutType": ("入り抜き", "指定方法", "0 長さ指定, 1 パーセント指定, 2 フェード（2 は推測）"),
    "BrushInOutBySpeed": ("入り抜き", "速度による入り抜き", "0/1"),
    "BrushInOutTarget": ("入り抜き", "入り抜き（影響先設定）", "(項目 ID, 有効, 最小値 %) の列。1001 ブラシサイズ, 1051 厚さ（確認）、1021 ブラシ濃度, 1041 間隔（並び順から推測）"),
    "BrushNotUseCommonInOut": ("入り抜き", "共通の入り抜きを使わない", ""),
    # 補正
    "BrushUseRevision": ("補正", "ベクター吸着/補正を使う", "0/1"),
    "BrushRevision": ("補正", "補正の強さ", ""),
    "BrushRevisionBySpeed": ("補正", "速度で補正", "0/1"),
    "BrushRevisionByViewScale": ("補正", "表示倍率で補正", "0/1"),
    "BrushRevisionBezier": ("補正", "ベジェ曲線で補正", "0/1"),
    "BrushSharpenCorner": ("補正", "角をとがらせる", "0/1"),
    "BrushAdjustVelocity": ("補正", "速度の調整", "0/1"),
    "BrushEnableSnap": ("補正", "スナップ可能", "0/1"),
    "BrushUseVectorMagnet": ("補正", "ベクター吸着", "0/1"),
    "BrushVectorMagnetPower": ("補正", "ベクター吸着の強さ", ""),
    "BrushUseReferLayer": ("補正", "参照レイヤーを使う", ""),
    # 消しゴム
    "BrushUseVectorEraser": ("消去", "ベクター消去", "0/1"),
    "BrushVectorEraserType": ("消去", "ベクター消去の方式", "0 触れた部分, 1 交点まで, 2 線全体"),
    "BrushVectorEraserReferAllLayer": ("消去", "ベクター消去で複数レイヤーを参照", "0/1"),
    "BrushEraseAllLayer": ("消去", "複数レイヤーを消去", "0/1"),
    # デュアルブラシ
    "UseDualBrush": ("2-ブラシ形状", "2-ブラシ形状（デュアルブラシを使う）", "0/1"),
    "DualBrushCompositeMode": ("2-ブラシ形状", "適用方法", "CSP の並び順 0 通常, 1 乗算, 2 加算(発光), 3 減算, 4 比較(暗), 5 比較(明), 6 スクリーン, 7 オーバーレイ, 8 覆い焼きカラー, 9 焼き込みカラー, 10 焼き込み(リニア), 11 ハードミックス, 12 高さ(リニア)（1 と 6 を確認）"),
    "DualBlendPatternByDarken": ("ストローク（デュアルブラシ）", "比較(暗)で合成", "0/1（1 つのチェックで 2 列とも変わる）"),
    "DualBlendPatternByDarken2": ("ストローク（デュアルブラシ）", "比較(暗)で合成（2 列目）", "0/1（DualBlendPatternByDarken と同じ値）"),
    # 塗りつぶし
    "FillColorMargin": ("塗りつぶし", "色の誤差", "%"),
    "FillUseExpand": ("塗りつぶし", "領域拡縮", "0/1"),
    "FillExpandLength": ("塗りつぶし", "領域拡縮の幅", ""),
    "FillExpandType": ("塗りつぶし", "拡縮方法", "ボタンの並び順 0 1 番目, 1 2 番目, 2 3 番目（0 と 2 を確認）"),
    "FillUseCloseGap": ("塗りつぶし", "隙間閉じ", "0/1"),
    "FillCloseGap": ("塗りつぶし", "隙間閉じの大きさ", "段階のバー（2 段目 3、4 段目 10）"),
    "FillAntiAlias": ("塗りつぶし", "アンチエイリアス", "0/1（塗りつぶしではチェックだけ）"),
    "FillReferMultiLayer": ("参照先", "複数参照", "0/1"),
    "FillReferLayerType": ("参照先", "複数参照の対象", "0 すべてのレイヤー, 1 参照レイヤー, 2 選択されたレイヤー, 3 フォルダー内のレイヤー（ボタンの並び順。1 は推測）"),
    "FillReferVectorCenter": ("参照先", "ベクターの中心線で塗り止まる", "0/1"),
    "FillIncludeVectorCenter": ("参照先", "中心線も含める", "0/1"),
    "FillSoakIntoNarrowArea": ("塗りつぶし", "細い領域にしみこむ", "0/1"),
    "FillContinuousPixel": ("塗りつぶし", "隣接ピクセルをたどる", "0/1"),
    "FillReferFrame": ("塗りつぶし", "コマ枠を参照", ""),
    "FillReferDrawFrame": ("参照先", "作画フレームを参照", "0/1"),
    "FillNotReferBalloon": ("参照先", "参照しないレイヤー: フキダシ", "0/1（3 番目のボタン、推測）"),
    "FillNotReferCurrent": ("参照先", "参照しないレイヤー: 編集レイヤー", "0/1（4 番目のボタン）"),
    "FillNotReferDraft": ("参照先", "参照しないレイヤー: 下描き", "0/1（1 番目のボタン、推測）"),
    "FillNotReferLocked": ("参照先", "参照しないレイヤー: ロック", "0/1（6 番目のボタン）"),
    "FillNotReferPaper": ("参照先", "参照しないレイヤー: 用紙", "0/1（5 番目のボタン）"),
    "FillNotReferText": ("参照先", "参照しないレイヤー: テキスト", "0/1（2 番目のボタン、推測）"),
    "FillUseNotStartColor": ("参照先", "塗りを開始しない色", "0/1"),
    "FillNotStartColorRed": ("参照先", "塗りを開始しない色 R", "0–255"),
    "FillNotStartColorGreen": ("参照先", "塗りを開始しない色 G", "0–255"),
    "FillNotStartColorBlue": ("参照先", "塗りを開始しない色 B", "0–255"),
    "EnableSnapSymmetry": ("ツール共通", "対称定規にスナップ", "0/1"),
    "CloseFillReferColorType": ("塗りつぶし", "囲って塗る: 参照する色", ""),
    "IncludeFill": ("塗りつぶし", "塗り残し部分に塗る", ""),
    "IncludeFillColor": ("塗りつぶし", "塗り残しの色", "16 バイトのレコード（未解明）"),
    "IncludeFillTouchVectorLine": ("塗りつぶし", "ベクター線に触れた部分", ""),
    "IncludeFillUseTouchVectorLine": ("塗りつぶし", "ベクター線に触れた部分を使う", ""),
    "ContourFillBlendGraph": ("等高線塗り", "等高線の混合の曲線", "点の列"),
    "ContourFillUseBlack": ("等高線塗り", "黒を使う", ""),
    "ContourFillUseTrans": ("等高線塗り", "透明を使う", ""),
    # グラデーション
    "GradationData": ("グラデーション", "グラデーションの色の点", "GradationData（gradient.rs）"),
    "GradationShapeKind": ("グラデーション", "形状", "0 直線, 1 円（推測）"),
    "GradationRepeatType": ("グラデーション", "端の形状", "0 繰り返さない, 1 繰り返し, 3 折り返し（推測）"),
    "GradationDither": ("グラデーション", "ディザリング", ""),
    "GradationMakeLayer": ("グラデーション", "作成先", "0 描画レイヤー, 1 グラデーションレイヤー（推測）"),
    "GradationReverseSaved": ("グラデーション", "反転を保存した印", "未解明"),
    "DustMode": ("ゴミ取り", "モード", ""), "DustModeForSelect": ("ゴミ取り", "選択範囲のモード", ""),
    "DustSize": ("ゴミ取り", "ゴミサイズ", ""),
}

# Prefix groups whose items are self-describing enough to label by rule.
PREFIX_GROUPS = [
    ("Text", "テキスト"), ("Balloon", "フキダシ"), ("Stream", "効果線"), ("Concentrate", "集中線"),
    ("Parallel", "流線"), ("ComicFrame", "コマ枠"), ("CutFrame", "コマ枠カット"), ("FixedShape", "図形"),
    ("Shape", "図形"), ("Line", "線"), ("Polyline", "折れ線"), ("Vector", "ベクター線"), ("Object3D", "3D"),
    ("Object", "操作"), ("Select", "選択範囲"), ("Spuit", "スポイト"), ("Ruler", "定規"),
    ("SpecialRuler", "特殊定規"), ("SymmetryRuler", "対称定規"), ("PerspectiveRuler", "パース定規"),
    ("Liquify", "ゆがみ"), ("LayerMove", "レイヤー移動"), ("LightTable", "ライトテーブル"), ("Rotate", "回転"),
    ("Zoom", "虫めがね"), ("TimeLine", "タイムライン"), ("Stamp", "コピースタンプ"), ("EnableSnap", "対称定規"),
]

# Checked in CSP (column → evidence); see docs/assets/validation/W3-csp.md.
CONFIRMED: dict[str, str] = {
    "BrushSize": "確認済み（W3 表示、W4）",
    "Opacity": "確認済み（W3 表示・書き出し、W4）",
    "AntiAlias": "確認済み（W3 表示: 1 弱、2 中）",
    "CompositeMode": "確認済み（W4: ハードミックス 20）",
    "BrushSizeSyncViewScale": "確認済み（W4）",
    "BrushMixColor": "確認済み（W3 表示）",
    "BrushMixAlpha": "確認済み（W3 表示）",
    "BrushSharpenCorner": "確認済み（W4）",
    "FlickerReduction": "確認済み（W4）",
    "FlickerReductionBySpeedType": "確認済み（W4）",
    "BrushFlow": "確認済み（W4）",
    "BrushHardness": "確認済み（W4）",
    "BrushRotation": "確認済み（W4）",
    "BrushThickness": "確認済み（W4）",
    "BrushUseVectorMagnet": "確認済み（W4）",
    "BrushVerticalThicknes": "確認済み（W4）",
    "BrushAutoIntervalType": "確認済み（W4: 固定 0、普通 2）",
    "BrushContinuousPlot": "確認済み（W4）",
    "BrushUseSpray": "確認済み（W4）",
    "BrushSpraySize": "確認済み（W4）",
    "BrushSprayDensity": "確認済み（W4）",
    "BrushSprayBias": "確認済み（W4）",
    "BrushRotationInSpray": "確認済み（W4）",
    "TextureImage": "確認済み（W4: 素材の参照）",
    "TextureDensity": "確認済み（W4）",
    "TextureCompositeMode": "確認済み（W4: 通常 0、比較 3）",
    "TextureReverseDensity": "確認済み（W4）",
    "TextureScale2": "確認済み（W4）",
    "TextureRotate": "確認済み（W4）",
    "TextureBrightness": "確認済み（W4）",
    "TextureContrast": "確認済み（W4）",
    "TextureForPlot": "確認済み（W4）",
    "BrushInOutTarget": "確認済み（W4: ブラシサイズ 1001、厚さ 1051）",
    "BrushInOutType": "確認済み（W4: 長さ指定 0、パーセント指定 1）",
    "BrushInOutBySpeed": "確認済み（W4）",
    "BrushInLength": "確認済み（W4）",
    "BrushOutLength": "確認済み（W4）",
    "BrushInRatio": "確認済み（W4 表示）",
    "BrushOutRatio": "確認済み（W4 表示）",
    "BrushUseWaterEdge": "確認済み（W4）",
    "BrushWaterEdgeRadius": "確認済み（W4）",
    "BrushWaterEdgeStyle": "確認済み（W4）",
    "BrushWaterEdgeAlphaPower": "確認済み（W4）",
    "BrushWaterEdgeValuePower": "確認済み（W4）",
    "BrushWaterEdgeStrength": "確認済み（W4）",
    "BrushWaterEdgeVividness": "確認済み（W4）",
    "BrushWaterEdgeAfterDrag": "確認済み（W4）",
    "BrushWaterEdgeBlur": "確認済み（W4）",
    "BrushChangePatternColor": "確認済み（W4）",
    "BrushHueChange": "確認済み（W4）",
    "BrushSaturationChange": "確認済み（W4）",
    "BrushValueChange": "確認済み（W4）",
    "BrushSubColor": "確認済み（W4）",
    "BrushChangeStrokeColor": "確認済み（W4）",
    "BrushStrokeHueChange": "確認済み（W4）",
    "BrushStrokeSaturationChange": "確認済み（W4）",
    "BrushStrokeValueChange": "確認済み（W4）",
    "BrushStrokeSubColor": "確認済み（W4）",
    "BrushChangeColorTarget": "確認済み（W4: メイン描画色 1、サブ描画色 2）",
    "BrushSizeEffector": "確認済み（W4: 入力のビット、入力ごとの最小値、傾きの最大値、筆圧のグラフ）",
    "UseDualBrush": "確認済み（W4）",
    "DualBrushCompositeMode": "確認済み（W4: 乗算 1、スクリーン 6）",
    "ChangeRGBByDual": "確認済み（W4）",
    "SyncDualBrushSize": "確認済み（W4）",
    "DualSize": "確認済み（W4）",
    "DualAntiAlias": "確認済み（W4: 中 2、強 3）",
    "DualHardness": "確認済み（W4）",
    "DualThickness": "確認済み（W4）",
    "DualVerticalThicknes": "確認済み（W4）",
    "DualRotation": "確認済み（W4）",
    "DualFlow": "確認済み（W4）",
    "DualAdjustFlowByInterval": "確認済み（W4）",
    "DualUseSpray": "確認済み（W4）",
    "DualSpraySize": "確認済み（W4）",
    "DualSpraySizeSyncBrushSize": "確認済み（W4）",
    "DualSprayDensity": "確認済み（W4）",
    "DualSprayBias": "確認済み（W4）",
    "DualRotationInSpray": "確認済み（W4）",
    "DualAutoIntervalType": "確認済み（W4: 普通 2、狭い 3）",
    "DualContinuousPlot": "確認済み（W4）",
    "DualBlendPatternByDarken": "確認済み（W4）",
    "DualBlendPatternByDarken2": "確認済み（W4）",
    "DualTextureImage": "確認済み（W4: 素材の参照）",
    "DualTextureDensity": "確認済み（W4）",
    "DualTextureStressDensity": "確認済み（W4）",
    "DualTextureForPlot": "確認済み（W4）",
    "DualTextureScale": "確認済み（W4）",
    "DualTextureRotate": "確認済み（W4）",
    "DualTextureBrightness": "確認済み（W4）",
    "DualTextureContrast": "確認済み（W4）",
    "DualTextureCompositeMode": "確認済み（W4: 通常 0、乗算 1）",
    "BrushFlowEffector": "確認済み（W4: 入力、最小値、傾きの最大値）",
    "BrushOpacityEffector": "確認済み（W4: 傾き）",
    "BrushThicknessEffector": "確認済み（W4: 速度）",
    "FillContinuousPixel": "確認済み（W4）",
    "FillColorMargin": "確認済み（W4）",
    "FillUseExpand": "確認済み（W4）",
    "FillExpandLength": "確認済み（W4）",
    "FillExpandType": "確認済み（W4: 1 番目 0、3 番目 2）",
    "FillUseCloseGap": "確認済み（W4）",
    "FillCloseGap": "確認済み（W4）",
    "FillSoakIntoNarrowArea": "確認済み（W4）",
    "FillAntiAlias": "確認済み（W4）",
    "FillReferMultiLayer": "確認済み（W4）",
    "FillReferLayerType": "確認済み（W4: すべて 0、選択 2、フォルダー 3）",
    "FillNotReferCurrent": "確認済み（W4）",
    "FillNotReferPaper": "確認済み（W4）",
    "FillNotReferLocked": "確認済み（W4）",
    "FillReferVectorCenter": "確認済み（W4）",
    "FillIncludeVectorCenter": "確認済み（W4）",
    "FillReferDrawFrame": "確認済み（W4）",
    "FillUseNotStartColor": "確認済み（W4）",
    "EnableSnapSymmetry": "確認済み（W4: 塗りつぶし）",
}


# Words of the remaining names, for a literal reading of items not in LABELS.
WORDS = {
    "Fill": "塗り", "Opacity": "不透明度", "Insert": "作成", "Type": "方式", "Tail": "しっぽ", "Curve": "曲線",
    "Kind": "種類", "Width": "幅", "Unit": "単位", "Tone": "トーン", "Unite": "結合", "Text": "テキスト",
    "Base": "基準", "Channel": "チャンネル", "Draw": "描画", "Line": "線", "Make": "作成", "Layer": "レイヤー",
    "Point": "点", "Interval": "間隔", "Angle": "角度", "Distance": "距離", "Use": "使う", "Ruler": "定規",
    "Cut": "分割", "Folder": "フォルダー", "Horizontal": "横", "Vertical": "縦", "Gap": "間隔", "Preference": "環境設定",
    "Aspect": "縦横比", "Height": "高さ", "Rate": "比率", "Round": "角丸", "Size": "サイズ", "Value": "値",
    "Corner": "角", "Vertex": "頂点", "Number": "数", "Lock": "固定", "Step": "刻み", "Add": "加算",
    "Constant": "一定", "Divide": "除算", "Multiply": "乗算", "Operation": "操作", "Remain": "残す", "Core": "芯",
    "Smooth": "滑らか", "Length": "長さ", "Subtract": "減算", "Density": "密度", "Strength": "強さ",
    "Refer": "参照", "Dragged": "ドラッグした", "Area": "範囲", "Auto": "自動", "Adjust": "調整", "Target": "対象",
    "Gesture": "ジェスチャー", "Control": "操作", "Camera": "カメラ", "Double": "2 本指", "Swipe": "スワイプ",
    "Magnify": "拡大", "Rotate": "回転", "Manipulator": "マニピュレーター", "Visible": "表示", "Move": "移動",
    "Scale": "拡縮", "Orientation": "向き", "Mouse": "マウス", "Button": "ボタン", "Wheel": "ホイール",
    "Pivot": "回転の中心", "Posing": "ポーズ", "Model": "モデル", "Limit": "制限", "Snap": "スナップ",
    "Flag": "フラグ", "Grid": "グリッド", "Pers": "パース", "Pos": "位置", "All": "すべての", "Always": "常に",
    "Show": "表示", "Frame": "枠", "Comic": "コマ", "Link": "連動", "At": "で", "Blank": "空白",
    "Resizable": "拡縮可能", "Transform": "変形", "Select": "選択", "Selectable": "選択できる対象",
    "Vector": "ベクター", "Change": "変更", "Stream": "線", "Max": "最大", "Add And Delete": "追加と削除",
    "Close": "閉じる", "Without": "なし", "Cubic": "3 次", "Designation": "指定", "Prediction": "予測",
    "Double Click": "ダブルクリック", "Shape": "図形", "Current": "編集中", "Graduation": "目盛り", "Create": "作成",
    "Not": "しない", "Draft": "下描き", "Locked": "ロック", "Monochrome": "モノクロ", "Image": "画像",
    "Method": "方法", "Off": "解除", "Condition": "条件", "To": "へ", "Center": "中心", "Color": "色",
    "Red": "R", "Green": "G", "Blue": "B", "Stroke": "線", "And": "と", "Saved": "保存", "Ver4": "（版 4）",
    "Inner": "内側", "Start": "開始", "Fix": "固定", "Around": "周囲", "Pixel": "ピクセル", "Paper": "用紙",
    "Cursor": "カーソル", "By": "で", "Pen": "ペン", "Position": "位置", "Set": "設定", "Mode": "モード",
    "Disturb": "乱れ", "Extend": "伸ばす", "Group": "まとまり", "Jag": "ギザギザ", "Symmetry": "対称",
    "Alignment": "揃え", "Justify": "均等", "Alternate": "代替", "Half": "半角", "Metrics": "メトリクス",
    "Anti": "アンチ", "Alias": "エイリアス", "Background": "背景", "Alpha": "不透明度", "Enabled": "有効",
    "Contextual": "前後関係に依存する", "Drag": "ドラッグ", "Edge": "フチ", "Enable": "有効", "Handle": "ハンドル",
    "Fast": "高速", "Preview": "プレビュー", "Font": "フォント", "Name": "名", "Style": "スタイル", "Unique": "固有",
    "Ratio": "比率", "Reverse": "反転", "Skew": "斜体", "Kerning": "カーニング", "Latest": "最新", "Version": "版",
    "Layout": "レイアウト", "Circular": "円形", "Direction": "方向", "Radius": "半径", "Space": "間隔",
    "Percent": "%", "Outline": "輪郭", "Rotation": "回転", "Ruby": "ルビ", "Word": "字", "Tate": "縦中横",
    "Chu": "", "Yoko": "", "Tool": "ツール", "Property": "プロパティ", "Tsume": "ツメ", "Wrapping": "折り返し",
    "Ligature": "合字", "Standard": "標準", "Pinch": "つまむ", "Connect": "連結", "Different": "異なる",
    "Level": "レベル", "Dust": "ゴミ", "Whole": "線全体", "Veretex": "頂点", "Pressure": "筆圧", "For": "の",
    "Radio": "ラジオボタン", "Click": "クリック", "Touch": "タッチ", "Clicked": "クリックした", "In": "の中",
    "Original": "元", "Expand": "拡大", "Next": "次", "Time": "時間", "Mouse L": "左", "L": "左", "M": "中",
    "R": "右", "Light": "ライト", "Table": "テーブル", "Special": "特殊", "Perspective": "パース", "Spuit": "スポイト",
    "Object3D": "3D", "Object": "オブジェクト", "Polyline": "折れ線", "Concentrate": "集中", "Parallel": "平行",
    "Balloon": "フキダシ", "Liquify": "ゆがみ", "Stamp": "スタンプ", "Fixed": "固定", "Zoom": "ズーム",
    "Two": "2", "Operation2": "操作 2", "Blank2": "空白 2",
}


def literal(name: str) -> str:
    import re
    parts = re.findall(r"[A-Z0-9][a-z0-9]*|[a-z]+", name)
    return " ".join(WORDS.get(p, p) for p in parts if WORDS.get(p, p))


def lookup(base: str):
    """(group, label, values, suffix words) for a base name, or None."""
    if base in LABELS:
        return (*LABELS[base], "")
    for suffix, words in (("Effector", "の影響元設定"), ("Unit", "の単位")):
        if base.endswith(suffix) and base[:-len(suffix)] in LABELS:
            values = "Effector（影響元: 筆圧・傾き・速度・ランダム）" if suffix == "Effector" else "0 px, 2 mm（推測）"
            group, label, _ = LABELS[base[:-len(suffix)]]
            return group, label, values, words
    return None


def describe(column: str) -> tuple[str, str, str, str]:
    """(group, label, values, certainty)."""
    found, extra = lookup(column), ""
    if found is None and column.startswith("Dual"):
        rest = column[4:]
        for candidate in ("Brush" + rest, rest, "Texture" + rest, rest.rstrip("2")):
            found = lookup(candidate)
            if found:
                break
        extra = "（デュアルブラシ）"
    if found:
        group, label, values, words = found
        certainty = "未解明" if "未解明" in values else "推測"
        return group + extra, label + words, values, CONFIRMED.get(column, certainty)
    for prefix, group in sorted(PREFIX_GROUPS, key=lambda p: -len(p[0])):
        if column.startswith(prefix):
            return group, literal(column[len(prefix):]) or group, "名前からの直訳", CONFIRMED.get(column, "推測（直訳）")
    return "", literal(column), "名前からの直訳", "未解明"


def main() -> int:
    census, out = Path(sys.argv[1]), Path(sys.argv[2])
    rows = list(csv.DictReader(census.open(encoding="utf-8"), delimiter="\t"))
    out.parent.mkdir(parents=True, exist_ok=True)
    counts: dict[str, int] = {}
    with out.open("w", encoding="utf-8", newline="") as f:
        f.write("column\tgroup\tlabel\tvalues\tcertainty\ttype\trows\tkinds\tmin\tmax\tblob\n")
        for row in rows:
            group, label, values, certainty = describe(row["column"])
            counts[certainty] = counts.get(certainty, 0) + 1
            kinds = ",".join(k.split(":")[0] for k in row["kinds"].split(",") if k)
            f.write("\t".join([row["column"], group, label, values, certainty, row["type"], row["rows"], kinds,
                               row["min"] if row["min"] != "None" else "", row["max"] if row["max"] != "None" else "",
                               row["blob"]]) + "\n")
    print(len(rows), "columns", counts)
    return 0


if __name__ == "__main__":
    sys.exit(main())
