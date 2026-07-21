# 実装指示書: スタンプカードのデザイン刷新（はんこ風スタンプ＋台紙風カード）

## 背景・目的

#25でリリースしたスタンプカード画像（`stamp_card_service::render_png`）は、白背景に矩形の塗りつぶしを並べただけの見た目で、「チープに見える」というフィードバックがあった。以下の2点を変更し、実物のスタンプカードに近い見た目にする。

1. 押印済みマスを、塗りつぶし矩形から実際のはんこ（判子）に近い二重丸のスタンプ意匠に変更する
2. カード全体の背景を白から台紙風のクリーム色にし、外周に飾り枠と「スタンプカード」というタイトルを追加する

基本設計は [docs/architecture.md 23節「スタンプ状況（スタンプカード画像）」の`stamp_card_service::render_png`節](../docs/architecture.md#stamp_card_servicerender_png)（改訂済み）を参照。今回の変更は`stamp_card_service.rs`の描画ロジックのみが対象で、DBスキーマ・APIエンドポイント・`game_service`/`line_client`側は一切変更しない。

## 実装対象ファイル

- `src/services/stamp_card_service.rs` — `render_png`の描画ロジックを刷新する。既存の`truncate_room_name`はそのまま流用し、新たに以下を追加する
  - 文字を2行に分割するヘルパー
  - 部屋名から回転角を決定論的に算出するヘルパー
  - カード全体のタイトル領域・二重線の飾り枠を描く処理

他のファイルの変更は不要（ルーティング・DB・`ReplyMessage`等は無変更）。

## テストケース（TDDの起点）

`src/services/stamp_card_service.rs`内の既存テスト（矩形の隅・中心のピクセル色を検証していたもの）は、新しいデザインに合わせて以下に置き換える。背景色が白（`[255,255,255,255]`）からクリーム色（`[0xFB,0xF3,0xE7,255]`）に変わる点に注意（「背景色のままであること」を検証する既存ケースは新しい背景色の期待値に置き換える）。

- [ ] ケース1（寸法・破壊的変更）: `render_png(&[], 15)` の画像サイズは幅520px・高さ600px（横幅は従来のまま、縦はタイトル領域60px分だけ増える。3列×5行、`height = 60 + rows * CELL_HEIGHT + PADDING * 2`）
- [ ] ケース2: `render_png(&["図書室".to_string()], 15)` の1マス目（スタンプ済み）で、セル中心から真上に42px移動した点（外側リングの帯の中心）が`[0xB5, 0x4B, 0x3A, 255]`であること
- [ ] ケース3: 同じマスで、セル中心から真上に34px移動した点（内側リングの帯の中心）も`[0xB5, 0x4B, 0x3A, 255]`であること
- [ ] ケース4: 同じマスで、セル中心から真上に37px移動した点（二重リングの隙間、塗りつぶされていないこと）が背景色`[0xFB, 0xF3, 0xE7, 255]`のままであること
- [ ] ケース5: `render_png(&[], 15)`（訪問済み部屋なし）の1マス目（未スタンプ）で、セル中心から真上に42px移動した点が輪郭色`[0xE2, 0xE4, 0xE9, 255]`であること（未スタンプのマスにも円の輪郭が描かれる仕様の確認）
- [ ] ケース6: 同じ未スタンプのマスの中心そのもの（オフセット0px）は背景色`[0xFB, 0xF3, 0xE7, 255]`のままであること
- [ ] ケース7（訪問順の確認）: `render_png(&["A".to_string(), "B".to_string(), "C".to_string()], 5)`で、3マス目（インデックス2、スタンプ済み）はセル中心から真上42pxの点がスタンプ色、4マス目（インデックス3、未訪問）は同じ点が輪郭色であること
- [ ] ケース8（回帰）: `render_png(&[], 0)`のように`total_rooms`が0以下でもpanicしないこと（`total_rooms`は1として扱われる、既存仕様のまま）
- [ ] ケース9（新規ヘルパー）: 部屋名（切り詰め後、最大6文字）を1〜2行に分割するヘルパー関数（例: `split_stamp_label_lines`）が、3文字以下の文字列はそのまま1行の配列で返し、4文字以上の文字列は前半・後半の2行に分割する（前半の文字数は`chars.len().div_ceil(2)`、奇数文字数のときは前半に多く割り当てる）ことを純粋関数として直接検証する（例: `"図書室"`（3文字）→`["図書室"]`、`"とても長い…"`(6文字)→`["とても長", "い…"]`）
- [ ] ケース10（新規ヘルパー）: 回転角度を算出するヘルパー関数（例: `stamp_rotation_degrees`）が、同じ部屋名に対して常に同じ値を返す（決定論的である）ことと、返り値が`-8.0..=8.0`の範囲に収まることを確認する
- [ ] ケース11（背景色）: `render_png(&[], 15)`の、どの円・枠線にも重ならない点（例: 座標`(40, 40)`。タイトル領域内だがタイトル文字・飾り枠のどちらとも重ならない位置）が背景色`[0xFB, 0xF3, 0xE7, 255]`であること
- [ ] ケース12（飾り枠）: `render_png(&[], 15)`の外枠線上の点（画像上端中央、座標`(260, 10)`）が飾り枠の色`[0xB5, 0x4B, 0x3A, 255]`であること
- [ ] ケース13（飾り枠・内側の線）: 同じ画像の内枠線上の点（座標`(260, 16)`）も飾り枠の色`[0xB5, 0x4B, 0x3A, 255]`であること

`truncate_room_name`自体（切り詰めロジック）は変更しないため、既存のテストはそのまま残してよい。画像内の文字（部屋名・タイトル）がピクセル単位で正しく描画されているかを検証するテストは、今回も書かない（#25の指示書と同じ方針）。ケース2〜7・11〜13で使う座標・色の期待値は下記の参考実装から導出したものなので、実装を変える場合はテストの期待値も整合させること。

## 実装仕様

### `src/services/stamp_card_service.rs`

以下は参考実装。

```rust
use std::io::Cursor;
use std::sync::LazyLock;

use ab_glyph::{FontRef, PxScale};
use image::{ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::geometric_transformations::{Interpolation, rotate_about_center};
use imageproc::rect::Rect;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Bold.ttf");

static FONT: LazyLock<FontRef<'static>> =
    LazyLock::new(|| FontRef::try_from_slice(FONT_BYTES).expect("bundled font must be valid"));

const COLUMNS: i32 = 3;
const CELL_WIDTH: i32 = 160;
const CELL_HEIGHT: i32 = 100;
const PADDING: i32 = 20;
const MAX_NAME_CHARS: usize = 6;

// カード全体（台紙風デザイン）
const TITLE_AREA_HEIGHT: i32 = 60;
const BORDER_MARGIN: i32 = 10;
const BORDER_GAP: i32 = 6;
const CARD_TITLE: &str = "スタンプカード";

const CARD_BACKGROUND: Rgba<u8> = Rgba([0xFB, 0xF3, 0xE7, 255]);
const CARD_BORDER_COLOR: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);

const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);
const STAMP_COLOR: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);
const EMPTY_BORDER: Rgba<u8> = Rgba([0xE2, 0xE4, 0xE9, 255]);

// スタンプ本体を描く一時バッファのサイズ。外側リングの外縁半径(44px)を
// 自身の中心を軸に回転させても外接円の大きさは変わらないため、
// アンチエイリアシングの縁のにじみを吸収できる程度の余白があれば十分。
const STAMP_BUFFER_SIZE: u32 = 96;

const OUTER_RING_OUTER_RADIUS: i32 = 44;
const OUTER_RING_INNER_RADIUS: i32 = 40;
const INNER_RING_OUTER_RADIUS: i32 = 35;
const INNER_RING_INNER_RADIUS: i32 = 33;

const EMPTY_RING_OUTER_RADIUS: i32 = 43;
const EMPTY_RING_INNER_RADIUS: i32 = 41;

pub fn render_png(room_names: &[String], total_rooms: i64) -> Vec<u8> {
    let total_rooms = total_rooms.max(1) as i32;
    let rows = total_rooms.div_ceil(COLUMNS);
    let width = (COLUMNS * CELL_WIDTH + PADDING * 2) as u32;
    let height = (TITLE_AREA_HEIGHT + rows * CELL_HEIGHT + PADDING * 2) as u32;

    let mut image: RgbaImage = ImageBuffer::from_pixel(width, height, CARD_BACKGROUND);

    draw_card_frame(&mut image, width, height);
    draw_card_title(&mut image, width);

    for i in 0..total_rooms {
        let col = i % COLUMNS;
        let row = i / COLUMNS;
        let center_x = PADDING + col * CELL_WIDTH + CELL_WIDTH / 2;
        let center_y = TITLE_AREA_HEIGHT + PADDING + row * CELL_HEIGHT + CELL_HEIGHT / 2;

        match room_names.get(i as usize) {
            Some(name) => draw_stamp(&mut image, center_x, center_y, name),
            None => draw_empty_ring(&mut image, center_x, center_y),
        }
    }

    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("PNG encoding should not fail");
    output.into_inner()
}

fn draw_card_frame(image: &mut RgbaImage, width: u32, height: u32) {
    let outer = Rect::at(BORDER_MARGIN, BORDER_MARGIN).of_size(
        width - (BORDER_MARGIN * 2) as u32,
        height - (BORDER_MARGIN * 2) as u32,
    );
    draw_hollow_rect_mut(image, outer, CARD_BORDER_COLOR);

    let inner_margin = BORDER_MARGIN + BORDER_GAP;
    let inner = Rect::at(inner_margin, inner_margin).of_size(
        width - (inner_margin * 2) as u32,
        height - (inner_margin * 2) as u32,
    );
    draw_hollow_rect_mut(image, inner, CARD_BORDER_COLOR);
}

fn draw_card_title(image: &mut RgbaImage, width: u32) {
    let scale = PxScale::from(28.0);
    let approx_width = CARD_TITLE.chars().count() as i32 * scale.x as i32;
    let x = (width as i32 - approx_width) / 2;
    draw_text_mut(image, CARD_BORDER_COLOR, x, 16, scale, &*FONT, CARD_TITLE);
}

fn draw_empty_ring(image: &mut RgbaImage, center_x: i32, center_y: i32) {
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        EMPTY_RING_OUTER_RADIUS,
        EMPTY_BORDER,
    );
    draw_filled_circle_mut(
        image,
        (center_x, center_y),
        EMPTY_RING_INNER_RADIUS,
        CARD_BACKGROUND,
    );
}

fn draw_stamp(image: &mut RgbaImage, center_x: i32, center_y: i32, name: &str) {
    let buffer_center = (STAMP_BUFFER_SIZE / 2) as i32;
    let mut buffer: RgbaImage =
        ImageBuffer::from_pixel(STAMP_BUFFER_SIZE, STAMP_BUFFER_SIZE, TRANSPARENT);

    draw_filled_circle_mut(
        &mut buffer,
        (buffer_center, buffer_center),
        OUTER_RING_OUTER_RADIUS,
        STAMP_COLOR,
    );
    draw_filled_circle_mut(
        &mut buffer,
        (buffer_center, buffer_center),
        OUTER_RING_INNER_RADIUS,
        TRANSPARENT,
    );
    draw_filled_circle_mut(
        &mut buffer,
        (buffer_center, buffer_center),
        INNER_RING_OUTER_RADIUS,
        STAMP_COLOR,
    );
    draw_filled_circle_mut(
        &mut buffer,
        (buffer_center, buffer_center),
        INNER_RING_INNER_RADIUS,
        TRANSPARENT,
    );

    let label = truncate_room_name(name);
    let lines = split_stamp_label_lines(&label);
    let scale = PxScale::from(20.0);
    let line_height = 22;
    let start_y = buffer_center - (lines.len() as i32 * line_height) / 2;
    for (idx, line) in lines.iter().enumerate() {
        // 文字数ベースの簡易的な中央寄せ。ピクセル単位の厳密な計測はしない
        // （render_pngのテスト方針として、文字の描画位置はピクセル単位で検証しないため）。
        let approx_width = line.chars().count() as i32 * scale.x as i32;
        draw_text_mut(
            &mut buffer,
            STAMP_COLOR,
            buffer_center - approx_width / 2,
            start_y + idx as i32 * line_height,
            scale,
            &*FONT,
            line,
        );
    }

    let theta = stamp_rotation_degrees(name).to_radians();
    let rotated = rotate_about_center(&buffer, theta, Interpolation::Bilinear, TRANSPARENT);

    let offset_x = (center_x - buffer_center) as i64;
    let offset_y = (center_y - buffer_center) as i64;
    image::imageops::overlay(image, &rotated, offset_x, offset_y);
}

fn truncate_room_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= MAX_NAME_CHARS {
        return name.to_string();
    }
    let mut truncated: String = chars[..MAX_NAME_CHARS - 1].iter().collect();
    truncated.push('…');
    truncated
}

fn split_stamp_label_lines(label: &str) -> Vec<String> {
    let chars: Vec<char> = label.chars().collect();
    if chars.len() <= 3 {
        return vec![label.to_string()];
    }
    let half = chars.len().div_ceil(2);
    vec![
        chars[..half].iter().collect(),
        chars[half..].iter().collect(),
    ]
}

fn stamp_rotation_degrees(name: &str) -> f32 {
    let sum: u32 = name.bytes().map(u32::from).sum();
    (sum % 17) as f32 - 8.0
}
```

補足:

- `draw_filled_circle_mut`で外側の色→内側を透明（または背景色）で塗りつぶす「くり抜き」を2段階行うことで、指定した帯幅のリングを表現している（`imageproc`に指定線幅つきの円描画APIが無いため）
- `rotate_about_center`は自身の中心を軸に画像全体を回転させる。円（リング）は中心を軸に回転させても占める画素の集合が変わらない（回転対称）ため、ケース2〜5・7で「セル中心から特定距離だけ離れた点」を検証する際、部屋名によって回転角が変わっても期待値は変わらない。テキストだけが回転によって向きを変えるが、本指示書ではテキストの描画位置をピクセル単位で検証しないため問題にならない
- `image::imageops::overlay`のシグネチャ（座標の型が`i64`か`u32`か等）はcrateのバージョンによって異なることがある。`cargo build`でエラーが出た場合は座標の型をシグネチャに合わせて調整すること
- `draw_hollow_rect_mut`・`Rect::at(x, y).of_size(w, h)`は#25の実装でも使用済みの標準API
- 座標の補足（テストケース2〜7で使う値）: 1マス目（インデックス0、`col=0, row=0`）の中心 = `(PADDING + 0 + CELL_WIDTH/2, TITLE_AREA_HEIGHT + PADDING + 0 + CELL_HEIGHT/2) = (20 + 80, 60 + 20 + 50) = (100, 130)`。4マス目（インデックス3、`col=0, row=1`）の中心 = `(100, 230)`
- 座標の補足（ケース12・13で使う値）: `total_rooms=15`のとき`width=520, height=600`。外枠の上辺は`y = BORDER_MARGIN = 10`、内枠の上辺は`y = BORDER_MARGIN + BORDER_GAP = 16`。画像中央のx座標は`width/2 = 260`

## 制約・注意事項

- 新規crateの追加は不要（`imageproc`・`ab_glyph`は#25で追加済み）。`imageproc::geometric_transformations`は`imageproc`本体に含まれるモジュールで、追加のfeatureフラグは不要なはず（`cargo build`で見つからない場合はバージョンを確認すること）
- `GET /public/stamp-card/{token}`のハンドラー・DBアクセス・`game_service`/`line_client`側のロジックは一切変更しない。今回のスコープは`stamp_card_service::render_png`の描画内容のみ
- 画像の高さがタイトル領域の分だけ増える（破壊的変更）。LINEの画像メッセージ側（`originalContentUrl`）はサイズ固定を前提にしていないため、この高さ変更によるLINE側の追従修正は不要
- 未スタンプのマスにも輪郭円を描く仕様・背景色がクリーム色になる仕様変更により、旧仕様（白背景・未スタンプは何も描画しない）を前提にしたテストが残っていれば、新仕様に合わせて書き換える
- 部屋名・タイトル文字がピクセル単位で正しく描画されているかを検証するテストは書かない（#25の指示書と同じ方針。文字の描画位置・折り返しはこの指示書のケース9のような文字列操作としてのみテストする）

## 完了条件

- [ ] 上記13テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] 実際に生成されたPNG画像を目視確認し（例: ローカルで`/public/stamp-card/{token}`にアクセスするか、テストコードから一時ファイルに書き出す）、台紙風の背景・二重線の飾り枠・タイトル・二重丸のはんこ風スタンプが意図通り表示されていることを確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
