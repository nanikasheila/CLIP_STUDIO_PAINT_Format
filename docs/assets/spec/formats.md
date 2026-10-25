# CSP の独自形式（キャンバス以外）

`.clip` / `.cmc` 以外で CLIP STUDIO PAINT が使うファイルの仕様です。公式の仕様はないので、PC 上の実ファイル（W1）と
CSP での確認（W3・W4）から書いています。確度は **確認済み**（CSP で確かめた）、**推測**（実ファイルと名前から）、**未解明** で示します。

全ファイルの一覧（拡張子・系統・用途）は `crates/csp-assets/src/kind.rs` の `FORMATS` にあり、`cspf asset kind` で判定できます。

## 1. 系統

| 系統 | 見分け方 | 形式 | 実装 |
|---|---|---|---|
| SQLite | `SQLite format 3\0` | サブツール（`.sut` `.todb` `.bfps`）、ワークスペース、ショートカット、各種設定（約 50 種） | `subtool`、汎用は `csp_format::database` |
| 型付き直列化 | `01 01 01 08 "typename"` | オートアクション（`.laf` `.las`）、グラデーションセット（`.sgs`） | `serial`、`action`、`gradient` |
| SLCC | `SLCC` | カラーセット（`.cls`） | `colorset` |
| C2F | `89 "C2F" 0D 0A 1A 0A` | 素材のデータ（`.layer` `.tool` `.pose` …）、`catalog.zip` `info.zip`、`webWhiteList.xml` | `c2f`（外枠だけ。中身は暗号化されていて復号しない） |
| XML | `<?xml` | 素材の目録（`catalog.xml` `info.xml` `icedata/layerData.xml`）、3D のプリセット | `material` |
| tar | ustar | `.sut` に同梱する素材、`description.tar` | `tar` |
| そのほか | 名前で判定 | `.cac`（目録のキャッシュ）、`InstallPathToCatalogId.dat`、状態ファイル | `material`、`kind` |

## 2. サブツール（`.sut` / `EditImageTool.todb` / `.bfps`）（確認済み: 構造、W1）

SQLite のデータベースで、表は `Manager`（1 行）、`Node`（ツール・グループ・サブツール）、`Variant`（設定）、
`.sut` だけ `MaterialFile`（同梱素材）。

- **ツリー**: `Manager.RootUuid` から、`Node.NodeFirstChildUuid`（最初の子）と `NodeNextUuid`（次の兄弟）でたどる。
  「なし」は 1 バイトの 0。ツール DB は ルート → ツール → グループ → サブツール。`.sut` はサブツールのノード 1 つで、ルートがそのノード。
  どこからも参照されない孤立ノードが残ることがある（CSP 自身が残したもの）。
- **設定**: `NodeVariantID` が今の設定、`NodeInitVariantID` が初期設定（「初期設定に戻す」の値）の `Variant.VariantID`。
  `Manager.CommonVariantID` は全サブツール共通の設定。ツールとグループは `NodeVariantID` 0。
- **種類**: `NodeInputOp` / `NodeOutputOp` の組。出力 10・11・14 は描画系（ペン・鉛筆・筆・エアブラシ）、12 消しゴム、13 色混ぜ、
  20 選択範囲、31・50・51 塗りつぶし、52・54 グラデーション、30・32・33 図形、70 テキスト、61・62 フキダシ、60・63 コマ枠、
  64・65 効果線、53・80〜85 ベクター編集、90〜93 定規、1〜3 表示、41 スポイト、120 ゆがみ（`ToolKind::of`）。
- **列**: `Variant` は観察された 530 列。意味・値の範囲・確度は [`subtool-columns.tsv`](subtool-columns.tsv)
  （`tools/assets/subtool_columns.py` で生成）。`.sut` の `Variant` は使う列だけを持つ（列のない設定は既定値）。
- **同梱素材**: `MaterialFile.FileData` は素材フォルダーの tar（`catalog.zip`、`info.zip`、`data/*`、`thumbnail/thumbnail.png`、
  `icedata/layerData.xml` の順）。CSP の tar は モード 0777、所有者 `compress`、チェックサムは 7 桁と NUL、時刻は素材データの更新時刻。
  `OriginalPath` は参照のパス、`CatalogPath` はユーザーの素材のとき `.:56:16:<uuid>`、`InstallFolder` は CSP 同梱の素材で 1。

### 2.1 バイナリ列（確認済み: バイト一致、W1）

全 41,056 件が 4 つの形に収まる（`subtool/blob.rs`）。すべてビッグエンディアン。

| 形 | 列 | 中身 |
|---|---|---|
| レコード | `VariantShowParam`、`BrushInOutTarget`、`IncludeFillColor`、`ContourFillBlendGraph`、`*FixedPointArray`、`Manager.PressureGraph` | `u32 12, u32 個数, u32 大きさ` とレコード。4 バイトは項目 ID、12 バイトは（項目 ID、有効、値）、16 バイトは `f64 x, y` の点 |
| 影響元設定 | `*Effector`（`BrushRotationEffector` などの整数の列を除く） | ヘッダー 40/44/48 バイト: 大きさ、使える入力、使う入力（0x10 筆圧、0x20 傾き、0x40 速度、0x80 ランダム、0x100 未解明）、入力ごとの最小値（%）、0、筆圧・傾きのグラフの大きさ、[44] 値（100 か 500、未解明）、[48] 速度のグラフの大きさ。続いてグラフ（レコード形の点） |
| 素材の列 | `*PatternImageArray`、`*TextureImage` | `u32 8, u32 個数`、素材ごとに `u32 大きさ`、パス（UTF-16LE、NUL 終端）、種類（3 同梱、2 ユーザー）、名前、1、[新しい形式] インストール先のパスと 0 |
| グラデーション | `GradationData` | 下の 4 章 |

## 3. 型付き直列化（`.laf` `.las` `.sgs`）（確認済み: バイト一致、W1）

```text
object = fields(head) fields(body)       head は "typename"（クラス名、空のことが多い）
fields = 個数, (キー, タグ, 値) × 個数
数     = u8 n, n バイトのビッグエンディアン（最小の幅）
タグ   0x06 文字列, 0x07 バイト列, 0x11 真偽, 0x13 整数（負なら 4 バイトの 2 の補数）, 0x14 符号なし,
       0x15 実数（f64）, 0x48 配列（u32 長さ, fields, 個数, (タグ 値) × 個数）, 0x49 オブジェクト（u32 長さ, object）
```

ファイルはタグも長さもない object 1 つ。後ろのバイトは CSP が無視する（同梱の 1 ファイルに 21 バイトあった）ので保持する。
JSON の形（`cspf asset dump`）は `serial.rs` の末尾を参照。

- **オートアクション**: `actionSetName`、`applicationUuid`、`actionArray`（`actionName`、`actionUuid`、`actionOpenArray`、
  `actionItemArray`（`actionItemKind`、`actionItemEnable`、`actionItemShowSettings`、`actionItemOpenParameter`、`actionItemParameter`））。
  コマンドの一覧は [`action-kinds.tsv`](action-kinds.tsv)。
- **グラデーションセット**: `gradationSetArray`（`gradationSetName`、`gradationItemArray`（`gradationName`、`gradationData`））。

## 4. GradationData（確認済み: バイト一致、W1）

```text
u32 ヘッダーの大きさ (16 か 24)  u32 色の点の大きさ (28)  u32 点の数  u32 16
[24: i32 混色 (0 通常, 1 知覚的, 2 知覚的 (Photoshop 互換), 3 リニア (Photoshop 互換)), i32 輝度補正]
点 × 数: u32 R, G, B, A（16 ビット値を上下に 2 回）, u32 フラグ（1 描画色, 2 背景色, 0x10 曲線）, u32 位置 (0..0x8000), u32 曲線の点の数
曲線の点（フラグ 0x10 の点の順）: f64 x, f64 y
```

サブツールの `GradationData`、`.sgs`、`.clip` のグラデーションレイヤーで共通。

## 5. カラーセット（`.cls`）（確認済み: バイト一致、W1）

リトルエンディアン。`"SLCC"`、`u16 版 (0x0100)`、`u32 n` と名前の塊（`u16 長さ` Shift_JIS の名前、`u32 0`、`u16 長さ` UTF-8 の名前）、
`u32 4`（色の形式）、`u32 色数`、`u32 色の並びのバイト数`、色ごとに `u32 n`、`R G B A`（A 0 は透明色）、`u32 フラグ`（1 なら名前付き）、
名前付きなら `u16 長さ` と UTF-16LE の名前。

## 6. 素材フォルダー（確認済み: 読み取り、W1）

`CLIPStudioCommon/Material/<2 桁>/<2 桁>/<uuid>/`。

- `catalog.xml`: `<archive><version/><catalog uuid>` に種類（`material` 1 件 / `catalog` セット）、名前、作成ツール、
  `<groups>` の入れ子に `<items><item uuid>`（種類 `image` `brush` `pose` …、名前、日付、`thumbnail` `data` `icedata` のファイル参照）、
  `<files><file id encrypt mime><path>`。
- `info.xml`、`icedata/layerData.xml`: `<infolist><info uuid><datalist key><data>`（タグ、素材パレットのフォルダー、タイリング、トーンの設定など）。
- `catalog*.cac`: `catalog.xml` のキャッシュ（`u16` 長さ + UTF-8 の文字列の並び、版 `1.0.1.x`）。
- `data/*`、`catalog.zip`、`info.zip`: C2F の外枠（`u32 長さ (LE)`、種類、データ、`u32 CRC-32 (LE)`。`HEAD`、`dATA` × n、`TAIL`）。
  中身は CELSYS が暗号化している。**復号しない**（要求 C5）。
- `xxx.dat`（20 バイト）、`asxx.dat`（17 バイト）: 状態ファイル（難読化、未解明）。`created.dat` / `downloaded.dat`: 空の印。
- CSP のインストール先 `Material/InstallPathToCatalogId.dat`: 旧形式の素材パスからインストール済みの素材パスへの対応表
  （`u64 個数`、`(u32 長さ, 文字列) × 2` の組、ビッグエンディアン）。

## 7. 未解明の一覧

- 影響元設定の入力ビット 0x100、ヘッダーの語 [10]（100 / 500）
- `BrushInOutTarget` の値の意味の細部、`IncludeFillColor` の 16 バイトのレコード
- CSP が `.sut` に値のない列を残す規則
- `.cls` の Shift_JIS の名前の後ろの `u32`、色の形式の語
- 素材の `xxx.dat` / `asxx.dat`
- サブツールの列のうち「推測（直訳）」の 296 列と「未解明」の 6 列（主にペン・ブラシ以外のツール）
