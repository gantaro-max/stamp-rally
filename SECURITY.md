# セキュリティポリシー — StampRallyBot（仮称）

このドキュメントは実装・運用上のセキュリティ方針をまとめたもの。実装レベルの詳細ルールは [CLAUDE.md](CLAUDE.md) / [AGENTS.md](AGENTS.md)、非機能要件は [docs/requirements.md](docs/requirements.md) を参照。

## 認証・認可

- 管理者はCookieベースのセッション認証でログインする
- パスワードは Argon2 でハッシュ化し、平文では保存しない
- プレイヤーはLINEアカウントのみで識別し、Webログインは行わない
- 管理画面（`/admin/*`）はすべてセッション認証ミドルウェアを通す

## CSRF対策

- すべてのPOSTフォームにCSRFトークンを付与する
- 例外: LINE Webhook `/callback` とLIFFチェックイン `/liff/checkin` はLINE/LIFFからの直接呼び出しのため、CSRF保護の対象外とする

## LINE Webhookの署名検証

- `/callback` は `x-line-signature` ヘッダー（チャネルシークレットによるリクエストボディのHMAC-SHA256をBase64エンコードした値）を検証してから処理する
- ヘッダー欠如・検証失敗はいずれも `401 Unauthorized` とし、以降のJSONパース・業務ロジック（`game_service`）を一切実行しない（なりすまし・改ざんされたWebhookリクエストの遮断）
- 検証は生のリクエストボディ（バイト列）に対して行う。JSONへデシリアライズ後に再シリアライズした値を使わない（元のボディとバイト単位で一致する保証がないため）
- 詳細: [docs/architecture.md](docs/architecture.md) 8節

## QRチェックインの検証

QRコード自体は各部屋の担当スタッフが目視判断で提示する運用のため、「QR画像の使い回し」自体はシステムでは積極的に防止せず、運用でカバーする方針としている。ただしサーバ側では、チェックインのたびに必ず以下を検証する。

- 送信されたUUIDが有効な部屋のものか
- そのプレイヤーの `current_room_id` と一致するか（案内されていない部屋のQRは無効）
- 判定モードが「QR＋正解入力」のイベントでは、`answer_verified` が true であることを確認する

クライアント（LIFF側）からの申告を信用せず、必ずサーバ側で再検証する。

## LIFFのLINEユーザーID検証（なりすまし対策）

- `POST /liff/checkin` はLINEアカウントで参加者を識別するが、クライアント（ブラウザ上のJavaScript）が送ってきた「LINEユーザーID」の文字列をそのまま信用しない（任意の文字列を送るだけで他人になりすませてしまうため）
- 代わりに `liff.getIDToken()` で取得したIDトークン（JWT）をLINEの検証エンドポイント（`POST https://api.line.me/oauth2/v2.1/verify`）に問い合わせ、有効な署名・有効期限・対象チャネル（`client_id` = `LINE_LOGIN_CHANNEL_ID`）であることを確認した上で、その応答に含まれる `sub` をLINEユーザーIDとして採用する
- 検証に失敗した場合（期限切れ・改ざん・対象チャネル不一致等）は401とし、以降のチェックイン処理を一切行わない
- 詳細: [docs/architecture.md](docs/architecture.md) 15節

## シークレット管理

- LINEチャネルシークレット・アクセストークン、DB接続情報、管理者パスワード等はすべて環境変数（`.env`）経由で注入する
- `.env` はコミットしない（`.gitignore` 済み）。値のテンプレートは [.env.example](.env.example) を参照する
- devcontainerの `compose.yaml` もハードコードを避け、`env_file` 経由で `.env` を参照する構成にしている

## 画像アップロード

- 拡張子だけでなく、マジックバイト検証で実際のファイル形式を確認する
- サイズ上限を設ける

## 依存関係・再現性

- 依存クレートのバージョンは `Cargo.lock` で固定する
- devcontainerのベースイメージ・CLIツールのバージョンも `.devcontainer/Dockerfile` で固定する

## 脆弱性を発見した場合

このプロジェクトは非公開の個人開発プロジェクト。脆弱性や懸念点を発見した場合は、開発者（gantaro）に直接連絡すること。
