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
    "SyncDualBrushSize": ("デュアルブラシ", "ブラシサイズを連動", "0/1"),
    # インク
    "BrushFlow": ("インク", "ブラシ濃度", "1–100 %"),
    "BrushOpacityEffector": ("インク", "不透明度の影響元設定", ""),
    "BrushAdjustFlowByInterval": ("インク", "間隔に合わせて濃度を調整", "0/1"),
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
    "BrushBlendPatternByDarken": ("ブラシ先端", "比較(暗)で重ねる", "0/1"),
    "BrushRibbon": ("ストローク", "リボン", "0/1"),
    "BrushInterval": ("ストローク", "間隔", "% (先端の直径に対する)"),
    "BrushAutoIntervalType": ("ストローク", "間隔", "0 固定, 1 広い（推測）, 2 普通, 3 狭い（推測）"),
    "BrushContinuousPlot": ("ストローク", "連続吹き付け", "0/1"),
    # 紙質
    "TextureImage": ("紙質", "紙質の素材", "素材の参照の列"),
    "TextureDensity": ("紙質", "紙質濃度", "%"),
    "TextureCompositeMode": ("紙質", "紙質の適用方法", "0 乗算, 1 比較(暗), 2 減算, 3 除算, …（推測）"),
    "TextureReverseDensity": ("紙質", "濃度を反転", "0/1"),
    "TextureStressDensity": ("紙質", "濃度を強調", "0/1"),
    "TextureScale": ("紙質", "拡大率（旧形式）", "%"),
    "TextureScale2": ("紙質", "拡大率", "%"),
    "TextureRotate": ("紙質", "回転角", "度"),
    "TextureBrightness": ("紙質", "明るさ", "-100–100"),
    "TextureContrast": ("紙質", "コントラスト", "-100–100"),
    "TextureForPlot": ("紙質", "ブラシ先端ごとに紙質を適用", "0/1"),
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
    "BrushSubColor": ("色の変化", "サブカラー混合率", "%"),
    "BrushUseWaterEdge": ("水彩境界", "水彩境界", "0/1"),
    "BrushWaterEdgeRadius": ("水彩境界", "範囲", ""),
    "BrushWaterEdgeAlphaPower": ("水彩境界", "透明度の影響", ""),
    "BrushWaterEdgeValuePower": ("水彩境界", "明度の影響", ""),
    "BrushWaterEdgeBlur": ("水彩境界", "ぼかし幅", ""),
    "BrushWaterEdgeAfterDrag": ("水彩境界", "描画後に処理", "0/1"),
    "BrushWaterEdgeStrength": ("水彩境界", "強さ（新しい形式）", ""),
    "BrushWaterEdgeStyle": ("水彩境界", "スタイル（新しい形式）", ""),
    "BrushWaterEdgeVividness": ("水彩境界", "鮮やかさ（新しい形式）", ""),
    # 色の変化
    "BrushChangePatternColor": ("色の変化", "吹き付けごとの色の変化", "0/1"),
    "BrushChangeStrokeColor": ("色の変化", "ストロークごとの色の変化", "0/1"),
    "BrushChangeColorTarget": ("色の変化", "色の変化の対象", ""),
    "BrushHueChange": ("色の変化", "色相（吹き付けごと）", ""),
    "BrushSaturationChange": ("色の変化", "彩度（吹き付けごと）", ""),
    "BrushValueChange": ("色の変化", "明度（吹き付けごと）", ""),
    "BrushStrokeHueChange": ("色の変化", "色相（ストロークごと）", ""),
    "BrushStrokeSaturationChange": ("色の変化", "彩度（ストロークごと）", ""),
    "BrushStrokeValueChange": ("色の変化", "明度（ストロークごと）", ""),
    "BrushStrokeSubColor": ("色の変化", "サブカラー（ストロークごと）", ""),
    "ChangeRGBByDual": ("デュアルブラシ", "2 つ目の先端で色を変える", ""),
    # 散布
    "BrushUseSpray": ("散布効果", "散布効果", "0/1"),
    "BrushSpraySize": ("散布効果", "粒子サイズ", ""),
    "BrushSpraySizeSyncBrushSize": ("散布効果", "粒子サイズをブラシサイズに連動", "0/1"),
    "BrushSprayDensity": ("散布効果", "粒子密度", "1–16"),
    "BrushSprayBias": ("散布効果", "散布偏り", "-100–100"),
    "BrushSprayUseFixedPoint": ("散布効果", "固定の散布位置を使う", "0/1"),
    "BrushSprayFixedPointArray": ("散布効果", "固定の散布位置", "点の列（標本では空）"),
    "BrushRotationInSpray": ("散布効果", "粒子の向き", "度"),
    "BrushRotationEffectorInSpray": ("散布効果", "粒子の向きの影響元", "ビット"),
    "BrushRotationRandomInSpray": ("散布効果", "粒子の向きのランダムの幅", "%"),
    # 入り抜き
    "BrushUseIn": ("入り抜き", "入り", "0/1"),
    "BrushUseOut": ("入り抜き", "抜き", "0/1"),
    "BrushInLength": ("入り抜き", "入りの長さ", ""),
    "BrushOutLength": ("入り抜き", "抜きの長さ", ""),
    "BrushInRatio": ("入り抜き", "入りの割合", "%"),
    "BrushOutRatio": ("入り抜き", "抜きの割合", "%"),
    "BrushInOutType": ("入り抜き", "入り抜きの指定方法", "0 長さ, 1 割合（推測）"),
    "BrushInOutBySpeed": ("入り抜き", "速度による入り抜き", "0/1"),
    "BrushInOutTarget": ("入り抜き", "入り抜きの影響先", "(項目 ID, 有効, 最小値 %) の列"),
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
    "UseDualBrush": ("デュアルブラシ", "デュアルブラシ", "0/1"),
    "DualBrushCompositeMode": ("デュアルブラシ", "合成モード", ""),
    # 塗りつぶし
    "FillColorMargin": ("塗りつぶし", "色の誤差", "%"),
    "FillUseExpand": ("塗りつぶし", "領域拡縮", "0/1"),
    "FillExpandLength": ("塗りつぶし", "領域拡縮の幅", ""),
    "FillExpandType": ("塗りつぶし", "領域拡縮の方法", ""),
    "FillUseCloseGap": ("塗りつぶし", "隙間閉じ", "0/1"),
    "FillCloseGap": ("塗りつぶし", "隙間閉じの大きさ", ""),
    "FillAntiAlias": ("塗りつぶし", "アンチエイリアス", ""),
    "FillReferMultiLayer": ("塗りつぶし", "複数参照", "0/1"),
    "FillReferLayerType": ("塗りつぶし", "参照するレイヤー", ""),
    "FillReferVectorCenter": ("塗りつぶし", "ベクターの中心線で塗る", "0/1"),
    "FillIncludeVectorCenter": ("塗りつぶし", "ベクターの中心線を含める", ""),
    "FillSoakIntoNarrowArea": ("塗りつぶし", "狭い領域にしみ込む", ""),
    "FillContinuousPixel": ("塗りつぶし", "隣接ピクセルをたどる", ""),
    "FillReferFrame": ("塗りつぶし", "コマ枠を参照", ""),
    "FillReferDrawFrame": ("塗りつぶし", "描画中のコマ枠を参照", ""),
    "FillNotReferBalloon": ("塗りつぶし", "フキダシを参照しない", ""),
    "FillNotReferCurrent": ("塗りつぶし", "編集レイヤーを参照しない", ""),
    "FillNotReferDraft": ("塗りつぶし", "下描きレイヤーを参照しない", ""),
    "FillNotReferLocked": ("塗りつぶし", "ロックしたレイヤーを参照しない", ""),
    "FillNotReferPaper": ("塗りつぶし", "用紙を参照しない", ""),
    "FillNotReferText": ("塗りつぶし", "テキストを参照しない", ""),
    "FillUseNotStartColor": ("塗りつぶし", "塗りつぶさない色を指定", ""),
    "FillNotStartColorRed": ("塗りつぶし", "塗りつぶさない色 R", ""),
    "FillNotStartColorGreen": ("塗りつぶし", "塗りつぶさない色 G", ""),
    "FillNotStartColorBlue": ("塗りつぶし", "塗りつぶさない色 B", ""),
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
