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

## 2. サブツール（`.sut` / `EditImageTool.todb` / `.bfps`）（確認済み: 構造 W1、読み込み W3）

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
- **読み込み**（確認済み: W3）: CSP は読み込んだサブツールに**ファイル名**を付ける（`NodeName` は使わない）。
  読み込んだ設定は今の形式にそろえられ（影響元設定のヘッダーは 48 バイト、ない列は既定値）、同梱素材はユーザーの素材フォルダーに登録し直される。

### 2.1 バイナリ列（確認済み: バイト一致、W1）

全 41,056 件が 4 つの形に収まる（`subtool/blob.rs`）。すべてビッグエンディアン。

| 形 | 列 | 中身 |
|---|---|---|
| レコード | `VariantShowParam`、`BrushInOutTarget`、`IncludeFillColor`、`ContourFillBlendGraph`、`*FixedPointArray`、`Manager.PressureGraph` | `u32 12, u32 個数, u32 大きさ` とレコード。4 バイトは項目 ID、12 バイトは（項目 ID、有効、値）、16 バイトは `f64 x, y` の点 |
| 影響元設定 | `*Effector`（`BrushRotationEffector` などの整数の列を除く） | ヘッダー 40/44/48 バイト: 大きさ、使える入力、使う入力（0x10 筆圧、0x20 傾き、0x40 速度、0x80 ランダム、0x100 未解明）、入力ごとの最小値（%。傾きは最小値と最大値を持ち、[1] がその最小値）、0、筆圧・傾きのグラフの大きさ、[44] 傾きの最大値（100–500 %、W4 で確認）、[48] 速度のグラフの大きさ。続いてグラフ（レコード形の点） |
| 素材の列 | `*PatternImageArray`、`*TextureImage` | `u32 8, u32 個数`、素材ごとに `u32 大きさ`、パス（UTF-16LE、NUL 終端）、種類（3 同梱、2 素材フォルダーの素材。CSP の紙質「インパスト」を選ぶと 2、W4）、名前、1、[新しい形式] インストール先のパスと 0 |
| グラデーション | `GradationData` | 下の 4 章 |

## 3. 型付き直列化（`.laf` `.las` `.sgs`）（確認済み: バイト一致、W1）

```text
object = fields(head) fields(body)       head は "typename"（クラス名、空のことが多い）
fields = 個数, (キー, タグ, 値) × 個数
数     = u8 n, n バイトのビッグエンディアン（最小の幅）
タグ   0x06 文字列, 0x07 バイト列, 0x11 真偽（いつも 1 バイト: 01 00 / 01 01）, 0x13 整数（負なら 4 バイトの 2 の補数）, 0x14 符号なし,
       0x15 実数（f64）, 0x48 配列（u32 長さ, fields, 個数, (タグ 値) × 個数）, 0x49 オブジェクト（u32 長さ, object）
```

真偽だけは最小の幅にしない。CSP は 0 バイトの真偽を「真」と読む（W3: 0 バイトで書いた `actionItemEnable` の偽が、読み込むとオンになった）。

ファイルはタグも長さもない object 1 つ。後ろのバイトは CSP が無視する（同梱の 1 ファイルに 21 バイトあった）ので保持する。
JSON の形（`cspf asset dump`）は `serial.rs` の末尾を参照。

- **オートアクション**: `actionSetName`、`applicationUuid`、`actionArray`（`actionName`、`actionUuid`、`actionOpenArray`、
  `actionItemArray`（`actionItemKind`、`actionItemEnable`、`actionItemShowSettings`、`actionItemOpenParameter`、`actionItemParameter`））。
  コマンドの一覧は [`action-kinds.tsv`](action-kinds.tsv)（CSP のパレットに出る名前は確認済み、W3）。
  CSP の「セットの書き出し」は `.laf` で書き、「セットの読み込み」も `.laf` を読む（W3）。読み込むと `actionUuid` は振り直され、
  そのほか（名前、項目、オン・オフ、パラメーター）はそのまま残る。`actionOpenArray` はパレットで開いているかどうか。
- **グラデーションセット**: `gradationSetArray`（`gradationSetName`、`gradationItemArray`（`gradationName`、`gradationData`））。
  CSP には `.sgs` をファイルとして読み込む操作がない。「グラデーションの編集」の「グラデーションの読み込み」が読むのは
  `.cgs`（カラーグラデーションファイル）と Photoshop の `.grd` で、セットは素材（「セットを素材として登録」「グラデーションセットを追加」）として受け渡す。

## 4. GradationData（確認済み: バイト一致 W1、CSP の表示 W3）

```text
u32 ヘッダーの大きさ (16 か 24)  u32 色の点の大きさ (28)  u32 点の数  u32 16
[24: i32 混色 (0 通常, 1 知覚的, 2 知覚的 (Photoshop 互換), 3 リニア (Photoshop 互換)), i32 輝度補正]
点 × 数: u32 R, G, B, A（16 ビット値を上下に 2 回）, u32 フラグ（1 描画色, 2 背景色, 0x10 曲線）, u32 位置 (0..0x8000), u32 曲線の点の数
曲線の点（フラグ 0x10 の点の順）: f64 x, f64 y
```

サブツールの `GradationData`、`.sgs`、`.clip` のグラデーションレイヤーで共通。

CSP の「グラデーションの編集」との対応（W3、同梱セット「空」「効果」で確認）:

- 「位置」は 位置 / 0x8000 × 100 を丸めた値（11241 → 34、22482 → 69）。「不透明度」は A の 16 ビット値 / 65535 × 100。
- フラグ 1 は「メイン描画色」、2 は「サブ描画色」、どちらもなければ「指定色」で、RGB の値が使われる。
- フラグ 0x10 の点は「混合率曲線」がオンで、曲線の点（0..1 の x と y、両端 (0, 0) (1, 1)）がそのままグラフの点になる。
  点が 2 つ（直線）でもフラグは立つ。最後の点は曲線を持たない（グラフが使えない）。
- 混色 0 は「通常」。そのとき「輝度の補正」は使えず、値 3 が「高」と表示される。

## 5. カラーセット（`.cls`）（確認済み: バイト一致 W1、CSP での読み込みと書き出し W3）

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

## 7. 設定のデータベース（確認済み: 読み取り、W1）

サブツール以外の SQLite 形式（45 種）の表・列・型・行数・値の範囲は [`sqlite-tables.tsv`](sqlite-tables.tsv)
（`tools/assets/sqlite_census.py` で生成。数値は最小・最大、文字列と BLOB はバイト数の範囲だけを載せ、ファイルの中身は載せない）。
CSP は列を必要になったときに足すので、同じ形式でもファイルによって列が違う（`files` が少ない列）。

### 7.1 ショートカットキー（`.khc`、`.spws`）（推測: 既定のキーと照合）

`Shortcut/default.khc` の `shortcutmenu`、ワークスペース（`.spws`）では `workspaceshortcutmenu`。1 行が 1 つのコマンドとキーの組で、
キーが複数あるコマンドは複数行になる。

| 列 | 中身 |
|---|---|
| `menucommandtype` | `basiccommand`（取り消し・コピーなど）か `advancedcommand`（メニューのコマンド） |
| `menucommand` | コマンドの ID（`filesaveas` など。一部は UUID） |
| `shortcut` | キーの名前（`S`、`F2`、`DELETE`、`NUM+` …）。キーなしは文字列の `NULL` |
| `modifier` | ビット: 1 Alt、2 Shift、4 Ctrl |

修飾キーのビットは、CSP の既定のキー（環境設定 Ctrl+K、ショートカットキー設定 Ctrl+Shift+Alt+K、やり直し Ctrl+Shift+Z、
上のレイヤーを選択 Alt+]、選択範囲外を消去 Shift+Delete）と合う。ワークスペースの `workspaceshortcuttool` は、
ツールのノードの UUID（ツール DB の `Node.NodeUuid`）とキーの組。`cspf asset shortcuts FILE --tools EditImageTool.todb` で名前付きで読める。

### 7.2 修飾キー設定（`.tomd`）（推測: 既定の設定と照合）

`Shortcut/DefaultToolModifyKey.tomd` の `ModifyKeySetting`。1 行がツールの種類 1 つ（`InputOperation` / `OutputOperation` /
`RangeOperation` はツール DB の `NodeInputOp` などと同じ。すべて 0 の行は「共通設定」）で、`SettingData` はビッグエンディアンの BLOB:

```text
u32 8, u32 個数
項目 × 個数: u32 大きさ（この 12 バイトを含む）, u32 キー, u32 動作, 中身
  キー: ビット 1 Shift, 2 Ctrl, 4 Alt, 8 Space（0x100 以上はペンのボタンなど、未解明）
  動作 2: u32 0, u32 16, ツールのノードの UUID（押している間そのツールに切り替え）
  動作 1: u32 操作の番号（100 直線、200〜202・310 選択範囲の追加・削除など）
  動作 64・4・65536・131072: u32 値（64 は Ctrl+Alt のブラシサイズ変更と推測）
```

キーのビットはショートカット（7.1）と違う。CSP の既定（共通設定の Space 手のひら、Ctrl+Space ズームイン、Alt+Space ズームアウト、
Shift+Space 回転、Ctrl レイヤー移動、Ctrl+Shift レイヤー選択、ペンの Alt スポイト・Ctrl オブジェクト・Shift 直線）と合う。
`cspf asset modifier-keys FILE --tools EditImageTool.todb` で読める。

## 8. 未解明の一覧

- 影響元設定の入力ビット 0x100（CSP の影響元設定の画面には筆圧・傾き・速度・ランダムしかない）
- `BrushInOutTarget` の項目 ID のうち 1001（ブラシサイズ）・1051（厚さ）以外（W4 では並び順からの推測）、`IncludeFillColor` の 16 バイトのレコード
- CSP が `.sut` に値のない列を残す規則
- `.cls` の Shift_JIS の名前の後ろの `u32`、色の形式の語
- 素材の `xxx.dat` / `asxx.dat`
- ワークスペースの配置（`workspaceplacement` の BLOB）、修飾キー設定の 0x100 以上のキーと動作 1・2 以外の値
- `.cgs`（CSP が読み込めるカラーグラデーションファイル。手元に実物がない）、`.las` の実物
- サブツールの列のうち「推測（直訳）」の 225 列と「未解明」の 5 列（主にペン・ブラシ以外のツール）。CSP で確認済みは 179 列（W4）
