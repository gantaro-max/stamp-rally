# 実装指示書: ダッシュボードへの友だち追加QRコード表示

## 背景・目的

参加者はスタンプラリーを始める前に、まずLINE公式アカウントを友だち追加する必要があるが、現状その導線（友だち追加用QRコード）を運営が現地で提示する手段が無い。管理画面ダッシュボード（`/admin/dashboard`）に友だち追加QRコードを表示し、受付スタッフがその場で参加者に提示できるようにする。

基本設計は [docs/architecture.md](../docs/architecture.md) 22節、要件は [docs/requirements.md](../docs/requirements.md)（管理者機能「友だち追加QRコードの表示」）、APIは [docs/api.md](../docs/api.md)（`/admin/dashboard`・`/admin/line-qr`の行）を参照。実装前に必ず目を通すこと。

QRコードの中身は、部屋QR（`docs/architecture.md` 5節、`src/handlers/rooms.rs::qr`、`src/services/qr_service.rs`）と全く同じ仕組みを再利用する。`qr_service::render_png(value: &str) -> Vec<u8>` は既に汎用的な関数であり、**変更は不要**。

## 実装対象ファイル

- `.env.example` — 新規環境変数 `LINE_ADD_FRIEND_URL` を追記（コメント付き、値は空のまま）
- `src/main.rs` — `AppState` に `line_add_friend_url: Option<Arc<str>>` を追加。起動時の環境変数読み込み処理を追加（未設定でもプロセスを終了しない）。新規ルート `GET /admin/line-qr` を登録。既存の `AppState::new` 呼び出し・テストコードを新しいシグネチャに追従させる
- `src/handlers/admin.rs` — `dashboard` ハンドラーを `State<AppState>` を受け取るように変更し、`DashboardTemplate` に `line_add_friend_url: Option<String>` を追加。新規ハンドラー `line_qr` を追加
- `templates/admin/dashboard.html` — 友だち追加QRコードのセクションを追加（4枚目のstat-card）

## テストケース（TDDの起点）

`src/main.rs` の `#[cfg(test)] mod tests` に追加する（既存の `authenticated_session_can_view_dashboard_with_empty_rooms` 等と同じ形式）。

- [ ] ケース1: `LINE_ADD_FRIEND_URL` 相当の値を設定していない状態（`AppState.line_add_friend_url = None`）でログイン済みセッションが `GET /admin/dashboard` にアクセスすると、200が返り、レスポンスHTMLに「LINE_ADD_FRIEND_URL が未設定です」旨の案内文が含まれ、`<img src="/admin/line-qr">` は含まれない
- [ ] ケース2: `AppState.line_add_friend_url = Some("https://lin.ee/test1234")` の状態でログイン済みセッションが `GET /admin/dashboard` にアクセスすると、200が返り、レスポンスHTMLに `<img src="/admin/line-qr">` と `https://lin.ee/test1234`（URLのテキスト表示）の両方が含まれる
- [ ] ケース3: `AppState.line_add_friend_url = Some("https://lin.ee/test1234")` の状態でログイン済みセッションが `GET /admin/line-qr` にアクセスすると、200・`Content-Type: image/png` が返り、レスポンスボディがPNGとしてデコード可能で、デコードしたQRコードの内容が `"https://lin.ee/test1234"` と一致する（`src/services/qr_service.rs` の既存テスト `renders_png_qr_code_with_original_value` と同じ検証方法。`rqrr` crateは既に依存関係に含まれている）
- [ ] ケース4: `AppState.line_add_friend_url = None` の状態でログイン済みセッションが `GET /admin/line-qr` にアクセスすると、404が返る
- [ ] ケース5: 未ログインで `GET /admin/line-qr` にアクセスすると、`/auth/login` へ302リダイレクトされる（`require_admin` ミドルウェアが適用されていることの確認。既存の `admin_dashboard_redirects_when_not_logged_in` 等と同じパターン）
- [ ] ケース6（回帰）: 既存のダッシュボード関連テスト（`authenticated_session_can_view_dashboard_with_empty_rooms` / `authenticated_dashboard_shows_room_count` / `authenticated_dashboard_shows_team_and_answer_check_modes`）が、`dashboard` ハンドラーのシグネチャ変更後も引き続きパスすること

環境変数の解決ロジック（未設定・空文字列を `None` として扱う部分）は、`resolve_port`（`src/main.rs`、`PORT` 環境変数のパースを行う既存の純粋関数）と同じ考え方で、`main()` から切り出した独立関数として実装し、単体テストを追加する。

- [ ] ケース7: 空文字列や `None` を渡すと `None` を返し、値ありの文字列を渡すとその文字列を `Some` で返す純粋関数のテスト（例: `resolve_line_add_friend_url`）

## 実装仕様

### `.env.example`

`LIFF_ID` のブロックの近くに以下を追記する（既存のコメントの書き方に合わせる）。

```
# LINE公式アカウントマネージャー（manager.line.biz）で発行される「友だち追加URL」
# （例: https://lin.ee/xxxxx）。ダッシュボードの友だち追加QRコードに使う。
# 未設定でもアプリは起動する（LINE公式アカウント発行前でも他の機能を動かせるようにするため）
LINE_ADD_FRIEND_URL=
```

### `src/main.rs`

- `AppState` 構造体に `pub line_add_friend_url: Option<Arc<str>>,` を追加する（フィールドの位置は `line_login_channel_id` の直後を推奨）
- `AppState::new` のシグネチャに `line_add_friend_url: Option<String>` を追加し（既存の `impl Into<Arc<str>>` パラメータとは異なり、`Option` のまま受け取って内部で `.map(Into::into)` する）、`Self { ..., line_add_friend_url: line_add_friend_url.map(Into::into), ... }` とする
- 環境変数の解決を純粋関数として切り出す（`resolve_port` の直後などに配置）:
  ```rust
  fn resolve_line_add_friend_url(value: Option<&str>) -> Option<String> {
      value
          .map(str::trim)
          .filter(|value| !value.is_empty())
          .map(str::to_owned)
  }
  ```
- `main()` 内、他のLINE関連環境変数を読んでいる箇所の近くに以下を追加する（他の変数と異なり `process::exit` しないことに注意）:
  ```rust
  let line_add_friend_url =
      resolve_line_add_friend_url(env::var("LINE_ADD_FRIEND_URL").ok().as_deref());
  ```
- `app_router_with_state(AppState::new(...))` の呼び出し（`main()` 内、116行目付近）に `line_add_friend_url` を追加する
- `#[cfg(test)] fn app_router(pool: MySqlPool) -> Router`（170行目付近）の `AppState::new(...)` 呼び出しに `None` を追加する
- `admin_router` に新規ルートを追加する: `.route("/line-qr", get(handlers::admin::line_qr))`（`require_admin` の `route_layer` が既にこのルーター全体に適用されているため、追加の保護設定は不要）
- 他に `AppState::new(` を呼び出している箇所（`src/handlers/liff.rs:221`・`src/handlers/line_webhook.rs:224`・`src/handlers/line_webhook.rs:265`、いずれもテストコード）にも `None` を追加すること。これらを直さないとビルドが失敗する

### `src/handlers/admin.rs`

- `use crate::services::{csrf_service, event_service, ranking_service, room_service};` の直前・直後に `use crate::AppState;` を追加する
- `dashboard` 関数のシグネチャを `pub async fn dashboard(session: Session, State(state): State<AppState>) -> Response` に変更し、関数内で使っていた `pool` を `&state.pool` に置き換える
- `DashboardTemplate` に `line_add_friend_url: Option<String>` を追加し、`dashboard` 関数内で `state.line_add_friend_url.as_deref().map(str::to_owned)` を渡す（テンプレート側の型を `Arc<str>` ではなく `String` に揃えるため。Askamaは`Option<Arc<str>>`より`Option<String>`の方が既存の`error_message: Option<String>`パターン（`templates/auth/login.html`参照）と一貫する）
- 新規関数 `line_qr` を追加する。`src/handlers/rooms.rs::qr`（396行目付近）と同じ形にする:
  ```rust
  pub async fn line_qr(State(state): State<AppState>) -> Response {
      let Some(url) = state.line_add_friend_url.as_deref() else {
          return StatusCode::NOT_FOUND.into_response();
      };
      let png = qr_service::render_png(url);

      ([(header::CONTENT_TYPE, "image/png")], png).into_response()
  }
  ```
  （`qr_service` は既に `use` 済み）

### `templates/admin/dashboard.html`

既存3枚のstat-cardの列に、4枚目として追加する（`col-12 col-md-4`だと4枚で行が崩れるため、他3枚を含めたレイアウト調整は実装者の裁量でよいが、Bootstrap 5のgridクラス自体の使用は既存カードと統一すること）。中身は `auth/login.html` の `{% match %}` パターンを踏襲する:

```html
<div class="col-12 col-md-4">
  <div class="stat-card">
    <div class="stat-label">友だち追加QRコード</div>
    {% match line_add_friend_url %}
    {% when Some with (url) %}
    <img src="/admin/line-qr" alt="友だち追加QRコード" class="img-fluid mb-2" style="max-width: 160px;">
    <p class="mb-0 text-break">{{ url }}</p>
    {% when None %}
    <p class="mb-0">LINE_ADD_FRIEND_URL が未設定です</p>
    {% endmatch %}
  </div>
</div>
```

文言・マークアップの細部はこの例に厳密に一致させる必要はないが、テストケース1・2で検証する内容（未設定時の案内文、設定時の`<img src="/admin/line-qr">`とURLテキスト）は必ず満たすこと。

## 制約・注意事項

- `qr_service::render_png` は変更しない（既に汎用的な `&str -> PNG` 関数のため）
- `LINE_ADD_FRIEND_URL` は他のLINE関連環境変数（`LINE_CHANNEL_SECRET`等）と異なり、未設定でもプロセスを終了させないこと。これは意図的な設計（`docs/architecture.md` 22節）であり、他の変数と同じ「未設定ならエラー終了」パターンに揃えないこと
- `GET /admin/line-qr` は他の管理画面と同様 `require_admin` 配下に置き、CSRF保護は不要（状態変更を伴わないため）
- スコープはダッシュボード画面と `/admin/line-qr` のみ。他の画面（部屋管理・設定・ランキング）・LINE Bot側の返信ロジックは変更しないこと
- `admin/_base.html` のナビゲーション・ログアウトフォームの構成は変更しないこと

## 完了条件

- [ ] 上記7テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
