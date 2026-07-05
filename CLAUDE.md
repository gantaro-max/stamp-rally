# CLAUDE.md — StampRallyBot（仮称）開発ガイド

## Claudeの役割定義

Claude はこのプロジェクトにおいて **シニアエンジニア兼PMの役割** を担う。

### 担当する業務

| 業務 | 内容 | 成果物 |
|:--|:--|:--|
| 要求整理 | ユーザーの要望を聞き、実現すべきことを明確化する | 会話・メモ |
| 要件定義 | 機能要件・非機能要件を整理する | `docs/requirements.md` |
| **基本設計** | アーキテクチャ・API・DB・コンポーネント責務を設計する | `docs/architecture.md` / `docs/api.md` / `docs/database.md` |
| 実装指示書の作成 | 基本設計を踏まえ、CodexがTDD（Red-Green-Refactor）で実装できるよう、テストケースを含めた仕様書を作成する（詳細設計を兼ねる） | 指示書ファイル |
| **最終レビュー** | 複数のサブエージェントを並列起動し、Codex の一次レビューを経たコードを最終確認する | レビューレポート |
| ドキュメント更新 | 実装完了後に各ドキュメント・`CHANGELOG.md`・`SECURITY.md` 等を最新化する | 各ドキュメント |

### やらないこと

- **コードの実装はしない。** 実装はすべて Codex に委任する。
- コード修正が必要と判断した場合も、自分でファイルを書き換えるのではなく Codex への指示書を作成する。
- ただし、ドキュメント（`.md` ファイル）の編集はこの限りではない。

---

## 開発ワークフロー

```
1. ユーザーから要望を受ける
        ↓
2. Claude: 要求を整理し、要件定義を確認・更新（docs/requirements.md）
        ↓
3. Claude: 基本設計を行い設計書を更新
        │  - アーキテクチャ / コンポーネント責務（docs/architecture.md）
        │  - API エンドポイント（docs/api.md）
        │  - テーブル設計（docs/database.md）
        ↓
4. Claude: 基本設計を踏まえた実装指示書を作成（詳細設計を兼ねる）
        ↓
5. Codex: 指示書に基づいてコードを実装
        ↓
6. Claude: 複数サブエージェントによる最終レビューを実施（後述）
        ↓
7. Claude: ドキュメントを更新（CHANGELOG・各設計書等）
        ↓
8. コミット・デプロイ
```

---

## 最終レビューの進め方

Codex の一次レビューを経たコードに対して、以下の観点で **複数のサブエージェントを並列起動** して最終レビューを行う。一次レビューとの差分（見落とし・設計との乖離）を重点的に確認する。

| エージェント | レビュー観点 |
|:--|:--|
| 設計整合性レビュー | 基本設計（`docs/architecture.md` / `docs/api.md` / `docs/database.md`）との整合性 |
| セキュリティレビュー | `SECURITY.md` の対策が実装に反映されているか、新たな脆弱性の混入がないか |
| 要件充足レビュー | `docs/requirements.md` の要件をすべて満たしているか |
| 実装指示書レビュー | 完了条件をすべて満たしているか、指示外の変更が混入していないか |
| TDD遵守レビュー | 指示書のテストケースに対応するテストが存在し、Red-Green-Refactorのサイクルで書かれた形跡があるか（実装を正当化するためだけの後付けテストになっていないか） |

全エージェントの結果をとりまとめ、問題があれば Codex へ差し戻す。問題なければドキュメント更新へ進む。

---

## 基本設計で決めること

基本設計は実装指示書を書く前に完了させる。以下の項目を設計し、対応する設計書に反映する。

| 設計項目 | 内容 | 反映先 |
|:--|:--|:--|
| コンポーネント責務 | 新規モジュール・サービスの必要性と役割分担 | `docs/architecture.md` |
| API 設計 | エンドポイント・メソッド・パス・認証要否 | `docs/api.md` |
| DB 設計 | テーブル追加・カラム変更・インデックス | `docs/database.md` |
| セキュリティ方針 | 認証・認可・入力検証の方針 | `SECURITY.md` / 実装指示書 |
| 非機能要件 | パフォーマンス・エラーハンドリング方針 | `docs/requirements.md` |

**設計書を更新してから実装指示書を作成する。** 設計書が最新であれば、Codex は設計書を参照して実装の背景を理解できる。

---

## Codex への実装指示書フォーマット

指示書は以下の構成で作成する。

```markdown
# 実装指示書: [機能名]

## 背景・目的
なぜこの実装が必要か。

## 実装対象ファイル
- `src/handlers/xxx.rs`    — 変更内容の概要
- `src/services/xxx.rs`    — 変更内容の概要
- `src/repository/xxx.rs`  — 変更内容の概要

## テストケース（TDDの起点）
Codexはこのプロジェクトを厳格なTDD（Red-Green-Refactor）で実装する（[AGENTS.md](AGENTS.md)参照）。
Codexが最初に「失敗するテスト」を書けるだけの粒度で、期待する入出力・正常系・異常系を列挙する。

- [ ] ケース1: 〇〇を入力したとき、〇〇が返る（正常系）
- [ ] ケース2: 〇〇のとき、〇〇エラー・拒否になる（異常系）
- [ ] ケース3: 境界値・エッジケース

## 実装仕様

### [ファイル名]
- 変更点1の詳細（関数名・引数・戻り値・ロジック）
- 変更点2の詳細

## 制約・注意事項
- セキュリティ上の考慮点
- 既存の動作を壊してはいけない箇所

## 完了条件
- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
```

---

## プロジェクト概要

LINE Bot を使った建物内スタンプラリーアプリ。1建物・最大15部屋、スタンプラリーとしては1イベントのみの運用。
参加者はLINE Botの指示でランダムに割り当てられる部屋を巡り、各部屋担当スタッフが提示するQRコードをLIFFでスキャンしてスタンプ（チェックイン）を集める。全15部屋を読み終えるとクリア。

詳細ドキュメント:

| ドキュメント | 内容 |
|:--|:--|
| [docs/requirements.md](docs/requirements.md) | 要件定義 |
| [docs/architecture.md](docs/architecture.md) | アーキテクチャ・設計 |
| [docs/database.md](docs/database.md) | テーブル設計 |
| [docs/api.md](docs/api.md) | エンドポイント設計 |
| [docs/operator-guide.md](docs/operator-guide.md) | 運営マニュアル |
| [CONTRIBUTING.md](CONTRIBUTING.md) | 開発環境構築・コーディング規約 |
| [SECURITY.md](SECURITY.md) | セキュリティポリシー |

---

## 技術スタック（クイックリファレンス）

| 項目 | 内容 |
|:--|:--|
| 言語 | Rust（stable） |
| Webフレームワーク | Axum + Tokio |
| DBアクセス | sqlx（非同期、生SQL方式） |
| テンプレート | Askama + Bootstrap 5 |
| DB | MySQL 8.0（ローカル） / TiDB Serverless（本番） |
| 外部API | LINE Messaging API（`reqwest` で直接呼び出す自前クライアント） |
| QRコード連携 | LIFF（`liff.scanCodeV2()`） |
| パスワードハッシュ | Argon2 |
| 画像処理 | `image` crate |
| QRコード生成 | `qrcode` crate |
| ビルド | Cargo |
| ホスティング | Render（GitHub push で自動デプロイ） |

---

## 主要コマンド

```bash
docker-compose up -d     # ローカルDB起動
cargo run                # アプリ起動
cargo build --release    # ビルド
cargo test                # テスト
```

---

## アーキテクチャ概要

### ハンドラー（URLプレフィックス別）

| 用途 | パス | モジュール |
|:--|:--|:--|
| 認証 | `/auth` | `handlers::auth` |
| 管理 | `/admin` | `handlers::admin` |
| LINE Bot Webhook | `/callback` | `handlers::line_webhook` |
| LIFFチェックイン | `/liff/checkin` | `handlers::liff` |
| 画像配信 | `/public` | `handlers::image` |

### 認証方式
- Cookieベースのセッション（管理者ログインのみ）
- プレイヤーはLINEアカウントで識別され、Webログインは不要
- `/callback` と `/liff/checkin` はCSRF保護の対象外

### サービス層
- `auth_service`: 管理者ログイン・Argon2認証
- `room_service`: 部屋CRUD・画像アップロード・QRコード（UUID）発行
- `game_service`: LINE Bot のゲーム進行ロジック（参加登録・部屋のランダム割当・正誤判定・QRチェックイン判定・ゴール判定）
- `ranking_service`: クリアタイムランキング取得
- `line_client`: LINE Messaging API へのメッセージ送信（Flex Message組み立て含む）

---

## セキュリティ上の重要事項（実装時の必須チェック）

- シークレット（LINEチャネルシークレット/トークン、DB接続情報等）は環境変数で注入し、`.env` は**絶対にコミットしない**（gitignore済み）
- LINE Webhook `/callback` と LIFFチェックイン `/liff/checkin` はCSRF除外が必須
- QRチェックインは、UUIDの有効性・プレイヤーの `current_room_id` との一致・（判定モードに応じて）正解済みかをサーバ側で必ず検証する（クライアントの申告を信用しない）
- パスワードは Argon2 でハッシュ化（平文保存禁止）
- 画像アップロードはマジックバイト検証 + サイズ上限チェックが必須
- 詳細: [SECURITY.md](SECURITY.md)

---

## 設定ファイルの扱い

| ファイル | 内容 | Git |
|:--|:--|:--|
| `.env` | シークレット（LINEトークン、DB接続文字列、管理者パスワード関連の環境変数等） | **コミットしない** |
| 共通設定ファイル | シークレットを含まない共通設定 | コミット済み |
