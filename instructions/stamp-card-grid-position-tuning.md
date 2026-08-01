# 実装指示書: スタンプ配置グリッドの微調整

## 背景・目的

管理者が独自デザインの台紙画像（PR C以降、`card_background`設定時にアプリの飾り枠・タイトルを描画しない仕組み）を実際にアップロードして確認したところ、スタンプの配置位置を台紙のデザインに合わせて微調整したいという要望があった。キャンバス全体のサイズ（横幅・同じ部屋数における縦幅）は変えず、グリッド内部の間隔・位置のみを調整する。

基本設計は [docs/architecture.md 23節「追記: スタンプ配置グリッドの微調整（今後の拡張）」](../docs/architecture.md#追記-スタンプ配置グリッドの微調整今後の拡張)を参照。

**スコープ外（今回は対応しない）**: 部屋ごとのスタンプ配置座標を管理画面から個別にカスタマイズできるようにする機能。今回はコード上の固定グリッド定数を一律で調整するのみ。

## 実装対象ファイル

- `src/services/stamp_card_service.rs` — グリッド定数（`CELL_WIDTH`・`CELL_HEIGHT`・`TITLE_AREA_HEIGHT`・`PADDING`）を変更し、`PADDING`を`PADDING_X`（横方向）・`PADDING_Y`（縦方向）に分割する。既存の座標依存テストの期待値を、新しい定数に基づいて再計算した値に更新する

他のファイルは変更しない。

## 実装仕様

### 定数の変更

```rust
const CELL_WIDTH: i32 = 140;   // 変更前 160
const CELL_HEIGHT: i32 = 89;   // 変更前 100
const TITLE_AREA_HEIGHT: i32 = 80; // 変更前 60
const PADDING_X: i32 = 50;     // 新設（旧 PADDING の横方向用途を置き換え。変更前は PADDING=20）
const PADDING_Y: i32 = 21;     // 新設（旧 PADDING の縦方向用途を置き換え。変更前は PADDING=20）
```

`PADDING`という単一定数は削除し、用途に応じて`PADDING_X`・`PADDING_Y`のいずれかに置き換える。

### 計算式の変更

キャンバスサイズ（`render_png`冒頭付近）:

```rust
let width = (COLUMNS * CELL_WIDTH + PADDING_X * 2) as u32;
let height = (TITLE_AREA_HEIGHT + rows * CELL_HEIGHT + PADDING_Y * 2) as u32;
```

各マスの中心座標（`render_png`内のループ）:

```rust
let center_x = PADDING_X + col * CELL_WIDTH + CELL_WIDTH / 2;
let center_y = TITLE_AREA_HEIGHT + PADDING_Y + row * CELL_HEIGHT + CELL_HEIGHT / 2;
```

計算式の構造自体（`PADDING`が`PADDING_X`/`PADDING_Y`に変わる以外）は変更しない。

### 新しい座標（検算済み、テスト値の根拠）

5部屋（3列2行）の場合の中心座標:

| 部屋 | 変更前 | 変更後 |
|:--|:--|:--|
| 1部屋目（col0, row0） | (100, 130) | (120, 145) |
| 2部屋目（col1, row0） | (260, 130) | (260, 145) |
| 3部屋目（col2, row0） | (420, 130) | (400, 145) |
| 4部屋目（col0, row1） | (100, 230) | (120, 234) |
| 5部屋目（col1, row1） | (260, 230) | (260, 234) |

キャンバスサイズ:
- 横幅: 常に520px（変更前後で不変。`3*140+2*50 = 3*160+2*20 = 520`）
- 15部屋（5行）時の高さ: 変更前`60+5*100+2*20=600` → 変更後`80+5*89+2*21=567`
- 0部屋（1行、最小）時の高さ: 変更前`60+1*100+2*20=200` → 変更後`80+1*89+2*21=211`

## 既存テストの期待値更新（TDDの起点）

`src/services/stamp_card_service.rs`のテストモジュール内、以下のテストが**座標定数に依存しており、期待値の更新が必要**。新しい定数のもとでは現在の期待値のままだと失敗する（Red）ので、まずテストの期待値を以下の新しい値に書き換え、そのあとに実装（定数変更）を行うこと（Green）。

- [ ] `render_empty_card_returns_png_with_expected_dimensions`: `(520, 600)` → `(520, 567)`
- [ ] `stamped_first_cell_has_outer_ring_at_top`: `get_pixel(100, 88)` → `get_pixel(120, 103)`
- [ ] `stamped_first_cell_has_inner_ring_at_top`: `get_pixel(100, 96)` → `get_pixel(120, 111)`
- [ ] `stamped_first_cell_keeps_gap_between_rings_unfilled`: `get_pixel(100, 93)` → `get_pixel(120, 108)`
- [ ] `empty_first_cell_has_ring_outline_at_top`: `get_pixel(100, 88)` → `get_pixel(120, 103)`
- [ ] `empty_first_cell_center_remains_card_background`: `get_pixel(100, 130)` → `get_pixel(120, 145)`
- [ ] `stamped_cells_are_ringed_in_visit_order`: `get_pixel(420, 88)` → `get_pixel(400, 103)`、`get_pixel(100, 188)` → `get_pixel(120, 192)`
- [ ] `zero_total_rooms_renders_one_cell_without_panicking`: `(520, 200)` → `(520, 211)`
- [ ] `custom_stamp_image_replaces_generated_stamp_at_cell_center`: `get_pixel(100, 130)` → `get_pixel(120, 145)`
- [ ] `stamp_without_custom_image_keeps_generated_stamp_ring`: `get_pixel(100, 88)` → `get_pixel(120, 103)`

以下のテストは**座標定数に依存しない（フレーム・タイトル文字は`BORDER_MARGIN`・`BORDER_GAP`・キャンバス横幅から算出され、横幅は変更前後で520pxのまま不変のため）ので変更不要**。そのまま通ることを確認すればよい:

- `title_area_point_away_from_text_and_frame_is_card_background`（`(40, 40)`のまま）
- `outer_card_frame_is_stamp_color`（`(260, 10)`のまま）
- `inner_card_frame_is_stamp_color`（`(260, 16)`のまま）
- `custom_card_background_replaces_default_background`
- `custom_card_background_keeps_frame_pixels_from_background_image`
- `custom_card_background_keeps_title_area_pixels_from_background_image`
- `missing_card_background_keeps_default_background`

上記3件の`custom_card_background_*`テストは`solid_image(CUSTOM_BACKGROUND_COLOR, 520, 600)`を入力にしているが、`render_png`内部で`resize_to_fill`により実際のキャンバスサイズ（15部屋なら520x567）に自動的に引き伸ばされるため、入力フィクスチャのサイズ（520x600）を実際のキャンバスサイズに合わせて修正する必要はない（そのままで良い）。

新規のテストケース追加は不要（既存テストの期待値更新のみで、グリッド定数の変更を過不足なく検証できる）。

## 制約・注意事項

- キャンバス横幅は部屋数に依らず変更前後で520pxのまま変えないこと（既にアップロード済みの台紙画像デザインとの横方向の互換性を保つため）
- スタンプの描画ロジック自体（リング半径・カスタム画像の円形クロップ・回転演出等）は変更しない。変更するのはグリッド定数と中心座標の計算式のみ
- `draw_card_frame`・`draw_card_title`の中身、`BORDER_MARGIN`・`BORDER_GAP`等の枠・タイトル関連の定数は変更しない
- 部屋ごとの配置を管理画面から個別カスタマイズできるようにする機能は今回もスコープ外

## 完了条件

- [ ] 上記10件のテストについて、実装前に期待値を新しい値へ書き換え、その時点でテストが失敗する（Red）ことを確認した
- [ ] 定数変更（`CELL_WIDTH`・`CELL_HEIGHT`・`TITLE_AREA_HEIGHT`の値変更、`PADDING`の`PADDING_X`/`PADDING_Y`への分割）を行い、テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ローカルの管理画面で、実際に台紙画像（独自デザイン）とスタンプをアップロードし、`/public/stamp-card/{token}`で新しい座標（1部屋目=(120,145)等）にスタンプが描画されることを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
