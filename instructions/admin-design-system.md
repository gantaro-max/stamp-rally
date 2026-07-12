# 実装指示書: 管理画面デザインシステムの導入

## 背景・目的

管理画面（`/admin/*`・`/auth/login`）がBootstrap 5の既定スタイルのままで、視認性・ブランドの一貫性に欠けるとのフィードバックがあった。[docs/architecture.md 20節「管理画面デザインシステム」](../docs/architecture.md#20-管理画面デザインシステム)に設計を追記済み。本指示書はこれに基づく実装指示。

外部から取り込まれた`DESIGN.md`（Anthropicのブランドデザイントークン）を確認したが、配色・専用書体・ロゴマーク等のAnthropicブランド固有要素は一切流用していない。本指示書のカラートークンはStampRallyBot独自に新規定義したものである。

## 実装対象ファイル

- `templates/admin/_base.html` — `<head>`にCSSカスタムプロパティ・ユーティリティクラスを定義する`<style>`ブロックを追加し、ナビゲーションバーを新トークンで restyle
- `templates/auth/login.html` — 同様の`<style>`ブロック（サブセット）を追加し、`bg-light`クラスを除去
- `templates/admin/dashboard.html` — 3枚の`stat-card`構成に restyle（部屋登録進捗・イベント設定・ランキング導線）
- `templates/admin/rooms/list.html` — `page-header`パターン・テーブルスタイルの適用
- `templates/admin/rooms/add.html` / `templates/admin/rooms/edit.html` — `page-header`パターンの適用、「戻る」ボタンを`btn-outline-secondary`に変更
- `templates/admin/settings.html` — `page-header`パターンの適用
- `templates/admin/ranking.html` — `page-header`パターンの適用、ランキング1位の行に`badge-rank-first`を表示

## テストケース（TDDの起点）

本変更は表示レイヤー（Askamaテンプレートの静的マークアップ・CSS）のみの変更であり、[AGENTS.md](../AGENTS.md)コーディング規約に定める「振る舞いを持たない変更」に該当する。ハンドラーの引数・戻り値・DBクエリ・ルーティングは一切変更しないため、新規のRed/Greenサイクルは不要とする。

- [ ] 既存テスト`src/handlers/rooms.rs`の`room_templates_include_logout_csrf_token`が変更後も失敗せず通ることを確認する（`_base.html`のログアウトフォーム構造 `action="/auth/logout"` ・ `name="csrf_token" value="..."` を変更しないこと）
- [ ] `cargo test`が全体で通ることを確認する（振る舞いが変わっていないことの確認であり、新規テストの追加は必須ではない）
- [ ] 実装後、`cargo run`でローカル起動し、以下の画面を目視確認する: `/auth/login`、`/admin/dashboard`、`/admin/rooms`、`/admin/rooms/add`、`/admin/rooms/edit/{id}`、`/admin/settings`、`/admin/ranking`

## 実装仕様

### templates/admin/_base.html

`<head>`内、Bootstrap CSSの`<link>`の直後に以下の`<style>`ブロックを追加する:

```html
<style>
  :root {
    --admin-primary: #B54B3A;
    --admin-primary-hover: #973C2E;
    --admin-primary-soft: #F3E3DF;
    --admin-bg: #F4F5F7;
    --admin-surface: #FFFFFF;
    --admin-border: #E2E4E9;
    --admin-text: #1F2328;
    --admin-text-muted: #6B7280;
    --admin-success: #2E9E6D;
    --admin-success-soft: #E1F3EA;
    --admin-radius-card: 12px;
    --admin-radius-control: 8px;

    --bs-primary: var(--admin-primary);
    --bs-primary-rgb: 181, 75, 58;
    --bs-body-bg: var(--admin-bg);
    --bs-body-color: var(--admin-text);
    --bs-border-color: var(--admin-border);
    --bs-border-radius: var(--admin-radius-control);
    --bs-border-radius-lg: var(--admin-radius-card);
    --bs-body-font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Hiragino Kaku Gothic ProN", "Hiragino Sans", Meiryo, sans-serif;
  }

  .btn-primary {
    background-color: var(--admin-primary);
    border-color: var(--admin-primary);
  }
  .btn-primary:hover,
  .btn-primary:active {
    background-color: var(--admin-primary-hover);
    border-color: var(--admin-primary-hover);
  }
  .btn-outline-primary {
    color: var(--admin-primary);
    border-color: var(--admin-primary);
  }
  .btn-outline-primary:hover {
    background-color: var(--admin-primary);
    border-color: var(--admin-primary);
  }

  .navbar {
    background-color: var(--admin-surface) !important;
    border-bottom: 1px solid var(--admin-border);
  }
  .navbar-brand {
    color: var(--admin-primary) !important;
    font-weight: 700;
  }

  .page-header {
    margin-bottom: 1.5rem;
  }
  .page-header h1 {
    font-weight: 700;
    font-size: 1.5rem;
    margin-bottom: 0.25rem;
  }
  .page-header .page-subtitle {
    color: var(--admin-text-muted);
    font-size: 0.9rem;
    margin-bottom: 0;
  }

  .card {
    border-color: var(--admin-border);
    border-radius: var(--admin-radius-card);
  }

  .stat-card {
    background: var(--admin-surface);
    border: 1px solid var(--admin-border);
    border-radius: var(--admin-radius-card);
    padding: 1.25rem 1.5rem;
    height: 100%;
  }
  .stat-card .stat-label {
    font-size: 0.8rem;
    color: var(--admin-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    margin-bottom: 0.5rem;
  }
  .stat-card .stat-value {
    font-size: 1.75rem;
    font-weight: 700;
    color: var(--admin-text);
    margin-bottom: 0.5rem;
  }
  .stat-card .stat-link {
    font-size: 0.9rem;
  }

  .badge-mode {
    background-color: var(--admin-primary-soft);
    color: var(--admin-primary-hover);
    font-weight: 600;
    padding: 0.35em 0.65em;
  }
  .badge-rank-first {
    background-color: var(--admin-success-soft);
    color: var(--admin-success);
    font-weight: 700;
  }

  table thead th {
    color: var(--admin-text-muted);
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    border-bottom: 2px solid var(--admin-border);
    font-weight: 600;
  }
</style>
```

ナビゲーションバー（`<nav>`）の`class`から`bg-body-tertiary`を除去する（上記`.navbar`ルールで背景を制御するため）。それ以外の構造（ブランドリンク・ナビリンク・ログアウトフォーム）は変更しない。

### templates/auth/login.html

`<head>`内、Bootstrap CSSの`<link>`の直後に以下のサブセット`<style>`ブロックを追加する:

```html
<style>
  :root {
    --admin-primary: #B54B3A;
    --admin-primary-hover: #973C2E;
    --admin-bg: #F4F5F7;
    --admin-text: #1F2328;
    --admin-border: #E2E4E9;

    --bs-primary: var(--admin-primary);
    --bs-primary-rgb: 181, 75, 58;
    --bs-body-bg: var(--admin-bg);
    --bs-body-color: var(--admin-text);
    --bs-border-color: var(--admin-border);
    --bs-border-radius: 8px;
    --bs-body-font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "Hiragino Kaku Gothic ProN", "Hiragino Sans", Meiryo, sans-serif;
  }
  .btn-primary {
    background-color: var(--admin-primary);
    border-color: var(--admin-primary);
  }
  .btn-primary:hover,
  .btn-primary:active {
    background-color: var(--admin-primary-hover);
    border-color: var(--admin-primary-hover);
  }
</style>
```

`<body class="bg-light">`から`bg-light`クラスを除去する（`bg-light`はBootstrapのユーティリティクラスで、`--bs-body-bg`の上書きより優先して既定の背景色を適用してしまうため）。`<body>`のみとする。

### templates/admin/dashboard.html

```html
{% extends "admin/_base.html" %}

{% block title %}ダッシュボード{% endblock %}

{% block content %}
<div class="page-header">
  <h1>ダッシュボード</h1>
  <p class="page-subtitle">イベントの設定状況と進捗を確認できます</p>
</div>

<div class="row g-3">
  <div class="col-12 col-md-4">
    <div class="stat-card">
      <div class="stat-label">部屋登録</div>
      <div class="stat-value">{{ room_count }} / 15</div>
      <div class="progress mb-2" style="height: 6px;">
        <div class="progress-bar" role="progressbar" style="width: {{ room_count * 100 / 15 }}%;"></div>
      </div>
      <a class="stat-link" href="/admin/rooms">部屋管理へ</a>
    </div>
  </div>
  <div class="col-12 col-md-4">
    <div class="stat-card">
      <div class="stat-label">イベント設定</div>
      <div class="mb-3">
        <span class="badge badge-mode">{% if is_team_mode %}チーム戦{% else %}個人戦{% endif %}</span>
        <span class="badge badge-mode">{% if require_answer_check %}QR＋正解入力{% else %}QR読み取りのみ{% endif %}</span>
      </div>
      <a class="stat-link" href="/admin/settings">設定を編集</a>
    </div>
  </div>
  <div class="col-12 col-md-4">
    <div class="stat-card">
      <div class="stat-label">ランキング</div>
      <p class="mb-3">クリアタイムのランキングを確認できます</p>
      <a class="stat-link" href="/admin/ranking">ランキングを見る</a>
    </div>
  </div>
</div>
{% endblock %}
```

`room_count * 100 / 15`は`usize`同士の整数演算（`DashboardTemplate.room_count`は`usize`）であり、ハンドラー側への変更は不要。15部屋を超える運用は要件上想定していない（[docs/requirements.md 4節](../docs/requirements.md#4-非機能要件)）ため、100%を超えるケースは考慮不要。

### templates/admin/rooms/list.html

`<h1 class="h3">部屋管理</h1>`を含む既存の`d-flex`見出し行を、他画面と統一した`page-header`パターンに合わせる（「新規登録」ボタンは同じ行に残してよい）:

```html
<div class="d-flex justify-content-between align-items-start mb-3">
  <div class="page-header mb-0">
    <h1>部屋管理</h1>
    <p class="page-subtitle">登録済み {{ rooms.len() }} / 15部屋</p>
  </div>
  <a class="btn btn-primary" href="/admin/rooms/add">新規登録</a>
</div>
```

テーブルの`class`から`table-striped`を除去し、`table align-middle`のみとする（縞模様ではなく罫線ベースのスタイルに統一するため。`thead th`のスタイルは`_base.html`側の共通CSSで適用される）。それ以外のテーブル構造・QR画像リンク・編集/削除ボタンは変更しない。

### templates/admin/rooms/add.html / templates/admin/rooms/edit.html

見出しを`page-header`パターンに合わせる:

```html
<div class="page-header">
  <h1>部屋登録</h1>
</div>
```

（edit.htmlは「部屋編集」に読み替え）

フォーム末尾の「戻る」リンクのクラスを`btn btn-link`から`btn btn-outline-secondary`に変更する。フォームのフィールド構成・`name`属性・`required`・CSRFトークン・`enctype`は変更しない。

### templates/admin/settings.html

見出しを`page-header`パターンに合わせる:

```html
<div class="page-header">
  <h1>イベント設定</h1>
</div>
```

チェックボックス・`name`属性・CSRFトークンは変更しない。

### templates/admin/ranking.html

見出しを`page-header`パターンに合わせ、クリア済みテーブルの1位（`entry.rank == 1`）の行の順位セルに`badge-rank-first`バッジを表示する:

```html
<div class="page-header">
  <h1>ランキング</h1>
</div>

<section class="mb-5">
  <h2 class="h5 mb-3">クリア済み</h2>
  {% if ranking.ranked.is_empty() %}
    <p>クリア済みの参加者はいません</p>
  {% else %}
    <table class="table align-middle">
      <thead>
        <tr>
          <th scope="col">順位</th>
          <th scope="col">参加者名</th>
          <th scope="col">所要時間</th>
        </tr>
      </thead>
      <tbody>
        {% for entry in ranking.ranked %}
          <tr>
            <td>
              {% if entry.rank == 1 %}
                <span class="badge badge-rank-first">{{ entry.rank }}位</span>
              {% else %}
                {{ entry.rank }}位
              {% endif %}
            </td>
            <td>{{ entry.player_name }}</td>
            <td>{{ entry.elapsed_display }}</td>
          </tr>
        {% endfor %}
      </tbody>
    </table>
  {% endif %}
</section>
```

`table-striped`は他テーブルと同様に除去する。「未クリア（圏外）」セクションの構造は変更しない。

## 制約・注意事項

- ハンドラー（`src/handlers/*.rs`）・サービス層・DBクエリ・ルーティングは一切変更しないこと。今回の対象はAskamaテンプレート（`templates/`配下）のマークアップとインラインCSSのみ
- `_base.html`・`rooms/add.html`のログアウト/CSRF関連の既存マークアップ（`action="/auth/logout"`・`name="csrf_token" value="{{ csrf_token }}"`）の属性名・構造は変更しないこと（既存テスト`room_templates_include_logout_csrf_token`が依存しているため）
- 新規の外部リソース（Webフォント・アイコンライブラリ等のCDN）を追加しないこと。Bootstrap本体のCDN読み込み（既存の`jsdelivr`リンク）はそのまま維持する
- `templates/liff/checkin.html`（プレイヤー向けLIFFページ）は対象外。変更しないこと
- カラーコード・CSS変数名は本指示書記載のものと完全に一致させること（`docs/architecture.md`20節の記載と一致させるため）

## 完了条件

- [ ] 上記実装対象ファイルすべてに指示どおりのスタイル・マークアップ変更を適用した
- [ ] `cargo test`が全体で通ることを確認した（既存テスト`room_templates_include_logout_csrf_token`を含む）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] `cargo run`でローカル起動し、対象7画面（`/auth/login`、`/admin/dashboard`、`/admin/rooms`、`/admin/rooms/add`、`/admin/rooms/edit/{id}`、`/admin/settings`、`/admin/ranking`）が崩れなく表示されることを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
