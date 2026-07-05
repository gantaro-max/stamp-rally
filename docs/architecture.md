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

- Cookieベースのセッション（`tower-sessions` 等を利用）
- 管理者ログインのみ（プレイヤーはLINEアカウントのみで識別、Webログイン不要）
- CSRF保護（`/callback` と `/liff/checkin` は除外）

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

- 部屋登録時に `qr_uuid` を発行し、`qrcode` crate でQR画像を生成
- 管理画面（`/admin/rooms`）で部屋ごとのQR画像を表示・印刷用に出力し、スタッフが現地で保持・提示する
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
- アップロード時に `image` crateで800px幅・JPEG 80%品質にリサイズ
