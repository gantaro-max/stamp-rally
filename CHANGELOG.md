# Changelog

このプロジェクトの変更履歴。[Keep a Changelog](https://keepachangelog.com/) の形式を参考にする。

## [Unreleased]

### Added
- プロジェクトの土台となる設計ドキュメント一式（要件定義・アーキテクチャ・DB設計・API設計・運営マニュアル）を `docs/` 配下に作成
- Claude（PM/設計）とCodex（実装）の役割分担、TDD（Red-Green-Refactor）運用、`feature/*` ブランチ＋PRによるブランチ運用ポリシーを `CLAUDE.md` / `AGENTS.md` に整備
- devcontainer構成（Rust + MySQL 8.0）を追加
- Rustプロジェクトの初期セットアップ（#2）
  - Axum, sqlx(MySQL), Askama, Argon2, image, qrcode, reqwest, tower-sessions などの依存クレートを追加
  - `GET /health` エンドポイントを追加（疎通確認用、TDDで実装）
  - 初期DBマイグレーション（`events`, `rooms`, `players`, `visited_rooms`, `room_images`）を追加
  - `handlers` / `services` / `repository` の初期モジュール構成を追加

### Changed
- devcontainerのシークレット管理を `.env` 経由の `env_file` 方式に変更し、MySQLヘルスチェック・ホストポート設定を堅牢化（#1）
- 設計ドキュメントを `docs/` フォルダにまとめて再編し、相互参照リンクを整理

### Security
- シークレット（LINEチャネル情報・DB接続情報・管理者パスワード等）はすべて環境変数（`.env`、gitignore対象）経由で注入する方針を明文化し、devcontainerの設定からハードコードを排除
