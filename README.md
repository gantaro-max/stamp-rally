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
