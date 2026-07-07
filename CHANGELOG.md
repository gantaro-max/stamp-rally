# Changelog

このプロジェクトの変更履歴。[Keep a Changelog](https://keepachangelog.com/) の形式を参考にする。

## [Unreleased]

### Added
- ランキング画面を追加（#8）
  - `GET /admin/ranking`：クリア済み参加者を所要時間（`finished_at - started_at`）の短い順に表示。未クリアの参加者は順位を付けず「圏外」セクションに`started_at`昇順で別掲
  - `ranking_service::build_ranking`をDB非依存の純粋関数として実装し、ソート・所要時間の表示形式（1時間未満は`M:SS`、1時間以上は`H:MM:SS`）・同着時の連番順位付けを単体テストで検証
  - 自動更新（WebSocket・ポーリング）は導入せず、ページ読み込みのたびにDBの最新状態を反映する形で「リアルタイム」要件を満たす
  - 管理画面ナビゲーションに「ランキング」リンクを追加
- イベント設定画面を追加（#7）
  - `GET /admin/settings`：現在の個人戦/チーム戦・判定モードを反映したフォームを表示
  - `POST /admin/settings`：`event_service`経由で `events.is_team_mode` / `events.require_answer_check` を更新（チェックボックス未送信時は `false` として扱う）
  - 設定変更は既存の `rooms`（`answer`/`hint_msg`）や `players` の進行状況を遡及的に書き換えない（次回の判定・部屋案内から反映される）
  - 管理画面ナビゲーションに「設定」リンクを追加
- LIFFチェックイン・ゴール判定機能を追加（#6）
  - `GET /liff/checkin`：LIFF SDKでQRコードをスキャンするページ（LINEチャットにはPush Messageで案内するため、ページ自体はチェックイン結果のステータス表示のみ）
  - `POST /liff/checkin`：LINEのIDトークン検証エンドポイント（`https://api.line.me/oauth2/v2.1/verify`）でLINEユーザーIDを検証してから、クライアント申告を信用せずサーバ側でチェックインを判定（部屋の存在・参加登録済みか・クリア済みでないか・案内された部屋と一致するか・判定モードが「QR＋正解入力」の場合は正解済みか、の順に検証）
  - `game_service::checkin`：検証通過後に `visited_rooms` へ記録し、登録済み全部屋を訪問済みならクリアタイム（`finished_at`）を記録、そうでなければ未訪問の部屋からランダムに次の部屋を割り当てる（部屋数の上限「15」をハードコードせず、実際の登録数を基準に判定）
  - `line_client`：LINE Push API（`POST https://api.line.me/v2/bot/message/push`）への送信を追加。チェックイン成功後の次のクエスト案内・クリア報告は、LIFFのレスポンスではなく常にLINEチャットへのPush Messageとして送る
- LINE Bot基盤・ゲーム進行ロジックを追加（#5）
  - LINE Webhook `POST /callback`：`x-line-signature` の署名検証（HMAC-SHA256・定数時間比較、生ボディに対して検証）に失敗した場合は401、以降のイベント処理は一切行わない
  - `game_service`：「開始」コマンド（個人戦は個人名・チーム戦はチーム名の入力を促す。登録待ち状態はDBに永続化せずアプリ内メモリで保持）、未訪問の部屋からのランダム割当、「ヒント」「遊び方」「ヘルプ」「リセット」、判定モード「QR＋正解入力」時の正誤判定（`answer`をカンマ区切りで複数許容、前後空白・大小文字を無視して比較）
  - `line_client`：LINE Messaging API（Reply）への送信、クエスト通知用Flex Messageの組み立て（`game_service`はLINE固有のJSON構造・`reqwest`に非依存）
  - `GET /public/image/{uuid}`：部屋画像の公開配信ハンドラーを追加（`room-management`ではスコープ外としていたもの。Flex Messageの画像参照に必要なため本機能で実装）
  - `AppState` を導入（`pool`・LINEチャネル情報・`PUBLIC_BASE_URL`・登録待ち状態を保持）。`FromRef` により既存の `State<MySqlPool>` ハンドラーは無変更で動作
- 部屋（チェックポイント）管理機能を追加
  - `/admin/rooms` 系の一覧・新規登録・編集・削除ハンドラー（最大15部屋、`require_admin` 保護）
  - 画像アップロード（マジックバイト検証・5MBサイズ上限・寸法上限・800px幅/JPEG品質80へのリサイズ）
  - 部屋ごとのQRコード（`qr_uuid`）をその場でPNG生成する `GET /admin/rooms/{id}/qr`
  - 判定モード（`require_answer_check`）に応じた `answer` / `hint_msg` の必須・NULL強制バリデーション
  - 画像更新時は新画像の保存に成功してから旧画像を削除する順序とし、失敗時の孤立参照を防止（#9）
  - `handlers::rooms`から`event_repository`への直接依存を除去し、`room_service::current_event`経由に統一（#9）
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

### Fixed
- `POST /admin/settings` で、チェックボックス（個人戦/チーム戦・判定モード）にチェックを入れて送信すると、HTMLが送る値`"on"`をAxumの`Form`抽出（bool型）が受け付けられず、常に422でハンドラーに到達できなかった不具合を修正（`"on"`/`"true"`を`true`として扱うカスタムデシリアライザを追加）。事実上、設定画面から各項目をONに切り替える操作が一切機能していなかった（#10）

### Changed
- devcontainerのシークレット管理を `.env` 経由の `env_file` 方式に変更し、MySQLヘルスチェック・ホストポート設定を堅牢化（#1）
- 設計ドキュメントを `docs/` フォルダにまとめて再編し、相互参照リンクを整理
- 部屋一覧画面（`/admin/rooms`）で、各部屋のQRコードをサムネイル画像として一覧に直接表示するよう変更（従来はテキストリンクで別ページに遷移するのみ）（#11）

### Security
- LIFF `/liff/checkin` で、クライアント（ブラウザJS）が申告するLINEユーザーIDを直接信用せず、LINEのIDトークン検証エンドポイントで検証した `sub` のみを正とするよう実装（なりすまし対策）
- LINE Webhook `/callback` の署名検証（`x-line-signature`、HMAC-SHA256・定数時間比較）を追加し、なりすまし・改ざんされたリクエストを401で遮断
- シークレット（LINEチャネル情報・DB接続情報・管理者パスワード等）はすべて環境変数（`.env`、gitignore対象）経由で注入する方針を明文化し、devcontainerの設定からハードコードを排除
- `csrf_service::verify_token` のトークン比較を定数時間比較に変更（`line_client`の署名検証と同様の方式に統一）（#11）
- LIFF `/liff/checkin` が読み込むBootstrap CDNの`<link>`にSubresource Integrity（`integrity`/`crossorigin`）属性を追加し、CDN改ざん時のサプライチェーンリスクを軽減（#11）
