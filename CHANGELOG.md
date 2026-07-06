# Changelog

このプロジェクトの変更履歴。[Keep a Changelog](https://keepachangelog.com/) の形式を参考にする。

## [Unreleased]

### Added
- 部屋（チェックポイント）管理機能を追加
  - `/admin/rooms` 系の一覧・新規登録・編集・削除ハンドラー（最大15部屋、`require_admin` 保護）
  - 画像アップロード（マジックバイト検証・5MBサイズ上限・寸法上限・800px幅/JPEG品質80へのリサイズ）
  - 部屋ごとのQRコード（`qr_uuid`）をその場でPNG生成する `GET /admin/rooms/{id}/qr`
  - 判定モード（`require_answer_check`）に応じた `answer` / `hint_msg` の必須・NULL強制バリデーション
  - 画像更新時は新画像の保存に成功してから旧画像を削除する順序とし、失敗時の孤立参照を防止
- 管理者認証機能（ログイン・ログアウト）を追加（#3）
  - Cookieベースのセッション（`tower-sessions` / `MemoryStore`、非アクティブ12時間で失効）
  - セッションに保存したトークンとフォーム隠しフィールドを突き合わせるCSRF対策（ダブルサブミット方式）
  - ログイン成功時のセッションIDローテーション（session fixation対策）
  - アプリ起動時、`events` が空であれば `ADMIN_PASSWORD` をArgon2ハッシュ化して初期シード
  - `/admin/*` と `POST /auth/logout` を保護する `require_admin` ミドルウェア、`GET /admin/dashboard` の保護動作確認用プレースホルダー
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
