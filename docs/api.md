# API エンドポイント設計 — StampRallyBot

認証方式: Cookieベースのセッション（管理者ログインのみ。プレイヤーはLINEアカウントで識別しWebログイン不要）

---

## 疎通確認 (`handlers::health`)

| メソッド | パス | 説明 | 認証 |
|:--|:--|:--|:--|
| GET | `/health` | 疎通確認用。常に `200 OK`（本文 `ok`）を返す | 不要 |

---

## 認証 (`handlers::auth`)

管理者は1アカウントのみを想定（自己登録フォームは設けない。初期パスワードは環境変数/シードで設定）。

| メソッド | パス | 説明 | 認証 |
|:--|:--|:--|:--|
| GET | `/auth/login` | ログイン画面 | 不要 |
| POST | `/auth/login` | ログイン処理。同一送信元からの失敗が5回に達すると15分間 `429 Too Many Requests`（`Retry-After` 付き）を返す（[architecture.md 25節](architecture.md#25-管理者ログインのレート制限post-authlogin)） | 不要 |
| POST | `/auth/logout` | ログアウト（POSTのみ） | 必要 |

---

## 管理画面 (`handlers::admin`)

すべて管理者セッションが必要。

| メソッド | パス | 説明 |
|:--|:--|:--|
| GET | `/admin/dashboard` | ダッシュボード（設定状況、部屋数、ランキングへのリンク、友だち追加QRコード） |
| GET | `/admin/settings` | イベント設定画面 |
| POST | `/admin/settings` | イベント設定更新（個人戦/チーム戦、判定モードの切替、スタンプカード台紙画像のアップロード） |
| GET | `/admin/rooms` | 部屋一覧 |
| GET | `/admin/rooms/add` | 部屋の新規登録画面 |
| POST | `/admin/rooms/add` | 部屋の新規登録（クエスト文・画像アップロード・スタンプ表示名（必須）・スタンプ画像アップロード（任意）・判定モードに応じて正解/ヒントを含む） |
| GET | `/admin/rooms/edit/{id}` | 部屋の編集画面 |
| POST | `/admin/rooms/update/{id}` | 部屋の更新（スタンプ表示名・スタンプ画像を含む） |
| POST | `/admin/rooms/delete/{id}` | 部屋の削除 |
| GET | `/admin/rooms/{id}/qr` | 部屋のQRコード画像を表示・印刷用に出力（スタッフが現地で保持するため） |
| GET | `/admin/line-qr` | LINE公式アカウントの友だち追加QRコード画像を表示（`LINE_ADD_FRIEND_URL`未設定時は404。詳細は [architecture.md](architecture.md) 22節） |
| GET | `/admin/ranking` | クリアタイムランキング（所要時間の短い順。未クリア者は圏外セクションに別掲。詳細は [architecture.md](architecture.md) 17節） |

---

## LINE Webhook (`handlers::line_webhook`)

| メソッド | パス | 説明 | CSRF |
|:--|:--|:--|:--|
| POST | `/callback` | LINE Webhook受信 | 除外（LINEサーバーからのリクエスト） |

- `x-line-signature` ヘッダーの検証に失敗（欠如・不一致）した場合は `401 Unauthorized` を返す。検証通過後は、個々のイベント処理でエラーが起きても常に `200 OK` を返す（詳細は [architecture.md](architecture.md) 8節）
- `type = message` かつ `message.type = text` 以外のWebhookイベントは無視する

### Bot コマンド一覧（プレイヤー向け）

コマンドの優先順位・状態ごとの分岐の詳細は [architecture.md](architecture.md) 10節を参照。

| メッセージ／状態 | 動作 |
|:--|:--|
| `開始`（未登録） | 登録待ち状態に遷移し、個人戦は個人名、チーム戦はチーム名の入力を促す |
| `開始`（登録待ち中） | 名前入力の催促を再送（重複登録はしない） |
| `開始`（登録済み・未クリア） | 現在案内中の部屋のクエストを再送（新たな部屋は割り当てない） |
| `開始`（登録済み・クリア済み） | 「クリア済みです。最初の部屋に戻ってください」を返信 |
| 任意のテキスト（登録待ち中、上記コマンドに非該当） | 個人名／チーム名として登録し、最初の部屋をランダムに通知 |
| 回答文字列（判定モードが「QR＋正解入力」かつ未正解の場合のみ有効） | 正誤判定。正解なら `answer_verified = true` にして「QRを読み込んでください」と案内 |
| `ヒント`（判定モードが「QR＋正解入力」の場合のみ有効） | 現在の部屋のヒントを返信。それ以外のモードでは「利用できません」と案内 |
| `スタンプ状況`（登録済み・未クリアの場合のみ有効） | 現在の訪問済み部屋数に応じたスタンプカード画像を返信 |
| `遊び方` / `ヘルプ` | 操作ガイドを返信（登録状態を問わず常に応答） |
| `リセット` | 自分の参加データ（`players`行、`visited_rooms`は連動削除）を削除。登録待ち状態のみの場合はその状態を解除 |
| 上記いずれにも該当しない自由入力（未登録・登録待ちでもない） | 「『開始』と送信してください」と案内 |

---

## LIFF連携 (`handlers::liff`)

| メソッド | パス | 説明 | 認証 | CSRF |
|:--|:--|:--|:--|:--|
| GET | `/liff/checkin` | LIFFページ（QRスキャンボタン）を表示 | 不要 | 対象外（Cookieセッションを使わないため） |
| POST | `/liff/checkin` | LIFFで読み取ったQRコードの内容（部屋のUUID）とLINE IDトークンを受け取り、チェックイン処理を行う | IDトークン検証 | 除外（LIFFからの直接呼び出し） |

### `POST /liff/checkin` リクエスト

```json
{ "id_token": "<liff.getIDToken()の値>", "qr_uuid": "<liff.scanCodeV2()で読み取った値>" }
```

### `POST /liff/checkin` の処理内容（詳細は [architecture.md](architecture.md) 15節）

1. `id_token` をLINEの検証エンドポイントに問い合わせ、有効であればLINEユーザーIDを得る。無効なら401
2. 送信されたUUIDが有効な部屋のものか確認する。該当部屋が無ければ404（`{"status":"rejected","reason":"room_not_found"}`）
3. そのLINEユーザーIDに対応する参加者が存在するか、`current_room_id` と一致するか、（判定モードが「QR＋正解入力」の場合は）`answer_verified` が `true` か、まだクリアしていないかを確認する。いずれか不成立なら403（`reason` は `not_registered` / `wrong_room` / `answer_not_verified` / `already_finished` のいずれか）
4. すべて条件を満たせば `visited_rooms` に記録し、200を返す（`{"status":"next"}` または、全部屋訪問済みなら `{"status":"cleared"}`）。次の部屋の案内・クリア報告は、このレスポンスではなくLINEチャットへのPush Messageとして別途送信する

---

## 画像配信 (`handlers::image`)

| メソッド | パス | 説明 | 認証 |
|:--|:--|:--|:--|
| GET | `/public/image/{uuid}` | 画像のバイナリを返す（部屋のクエスト画像・スタンプ画像・スタンプカード台紙画像を共通で配信） | 不要 |
| GET | `/public/stamp-card/{token}` | そのプレイヤーのスタンプカード画像（PNG、訪問済み部屋名入り）をその場で生成して返す（詳細は [architecture.md](architecture.md) 23節） | 不要 |

- UUID・`token`は推測不可能なランダム値
- Content-Typeは `image/jpeg`（部屋画像）・`image/png`（スタンプカード）固定（stored XSS対策）
- 該当UUID・`token`が存在しない場合は404
- LINE BotのFlex Message・画像メッセージから直接参照されるため認証不要
