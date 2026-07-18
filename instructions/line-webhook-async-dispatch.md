# 実装指示書: `/callback`のレスポンス即時返却とイベント処理の非同期化

## 背景・目的

PR #22（`db-external-call-timeout-resilience`）でDBアクセス・LINE API呼び出しにタイムアウトを追加したが、本番デプロイ後も同じ操作（「開始」→チーム名入力）でBotが無反応になる事象が再発した。

調査の結果、LINE Developers Consoleの「Webhookのエラー統計」に`request_timeout`（LINEプラットフォーム自身がWebhookレスポンスを待ちきれずタイムアウトした）が記録されていることを確認した。`/callback`ハンドラーは署名検証→`game_service`呼び出し（DBアクセス）→LINEへの返信送信→200を返す、という一連の処理を同期的に行っているため、PR #22で追加した15秒のDB呼び出しタイムアウトが発動する前に、LINEプラットフォーム自身がWebhookの応答を待つのをやめてしまい、こちらが最終的に200を返しても参加者には何も届かない状態になっていた。

設計は [docs/architecture.md 8節](../docs/architecture.md#8-line-webhookcallbackの受信と署名検証) および [21節「追記: `/callback`はさらにレスポンスの即時返却が必要だった」](../docs/architecture.md#追記-callbackはさらにレスポンスの即時返却が必要だった) に追記済み。本指示書はこれに基づく実装指示。

## 実装対象ファイル

- `src/main.rs` — `AppState`にテスト用フック`spawn_background_tasks: bool`を追加
- `src/handlers/line_webhook.rs` — イベントごとの実処理を`tokio::spawn`によるバックグラウンドタスクに切り離す

## テストケース（TDDの起点）

- [ ] ケース1（正常系・既存の回帰）: 既存の`callback_with_valid_text_message_updates_game_state`が、`state.spawn_background_tasks = false`（新規追加。既存の`state.send_line_replies = false`と同様の場所に追記）を設定した上で、引き続きパスすること（`.await`による同期処理経路の担保）
- [ ] ケース2（新規・即時レスポンスの検証）: `spawn_background_tasks = true`（本番のデフォルト）の場合、`/callback`のレスポンスが、内部処理の完了を待たずに返ってくることを検証する
  - 実際のDBやネットワーク遅延に依存させず、`line_webhook.rs`内に切り出す小さなヘルパー関数（下記実装仕様参照）を直接ユニットテストする形にする
  - `std::future::pending::<()>()`（＝永久に完了しないfuture）を渡して`spawn=true`で呼び出し、その呼び出し自体を短いタイムアウト（例: `tokio::time::timeout(Duration::from_millis(100), ...)`）で包んでも、タイムアウトせずに完了することを確認する（＝内部のfutureがどれだけ待っても終わらなくても、ヘルパー自体はすぐ返ることの証明）
- [ ] ケース3（新規・同期経路の検証）: 同じヘルパー関数を`spawn=false`で呼び出した場合、渡したfutureが実際に完了してから戻ることを検証する（例: `AtomicBool`や`Arc<Mutex<bool>>`をfuture内でtrueにセットし、ヘルパー呼び出しの直後にtrueになっていることをアサートする）

## 実装仕様

### `src/handlers/line_webhook.rs`

イベントごとの実処理（現在の80〜107行目、`game_service::with_db_call_timeout(...)`の呼び出しからLINE返信送信までの一連の処理）を、`state`をムーブしたasyncブロックとして構築し、以下のヘルパーで実行する。

```rust
async fn dispatch<F>(spawn: bool, fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if spawn {
        tokio::spawn(fut);
    } else {
        fut.await;
    }
}
```

`callback`関数のループ内は、既存の処理本体（`reply`を取得してLINEに返信する一連の処理）を`async move { ... }`ブロックに包み、`dispatch(state.spawn_background_tasks, ...).await`を呼ぶ形に変更する。ループ自体は`for`のまま、各イベントについて`dispatch`を呼んだら次のイベントに進んでよい（`dispatch`は`spawn=true`の場合ほぼ即座に返るため、ループが実処理の完了を待つことはない）。

ループを抜けた後、既存通り`StatusCode::OK`を返す。

（`state`は`AppState`が`Clone`なので、asyncブロックに渡す前に`state.clone()`する。`reply_token`・`user_id`・`text`など、ループ内で借用していた値も同様に、asyncブロックにムーブできる形（`String`等の所有型）で渡すこと）

### `src/main.rs`

`AppState`に`pub spawn_background_tasks: bool`フィールドを追加し、`AppState::new`では`true`を設定する（他のテスト用フック`verify_id_tokens`・`send_line_replies`と同じ並びに追記）。

## 制約・注意事項

- `/liff/checkin`（`src/handlers/liff.rs`）は今回の変更対象外。理由はdocs/architecture.md 21節「追記」の「`/liff/checkin`は対象外」を参照（呼び出し元がLINEプラットフォームではなくLIFFページ自身のブラウザであり、レスポンス内容自体がページ表示に必要なため）
- PR #22で追加した`with_db_call_timeout`（15秒）・`reqwest::Client`のタイムアウト（10秒）は変更しない。これらはバックグラウンドタスクがDBコネクションプールを無期限に占有し続けることを防ぐ安全装置として引き続き必要
- 8節・13節の既存方針（1件のイベント処理の失敗が他のイベント処理に波及しない、`/callback`は常に200を返す）は変更しないこと。バックグラウンドタスク内のエラーも、従来通り`tracing::error!`でログに記録するのみとする
- `tokio::spawn`されたタスクの`JoinHandle`は明示的に`await`せず破棄してよい（`drop`してもタスク自体はランタイム上で実行が継続される。これがまさに「レスポンスを待たずにバックグラウンドで処理を続ける」という今回の狙い）
- 既存の`callback_with_valid_text_message_updates_game_state`テストの修正時、`state.send_line_replies = false;`の近くに`state.spawn_background_tasks = false;`を追記するだけでよく、テストの本体（リクエスト送信・アサーション）は変更しない

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy -- -D warnings` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
