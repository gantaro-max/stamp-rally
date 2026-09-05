# StampRallyBot

**LINE Bot で遊ぶ、建物内スタンプラリーアプリ。**
参加者は LINE から届く指示に従って建物内の部屋を巡り、各部屋のスタッフが提示する QR コードを LIFF でスキャンしてスタンプを集める。全部屋を回るとクリアタイムが記録され、ランキングに載る。

Rust（Axum）製のフルスタック Web アプリケーション。LINE Messaging API・LIFF 連携、管理画面、動的な画像生成までを含み、Koyeb 上で実際に稼働させることを前提に設計・実装した個人プロジェクト。

> **このリポジトリの見どころ**
> 成果物そのものに加えて、**AI エージェントに役割を分担させた TDD 開発プロセス**を丸ごと記録している点にあります。
> 設計書・実装指示書・PR・コミット履歴がすべて残っており、[開発プロセス](#開発プロセスai-エージェント分業による-tdd)の節から辿れます。

---

## 目次

- [解決したい課題](#解決したい課題)
- [主な機能](#主な機能)
- [技術スタック](#技術スタック)
- [アーキテクチャ](#アーキテクチャ)
- [設計上の判断](#設計上の判断)
- [開発プロセス：AI エージェント分業による TDD](#開発プロセスai-エージェント分業による-tdd)
- [セキュリティ](#セキュリティ)
- [ローカルでの起動](#ローカルでの起動)
- [ドキュメント](#ドキュメント)
- [ライセンス](#ライセンス)

---

## 解決したい課題

建物内で複数の催し（セッション・展示など）が同時開催されるイベントで、来場者に各部屋を回遊してもらうための仕掛けが欲しい。しかし——

- 紙のスタンプ台紙は印刷・配布・集計のコストが高い
- 専用アプリのインストールは、単発イベントの参加者にとってハードルが高すぎる
- 運営スタッフは本来の催しの進行で手一杯であり、複雑な操作は覚えられない

そこで、**参加者は「LINE 公式アカウントを友だち追加するだけ」**、**運営スタッフは「QR コードを提示するだけ」**で成立するスタンプラリーを構築した。集計・ランキングはシステムが自動で行う。

---

## 主な機能

### 参加者（LINE Bot / LIFF）

| 機能 | 内容 |
|:--|:--|
| 参加登録 | 「開始」と送るだけで参加登録。個人戦は個人名、チーム戦はチーム名を入力 |
| 部屋の案内 | 未訪問の部屋からランダムに 1 部屋を選出し、クエスト文・画像を Flex Message で通知 |
| QR チェックイン | Flex Message のボタンから LIFF を起動 → `liff.scanCodeV2()` で QR をスキャン → サーバ側で検証してスタンプ付与 |
| スタンプカード | 訪問済みの部屋に応じたスタンプカード画像を**サーバ側で動的生成**して返信（後述） |
| クリア演出 | 最終部屋の QR を読んだ時点でクリアタイムを記録し、全スタンプが揃ったカード画像とともに Flex Message で演出 |
| その他コマンド | 「ヒント」「スタンプ状況」「遊び方」「リセット」 |

### 管理者（Web 管理画面）

| 機能 | 内容 |
|:--|:--|
| 部屋管理 | 部屋（チェックポイント）の CRUD。部屋名・クエスト文・画像・スタンプ表示名・スタンプ画像を登録（最大 15 部屋） |
| QR コード発行 | 部屋ごとの QR コードを動的生成して表示。スタッフはこれを印刷 or 端末表示して持ち歩く |
| イベント設定 | 個人戦 / チーム戦の切替、判定モード（「QR のみ」⇔「QR ＋ LINE 上での正解入力」）の切替 |
| デザインカスタマイズ | スタンプカードの台紙画像・部屋ごとのスタンプ画像を差し替えて、イベント独自のデザインにできる |
| ランキング | クリアタイム順のリアルタイムランキング |
| ダッシュボード | 参加者数・進捗の把握、LINE 公式アカウントの友だち追加 QR の表示 |

### スタンプカードの動的生成

参加者の進捗に応じたスタンプカード PNG を、リクエストのたびに `image` + `imageproc` + `ab_glyph` でその場で描画している。事前生成した画像を保存する方式ではないため、部屋の追加・デザイン変更が即座に反映される。

- 未訪問の部屋は空欄、訪問済みの部屋には「はんこ風」のスタンプを描画
- 部屋ごとにスタンプ画像がアップロードされていればそれを使用し、未設定なら**スタンプ表示名から自動でデザインを生成**
- 台紙画像が設定されていれば、アプリ側の既定の飾り枠・タイトル描画を抑制して重ならないようにする
- 日本語表示のために Noto Sans JP を同梱（[OFL](assets/fonts/OFL.txt)）

---

## 技術スタック

| 領域 | 採用技術 |
|:--|:--|
| 言語 | Rust（edition 2024） |
| Web フレームワーク | Axum 0.8 + Tokio + Tower |
| DB アクセス | sqlx 0.8（非同期・生 SQL・コンパイル時クエリ検証） |
| DB | MySQL 8.0（ローカル） / TiDB Serverless（本番） |
| テンプレート | Askama + Bootstrap 5 |
| 外部 API | LINE Messaging API（`reqwest` による自前クライアント） |
| フロント連携 | LIFF（`liff.scanCodeV2()` / `liff.getIDToken()`） |
| 認証 | Argon2 + `tower-sessions` によるセッション |
| 画像処理 | `image` / `imageproc` / `ab_glyph` |
| QR 生成 | `qrcode` |
| 開発環境 | Dev Container（Rust + MySQL） |
| 本番環境 | Koyeb（マルチステージ Dockerfile） |

外部 SDK に頼らず、LINE Messaging API クライアント・Webhook 署名検証・Flex Message の組み立て・LIFF ID トークン検証をすべて自前で実装している。

---

## アーキテクチャ

```
                    ┌─────────────┐
   LINE アプリ ───▶ │  /callback  │  Webhook（HMAC-SHA256 署名検証）
                    ├─────────────┤
   LIFF (WebView)──▶│ /liff/checkin│  QR スキャン結果（ID トークン検証）
                    ├─────────────┤
   ブラウザ（運営）─▶│   /admin/*  │  管理画面（セッション認証 + CSRF）
                    ├─────────────┤
   LINE サーバ ────▶│  /public/*  │  画像・スタンプカード配信（認証不要）
                    └──────┬──────┘
                           │
   Handler(Axum) ─▶ Service ─▶ Repository(sqlx) ─▶ MySQL / TiDB
                           │
                           └─▶ LineClient ─▶ LINE Messaging API
```

**レイヤー責務**

| 層 | 責務 |
|:--|:--|
| Handler | HTTP の入出力、認証・CSRF・署名検証、テンプレートレンダリング |
| Service | ドメインロジック（`game_service` / `room_service` / `event_service` / `ranking_service` / `stamp_card_service` / `image_service` / `auth_service` / `csrf_service` / `qr_service`） |
| Repository | sqlx による生 SQL のデータアクセス |
| LineClient | LINE Messaging API 呼び出しと Flex Message 組み立て |

**データモデル**（6 テーブル）

`events` / `rooms` / `players` / `visited_rooms` / `room_images` / `pending_registrations`

詳細は [docs/architecture.md](docs/architecture.md) / [docs/database.md](docs/database.md) / [docs/api.md](docs/api.md) を参照。

---

## 設計上の判断

実装中・本番運用中に直面した問題と、それに対する設計判断を抜粋する。**判断の経緯はすべて [docs/architecture.md](docs/architecture.md) に理由付きで記録している。**

<details>
<summary><b>Webhook 署名検証は「生バイト列」に対して行う</b></summary>

LINE の署名はリクエストボディの生バイト列に対する HMAC-SHA256 である。JSON にデシリアライズしてから再シリアライズした結果は、キー順や空白の扱いによって元のバイト列と一致する保証がない。そのため `axum::body::Bytes` で生ボディを受け取り、**JSON パースより前に**署名検証を行う。検証ロジックは DB・ネットワークに依存しない純粋関数として切り出し、単体テスト可能にしている。
（[architecture.md 8節](docs/architecture.md#8-line-webhookcallbackの受信と署名検証)）
</details>

<details>
<summary><b>Webhook のレスポンスは即返し、実処理はバックグラウンドへ</b></summary>

本番運用中、LINE プラットフォーム側の Webhook タイムアウトに引っかかり `request_timeout` として配信失敗になる事象が実際に発生した。参加者から見ると「Bot が無反応」になる。

対策として、署名検証と JSON パースが終わった時点で 200 を返し、実処理は `tokio::spawn` でバックグラウンドに逃がした。ただし**イベントごとに spawn してはならない**——同一ペイロードに複数イベントが含まれる場合（参加者が短時間に連続送信した際に LINE 側でまとめられる）、本来配列順に逐次処理されるべきものが並行実行され、データ不整合を起こす。ペイロード全体を 1 つのタスクにまとめることで逐次性を保っている。**これは最終レビューのサブエージェントが発見した見落としである。**
（[architecture.md 21節](docs/architecture.md#21-db接続外部api呼び出しのタイムアウト)）
</details>

<details>
<summary><b>クライアントの申告を一切信用しない</b></summary>

- **LIFF チェックイン**：クライアントが送ってきた LINE ユーザー ID は信用せず、`liff.getIDToken()` の ID トークンを LINE の検証エンドポイントに問い合わせ、署名・有効期限を確認したうえで、そこに含まれる `sub` をユーザー ID として採用する
- **チェックイン判定**：QR の UUID が有効か → プレイヤーの `current_room_id` と一致するか（案内された部屋以外は無効）→ 判定モードに応じて正解済みか、をサーバ側で順に検証する
- **フォーム入力**：判定モードが「QR のみ」のイベントでは、仮に正解・ヒントが POST されても保存せず常に NULL とする

（[architecture.md 5節・15節](docs/architecture.md#5-qrコードの仕組み)）
</details>

<details>
<summary><b>画像アップロードは拡張子を見ない</b></summary>

拡張子ではなく `image::guess_format` によるマジックバイト判定で実フォーマットを確定し、JPEG / PNG のみを許可する。加えて、**デコード前に**寸法上限（4096px・1600 万画素）で弾くことで decompression bomb によるメモリ枯渇を防ぐ。通過した画像のみ 800px 幅・JPEG 80% にリサイズして保存し、出力フォーマットを統一している。
（[architecture.md 6節](docs/architecture.md#6-画像配信の仕組み)）
</details>

<details>
<summary><b>画像差し替えは「挿入 → 張り替え → 削除」の順に行う</b></summary>

部屋の画像を更新する際、先に古い画像を削除すると、新しい画像の保存に失敗した時点で `rooms.image_id` が存在しない行を指す状態が残る。新しい画像を挿入して参照を張り替えてから、旧行を削除する順序を守ることで、失敗しても古い画像が残る安全側に倒している。
（[architecture.md 7節](docs/architecture.md#7-部屋チェックポイント管理の実装方針)）
</details>

<details>
<summary><b>無料枠のスケール to ゼロを前提に状態を持つ</b></summary>

Koyeb 無料枠は最小インスタンス数を 0 に固定できず、アイドル時にプロセスが落ちうる。そのため「開始」〜名前入力までの参加登録の一時状態を、プロセス内メモリではなく `pending_registrations` テーブルに永続化し、再起動をまたいでも会話が壊れないようにした。一方でセッションはプロセス内保持のままとし、インスタンス数 1 固定の運用でカバーする（管理者の再ログインで済む範囲のため）。
（[architecture.md 9節・18節](docs/architecture.md#9-会話状態管理参加登録の一時状態)）
</details>

<details>
<summary><b>外部依存にはすべてタイムアウトを張る</b></summary>

TiDB Serverless・LINE Messaging API のいずれも、ネットワーク層で応答が返らなくなる（コネクション切断を検知できない）事象が起こりうる。DB アクセス・外部 API 呼び出しにタイムアウトを設け、1 件のリクエスト処理が無期限にハングして参加者への応答が失われる事態を防いでいる。
（[architecture.md 21節](docs/architecture.md#21-db接続外部api呼び出しのタイムアウト)）
</details>

---

## 開発プロセス：AI エージェント分業による TDD

このプロジェクトは、**役割を明確に分けた 2 つの AI エージェントと人間の 3 者体制**で開発した。プロセス自体を検証可能な形で残すことを目的の一つとしている。

| 役割 | 担当 | 成果物 |
|:--|:--|:--|
| **要求・意思決定** | 人間 | 何を作るか、どこまでやるかの判断 |
| **PM / 設計 / レビュー** | Claude | 要件定義・基本設計・実装指示書の作成、最終レビュー、ドキュメント更新 |
| **実装** | Codex | `feature/*` ブランチ上での TDD 実装と PR 作成 |

**Claude は一切コードを書かない。** 修正が必要と判断した場合も、自分でファイルを書き換えず Codex への指示書を起こす。この制約により、設計意図が必ず文書として残る。

### ワークフロー

```
要望 → 要件定義の更新 → 基本設計（architecture / api / database）
     → 実装指示書の作成（テストケースを含む詳細設計）
     → Codex が TDD で実装し PR 作成
     → 複数サブエージェントによる並列最終レビュー
     → ドキュメント更新 → Squash merge
```

### 検証できること

このリポジトリでは、上記が「そう書いてあるだけ」ではないことをコミット履歴から確認できる。

**1. 設計 → 実装 → 記録のサイクルが 1 機能ごとに残っている**

```
$ git log --oneline
68e4dbe Document PR #32 (admin image preview) in CHANGELOG; archive instruction sheet
93440fe Feature/admin image preview (#32)
52c7530 Design admin image preview and add implementation instructions
```

「設計・指示書作成」→「実装 PR」→「ドキュメント反映」の 3 コミットが、機能ごとに繰り返されている。ワークフローを確立する前の最初期（#1・#2）を除き、すべての機能追加がこの形をとっている。

**2. Red-Green-Refactor が実際に守られている**

各 PR は Squash merge されているが、多くはコミットメッセージ本文に元の粒度が保存されている。

```
$ git log -1 --format=%b 292930e
* test: add failing stamp card image render tests
* feat: render custom stamp card images
* refactor: simplify stamp card handler data type
```

粒度が残っている 28 件の PR のうち、**26 件が `test:`（失敗するテスト）から始まっている。** 実装を正当化するための後付けテストではないことが履歴から確認できる。

残りの内訳も記しておく。`test:` から始まらない 2 件は、TDD 運用を確立する前の初期セットアップ（#2）と、振る舞いを持たない小規模な堅牢化（#11）。ほかに 6 件は Squash 時に単一コミットへまとめられており、`main` の履歴からは粒度を判定できない。これらの多くは [AGENTS.md](AGENTS.md) の「振る舞いを持たない変更は TDD サイクルの対象外でよい」という規定に該当する変更である。

**3. 実装指示書が 38 本残っている**

[instructions/done/](instructions/done/) に、機能ごとの実装指示書（背景・目的、対象ファイル、テストケース一覧、実装仕様、制約、完了条件）が全て残っている。実装前にどこまで設計が固まっていたかを直接読める。

**4. 最終レビューは 5 観点の並列サブエージェント**

Codex の一次レビューを経たコードに対し、以下の観点でサブエージェントを並列起動して最終レビューを行う。

| 観点 | 確認内容 |
|:--|:--|
| 設計整合性 | 基本設計との乖離がないか |
| セキュリティ | `SECURITY.md` の対策が反映されているか、新たな脆弱性がないか |
| 要件充足 | `docs/requirements.md` の要件を満たしているか |
| 実装指示書 | 完了条件を満たしているか、指示外の変更が混入していないか |
| TDD 遵守 | 指示書のテストケースに対応するテストが存在し、履歴から Red-Green-Refactor が確認できるか |

前掲の「Webhook のイベント単位 spawn による順序問題」は、このレビューで検出して差し戻した実例である。

### 規模

| 指標 | 値 |
|:--|:--|
| Rust コード | 約 10,200 行 |
| テスト | 212 件（うち DB 結合テスト 128 件） |
| マージ済み PR | 33 |
| 実装指示書 | 38 本 |
| コミット | 120 |

役割定義の詳細は [CLAUDE.md](CLAUDE.md)（PM 側）と [AGENTS.md](AGENTS.md)（実装側のコーディング規約・TDD 手順）に記述している。

---

## セキュリティ

| 対策 | 内容 |
|:--|:--|
| パスワード | Argon2 でハッシュ化。平文保存なし |
| Webhook | `x-line-signature` の HMAC-SHA256 検証。失敗時は 401 で即座に処理を打ち切る |
| LIFF | ID トークンを LINE の検証エンドポイントで検証し、`sub` をユーザー ID として採用 |
| CSRF | セッション格納トークンとのダブルサブミット。`/callback` と `/liff/checkin` は仕様上の対象外 |
| 認可 | 管理画面は `require_admin` ミドルウェアで一括保護 |
| IDOR | チェックインは `current_room_id` との一致をサーバ側で検証 |
| アップロード | マジックバイト判定・サイズ上限・寸法上限（decompression bomb 対策） |
| シークレット | すべて環境変数で注入。`.env` はコミットしない |

方針の全体像は [SECURITY.md](SECURITY.md) を参照。

なお QR コードの不正利用（撮影した QR の使い回し）は、システムでは対策していない。QR は部屋担当スタッフが目視確認のうえで提示する運用のため、システム側の対策は費用対効果に見合わないと判断した——という**やらないことの判断も明示的に記録している**（[docs/requirements.md](docs/requirements.md)）。

---

## ローカルでの起動

Dev Container 構成済み。Docker と VS Code があれば動く。

```bash
# 1. VS Code でこのフォルダを開き「Reopen in Container」を実行
#    （.devcontainer/ の設定で Rust + MySQL 8.0 のコンテナが起動する）

# 2. 環境変数を用意（コンテナ作成時に自動コピーされるが、値は自分で埋める）
cp .env.example .env

# 3. 起動
cargo run
```

管理画面は `http://localhost:8099/auth/login`。ログインパスワードは `.env.example` に設定済みのローカル開発専用値を使用する。

```bash
cargo test      # テスト（212 件）
cargo clippy    # Lint
```

> **DB 結合テストについて**
> 212 件のうち 128 件は `#[sqlx::test]` による DB 結合テストで、テストごとに使い捨てのデータベース（`_sqlx_test_*`）を作成する。devcontainer を使う場合は `postCreateCommand` が必要な権限を自動で付与するため、`cargo test` をそのまま実行できる。devcontainer を使わず自前で DB を用意する場合は、`DATABASE_URL` のユーザーに `_sqlx_test_*` という名前のデータベースを作成・削除できる権限が必要となる。
>
> この変更より前から同じ devcontainer を使い続けている場合は、MySQL クライアントと `postCreateCommand` の変更を反映するためにコンテナをリビルドすること。

LINE Bot 部分を実際に動かすには、LINE 公式アカウント（Messaging API チャネル）と LIFF アプリの登録、および公開 URL が必要になる。チャネル発行から Koyeb へのデプロイまでの手順は [docs/deployment.md](docs/deployment.md) にまとめている。

---

## ドキュメント

| ドキュメント | 内容 |
|:--|:--|
| [docs/requirements.md](docs/requirements.md) | 要件定義（機能要件・非機能要件・スコープ外の判断） |
| [docs/architecture.md](docs/architecture.md) | アーキテクチャと設計判断の記録 |
| [docs/database.md](docs/database.md) | テーブル設計 |
| [docs/api.md](docs/api.md) | エンドポイント設計 |
| [docs/operator-guide.md](docs/operator-guide.md) | 運営スタッフ向けマニュアル |
| [docs/deployment.md](docs/deployment.md) | LINE チャネル発行・Koyeb デプロイ手順 |
| [SECURITY.md](SECURITY.md) | セキュリティポリシー |
| [CHANGELOG.md](CHANGELOG.md) | 変更履歴 |
| [CLAUDE.md](CLAUDE.md) | PM エージェントの役割定義とワークフロー |
| [AGENTS.md](AGENTS.md) | 実装エージェントのコーディング規約・TDD 手順 |
| [instructions/done/](instructions/done/) | 全 38 本の実装指示書 |

---

## ライセンス

[MIT License](LICENSE)

同梱フォント Noto Sans JP は SIL Open Font License 1.1（[assets/fonts/OFL.txt](assets/fonts/OFL.txt)）に基づく。

---

## Overview (English)

A stamp rally (checkpoint tour) application for indoor events, built as a LINE Bot in Rust.

Participants add the event's LINE Official Account as a friend, receive randomly assigned rooms as Flex Messages, and collect stamps by scanning staff-held QR codes through LIFF. Stamp card images are rendered server-side on every request. Organizers manage checkpoints, QR codes, event settings and a live clear-time ranking through an Askama + Bootstrap admin panel.

Stack: Rust (edition 2024), Axum, Tokio, sqlx (MySQL / TiDB Serverless), Askama, Argon2, LINE Messaging API and LIFF — all API integration written from scratch without an official SDK.

This repository also documents an **AI-agent-divided TDD workflow**: Claude acted as PM (requirements, architecture, implementation specs, multi-agent final review) and never wrote code, while Codex implemented every feature test-first on `feature/*` branches. Of the PRs whose squashed bodies preserve their original commit granularity, 26 of 28 begin with a failing-test commit, and all 38 implementation specs are preserved under [instructions/done/](instructions/done/).
