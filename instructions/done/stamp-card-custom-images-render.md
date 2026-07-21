# 実装指示書: スタンプ・スタンプカードのカスタム画像設定（PR B: render_pngへの反映）

## 背景・目的

PR A（`instructions/done/stamp-card-custom-images-db.md`）で、部屋ごとの「スタンプ表示名」（必須・4文字まで）・「スタンプ画像」（任意）、イベント全体の「スタンプカード台紙画像」（任意）をDB・管理画面に追加した。今回のPR Bでは、これらを実際にスタンプカード画像（`stamp_card_service::render_png`）へ反映する。

基本設計は [docs/architecture.md 23節「追記: 部屋ごとのカスタムスタンプ画像・カード台紙画像への対応（PR B）」](../docs/architecture.md#追記-部屋ごとのカスタムスタンプ画像カード台紙画像への対応pr-b)を参照。DB設計（`rooms.stamp_label`・`stamp_image_id`、`events.stamp_card_background_image_id`）は[docs/database.md](../docs/database.md)を参照。

## 実装対象ファイル

- `src/services/stamp_card_service.rs` — `render_png`のシグネチャを変更し、部屋ごとのカスタムスタンプ画像・カード台紙画像を受け取れるようにする
- `src/repository/room_repository.rs` — `find_visited_room_names_ordered`を拡張し、`stamp_label`・`stamp_image_id`も返すようにする
- `src/repository/room_image_repository.rs` — 内部IDで画像データを取得する`find_by_id`を新設する
- `src/main.rs` — `AppState`に`stamp_image_cache`フィールドを追加する
- `src/handlers/image.rs` — `stamp_card`ハンドラーを、`stamp_label`のフォールバック解決・カスタム画像の取得（キャッシュ経由）・イベントの台紙画像取得に対応させる。`State<MySqlPool>`から`State<AppState>`に変更する

他のファイル（`game_service`・`line_client`・管理画面側）は変更しない。

## テストケース（TDDの起点）

### `src/repository/room_repository.rs`

- [ ] ケース1: `find_visited_room_names_ordered`が返す各要素に、部屋名に加えて`stamp_label`（`Option<String>`）・`stamp_image_id`（`Option<i32>`）が含まれ、訪問順（既存仕様）も維持されていること

### `src/repository/room_image_repository.rs`

- [ ] ケース2: `insert`で保存した画像を`find_by_id`（内部ID指定）で取得できる
- [ ] ケース3: 存在しないIDで`find_by_id`を呼ぶと`None`が返る

### `src/services/stamp_card_service.rs`

既存のリング・背景・飾り枠のテスト（座標ベース）は、`StampCell { label, custom_image: None }`の配列を渡す形に書き換えれば、期待値を変えずにそのまま通るはずである（ラベルのみのケースは今まで通りはんこ風自動生成になるため）。以下は新規に追加するテスト。

- [ ] ケース4: `custom_image`に画像を指定した`StampCell`を含めて`render_png`を呼ぶと、そのマスの中心ピクセルが（はんこの色ではなく）指定した画像の色になっている（例: 単色で塗った84×84以上のテスト画像を用意し、マス中心の色がその色と一致することを確認する。はんこの二重リング色・輪郭色とは異なる色をテスト画像に使うことで、はんこ自動生成が描画されていないことも同時に確認できる）
- [ ] ケース5: `custom_image`が`None`のマスは、従来通りはんこ風自動生成（ケース2〜3相当の座標検証）になっていること（回帰確認）
- [ ] ケース6: `card_background`に画像を指定して`render_png`を呼ぶと、カード背景（飾り枠・タイトル・スタンプのいずれにも重ならない点）が指定した画像由来の色になっている（クリーム色の`CARD_BACKGROUND`ではないこと）
- [ ] ケース7: `card_background`が`None`のときは、従来通りクリーム色の背景のままであること（回帰確認）

### `src/handlers/image.rs`

- [ ] ケース8: `stamp_image_id`が設定された部屋を訪問済みのプレイヤーが`GET /public/stamp-card/{token}`にアクセスすると、200・`Content-Type: image/png`が返る（実際に画像が反映されているかはコード上のロジックで保証し、ハンドラーレベルのテストではエンドポイントが成功することの確認で十分。ピクセル単位の検証は`stamp_card_service`側のテストで行う）
- [ ] ケース9（回帰）: `stamp_image_id`・`stamp_card_background_image_id`のいずれも未設定のプレイヤー・イベントで`GET /public/stamp-card/{token}`にアクセスしても、引き続き200が返る

## 実装仕様

### `src/repository/room_image_repository.rs`

```rust
pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<(Vec<u8>, String)>, sqlx::Error> {
    let Some(row) = sqlx::query("SELECT data, mime_type FROM room_images WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    Ok(Some((row.try_get("data")?, row.try_get("mime_type")?)))
}
```

### `src/repository/room_repository.rs`

`find_visited_room_names_ordered`のSELECT文に`rooms.stamp_label, rooms.stamp_image_id`を追加し、返り値の型を`Vec<String>`から以下のような構造体の配列に変更する（関数名はそのままでよい）:

```rust
pub struct VisitedRoomStamp {
    pub room_name: String,
    pub stamp_label: Option<String>,
    pub stamp_image_id: Option<i32>,
}

pub async fn find_visited_room_names_ordered(
    pool: &MySqlPool,
    player_id: i32,
) -> Result<Vec<VisitedRoomStamp>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT rooms.room_name AS room_name, rooms.stamp_label AS stamp_label, rooms.stamp_image_id AS stamp_image_id
        FROM visited_rooms
        JOIN rooms ON rooms.id = visited_rooms.room_id
        WHERE visited_rooms.player_id = ?
        ORDER BY visited_rooms.visited_at ASC
        "#,
    )
    .bind(player_id)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(VisitedRoomStamp {
                room_name: row.try_get("room_name")?,
                stamp_label: row.try_get("stamp_label")?,
                stamp_image_id: row.try_get("stamp_image_id")?,
            })
        })
        .collect()
}
```

呼び出し元は`src/handlers/image.rs`の`stamp_card`のみなので、そちらを合わせて更新する。

### `src/services/stamp_card_service.rs`

```rust
use std::sync::Arc;
use image::DynamicImage;

pub struct StampCell {
    pub label: String,
    pub custom_image: Option<Arc<DynamicImage>>,
}

const CUSTOM_STAMP_RADIUS: i32 = 42; // 既存のOUTER_RING_OUTER_RADIUSと同じ大きさに揃える

pub fn render_png(
    stamps: &[StampCell],
    total_rooms: i64,
    card_background: Option<&DynamicImage>,
) -> Vec<u8> {
    let total_rooms = total_rooms.max(1) as i32;
    let rows = (total_rooms + COLUMNS - 1) / COLUMNS;
    let width = (COLUMNS * CELL_WIDTH + PADDING * 2) as u32;
    let height = (TITLE_AREA_HEIGHT + rows * CELL_HEIGHT + PADDING * 2) as u32;

    let mut image: RgbaImage = match card_background {
        Some(background) => background
            .resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
            .to_rgba8(),
        None => ImageBuffer::from_pixel(width, height, CARD_BACKGROUND),
    };

    draw_card_frame(&mut image, width, height);
    draw_card_title(&mut image, width);

    for i in 0..total_rooms {
        let col = i % COLUMNS;
        let row = i / COLUMNS;
        let center_x = PADDING + col * CELL_WIDTH + CELL_WIDTH / 2;
        let center_y = TITLE_AREA_HEIGHT + PADDING + row * CELL_HEIGHT + CELL_HEIGHT / 2;

        match stamps.get(i as usize) {
            Some(StampCell { custom_image: Some(custom), .. }) => {
                draw_custom_stamp(&mut image, center_x, center_y, custom);
            }
            Some(StampCell { label, .. }) => {
                draw_stamp(&mut image, center_x, center_y, label);
            }
            None => draw_empty_ring(&mut image, center_x, center_y),
        }
    }

    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("PNG encoding should not fail");
    output.into_inner()
}

fn draw_custom_stamp(image: &mut RgbaImage, center_x: i32, center_y: i32, custom: &DynamicImage) {
    let diameter = (CUSTOM_STAMP_RADIUS * 2) as u32;
    let mut cropped = custom
        .resize_to_fill(diameter, diameter, image::imageops::FilterType::Lanczos3)
        .to_rgba8();

    let center = CUSTOM_STAMP_RADIUS;
    for y in 0..diameter as i32 {
        for x in 0..diameter as i32 {
            let dx = x - center;
            let dy = y - center;
            if dx * dx + dy * dy > CUSTOM_STAMP_RADIUS * CUSTOM_STAMP_RADIUS {
                cropped.put_pixel(x as u32, y as u32, Rgba([0, 0, 0, 0]));
            }
        }
    }

    let offset_x = (center_x - CUSTOM_STAMP_RADIUS) as i64;
    let offset_y = (center_y - CUSTOM_STAMP_RADIUS) as i64;
    image::imageops::overlay(image, &cropped, offset_x, offset_y);
}
```

`draw_stamp`（既存のはんこ自動生成関数）・`draw_empty_ring`・`draw_card_frame`・`draw_card_title`・`truncate_room_name`・`split_stamp_label_lines`・`stamp_rotation_degrees`は変更しない。`render_png`の呼び出し元（`room_names: &[String]`を渡していた箇所）は、`StampCell { label, custom_image: None }`の配列に置き換える。

`resize_to_fill`は`image`crateの`DynamicImage`に既に存在するメソッドで、アスペクト比を保ったまま指定サイズを覆うようにリサイズ・中央クロップする（新規依存の追加は不要）。

### `src/handlers/image.rs`

```rust
pub async fn stamp_card(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    use crate::repository::{event_repository, player_repository, room_image_repository, room_repository};
    use crate::services::game_service::{self, GameServiceError};
    use crate::services::stamp_card_service::StampCell;

    let pool = &state.pool;

    let result: Result<Option<(Vec<room_repository::VisitedRoomStamp>, i64, Option<i32>)>, GameServiceError> =
        game_service::with_db_call_timeout(async {
            let Some(player) = player_repository::find_by_stamp_card_token(pool, &token).await?
            else {
                return Ok(None);
            };
            let visited = room_repository::find_visited_room_names_ordered(pool, player.id).await?;
            let total_rooms = room_repository::count(pool, player.event_id).await?;
            let event = event_repository::find_singleton(pool).await?;
            let background_image_id = event.and_then(|event| event.stamp_card_background_image_id);
            Ok(Some((visited, total_rooms, background_image_id)))
        })
        .await;

    let (visited, total_rooms, background_image_id) = match result {
        Ok(Some(data)) => data,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(?err, "failed to load stamp card data");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut stamps = Vec::with_capacity(visited.len());
    for room in visited {
        let custom_image = match room.stamp_image_id {
            Some(image_id) => match load_cached_image(pool, &state.stamp_image_cache, image_id).await {
                Ok(image) => image,
                Err(err) => {
                    tracing::error!(?err, "failed to load stamp image");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            },
            None => None,
        };
        let label = room.stamp_label.unwrap_or(room.room_name);
        stamps.push(StampCell { label, custom_image });
    }

    let card_background = match background_image_id {
        Some(image_id) => match load_cached_image(pool, &state.stamp_image_cache, image_id).await {
            Ok(image) => image,
            Err(err) => {
                tracing::error!(?err, "failed to load stamp card background image");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        },
        None => None,
    };

    let png = crate::services::stamp_card_service::render_png(
        &stamps,
        total_rooms,
        card_background.as_deref(),
    );

    (
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CACHE_CONTROL, "private, max-age=60".to_string()),
        ],
        png,
    )
        .into_response()
}

async fn load_cached_image(
    pool: &MySqlPool,
    cache: &StampImageCache,
    image_id: i32,
) -> Result<Option<Arc<image::DynamicImage>>, sqlx::Error> {
    if let Some(cached) = cache.read().expect("cache lock poisoned").get(&image_id) {
        return Ok(Some(cached.clone()));
    }

    let Some((data, _mime_type)) =
        crate::repository::room_image_repository::find_by_id(pool, image_id).await?
    else {
        return Ok(None);
    };
    let decoded = Arc::new(image::load_from_memory(&data).expect("stored image should decode"));
    cache
        .write()
        .expect("cache lock poisoned")
        .insert(image_id, decoded.clone());
    Ok(Some(decoded))
}
```

ロックは`.read()`/`.write()`取得後すぐに値を取り出し（`.get(...).cloned()`）、ガードを保持したまま`.await`をまたがないこと（`load_cached_image`内のDBアクセス`.await`の時点では読み取りロックは既に解放されている必要がある）。上記コード例ではロックのスコープが`if let`文の中で完結しているため問題ない。

### `src/main.rs`

- `pub type StampImageCache = Arc<std::sync::RwLock<std::collections::HashMap<i32, Arc<image::DynamicImage>>>>;`を定義する
- `AppState`に`pub stamp_image_cache: StampImageCache,`を追加する
- `AppState::new`で`stamp_image_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),`を初期化する（引数には追加しない。呼び出し元を変更せずに済む）
- `/public/stamp-card/{token}`ルートの`handlers::image::stamp_card`は、ハンドラー側のシグネチャ変更（`State<MySqlPool>`→`State<AppState>`）に伴い、ルーティング定義自体は変更不要（`AppState`を全体で共有しているため）

## 制約・注意事項

- `render_png`のテキスト描画ロジック（`truncate_room_name`・`split_stamp_label_lines`・`stamp_rotation_degrees`・回転演出・二重リング）は変更しない。カスタム画像がある場合はそれらを完全にバイパスして円形クロップ画像を合成するだけにする
- カスタム画像には回転演出を適用しない（`draw_stamp`のような`rotate_about_center`は使わない）
- 新規依存クレートの追加は不要（`resize_to_fill`は既存の`image`crateに含まれる）
- 画像デコードのキャッシュは`room_images.id`をキーにする。画像を張り替えた場合は新しいIDになるため、明示的なキャッシュ無効化・TTL・上限設定は不要
- キャッシュのロックは同期ロック（`std::sync::RwLock`）でよい。非同期ロック（`tokio::sync::RwLock`）は導入しない
- `GET /public/stamp-card/{token}`のDBアクセス（プレイヤー検索・訪問済み部屋取得・部屋数取得・イベント取得の4クエリ）は、引き続き`game_service::with_db_call_timeout`でラップすること（画像のキャッシュ取得自体はタイムアウトの外でよい。キャッシュヒット時はDBアクセスが発生しないため）
- 既存の`docs/architecture.md`・`docs/database.md`の設計との整合性を保つこと

## 完了条件

- [ ] 上記9テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] 管理画面でスタンプ画像・台紙画像を実際にアップロードし、LINE Bot側（またはローカルで`/public/stamp-card/{token}`に直接アクセス）でスタンプカード画像に反映されることを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
