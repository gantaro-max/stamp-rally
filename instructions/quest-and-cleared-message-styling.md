# 実装指示書: クエスト通知・クリア報告のFlex Message装飾強化

## 背景・目的

本番運用のフィードバックで、LINE Botのメッセージが装飾に乏しく味気ないという指摘があった。[docs/architecture.md 13節「`line_client` と Flex Message」](../docs/architecture.md#13-line_clientとflex-message)に、クエスト通知の見た目強化と、クリア時のメッセージを平文からFlex Messageの演出に変更する設計を追記済み。本指示書はこれに基づく実装指示。

## 実装対象ファイル

- `src/services/ranking_service.rs` — `format_elapsed`関数の可視性を`pub(crate)`に変更
- `src/services/game_service.rs` — `ReplyMessage`に`Cleared`バリアントを追加、`CheckinOutcome::Cleared`が`ReplyMessage`を保持するように変更、`checkin`関数でクリアタイムを計算
- `src/services/line_client.rs` — `build_quest_flex_message`に装飾（header・separator・文字装飾）を追加、新規`build_cleared_flex_message`を追加、`to_line_message`に`Cleared`バリアントの処理を追加
- `src/handlers/liff.rs` — `CheckinOutcome::Cleared`の扱いを変更（`ReplyMessage`を`to_line_message`経由でFlex Messageに変換して送信する形に統一）

## テストケース（TDDの起点）

### ranking_service::format_elapsed

- [ ] ケース1（回帰）: 可視性変更後も既存の`ranking_service`内のテスト（`build_ranking_formats_elapsed_time_over_one_hour`等）が引き続きパスすること

### line_client::build_quest_flex_message（装飾）

- [ ] ケース2: `contents.header`に、背景色`#2E7D32`のboxで「次のクエスト」という白文字・太字のテキストが含まれること
- [ ] ケース3: `contents.body.contents`に、部屋名（太字・`size: xl`）、`type: separator`の要素、クエスト文（`size: md`・`color: #555555`）がこの順で含まれること
- [ ] ケース4（回帰）: 画像あり/なし双方で`hero`の有無が従来通り制御されること、`footer`のQRを読むボタンが従来通り存在すること（既存の`build_quest_flex_message_includes_hero_when_image_url_is_present`・`_omits_hero_when_image_url_is_absent`テストを、新しい`body.contents`のインデックス構成に合わせて更新する）

### line_client::build_cleared_flex_message（新規）

- [ ] ケース5: `build_cleared_flex_message(elapsed: &str)`が、`type: flex`、`altText`が空でないこと、`contents.header`に背景色`#FFC107`・白文字・太字・中央寄せ・`size: xl`で「🎉 クリア！」を含むこと
- [ ] ケース6: `contents.body.contents`に「全部屋制覇おめでとうございます！」（太字）、`elapsed`引数の値を含む「クリアタイム: {elapsed}」という文字列、「最初の部屋にお戻りください。お疲れ様でした！」（`size: sm`・グレー系文字色）の3つのテキスト要素がこの順で含まれること

### line_client::to_line_message

- [ ] ケース7: `ReplyMessage::Cleared { elapsed }`を渡すと、`build_cleared_flex_message`に`elapsed`がそのまま渡された結果が返ること
- [ ] ケース8（回帰）: `Text`・`Quest`バリアントの挙動は変更されないこと

### game_service::checkin（クリア判定）

- [ ] ケース9: 最後の部屋をチェックインしてクリアするとき、`CheckinOutcome::Cleared(ReplyMessage::Cleared { elapsed })`が返り、`elapsed`が`players.started_at`からの経過時間を`ranking_service::format_elapsed`と同じ形式（`M:SS`または`H:MM:SS`）で表した文字列になっていること（`started_at`をテストで既知の過去時刻に設定し、`elapsed`の形式—コロン区切りの数値であること—を確認する。実行時間に依存する秒単位の厳密一致は要求しない）
- [ ] ケース10（回帰）: 既存の`assert!(matches!(outcome, super::CheckinOutcome::Cleared))`という2箇所のアサーション（`CheckinOutcome::Cleared`がフィールドを持つようになるため`matches!(outcome, super::CheckinOutcome::Cleared(_))`に書き換えが必要）が、書き換え後も意図通りパスすること

### handlers/liff.rs（統合）

- [ ] ケース11（回帰）: `POST /liff/checkin`で最後の部屋をクリアしたときのレスポンス（`{"status": "cleared"}`）自体は変更されないこと。既存の`post_checkin_returns_cleared_and_marks_finished`相当のテストが引き続きパスすること

## 実装仕様

### src/services/ranking_service.rs

```rust
pub(crate) fn format_elapsed(duration: TimeDelta) -> String {
```
（`fn format_elapsed` → `pub(crate) fn format_elapsed` に変更するのみ。中身は変更しない）

### src/services/game_service.rs

`ReplyMessage`に`Cleared`バリアントを追加:

```rust
pub enum ReplyMessage {
    Text(String),
    Quest { room_name: String, quest_text: String, image_url: Option<String> },
    Cleared { elapsed: String },
}
```

`CheckinOutcome`を変更:

```rust
pub enum CheckinOutcome {
    NextQuest(ReplyMessage),
    Cleared(ReplyMessage),
    Rejected(CheckinRejection),
}
```

`checkin`関数内、2箇所ある`return Ok(CheckinOutcome::Cleared);`（`player_repository::mark_finished`呼び出し直後）を、以下のように変更する:

```rust
player_repository::mark_finished(pool, player.id).await?;
let elapsed = crate::services::ranking_service::format_elapsed(
    chrono::Utc::now().naive_utc() - player.started_at,
);
return Ok(CheckinOutcome::Cleared(ReplyMessage::Cleared { elapsed }));
```

（`finished_at`はDB側で`NOW()`により確定するが、Flex Message表示用の経過時間はアプリ側時刻から計算してよい。ランキング画面自体はDBの`finished_at`をそのまま使うため、表示用途に限りこの近似は許容される。設計書参照）

### src/services/line_client.rs

`build_quest_flex_message`の`body`部分を、以下のような構成に変更する（`hero`・`footer`の既存ロジックはそのまま）:

```rust
"body": {
    "type": "box",
    "layout": "vertical",
    "spacing": "md",
    "contents": [
        {"type": "text", "text": room_name, "weight": "bold", "size": "xl", "wrap": true},
        {"type": "separator"},
        {"type": "text", "text": quest_text, "wrap": true, "size": "md", "color": "#555555"}
    ]
},
"header": {
    "type": "box",
    "layout": "vertical",
    "backgroundColor": "#2E7D32",
    "paddingAll": "12px",
    "contents": [
        {"type": "text", "text": "次のクエスト", "color": "#FFFFFF", "size": "sm", "weight": "bold"}
    ]
}
```

新規関数`build_cleared_flex_message`:

```rust
pub fn build_cleared_flex_message(elapsed: &str) -> Value {
    json!({
        "type": "flex",
        "altText": "全部屋クリアしました！",
        "contents": {
            "type": "bubble",
            "header": {
                "type": "box",
                "layout": "vertical",
                "backgroundColor": "#FFC107",
                "paddingAll": "16px",
                "contents": [
                    {"type": "text", "text": "🎉 クリア！", "color": "#FFFFFF", "weight": "bold", "size": "xl", "align": "center"}
                ]
            },
            "body": {
                "type": "box",
                "layout": "vertical",
                "spacing": "md",
                "contents": [
                    {"type": "text", "text": "全部屋制覇おめでとうございます！", "weight": "bold", "wrap": true},
                    {"type": "text", "text": format!("クリアタイム: {elapsed}"), "wrap": true},
                    {"type": "text", "text": "最初の部屋にお戻りください。お疲れ様でした！", "wrap": true, "size": "sm", "color": "#888888"}
                ]
            }
        }
    })
}
```

`to_line_message`に分岐を追加:

```rust
ReplyMessage::Cleared { elapsed } => build_cleared_flex_message(elapsed),
```

### src/handlers/liff.rs

`CheckinOutcome::Cleared`の処理を、`NextQuest`と同じパターン（`to_line_message`経由）に統一する:

```rust
game_service::CheckinOutcome::Cleared(reply) => {
    if state.send_line_replies {
        let message = line_client::to_line_message(&reply, &state.liff_id);
        if let Err(err) = line_client::push_message(
            &state.http_client,
            &state.line_channel_access_token,
            &line_user_id,
            message,
        )
        .await
        {
            tracing::error!(?err, "failed to push cleared message");
        }
    }
    (StatusCode::OK, Json(json!({"status": "cleared"}))).into_response()
}
```

## 制約・注意事項

- `POST /liff/checkin`自体のレスポンスJSON（`{"status": "cleared"}`）は変更しないこと。変更するのはLINEへのPush Message（LINEチャット側に届く内容）のみ
- `game_service`はLINE固有のJSON構造や`reqwest`を一切知らない、という既存の設計原則（13節）を守ること。`ReplyMessage::Cleared`は文字列（`elapsed`）のみを保持し、Flex MessageのJSON組み立ては`line_client`側に閉じ込める
- 色・文言は指示書・`docs/architecture.md`の記載と完全に一致させること
- ランキング画面（`/admin/ranking`）・`ranking_service::build_ranking`のロジックには一切手を加えないこと（`format_elapsed`の可視性変更のみ）

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
