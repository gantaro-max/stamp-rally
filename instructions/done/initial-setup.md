# 実装指示書: プロジェクト初期セットアップ（依存関係・ディレクトリ構成・DBマイグレーション）

## 背景・目的

現状、`Cargo.toml` は依存関係が空、`src/main.rs` は `cargo init` 直後のテンプレート（`Hello, world!`）のままになっている。
[architecture.md](../../docs/architecture.md) で決定した技術スタック（Axum, sqlx, Askama, Argon2 等）を導入し、以降の機能実装（部屋管理・LINE Bot連携・LIFFチェックイン等）を進められる土台を作る。

この指示書のスコープは **アプリが起動し、DBに接続でき、ルーティングの土台ができている状態を作ること** まで。認証・ゲームロジック等のビジネスロジックは対象外（後続の指示書で追加していく）。

---

## 実装対象ファイル

- `Cargo.toml` — 依存クレートの追加
- `src/main.rs` — アプリケーションのエントリーポイント（DB接続・ルーター起動）
- `src/handlers/mod.rs` — ハンドラーモジュールの公開
- `src/handlers/health.rs` — 疎通確認用ハンドラー
- `src/services/mod.rs` — サービス層モジュールの雛形（空でよい）
- `src/repository/mod.rs` — リポジトリ層モジュールの雛形（空でよい）
- `migrations/0001_init.sql` — [database.md](../../docs/database.md) のテーブル定義に基づくDDL

---

## テストケース（TDDの起点）

Cargo.tomlへの依存追加・モジュールの空雛形・マイグレーションDDLは振る舞いを持たないためTDD対象外とする。
`GET /health` ハンドラーのみ、以下のテストケースを先に書き、失敗を確認してから実装すること（[AGENTS.md](../../AGENTS.md)のTDD規約参照）。

- [ ] ケース1: `GET /health` にリクエストすると、ステータス200かつボディ `"ok"` が返る

## 実装仕様

### Cargo.toml

以下の依存クレートを追加する（バージョンは実装時点の最新安定版を使用してよい）。

| クレート | features | 用途 |
|:--|:--|:--|
| `axum` | - | Webフレームワーク |
| `tokio` | `full` | 非同期ランタイム |
| `sqlx` | `mysql`, `runtime-tokio`, `macros`, `chrono`, `uuid` | DBアクセス |
| `askama` | - | テンプレートエンジン |
| `askama_axum`（または同等のAxum統合クレート） | - | AskamaテンプレートをAxumのレスポンスとして返す |
| `argon2` | - | パスワードハッシュ |
| `image` | - | 画像リサイズ |
| `qrcode` | - | QRコード生成 |
| `reqwest` | `json` | LINE Messaging API呼び出し |
| `serde` / `serde_json` | `derive` | シリアライズ |
| `dotenvy` | - | `.env` 読み込み |
| `tower-sessions`（または同等のセッション管理クレート） | - | Cookieベースのセッション管理 |
| `uuid` | `v4` | QRコード用UUID発行 |
| `chrono` | - | 日時操作 |
| `tracing` / `tracing-subscriber` | - | ロギング |

### src/main.rs

1. `dotenvy::dotenv()` で `.env` を読み込む（ファイルが存在しなくてもエラーにしない）
2. `tracing_subscriber` で簡易ロギングを初期化する
3. 環境変数 `DATABASE_URL` から `sqlx::MySqlPool` を作成する。接続失敗時はエラーメッセージを出力してプロセスを終了する
4. Axumの `Router` を作成し、`handlers::health` の `GET /health` ハンドラーを登録する
5. `0.0.0.0:8000` でリッスンする（[.devcontainer/compose.yaml](../../.devcontainer/compose.yaml) のポートマッピング `8099:8000` と一致させる）

### src/handlers/mod.rs, src/handlers/health.rs

- `health.rs`: `GET /health` 用のハンドラー関数を実装する。本文は文字列 `"ok"` を返すのみでよい
- `mod.rs`: `pub mod health;` として公開する

### src/services/mod.rs, src/repository/mod.rs

- 現時点では空のモジュール宣言のみでよい（後続の指示書で `auth_service` / `room_service` / `game_service` / `ranking_service` 等のサービスと、対応するリポジトリを追加していく）

### migrations/0001_init.sql

`sqlx-cli`（devcontainerに導入済み）のマイグレーション形式で、[database.md](../../docs/database.md) の以下のテーブルを作成するDDLを記述する。

- `events`（`id`, `event_name`, `admin_pass_hash`, `is_team_mode`, `require_answer_check`）
- `rooms`（`id`, `event_id`, `room_name`, `quest_text`, `answer`, `hint_msg`, `image_id`, `qr_uuid`）
- `players`（`id`, `line_user_id`, `event_id`, `player_name`, `current_room_id`, `answer_verified`, `started_at`, `finished_at`）
- `visited_rooms`（`player_id`, `room_id`, `visited_at`。複合主キー `(player_id, room_id)`）
- `room_images`（`id`, `uuid`, `data`, `mime_type`）

外部キー制約・ユニーク制約（`players` の `(line_user_id, event_id)` など）も `docs/database.md` の記載通りに設定する。

---

## 制約・注意事項

- シークレット（`DATABASE_URL` 等）はコードに直書きせず、必ず環境変数経由で読み込むこと（[CLAUDE.md](../../CLAUDE.md) 参照）
- この段階では認証・ゲームロジックは実装しない
- `cargo build` と `cargo clippy` が警告なく通ることを確認する
- マイグレーションは `sqlx migrate run` で適用できる形式にする

---

## 完了条件

- [ ] `GET /health` のテストケースについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] そのテストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もテストが通ることを確認した（Refactor）
- [ ] `cargo build` が成功する
- [ ] `cargo test` が通る
- [ ] `cargo clippy` が警告なしで通る
- [ ] devcontainer内で `sqlx migrate run` が成功し、[database.md](../../docs/database.md) の全テーブルが作成される
- [ ] `cargo run` でアプリが起動する
- [ ] `.env` の `DATABASE_URL` を使ってDBに接続できることを確認する
