# 実装指示書: スタンプカードの文字を太く・見やすくする

## 背景・目的

#26でリリースしたスタンプカード（はんこ風デザイン）の部屋名・タイトル文字が「薄く細くて見にくい」というフィードバックがあった。使用しているフォント（`assets/fonts/NotoSansJP-Bold.ttf`）自体はBoldウェイトだが、小さいサイズ（部屋名20px・タイトル28px）で描画するとアンチエイリアシングにより線が細く見える。`imageproc::drawing::draw_text_mut`には線の太さを指定する機能が無いため、同じ文字列を数pxずらして複数回重ね描きする「疑似ボールド」で見た目の太さを補う。

基本設計は [docs/architecture.md 23節「スタンプ状況（スタンプカード画像）」の`stamp_card_service::render_png`節](../docs/architecture.md#stamp_card_servicerender_png)（改訂済み）を参照。新規フォントアセットの追加は行わない。DBスキーマ・APIエンドポイント・`game_service`/`line_client`側は今回も変更しない。

## 実装対象ファイル

- `src/services/stamp_card_service.rs` — 文字描画（部屋名・タイトル）を疑似ボールドで重ね描きするヘルパーを追加し、既存の`draw_text_mut`呼び出し（`draw_stamp`内・`draw_card_title`内）を置き換える。あわせて文字サイズ（`PxScale`）を少し大きくする

他のファイルの変更は不要。

## テストケース（TDDの起点）

文字の描画位置・太さをピクセル単位で検証するテストは書かない（#25・#26の指示書と同じ方針。フォントレンダリングの実装詳細に依存し、テストが脆くなるため）。今回追加するのは以下の1点のみで、既存のリング・背景・飾り枠のテスト（#26で書かれたもの）は変更なしで通ることを確認する。

- [ ] ケース1（回帰）: `src/services/stamp_card_service.rs`内の既存テストすべてが、文字サイズ変更後も引き続き通ること（リング・背景・飾り枠の座標はテキストの太さ・サイズ変更の影響を受けない設計になっているため、期待値の変更は不要なはず。実際に`cargo test`で確認する）

## 実装仕様

### `src/services/stamp_card_service.rs`

- 疑似ボールド用のヘルパーを追加する:
  ```rust
  const BOLD_OFFSETS: [(i32, i32); 5] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)];

  #[allow(clippy::too_many_arguments)]
  fn draw_bold_text_mut(
      image: &mut RgbaImage,
      color: Rgba<u8>,
      x: i32,
      y: i32,
      scale: PxScale,
      font: &FontRef<'_>,
      text: &str,
  ) {
      for (dx, dy) in BOLD_OFFSETS {
          draw_text_mut(image, color, x + dx, y + dy, scale, font, text);
      }
  }
  ```
- `draw_card_title`内の`draw_text_mut(image, CARD_BORDER_COLOR, x, 16, scale, &*FONT, CARD_TITLE)`を`draw_bold_text_mut(...)`に置き換える。あわせてタイトルの`PxScale::from(28.0)`を`PxScale::from(30.0)`に変更する（横幅の中央寄せ計算`approx_width`もこの新しいscaleを使うこと）
- `draw_stamp`内、2行分の部屋名を描画しているループ内の`draw_text_mut(&mut buffer, STAMP_COLOR, ...)`を`draw_bold_text_mut(&mut buffer, STAMP_COLOR, ...)`に置き換える。あわせて`PxScale::from(20.0)`を`PxScale::from(22.0)`に変更する
- 疑似ボールドは同じ文字を5回重ね描きするため描画コストが増えるが、1枚のスタンプカード画像あたり最大15マス程度・1マスにつき最大2行という規模であり、実測で許容範囲内であることを`cargo test`の実行時間で確認する程度でよい（性能テストの追加は不要）

## 制約・注意事項

- 新規crate・新規フォントアセットの追加は不要
- リング・背景・飾り枠を描画する座標・半径・色は変更しない（#26のテストがそのまま通ることを確認する）
- 文字サイズを大きくしすぎて二重リングの内側からはみ出しても、既存方針通り厳密なピクセル単位の幅計測はしないでよいが、常識的な範囲（明らかに外側リングを突き破るほど巨大にはしない）に収めること
- `GET /public/stamp-card/{token}`のハンドラー・DBアクセス・`game_service`/`line_client`側のロジックは変更しない

## 完了条件

- [ ] 上記回帰ケースを含め、`cargo test`が全体で通ることを確認した
- [ ] `cargo clippy`が警告なく通る
- [ ] 実際に生成されたPNG画像を目視確認し（例: ローカルで`/public/stamp-card/{token}`にアクセスするか、テストコードから一時ファイルに書き出す）、部屋名・タイトルの文字が以前より太く見やすくなっていることを確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
