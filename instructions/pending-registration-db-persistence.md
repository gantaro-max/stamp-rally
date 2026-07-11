# 実装指示書: 参加登録の一時状態をDB永続化する

## 背景・目的

Koyebへの本番デプロイ作業中に、無料枠（Ecoインスタンス）は最小インスタンス数を0に固定できず、アイドル時にインスタンス数0へスケールダウンすることが判明した。

現状、LINE Bot参加登録の一時状態（「開始」送信後、名前／チーム名の入力を待つ「登録待ち」状態）は`AppState.pending_registrations: Arc<Mutex<HashSet<String>>>`としてアプリ内メモリのみで保持している（[docs/architecture.md 9節（旧版）](../docs/architecture.md)参照）。この状態のままだと、参加者が「開始」を送ってから名前を入力するまでの間にインスタンスがスケールダウン・再起動すると、登録待ち状態が失われ、名前を送っても正しく処理されない（無視されるか、想定外の分岐に落ちる）。

これを避けるため、`pending_registrations`テーブルを新設し、DBに永続化する方針に変更した。設計変更の詳細は以下を参照:

- [docs/requirements.md 4節（非機能要件）](../docs/requirements.md)
- [docs/architecture.md 9節・18節「セッションストア・Cookie」](../docs/architecture.md)
- [docs/database.md「pending_registrations（参加登録の一時状態）」](../docs/database.md)

なお、管理者セッション（`tower_sessions::MemoryStore`）は今回のスコープ外。再ログインで復帰できる実害の小さい範囲として、引き続きメモリ保持のままでよい（設計書に明記済み）。

## 実装対象ファイル

- `migrations/0002_pending_registrations.sql`（新規） — テーブル追加
- `src/repository/pending_registration_repository.rs`（新規） — `exists`/`insert`/`delete`関数
- `src/repository/mod.rs` — 新規モジュールの登録
- `src/services/game_service.rs` — `PendingRegistrations`型・`AppState`依存のヘルパー関数（`is_pending`/`add_pending`/`remove_pending`/`was_pending_removed`）を削除し、`pending_registration_repository`呼び出しに置き換える。`handle_text_message`のシグネチャから`pending: &PendingRegistrations`引数を削除する
- `src/handlers/line_webhook.rs` — `handle_text_message`呼び出し箇所から`&state.pending_registrations`引数を削除
- `src/main.rs` — `AppState`から`pending_registrations`フィールドと初期化コードを削除

## テストケース（TDDの起点）

既存の`game_service`のテスト（`src/services/game_service.rs`内、`#[sqlx::test]`）は、`pending()`ヘルパー（`Arc<Mutex<HashSet<String>>>`を作る）を使い、`pending.lock().unwrap().insert(...)`のような形で登録待ち状態をセットアップ・アサートしている。これらをDB行のセットアップ・アサートに置き換える。**新しいテストケースを増やす必要はなく、既存のテストが表す振る舞いをDB永続化後も維持することが目的**（回帰確認がTDDの起点になる）。

- [ ] ケース1（回帰）: `handle_text_message`に「開始」を送ると、`pending_registrations`テーブルに`(line_user_id, event_id)`の行が作られること（従来`pending.lock().unwrap().contains(...)`でアサートしていた箇所を、テーブルへの`SELECT`に置き換える）
- [ ] ケース2（回帰）: `pending_registrations`に該当行がある状態で名前を送ると、`players`行が作成され、`pending_registrations`の該当行が削除されること
- [ ] ケース3（回帰）: 空白のみの名前を送っても`pending_registrations`の行は消えないこと（再入力を促す）
- [ ] ケース4（回帰）: 部屋が0件の状態で名前を送るとエラーメッセージを返し、`pending_registrations`の行は削除されること（プレイヤーも作成されない）
- [ ] ケース5（回帰）: 「リセット」を送ると、`player`が存在する場合は`player`を削除し、存在しない場合で`pending_registrations`に行がある場合はその行を削除して「登録をキャンセルしました」を返すこと。どちらもない場合は「現在参加登録されていません」を返すこと
- [ ] ケース6（回帰）: 既に`pending_registrations`に行がある状態で再度「開始」を送ると、行を重複させず（あるいはエラーにせず）名前入力の催促を再送すること
- [ ] ケース7（新規・重要）: **プロセス再起動を模したテスト**。「開始」を送って`pending_registrations`に行を作った後、その行を直接DBに残したまま**新しい`MySqlPool`インスタンス**（同じDBに接続する別のプール、または同じプールでも構わないが「メモリ上の状態を一切引き継がない」ことを表現できる形）で名前を送るテストを追加し、登録が正しく完了することを確認する（DB永続化の効果を検証する唯一のテストケース。既存の`sqlx::test`は同一テスト関数内で同じ`pool`を使い回すため、明示的にこのケースを書かないとメモリ保持でもDB永続化でも見分けがつかない）

## 実装仕様

### migrations/0002_pending_registrations.sql

```sql
CREATE TABLE pending_registrations (
    line_user_id VARCHAR(255) NOT NULL,
    event_id INT NOT NULL,
    created_at DATETIME NOT NULL,
    PRIMARY KEY (line_user_id, event_id),
    CONSTRAINT fk_pending_registrations_event_id
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

`docs/database.md`の定義と完全に一致させること。

### src/repository/pending_registration_repository.rs（新規）

既存の`repository`配下のモジュール（例: `player_repository.rs`）と同じスタイル（生SQL、`sqlx::query!`系マクロは使わず`sqlx::query`/`query_as`を使う既存パターンに合わせる。実際のコードを確認して既存の書き方に倣うこと）で、以下3関数を実装する。

```rust
pub async fn exists(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<bool, sqlx::Error> { ... }

pub async fn insert(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<(), sqlx::Error> {
    // INSERT ... ON DUPLICATE KEY UPDATE created_at = VALUES(created_at)
    // 既に行がある状態で再度「開始」が来ても（ケース6）エラーにしない
}

pub async fn delete(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<bool, sqlx::Error> {
    // DELETE FROM pending_registrations WHERE line_user_id = ? AND event_id = ?
    // 戻り値は削除された行数 > 0（既存の`was_pending_removed`と同じセマンティクス）
}
```

`event_id`の型は既存の`event_repository`/`player_repository`の`Event.id`の型（`i32`など、実コードを確認して合わせること）に揃える。

### src/services/game_service.rs

- ファイル冒頭の`use std::{collections::HashSet, sync::{Arc, Mutex}};`と`pub type PendingRegistrations = Arc<Mutex<HashSet<String>>>;`を削除
- `use crate::repository::{..., pending_registration_repository};`を追加
- `is_pending`/`add_pending`/`remove_pending`/`was_pending_removed`の4関数を削除
- `handle_text_message`のシグネチャから`pending: &PendingRegistrations`引数を削除
- 関数内の呼び出し箇所を置き換える:
  - `was_pending_removed(pending, line_user_id)` → `pending_registration_repository::delete(pool, line_user_id, event.id).await?`
  - `is_pending(pending, line_user_id)` → `pending_registration_repository::exists(pool, line_user_id, event.id).await?`
  - `add_pending(pending, line_user_id)` → `pending_registration_repository::insert(pool, line_user_id, event.id).await?`
  - `remove_pending(pending, line_user_id)` → `pending_registration_repository::delete(pool, line_user_id, event.id).await?`（戻り値の`bool`が不要な呼び出し箇所では単に`?`で結果を捨ててよい）
- `mod tests`内の`pending()`ヘルパー・`PendingRegistrations`のインポートを削除し、各テストで`pending.lock().unwrap().insert(...)`のような操作をしている箇所を、`pending_registration_repository::insert(&pool, "line-xxx", event_id).await.unwrap();`のようなDB操作に置き換える。アサート側（`pending.lock().unwrap().contains(...)` / `!pending.lock().unwrap().contains(...)`）も`pending_registration_repository::exists(&pool, "line-xxx", event_id).await.unwrap()`に置き換える

### src/handlers/line_webhook.rs

- `game_service::handle_text_message(&state.pool, &state.pending_registrations, &state.public_base_url, &user_id, &text)`から`&state.pending_registrations`を削除する
- テスト内の`let pending = state.pending_registrations.clone();`等、不要になった変数・参照は削除する

### src/main.rs

- `AppState`構造体から`pub pending_registrations: services::game_service::PendingRegistrations,`フィールドを削除
- `AppState::new`から該当フィールドの初期化（`pending_registrations: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),`）を削除
- 不要になった`use`があれば削除する

## 制約・注意事項

- スコープは参加登録の一時状態のみ。管理者セッション（`tower_sessions::MemoryStore`）には手を加えないこと（設計書で明示的にスコープ外とした）
- `players`・`rooms`・`events`等、既存テーブルのスキーマ・既存repositoryのロジックには手を加えないこと
- `pending_registration_repository::insert`は「既に行がある状態で再度呼ばれても失敗しない」実装にすること（ケース6のテストが根拠）
- ローカル開発環境（`docker-compose up -d`）で`sqlx migrate run`相当（`cargo run`時の自動マイグレーションは行わない設計のため、手動で`sqlx migrate run`を実行）が正しく通ることを確認すること

## 完了条件

- [ ] 上記テストケース（特にケース7）について、実装前に失敗する状態を確認した（Red）。ケース1〜6は既存テストの書き換えのため、書き換え後に一度実行してみて意図通り失敗する／通ることを確認した上でコミットすること
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
