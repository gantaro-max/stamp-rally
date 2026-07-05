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
- LIFFアプリの「QRを読む」ボタンから `liff.scanCodeV2()` を呼び出し、読み取ったUUIDを `/liff/checkin` にPOST
- サーバ側の検証項目：
  1. UUIDが有効な部屋のものか
  2. そのプレイヤーの `current_room_id` と一致するか（案内された部屋以外は無効）
  3. `require_answer_check` がtrueのイベントでは `answer_verified` がtrueか
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
- 判定・デコードに成功した画像のみリサイズして保存する

---

## 7. 部屋（チェックポイント）管理の実装方針

- 新規登録時、既存の部屋数が15件に達している場合は登録を拒否する（イベントあたり最大15部屋。`docs/requirements.md` 参照）
- `require_answer_check`（判定モード）は `events` の該当イベント1行を参照して判定する
  - `true` の場合のみ `answer`（正解）を必須項目として扱う。`hint_msg` は任意
  - `false` の場合、フォームに正解・ヒント欄を表示しない。仮に送信されても `answer` / `hint_msg` は保存せず常にNULLとする（クライアントの申告を信用しない）
- 画像を伴う登録・更新は `multipart/form-data` で受け取る。画像が添付されていない場合は画像なしで登録できる（`docs/requirements.md`：画像は任意）
- 部屋の画像を更新する場合、新しい `room_images` 行を作成して `rooms.image_id` を張り替え、更新前に参照されていた `room_images` 行は削除する（孤立データを残さない）
- 部屋を削除する場合、`rooms` 行の削除に合わせて、参照していた `room_images` 行も削除する（`visited_rooms` は既存の `ON DELETE CASCADE` で自動的に削除される）
- 部屋一覧・登録・編集・削除・QR表示はすべて `/admin/*` 配下（`require_admin` 済み）。フォームは既存の `csrf_service`（セッション格納トークンとのダブルサブミット）を再利用する
- 管理画面のAskamaテンプレートは `templates/admin/_base.html` を共通レイアウト（Bootstrap 5のナビゲーション等）として `{% extends %}` で利用する。今後追加する設定画面・ランキング画面もこのレイアウトに乗せる（`templates/auth/login.html` はログイン前の独立画面のため対象外のまま）
