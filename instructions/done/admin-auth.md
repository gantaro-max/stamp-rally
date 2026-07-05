# 実装指示書: 管理者認証（ログイン・ログアウト・セッション保護）

## 背景・目的

現状、アプリには `GET /health` の疎通確認ハンドラーしか存在しない。今後実装する部屋管理・ランキング等の管理画面（`/admin/*`）はすべて管理者セッション認証を前提とするため、まずその土台となる認証機能を実装する。

[architecture.md](../../docs/architecture.md) の「認証方式」「セッション実装」「CSRF実装」「起動時シード処理」で決定した方針に基づき、以下を実装する。

- アプリ起動時、`events` テーブルが空であれば `ADMIN_PASSWORD` 環境変数からArgon2ハッシュを生成し初期の1行をシードする
- `POST /auth/login` によるログイン（セッションCookie発行）
- `POST /auth/logout` によるログアウト（セッション無効化）
- `/admin/*` および `POST /auth/logout` を保護する `require_admin` ミドルウェア
- セッションに保存したトークンとフォーム隠しフィールドを突き合わせるCSRF対策（ダブルサブミット方式）

この指示書のスコープは **認証の仕組みと、それを検証するための最小限の `/admin/dashboard` プレースホルダー** まで。ダッシュボードの実コンテンツ（設定状況・部屋数・ランキングへのリンク）は対象外で、後続の指示書（部屋管理機能）で `handlers::admin::dashboard` を拡張する。

---

## 実装対象ファイル

- `src/main.rs` — `SessionManagerLayer` の適用、起動時シード処理の呼び出し、`/auth` `/admin` ルーターの登録
- `src/middleware/mod.rs`（新規） — ミドルウェアモジュールの公開
- `src/middleware/require_admin.rs`（新規） — セッションに `admin_authenticated = true` が無い場合 `/auth/login` へ302リダイレクトするミドルウェア
- `src/handlers/mod.rs` — `auth` `admin` モジュールの公開
- `src/handlers/auth.rs`（新規） — `GET /auth/login`, `POST /auth/login`, `POST /auth/logout`
- `src/handlers/admin.rs`（新規） — `GET /admin/dashboard`（プレースホルダー）
- `src/services/mod.rs` — `auth_service` `csrf_service` の公開
- `src/services/auth_service.rs`（新規） — パスワードのハッシュ化・検証、ログイン処理、起動時シード処理
- `src/services/csrf_service.rs`（新規） — CSRFトークンの発行・検証
- `src/repository/mod.rs` — `event_repository` の公開
- `src/repository/event_repository.rs`（新規） — `events` テーブルへのアクセス（件数取得・シード用INSERT・1行取得）
- `templates/auth/login.html`（新規） — ログイン画面（Askama）

---

## テストケース（TDDの起点）

[AGENTS.md](../../AGENTS.md) のTDD規約に従い、以下の順にRed-Green-Refactorを回す。DBに依存するテスト（`event_repository`）は `sqlx::test` を使うこと。

- [ ] ケース1: `auth_service` のパスワードハッシュ化・検証が正しく往復する（平文一致で `true`、不一致で `false`。ハッシュ値自体は平文と異なる）
- [ ] ケース2: `events` テーブルが0件の状態でシード処理を実行すると、`ADMIN_PASSWORD` をArgon2ハッシュ化した1行が作成される
- [ ] ケース3: `events` テーブルに既に1行存在する状態でシード処理を実行しても、行数が変わらない（重複作成されない）
- [ ] ケース4: `GET /auth/login` は200を返し、レスポンスボディに `action="/auth/login"` のPOSTフォームと `csrf_token` の隠しフィールドが含まれる
- [ ] ケース5: 正しいパスワード・正しいCSRFトークンで `POST /auth/login` すると、302で `/admin/dashboard` へリダイレクトし、セッションCookieが発行される
- [ ] ケース6: 誤ったパスワードで `POST /auth/login` すると、200でログイン画面が再表示され、エラーメッセージが含まれる（管理者セッションは確立されない）
- [ ] ケース7: CSRFトークンが空・不一致・未送信の状態で `POST /auth/login` すると403を返す
- [ ] ケース8: 未ログイン状態で `GET /admin/dashboard` にアクセスすると302で `/auth/login` へリダイレクトされる
- [ ] ケース9: ログイン済みセッションで `GET /admin/dashboard` にアクセスすると200が返る
- [ ] ケース10: 未ログイン状態で `POST /auth/logout` すると302で `/auth/login` へリダイレクトされる（`require_admin` の対象であることの確認）
- [ ] ケース11: ログイン済みセッションで `POST /auth/logout`（正しいCSRFトークン付き）すると302で `/auth/login` へリダイレクトし、以降同じセッションCookieで `GET /admin/dashboard` にアクセスしても302になる（セッションが無効化されている）

---

## 実装仕様

### src/repository/event_repository.rs

- `Event` 構造体（`id`, `event_name`, `admin_pass_hash`, `is_team_mode`, `require_answer_check`）。`sqlx::FromRow` を導出する
- `count(pool: &MySqlPool) -> Result<i64, sqlx::Error>` — `events` の件数を返す
- `insert_initial(pool: &MySqlPool, event_name: &str, admin_pass_hash: &str) -> Result<(), sqlx::Error>` — 初期の1行をINSERTする（`is_team_mode = false`, `require_answer_check = false`）
- `find_singleton(pool: &MySqlPool) -> Result<Option<Event>, sqlx::Error>` — `events` の1行目を取得する（`LIMIT 1`。本運用では常に1行）
- 生SQLは `sqlx::query!` / `sqlx::query_as!` マクロを使う（`Cargo.toml` の `macros` フィーチャは導入済み。devcontainer内で `DATABASE_URL` を参照してコンパイル時検証される想定）

### src/services/auth_service.rs

- `hash_password(plain: &str) -> String` — Argon2でハッシュ化（ソルトはArgon2のデフォルト機構でランダム生成）
- `verify_password(plain: &str, hash: &str) -> bool` — 検証。ハッシュのパースに失敗した場合も `false` を返す（panicしない）
- `seed_admin_event_if_empty(pool: &MySqlPool, admin_password: &str, event_name: &str) -> Result<(), ...>` — `event_repository::count` が0の場合のみ `hash_password` した値で `insert_initial` を呼ぶ
- `try_login(pool: &MySqlPool, submitted_password: &str) -> Result<bool, ...>` — `event_repository::find_singleton` を取得し `verify_password` で照合する。`events` が0件（シード未実施）の場合は `false` を返す

### src/services/csrf_service.rs

- `issue_token(session: &Session) -> String` — セッションに既存のトークンがあればそれを返し、無ければ `Uuid::new_v4()` で新規発行してセッションに保存してから返す
- `verify_token(session: &Session, submitted: &str) -> bool` — セッション内のトークンと `submitted` を比較する。セッションにトークンが無い場合は `false`

### src/middleware/require_admin.rs

- `axum::middleware::from_fn` 等で実装し、`Session` からセッション値 `admin_authenticated`（bool）を読む
- `true` でなければ `/auth/login` への302レスポンスを即返す（後続ハンドラーを呼ばない）
- `true` であれば後続へ処理を渡す

### src/handlers/auth.rs

- `GET /auth/login`
  - `csrf_service::issue_token` でトークンを発行し、`templates/auth/login.html` をレンダリングして返す（エラーメッセージは無し）
- `POST /auth/login`
  - フォームで `csrf_token` と `password` を受け取る
  - `csrf_service::verify_token` が `false` なら403を返す（テンプレートは返さずステータスのみでよい）
  - `auth_service::try_login` が `true` ならセッションに `admin_authenticated = true` を保存し、`/admin/dashboard` へ302
  - `false` ならログイン画面をエラーメッセージ付きで200で再表示する（この際も新しいCSRFトークンを発行し直す）
- `POST /auth/logout`（`require_admin` 経由でのみ到達する想定だが、ハンドラー自身もCSRF検証を行う）
  - `csrf_service::verify_token` が `false` なら403
  - セッションを破棄（`Session::flush()` 等でストア上のデータを空にする）し、`/auth/login` へ302

### src/handlers/admin.rs

- `GET /admin/dashboard` — 認証済みであることの確認用プレースホルダー。ステータス200、ボディは `"ok"` などの簡易文字列でよい（実コンテンツは後続の指示書で追加）

### templates/auth/login.html

- Askama テンプレート。Bootstrap 5（CDN）を用いた簡易なログインフォーム
- `method="post"` `action="/auth/login"` のフォームに、`password` 入力欄と `csrf_token` の `type="hidden"` フィールドを含める
- エラーメッセージ（あれば）をフォーム上部に表示する

### src/main.rs

- 起動時、DB接続後に `auth_service::seed_admin_event_if_empty` を呼ぶ。`ADMIN_PASSWORD` が未設定かつ `events` が0件の場合はエラーを出力してプロセスを終了する（`DATABASE_URL` 未設定時と同様の扱い）
- `tower_sessions::SessionManagerLayer`（`MemoryStore`、`Expiry::OnInactivity(time::Duration::hours(12))`）をルーター全体に適用する
- `/auth` ルーター（`GET/POST /auth/login`, `POST /auth/logout`）と `/admin` ルーター（`GET /admin/dashboard`）を登録する
- `/admin` ルーターと `POST /auth/logout` に `require_admin` ミドルウェアを適用する（`GET /auth/login` `POST /auth/login` は対象外）

---

## 制約・注意事項

- パスワードは平文で保存・ログ出力しない（Argon2ハッシュのみ保存）
- `ADMIN_PASSWORD` はコードに直書きせず、環境変数から読む
- CSRF検証は `POST /auth/login` `POST /auth/logout` の両方で行う（`require_admin` を通っていても省略しない）
- `require_admin` は必ず「セッション不備 → 即302リダイレクト」で、後続処理（DBアクセス等）を実行しないこと
- 既存の `GET /health` の挙動・テストを壊さないこと
- `docs/api.md` に記載のパス・メソッド・認証要否と一致させること
- `cargo clippy` が警告なく通ること

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] `cargo run` でアプリが起動し、`.env` の `ADMIN_PASSWORD` でログインできることを手動確認した
