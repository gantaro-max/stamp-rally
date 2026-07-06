# 実装指示書: イベント設定画面（Slice C）

## 背景・目的

これまでの機能（管理者認証・部屋管理・LINE Bot基盤・LIFFチェックイン）により、ゲームの進行ロジックは個人戦/チーム戦・判定モードの両方に対応済みだが、これらを切り替えるための管理画面がまだ無く、テストでは `events` 行を直接更新して両モードを再現していた。本スライスで、管理者がブラウザから個人戦/チーム戦・判定モードを切り替えられる `/admin/settings` を実装する。

[docs/architecture.md](../docs/architecture.md) の16節（本指示書と合わせて追記済み）で決定した方針に基づき、以下を実装する。

- `GET /admin/settings`：現在の設定を反映したフォームを表示
- `POST /admin/settings`：個人戦/チーム戦・判定モードの更新
- `event_service`：`events`（シングルトン）の取得・更新

`event_name`の編集、ランキング画面（`/admin/ranking`）はこの指示書のスコープ外（後続の指示書で対応）。また、既存の `room_service` / `handlers::rooms.rs` が `event_repository::find_singleton` を直接呼んでいる箇所を今回の指示書内で `event_service` 経由に置き換える必要はない（本スライスのスコープ外。新設する `event_service` はこの `/admin/settings` 機能のためだけに使う）。

---

## 実装対象ファイル

- `src/main.rs` — `admin_router` に `/settings` ルーターを追加
- `src/handlers/admin.rs` — `settings_form`（GET）・`update_settings`（POST）ハンドラーを追加
- `src/services/mod.rs` — `event_service` の公開
- `src/services/event_service.rs`（新規） — イベント設定の取得・更新ロジック
- `src/repository/event_repository.rs` — `update_settings` を追加
- `templates/admin/settings.html`（新規） — 設定フォーム
- `templates/admin/_base.html` — ナビゲーションに「設定」リンクを追加

---

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、以下の順にRed-Green-Refactorを回す。DBに依存するテストは `sqlx::test` を使うこと。

### event_repository（追加分、`sqlx::test`）

- [ ] ケース1: `update_settings` で `is_team_mode` / `require_answer_check` をそれぞれ `true`→`false`・`false`→`true` の両方向に更新できる（`find_singleton` で確認）

### event_service（`sqlx::test`）

- [ ] ケース2: `current` が唯一の `events` 行を返す
- [ ] ケース3: `update_settings` を呼ぶと、DB上の `is_team_mode` / `require_answer_check` が更新される

### ハンドラー（`sqlx::test` / 結合テスト）

- [ ] ケース4: 未ログイン状態で `GET /admin/settings` にアクセスすると302で `/auth/login` へリダイレクトされる
- [ ] ケース5: ログイン済みで `GET /admin/settings` にアクセスすると200が返り、現在の `is_team_mode` / `require_answer_check` の値がフォームのチェック状態（`checked`属性の有無）に反映されている（`true`の場合と`false`の場合の両方を確認する）
- [ ] ケース6: ログイン済み・正しいCSRFトークンで、両方のチェックボックスをオンにして `POST /admin/settings` すると302で `/admin/settings` にリダイレクトし、DBの `is_team_mode` / `require_answer_check` が両方 `true` になる
- [ ] ケース7: 同様に両方のチェックボックスをオフ（リクエストにフィールド自体を含めない）にして送信すると、DBの両方が `false` になる（チェックボックス未送信＝falseとして扱えることの確認）
- [ ] ケース8: CSRFトークンが不正・未送信の状態で `POST /admin/settings` すると403になり、DBの値は変化しない

---

## 実装仕様

### src/repository/event_repository.rs（追加分）

- `update_settings(pool: &MySqlPool, id: i32, is_team_mode: bool, require_answer_check: bool) -> Result<(), sqlx::Error>` — `UPDATE events SET is_team_mode = ?, require_answer_check = ? WHERE id = ?`

### src/services/event_service.rs

- `pub enum EventError { NotFound, Database(sqlx::Error) }`（`From<sqlx::Error>`を実装）
- `pub struct SettingsInput { pub is_team_mode: bool, pub require_answer_check: bool }`
- `pub async fn current(pool: &MySqlPool) -> Result<event_repository::Event, EventError>` — `event_repository::find_singleton` を呼び、`None` なら `EventError::NotFound`（起動時シードにより通常発生しない）
- `pub async fn update_settings(pool: &MySqlPool, input: SettingsInput) -> Result<(), EventError>` — `current` で現在の `id` を取得し、`event_repository::update_settings` を呼ぶ

### src/handlers/admin.rs

- `SettingsForm { #[serde(default)] is_team_mode: bool, #[serde(default)] require_answer_check: bool, csrf_token: String }`（`serde::Deserialize`。チェックボックス2項目は`#[serde(default)]`必須。[architecture.md](../docs/architecture.md) 16節参照）
- `pub async fn settings_form(session: Session, State(pool): State<MySqlPool>) -> impl IntoResponse` — 既存のCSRFトークン発行パターン（`csrf_service::issue_token`）を使い、`event_service::current` の結果を `templates/admin/settings.html` に渡してレンダリングする
- `pub async fn update_settings(session: Session, State(pool): State<MySqlPool>, Form(form): Form<SettingsForm>) -> impl IntoResponse` — 既存の `csrf_service::verify_token` パターンで検証（不一致は403）。成功したら `event_service::update_settings` を呼び、302で `/admin/settings` にリダイレクトする
- 既存の `rooms.rs` の各ハンドラーが `require_admin` ミドルウェア配下にある（`main.rs`の`admin_router`）のと同じ扱いにする

### templates/admin/settings.html

- `_base.html` を継承する
- `<input type="checkbox" name="is_team_mode" {% if event.is_team_mode %}checked{% endif %}>` のように、現在値に応じて `checked` を出し分ける（`require_answer_check` も同様）。それぞれラベルで「チーム戦にする」「QR＋正解入力を必須にする」等、わかる文言を付ける
- CSRF隠しフィールドを含める
- 送信ボタン1つ（「保存」）

### templates/admin/_base.html

- 既存の「部屋管理」リンクの並びに「設定」（`/admin/settings`）へのリンクを追加する

---

## 制約・注意事項

- 既存の `/health`・管理者認証・部屋管理・LINE Bot基盤・LIFFチェックインの挙動とテストを壊さないこと
- `require_admin` を通っていても、状態変更を伴うPOSTは必ずCSRF検証を行うこと
- 設定変更時に既存の `rooms`（`answer`/`hint_msg`）や `players`（進行状況）を書き換える処理を追加しないこと（16節参照。設定はイベントの現在値としてのみ扱われ、既存データへの遡及的な影響を与えない設計のため）
- `event_name` の編集フォームを追加しないこと（スコープ外）
- 既存の `room_service` / `handlers::rooms.rs` が `event_repository::find_singleton` を直接呼んでいる箇所を `event_service` 経由に置き換えるリファクタリングは行わないこと（本スライスのスコープ外。指示が無い範囲のリファクタリングをしないこと）
- `docs/api.md` に記載のパス・メソッドと一致させること
- `cargo clippy --all-targets -- -D warnings` が警告なく通ること

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy --all-targets -- -D warnings` が警告なく通る
- [ ] `cargo run` で `/admin/settings` にアクセスし、個人戦/チーム戦・判定モードの切替が保存され、画面に反映されることを手動で確認した
