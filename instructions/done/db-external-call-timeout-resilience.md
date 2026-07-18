# 実装指示書: DB接続・外部API呼び出しのタイムアウト設定

## 背景・目的

本番運用中、参加者が「開始」→チーム名入力（`players`行作成・部屋割当を含む一連のDBアクセス）を行った際にBotが無反応になる障害が発生した。サーバログを確認したところ、DBコネクションプールで以下のようなWARNが記録された直後、当該リクエストに関するログが一切続かず（`failed to handle LINE text message`等のエラーログも出ない）、処理がハングしたまま応答が返らない状態だった。

```
WARN sqlx_core::pool::connection: error occurred while testing the connection on-release error=encountered unexpected or invalid data: expected 0x00 or 0xfe (OK_Packet) but found 0x03
```

調査の結果、`MySqlPoolOptions::new().connect(...)`（`src/main.rs`）・`reqwest::Client::new()`（`AppState::new`、同じく`src/main.rs`）のいずれにもタイムアウトが設定されていないことが判明した。TiDB Serverless側でコネクションが無応答のまま破棄された場合、sqlxの`test_before_acquire`（既定で有効）によるpingも含めてクエリが無期限にハングしうる。これにより、Webhook（`/callback`）・LIFFチェックイン（`/liff/checkin`）の処理が完了せず、参加者への応答が失われたままエラーログも残らない。

設計は [docs/architecture.md 21節「DB接続・外部API呼び出しのタイムアウト」](../docs/architecture.md#21-db接続外部api呼び出しのタイムアウト) と [docs/requirements.md 非機能要件「耐障害性（タイムアウト）」](../docs/requirements.md) に追記済み。本指示書はこれに基づく実装指示。

## 実装対象ファイル

- `src/main.rs` — `reqwest::Client`にタイムアウトを設定、`MySqlPoolOptions`の見直し（後述）
- `src/services/game_service.rs` — `GameServiceError`に`Timeout`バリアントを追加
- `src/handlers/line_webhook.rs` — `game_service::handle_text_message`呼び出しを`tokio::time::timeout`でラップ
- `src/handlers/liff.rs` — `game_service::checkin`呼び出しを`tokio::time::timeout`でラップ

## テストケース（TDDの起点）

sqlxの`PoolOptions`や実際のネットワーク遅延を統合テストで再現するのは非現実的（実DB・実ネットワークに依存し、遅くて不安定なテストになる）。そのため、「タイムアウトでラップする」というロジック自体は、実際のDB呼び出しに依存しない形で単体テストする。

- [ ] ケース1: `tokio::time::timeout`で包んだ処理が、指定時間内に完了する場合は、その結果（`Ok`/`Err`いずれも）がそのまま呼び出し元に伝わること
  - 例: 即座に`Ok(GameServiceError相当のダミー値)`を返すfutureを渡し、タイムアウトエラーにならないことを確認する
- [ ] ケース2: `tokio::time::timeout`で包んだ処理が、指定時間を超えても完了しない場合、`GameServiceError::Timeout`相当のエラーとして扱われること
  - `tokio::time::pause()`（`#[tokio::test(start_paused = true)]`）と、`std::future::pending::<T>()`（＝永久に完了しないfuture）を使い、仮想時刻を`tokio::time::advance()`で進めることで、実際に秒単位で待たずにタイムアウト発火を検証する
- [ ] ケース3（回帰）: 既存の`line_webhook.rs`の`callback_with_valid_text_message_updates_game_state`、`liff.rs`の既存チェックイン系テストが、タイムアウトラップ追加後も変更なくパスすること（＝正常系のレイテンシではタイムアウトが誤発火しないこと）

## 実装仕様

### `src/services/game_service.rs`

`GameServiceError`に`Timeout`バリアントを追加する。

```rust
#[derive(Debug)]
pub enum GameServiceError {
    Database(sqlx::Error),
    Timeout,
}

impl std::fmt::Display for GameServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
            Self::Timeout => write!(f, "operation timed out"),
        }
    }
}
```

（`From<sqlx::Error>`実装は`Database`のみのままでよい。`Timeout`はハンドラー側で`tokio::time::timeout`の結果から直接組み立てる）

### `src/handlers/line_webhook.rs`

`game_service::handle_text_message`の呼び出し（41〜111行目付近）を`tokio::time::timeout`でラップする。タイムアウト値は`AppState`に持たせず、モジュール内の定数（例: `const DB_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);`）としてよい（本番運用中に調整することはあっても、テスト時に差し替える必要はないため。テストは`tokio::time::pause`で仮想時刻を進めるので実際に15秒待つ必要はない）。

```rust
let reply = match tokio::time::timeout(
    DB_CALL_TIMEOUT,
    game_service::handle_text_message(&state.pool, &state.public_base_url, &user_id, &text),
)
.await
{
    Ok(Ok(reply)) => reply,
    Ok(Err(err)) => {
        tracing::error!(?err, "failed to handle LINE text message");
        continue;
    }
    Err(_) => {
        tracing::error!("timed out handling LINE text message");
        continue;
    }
};
```

### `src/handlers/liff.rs`

`game_service::checkin`の呼び出しを同様にラップする。既存の「エラー時は500を返す」処理経路（`Err(err) => { tracing::error!(...); return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }`）に、タイムアウト時も同じレスポンスを返すよう分岐を追加する。

```rust
let outcome = match tokio::time::timeout(
    DB_CALL_TIMEOUT,
    game_service::checkin(&state.pool, &state.public_base_url, &line_user_id, &body.qr_uuid),
)
.await
{
    Ok(Ok(outcome)) => outcome,
    Ok(Err(err)) => {
        tracing::error!(?err, "failed to process LIFF checkin");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Err(_) => {
        tracing::error!("timed out processing LIFF checkin");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
};
```

（`DB_CALL_TIMEOUT`は`line_webhook.rs`と同じ値を、重複を避けたいなら共有の場所（例: `game_service`モジュール、または新設の小さな定数モジュール）に定義してどちらからも参照してよい。判断はCodexに委ねる）

### `src/main.rs`

`AppState::new`内の`reqwest::Client::new()`（47行目付近）を、タイムアウト付きの`ClientBuilder`に変更する。

```rust
http_client: reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .expect("failed to build reqwest client"),
```

（`std::time::Duration`は`main.rs`に未importなので、フル修飾で書くか、既存の`time::Duration`（セッション有効期限用）と名前が衝突しないよう別名で`use`する。フル修飾のほうが衝突のリスクがなく安全）

`MySqlPoolOptions::new().connect(&database_url)`（101〜102行目）については、今回のスコープでは変更不要（21節に記載の通り、根本原因は`idle_timeout`/`max_lifetime`ではなく操作単位のタイムアウト欠如のため）。プール設定自体のチューニングは対象外とする。

## 制約・注意事項

- 本指示書のスコープは「タイムアウトを設けてハングを防ぐ」ことのみ。DBコネクションプールの`idle_timeout`・`max_lifetime`等のチューニングは対象外（21節参照。誤って着手しないこと）
- 8節・13節の既存方針（1件のイベント処理の失敗が他のイベント処理に波及しない、`/callback`は常に200を返す）は変更しないこと
- `reqwest::Client::builder().build()`は通常失敗しない（TLS初期化失敗等の極めて稀なケースのみ）が、`main`関数の起動時なので`expect`で落として構わない（`DATABASE_URL`等、他の起動時必須設定と同じ扱い）
- タイムアウト定数の値（10秒・15秒）はdocs/architecture.md 21節に記載した初期見積もり値をそのまま使うこと。実装時に変更が必要と判断した場合はdocs側の値も合わせて更新すること

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy -- -D warnings` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
