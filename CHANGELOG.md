# Changelog

このプロジェクトの変更履歴。[Keep a Changelog](https://keepachangelog.com/) の形式を参考にする。

## [Unreleased]

### Added
- 管理画面（`/admin/*`・`/auth/login`）にデザインシステムを導入（#21）
  - CSSカスタムプロパティによる独自カラートークン（スタンプのインクをイメージした朱色系プライマリカラー`#B54B3A`等）を`Bootstrap 5.3`の変数上書きで適用。新規の静的ファイル配信基盤・Webフォントは追加せず、既存テンプレートへのインライン`<style>`のみで実現
  - ダッシュボードを3枚のstat-cardレイアウト（部屋登録進捗のプログレスバー・イベント設定バッジ・ランキング導線）に刷新。各画面の見出しを`page-header`パターンに統一し、テーブルの縞模様を除去、ランキング1位に強調バッジを追加
  - ハンドラー・サービス層・DBクエリ・ルーティングは無変更（表示レイヤーのみの変更）。設計は[docs/architecture.md 20節「管理画面デザインシステム」](docs/architecture.md#20-管理画面デザインシステム)を参照
  - 外部から取り込まれたAnthropicブランドのデザイントークン（`DESIGN.md`）を参考検討したが、ブランド混同を避けるため配色・書体等の固有要素は流用せず、独自トークンとして新規設計した
- クエスト通知のFlex Messageに状況に応じた「つなぎの文」を追加（#20）
  - `ReplyMessage::Quest`に`intro`フィールドを追加し、`body`の先頭に表示。参加登録直後は「最初の部屋は」、「開始」再送信時（未クリアの部屋の再案内）は「現在向かっている部屋は」、QRチェックイン成功で次の部屋に案内するときは「【直前にクリアした部屋名】クリアおめでとうございます。次の部屋は」を表示し、部屋案内メッセージが唐突に始まる分かりにくさを解消
  - `header`・`hero`・`footer`（QRを読むボタン）や`ReplyMessage::Cleared`のクリア演出は変更なし
- LINE Bot参加登録の一時状態（「開始」〜名前入力までの間の状態）をDBに永続化（#15）
  - `pending_registrations`テーブルを新設し、従来アプリ内メモリ（`Arc<Mutex<HashSet<String>>>`）で保持していた登録待ち状態を`pending_registration_repository`経由のDB操作に置き換え
  - Koyeb無料枠（Ecoインスタンス）が最小インスタンス数を0に固定できず、アイドル時にインスタンス数0へスケールダウンすることが実際のデプロイ作業で判明したため。メモリ保持のままだと、参加者が「開始」を送ってから名前を入力するまでの間にインスタンスが再起動すると登録待ち状態が失われる実害があった
  - 管理者セッション（`tower_sessions::MemoryStore`）はスコープ外とし、引き続きメモリ保持のまま（再ログインで復帰できる実害の小さい範囲として許容）
- 管理画面ダッシュボードを実装（#13）
  - `GET /admin/dashboard`：管理者認証実装時からの仮実装（`"ok"`を返すのみ）を置き換え、イベント設定状況（個人戦/チーム戦・判定モード）・部屋登録数（`n / 15部屋`）・ランキングへのリンクを表示する画面を追加
  - ハンドラーは既存の`event_service::current`・`room_service::list`を組み合わせるのみの薄い実装とし、新規service/repository関数は追加していない。部屋ごとの個別リンクはダッシュボードに複製せず`/admin/rooms`側の既存導線に一本化（一覧の二重管理を避けるため）
  - `docs/operator-guide.md`2節の記載とダッシュボードの実装が食い違っていた状態を解消
- Koyebへの本番デプロイに向けた土台を整備（#12）
  - リッスンポートを`PORT`環境変数から決定する`resolve_port`関数を追加（未設定・パース失敗時は8000にフォールバック）。Koyebなど、動的にポート番号を割り当てるPaaS上での起動に対応
  - 本番用のマルチステージ`Dockerfile`を新規追加（ビルドステージ`rust:1-bookworm` → 実行ステージ`debian:bookworm-slim`）。実行イメージにはコンパイル済みバイナリと`ca-certificates`のみを含め、ソースコード・ビルド中間成果物は含めない（`.devcontainer/Dockerfile`とは別物で、開発用構成は変更していない）
  - `cargo build --release --locked`でビルドし、`Cargo.lock`との不整合による意図しない依存関係更新を防止
  - 実行ステージは非rootユーザー（`appuser`）でアプリケーションを起動し、コンテナの多層防御を強化
  - デプロイ構成の基本設計（ビルド方式・DB分離方針・環境変数・セッションストアの制約等）は`docs/architecture.md`18節、実際のセットアップ手順は`README.md`「本番デプロイ（Koyeb）」に記載
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
- 部屋（チェックポイント）管理機能を追加（#4）
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
- 本番DB（TiDB Serverless）・LINE Messaging APIへの呼び出しにタイムアウトが設定されておらず、コネクションが応答不能になった場合、リクエスト処理が無期限にハングして参加者への応答が返らなくなる不具合を修正。本番運用前の動作確認で、参加者が「開始」→チーム名入力後に無反応になる事象が発生し発覚した（#22）
  - `reqwest::Client`にリクエスト全体のタイムアウト（10秒）を設定
  - `/callback`（LINE Webhook）・`/liff/checkin`双方で、DBアクセスを伴う本体処理（`game_service::handle_text_message` / `game_service::checkin`）を`game_service::with_db_call_timeout`（`tokio::time::timeout`、15秒）でラップし、`GameServiceError::Timeout`として既存のエラーログ・エラー処理経路に統合
  - 個別のリクエストが無期限にハングしてDBコネクションプールを占有し続け、他の参加者の処理にも波及する事態を防ぐ。設計は[docs/architecture.md 21節「DB接続・外部API呼び出しのタイムアウト」](docs/architecture.md#21-db接続外部api呼び出しのタイムアウト)を参照
- LIFFチェックイン失敗時（`/liff/checkin`）のメッセージが、理由（`room_not_found`/`not_registered`/`already_finished`/`wrong_room`/`answer_not_verified`/`invalid_id_token`）によらず単一の汎用文言しか表示されなかった問題を修正。本番運用の動作確認で、特に案内されていない部屋のQRを読んだ場合（`wrong_room`）に原因・次の行動が分からず参加者が混乱するとのフィードバックがあったため、理由ごとにメッセージを出し分けるよう変更（`templates/liff/checkin.html`のクライアント側表示のみの変更、サーバー側の判定ロジックは無変更）（#17）
- クエスト通知（Flex Message）に「QRコードを読み込んでください」という案内文はあるが、実際にLIFFページ（QRスキャン画面）を開く手段がどこにも無かった不具合を修正。本番デプロイ後の動作確認で発覚した（案内文だけで導線が存在せず、参加者がQRスキャンにたどり着けない状態だった）。クエスト通知のFlex Messageの`footer`に「QRを読む」ボタン（`https://liff.line.me/{LIFF_ID}`を開く`uri`アクション）を追加（#16）
- `sqlx`依存にTLSバックエンド（`tls-native-tls`）が指定されておらず、TLS必須接続の本番DB（TiDB Serverless）に接続しようとした瞬間に`"SQLx was built without TLS support enabled"`エラーで起動不能になる不具合を修正。Koyeb本番デプロイに向けた疎通確認作業（`sqlx migrate run`）で発覚した。`reqwest`が既に依存している`native-tls`（システムのOpenSSL、本番用`Dockerfile`が`ca-certificates`を含む既存方針）を再利用する形とし、`rustls`系は新たに導入していない（#14）
- `POST /admin/settings` で、チェックボックス（個人戦/チーム戦・判定モード）にチェックを入れて送信すると、HTMLが送る値`"on"`をAxumの`Form`抽出（bool型）が受け付けられず、常に422でハンドラーに到達できなかった不具合を修正（`"on"`/`"true"`を`true`として扱うカスタムデシリアライザを追加）。事実上、設定画面から各項目をONに切り替える操作が一切機能していなかった（#10）

### Changed
- LIFFチェックインページに「LINEチャットに戻る」ボタンを追加（#18）
  - QRコード読み込み後、`liff.closeWindow()`でLIFFブラウザを閉じてトーク画面に戻れるようにした。本番運用フィードバックで「チェックイン後の次の操作が分からない」との指摘があったため
  - チェックイン結果（成功/クリア/拒否のいずれか）を受け取った後にのみ表示。QRスキャン失敗時・LIFF初期化失敗時は表示しない
- クエスト通知・クリア報告のFlex Messageの見た目を強化（#19）
  - クエスト通知に緑のヘッダーバナー（「次のクエスト」）・区切り線・文字装飾を追加
  - クリア報告を平文テキストから、クリアタイムを含む専用のFlex Message（🎉演出）に変更。`ranking_service::format_elapsed`（ランキング画面の経過時間フォーマット関数）を`pub(crate)`化し`game_service`から再利用
  - `CheckinOutcome::Cleared`が`ReplyMessage`を保持する形に変更し、`NextQuest`と同じ経路（`to_line_message`）でFlex Messageを組み立てるよう統一。`POST /liff/checkin`自体のレスポンスJSONは変更なし
- devcontainerのシークレット管理を `.env` 経由の `env_file` 方式に変更し、MySQLヘルスチェック・ホストポート設定を堅牢化（#1）
- 設計ドキュメントを `docs/` フォルダにまとめて再編し、相互参照リンクを整理
- 部屋一覧画面（`/admin/rooms`）で、各部屋のQRコードをサムネイル画像として一覧に直接表示するよう変更（従来はテキストリンクで別ページに遷移するのみ）（#11）

### Security
- LIFF `/liff/checkin` で、クライアント（ブラウザJS）が申告するLINEユーザーIDを直接信用せず、LINEのIDトークン検証エンドポイントで検証した `sub` のみを正とするよう実装（なりすまし対策）
- LINE Webhook `/callback` の署名検証（`x-line-signature`、HMAC-SHA256・定数時間比較）を追加し、なりすまし・改ざんされたリクエストを401で遮断
- シークレット（LINEチャネル情報・DB接続情報・管理者パスワード等）はすべて環境変数（`.env`、gitignore対象）経由で注入する方針を明文化し、devcontainerの設定からハードコードを排除
- `csrf_service::verify_token` のトークン比較を定数時間比較に変更（`line_client`の署名検証と同様の方式に統一）（#11）
- LIFF `/liff/checkin` が読み込むBootstrap CDNの`<link>`にSubresource Integrity（`integrity`/`crossorigin`）属性を追加し、CDN改ざん時のサプライチェーンリスクを軽減（#11）
