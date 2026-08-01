# 実装指示書: 台紙カスタマイズ時のアプリ描画枠・タイトルの抑制

## 背景・目的

#28（`stamp-card-custom-images-render`）で、イベント設定からスタンプカード全体の台紙画像（`events.stamp_card_background_image_id`）をアップロードできるようにしたが、`stamp_card_service::render_png`は台紙の有無に関わらず、常にアプリ描画の二重線の飾り枠（`draw_card_frame`）とタイトル文字「スタンプカード」（`draw_card_title`）を上から重ねて描画していた。

運用担当者が独自デザインの台紙画像（タイトル・枠・装飾込みの完成品）を用意した場合、アプリ側の枠・タイトルが二重に重なってしまい、意図した見た目にならない。管理者から「台紙はラリー全体で共通のオリジナルデザインにしたい」という要望があり、台紙画像を設定した場合はアプリ側の枠・タイトルを描画しないようにする。

基本設計は [docs/architecture.md 23節「追記: 台紙カスタマイズ時のアプリ描画枠・タイトルの抑制（今後の拡張）」](../docs/architecture.md#追記-台紙カスタマイズ時のアプリ描画枠タイトルの抑制今後の拡張)を参照。要件は[docs/requirements.md](../docs/requirements.md)「スタンプカード台紙のカスタマイズ」項を参照。

**スコープ外（今回は対応しない）**: 部屋ごとのスタンプ配置座標（3列固定グリッド）を可変にする機能。台紙画像側のデザインは、既存の固定グリッド座標（`PADDING + col*CELL_WIDTH + CELL_WIDTH/2`、`TITLE_AREA_HEIGHT + PADDING + row*CELL_HEIGHT + CELL_HEIGHT/2`）に運用側で合わせ込む前提とする。

## 実装対象ファイル

- `src/services/stamp_card_service.rs` — `render_png`内で、`card_background`が`Some`のときは`draw_card_frame`・`draw_card_title`の呼び出しをスキップする

他のファイルは変更しない（`handlers::image`・`room_repository`・`room_image_repository`・`main.rs`は今回のPRで既にあるべき形になっており、変更不要）。

## テストケース（TDDの起点）

`src/services/stamp_card_service.rs`のテストモジュールに追加する。

- [ ] ケース1: `card_background`を指定して`render_png`を呼ぶと、飾り枠が描画される座標（既存テスト`outer_card_frame_is_stamp_color`・`inner_card_frame_is_stamp_color`が使っている座標、`(260, 10)`・`(260, 16)`）のピクセル色が、飾り枠の色（`STAMP_COLOR`）**ではなく**台紙画像由来の色になっていることを確認する（例: 台紙をSTAMP_COLOR・CARD_BACKGROUND・CUSTOM_STAMP_COLORのいずれとも異なる単色で塗ったテスト画像にし、その色と一致することを確認する）
- [ ] ケース2: 同条件で、タイトル文字が描画される領域（`draw_card_title`は`y=16`・`PxScale::from(30.0)`・幅520pxなら中央付近`x≈155〜365`の帯）のいずれかの点が、`STAMP_COLOR`（文字の色）ではなく台紙画像由来の色になっていることを確認する。タイトル文字はアンチエイリアス・太字重ね描き（`BOLD_OFFSETS`）があるため、ピクセル位置の厳密な予測が難しい場合は、帯の中の複数点をサンプリングしていずれも`STAMP_COLOR`と一致しないことを確認する形でもよい
- [ ] ケース3（回帰）: `card_background`が`None`のときは、従来通り飾り枠・タイトルが描画されること（既存テスト`outer_card_frame_is_stamp_color`・`inner_card_frame_is_stamp_color`・`title_area_point_away_from_text_and_frame_is_card_background`がそのまま通ることを確認すれば足りる。新規テスト追加は不要）
- [ ] ケース4（回帰）: `card_background`を指定しても、スタンプの配置座標・部屋のスタンプ描画（はんこ自動生成・カスタム画像）自体は従来通り動作すること（既存の`custom_stamp_image_replaces_generated_stamp_at_cell_center`等が引き続き通ることを確認すれば足りる。新規テスト追加は不要）

## 実装仕様

### `src/services/stamp_card_service.rs`

```rust
let mut image: RgbaImage = match card_background {
    Some(background) => background
        .resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
        .to_rgba8(),
    None => ImageBuffer::from_pixel(width, height, CARD_BACKGROUND),
};

if card_background.is_none() {
    draw_card_frame(&mut image, width, height);
    draw_card_title(&mut image, width);
}
```

`draw_card_frame`・`draw_card_title`関数自体の中身は変更しない。呼び出し元の`render_png`で条件分岐を追加するだけでよい。

## 制約・注意事項

- `card_background`が`Some`のときにスタンプ（はんこ自動生成・カスタム画像・空リング）を描画するロジックは一切変更しない。抑制するのは飾り枠とタイトルのみ
- 台紙画像未設定（`None`）時の見た目・既存テストの結果は一切変更してはならない
- ドキュメント（`docs/architecture.md`・`docs/requirements.md`・`docs/operator-guide.md`）は既に本変更を前提に更新済み。実装内容がドキュメントと矛盾しないこと

## 完了条件

- [ ] 上記4テストケースについて、実装前に失敗するテストを書いたことを確認した（Red。ケース3・4は既存テストの回帰確認のため、既存テストの内容を変更せずそのまま通ることの確認でよい）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] 管理画面で、タイトル・枠を含んだ独自デザインの台紙画像を実際にアップロードし、`/public/stamp-card/{token}`でアプリ側の枠・タイトルが重ならず、台紙画像がそのまま見えることを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
