# StampRallyBot（仮称）

LINE Bot を使った建物内スタンプラリーアプリ。1建物・最大15部屋、1イベント運用を想定。
参加者はLINE Botの指示でランダムに割り当てられる部屋を巡り、各部屋担当スタッフが提示するQRコードをLIFFでスキャンしてスタンプ（チェックイン）を集める。

## ドキュメント

| ドキュメント | 内容 |
|:--|:--|
| [docs/requirements.md](docs/requirements.md) | 要件定義 |
| [docs/architecture.md](docs/architecture.md) | アーキテクチャ・設計 |
| [docs/database.md](docs/database.md) | テーブル設計 |
| [docs/api.md](docs/api.md) | エンドポイント設計 |
| [docs/operator-guide.md](docs/operator-guide.md) | 運営マニュアル |
| [CLAUDE.md](CLAUDE.md) | Claude（PM）の役割・ワークフロー |
| [AGENTS.md](AGENTS.md) | Codex の役割・コーディング規約 |
| [SECURITY.md](SECURITY.md) | セキュリティポリシー |
| [CHANGELOG.md](CHANGELOG.md) | 変更履歴 |

## 技術スタック

Rust（Axum + Tokio） / sqlx（MySQL） / Askama + Bootstrap 5 / Argon2 / LINE Messaging API / LIFF

詳細は [docs/architecture.md](docs/architecture.md) を参照。

## 開発環境のセットアップ

このプロジェクトは Dev Container 構成済みです。

1. VS Code でこのフォルダを開く
2. 「Reopen in Container」を実行（`.devcontainer/` の設定に従って Rust + MySQL のコンテナが起動する）
3. `.env.example` を `.env` にコピーし、値を設定する（`.env` が無い場合はコンテナ作成時に自動でコピーされるが、値は自分で埋める必要がある）
   ```bash
   cp .env.example .env
   ```
4. コンテナ内で以下を実行
   ```bash
   cargo run
   ```

ローカルDBは `.devcontainer/compose.yaml` の `db` サービス（MySQL 8.0）が自動起動します。

## 本番デプロイ（Koyeb）

デプロイ構成の設計判断（ビルド方式・DB・環境変数・セッション等）は [docs/architecture.md 18節](docs/architecture.md#18-デプロイ構成koyeb) を参照。ここでは実際にデプロイする際の作業手順のみをまとめる。

### 1. 外部サービスの準備（初回のみ）

1. **LINE Developers コンソール**でMessaging APIチャネルを作成し、`LINE_CHANNEL_SECRET` / `LINE_CHANNEL_ACCESS_TOKEN` を発行する
2. 同コンソールでLIFFアプリを追加し、`LIFF_ID` を発行する（エンドポイントURLは手順3でKoyebのURLが確定してから設定する）
3. LIFFアプリが属するチャネルのIDを `LINE_LOGIN_CHANNEL_ID` として控える
4. 既存のTiDB Cloudアカウント・クラスタ内に、このアプリ専用のデータベース（`stamprally`）と専用DBユーザーを作成する（既存の別アプリのデータベースとは分離する。詳細は [SECURITY.md](SECURITY.md)「本番DB（TiDB Serverless）の接続方針」）。発行された接続文字列を控える（`DATABASE_URL`に設定する値）

### 2. Koyebでの初回セットアップ

1. Koyebで新規Webサービスを作成し、このリポジトリを接続する。ビルド方式は**Dockerfile**を選択する（リポジトリルートの本番用`Dockerfile`を使用。`.devcontainer/Dockerfile`とは別物）
2. ポート設定を、アプリがリッスンするポート（`PORT`環境変数を読む実装。デフォルト8000）と一致させる
3. Health Check Pathに `/health` を設定する
4. インスタンス数が1（オートスケールしない構成）になっていることを確認する（セッションをプロセス内メモリで保持する設計のため）
5. Environment Variables に、上記手順1で控えた値と `PUBLIC_BASE_URL`（Koyebが割り当てたURL）・`ADMIN_PASSWORD`（初期管理者パスワード）を設定する
6. `DATABASE_URL` に手元から `sqlx migrate run` でマイグレーション適用済みのTiDB Serverless接続文字列を設定する（マイグレーションの適用方法は次節）
7. デプロイを実行し、`https://<Koyebドメイン>/health` が200を返すことを確認する
8. LINE DevelopersコンソールのWebhook URLを `https://<Koyebドメイン>/callback` に、LIFFのエンドポイントURLを `https://<Koyebドメイン>/liff/checkin` に設定する

### 3. スキーマ変更を含むリリース時の手順

本番DBへの自動マイグレーションは行わない。スキーマ変更（`migrations/`への追加）を含むリリースでは、コードをデプロイする前に手元で以下を実行する。

```bash
DATABASE_URL=<本番のTiDB Serverless接続文字列> sqlx migrate run
```

適用結果を確認してから、通常どおりコードをKoyebにデプロイする。
