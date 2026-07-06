# 実装指示書: 部屋（チェックポイント）管理

## 背景・目的

管理者認証機能（`feature/admin-auth`、マージ済み）により `/admin/*` はセッション認証で保護される土台ができた。次は管理者機能の中核である部屋（チェックポイント）のCRUD・画像アップロード・QRコード表示を実装する。

[architecture.md](../docs/architecture.md) の「QRコードの仕組み」「画像配信の仕組み」「部屋（チェックポイント）管理の実装方針」で決定した方針に基づき、以下を実装する。

- 部屋の一覧・新規登録・編集・削除（最大15部屋）
- 画像アップロード（マジックバイト検証・サイズ上限・リサイズ）
- 部屋ごとのQRコード画像のその場生成・表示

この指示書のスコープは `/admin/rooms*` 系のCRUDとQR表示まで。イベント設定画面（個人戦/チーム戦・判定モードの切替、`/admin/settings`）、ランキング画面、画像の公開配信エンドポイント（`/public/image/{uuid}`）は対象外（後続の指示書で追加する）。

判定モード（`events.require_answer_check`）を切り替えるUIはまだ存在しないため、テストでは `events` 行を直接更新して両モードを再現すること。

---

## 実装対象ファイル

- `src/main.rs` — `/admin/rooms*` ルーターの登録（`admin_router` に追加）
- `src/handlers/mod.rs` — `rooms` モジュールの公開
- `src/handlers/rooms.rs`（新規） — 部屋一覧・登録・編集・更新・削除・QR表示ハンドラー
- `src/services/mod.rs` — `room_service` `image_service` `qr_service` の公開
- `src/services/room_service.rs`（新規） — 部屋CRUDの業務ロジック（最大15件・判定モード別バリデーション・画像の張り替え）
- `src/services/image_service.rs`（新規） — アップロード画像のマジックバイト検証・サイズ上限チェック・リサイズ
- `src/services/qr_service.rs`（新規） — `qr_uuid` からQRコードPNG画像を生成
- `src/repository/mod.rs` — `room_repository` `room_image_repository` の公開
- `src/repository/room_repository.rs`（新規） — `rooms` テーブルへのアクセス
- `src/repository/room_image_repository.rs`（新規） — `room_images` テーブルへのアクセス
- `templates/admin/_base.html`（新規） — 管理画面共通レイアウト（Bootstrap 5）
- `templates/admin/rooms/list.html`（新規） — 部屋一覧
- `templates/admin/rooms/add.html`（新規） — 新規登録フォーム
- `templates/admin/rooms/edit.html`（新規） — 編集フォーム

---

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、以下の順にRed-Green-Refactorを回す。DBに依存するテストは `sqlx::test` を使うこと。

### image_service（DB非依存）

- [ ] ケース1: 5MBを超えるバイト列を渡すと、サイズ超過のエラーを返す
- [ ] ケース2: JPEG・PNGどちらでもないバイト列（マジックバイト不一致）を渡すと、フォーマット不正のエラーを返す
- [ ] ケース3: 有効なJPEG/PNGを渡すと、幅800px以下・JPEGにリサイズされたバイト列を返す

### qr_service（DB非依存）

- [ ] ケース4: 任意の文字列を渡すと、PNGとしてデコード可能な画像バイト列が返り、QRコードとしてデコードすると元の文字列に一致する

### room_repository / room_image_repository（`sqlx::test`）

- [ ] ケース5: 部屋を登録し、`find_by_id` で取得した内容が一致する
- [ ] ケース6: `count` が登録済み件数を正しく返す
- [ ] ケース7: 部屋を更新すると内容が反映される
- [ ] ケース8: 部屋を削除すると `find_by_id` がNoneを返す

### room_service（`sqlx::test`）

- [ ] ケース9: 既に15件登録されている状態で新規登録すると、上限エラーになり登録されない
- [ ] ケース10: `require_answer_check = true` のイベントで `answer` が空の新規登録は、エラーになり登録されない
- [ ] ケース11: `require_answer_check = false` のイベントで `answer` / `hint_msg` を含めて新規登録しても、保存される値は常にNULLになる
- [ ] ケース12: 画像付きで新規登録すると `room_images` に1行作成され、`rooms.image_id` がそれを指す
- [ ] ケース13: 既に画像を持つ部屋を新しい画像で更新すると、古い `room_images` 行が削除され、新しい行に張り替わる
- [ ] ケース14: 部屋を削除すると、紐づく `room_images` 行も削除される

### ハンドラー（`sqlx::test` / 結合テスト）

- [ ] ケース15: 未ログイン状態で `GET /admin/rooms` にアクセスすると302で `/auth/login` へリダイレクトされる
- [ ] ケース16: ログイン済みで `GET /admin/rooms` にアクセスすると200で、登録済みの部屋名が一覧に表示される
- [ ] ケース17: ログイン済みで `GET /admin/rooms/add` にアクセスすると200で、CSRFトークン付きのフォームが返る
- [ ] ケース18: 正しいCSRFトークンで `POST /admin/rooms/add`（画像なし）すると、302で `/admin/rooms` にリダイレクトし、部屋が1件増える
- [ ] ケース19: CSRFトークンが不正・未送信の状態で `POST /admin/rooms/add` すると403になり、部屋は増えない
- [ ] ケース20: `GET /admin/rooms/edit/{id}` で存在しないIDを指定すると404になる
- [ ] ケース21: `POST /admin/rooms/delete/{id}` で部屋を削除すると、302で `/admin/rooms` にリダイレクトし、一覧から消える
- [ ] ケース22: `GET /admin/rooms/{id}/qr` にアクセスすると200・`Content-Type: image/png` の画像が返る

---

## 実装仕様

### src/repository/room_repository.rs

- `Room` 構造体（`id`, `event_id`, `room_name`, `quest_text`, `answer: Option<String>`, `hint_msg: Option<String>`, `image_id: Option<i32>`, `qr_uuid: String`）。`sqlx::FromRow` を導出
- `count(pool, event_id) -> Result<i64, sqlx::Error>`
- `insert(pool, event_id, room_name, quest_text, answer: Option<&str>, hint_msg: Option<&str>, image_id: Option<i32>, qr_uuid: &str) -> Result<i32, sqlx::Error>`（作成した部屋のIDを返す）
- `find_all(pool, event_id) -> Result<Vec<Room>, sqlx::Error>`
- `find_by_id(pool, id) -> Result<Option<Room>, sqlx::Error>`
- `update(pool, id, room_name, quest_text, answer: Option<&str>, hint_msg: Option<&str>, image_id: Option<i32>) -> Result<(), sqlx::Error>`
- `delete(pool, id) -> Result<(), sqlx::Error>`
- 既存の `event_repository` と同様、実行時チェックの `sqlx::query` / `Row::try_get` を使う（このプロジェクトの現行の慣習。コンパイル時マクロは使わない）

### src/repository/room_image_repository.rs

- `insert(pool, uuid: &str, data: &[u8], mime_type: &str) -> Result<i32, sqlx::Error>`
- `delete(pool, id: i32) -> Result<(), sqlx::Error>`

### src/services/image_service.rs

- サイズ上限 5MB、許可フォーマット JPEG/PNG（`image::guess_format` でマジックバイト判定）
- `process_upload(bytes: &[u8]) -> Result<Vec<u8>, ImageError>` — 上限超過・フォーマット不正・デコード失敗をそれぞれ区別できるエラー型を返す。成功時は幅800px以下・JPEG（品質80）にリサイズしたバイト列を返す

### src/services/qr_service.rs

- `render_png(value: &str) -> Vec<u8>` — `qrcode` crateで `value` をエンコードし、PNGバイト列にレンダリングして返す

### src/services/room_service.rs

- 定数 `MAX_ROOMS: i64 = 15`
- 新規登録用・更新用の入力構造体（`room_name`, `quest_text`, `answer: Option<String>`, `hint_msg: Option<String>`, `image_bytes: Option<Vec<u8>>` 等）を定義する
- `create(pool, event_id, input) -> Result<i32, RoomError>`
  1. `room_repository::count` が `MAX_ROOMS` 以上なら `RoomError::MaxRoomsReached`
  2. `event_repository::find_singleton` で現在の `require_answer_check` を取得
  3. `true` かつ `answer` が空なら `RoomError::AnswerRequired`
  4. `false` の場合、`answer` / `hint_msg` は常に `None` として扱う（送信値を無視する）
  5. `image_bytes` があれば `image_service::process_upload` → 成功したら `room_image_repository::insert`（UUIDは新規発行） → `image_id` を得る。失敗したら `RoomError::Image(...)`
  6. `room_repository::insert` で登録し、`qr_uuid`（新規UUID）を発行する
- `update(pool, id, input) -> Result<(), RoomError>` — 上記2〜4と同様のバリデーションに加え、`image_bytes` がある場合は既存の `image_id`（あれば）を `room_image_repository::delete` してから新しい画像を挿入し直す。`image_bytes` が無ければ既存の `image_id` を変更しない
- `delete(pool, id) -> Result<(), RoomError>` — `find_by_id` で `image_id` を取得し、あれば `room_image_repository::delete`、その後 `room_repository::delete`
- `list(pool, event_id) -> Result<Vec<Room>, RoomError>` / `get(pool, id) -> Result<Option<Room>, RoomError>`

### src/handlers/rooms.rs

- `GET /admin/rooms` — `room_service::list` の結果を `templates/admin/rooms/list.html` にレンダリング
- `GET /admin/rooms/add` — CSRFトークンを発行し `templates/admin/rooms/add.html` を返す
- `POST /admin/rooms/add` — `multipart/form-data` で `room_name`, `quest_text`, `answer`, `hint_msg`, `image`（任意）, `csrf_token` を受け取る。CSRF不一致は403。`room_service::create` が失敗したらエラー内容に応じたメッセージ付きで200再表示、成功したら302で `/admin/rooms`
- `GET /admin/rooms/edit/{id}` — 部屋が存在しなければ404。存在すれば現在値を埋めたフォーム（CSRFトークン発行）を返す
- `POST /admin/rooms/update/{id}` — `add` と同様の受け取り・CSRF検証。`room_service::update` が失敗したらエラー付きで200再表示、成功したら302で `/admin/rooms`
- `POST /admin/rooms/delete/{id}` — CSRF検証後 `room_service::delete` を呼び、302で `/admin/rooms`
- `GET /admin/rooms/{id}/qr` — 部屋が存在しなければ404。存在すれば `qr_service::render_png(&room.qr_uuid)` の結果を `Content-Type: image/png` で返す

### templates

- `templates/admin/_base.html` — Bootstrap 5（CDN）のナビゲーション（ダッシュボード・部屋管理へのリンク、ログアウトボタン）を持つ共通レイアウト。`{% block content %}` を子テンプレートに提供する
- `templates/admin/rooms/list.html` — `_base.html` を継承し、部屋一覧テーブル（部屋名・QRリンク・編集/削除リンク）と「新規登録」へのリンクを表示
- `templates/admin/rooms/add.html` / `edit.html` — `_base.html` を継承し、`room_name`, `quest_text`, 画像アップロード欄、（判定モードが `true` の場合のみ）`answer` / `hint_msg` 欄、CSRF隠しフィールドを持つフォーム。エラーメッセージがあれば表示する

---

## 制約・注意事項

- 既存の `GET /health`・管理者認証機能の挙動・テストを壊さないこと
- 画像アップロードは拡張子ではなくマジックバイトで検証すること（[SECURITY.md](../SECURITY.md) 参照）
- 削除・更新時に `room_images` の孤立行を残さないこと
- `require_admin` を通っていても、状態変更を伴うPOSTは必ずCSRF検証を行うこと
- `docs/api.md` に記載のパス・メソッドと一致させること
- `cargo clippy` が警告なく通ること

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] `cargo run` で `/admin/rooms` にアクセスし、部屋の登録・編集・削除・QR表示が手動で確認できた
