# 07 作品（.cmc）

`.cmc` は複数ページの作品。中身は素の SQLite のデータベースで（CSFCHUNK に包まれていない）、各ページの `.clip` が同じフォルダーにある。
実装: `crates/csp-format/src/cmc.rs`。確度: 確認済み（V1: 42 件、ページの並びと参照がすべて閉じる）。

## `Project`（1 行）

作品の設定。主な列（全列は [スキーマ目録](schema-catalog.md) の `Project`）:

| 列 | 意味 | 確度 |
|---|---|---|
| `ProjectInternalVersion` | 内部の版 | 確認済み |
| `ProjectRootCanvasNode` | ページの並びのルート（`CanvasNode.MainId`） | 確認済み |
| `MaxCanvasFileIndex` | ページのファイル名に使った最大の番号。新しいページのファイルは `page(番号 + 1).clip` | 確認済み |
| `ComicStory*` | 作品名・話数・作者名・ノンブルの書式。各ページの `Canvas` の同じ列はこの写し | 確認済み |
| `ComicBindPosition`、`ComicStartPage`、`ComicCoverExist` | 綴じる位置（0・1。どちらが右綴じかは CSP で未確認）、開始ページ、表紙の有無 | 推測 |
| `DefaultPage*` | ページを足すときの既定（単位・大きさ・解像度・見開き・テンプレート・トンボ） | 確認済み（大きさ・単位・解像度）、ほかは推測 |

## `CanvasNode`（ページの並び、確認済み）

```text
ルート（Type 0）: FirstChildIndex → 最初のページ、SelectedIndex → 選んでいるページ
ページ（Type 2）: NextIndex → 次のページ（0 で終わり）
                  LinkPath ".:pageNNNN.clip"（":" が区切り、"." は .cmc のフォルダー）
                  PageFlag 1 = 見開きのページ
```

- ページの順は連結リストの順で、`MainId` やファイル名の番号とは関係ない。ページを並べ替えると `NextIndex` だけが変わり、ファイルは名前を保つ
- `LinkWidth`、`LinkHeight`、`LinkUnit`、`LinkResolution`、`LinkWrite*`（年月日時分秒）は、ページのファイルの大きさと保存日時の控え（推測）。
  CSP 自身のファイルでも 0 や崩れた値の行があり、読み手はページのファイルそのものを見る
- `MainId` の最大値は `ElemScheme`（`CanvasNode`）の `MaxIndex` にも記録される

## ページの `.clip` との関係

- 各ページの `Canvas.ComicPageIndex` が作品の中の位置（0 から、見開きは 2 ページ分）。左右はこの位置が 1 つ進むごとに入れ替わる
- 作品情報のレイヤー（03 章）の番号と位置は、CSP がページを開くときに作品の設定に合わせ直す
- ページの一覧の表示には、各ページの `CanvasPreview` が使われる（白紙のままだと一覧が白紙に見える）。
  `CanvasNode.CanvasThumbnail` は 42 件のどのページにも入っていない。`cmc.rs` の `page_thumbnail` はページのファイルのプレビューを返す（42 件・437 ページすべてで取得できた）

- 左右の印（コーパス 42 件・437 ページ。位置は連結リストで数え、見開きは 2 つ進める）: `ComicIsLeftPage` は位置と `Project.ComicStartPage` の和が偶数のとき 1。
  列のあるページで外れたのは 2 ページだけ（保存し直されていない古いページとみられる）。
  `CropFrameOptionFlag` のビット 2 が左のページ、4 が右のページ、1 が見開き（16 など他のビットはトンボの設定で、そのまま残す）
- `.cmc` だけを並べ替えると、ページのファイルの `ComicPageIndex`・`ComicIsLeftPage`・`CropFrameOptionFlag`・作品情報のレイヤーは古いまま残る。
  CSP は並べ替えた作品を正しい順で開き、ページを開いた時点でこれらを直して「変更されています」と保存を尋ねる（V6 8 章）。
  ページのファイルを開かずに使う読み手（ノンブルを読む道具など）は、位置を `.cmc` の連結リストから数える

CSP で開いての確認は V6 の 8 章。
