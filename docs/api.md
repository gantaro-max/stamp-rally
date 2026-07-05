# API エンドポイント設計 — StampRallyBot（仮称）

認証方式: Cookieベースのセッション（管理者ログインのみ。プレイヤーはLINEアカウントで識別しWebログイン不要）

---

## 認証 (`handlers::auth`)

管理者は1アカウントのみを想定（自己登録フォームは設けない。初期パスワードは環境変数/シードで設定）。

| メソッド | パス | 説明 | 認証 |
|:--|:--|:--|:--|
| GET | `/auth/login` | ログイン画面 | 不要 |
| POST | `/auth/login` | ログイン処理 | 不要 |
| POST | `/auth/logout` | ログアウト（POSTのみ） | 必要 |

---

## 管理画面 (`handlers::admin`)

すべて管理者セッションが必要。

| メソッド | パス | 説明 |
|:--|:--|:--|
| GET | `/admin/dashboard` | ダッシュボード（設定状況、部屋数、ランキングへのリンク） |
| GET | `/admin/settings` | イベント設定画面 |
| POST | `/admin/settings` | イベント設定更新（個人戦/チーム戦、判定モードの切替） |
| GET | `/admin/rooms` | 部屋一覧 |
| GET | `/admin/rooms/add` | 部屋の新規登録画面 |
| POST | `/admin/rooms/add` | 部屋の新規登録（クエスト文・画像アップロード・判定モードに応じて正解/ヒントを含む） |
| GET | `/admin/rooms/edit/{id}` | 部屋の編集画面 |
| POST | `/admin/rooms/update/{id}` | 部屋の更新 |
| POST | `/admin/rooms/delete/{id}` | 部屋の削除 |
| GET | `/admin/rooms/{id}/qr` | 部屋のQRコード画像を表示・印刷用に出力（スタッフが現地で保持するため） |
| GET | `/admin/ranking` | クリアタイムランキング |

---

## LINE Webhook (`handlers::line_webhook`)

| メソッド | パス | 説明 | CSRF |
|:--|:--|:--|:--|
| POST | `/callback` | LINE Webhook受信 | 除外（LINEサーバーからのリクエスト） |

### Bot コマンド一覧（プレイヤー向け）

| メッセージ | 動作 |
|:--|:--|
| `開始` | 参加登録開始（個人戦は個人名、チーム戦はチーム名の入力を促す） |
| 個人名 / チーム名（参加直後） | 参加登録完了、最初の部屋をランダムに通知 |
| 回答文字列（判定モードが「QR＋正解入力」の場合のみ有効） | 正誤判定。正解なら「QRを読み込んでください」と案内 |
| `ヒント`（判定モードが「QR＋正解入力」の場合のみ有効） | 現在の部屋のヒントを返信 |
| `遊び方` / `ヘルプ` | 操作ガイドを返信 |
| `リセット` | 自分の参加データを削除 |

---

## LIFF連携 (`handlers::liff`)

| メソッド | パス | 説明 | CSRF |
|:--|:--|:--|:--|
| POST | `/liff/checkin` | LIFFで読み取ったQRコードの内容（部屋のUUID）とLINEユーザーIDを受け取り、チェックイン処理を行う | 除外（LIFFからの直接呼び出し） |

### `/liff/checkin` の処理内容

1. LIFF SDK（`liff.getIDToken()` 等）で取得したLINEユーザーIDを検証する
2. 送信されたUUIDが有効な部屋のものか確認する
3. そのプレイヤーの `current_room_id` と一致するか確認する（案内された部屋以外は無効）
4. 判定モードが「QR＋正解入力」のイベントでは、`answer_verified` が `true` であることも確認する
5. すべて条件を満たせば `visited_rooms` に記録し、以下のいずれかを返す
   - 未訪問部屋が残っている場合: 次の部屋（ランダム選出）の情報
   - 全15部屋を訪問済みの場合: クリア完了（`finished_at` 記録済み）の情報

---

## 画像配信 (`handlers::image`)

| メソッド | パス | 説明 | 認証 |
|:--|:--|:--|:--|
| GET | `/public/image/{uuid}` | 部屋画像のバイナリを返す | 不要 |

- UUIDは推測不可能なランダム値
- Content-Typeは `image/jpeg` 固定（stored XSS対策）
- LINE BotのFlex Messageから直接参照されるため認証不要
