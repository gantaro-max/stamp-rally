# 実装指示書: スタンプ状況（部屋名入りスタンプカード画像）の返信への追加

## 背景・目的

参加者が「今何個スタンプを集めたか」をLINE上で確認する手段が無いとの本番運用フィードバックを受け、訪問済み部屋に応じてスタンプが押された見た目のカード画像を、(a) 次のクエスト案内に自動的に添付する、(b) 「スタンプ状況」コマンドでいつでも参照できる、の2通りで提供する。押されるスタンプには、**その部屋の部屋名を実際に画像として焼き込む**（達成数のみの汎用カードではなく、訪問した部屋の名前が見える本物のスタンプカードに近い見た目にしたいという要望）。

基本設計は [docs/architecture.md](../docs/architecture.md) 23節（新規追加）・10節（コマンド優先順位、7番目に追加）・13節（`line_client`のメッセージ複数化）・18節（Dockerfileへの`assets`追加）を参照。要件は [docs/requirements.md](../docs/requirements.md)（プレイヤー機能「部屋の指示」「スタンプ状況の確認」）、APIは [docs/api.md](../docs/api.md)（Botコマンド一覧・`/public/stamp-card/{token}`）、DB変更は [docs/database.md](../docs/database.md)（`players.stamp_card_token`）を参照。実装前に必ず目を通すこと。

部屋はランダム割当のため、訪問順にマスを埋めていく（1番目に訪れた部屋の名前が1マス目、2番目が2マス目、…）。未訪問のマスはどの部屋が入るか分からないため、枠のみで名前は表示しない。

## 実装対象ファイル

- `Cargo.toml` — `imageproc`・`ab_glyph`を追加（`cargo add imageproc ab_glyph`で追加し、既存の`image = "0.25.10"`と依存が衝突しないことを`cargo build`で確認する）
- `assets/fonts/NotoSansJP-Bold.ttf`（新規、バイナリ） — スタンプカードの文字描画に使うフォント本体
- `assets/fonts/OFL.txt`（新規） — 上記フォントのライセンス全文（SIL Open Font License 1.1。フォント再配布時の同梱が必須のため）
- `migrations/0003_players_stamp_card_token.sql`（新規） — `players.stamp_card_token`カラムとユニーク制約の追加
- `src/repository/player_repository.rs` — `Player`に`stamp_card_token`を追加、`insert`のシグネチャ変更、`find_by_stamp_card_token`を新設
- `src/repository/room_repository.rs` — `find_visited_room_names_ordered`を新設
- `src/services/stamp_card_service.rs`（新規） — スタンプカードPNGを生成する純粋関数
- `src/handlers/image.rs` — `GET /public/stamp-card/{token}` ハンドラーを追加（DB呼び出しは`game_service::with_db_call_timeout`でラップする、後述）
- `src/main.rs` — 上記ルートを登録
- `src/services/game_service.rs` — `ReplyMessage`に`stamp_card_url`（`Quest`）、新規`StampStatus`バリアントを追加。「スタンプ状況」コマンドの分岐、`help_text()`の文言更新。`player_repository::insert`の呼び出し元を更新
- `src/services/line_client.rs` — `build_stamp_status_image_message`を追加。`to_line_message`を`to_line_messages`に改名し`Vec<Value>`を返すよう変更。`send_reply`・`push_message`の引数を`Vec<Value>`に変更
- `src/handlers/line_webhook.rs` / `src/handlers/liff.rs` — 上記のリネーム・シグネチャ変更に呼び出し側を追従させる
- `Dockerfile` — ビルドステージに`COPY assets ./assets`を追加
- 既存テスト（`src/services/ranking_service.rs`・`src/handlers/liff.rs`・`src/main.rs`）内の`player_repository::insert(...)`呼び出し箇所（下記「呼び出し元一覧」参照）— 新しい引数に追従させる

### フォントの入手について

`assets/fonts/NotoSansJP-Bold.ttf`は、Google FontsのNoto Sans JP（SIL OFL 1.1、静的Boldウェイト単体ファイル）を取得して配置する。Google Fontsの公式配布元（fonts.google.com のダウンロード機能、または `google/fonts` のGitHubリポジトリ内 `ofl/notosansjp/` 配下）から取得できる。実装環境がインターネットに到達できずフォントを取得できない場合、この指示書は完了できないため、着手前に取得可否を確認し、取得できなければ運用者に相談すること。

## テストケース（TDDの起点）

### `src/repository/player_repository.rs`

- [ ] ケース1: `insert(pool, line_user_id, event_id, player_name, "token-abc")`で作成したプレイヤーを`find_by_line_user_and_event`で取得すると、`stamp_card_token == "token-abc"`である
- [ ] ケース2: `find_by_stamp_card_token(pool, "token-abc")`が該当プレイヤーを返す。存在しないトークンでは`None`を返す

### `src/repository/room_repository.rs`

- [ ] ケース3: `find_visited_room_names_ordered(pool, player_id)`が、訪問順（`visited_rooms.visited_at`昇順）に部屋名を返す（例: 2部屋目→1部屋目の順でチェックインした場合、返り値は`[2部屋目の部屋名, 1部屋目の部屋名]`の順）
- [ ] ケース4: 訪問記録が無いプレイヤーに対しては空配列を返す

### `src/services/stamp_card_service.rs`（新規ファイル）

- [ ] ケース5: `render_png(&[], 15)` が有効なPNG（`image::guess_format`で`ImageFormat::Png`）を返し、寸法が幅520px・高さ540px（3列×5行、余白20px、1マス幅160px・高さ100px）であること
- [ ] ケース6: `render_png(&["図書室".to_string()], 15)` の1マス目、矩形の左上隅から4pxずつ内側に入ったピクセル座標`(32, 32)`（文字の描画位置とは重ならない、塗りつぶし部分の隅）が、スタンプ色`[0xB5, 0x4B, 0x3A, 255]`であること
- [ ] ケース7: `render_png(&[], 15)` の1マス目の中心ピクセル（矩形の中心、計算式は実装仕様参照）が背景色`[255, 255, 255, 255]`のままであること（未スタンプは塗りつぶさない）
- [ ] ケース8: `render_png(&["A".to_string(), "B".to_string(), "C".to_string()], 5)`（5マス中3マスがスタンプ済み）で、3マス目（インデックス2）はスタンプ色が確認でき、4マス目（インデックス3、未訪問）の中心ピクセルは背景色のままであること（訪問順に先頭から埋まっていることの確認）
- [ ] ケース9: `render_png(&[], 0)` のように`total_rooms`が0以下でもpanicしないこと（`total_rooms`は1として扱われ、1マス分の画像が返る）
- [ ] ケース10: 部屋名の切り詰めを行う内部関数（`truncate_room_name`のような名前を想定。文字数ベースの純粋関数として実装し、`#[cfg(test)]`のユニットテストで直接検証する）について、6文字以下の部屋名はそのまま返し、7文字以上の部屋名は先頭5文字+「…」（計6文字）に切り詰められることを確認する（例: `"とても長い部屋の名前です"` → `"とても長い部…"`）

画像内の文字が実際に「その部屋名」として正しく描画されているかをピクセル単位で検証することはしない（フォントレンダリングの実装詳細に依存し、テストが脆くなるため）。テストで検証するのは「スタンプ済みマスが塗りつぶされているか」「未スタンプマスが塗りつぶされていないか」「訪問順に先頭から埋まるか」「切り詰めロジックが文字数として正しいか」の4点で十分。

### `src/handlers/image.rs`（`GET /public/stamp-card/{token}`）

- [ ] ケース11: 有効な`stamp_card_token`を持つプレイヤーが1部屋訪問済みの状態で`GET /public/stamp-card/{token}`にアクセスすると、200・`Content-Type: image/png`が返る
- [ ] ケース12: 存在しない`token`で`GET /public/stamp-card/{token}`にアクセスすると404が返る

### `src/services/game_service.rs`

- [ ] ケース13: 参加登録直後（名前入力後）の最初のクエスト案内（`ReplyMessage::Quest`）の`stamp_card_url`が、そのとき発行された`players.stamp_card_token`を含む形式（`{public_base_url}/public/stamp-card/{token}`）になっている
- [ ] ケース14: QRチェックイン成功後、次のクエスト案内の`stamp_card_url`が、登録時と同じ`stamp_card_token`を使い続けている（プレイヤーごとに固定であることの確認）
- [ ] ケース15: 「開始」再送信時（登録済み・未クリア）の案内も、同じ`stamp_card_token`を使う
- [ ] ケース16: 登録済み・未クリアのプレイヤーが「スタンプ状況」を送信すると、`ReplyMessage::StampStatus { image_url }`が返り、`image_url`がそのプレイヤーの`stamp_card_token`を含む
- [ ] ケース17: 未登録のプレイヤーが「スタンプ状況」を送信すると、既存の未登録案内（「『開始』と送信して参加登録してください。」）が返る（新規コマンドとして特別扱いしない）
- [ ] ケース18: クリア済みのプレイヤーが「スタンプ状況」を送信すると、既存の「クリア済みです。最初の部屋に戻ってください。」が返る（`finished_at`チェックが先に評価されるため、スタンプ状況の分岐には到達しない。10節参照）
- [ ] ケース19（回帰・文言確認）: `help_text()`（「遊び方」「ヘルプ」の返信）に「スタンプ状況」コマンドの説明が含まれる

### `src/services/line_client.rs`

- [ ] ケース20: `build_stamp_status_image_message(url)` が `{"type": "image", "originalContentUrl": url, "previewImageUrl": url}` を返す
- [ ] ケース21: `to_line_messages`に`ReplyMessage::Quest`を渡すと、2件のメッセージ（1件目がクエストFlex Message、2件目が`stamp_card_url`を使った画像メッセージ）を返す
- [ ] ケース22: `to_line_messages`に`ReplyMessage::StampStatus`を渡すと、1件の画像メッセージ（`image_url`を使用）を返す
- [ ] ケース23（回帰）: `to_line_messages`に`ReplyMessage::Text`・`ReplyMessage::Cleared`を渡した場合、それぞれ従来通り1件のメッセージを返す（既存の`to_line_message_builds_text_messages`・`to_line_message_builds_cleared_flex_message`相当のテストを、`Vec`の1件目を見る形に書き換える）

既存テストのうち、`ReplyMessage::Quest`のフィールド追加・`to_line_message`のリネーム・`player_repository::insert`のシグネチャ変更によってコンパイルが通らなくなる箇所は、既存の検証内容を変えずに機械的に追従させること（下記「呼び出し元一覧」参照）。これらはTDDの新規ケースではないが、上記ケースを書く過程で合わせて直すことになる。

## 実装仕様

### `migrations/0003_players_stamp_card_token.sql`

```sql
ALTER TABLE players
    ADD COLUMN stamp_card_token VARCHAR(36) NULL,
    ADD UNIQUE KEY uq_players_stamp_card_token (stamp_card_token);
```

ローカル開発DBに既存のテストデータ行がある状態でこのマイグレーションを流しても、NOT NULL制約は付けていないため失敗しない。アプリケーション側（`player_repository::insert`）は常に値を渡すため、以降に登録されるプレイヤーは実質的に必ず値を持つ。

### `src/repository/player_repository.rs`

- `Player`構造体に`pub stamp_card_token: String,`を追加する
- `player_from_row`に`stamp_card_token: row.try_get("stamp_card_token")?,`を追加する
- `find_by_line_user_and_event`・`find_all_by_event`のSELECT文の列リストに`stamp_card_token`を追加する
- `insert`のシグネチャに`stamp_card_token: &str`を追加し、INSERT文・バインドにも追加する:
  ```rust
  pub async fn insert(
      pool: &MySqlPool,
      line_user_id: &str,
      event_id: i32,
      player_name: &str,
      stamp_card_token: &str,
  ) -> Result<i32, sqlx::Error> {
      let result = sqlx::query(
          r#"
          INSERT INTO players (line_user_id, event_id, player_name, current_room_id, answer_verified, started_at, finished_at, stamp_card_token)
          VALUES (?, ?, ?, NULL, FALSE, NOW(), NULL, ?)
          "#,
      )
      .bind(line_user_id)
      .bind(event_id)
      .bind(player_name)
      .bind(stamp_card_token)
      .execute(pool)
      .await?;

      Ok(result.last_insert_id() as i32)
  }
  ```
- 新規関数`find_by_stamp_card_token`を、`find_by_line_user_and_event`と同じ形で追加する（`WHERE stamp_card_token = ?`）

#### `insert`呼び出し元一覧（すべて更新が必要）

- `src/services/game_service.rs:178`付近（本番コード。次項参照）
- `src/services/game_service.rs`内のテスト2箇所（`crate::repository::player_repository::insert(pool, line_user_id, event_id, "Alice")`のパターン、およびもう1箇所の複数行呼び出し）
- `src/services/ranking_service.rs`内のテスト2箇所
- `src/handlers/liff.rs`内のテスト1箇所
- `src/main.rs`内の`seed_ranking_player`ヘルパー1箇所

テストコードでは、固定の文字列リテラル（例: `"test-stamp-token"`、テストごとに衝突しなければ同じ値の使い回しでよい。`stamp_card_token`のユニーク制約はテストの検証観点ではないため、テストDBごとに毎回作り直される前提で衝突を気にする必要はない）を渡せばよい。

### `src/repository/room_repository.rs`

```rust
pub async fn find_visited_room_names_ordered(
    pool: &MySqlPool,
    player_id: i32,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT rooms.room_name AS room_name
        FROM visited_rooms
        JOIN rooms ON rooms.id = visited_rooms.room_id
        WHERE visited_rooms.player_id = ?
        ORDER BY visited_rooms.visited_at ASC
        "#,
    )
    .bind(player_id)
    .fetch_all(pool)
    .await?;

    rows.iter().map(|row| row.try_get("room_name")).collect()
}
```

### `src/services/stamp_card_service.rs`（新規）

以下は参考実装。テストケース5〜9の座標・色の期待値はこの実装から導出したものなので、実装を変える場合はテストの期待値も整合させること。

```rust
use std::io::Cursor;
use std::sync::LazyLock;

use ab_glyph::{FontRef, PxScale};
use image::{ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Bold.ttf");

// フォントのパース（TrueType/OpenTypeテーブルの読み込み）はμs〜ms オーダーとはいえ無視できないCPUコストで、
// このハンドラーはLINEから部屋案内・「スタンプ状況」コマンドのたびに繰り返し叩かれる。
// リクエストごとに再パースせず、プロセス内で1回だけ行い使い回す。
static FONT: LazyLock<FontRef<'static>> =
    LazyLock::new(|| FontRef::try_from_slice(FONT_BYTES).expect("bundled font must be valid"));

const COLUMNS: i32 = 3;
const CELL_WIDTH: i32 = 160;
const CELL_HEIGHT: i32 = 100;
const PADDING: i32 = 20;
const CELL_MARGIN: i32 = 8;
const MAX_NAME_CHARS: usize = 6;

const BACKGROUND: Rgba<u8> = Rgba([255, 255, 255, 255]);
const STAMPED_FILL: Rgba<u8> = Rgba([0xB5, 0x4B, 0x3A, 255]);
const STAMPED_TEXT: Rgba<u8> = Rgba([255, 255, 255, 255]);
const EMPTY_BORDER: Rgba<u8> = Rgba([0xE2, 0xE4, 0xE9, 255]);

pub fn render_png(room_names: &[String], total_rooms: i64) -> Vec<u8> {
    let total_rooms = total_rooms.max(1) as i32;
    let rows = total_rooms.div_ceil(COLUMNS);
    let width = (COLUMNS * CELL_WIDTH + PADDING * 2) as u32;
    let height = (rows * CELL_HEIGHT + PADDING * 2) as u32;

    let mut image: RgbaImage = ImageBuffer::from_pixel(width, height, BACKGROUND);
    let scale = PxScale::from(24.0);

    for i in 0..total_rooms {
        let col = i % COLUMNS;
        let row = i / COLUMNS;
        let x = PADDING + col * CELL_WIDTH + CELL_MARGIN;
        let y = PADDING + row * CELL_HEIGHT + CELL_MARGIN;
        let rect_width = (CELL_WIDTH - CELL_MARGIN * 2) as u32;
        let rect_height = (CELL_HEIGHT - CELL_MARGIN * 2) as u32;
        let rect = Rect::at(x, y).of_size(rect_width, rect_height);

        if let Some(name) = room_names.get(i as usize) {
            draw_filled_rect_mut(&mut image, rect, STAMPED_FILL);
            let label = truncate_room_name(name);
            draw_text_mut(
                &mut image,
                STAMPED_TEXT,
                x + 10,
                y + rect_height as i32 / 2 - 12,
                scale,
                &*FONT,
                &label,
            );
        } else {
            draw_hollow_rect_mut(&mut image, rect, EMPTY_BORDER);
        }
    }

    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("PNG encoding should not fail");
    output.into_inner()
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
```

座標の補足（テストケース6・7・8で使う値）:

- 1マス目（インデックス0、`col=0, row=0`）: `x = 20 + 0 + 8 = 28`, `y = 28`。左上隅から4px内側 = `(32, 32)`。矩形の幅は `CELL_WIDTH(160) - CELL_MARGIN*2(16) = 144`、高さは `CELL_HEIGHT(100) - 16 = 84`。中心 = `(28 + 144/2, 28 + 84/2) = (100, 70)`
- 4マス目（インデックス3、`col=0, row=1`、`total_rooms=5`のとき）: `x = 20 + 0 + 8 = 28`, `y = 20 + 100 + 8 = 128`。中心 = `(100, 170)`

`Rect::at(x, y).of_size(w, h)`・`draw_filled_rect_mut`・`draw_hollow_rect_mut`・`draw_text_mut`は`imageproc::drawing`・`imageproc::rect`の標準API。バージョンはCargoが解決したものをそのまま使ってよい（`image 0.25`系と互換性のあるバージョンが解決されるはずだが、`cargo build`でエラーが出た場合はバージョンを調整すること）。

### `src/handlers/image.rs`

`serve`関数の下に追加する。DBアクセスが必要になるため`State<MySqlPool>`を受け取る（従来の`serve`と同じ形）。

**この画像は部屋案内・「スタンプ状況」コマンドのたびにLINE側から取得される、未認証の公開エンドポイントである。`docs/architecture.md` 21節で確立した「DBを伴う処理は必ずタイムアウトで保護する」方針（本番でDBコネクションが無応答のまま滞留し、コネクションプールが枯渇して全参加者の処理が止まった実際の障害を踏まえた方針）をここにも適用する。** 3回のDB呼び出し（トークンからプレイヤー検索・訪問済み部屋名取得・部屋数取得）を1つの`async`ブロックにまとめ、`game_service::with_db_call_timeout`（`pub(crate)`で公開済み、`docs/architecture.md` 21節参照）でラップすること。

```rust
pub async fn stamp_card(
    State(pool): State<MySqlPool>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    use crate::repository::{player_repository, room_repository};
    use crate::services::game_service::{self, GameServiceError};

    let result: Result<Option<(Vec<String>, i64)>, GameServiceError> =
        game_service::with_db_call_timeout(async {
            let Some(player) = player_repository::find_by_stamp_card_token(&pool, &token).await?
            else {
                return Ok(None);
            };
            let room_names =
                room_repository::find_visited_room_names_ordered(&pool, player.id).await?;
            let total_rooms = room_repository::count(&pool, player.event_id).await?;
            Ok(Some((room_names, total_rooms)))
        })
        .await;

    let (room_names, total_rooms) = match result {
        Ok(Some(data)) => data,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(?err, "failed to load stamp card data");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let png = crate::services::stamp_card_service::render_png(&room_names, total_rooms);

    (
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CACHE_CONTROL, "private, max-age=60".to_string()),
        ],
        png,
    )
        .into_response()
}
```

`Cache-Control: private, max-age=60`を付けているのは必須ではないが、同一プレイヤーが「スタンプ状況」を連打した場合などに毎回DB＋描画をやり直さずに済むようにするための軽い最適化（`private`はプレイヤー固有の内容のため共有キャッシュに乗せない指定）。`GameServiceError`は`sqlx::Error`から`From`実装済みなので、`async`ブロック内の`?`でそのまま変換される。

### `src/main.rs`

- `/public/image/{uuid}` の直後に以下を追加する:
  ```rust
  .route("/public/stamp-card/{token}", get(handlers::image::stamp_card))
  ```
- `src/services/mod.rs`に`pub mod stamp_card_service;`を追加する

### `src/services/game_service.rs`

- `ReplyMessage`を以下のように変更する:
  ```rust
  pub enum ReplyMessage {
      Text(String),
      Quest {
          intro: String,
          room_name: String,
          quest_text: String,
          image_url: Option<String>,
          stamp_card_url: String,
      },
      StampStatus {
          image_url: String,
      },
      Cleared {
          elapsed: String,
      },
  }
  ```
- 新規のprivateヘルパーを追加する:
  ```rust
  fn stamp_card_url(public_base_url: &str, token: &str) -> String {
      format!(
          "{}/public/stamp-card/{token}",
          public_base_url.trim_end_matches('/')
      )
  }
  ```
- `quest_reply_for_room`のシグネチャに`stamp_card_token: &str`を追加し、返り値の`ReplyMessage::Quest`に`stamp_card_url(public_base_url, stamp_card_token)`を含める
- 呼び出し元3箇所を更新する:
  1. 名前入力後の初回登録処理: `Uuid::new_v4().to_string()`で`stamp_card_token`を発行し（`use uuid::Uuid;`を追加。`src/services/room_service.rs`が`qr_uuid`を発行しているのと同じパターン）、`player_repository::insert`に渡す。同じ変数を`quest_reply_for_room(..., "最初の部屋は", &stamp_card_token)`にも渡す
  2. `quest_reply_for_player`（「開始」再送信時）: `quest_reply_for_room(..., "現在向かっている部屋は", &player.stamp_card_token)`
  3. `checkin`関数内、次の部屋を案内する箇所: `quest_reply_for_room(..., intro, &player.stamp_card_token)`
- `handle_text_message`に「スタンプ状況」の分岐を追加する。位置は`ヒント`の分岐の直後（`player.finished_at`チェックより後、つまり登録済み・未クリアのプレイヤーのみ到達する）:
  ```rust
  if text == "スタンプ状況" {
      return Ok(ReplyMessage::StampStatus {
          image_url: stamp_card_url(public_base_url, &player.stamp_card_token),
      });
  }
  ```
- `help_text()`の文言に「スタンプ状況」コマンドの説明を追加する（例: `"『開始』で参加登録します。案内された部屋へ移動し、必要に応じて答えを送信してからQRコードを読み込んでください。『ヒント』でヒント、『スタンプ状況』で現在集めたスタンプを確認、『リセット』で参加データを削除できます。"`。文言の細部はテストケース19の検証内容（「スタンプ状況」という文字列が含まれること）を満たせば厳密に一致させる必要はない）

### `src/services/line_client.rs`

- `build_stamp_status_image_message`を追加する:
  ```rust
  pub fn build_stamp_status_image_message(url: &str) -> Value {
      json!({"type": "image", "originalContentUrl": url, "previewImageUrl": url})
  }
  ```
- `to_line_message`を`to_line_messages`に改名し、`Vec<Value>`を返すようにする:
  ```rust
  pub fn to_line_messages(reply: &ReplyMessage, liff_id: &str) -> Vec<Value> {
      match reply {
          ReplyMessage::Text(text) => vec![build_text_message(text)],
          ReplyMessage::Quest {
              intro,
              room_name,
              quest_text,
              image_url,
              stamp_card_url,
          } => vec![
              build_quest_flex_message(intro, room_name, quest_text, image_url.as_deref(), liff_id),
              build_stamp_status_image_message(stamp_card_url),
          ],
          ReplyMessage::StampStatus { image_url } => {
              vec![build_stamp_status_image_message(image_url)]
          }
          ReplyMessage::Cleared { elapsed } => vec![build_cleared_flex_message(elapsed)],
      }
  }
  ```
- `send_reply`・`push_message`の`message: Value`パラメータを`messages: Vec<Value>`に変更し、`json!({"replyToken": reply_token, "messages": messages})` / `json!({"to": to, "messages": messages})`とする（呼び出し側から渡された`Vec`をそのまま使い、`[message]`のようなラップをしない）

### `src/handlers/line_webhook.rs` / `src/handlers/liff.rs`

- `line_client::to_line_message(&reply, &state.liff_id)` の呼び出しをすべて `line_client::to_line_messages(&reply, &state.liff_id)` に変更し、変数名も`message`から`messages`に変える
- `line_client::send_reply(..., message)` / `line_client::push_message(..., message)` の最後の引数を`messages`に変更する（`Vec<Value>`をそのまま渡す）

### `Dockerfile`

ビルドステージの`COPY templates ./templates`の下に以下を追加する:

```dockerfile
COPY assets ./assets
```

（実行ステージには追加しない。フォントはコンパイル時に`include_bytes!`でバイナリへ埋め込まれるため、実行イメージに`assets/`は不要）

## 制約・注意事項

- 新規crateは`imageproc`・`ab_glyph`のみに限定する（他の画像・フォント関連crateは追加しない）
- `CheckinOutcome::Cleared`（全部屋クリア時）のメッセージにはスタンプカードを追加しない。対象は`ReplyMessage::Quest`と新規`ReplyMessage::StampStatus`のみ
- 「スタンプ状況」コマンドは、既存の優先順位（`docs/architecture.md` 10節）を変更しない範囲に追加する。具体的には、未登録・クリア済みのプレイヤーに対する既存の案内文言が変わらないこと（ケース17・18で確認）
- `GET /public/stamp-card/{token}`は`players.stamp_card_token`に一致する行が無ければ404を返す。`total`や`visited`のような数値をパスパラメータで受け取る設計ではないため、数値の範囲検証は不要（トークンがDBに存在するかどうかだけで判定すれば十分）
- `GET /public/stamp-card/{token}`のDB呼び出しは必ず`game_service::with_db_call_timeout`でラップすること（実装仕様参照）。省略しないこと。このエンドポイントは`/callback`・`/liff/checkin`と同様、未認証かつ高頻度に叩かれる経路であり、DBコネクションが無応答のまま滞留した場合の影響（プール枯渇による全参加者への波及）が`docs/architecture.md` 21節で説明されている障害と同じ構造で起こりうる
- フォント（`FontRef::try_from_slice`）はリクエストごとに再パースしない。参考実装の`static FONT: LazyLock<...>`のように、プロセス内で1回だけパースして使い回すこと
- 画像内の文字が「その部屋名」として正しいかをピクセル単位で検証するテストは書かない（実装仕様の「テストケース5〜9」の注記を参照）。部屋名の切り詰めロジックのみ、文字列操作として直接テストする
- `send_reply`・`push_message`はネットワーク呼び出しを伴うため、これらの関数自体の自動テストは追加しない（既存方針、`docs/architecture.md` 13節）。テスト対象はメッセージを組み立てる純粋関数（`to_line_messages`・`build_stamp_status_image_message`）のみ
- フォントファイル（`assets/fonts/NotoSansJP-Bold.ttf`）と同じディレクトリに、SIL Open Font License 1.1の全文（`OFL.txt`）を同梱すること（フォント再配布のライセンス条件）
- 既存の部屋QR（`qr_service`）・LIFFチェックイン・ランキング等、本指示書のスコープ外の機能・テンプレートは変更しないこと

## 完了条件

- [ ] 上記23テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] `docker build .`（本番用Dockerfile）がローカルで成功する（`assets`同梱漏れがないことの確認）
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
