# アーキテクチャ設計 — StampRallyBot（仮称）

## 1. 技術スタック

| 項目 | 内容 |
|:--|:--|
| 言語 | Rust（stable） |
| Webフレームワーク | Axum + Tokio |
| DBアクセス | sqlx（非同期、生SQL方式） |
| テンプレート | Askama + Bootstrap 5 |
| DB（ローカル） | MySQL 8.0（Docker） |
| DB（本番） | TiDB Serverless（MySQL互換） |
| 外部API | LINE Messaging API（`reqwest` で直接呼び出す自前クライアント） |
| QRコード連携 | LIFF（`liff.scanCodeV2()`）でスキャン、結果をサーバAPIに送信 |
| パスワードハッシュ | Argon2 |
| 画像処理 | `image` crate（リサイズ） |
| QRコード生成 | `qrcode` crate（部屋ごとのQR画像を生成） |
| ビルド | Cargo |
| ホスティング | Koyeb（詳細は18節） |

### MysteryBot（Java）との対応

| MysteryBot | StampRallyBot |
|:--|:--|
| Spring Boot / Spring Security | Axum + Tower（ミドルウェアでセッション・認証管理） |
| MyBatis（XMLマッパー） | sqlx（生SQL、コンパイル時クエリ検証） |
| Thymeleaf | Askama |
| 公式LINE Java SDK | `reqwest` による自前HTTPクライアント |
| Thumbnailator | `image` crate |
| BCrypt | Argon2 |
| Gradle | Cargo |

---

## 2. レイヤー構成

```
Handler(Axum) → Service → Repository（sqlx） → DB
                   ↓
              LineClient → LINE Messaging API
```

### ハンドラー構成

| 用途 | パス | 説明 |
|:--|:--|:--|
| ヘルスチェック | `/health` | 疎通確認用（認証不要・DB非依存。デプロイ先のヘルスチェックにも使用、18節） |
| 認証 | `/auth/*` | 管理者ログイン・ログアウト |
| 管理画面 | `/admin/*` | ダッシュボード・部屋管理・QRコード発行・設定・ランキング閲覧 |
| LINE Webhook | `/callback` | LINE Webhookの受信・処理 |
| LIFF連携 | `/liff/checkin` | LIFFから送られたQRスキャン結果を受け取るAPI |
| 画像配信 | `/public/image/{uuid}` | 部屋画像のバイナリ配信（認証不要） |

### サービス層

| モジュール | 責務 |
|:--|:--|
| `auth_service` | 管理者ログイン、Argon2によるパスワード認証 |
| `room_service` | 部屋CRUD、画像アップロード、QRコード（UUID）発行・生成 |
| `event_service` | イベント設定（個人戦/チーム戦、判定モード）の取得・更新 |
| `game_service` | ゲーム進行ロジック（参加登録、部屋のランダム割当、正誤判定、QRチェックイン判定、ゴール判定） |
| `ranking_service` | クリアタイムランキングの取得 |
| `line_client` | LINE Messaging APIへのメッセージ送信（Flex Message組み立て含む） |
| `csrf_service` | セッション格納トークンによるCSRFダブルサブミット検証（発行・検証） |
| `image_service` | アップロード画像のマジックバイト検証・サイズ/寸法上限チェック・JPEG再エンコード |
| `qr_service` | 部屋UUIDからQRコードPNGを生成 |

### 認証方式

- Cookieベースのセッション（`tower-sessions`）
- 管理者ログインのみ（プレイヤーはLINEアカウントのみで識別、Webログイン不要）
- CSRF保護（`/callback` と `/liff/checkin` は除外）

#### セッション実装

- `tower_sessions::SessionManagerLayer` + `tower_sessions::MemoryStore` をルーター全体に適用する
  - 管理者は1名のみの運用のため、プロセス再起動でセッションが失われても再ログインで足りる（永続ストアは採用しない）
  - 現時点では `/health` `/auth/*` `/admin/*` にのみ影響する。今後 `/callback`（LINE Webhook）や `/liff/checkin` を追加する際、これらはセッションを利用しない想定のため、レイヤーの適用範囲をルーター全体のままにするか、`/auth`・`/admin` 配下に絞るかを実装指示書作成時に改めて検討する
- セッションの有効期限は非アクティブ12時間（`Expiry::OnInactivity`）
- ログイン成功時にセッションへ `admin_authenticated = true` を保存し、あわせてセッションIDをローテーションする（`tower_sessions::Session::cycle_id()`。session fixation対策）
- `/admin/*` と `POST /auth/logout` は、セッションに `admin_authenticated = true` が無ければ `/auth/login` へ302リダイレクトする専用ミドルウェア（`require_admin`）を通す

#### CSRF実装

- 署名付きトークンやCSRF専用クレートは導入せず、セッションに保存したランダムトークン（UUID v4）とフォームの隠しフィールドを突き合わせるダブルサブミット方式とする
- `GET` でフォームを表示する際にセッションにトークンが無ければ発行し、隠しフィールド `csrf_token` に埋め込む
- 対応する `POST` ハンドラーで、フォームの `csrf_token` とセッション内の値が一致することを検証する（不一致は403）
- `/callback` と `/liff/checkin` はセッション自体を使わないため、この検証の対象外（設計上そもそも対象にならない）

#### 起動時シード処理（管理者パスワード）

- `events` テーブルは本運用では1行のみ（複数イベント対応は将来拡張）
- アプリ起動時（`main.rs`）に `events` の行数を確認し、0件であれば環境変数 `ADMIN_PASSWORD` をArgon2でハッシュ化した上で初期の1行（`event_name` はデフォルト値、`is_team_mode = false`, `require_answer_check = false`）を作成する
- 既に1行以上存在する場合はシードを行わない（`ADMIN_PASSWORD` は無視される。パスワード変更機能は将来の拡張とし今回のスコープ外）
- `events` が0件かつ `ADMIN_PASSWORD` 未設定の場合はエラーを出力してプロセスを終了する（`DATABASE_URL` 未設定時と同様の扱い）

#### アプリ状態（`AppState`）と `FromRef`

- LINE連携の追加に伴い、`main.rs` の `with_state` を単一の `MySqlPool` から `AppState`（`Clone`）に切り替える
  - `AppState { pool: MySqlPool, line_channel_secret: Arc<str>, line_channel_access_token: Arc<str>, public_base_url: Arc<str>, liff_id: Arc<str>, line_login_channel_id: Arc<str>, verify_id_tokens: bool, pending_registrations: PendingRegistrations, http_client: reqwest::Client, send_line_replies: bool }`（`liff_id`・`line_login_channel_id`はSlice B、`verify_id_tokens`・`send_line_replies`はテスト用フック、`http_client`はLINE API呼び出し共用のため後続スライスで追加）
  - 既存ハンドラーは `State<MySqlPool>` を使い続けられるよう、`impl FromRef<AppState> for MySqlPool`（`state.pool.clone()` を返す）を実装し、既存シグネチャを変更しない
  - LINE Webhook・画像配信ハンドラーは必要に応じて `State<AppState>` や `State<Arc<str>>`（`FromRef` 経由）を使う
- `LINE_CHANNEL_SECRET` / `LINE_CHANNEL_ACCESS_TOKEN` / `PUBLIC_BASE_URL` / `LIFF_ID` / `LINE_LOGIN_CHANNEL_ID` は `DATABASE_URL` と同様、起動時に未設定ならエラー出力してプロセスを終了する（`.env.example` に追記済み）

---

## 3. データモデル（案）

MysteryBotの `team_groups` に相当する概念は今回1レコードのみの運用だが、将来の複数イベント対応の拡張余地としてテーブル自体は残す。

### events

| カラム | 型 | 説明 |
|:--|:--|:--|
| `id` | INT(PK) | イベントID |
| `event_name` | VARCHAR | イベント名 |
| `admin_pass_hash` | VARCHAR | Argon2ハッシュ化パスワード |
| `is_team_mode` | BOOLEAN | チーム戦フラグ（false=個人戦） |
| `require_answer_check` | BOOLEAN | 判定モード（true=QR＋正解入力必須） |

### rooms（部屋 / チェックポイント）

| カラム | 型 | 説明 |
|:--|:--|:--|
| `id` | INT(PK) | 部屋ID |
| `event_id` | INT(FK) | 所属イベント |
| `room_name` | VARCHAR | 部屋名 |
| `quest_text` | TEXT | 部屋で提示するクエスト文 |
| `answer` | VARCHAR(NULL可) | 正解（`require_answer_check` がtrueのイベントでのみ使用、カンマ区切りで複数可） |
| `hint_msg` | VARCHAR(NULL可) | ヒント |
| `image_id` | INT(FK, NULL可) | 画像ID（`room_images` 参照） |
| `qr_uuid` | VARCHAR(36) | QRコードに埋め込む一意なUUID |

### players（参加者）

| カラム | 型 | 説明 |
|:--|:--|:--|
| `id` | INT(PK) | 自動採番 |
| `line_user_id` | VARCHAR | LINE User ID |
| `event_id` | INT(FK) | 参加イベント |
| `player_name` | VARCHAR | 個人名 または チーム名 |
| `current_room_id` | INT(FK, NULL可) | 現在向かうよう案内している部屋 |
| `answer_verified` | BOOLEAN | 現在の部屋で正解済みか（`require_answer_check` がtrueの場合のみ使用） |
| `started_at` | DATETIME | 参加登録日時 |
| `finished_at` | DATETIME(NULL可) | 15部屋目クリア日時（未クリアはNULL） |

ユニーク制約: `(line_user_id, event_id)`

### visited_rooms（訪問済み部屋の記録）

| カラム | 型 | 説明 |
|:--|:--|:--|
| `player_id` | INT(FK) | プレイヤー |
| `room_id` | INT(FK) | 訪問済みの部屋 |
| `visited_at` | DATETIME | チェックイン日時 |

複合PK: `(player_id, room_id)`

### room_images（画像ストレージ）

| カラム | 型 | 説明 |
|:--|:--|:--|
| `id` | INT(PK) | 内部管理ID |
| `uuid` | VARCHAR(36) | 公開URL用ID（`/public/image/{uuid}`） |
| `data` | LONGBLOB | 画像バイナリ |
| `mime_type` | VARCHAR | MIMEタイプ |

---

## 4. ゲームフロー（LINE Bot / LIFF）

```
プレイヤー「開始」
    → 個人戦: 個人名を入力 / チーム戦: チーム名を入力
    → game_service が未訪問の部屋からランダムに1部屋選出
    → クエスト文＋画像をFlex Messageで送信、current_room_id を更新

【判定モード: QRのみ】
プレイヤーが現地でクエストをこなす（クスタッフの目視判断のみ）
    → LIFFでQRコードをスキャン → /liff/checkin にUUIDを送信
    → 該当部屋が current_room_id と一致し、未訪問であることを確認
    → visited_rooms に記録

【判定モード: QR＋正解入力】
プレイヤーがLINEで正解を送信
    → 正誤判定 → 正解なら answer_verified = true、「QRを読み込んでください」と案内
    → LIFFでQRコードをスキャン → answer_verified が true であることも確認した上で visited_rooms に記録

チェックイン成功後:
    → 訪問済み部屋数が15未満 → 未訪問部屋からランダムに次の部屋を通知
    → 訪問済み部屋数が15に到達 → finished_at を記録、「最初の部屋に戻ってください」と案内（以降の処理なし）
```

---

## 5. QRコードの仕組み

- 部屋登録時に `qr_uuid`（UUID v4）を発行してDBに保存する（QR画像自体はDBに保存しない）
- 管理画面 `GET /admin/rooms/{id}/qr` にアクセスするたびに、`qrcode` crate で `qr_uuid` の文字列をその場でPNGにエンコードして返す（`Content-Type: image/png`、`require_admin` 経由で保護）。印刷はスタッフがブラウザの印刷機能を使う想定で、専用の印刷レイアウトは用意しない
- QRコードの中身（LIFFがスキャンして取得する文字列）は `qr_uuid` そのもの。URLなどでラップしない
- LIFFアプリの「QRを読む」ボタンから `liff.scanCodeV2()` を呼び出し、読み取ったUUIDと `liff.getIDToken()` で取得したIDトークンを `/liff/checkin` にPOST
- サーバ側の検証項目（詳細は15節）：
  1. IDトークンをLINEの検証エンドポイントに問い合わせ、有効な署名・有効期限であることと、そこに含まれる `sub`（LINEユーザーID）を確認する（クライアントが送ってきた文字列をそのままLINEユーザーIDとして信用しない）
  2. UUIDが有効な部屋のものか
  3. そのプレイヤーの `current_room_id` と一致するか（案内された部屋以外は無効）
  4. `require_answer_check` がtrueのイベントでは `answer_verified` がtrueか
- QRコードの不正利用（写真の使い回し等）はシステムでは対策せず、スタッフの目視提示運用でカバーする

---

## 6. 画像配信の仕組み

- 画像はDB（`room_images.data`）にLONGBLOBで保存（MysteryBot踏襲）
- アップロード時にUUIDを生成し、公開URLは `/public/image/{uuid}`
- 認証不要エンドポイント（LINE BotのFlex Messageから直接参照されるため）
- アップロード時に `image` crateで800px幅・JPEG 80%品質にリサイズし、`mime_type` は常に `image/jpeg` を保存する（入力フォーマットによらず出力を統一する）

### 画像アップロードの検証

- アップロードサイズ上限: 5MB（リサイズ前の生データ）。超過時はアップロードを拒否する
- 拡張子は見ず、`image` crateの `image::guess_format` でマジックバイトから実フォーマットを判定する
- 許可フォーマット: JPEG, PNG のみ（それ以外はデコードを試みず拒否する）
- 画像の寸法上限（4096px・1600万画素）を設け、超過する画像はデコード前に拒否する（巨大画像のデコードによるメモリ枯渇・decompression bomb対策）
- 判定・デコードに成功した画像のみリサイズして保存する

### `GET /public/image/{uuid}`（配信ハンドラー）

- `room-management` 指示書ではスコープ外としていたが、LINE BotのFlex Messageが画像を参照するために本ハンドラーが必須となるため、`line-bot-core`（本ドキュメントの8節以降）であわせて実装する
- `room_images.uuid` で1件検索し、無ければ404。存在すれば `data` を body、`Content-Type` を `mime_type` の値（常に `image/jpeg`）で返す
- 認証不要（LINEのサーバーから直接参照されるため）。`require_admin` は通さない

---

## 7. 部屋（チェックポイント）管理の実装方針

- 新規登録時、既存の部屋数が15件に達している場合は登録を拒否する（イベントあたり最大15部屋。`docs/requirements.md` 参照）
- `require_answer_check`（判定モード）は `events` の該当イベント1行を参照して判定する
  - `true` の場合のみ `answer`（正解）を必須項目として扱う。`hint_msg` は任意
  - `false` の場合、フォームに正解・ヒント欄を表示しない。仮に送信されても `answer` / `hint_msg` は保存せず常にNULLとする（クライアントの申告を信用しない）
- 画像を伴う登録・更新は `multipart/form-data` で受け取る。画像が添付されていない場合は画像なしで登録できる（`docs/requirements.md`：画像は任意）
- 部屋の画像を更新する場合、新しい画像を先に `room_images` へ挿入して `rooms.image_id` を張り替えてから、更新前に参照されていた `room_images` 行を削除する（この順序を守ることで、新しい画像の保存に失敗した場合でも古い画像が残り、`rooms.image_id` が無効な行を指す状態を防ぐ）
- 部屋を削除する場合、`rooms` 行の削除に合わせて、参照していた `room_images` 行も削除する（`visited_rooms` は既存の `ON DELETE CASCADE` で自動的に削除される）
- 部屋一覧・登録・編集・削除・QR表示はすべて `/admin/*` 配下（`require_admin` 済み）。フォームは既存の `csrf_service`（セッション格納トークンとのダブルサブミット）を再利用する
- 管理画面のAskamaテンプレートは `templates/admin/_base.html` を共通レイアウト（Bootstrap 5のナビゲーション等）として `{% extends %}` で利用する。今後追加する設定画面・ランキング画面もこのレイアウトに乗せる（`templates/auth/login.html` はログイン前の独立画面のため対象外のまま）

---

## 8. LINE Webhook（`/callback`）の受信と署名検証

- LINEはWebhookリクエストに `x-line-signature` ヘッダーを付与する。値は「チャネルシークレットをキーにしたリクエストボディ（生バイト列）のHMAC-SHA256をBase64エンコードしたもの」
- ハンドラーは `axum::body::Bytes` で生ボディを受け取り、JSONへのデシリアライズより前に署名検証を行う（デシリアライズ後の再シリアライズ結果はバイト単位で元のボディと一致する保証がないため、必ず生ボディに対して検証する）
- 検証は `line_client::verify_signature(channel_secret: &str, body: &[u8], signature_header: &str) -> bool` という純粋関数として実装し、ネットワーク・DBに依存させない（単体テスト容易性のため）
- ヘッダー欠如・検証失敗はどちらも `401 Unauthorized` を返し、以降の処理（JSONパース・`game_service` 呼び出し）は一切行わない
- 検証成功後、ボディをLINEのWebhookイベントスキーマ（`events: [...]`）としてパースする。`type` が `"message"` かつ `message.type` が `"text"` のイベントのみ処理対象とし、それ以外（`follow`/`unfollow`/`postback`等）は無視する（今回のスコープ外）
- 各イベントは `source.userId`（LINEユーザーID）・`replyToken`・`message.text` を取り出し、`game_service` の1回の呼び出しに対応させる
- **1件のイベント処理で内部エラーが起きても、Webhookレスポンス全体は200を返す**（LINEは非200応答時にWebhookを再送するため、DB状態が既に変化した後の再送は二重処理につながる。個別イベントのエラーはログに残すのみとし、他のイベントの処理は継続する）
- 署名検証自体に失敗した場合（なりすまし・改ざんの疑い）は例外的に401を返す（この場合はまだ何も処理していないため、LINE側の再送があっても問題ない）
- **ペイロード内の全イベントの実処理（`game_service`呼び出し・LINEへの返信送信）は、まとめて1つの`tokio::spawn`バックグラウンドタスクとして起動し、ハンドラー自体は署名検証・JSONパースが終わり次第、実処理の完了を待たずに200を返す**（21節参照。理由: LINEプラットフォーム自体がWebhookレスポンスを待つ時間に独自のタイムアウトを持っており、本番運用でこれより実処理が長引いた際に`request_timeout`としてLINE側から見た配信失敗になり、参加者への応答が失われる事象が実際に発生したため）
  - **イベントごとに個別の`tokio::spawn`を行ってはならない**。`payload.events`をループする処理全体を1つのasyncブロックにまとめ、それを丸ごと1回だけ`tokio::spawn`すること。理由: イベント単位でspawnすると、同一ペイロードに複数イベント（例: 同一参加者が短時間に連続送信したメッセージがLINE側で1回の配信にまとめられた場合）が含まれる際に、本来配列順に逐次処理されるべきイベントが並行実行され、`game_service`側に排他制御が無いことと相まって、処理順序が入れ替わる・データ不整合が起きるリスクがある（最終レビューで発見）。ペイロード全体を1つのタスクにまとめることで、そのタスク内では従来通り配列順の逐次処理が保たれる
  - 上記はあくまで**同一ペイロード内**のイベント順序の話であり、LINEから**別々のWebhookリクエスト**として届いた場合（例: 参加者の連続送信が別々の配信としてLINEから送られてきた場合）まで順序を保証するものではない。これは本対応の前から存在する制約であり、今回のスコープでは変更しない
  - `AppState`にテスト用フック`spawn_background_tasks: bool`（本番は常に`true`固定）を持たせ、テスト時のみ`false`にしてバックグラウンド起動せずその場で`.await`する経路に切り替えられるようにする（Slice Aの`send_line_replies`と同じ考え方のテスト用フック。`false`にしないと、レスポンス返却直後にDB状態をアサートする既存テストがタイミング依存になってしまうため）
  - バックグラウンドタスク内のエラー処理（ログ記録のみで処理継続）は、実処理を`.await`する場所が変わるだけで、8節の既存方針・21節のタイムアウト処理と変わらない

---

## 9. 会話状態管理（参加登録の一時状態）

- 「開始」送信後、名前（個人戦）／チーム名（チーム戦）の入力を待つ「登録待ち」状態が必要になるが、`players` 行は名前確定まで作成しない（`player_name` はNOT NULLのため）
- **`pending_registrations` テーブルにDB永続化する**（[docs/database.md](database.md)参照）
  - 従来はアプリ内メモリ（`AppState.pending_registrations: Arc<Mutex<HashSet<String>>>`）で保持する方針だったが、Koyeb無料枠（Ecoインスタンス）は最小インスタンス数を0に固定できず、アイドル時にインスタンス数0へスケールダウンしうることが実際のデプロイ作業で判明した（18節）。メモリ保持のままだと、参加者が「開始」を送ってから名前を入力するまでの間にインスタンスが再起動すると登録待ち状態が失われ、名前入力が正しく処理されない実害があるため、DB永続化に方針を変更した
  - 実装: `pending_registration_repository`（`line_user_id` + `event_id` をキーに存在確認・追加・削除を行う）。「開始」受信時に挿入、名前受信で消費（登録処理後に削除）、「リセット」受信時にも削除する
- 将来複数イベント運用に拡張する場合はこの節の設計を見直す（今回は `events` が1行のみのため、どのイベントの登録待ちかを気にする必要がない）

---

## 10. `game_service` のコマンド分岐

プレイヤーからのテキストメッセージ受信時、以下の優先順位で分岐する（`event` は `event_repository::find_singleton` で取得した唯一の行、`player` は `line_user_id` + `event.id` で検索した `players` 行）。

1. `遊び方` / `ヘルプ` → 常に操作ガイドを返信（登録状態を問わない）
2. `リセット` →
   - `player` が存在する → 削除（`visited_rooms` は `ON DELETE CASCADE` で連動削除）し、登録待ち状態も解除。「参加データを削除しました」と案内
   - 登録待ち状態のみ → 状態を解除し「登録をキャンセルしました」と案内
   - どちらでもない → 「現在参加登録されていません」と案内
3. `開始` →
   - `player` が既に存在し `finished_at` が設定済み → 「クリア済みです。最初の部屋に戻ってください」と案内（再登録しない）
   - `player` が既に存在し未クリア → 現在の `current_room_id` のクエスト（Flex Message）を再送する（案内メッセージを見失った参加者向けの救済。新規に部屋を割り当て直さない）
   - 登録待ち状態 → 名前入力の催促を再送（重複登録は作らない）
   - どちらでもない → 登録待ち状態に遷移し、`event.is_team_mode` に応じて「個人名」または「チーム名」の入力を促す
4. 登録待ち状態 かつ 上記いずれのコマンドにも該当しないテキスト → 名前入力として扱う
   - 前後の空白を除去し、空文字なら再度入力を促す（登録待ち状態は解除しない）
   - 有効なら `players` 行を作成（`started_at = now`）→ 登録待ち状態を解除 → 未訪問の部屋からランダムに1部屋を選出（11節）→ `current_room_id` を更新 → クエストをFlex Messageで送信
5. `player` が存在し、`finished_at` が設定済み → 「最初の部屋に戻ってください」の案内を再送
6. `ヒント` →
   - `event.require_answer_check = true` → 現在の部屋の `hint_msg` を返信（NULLなら「ヒントは登録されていません」）
   - `false` → 「このイベントではヒント機能は利用できません」
7. 上記のどれにも該当しないテキスト（`player` が存在し、未クリアで、コマンドでもない自由入力） →
   - `event.require_answer_check = false` → 「QRコードを読み込んでください」の案内を返信（正誤判定は行わない）
   - `event.require_answer_check = true` かつ `answer_verified = false` → 正誤判定（12節）
   - `event.require_answer_check = true` かつ `answer_verified = true`（既に正解済みでQR待ち） → 「QRコードを読み込んでください」の案内を再送
8. `player` が存在せず、登録待ちでもなく、上記のどのコマンドにも該当しない → 「『開始』と送信して参加登録してください」と案内

### 11. 部屋のランダム割当

- `room_repository::find_random_unvisited(pool, event_id, player_id)` を新設し、`rooms` から「`visited_rooms` に存在しない」行を `ORDER BY RAND() LIMIT 1` で1件取得する（乱数生成のためだけに新規crateは追加せず、既存のsqlx生SQLの範囲で完結させる）
- 該当行が無い（＝全部屋訪問済み）はずの経路で呼ばれた場合はロジック上の不整合なので `game_service` 側で早期にゴール判定（12節）を行い、このケースが発生しないようにする

### 12. 正誤判定・ゴール判定

- 正誤判定：`rooms.answer` をカンマ区切りで分割し、各候補・入力文字列の両方を前後空白除去＋小文字化してから比較する（いずれか1つでも一致すれば正解）
  - 正解 → `players.answer_verified = true` に更新し、「正解です！QRコードを読み込んでください」と返信
  - 不正解 → 状態を変更せず「不正解です。もう一度お試しください」と返信（`require_answer_check = true` の場合のみこの分岐に到達する）
- ゴール判定はチェックイン成立時（`/liff/checkin`、後続の指示書で実装）に行う。本節はチェックイン前の会話（正誤判定・ヒント）のみを対象とし、`visited_rooms` への記録・`finished_at` の設定は行わない

---

## 13. `line_client` と Flex Message

- LINE Messaging APIの [Reply API](https://developers.line.biz/) (`POST https://api.line.me/v2/bot/message/reply`) を、Webhook（`/callback`）への応答に使用する。`Authorization: Bearer {LINE_CHANNEL_ACCESS_TOKEN}` ヘッダーを付与する
- LIFFチェックイン（`/liff/checkin`、15節）はLINE Webhookのイベントではなく、LIFFページからの直接のHTTPリクエストであり `replyToken` を持たない。そのため、チェックイン成功後の次の部屋案内・クリア報告は [Push API](https://developers.line.biz/) (`POST https://api.line.me/v2/bot/message/push`、ボディは `{"to": line_user_id, "messages": [...]}`) を使う。Slice A時点では「Push APIは今回のスコープ外」としていたが、Slice B（LIFFチェックイン）でのみ使用する
- 返信メッセージは `game_service` が組み立てる中間表現（例: テキスト／クエスト通知の列挙型）を受け取り、`line_client` がLINEのJSONスキーマに変換して送信する。`game_service` はLINE固有のJSON構造や `reqwest` を一切知らない（DBに依存するロジックをネットワーク呼び出しから切り離し、`sqlx::test` で検証できるようにするため）
- クエスト通知はFlex Message（bubble）を使う。`altText` は必須（プッシュ通知等での代替テキスト）。画像がある部屋は `hero` に `/public/image/{uuid}` の絶対URL（`PUBLIC_BASE_URL` を前置）を設定し、無い部屋は `hero` を省略する
- クエスト通知には `footer` に「QRを読む」ボタン（`action.type = "uri"`、`uri = "https://liff.line.me/{LIFF_ID}"`）を含める。参加者はこのボタンをタップするだけでLIFFページ（15節）を開ける（従来、案内文で「QRコードを読み込んでください」と伝えるだけで実際に開く導線が無く、実運用で参加者がQRスキャンにたどり着けない欠落があったため追加）。`build_quest_flex_message` は `LIFF_ID` を引数として受け取り、URLを組み立てる
- クエスト通知の見た目（本番運用フィードバックを受けて装飾を追加）:
  - `header`: 背景色 `#2E7D32`（緑）のboxに、白文字・太字・小サイズで「次のクエスト」ラベルを表示する
  - `body`: 先頭に状況に応じた「つなぎの文」（`size: sm`・グレー系の文字色`#888888`）を表示し、続けて部屋名を太字・サイズ`xl`で表示、`separator`を挟んでからクエスト文を`size: md`・グレー系の文字色（`#555555`）で表示する
- 「つなぎの文」（`ReplyMessage::Quest`に`intro: String`フィールドを追加し、`game_service`が状況に応じて組み立てる。本番運用フィードバックで「部屋の案内だけでは味気ない・文脈が分かりにくい」との指摘があったため追加）:

  | 状況 | つなぎの文 |
  |:--|:--|
  | 参加登録（名前入力）直後、最初の部屋を案内するとき | 最初の部屋は |
  | QRチェックイン成功後、次の部屋を案内するとき | 【（直前にクリアした部屋名）】クリアおめでとうございます。次の部屋は |
  | 「開始」再送信時、案内済みの現在の部屋を再送するとき（案内を見失った参加者向けの救済） | 現在向かっている部屋は |
  - `hero`（画像がある場合）・`footer`（QRを読むボタン）は既存の構成を維持する
- クリア（15節、`CheckinOutcome::Cleared`）時のメッセージも、従来の平文テキストから専用のFlex Messageに変更する:
  - `CheckinOutcome::Cleared` は `ReplyMessage::Cleared { elapsed: String }` を保持するようになる（`NextQuest(ReplyMessage)` と同じ形に揃える）。`elapsed` は `players.finished_at - players.started_at` を `ranking_service::format_elapsed`（既存のランキング画面向け経過時間フォーマット関数。`M:SS` / `H:MM:SS`）で整形した文字列で、`finished_at` はDBの`NOW()`を待たず、`mark_finished` 呼び出し時点のアプリ側時刻（`chrono::Utc::now()`）から計算してよい（クリア演出用の表示にとどまり、ランキング画面自体はDBの`finished_at`をそのまま使うため、数百ミリ秒程度の差異は実害がない）
  - `header`: 背景色 `#FFC107`（黄）のboxに、白文字・太字・中央寄せ・サイズ`xl`で「🎉 クリア！」を表示する
  - `body`: 「全部屋制覇おめでとうございます！」（太字）、「クリアタイム: {elapsed}」、「最初の部屋にお戻りください。お疲れ様でした！」（小サイズ・グレー系）の3行
  - `ranking_service::format_elapsed` は現状privateな関数だが、`game_service` からも呼べるよう可視性を `pub(crate)` に変更する（`ranking_service`・`game_service`間の重複実装を避けるため）
- JSON組み立て関数（例: `build_text_message` / `build_quest_flex_message`）は純粋関数として実装し、実際にLINEへ送信する関数（`reqwest`を使う）と分離する。前者のみ自動テストの対象とし、後者（実ネットワーク呼び出し）は `AGENTS.md` の `sqlx::test` DB接続と同様、この開発環境ではテスト対象外とする（ネットワーク到達性が無いため）
- 送信（`reqwest`呼び出し）が失敗しても、Webhookハンドラーは200を返す（8節）。送信失敗はログに記録するのみで、参加者側の状態（`players`・`visited_rooms`）は既に確定しているため、Webhook自体を失敗扱いにしない。`/liff/checkin` のPush送信失敗も同様に、DB状態は既に確定しているためログ記録のみとし、レスポンス自体は成功として返す

---

## 14. 環境変数（LINE連携で追加）

| 変数 | 内容 | 起動時未設定の挙動 |
|:--|:--|:--|
| `LINE_CHANNEL_SECRET` | Webhook署名検証用のチャネルシークレット | `DATABASE_URL`と同様、エラー出力してプロセス終了 |
| `LINE_CHANNEL_ACCESS_TOKEN` | Messaging API呼び出し用のアクセストークン | 同上 |
| `PUBLIC_BASE_URL` | 画像URL等を組み立てる際に前置する公開ベースURL（末尾スラッシュなし、例 `https://xxxx.koyeb.app`） | 同上 |
| `LIFF_ID` | LIFFページで `liff.init({ liffId })` に渡すLIFF ID（15節） | 同上（Slice Bから必須化） |
| `LINE_LOGIN_CHANNEL_ID` | IDトークン検証エンドポイントに渡す `client_id`（LIFFアプリが属するチャネルのID。15節） | 同上（Slice Bで追加） |

---

## 15. LIFFチェックイン（`/liff/checkin`）とゴール判定（Slice B）

### IDトークン検証

- LIFFページは `liff.getIDToken()` で取得したIDトークン（JWT）と、`liff.scanCodeV2()` で読み取った `qr_uuid` を `POST /liff/checkin` に送信する
- サーバは受け取ったIDトークンをそのまま信用せず、LINEの検証エンドポイント `POST https://api.line.me/oauth2/v2.1/verify`（`application/x-www-form-urlencoded`、`id_token` と `client_id`（`LINE_LOGIN_CHANNEL_ID`）を送信）に問い合わせる。有効なら応答JSONの `sub` がLINEユーザーIDとなる。無効・期限切れ・`aud`不一致等でエラー応答の場合は401とし、以降の処理を行わない
- レスポンスJSONのパース（`sub` の抽出）は `line_client` 内の純粋関数として切り出し、自動テストの対象とする。実際のネットワーク呼び出し自体は13節と同様にテスト対象外とする
- テスト容易性のため、`AppState` に `verify_id_tokens: bool`（本番は常に`true`固定）を持たせ、テスト時のみ`false`にして「リクエストの `id_token` フィールドの値をそのままLINEユーザーIDとして扱う」経路に切り替えられるようにする（Slice Aの `send_line_replies` と同じ考え方のテスト用フック）

### `game_service::checkin` の判定順序

1. `qr_uuid` に対応する部屋が存在しない → 拒否（`room_not_found`）
2. 呼び出し元の（IDトークンから得た）LINEユーザーIDに対応する `players` 行が無い → 拒否（`not_registered`）
3. `players.finished_at` が設定済み → 拒否（`already_finished`）
4. 部屋の `id` が `players.current_room_id` と一致しない → 拒否（`wrong_room`。案内されていない部屋のQRは常に無効）
5. `require_answer_check` がtrueかつ `players.answer_verified` がfalse → 拒否（`answer_not_verified`）
6. すべて満たせば `visited_rooms` に記録する
7. 記録後の訪問数が、そのイベントに登録済みの部屋数（`room_repository::count`。運用上は15だが、登録数に依存させることで実際の登録数がそれ未満の場合にも正しく動作する）に達した場合、`players.finished_at` を記録し「クリア」を表す結果を返す
8. 達していない場合、未訪問の部屋からランダムに1部屋選出し（11節と同じ関数）、`current_room_id` を更新して次のクエストを表す結果を返す

### レスポンス設計

- チェックインの成否に関わらず、`/liff/checkin` 自体のレスポンスはLIFFページ表示用の最小限のJSON（`{"status": "next"}` / `{"status": "cleared"}` / `{"status": "rejected", "reason": "..."}`）のみとする。次の部屋のクエスト文・画像やクリア報告といった実際の案内内容は、常にLINEチャット側にPush Messageとして送る（LIFFページ自体にFlex Message相当の表示ロジックを持たせない。案内内容を`game_service`/`line_client`に一本化するため）
- HTTPステータス: 成功系は200。`room_not_found` は404。その他の拒否理由（`not_registered`/`wrong_room`/`answer_not_verified`/`already_finished`）は403

### LIFFページ（`GET /liff/checkin`）

- 認証不要（LIFFのIDトークンによる検証は`POST`側で行う）。LINEアプリ内ブラウザ（またはLIFFの外部ブラウザモード）で開かれる想定
- LINEのLIFF SDK（`https://static.line-scdn.net/liff/edge/2/sdk.js`）を読み込み、`liff.init({ liffId: LIFF_ID })` の後、「QRを読む」ボタンから `liff.scanCodeV2()` を呼び出す
- 画面上はチェックイン結果（成功/クリア/エラー理由）を簡潔に表示するのみで、クエストの詳細はLINEチャットを確認するよう促す
- 失敗時（`status: "rejected"`）は、`reason` の値ごとにメッセージを出し分ける（当初は理由によらず単一の汎用メッセージだったが、特に `wrong_room`（案内されていない部屋のQR）で「なぜ失敗したか」が分からず参加者が混乱したため、本番運用時のフィードバックを受けて出し分けに変更した）

  | `reason` | 表示メッセージ |
  |:--|:--|
  | `wrong_room` | このQRコードはご案内している部屋のものではありません。LINEチャットで案内されている部屋をご確認ください。 |
  | `already_finished` | 既に全部屋クリア済みです。 |
  | `not_registered` | 参加登録が完了していません。LINEで「開始」と送信してください。 |
  | `answer_not_verified` | 先にLINEで正解を送信してから、QRコードを読み込んでください。 |
  | `room_not_found` | 無効なQRコードです。もう一度お試しください。 |
  | `invalid_id_token` | 認証に失敗しました。時間をおいてもう一度お試しください。 |

- チェックイン結果（成功・クリア・拒否のいずれか）を受け取った後、「LINEチャットに戻る」ボタンを表示する。タップすると LIFF SDK の `liff.closeWindow()` を呼び、LIFFブラウザを閉じてLINEのトーク画面に戻る（本番運用フィードバックで「QR読み込み後の次の操作が分からない」との指摘があったため追加。結果表示前は非表示。「QRを読む」ボタンは結果表示後も残し、再スキャンできるようにする）

---

## 16. イベント設定画面（`/admin/settings`、Slice C）

- 個人戦/チーム戦（`events.is_team_mode`）・判定モード（`events.require_answer_check`）の2項目のみを切り替える。`event_name`の編集は`docs/requirements.md`の要件に含まれないため、本画面のスコープ外とする
- `events`はシングルトン運用のため、`event_service::current(pool)`で唯一の行を取得し、`event_service::update_settings(pool, input)`で更新する（`room_service::current_event`と同種のラッパー。なお`room_service`/`handlers::rooms.rs`側も`room-management-fixes-2`で`event_repository::find_singleton`の直接呼び出しを`room_service::current_event`経由に統一済み）
- HTMLの`<input type="checkbox">`はチェックが外れているとフィールド自体がフォームデータに含まれない。Axumの`Form`抽出でこれを扱うため、対応するリクエスト構造体のbool項目には`#[serde(default)]`を付与し、「送信されていない＝false」として扱う。また、チェックが入っている場合HTMLは値`"on"`を送信するが、`serde_urlencoded`のbool型デシリアライザは`"true"`/`"false"`しか受け付けないため、`#[serde(default)]`だけでは不十分（送信時に422になる）。`"on"`/`"true"`を`true`として扱うカスタム`deserialize_with`関数を併用すること（`settings-checkbox-fix`で対応）
- 設定変更は既存データを一切書き換えない（例: `require_answer_check`を`false`に切り替えても、既存の`rooms.answer`/`hint_msg`はそのままDBに残る。`game_service`は常にイベントの現在の`require_answer_check`を見て使うかどうかを判断するため、未使用の値が残っていても実害はない。切替のたびに既存部屋のデータを消去・追従させるような処理は行わない）
- 既存の参加者（`players`）・進行状況への影響も特に考慮しない（設定変更は次回の判定・部屋案内から反映される程度で十分とする。要件上、運用中の切替を想定していないため）
- フォームは`admin/_base.html`を継承し、既存の`csrf_service`（ダブルサブミット）を再利用する。ナビゲーションに「設定」リンクを追加する

---

## 17. ランキング画面（`/admin/ranking`、Slice D）

- クリアタイム順（`finished_at - started_at`の所要時間が短い順）にランキングを表示する。**絶対時刻の`finished_at`が早い順ではない**（参加者ごとに参加登録のタイミングが異なるため、所要時間で比較する）
- 未クリアの参加者（`finished_at IS NULL`）は要件通り「圏外」として順位を付けず、ランキング表とは別のセクションに一覧表示する（一切表示しないのではなく、順位無しで参加登録日時の昇順に並べる。運営が「誰がまだ回っているか」を把握できるようにするため）
- 「リアルタイム」表示は、WebSocket/自動ポーリング等は導入せず、ページを読み込むたびにDBの最新状態を反映するという意味で満たす（他の管理画面同様、キャッシュを挟まないサーバーサイドレンダリング。自動更新が必要になった場合は将来の拡張とする）
- レイヤー構成: `player_repository::find_all_by_event(pool, event_id)` で対象イベントの全参加者を取得し、`ranking_service::build_ranking(players)` という**DBに依存しない純粋関数**で「クリア済み（順位・所要時間つき）」と「未クリア」に振り分けてソートする（sqlx::testを使わず通常の単体テストで検証できるようにするため）。`ranking_service::get_ranking(pool, event_id)`はこの2つを組み合わせるだけの薄いラッパーとする
- 所要時間の表示形式は `M:SS`（1時間以上は `H:MM:SS`）とし、フォーマット関数もDB非依存の純粋関数として実装する
- 同着（所要時間が同一）の場合の特別な順位表記（同着1位を2件表示する等）は行わず、単純に到達順で連番を振る
- `GET /admin/ranking` は他の管理画面と同様 `require_admin` 配下、状態変更を伴わないため CSRF は不要
- ナビゲーションに「ランキング」リンクを追加する

---

## 18. デプロイ構成（Koyeb）

### ビルド方式: 本番専用Dockerfile（マルチステージビルド）

- `.devcontainer/Dockerfile` はVS Code Dev Containers向け（`sqlx-cli`・clippy等を含む開発用の大きめイメージ）であり、本番デプロイには使わない。Koyebの無料枠インスタンス（リソース制約がある）に適したイメージにするため、リポジトリルートに**本番専用の`Dockerfile`**をマルチステージビルドで実装済み（#12）
  - ビルドステージ: `rust:1-bookworm`で`cargo build --release --locked`（`--locked`は`Cargo.lock`との不整合による意図しない依存関係更新を防止する。`SECURITY.md`「依存関係・再現性」の方針に対応）
  - 実行ステージ: `debian:bookworm-slim`に、ビルドステージで生成したバイナリと`ca-certificates`（`reqwest`のTLS通信に必要）のみをコピーする。ソースコード・`target/`の中間成果物は実行イメージに含めない。非rootユーザー（`appuser`）でアプリケーションを起動する（多層防御）
- Koyebの当該Webサービスに、このDockerfileを使ってビルド・デプロイするよう設定する（Koyebのgit連携でリポジトリを指定し、Dockerfileベースのビルドを選択する）
- ヘルスチェックパス: `/health`（Koyebのヘルスチェックに登録。認証不要・DB非依存のエンドポイントなので疎通確認に適する）

### リッスンポート: `PORT` 環境変数を読む

- Koyeb（および多くのPaaS）は、動的に割り当てた/設定したポート番号を`PORT`環境変数でアプリに伝える方式を採ることが多い。`main.rs`の`resolve_port`関数が`PORT`環境変数をパースし、未設定・パース失敗時は8000にフォールバックする形で実装済み（#12）。バインド処理（`SocketAddr::from(([0, 0, 0, 0], port))`）もこの値を使う

### 本番DB: TiDB Serverless

- 本番DBは既存の別アプリと同じTiDB Cloudアカウント・同じクラスタを共用する（クラスタ単位ではなく**データベース単位**で分離する）。このアプリ専用のデータベース（`stamprally`）と、そのデータベースにのみ権限を持つ専用DBユーザーを作成し、発行される接続文字列（TLS必須）を`DATABASE_URL`に設定する。既存アプリとの分離方針・パブリックエンドポイントであることの受容リスクの詳細は [SECURITY.md](../SECURITY.md)「本番DB（TiDB Serverless）の接続方針」を参照
- **起動時の自動マイグレーションは行わない**（`main.rs`に`sqlx::migrate!()`の呼び出しは無く、意図的にその設計を踏襲する）。スキーマ変更を含むリリースでは、developerが手元から本番の`DATABASE_URL`を指定して`sqlx migrate run`を実行し、反映を確認してからコードをデプロイする運用とする（自動化は対象規模（1建物・1イベント運用）に対して過剰と判断）

### 環境変数

Koyebの Environment Variables（Secrets）に以下を設定する。値はKoyebダッシュボードから個別に入力し、リポジトリには含めない。

| 変数 | 本番での値の目安 |
|:--|:--|
| `DATABASE_URL` | TiDB Serverlessの接続文字列（`stamprally`専用データベース・専用ユーザー） |
| `LINE_CHANNEL_SECRET` / `LINE_CHANNEL_ACCESS_TOKEN` | LINE Developersコンソール（Messaging APIチャネル）で発行 |
| `PUBLIC_BASE_URL` | KoyebのURL（例 `https://xxxx.koyeb.app`。独自ドメインを割り当てる場合はそちらを設定） |
| `LIFF_ID` | LINE Developersコンソール、LIFFアプリ登録用に別途作成した**LINEログインチャネル**（Messaging APIチャネルとは別物。Messaging APIチャネルへのLIFF直接追加は廃止されているため）で発行 |
| `LINE_LOGIN_CHANNEL_ID` | 上記LINEログインチャネル自体のチャネルID（Messaging APIチャネルのIDとは異なる） |
| `ADMIN_PASSWORD` | 初回起動（`events`が空の時）のみ使用される、管理者ログイン用の初期パスワード |

### セッションストア・Cookie

- Koyeb無料枠（Ecoインスタンス）は最小インスタンス数を0に固定することができず、アイドル時にインスタンス数0へスケールダウンする（実際のデプロイ作業で確認済み）。**常時起動は保証されない**前提で設計する
- セッションストアは`MemoryStore`（プロセス内メモリ）のまま運用する。スケールダウン（インスタンス再起動）が発生すると管理者のログインセッションは失われるが、影響は管理者が再ログインするだけで実害は小さいため許容する（管理者アカウントは1つのみ、同時アクセスも稀という前提。[docs/requirements.md](requirements.md)4節）
- 一方、LINE Botの参加登録の一時状態（「開始」〜名前入力までの間の状態）は、スケールダウンで失われるとプレイヤー体験に実害があるため、**DBに永続化する**方針に変更した（9節を参照）
- 0-1の範囲を超える複数インスタンスへの水平スケールは引き続き想定しない（対象規模から見合わないため）。0-1の範囲でのスケールtoゼロ・コールドスタートのみを許容する
- セッションCookieの`Secure`属性は`tower-sessions`のデフォルト（`true`）のまま変更不要（[main.rs](../src/main.rs)で`.with_secure(false)`等の上書きをしていないことを確認済み）。本番はKoyebがTLS終端するHTTPS配信のため問題なく送信される。ローカル開発（`http://localhost`）でも動作に支障はない（`localhost`はブラウザ・主要HTTPクライアントから「trustworthy origin」として扱われ、Secure Cookieの送受信が許可されるため）

### LINE Developers コンソール側の設定（Koyeb URL確定後に反映）

- Messaging APIチャネルのWebhook URLを `https://<Koyebドメイン>/callback` に設定し、Webhookの利用をONにする
- LIFFアプリのエンドポイントURLを `https://<Koyebドメイン>/liff/checkin` に設定する

---

## 19. 管理画面ダッシュボード（`/admin/dashboard`）

- `GET /admin/dashboard` は管理者認証（#3）実装時に「保護ミドルウェアの動作確認用プレースホルダー」（`"ok"`を返すのみ）として仮実装されたまま、後続の各機能スライス（設定・部屋管理・ランキング）でも中身を実装するタスクが積み残しになっていた。[docs/operator-guide.md](operator-guide.md)2節は当初からこのページが備えるべき内容として以下3セクションを記載しており、実装をこの記載に合わせる
- レイヤー構成: 既存の`event_service::current`（イベント設定取得）と`room_service::list`（部屋一覧取得。件数は戻り値の`Vec`の長さで足りるため、件数専用の別クエリは発行しない）を組み合わせるだけの薄いハンドラーとする。新規のservice関数は不要
- 表示内容（[docs/operator-guide.md](operator-guide.md)2節と対応）:
  1. **イベント設定状況**: `is_team_mode`（個人戦/チーム戦）・`require_answer_check`（判定モード）の現在値を表示し、`/admin/settings`へのリンクを設置する
  2. **部屋一覧**: 登録済み部屋数（`room_service::list`の件数、上限15との対比が分かる形。例:「3 / 15部屋」）を表示し、`/admin/rooms`（部屋ごとの編集・QR表示はそちらに既存の導線がある）へのリンクを設置する。ダッシュボード自体に部屋ごとの個別リンクを複製する必要はない（一覧の二重管理を避けるため）
  3. **ランキング**: `/admin/ranking`へのリンクを設置する（ダッシュボード側でランキングデータ自体は取得・表示しない。既存の`/admin/ranking`が詳細表示を担う）
- 他の管理画面同様`admin/_base.html`を継承し、ナビゲーション（部屋管理・設定・ランキング）とログアウトフォームを共有する。ログアウトフォームが要求する`csrf_token`をハンドラーで発行する（他のGET専用管理画面ハンドラー、例: `ranking`と同じパターン）
- 状態変更を伴わない画面のため、ダッシュボード自体のCSRF保護は不要（共有レイアウトのログアウトフォームのCSRF検証は既存の`/auth/logout`ハンドラー側で行われる）

## 20. 管理画面デザインシステム

本番運用フィードバックで、管理画面（`/admin/*`・`/auth/login`）がBootstrap 5の既定スタイルのままで視認性・ブランドの一貫性に欠けるとの指摘があった。以下の方針で軽量なデザイントークンを導入する。

### 実装方針

- 新規の静的ファイル配信基盤（`/static`ルート等）は追加しない。既存の`admin/_base.html`（各管理画面が継承）と`auth/login.html`（単独ページ）それぞれの`<head>`内に`<style>`ブロックとしてCSSカスタムプロパティ・ユーティリティクラスをインラインで定義する
- Bootstrap 5.3のCSSカスタムプロパティ（`--bs-primary`・`--bs-body-bg`等）を上書きする形でテーマを適用し、Bootstrapのコンポーネント・グリッド自体は引き続き利用する（独自CSSフレームワークへの置き換えは行わない）
- 新規Webフォント（Google Fonts等）は追加しない。OSの日本語フォントを優先する`font-family`スタック（`-apple-system, BlinkMacSystemFont, "Segoe UI", "Hiragino Kaku Gothic ProN", "Hiragino Sans", Meiryo, sans-serif`）に置き換えるのみとし、外部リクエストを増やさない
- 対象範囲は管理者向け画面（`/admin/*`・`/auth/login`）のみ。プレイヤー向け`/liff/checkin`は対象外（13節・15節の設計に基づき別途スタイリング済みのため）
- 新規DBクエリ・新規ルート・新規ハンドラー引数は追加しない。既存のAskamaテンプレート構造体（`DashboardTemplate`等）が受け取るフィールドは変更しない。ダッシュボードの部屋登録進捗（プログレスバー）のような派生値は、テンプレート内の算術式（例: `room_count * 100 / 15`）で完結させ、ハンドラー側に新しいフィールドを追加しない

### カラートークン

| トークン | 値 | 用途 |
|:--|:--|:--|
| `--admin-primary` | `#B54B3A` | プライマリボタン・ブランドアクセント（スタンプのインクをイメージした朱色系） |
| `--admin-primary-hover` | `#973C2E` | プライマリボタンのホバー・アクティブ状態 |
| `--admin-primary-soft` | `#F3E3DF` | バッジ等の淡いアクセント背景 |
| `--admin-bg` | `#F4F5F7` | ページ背景 |
| `--admin-surface` | `#FFFFFF` | カード・ナビゲーションバー背景 |
| `--admin-border` | `#E2E4E9` | カード・テーブルの罫線 |
| `--admin-text` | `#1F2328` | 本文・見出しの文字色 |
| `--admin-text-muted` | `#6B7280` | 補助テキスト（ラベル・サブタイトル） |
| `--admin-success` | `#2E9E6D` | ランキング1位バッジの文字色 |
| `--admin-success-soft` | `#E1F3EA` | ランキング1位バッジの背景 |
| `--admin-radius-card` | `12px` | カードの角丸 |
| `--admin-radius-control` | `8px` | ボタン・入力欄の角丸 |

外部から取り込んだ`DESIGN.md`（claude.com向けのAnthropicブランドデザイントークン）を参考にする案が出たが、配色（コーラル`#cc785c`）・専用書体（Copernicus/StyreneB）・スパイクマーク等はAnthropic固有のブランド識別要素であり、第三者プロダクトでの流用はブランドの誤認を招くリスクがあるため採用しない。上記トークンはStampRallyBot独自の配色として新規に定義したものであり、`DESIGN.md`の値を転用していない。

### コンポーネント

- **stat-card**: ダッシュボードの集計カード（部屋登録進捗・イベント設定・ランキード導線の3枚）。`--admin-surface`背景・`--admin-radius-card`角丸・1px罫線
- **badge-mode**: イベント設定状況（個人戦/チーム戦・判定モード）を表すピル状バッジ。`--admin-primary-soft`背景
- **badge-rank-first**: ランキング1位の行を強調するバッジ。`--admin-success-soft`背景
- **page-header**: 各画面共通の見出し＋補足テキストのパターン（`<h1>`＋`.page-subtitle`）
- テーブルヘッダー（`thead th`）は大文字・グレー系の補助テキスト色に統一し、Bootstrap既定の縞模様よりも罫線ベースの落ち着いた表現にする

### 適用範囲外・非対象

- 認証ロジック・CSRF・DBクエリ・ルーティングへの変更はなし（純粋に表示レイヤーの変更）
- `admin/_base.html`のログアウトフォーム（`action="/auth/logout"`・`csrf_token`）の構造は変更しない（`src/handlers/rooms.rs`の既存テスト`room_templates_include_logout_csrf_token`がこの構造に依存しているため）

## 21. DB接続・外部API呼び出しのタイムアウト

本番運用中、参加者が「開始」→チーム名入力（`pending_registrations`からの`players`行作成・部屋割当を含む一連のDBアクセス、10節参照）を行った際にBotが無反応になる障害が発生した。調査の結果、以下が判明した。

- `MySqlPoolOptions::new().connect(...)`（`main.rs`）にタイムアウト関連のオプションを一切設定していない
- `reqwest::Client::new()`（`AppState::new`内）にもリクエスト全体のタイムアウトを設定していない
- sqlxの`test_before_acquire`（既定で有効）はコネクション取得前にpingを行うが、pingも同じ非同期の読み取りに依存するため、コネクションが「エラーを返さず応答も返ってこない」状態（TiDB Serverless側がTCP接続を無応答のまま内部的に破棄した場合など）になっていると、ping自体も無期限にハングしうる。したがって`idle_timeout`・`max_lifetime`のチューニングだけでは、この種のハングを確実には防げない

この状態だと、Webhook（`/callback`）・LIFFチェックイン（`/liff/checkin`）いずれの処理でも、DBアクセスやLINE API呼び出しがハングした場合、そのリクエスト処理タスクが完了せず、参加者への応答が返らないままエラーログすら残らない（8節・13節の「エラーはログに記録し処理を継続する」という既存方針は、処理が“エラーで終わる”ことを前提としており、“ハングして終わらない”ケースをカバーできていなかった）。加えて、ハングしたリクエストが確保したDBコネクションを返却しないまま滞留すると、コネクションプールの上限（既定10）に達し、後続の別参加者のリクエストもコネクション取得待ちでハングする、事実上の全面停止に発展しうる。

### 対策方針

1. **`reqwest::Client`にリクエスト全体のタイムアウトを設定する**（`.timeout(std::time::Duration::from_secs(10))`）。LINEの各API呼び出し（reply / push / IDトークン検証）はいずれも通常数百ms程度で完了するため、10秒は十分に余裕を持った上限とする
2. **Webhook（`/callback`）・LIFFチェックイン（`/liff/checkin`）それぞれのハンドラーで、DBアクセスを伴う本体処理（`game_service::handle_text_message` / `game_service::checkin`）全体を`tokio::time::timeout(std::time::Duration::from_secs(15), ...)`でラップする**
   - タイムアウトした場合は、既存のエラー処理経路と同じ扱いにする：`/callback`はログに記録した上で当該イベントの処理を打ち切り、他のイベント処理は継続し、レスポンス自体は200を返す（8節の方針を維持）。`/liff/checkin`はログに記録した上で500を返す（既存の`GameServiceError`のハンドリング、`handlers/liff.rs`と同じ扱い）
   - タイムアウトは新設の`game_service::GameServiceError::Timeout`として扱い、既存の`Database`バリアントと同様に`tracing::error!`でログに記録する
   - futureがタイムアウトでdropされることで、そのfutureが保持していたDBコネクションガードも解放される（プールに滞留し続けることを防ぐ）
3. 上記のいずれも、1件のリクエスト処理の失敗が他の参加者の処理に波及しない、という既存方針（8節・13節）を崩さない。上記のタイムアウト値（10秒・15秒）は初期の見積もりであり、本番の実測レイテンシに応じて今後調整してよい

### 追記: `/callback`はさらにレスポンスの即時返却が必要だった

上記1〜3を実装・デプロイした後も、本番で同じ操作（「開始」→チーム名入力）を行うと参加者への応答が返らない事象が再発した。調査の結果、以下が判明した。

- サーバのログには、DBコネクションが壊れたことを示す記録（`ping on idle connection returned error`等）はあるが、`game_service::with_db_call_timeout`がタイムアウトした際に出るはずの`failed to handle LINE text message`ログが1分待っても一切出力されていなかった
- LINE Developers Consoleの「Webhookのエラー統計」を有効化して同じ操作を再試行したところ、`request_timeout`（LINEプラットフォーム自身がWebhookレスポンスを待ちきれずタイムアウトした）が記録されていることを確認した

これは、`/callback`のレスポンス（署名検証→`game_service`呼び出し→LINEへの返信送信→200を返す、という一連の同期処理）に要する時間が、**LINEプラットフォーム自身がWebhookの応答を待つ時間（LINE側のクライアントタイムアウト）よりも長くなりうる**ことが原因である。今回追加した15秒のDB呼び出しタイムアウトは、あくまで自サーバ内の話であり、LINE側のタイムアウトの方が短ければ、こちらが15秒待ってエラーを検知する前に、LINE側は既に配信失敗と判断して待つのをやめている。この場合、Webhookレスポンス自体（Botの視点では最終的に200を返している）がLINE側に届いても意味がなく、参加者には何も届かない。

**対策**: 8節に記載の通り、`/callback`は署名検証・JSONパースが完了した時点で即座に200を返し、イベントごとの実処理（`game_service`呼び出し・LINEへの返信送信）は`tokio::spawn`によるバックグラウンドタスクとして切り離す。これにより、Webhookレスポンスの所要時間は署名検証・JSONパース（通常マイクロ秒〜ミリ秒オーダー）のみに依存するようになり、DBやLINE API呼び出しがどれだけ遅延しても（今回のような接続断も含め）LINE側のタイムアウトに引っかかることが構造的になくなる。1〜3で設定したタイムアウト値（10秒・15秒）自体は、バックグラウンドタスクがDBコネクションプールを無期限に占有し続けることを防ぐ安全装置として引き続き有効であり、変更しない。

**`/liff/checkin`は対象外**: この問題はLINEプラットフォームがWebhookの呼び出し元（HTTPクライアント）として振る舞う`/callback`に固有のものである。`/liff/checkin`の呼び出し元はLIFFページ自身のブラウザ（`fetch`）であり、LINEプラットフォームのような固定の応答待ちタイムアウトを持たない。また`/liff/checkin`のレスポンス自体がLIFFページの表示（成功/クリア/拒否）に必要なため、`/callback`と同様に「即座に200を返してバックグラウンド処理に切り離す」設計は適用できない（有効な応答内容を返す前にレスポンスを返してしまうと、LIFFページが正しい結果を表示できなくなる）。`/liff/checkin`は1〜3で設定した15秒のDB呼び出しタイムアウトのままとする。

---
