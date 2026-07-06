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
| ホスティング | Render |

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
| 認証 | `/auth/*` | 管理者ログイン・ログアウト |
| 管理画面 | `/admin/*` | 部屋管理・QRコード発行・設定・ランキング閲覧 |
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
  - `AppState { pool: MySqlPool, line_channel_secret: Arc<str>, line_channel_access_token: Arc<str>, public_base_url: Arc<str>, pending_registrations: PendingRegistrations }`
  - 既存ハンドラーは `State<MySqlPool>` を使い続けられるよう、`impl FromRef<AppState> for MySqlPool`（`state.pool.clone()` を返す）を実装し、既存シグネチャを変更しない
  - LINE Webhook・画像配信ハンドラーは必要に応じて `State<AppState>` や `State<Arc<str>>`（`FromRef` 経由）を使う
- `LINE_CHANNEL_SECRET` / `LINE_CHANNEL_ACCESS_TOKEN` / `PUBLIC_BASE_URL` は `DATABASE_URL` と同様、起動時に未設定ならエラー出力してプロセスを終了する（`.env.example` に追記済み）

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

---

## 9. 会話状態管理（参加登録の一時状態）

- 「開始」送信後、名前（個人戦）／チーム名（チーム戦）の入力を待つ「登録待ち」状態が必要になるが、`players` 行は名前確定まで作成しない（`player_name` はNOT NULLのため）
- 管理者セッション（`tower_sessions::MemoryStore`）と同じ考え方で、**この一時状態もDBに永続化せずアプリ内メモリで保持する**
  - 理由: 「開始」から名前入力までは数秒〜数十分の短時間の状態であり、プロセス再起動で失われても参加者は「開始」を送り直すだけで復帰できる（実害が小さい）。DBテーブルを新設するコストに見合わない
  - 実装: `AppState.pending_registrations: Arc<Mutex<HashSet<String>>>`（キーはLINEユーザーID）。「開始」受信時に挿入、名前受信で消費（登録処理後に削除）、「リセット」受信時にも削除する
- 将来複数イベント運用に拡張する場合はこの节の設計を見直す（今回は `events` が1行のみのため、どのイベントの登録待ちかを気にする必要がない）

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
- JSON組み立て関数（例: `build_text_message` / `build_quest_flex_message`）は純粋関数として実装し、実際にLINEへ送信する関数（`reqwest`を使う）と分離する。前者のみ自動テストの対象とし、後者（実ネットワーク呼び出し）は `AGENTS.md` の `sqlx::test` DB接続と同様、この開発環境ではテスト対象外とする（ネットワーク到達性が無いため）
- 送信（`reqwest`呼び出し）が失敗しても、Webhookハンドラーは200を返す（8節）。送信失敗はログに記録するのみで、参加者側の状態（`players`・`visited_rooms`）は既に確定しているため、Webhook自体を失敗扱いにしない。`/liff/checkin` のPush送信失敗も同様に、DB状態は既に確定しているためログ記録のみとし、レスポンス自体は成功として返す

---

## 14. 環境変数（LINE連携で追加）

| 変数 | 内容 | 起動時未設定の挙動 |
|:--|:--|:--|
| `LINE_CHANNEL_SECRET` | Webhook署名検証用のチャネルシークレット | `DATABASE_URL`と同様、エラー出力してプロセス終了 |
| `LINE_CHANNEL_ACCESS_TOKEN` | Messaging API呼び出し用のアクセストークン | 同上 |
| `PUBLIC_BASE_URL` | 画像URL等を組み立てる際に前置する公開ベースURL（末尾スラッシュなし、例 `https://example.onrender.com`） | 同上 |
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

---

## 16. イベント設定画面（`/admin/settings`、Slice C）

- 個人戦/チーム戦（`events.is_team_mode`）・判定モード（`events.require_answer_check`）の2項目のみを切り替える。`event_name`の編集は`docs/requirements.md`の要件に含まれないため、本画面のスコープ外とする
- `events`はシングルトン運用のため、`event_service::current(pool)`で唯一の行を取得し、`event_service::update_settings(pool, input)`で更新する（`room-management-fixes`で提案した`room_service::current_event`と同種のラッパーだが、既存の`room_service`/`handlers::rooms.rs`が`event_repository::find_singleton`を直接呼ぶ既存実装への遡及的なリファクタリングは本スライスのスコープ外とする。新設する`event_service`内で完結させる）
- HTMLの`<input type="checkbox">`はチェックが外れているとフィールド自体がフォームデータに含まれない。Axumの`Form`抽出でこれを扱うため、対応するリクエスト構造体のbool項目には`#[serde(default)]`を付与し、「送信されていない＝false」として扱う
- 設定変更は既存データを一切書き換えない（例: `require_answer_check`を`false`に切り替えても、既存の`rooms.answer`/`hint_msg`はそのままDBに残る。`game_service`は常にイベントの現在の`require_answer_check`を見て使うかどうかを判断するため、未使用の値が残っていても実害はない。切替のたびに既存部屋のデータを消去・追従させるような処理は行わない）
- 既存の参加者（`players`）・進行状況への影響も特に考慮しない（設定変更は次回の判定・部屋案内から反映される程度で十分とする。要件上、運用中の切替を想定していないため）
- フォームは`admin/_base.html`を継承し、既存の`csrf_service`（ダブルサブミット）を再利用する。ナビゲーションに「設定」リンクを追加する
