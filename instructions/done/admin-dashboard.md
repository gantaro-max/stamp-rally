# 実装指示書: 管理画面ダッシュボード（`/admin/dashboard`）の実装

## 背景・目的

`GET /admin/dashboard`は、管理者認証機能（#3）実装時に「保護ミドルウェア（`require_admin`）の動作確認用プレースホルダー」として`"ok"`という文字列を返すだけの仮実装のまま、その後の各機能スライス（イベント設定・部屋管理・ランキング）でも中身を実装するタスクが誰にも割り当てられず積み残しになっていた。

[docs/operator-guide.md](../docs/operator-guide.md)2節は、このページに以下3セクションが存在する前提で運営向けの説明を書いており、実装とドキュメントが食い違っている状態だった。今回、実際のダッシュボード画面を実装してこの食い違いを解消する。基本設計は[docs/architecture.md](../docs/architecture.md)19節を参照。

## 実装対象ファイル

- `src/handlers/admin.rs` — `dashboard`関数を実装（現状の`pub async fn dashboard() -> &'static str { "ok" }`を置き換える）
- `templates/admin/dashboard.html`（新規） — ダッシュボード画面テンプレート

## テストケース（TDDの起点）

- [ ] ケース1（回帰確認）: 既存の`tests::admin_dashboard_redirects_when_not_logged_in`（`src/main.rs`）が、実装後も引き続きパスすること（未ログイン時は302リダイレクト）
- [ ] ケース2: ログイン済みセッションで部屋が0件のとき、`GET /admin/dashboard`が200を返し、レスポンスHTMLに以下が含まれる
  - 個人戦/チーム戦の現在設定を示す文言（`events.is_team_mode`に対応するもの）
  - 判定モードの現在設定を示す文言（`events.require_answer_check`に対応するもの）
  - 部屋登録数が「0」であることが分かる表示（上限15との対比、例:「0 / 15部屋」）
  - `/admin/rooms`・`/admin/settings`・`/admin/ranking`それぞれへの`<a href="...">`リンク
- [ ] ケース3: 部屋を2件登録した状態で`GET /admin/dashboard`にアクセスすると、部屋登録数の表示が「2」になる
- [ ] ケース4: イベントが`is_team_mode = true`・`require_answer_check = true`のとき、ケース2とは異なる文言（チーム戦・QR＋正解入力である旨）が表示される（個人戦/QRのみの場合と出し分けられていることの確認）

## 実装仕様

### src/handlers/admin.rs

- 既存の`ranking`関数（同ファイル91行目付近）と同じパターンで実装する:
  ```rust
  pub async fn dashboard(session: Session, State(pool): State<MySqlPool>) -> Response {
      let csrf_token = match csrf_service::issue_token(&session).await {
          Ok(token) => token,
          Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
      };
      let event = match event_service::current(&pool).await {
          Ok(event) => event,
          Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
      };
      let rooms = match room_service::list(&pool, event.id).await {
          Ok(rooms) => rooms,
          Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
      };
      let template = DashboardTemplate {
          csrf_token,
          is_team_mode: event.is_team_mode,
          require_answer_check: event.require_answer_check,
          room_count: rooms.len(),
      };
      match template.render() {
          Ok(body) => Html(body).into_response(),
          Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
      }
  }
  ```
- `room_service`を`use crate::services::{csrf_service, event_service, ranking_service};`のuse文に追加する
- `DashboardTemplate`構造体を`RankingTemplate`等と同様に`#[derive(Template)]` `#[template(path = "admin/dashboard.html")]`で定義する。フィールドは`csrf_token: String`, `is_team_mode: bool`, `require_answer_check: bool`, `room_count: usize`
- 部屋の上限15はテンプレート側にハードコードしてよい（`docs/database.md`「最大登録数: 15部屋」に対応する既存の前提。他の箇所でも15はハードコードされている）
- 新規のservice/repository関数は不要（`event_service::current`・`room_service::list`のみで足りる）

### templates/admin/dashboard.html

`admin/_base.html`を継承し、`admin/ranking.html`等の既存テンプレートと同じBootstrap 5のスタイルで、以下3セクションを表示する:

```html
{% extends "admin/_base.html" %}
{% block title %}ダッシュボード{% endblock %}
{% block content %}
<h1 class="h3 mb-4">ダッシュボード</h1>

<div class="card mb-3">
  <div class="card-body">
    <h2 class="h5">イベント設定状況</h2>
    <p>
      {% if is_team_mode %}チーム戦{% else %}個人戦{% endif %} /
      {% if require_answer_check %}QR＋正解入力{% else %}QR読み取りのみ{% endif %}
    </p>
    <a href="/admin/settings">設定を編集</a>
  </div>
</div>

<div class="card mb-3">
  <div class="card-body">
    <h2 class="h5">部屋一覧</h2>
    <p>登録済み: {{ room_count }} / 15部屋</p>
    <a href="/admin/rooms">部屋管理へ</a>
  </div>
</div>

<div class="card mb-3">
  <div class="card-body">
    <h2 class="h5">ランキング</h2>
    <a href="/admin/ranking">ランキングを見る</a>
  </div>
</div>
{% endblock %}
```

文言・マークアップの細部（カードのレイアウト等）はこの例に厳密に一致させる必要はないが、テストケース2〜4で検証する内容（個人戦/チーム戦・判定モードの文言、部屋数表示、3つのリンク）は必ず満たすこと。

## 制約・注意事項

- スコープはダッシュボード画面のみ。他の画面（部屋管理・設定・ランキング）のテンプレート・ハンドラーは変更しないこと
- `admin/_base.html`のナビゲーション・ログアウトフォームの構成は変更しないこと
- ダッシュボード自体は状態変更を伴わないためCSRF検証は不要（`csrf_token`の発行は共有レイアウトのログアウトフォームのために必要なだけ）
- `docs/operator-guide.md`2節の記載（イベント設定状況／部屋一覧／ランキングの3セクション）と矛盾しない実装にすること

## 完了条件

- [ ] 上記4テストケースについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
