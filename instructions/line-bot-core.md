# 実装指示書: LINE Bot基盤・ゲーム進行ロジック（Slice A）

## 背景・目的

管理者機能（認証・部屋管理、`feature/admin-auth` / `feature/room-management` マージ済み）により、イベント準備（部屋登録・QR発行）は完成した。次はプレイヤー向けの中核機能である、LINE Bot経由のゲーム進行（参加登録〜部屋案内〜正誤判定〜ヒント〜リセット）を実装する。

[architecture.md](../docs/architecture.md) の8〜14節（本指示書と合わせて追記済み）で決定した方針に基づき、以下を実装する。

- LINE Webhook (`POST /callback`) の署名検証とイベント受信
- `game_service`：参加登録、部屋のランダム割当、ヒント、リセット、正誤判定（チェックインより前の会話ロジック）
- `line_client`：署名検証・Flex Message組み立て・LINE Messaging API（Reply）への送信
- `GET /public/image/{uuid}`：部屋画像の公開配信（Flex Messageが参照するため本スライスで必要。`room-management`指示書ではスコープ外としていたもの）

この指示書のスコープは「LINEでの会話が完結するところ」まで。**LIFFでのQRチェックイン（`/liff/checkin`）、`visited_rooms`への記録、ゴール判定（`finished_at`の設定）は対象外**（後続の指示書で追加する）。そのため `players.finished_at` を読むロジックは実装するが、書き込むロジックはこの指示書では登場しない。

イベント設定画面（`/admin/settings`）はまだ存在しないため、テストでは `events` 行を直接更新して `is_team_mode` / `require_answer_check` の両方の組み合わせを再現すること（`room-management`指示書と同様の方針）。

---

## 実装対象ファイル

- `Cargo.toml` — `hmac`, `sha2`, `base64` を追加（LINE Webhook署名検証用）
- `.env.example` — 反映済み（`PUBLIC_BASE_URL` 追加）。`main.rs`で読む変数名と一致していることを確認する
- `src/main.rs` — `AppState`導入（`FromRef`で既存の`State<MySqlPool>`ハンドラーを壊さない）、起動時の環境変数必須チェック追加、`/callback` `/public/image/{uuid}` ルーター登録
- `src/handlers/mod.rs` — `line_webhook` `image` モジュールの公開
- `src/handlers/line_webhook.rs`（新規） — `POST /callback`
- `src/handlers/image.rs`（新規） — `GET /public/image/{uuid}`
- `src/services/mod.rs` — `game_service` `line_client` の公開
- `src/services/game_service.rs`（新規） — コマンド分岐・部屋割当・正誤判定・登録待ち状態管理
- `src/services/line_client.rs`（新規） — 署名検証・メッセージJSON組み立て・LINE API送信
- `src/repository/mod.rs` — `player_repository` の公開
- `src/repository/player_repository.rs`（新規） — `players`テーブルへのアクセス
- `src/repository/room_repository.rs` — `find_random_unvisited`を追加
- `src/repository/room_image_repository.rs` — `find_by_uuid`を追加

---

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、以下の順にRed-Green-Refactorを回す。DBに依存するテストは `sqlx::test` を使うこと。

### line_client（DB・ネットワーク非依存の純粋関数）

- [ ] ケース1: 正しいチャネルシークレットで計算した署名は、`verify_signature`がtrueを返す
- [ ] ケース2: 誤ったシークレット・改ざんされたボディのいずれかで計算した署名は、`verify_signature`がfalseを返す
- [ ] ケース3: 署名ヘッダーが空文字の場合、`verify_signature`はfalseを返す
- [ ] ケース4: `build_text_message`が `{"type":"text","text":"..."}` 相当のJSONを返す
- [ ] ケース5: 画像URLありで`build_quest_flex_message`を呼ぶと、`hero`に画像URLを含むFlex Message JSONが返り、`altText`が設定されている
- [ ] ケース6: 画像URLなしで`build_quest_flex_message`を呼ぶと、`hero`を含まないFlex Message JSONが返る

### room_repository（`sqlx::test`、追加分）

- [ ] ケース7: 3部屋登録済み・1部屋訪問済み（`visited_rooms`に直接INSERT）の状態で`find_random_unvisited`を呼ぶと、訪問済みでない2部屋のいずれかが返る
- [ ] ケース8: 登録済みの全部屋が訪問済みの状態で`find_random_unvisited`を呼ぶと`None`が返る
- [ ] ケース9: 部屋が1件も無いイベントで`find_random_unvisited`を呼ぶと`None`が返る

### room_image_repository（`sqlx::test`、追加分）

- [ ] ケース10: 画像を1件INSERTし、その`uuid`で`find_by_uuid`を呼ぶと`data`・`mime_type`が一致する
- [ ] ケース11: 存在しない`uuid`で`find_by_uuid`を呼ぶと`None`が返る

### player_repository（`sqlx::test`、新規）

- [ ] ケース12: `insert`後に`find_by_line_user_and_event`で取得すると、`player_name`・`current_room_id = None`・`answer_verified = false`・`finished_at = None`が一致する
- [ ] ケース13: `update_current_room`を呼ぶと`current_room_id`が更新され、`answer_verified`が`false`にリセットされる（事前に`true`にしておいたものが`false`に戻ることを確認）
- [ ] ケース14: `set_answer_verified(true)`を呼ぶと`answer_verified`が`true`になる
- [ ] ケース15: `delete_by_line_user_and_event`を呼ぶと`players`行が削除され、`find_by_line_user_and_event`は`None`を返す
- [ ] ケース16: 訪問記録が残っている状態で`delete_by_line_user_and_event`を呼んでも、`visited_rooms`テーブルへの直接クエリで確認するとエラーにならず削除できる（既存の`ON DELETE CASCADE`が効いていることの確認。手動で`visited_rooms`にINSERTしてから検証する）

### game_service（`sqlx::test`、LINE送信なし。`ReplyMessage`の内容を検証する）

- [ ] ケース17: 個人戦イベントで未登録ユーザーが「開始」→ 登録待ち状態になり、個人名の入力を促す`ReplyMessage::Text`が返る
- [ ] ケース18: チーム戦イベント（`events.is_team_mode = true`）で未登録ユーザーが「開始」→ チーム名の入力を促す文言になる
- [ ] ケース19: 登録待ち中に任意の名前テキストを送ると`players`行が作成され、部屋が1つ割り当てられ（`current_room_id`が設定される）、`ReplyMessage::Quest`（部屋名・クエスト文が一致）が返る
- [ ] ケース20: 登録待ち中に空白のみのテキストを送ると、再度入力を促す`ReplyMessage::Text`が返り、`players`行は作成されない
- [ ] ケース21: 登録待ち中に部屋が1件も登録されていない状態で有効な名前を送ると、「参加できる部屋がない」旨のエラー文言が返り、`players`行は作成されない
- [ ] ケース22: 登録済み・未クリアのユーザーが「開始」を送ると、現在の`current_room_id`のクエストが再送される（`current_room_id`は変化しない）
- [ ] ケース23: 登録済み・クリア済み（`finished_at`を直接セット）のユーザーが「開始」を送ると、クリア済みの案内が返り、新規登録は行われない
- [ ] ケース24: 登録済みユーザーが「リセット」を送ると`players`行が削除される
- [ ] ケース25: 未登録・登録待ちでもないユーザーが「リセット」を送ると、「登録されていません」の案内が返る
- [ ] ケース26: 任意の状態（未登録／登録待ち／登録済み）から「遊び方」または「ヘルプ」を送ると、常にガイド文言が返る
- [ ] ケース27: `require_answer_check = false`のイベントで登録済みユーザーが「ヒント」を送ると、「利用できません」の案内が返る
- [ ] ケース28: `require_answer_check = true`のイベントで登録済みユーザーが「ヒント」を送ると、現在の部屋の`hint_msg`が返る
- [ ] ケース29: `require_answer_check = true`・`hint_msg = NULL`の部屋で「ヒント」を送ると、「ヒントは登録されていません」の案内が返る
- [ ] ケース30: `require_answer_check = true`・`answer_verified = false`で、`rooms.answer`（例: `"Red, blue"`）に対し前後空白・大小文字違いのテキスト（例: `" red "`）を送ると正解と判定され、`answer_verified`が`true`になり「QRを読み込んでください」の案内が返る
- [ ] ケース31: 同条件で不一致のテキストを送ると、`answer_verified`は`false`のまま「不正解です」の案内が返る
- [ ] ケース32: `require_answer_check = true`・`answer_verified = true`の状態で任意のテキストを送ると、正誤判定を行わずに「QRを読み込んでください」の案内が再送される
- [ ] ケース33: `require_answer_check = false`のイベントで登録済みユーザーが自由入力（コマンド以外）を送ると、正誤判定を行わずに「QRコードを読み込んでください」の案内が返る
- [ ] ケース34: 未登録・登録待ちでもないユーザーが上記いずれのコマンドにも該当しないテキストを送ると、「『開始』と送信してください」の案内が返る

### ハンドラー（`sqlx::test` / 結合テスト）

- [ ] ケース35: `x-line-signature`ヘッダーを付けずに`POST /callback`すると401になる
- [ ] ケース36: 不正な署名で`POST /callback`すると401になる
- [ ] ケース37: 正しい署名・テキストメッセージイベントを含むボディで`POST /callback`すると200になる（LINEへの実送信は本環境ではネットワーク到達性が無いため成否を検証しない。`game_service`が正しく呼ばれ、DBの状態が期待通り変化していること＝例えば未登録ユーザーの「開始」で登録待ち状態になることをもってテストの合格条件とする。この制約は`AGENTS.md`の`sqlx::test`に関する既知の環境制約と同様の扱いとして、テストコード中にコメントで明記する）
- [ ] ケース38: `GET /public/image/{uuid}` に存在するuuidでアクセスすると200・`Content-Type: image/jpeg`・bodyが登録した画像バイト列と一致する
- [ ] ケース39: 存在しないuuidで`GET /public/image/{uuid}`にアクセスすると404になる

---

## 実装仕様

### Cargo.toml

- `hmac = "0.12"` / `sha2 = "0.10"` / `base64 = "0.22"` を追加する（依存追加自体はTDD対象外。追加によって生まれる`verify_signature`の振る舞いはケース1〜3で検証する）

### src/repository/player_repository.rs

- `Player`構造体（`id`, `line_user_id: String`, `event_id: i32`, `player_name: String`, `current_room_id: Option<i32>`, `answer_verified: bool`, `started_at: chrono::NaiveDateTime`, `finished_at: Option<chrono::NaiveDateTime>`）。`sqlx::FromRow`导出はせず、既存の`event_repository`/`room_repository`と同様に`Row::try_get`で手動マッピングする（本プロジェクトの現行の慣習）
- `find_by_line_user_and_event(pool, line_user_id: &str, event_id: i32) -> Result<Option<Player>, sqlx::Error>`
- `insert(pool, line_user_id: &str, event_id: i32, player_name: &str) -> Result<i32, sqlx::Error>` — `started_at = NOW()`、`current_room_id = NULL`、`answer_verified = FALSE`、`finished_at = NULL`で作成し、作成したIDを返す
- `update_current_room(pool, player_id: i32, room_id: i32) -> Result<(), sqlx::Error>` — `current_room_id`を更新し、同時に`answer_verified = FALSE`にリセットする（1つのUPDATE文で両方行う）
- `set_answer_verified(pool, player_id: i32, verified: bool) -> Result<(), sqlx::Error>`
- `delete_by_line_user_and_event(pool, line_user_id: &str, event_id: i32) -> Result<(), sqlx::Error>`

### src/repository/room_repository.rs（追加分）

- `find_random_unvisited(pool, event_id: i32, player_id: i32) -> Result<Option<Room>, sqlx::Error>` — `rooms`のうち`event_id`が一致し、かつ`visited_rooms`に`(player_id, room_id)`が存在しない行から`ORDER BY RAND() LIMIT 1`で1件取得する。乱数生成のためだけに新しいcrateは追加せず、SQLの`RAND()`で完結させる

### src/repository/room_image_repository.rs（追加分）

- `find_by_uuid(pool, uuid: &str) -> Result<Option<(Vec<u8>, String)>, sqlx::Error>` — `(data, mime_type)`のタプル、または`None`を返す

### src/services/line_client.rs

- `pub fn verify_signature(channel_secret: &str, body: &[u8], signature_header: &str) -> bool`
  - `hmac`/`sha2`で`HMAC-SHA256(channel_secret, body)`を計算し、`base64`で標準エンコードした文字列と`signature_header`を比較する
  - 比較はタイミング攻撃対策として、単純な`==`ではなく、両者をバイト列にデコードした上でXOR差分を全バイト累積して`0`かどうかを見る、といった定数時間比較のヘルパーを自前で実装して使う（新規crateは追加しない）。長さが異なる場合は明確に`false`を返す（デコード失敗時も`false`）
- `pub fn build_text_message(text: &str) -> serde_json::Value` — `{"type": "text", "text": text}`
- `pub fn build_quest_flex_message(room_name: &str, quest_text: &str, image_url: Option<&str>) -> serde_json::Value` — `type: "flex"`、`altText`（例: `"{room_name} のクエスト"`）、`contents`は`type: "bubble"`。`image_url`が`Some`なら`hero`（`type: "image"`, `size: "full"`, `aspectRatio: "20:13"`, `aspectMode: "cover"`）を含める。`body`は`room_name`（太字）と`quest_text`（`wrap: true`）のテキストを縦に並べる
- `pub fn to_line_message(reply: &crate::services::game_service::ReplyMessage) -> serde_json::Value` — `ReplyMessage::Text`は`build_text_message`、`ReplyMessage::Quest { room_name, quest_text, image_url }`は`build_quest_flex_message`に委譲する
- `pub async fn send_reply(client: &reqwest::Client, access_token: &str, reply_token: &str, message: serde_json::Value) -> Result<(), LineClientError>` — `POST https://api.line.me/v2/bot/message/reply`、ヘッダー`Authorization: Bearer {access_token}`、ボディ`{"replyToken": reply_token, "messages": [message]}`。ステータスが成功でなければ`LineClientError`を返す。**この関数は自動テストの対象外**（本開発環境にはLINE APIへのネットワーク到達性が無いため。`AGENTS.md`記載の`sqlx::test`のDB接続同様の既知の環境制約として、実装コード中に短いコメントで明記する）
- `LineClientError`は`reqwest::Error`をラップするenumでよい

### src/services/game_service.rs

- `pub type PendingRegistrations = std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>;`（キーは`line_user_id`）
- `pub enum ReplyMessage { Text(String), Quest { room_name: String, quest_text: String, image_url: Option<String> } }`
- `pub enum GameServiceError { Database(sqlx::Error) }`（`From<sqlx::Error>`を実装）
- `pub async fn handle_text_message(pool: &MySqlPool, pending: &PendingRegistrations, public_base_url: &str, line_user_id: &str, text: &str) -> Result<ReplyMessage, GameServiceError>`
  - [architecture.md](../docs/architecture.md) 10節の優先順位に厳密に従って分岐する
  - `event_repository::find_singleton`で`event`を取得（`events`が空の状態は起動時シードにより発生しない前提。取得できなければ`GameServiceError`ではなくpanicにせず、`Text`で汎用エラー文言を返す簡易フォールバックでよい）
  - `player_repository::find_by_line_user_and_event`で`player`を取得
  - 名前入力確定時（登録待ち状態かつ有効なテキスト）：
    1. `room_repository::count(pool, event.id)`（既存の`room_service`が使っているものと同じ関数）が0なら、登録待ち状態を解除し「参加できる部屋が登録されていません。管理者にお問い合わせください。」を返す（`players`行は作成しない）
    2. `player_repository::insert`で登録
    3. `room_repository::find_random_unvisited`で部屋を選出し、`player_repository::update_current_room`で反映
    4. 部屋情報から`quest_reply_for_room`相当の内部ヘルパーで`ReplyMessage::Quest`を組み立てて返す（画像がある場合は`image_url = Some(format!("{public_base_url}/public/image/{uuid}"))`、無ければ`None`）
  - 正誤判定（`rooms.answer`をカンマ区切り）：各候補と入力の両方を`trim()`＋小文字化してから比較し、1つでも一致すれば正解
  - `pending`（`Mutex<HashSet<String>>`）への出し入れは同期ロックで完結させ、`.await`をまたいで保持しない

### src/handlers/line_webhook.rs

- `pub async fn callback(State(state): State<AppState>, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> axum::http::StatusCode`
  1. `x-line-signature`ヘッダーを取得。無ければ`StatusCode::UNAUTHORIZED`
  2. `line_client::verify_signature(&state.line_channel_secret, &body, signature)`が`false`なら`StatusCode::UNAUTHORIZED`
  3. `body`をLINE Webhookのイベント配列としてJSONパース（`serde_json`。`events: Vec<{ type, replyToken, source: { userId }, message: Option<{ type, text }> }>`程度の最小限の構造体でよい）。パース失敗時は`StatusCode::OK`を返す（LINEの仕様上、原因不明のペイロードで再送ループを起こさないため。この判断は8節参照）
  4. 各イベントについて、`type == "message"`かつ`message.type == "text"`のものだけを処理する。それ以外はスキップする
  5. `game_service::handle_text_message`を呼び、成功したら`line_client::to_line_message` → `line_client::send_reply`で送信する。この間に発生したどのエラー（DB・LINE送信）もログに記録し、次のイベントの処理を継続する
  6. 常に`StatusCode::OK`を返す（署名検証を通過した後は、個別イベントの結果に関わらず200。理由は8節参照）

### src/handlers/image.rs

- `pub async fn serve(State(pool): State<MySqlPool>, Path(uuid): Path<String>) -> impl IntoResponse`
  - `room_image_repository::find_by_uuid`で検索。`None`なら`StatusCode::NOT_FOUND`
  - `Some((data, mime_type))`なら`([(header::CONTENT_TYPE, mime_type)], data)`を返す

### src/main.rs

- `AppState`（`#[derive(Clone)]`）を追加：`pool: MySqlPool`, `line_channel_secret: std::sync::Arc<str>`, `line_channel_access_token: std::sync::Arc<str>`, `public_base_url: std::sync::Arc<str>`, `pending_registrations: game_service::PendingRegistrations`
- `impl axum::extract::FromRef<AppState> for MySqlPool { fn from_ref(state: &AppState) -> Self { state.pool.clone() } }` を実装し、既存の`State<MySqlPool>`を使うハンドラー（`admin`・`rooms`・`auth`・`health`）は一切変更しない
- 起動時、`LINE_CHANNEL_SECRET` / `LINE_CHANNEL_ACCESS_TOKEN` / `PUBLIC_BASE_URL`を`DATABASE_URL`と同じパターン（未設定ならエラー出力して`process::exit(1)`）で読み込む
- `app_router`の戻り値・`with_state`の型を`AppState`に変更し、ルーターを追加:
  - `.route("/callback", post(handlers::line_webhook::callback))`
  - `.route("/public/image/{uuid}", get(handlers::image::serve))`（`require_admin`は通さない）
- 既存のテストモジュール（`#[cfg(test)] mod tests`）内で`app_router`を呼んでいる箇所・`State<MySqlPool>`を直接組み立てているテスト用ルーターがあれば、型の整合を保つよう最小限調整する（既存テストの意図・アサーションは変更しない）

---

## 制約・注意事項

- 既存の`/health`・管理者認証・部屋管理の挙動とテストを壊さないこと（`AppState`導入によるリファクタリングであり、動作は変えない）
- LINE Webhookの署名検証は生のリクエストボディ（`Bytes`）に対して行うこと。JSONへデシリアライズしてから再シリアライズしたバイト列を検証に使わないこと（[SECURITY.md](../SECURITY.md) 参照）
- 署名検証の比較は定数時間比較を用いること（タイミング攻撃対策）
- `/callback` と `/public/image/{uuid}` はCSRF検証・`require_admin`のいずれも通さないこと（[docs/api.md](../docs/api.md) 参照）
- `game_service`はLINE固有のJSON構造・`reqwest`を一切知らないこと（`line_client`に閉じ込める）。この分離により`game_service`のテストは`sqlx::test`のみで完結し、ネットワークに依存しないこと
- `line_client::send_reply`（実ネットワーク呼び出し）はテスト対象外とし、その旨をコード中に一言コメントで残すこと。テストを書かないことの言い訳に使わず、`build_text_message`・`build_quest_flex_message`・`to_line_message`・`verify_signature`は必ずテストすること
- 正誤判定・ヒント・部屋案内はすべて`require_answer_check`（イベント単位）の値で分岐すること。クライアント（LINEメッセージ本文）からの自己申告を信用しないこと（DBの状態のみを正とする）
- `docs/api.md` / `docs/architecture.md` に記載のパス・分岐と一致させること
- `cargo clippy` が警告なく通ること

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] `cargo run` で、LINE Developersコンソールの検証機能または`curl`で署名付きリクエストを`/callback`に送り、「開始」→名前入力→部屋案内（Flex Message）までの一連の会話が手動で確認できた（実際のLINEアカウントでのエンドツーエンド確認が難しい場合は、`curl`でヘッダー・ボディを手動生成して署名検証〜200応答までを確認した記録を報告に残す）
