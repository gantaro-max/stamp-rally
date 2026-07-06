# 実装指示書: LIFFチェックイン・ゴール判定（Slice B）

## 背景・目的

`feature/line-bot-core`（PR #5、マージ済み）により、LINE Bot上での会話（開始・部屋案内・ヒント・正誤判定・リセット）は完成した。次はゲームループを完結させる、LIFFでのQRコードチェックイン・ゴール判定を実装する。

[docs/architecture.md](../docs/architecture.md) の15節（本指示書と合わせて追記済み）で決定した方針に基づき、以下を実装する。

- LIFFページ（`GET /liff/checkin`）：QRスキャンボタンを表示する最小限のページ
- チェックインAPI（`POST /liff/checkin`）：LINEのIDトークン検証、部屋UUIDの照合、`visited_rooms`への記録、ゴール判定
- `game_service::checkin`：チェックインの判定ロジック（部屋の存在・案内済み部屋との一致・正誤判定済みか・ゴール判定・次の部屋のランダム割当）
- `line_client`：IDトークン検証（LINEの `/oauth2/v2.1/verify` 呼び出し）、Push Message送信（次の部屋案内・クリア報告をLINEチャットに送る）

この指示書のスコープはチェックインとゴール判定まで。イベント設定画面（`/admin/settings`）・ランキング画面（`/admin/ranking`）は対象外（後続の指示書で追加する）。

`events.require_answer_check` を切り替えるUIはまだ存在しないため、テストでは `events` 行を直接更新して両モードを再現すること（`room-management`・`line-bot-core` と同じ方針）。

---

## 実装対象ファイル

- `.env.example` — 反映済み（`LINE_LOGIN_CHANNEL_ID` 追加）
- `src/main.rs` — `AppState` に `liff_id` / `line_login_channel_id` / `verify_id_tokens` を追加、`LIFF_ID` を起動時必須環境変数に格上げ、`GET /liff/checkin` `POST /liff/checkin` ルーター登録
- `src/handlers/mod.rs` — `liff` モジュールの公開
- `src/handlers/liff.rs`（新規） — `GET /liff/checkin`（ページ表示）・`POST /liff/checkin`（チェックインAPI）
- `templates/liff/checkin.html`（新規） — LIFF SDKを使った最小限のQRスキャンページ
- `src/services/game_service.rs` — `checkin` 関数・`CheckinOutcome` / `CheckinRejection` を追加
- `src/services/line_client.rs` — IDトークン検証（`verify_id_token` / レスポンスパース関数）・`push_message` を追加
- `src/repository/room_repository.rs` — `find_by_qr_uuid` を追加
- `src/repository/player_repository.rs` — `insert_visited_room` / `count_visited` / `mark_finished` を追加

---

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、以下の順にRed-Green-Refactorを回す。DBに依存するテストは `sqlx::test` を使うこと。

### room_repository（追加分、`sqlx::test`）

- [ ] ケース1: `find_by_qr_uuid` が、指定した `qr_uuid` に一致する部屋を返す
- [ ] ケース2: `find_by_qr_uuid` が、存在しない `qr_uuid` に対して `None` を返す

### player_repository（追加分、`sqlx::test`）

- [ ] ケース3: `insert_visited_room` で記録した行数だけ `count_visited` が増える（複数回挿入して確認）
- [ ] ケース4: `mark_finished` を呼ぶと `finished_at` が設定される（`find_by_line_user_and_event` で確認）

### line_client（追加分、DB・ネットワーク非依存の純粋関数のみ）

- [ ] ケース5: LINEの検証エンドポイントが返す形式のJSON文字列（例: `{"sub": "line-user-1", "exp": 1234567890, "aud": "channel-id"}`）をパースする関数が、`sub` の値を正しく取り出す
- [ ] ケース6: `sub` フィールドが無い、またはJSONとして不正な文字列を渡すと、パース関数がエラーを返す
- [ ] ケース7: `build_text_message` を用いた既存のPush Message組み立て（`to_line_message` の再利用）が、チェックイン完了時のテキスト・クリア時のテキストそれぞれで期待通りのJSONになる（新規のメッセージ組み立て関数を追加した場合はそれを対象にする）
- 実際にLINEへHTTPリクエストを送る `verify_id_token` の通信部分・`push_message` はテスト対象外とする（`line-bot-core` の `send_reply` と同じ、この開発環境にネットワーク到達性が無いため）。その旨をコード中に一言コメントで残すこと

### game_service::checkin（`sqlx::test`、LINE送信・IDトークン検証なし。`line_user_id: &str` を直接渡してテストする）

- [ ] ケース8: 未訪問の部屋が残っている状態で、案内済みの部屋のUUIDを正しいプレイヤーが送ると、`visited_rooms` に記録され、`current_room_id` が新しい未訪問の部屋に更新され、`answer_verified` が `false` にリセットされ、`CheckinOutcome::NextQuest` が返る
- [ ] ケース9: 最後の1部屋（登録済み部屋数と `visited_rooms` の件数が一致することになる部屋）をチェックインすると、`finished_at` が設定され、`CheckinOutcome::Cleared` が返る
- [ ] ケース10: 存在しない `qr_uuid` を渡すと `CheckinOutcome::Rejected(CheckinRejection::RoomNotFound)` が返り、DBは変化しない
- [ ] ケース11: 参加登録されていない `line_user_id` を渡すと `Rejected(NotRegistered)` が返る
- [ ] ケース12: 案内されていない部屋（`current_room_id` と不一致）のUUIDを渡すと `Rejected(WrongRoom)` が返り、`visited_rooms` は増えない
- [ ] ケース13: `require_answer_check = true` のイベントで `answer_verified = false` の状態でチェックインすると `Rejected(AnswerNotVerified)` が返り、`visited_rooms` は増えない
- [ ] ケース14: `require_answer_check = true` のイベントで `answer_verified = true` の状態なら正常にチェックインできる（境界確認）
- [ ] ケース15: 既に `finished_at` が設定済みのプレイヤーが（案内された部屋が無い状態で）チェックインを試みると `Rejected(AlreadyFinished)` が返る

### ハンドラー（`sqlx::test` / 結合テスト。`AppState.verify_id_tokens = false` にして `id_token` フィールドの値をそのまま `line_user_id` として扱うテスト用経路を使う）

- [ ] ケース16: 未訪問の部屋が残っている状態で正しい `POST /liff/checkin` を送ると200・`{"status":"next"}` が返り、DBの `current_room_id` が更新されている
- [ ] ケース17: 最後の部屋を `POST /liff/checkin` すると200・`{"status":"cleared"}` が返り、`finished_at` が設定されている
- [ ] ケース18: 存在しない `qr_uuid` で `POST /liff/checkin` すると404・`{"status":"rejected","reason":"room_not_found"}` が返る
- [ ] ケース19: 案内されていない部屋のUUIDで `POST /liff/checkin` すると403・`{"status":"rejected","reason":"wrong_room"}` が返る
- [ ] ケース20: `GET /liff/checkin` にアクセスすると200が返り、レスポンスHTMLに `LIFF_ID` の値（環境変数から読み込んだ値）が含まれている

---

## 実装仕様

### src/repository/room_repository.rs（追加分）

- `find_by_qr_uuid(pool: &MySqlPool, qr_uuid: &str) -> Result<Option<Room>, sqlx::Error>`（既存の `find_by_id` と同様、手動で `Row::try_get` する）

### src/repository/player_repository.rs（追加分）

- `insert_visited_room(pool: &MySqlPool, player_id: i32, room_id: i32) -> Result<(), sqlx::Error>` — `visited_rooms` に `(player_id, room_id, visited_at = NOW())` を挿入する
- `count_visited(pool: &MySqlPool, player_id: i32) -> Result<i64, sqlx::Error>` — `visited_rooms` で `player_id` に一致する行数を返す
- `mark_finished(pool: &MySqlPool, player_id: i32) -> Result<(), sqlx::Error>` — `players.finished_at = NOW()` を設定する

### src/services/line_client.rs（追加分）

- `pub struct IdTokenClaims { pub sub: String }`（`serde::Deserialize`。他のフィールド（`exp`/`aud`等）は無視してよい。`#[serde(deny_unknown_fields)]` は付けない）
- `pub fn parse_id_token_claims(body: &str) -> Result<IdTokenClaims, serde_json::Error>` — LINEの検証エンドポイントのレスポンスボディ（JSON文字列）から `IdTokenClaims` をデシリアライズする純粋関数。ケース5・6はこの関数をテストする
- `pub async fn verify_id_token(client: &reqwest::Client, id_token: &str, channel_id: &str) -> Result<IdTokenClaims, LineClientError>` — `POST https://api.line.me/oauth2/v2.1/verify`、`Content-Type: application/x-www-form-urlencoded`、ボディ `id_token={id_token}&client_id={channel_id}`。ステータスが成功でなければ `LineClientError::ApiStatus`。成功時はレスポンスボディを `parse_id_token_claims` に渡す。**この関数の実ネットワーク呼び出し自体は自動テストの対象外**（`send_reply` と同様の既知の環境制約）
- `pub async fn push_message(client: &reqwest::Client, access_token: &str, to: &str, message: serde_json::Value) -> Result<(), LineClientError>` — `POST https://api.line.me/v2/bot/message/push`、ボディ `{"to": to, "messages": [message]}`。`send_reply` と同様の構造。**この関数もテスト対象外**
- 既存の `to_line_message` をそのまま再利用してチェックイン後の案内メッセージ（`ReplyMessage::Quest` / `ReplyMessage::Text`）をPush Message用のJSONに変換する

### src/services/game_service.rs（追加分）

- `pub enum CheckinRejection { RoomNotFound, NotRegistered, AlreadyFinished, WrongRoom, AnswerNotVerified }`
- `pub enum CheckinOutcome { NextQuest(ReplyMessage), Cleared, Rejected(CheckinRejection) }`
- `pub async fn checkin(pool: &MySqlPool, public_base_url: &str, line_user_id: &str, room_qr_uuid: &str) -> Result<CheckinOutcome, GameServiceError>`
  - [architecture.md](../docs/architecture.md) 15節の判定順序（部屋の存在 → 参加登録の有無 → クリア済みか → 案内された部屋と一致するか → 正誤判定済みか → 記録 → ゴール判定）に厳密に従う
  - ゴール判定は「記録後の `count_visited` が `room_repository::count(pool, event.id)` 以上」で行う（15部屋固定にせず、実際の登録数を基準にする）
  - ゴール到達時は `player_repository::mark_finished` を呼び `CheckinOutcome::Cleared` を返す。未到達時は `room_repository::find_random_unvisited` で次の部屋を選び `player_repository::update_current_room`（既存関数。`answer_verified` のリセットも兼ねる）してから、既存の（同ファイル内で再利用できる）クエスト組み立てロジックで `CheckinOutcome::NextQuest` を返す
  - `room_repository::find_random_unvisited` が理論上あり得ない `None` を返した場合（ゴール判定の条件と矛盾する状態）は、防御的に `Cleared` として扱ってよい

### src/handlers/liff.rs

- リクエストボディ用の構造体: `struct CheckinRequest { id_token: String, qr_uuid: String }`（`serde::Deserialize`）
- レスポンスは `serde_json::json!` で組み立てる: 成功系 `{"status": "next"}` / `{"status": "cleared"}`、拒否系 `{"status": "rejected", "reason": "<snake_case>"}`
- `pub async fn checkin_page(State(state): State<AppState>) -> impl IntoResponse` — `templates/liff/checkin.html` に `state.liff_id` を渡してレンダリングする
- `pub async fn checkin(State(state): State<AppState>, Json(body): Json<CheckinRequest>) -> impl IntoResponse`
  1. `state.verify_id_tokens` が `true` の場合、`line_client::verify_id_token(&state.http_client, &body.id_token, &state.line_login_channel_id)` を呼ぶ。失敗したら `401` ＋ `{"status":"rejected","reason":"invalid_id_token"}`。成功したら `claims.sub` を `line_user_id` とする
  2. `false` の場合（テスト用経路）、`body.id_token` の値をそのまま `line_user_id` として扱う
  3. `game_service::checkin(&state.pool, &state.public_base_url, &line_user_id, &body.qr_uuid)` を呼ぶ
  4. `CheckinOutcome::NextQuest(reply)` → `line_client::push_message(&state.http_client, &state.line_channel_access_token, &line_user_id, line_client::to_line_message(&reply))` を呼ぶ（送信失敗はログに残すのみで無視。13節参照）→ `200 {"status":"next"}`
  5. `CheckinOutcome::Cleared` → 「クリアしました！最初の部屋に戻ってください。お疲れ様でした！」のテキストをPushで送る（送信失敗は同様に無視）→ `200 {"status":"cleared"}`
  6. `CheckinOutcome::Rejected(reason)` → `RoomNotFound` は `404`、それ以外（`NotRegistered`/`AlreadyFinished`/`WrongRoom`/`AnswerNotVerified`）は `403`。`reason` はそれぞれ `room_not_found`/`not_registered`/`already_finished`/`wrong_room`/`answer_not_verified` という文字列にする

### templates/liff/checkin.html

- `_base.html` は継承しない（ログイン前・管理画面外の独立ページ。`templates/auth/login.html` と同じ扱い）
- LINEのLIFF SDK（`https://static.line-scdn.net/liff/edge/2/sdk.js`）を`<script>`タグで読み込み、`liff.init({ liffId: "{{ liff_id }}" })` を実行する
- 「QRを読む」ボタンを1つ表示し、クリック時に `liff.scanCodeV2()` を呼び出す。スキャン結果の `value` と `liff.getIDToken()` を `POST /liff/checkin` に `fetch` でJSON送信し、レスポンスの `status`（`next`/`cleared`/`rejected`）に応じたメッセージを画面に表示する（詳細なクエスト文・画像は表示しない。LINEチャットを確認するよう案内するのみ。15節参照）

### src/main.rs

- `AppState` に `liff_id: Arc<str>`・`line_login_channel_id: Arc<str>`・`verify_id_tokens: bool` を追加し、`AppState::new` の引数を増やす（`verify_id_tokens` はデフォルト `true`）。既存の呼び出し箇所（`line-bot-core` で追加されたテスト等）が壊れないよう、コンストラクタの引数追加に伴う修正を行う
- 起動時、`LIFF_ID` と `LINE_LOGIN_CHANNEL_ID` を他のLINE関連環境変数と同じパターン（未設定なら `process::exit(1)`）で読み込む
- ルーターに以下を追加する:
  - `.route("/liff/checkin", get(handlers::liff::checkin_page).post(handlers::liff::checkin))`（`require_admin` は通さない）

---

## 制約・注意事項

- 既存の `/health`・管理者認証・部屋管理・LINE Bot基盤（`line-bot-core`）の挙動とテストを壊さないこと
- クライアント（LIFFページのJavaScript）が送ってくるLINEユーザーIDを直接信用しないこと。必ずIDトークン検証を経由すること（[SECURITY.md](../SECURITY.md) 参照）
- `/liff/checkin`（GET・POST）はCSRF検証・`require_admin`のいずれも通さないこと
- `game_service::checkin`はLINE固有のJSON構造・`reqwest`・IDトークン検証を一切知らないこと（`line_client`とハンドラー側に閉じ込める）。`line_user_id`は文字列としてすでに検証済みの前提で受け取る
- チェックイン成立後の案内内容（次のクエスト・クリア報告）は必ずLINEチャットへのPush Messageとして送ること。`/liff/checkin`のレスポンス自体にクエスト文・画像を含めないこと（15節）
- ゴール判定は登録済み部屋数（`room_repository::count`）を基準にすること。`15`をハードコードしないこと
- `docs/api.md` / `docs/architecture.md` に記載のパス・分岐と一致させること
- `cargo clippy --all-targets -- -D warnings` が警告なく通ること（テストコードも対象に含める。`line-bot-core`の差し戻しで判明した見落としを繰り返さないこと）

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy --all-targets -- -D warnings` が警告なく通る
- [ ] `cargo run` で、`LIFF_ID`・`LINE_LOGIN_CHANNEL_ID` を設定した上で `GET /liff/checkin` にアクセスし、ページが表示されることを手動で確認した（実際のLIFF環境・LINEアカウントでのQRスキャンによるエンドツーエンド確認が難しい場合は、`curl`で`POST /liff/checkin`に対して`verify_id_tokens`をテスト用経路にした場合の応答を確認した記録を報告に残す）
