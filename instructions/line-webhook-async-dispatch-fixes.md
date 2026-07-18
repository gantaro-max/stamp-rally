# 実装指示書: LINE Webhook非同期化 差し戻し修正（最終レビュー指摘対応）

## 背景・目的

`feature/line-webhook-async-dispatch`（PR #23、`/callback`のレスポンス即時返却・実処理の非同期化）はCodexの一次レビューを経て実装済みだが、Claudeによる最終レビュー（設計整合性・セキュリティ・要件充足・実装指示書・TDD遵守の5観点）のうち、設計整合性・要件充足の2観点で以下の問題が見つかった。マージ前にこのブランチ上で修正すること。

**問題**: 現在の実装は、`payload.events`をループする`for`文の**各イテレーション内**で`dispatch(state.spawn_background_tasks, ...)`を呼んでいる（`src/handlers/line_webhook.rs`の該当箇所）。これにより、同一Webhookペイロードに複数イベントが含まれる場合（例: 同一参加者が短時間に連続送信したメッセージがLINE側で1回の配信にまとめられた場合）、各イベントが**個別に**`tokio::spawn`され、並行実行されてしまう。

変更前（PR #23より前のmain）は、`for`ループ内で1件ずつ`.await`していたため、同一ペイロード内のイベントは配列順に逐次処理されることが保証されていた。`game_service::handle_text_message`（`src/services/game_service.rs`）は「リセット」「開始」等の分岐をトランザクションや排他制御なしに複数のDB操作で実装しているため、同一参加者の複数イベントが並行実行されると、送信順序と異なる最終状態（例: 「開始」→名前入力の直後に「リセット」が来た場合に、処理順序が入れ替わって参加登録データが不整合になる）が起こり得る。

設計は [docs/architecture.md 8節](../docs/architecture.md#8-line-webhookcallbackの受信と署名検証) に追記済み（「イベントごとに個別の`tokio::spawn`を行ってはならない」の記載）。本指示書はこれに基づく修正指示。

## 実装対象ファイル

- `src/handlers/line_webhook.rs` — ペイロード内の全イベント処理をまとめて1つのバックグラウンドタスクにする

## 修正内容（TDDの起点）

現在の`dispatch`関数（`spawn: bool`で`tokio::spawn`するか`.await`するかを切り替えるヘルパー）自体の実装・契約は変更不要（既存の`dispatch_with_spawn_returns_without_waiting_for_future`・`dispatch_without_spawn_waits_for_future`はそのまま維持する）。修正が必要なのは`callback`関数内で`dispatch`を**呼ぶ場所**であり、「イベントごとに1回」ではなく「ペイロード全体で1回」に変える。

- [ ] ケース1（新規・複数イベントの逐次処理を検証）: 1回の`/callback`リクエストのペイロードに、同一`line_user_id`からの2件のイベント（例: 1件目`text: "開始"`、2件目に続けて個人名/チーム名を入れた自由記入テキスト）を含めて送信した場合、2件目が1件目の処理結果（`pending_registrations`の作成）を前提として正しく処理され、最終的に`players`行が作成され部屋が割り当てられること（＝配列順に逐次処理されたことをDB状態で確認する）
  - このテストは、イベント処理本体を`callback`から切り出した関数（下記実装仕様の`process_events`）を**直接呼び出して**（`dispatch`を介さず）検証してよい。これにより、`spawn`の有無に依存しない、決定的（非フレーキー）なテストになる
- [ ] ケース2（回帰）: 既存の`callback_with_valid_text_message_updates_game_state`（単一イベント）が変更なくパスすること
- [ ] ケース3（回帰）: 既存の`dispatch_with_spawn_returns_without_waiting_for_future`・`dispatch_without_spawn_waits_for_future`が変更なくパスすること（`dispatch`関数自体は変更しないため）

## 実装仕様

### `src/handlers/line_webhook.rs`

現在`callback`の`for`ループ内にある「1イベント分の検証・`game_service`呼び出し・LINE返信送信」の処理本体を、`payload.events`（`Vec<WebhookEvent>`）を受け取ってループごと処理する関数に切り出す。

```rust
async fn process_events(state: AppState, events: Vec<WebhookEvent>) {
    for event in events {
        if event.event_type != "message" {
            continue;
        }
        let Some(message) = event.message else {
            continue;
        };
        if message.message_type != "text" {
            continue;
        }
        let (Some(reply_token), Some(source), Some(text)) =
            (event.reply_token, event.source, message.text)
        else {
            continue;
        };
        let Some(user_id) = source.user_id else {
            continue;
        };

        let reply = match game_service::with_db_call_timeout(game_service::handle_text_message(
            &state.pool,
            &state.public_base_url,
            &user_id,
            &text,
        ))
        .await
        {
            Ok(reply) => reply,
            Err(err) => {
                tracing::error!(?err, "failed to handle LINE text message");
                continue;
            }
        };
        if !state.send_line_replies {
            continue;
        }
        let message = line_client::to_line_message(&reply, &state.liff_id);
        if let Err(err) = line_client::send_reply(
            &state.http_client,
            &state.line_channel_access_token,
            &reply_token,
            message,
        )
        .await
        {
            tracing::error!(?err, "failed to send LINE reply");
        }
    }
}
```

`callback`本体は、署名検証・JSONパースの後、この関数呼び出し**1回だけ**を`dispatch`に渡す形にする。

```rust
dispatch(state.spawn_background_tasks, process_events(state.clone(), payload.events)).await;

StatusCode::OK
```

（`state.clone()`は`dispatch`に渡す前の1回のみ。現在のようにイベントごとに`clone`する必要はなくなる）

## 制約・注意事項

- `dispatch`関数自体のシグネチャ・実装は変更しないこと（既存のテスト2件が対象としているため）
- `/liff/checkin`（`src/handlers/liff.rs`）・PR #22で追加した`game_service::with_db_call_timeout`（15秒）・`reqwest::Client`のタイムアウト（10秒）は今回のスコープ外。変更しないこと
- 8節・21節の既存方針（1件のイベント処理の失敗が他のイベント処理に波及しない、`/callback`は常に200を返す）は変更しないこと
- 「別々のWebhookリクエストとして届く場合」の順序保証まで作り込む必要はない（docs/architecture.md 8節に明記の通り、今回のスコープ外）。あくまで同一ペイロード内の順序保証のみが対象
- 今回の修正はCodexの一次レビュー・Claudeの最終レビューを経た既存PR（#23）に対する差し戻しであるため、新しいテストケース（ケース1）はRed→Greenを踏むこと。ケース2・3は既存テストの回帰確認であり、修正後も変更なくパスすることを確認するだけでよい

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy -- -D warnings` が警告なく通る
- [ ] 同じPR（#23）のブランチに追加コミットをpushした（新しいブランチ・新しいPRは作らないこと）
