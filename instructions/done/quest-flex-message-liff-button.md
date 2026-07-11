# 実装指示書: クエスト通知にLIFF起動ボタンを追加する

## 背景・目的

本番デプロイ後の動作確認で、LINE Botが送るクエスト通知（Flex Message）に「QRコードを読み込んでください」という案内文はあるが、実際にLIFFページ（`/liff/checkin`、QRスキャン画面）を開く手段がどこにも無いことが判明した。参加者はQRコードをスキャンするための入り口が存在せず、ゲームを進められない状態にある。

[docs/architecture.md 13節](../docs/architecture.md#13-line_clientとflex-message)に、クエスト通知のFlex Messageに「QRを読む」ボタン（LIFFページを開く`uri`アクション）を追加する設計を追記済み。本指示書はこれに基づく実装指示。

## 実装対象ファイル

- `src/services/line_client.rs` — `build_quest_flex_message`・`to_line_message`に`liff_id`を渡し、Flex Messageの`footer`にボタンを追加
- `src/handlers/line_webhook.rs` — `line_client::to_line_message`呼び出しに`&state.liff_id`を渡す
- `src/handlers/liff.rs` — 同上（チェックイン成功後のPush通知でも同じ関数を使っている）

## テストケース（TDDの起点）

- [ ] ケース1: `build_quest_flex_message(room_name, quest_text, image_url, liff_id)`が返すJSONの`contents.footer`に、`action.type = "uri"`・`action.uri = "https://liff.line.me/{liff_id}"`（`liff_id`は実際に渡した値で置換）を持つボタン要素が含まれること（画像ありのケース）
- [ ] ケース2: 画像なしのケースでも同様に`footer`のボタンが含まれること（`hero`の有無とボタンの有無は独立していることの確認）
- [ ] ケース3: `to_line_message`に`ReplyMessage::Quest{...}`と`liff_id`を渡すと、`build_quest_flex_message`に`liff_id`がそのまま伝搬されること（既存の`to_line_message_builds_checkin_push_texts`テストと対になる形で、Questバリアント用のテストを追加、または既存のQuest系テストがあれば更新する）
- [ ] ケース4（回帰）: `ReplyMessage::Text`バリアントについては、`to_line_message`に`liff_id`を渡しても出力JSONが従来と変わらないこと（`liff_id`はTextバリアントでは無視される）
- [ ] ケース5（回帰）: 既存の`build_quest_flex_message_includes_hero_when_image_url_is_present`・`build_quest_flex_message_omits_hero_when_image_url_is_absent`テストが、シグネチャ変更（`liff_id`引数の追加）に伴い引き続き意図通りパスすること（呼び出し箇所にダミーのliff_id、例: `"test-liff-id"`を渡す形に更新）

## 実装仕様

### src/services/line_client.rs

`build_quest_flex_message`のシグネチャを変更する:

```rust
pub fn build_quest_flex_message(
    room_name: &str,
    quest_text: &str,
    image_url: Option<&str>,
    liff_id: &str,
) -> Value {
    // 既存の body / hero 組み立てはそのまま

    contents["footer"] = json!({
        "type": "box",
        "layout": "vertical",
        "contents": [
            {
                "type": "button",
                "style": "primary",
                "action": {
                    "type": "uri",
                    "label": "QRを読む",
                    "uri": format!("https://liff.line.me/{liff_id}")
                }
            }
        ]
    });

    // 既存通り json!({"type": "flex", "altText": ..., "contents": contents}) を返す
}
```

`to_line_message`のシグネチャを変更する:

```rust
pub fn to_line_message(reply: &ReplyMessage, liff_id: &str) -> Value {
    match reply {
        ReplyMessage::Text(text) => build_text_message(text),
        ReplyMessage::Quest { room_name, quest_text, image_url } =>
            build_quest_flex_message(room_name, quest_text, image_url.as_deref(), liff_id),
    }
}
```

### src/handlers/line_webhook.rs

`line_client::to_line_message(&reply)`の呼び出しを`line_client::to_line_message(&reply, &state.liff_id)`に変更する。`state.liff_id`は既に`AppState`に存在するフィールド（`Arc<str>`）。

### src/handlers/liff.rs

同様に、チェックイン成功後の次クエスト・クリア報告のPush通知を組み立てている`line_client::to_line_message(&reply)`呼び出しを`line_client::to_line_message(&reply, &state.liff_id)`に変更する。

## 制約・注意事項

- LIFFページ自体（`GET /liff/checkin`、`src/handlers/liff.rs`のページ本体・`liff.scanCodeV2()`呼び出し）には手を加えないこと。今回追加するのはあくまで「LINEチャット上のFlex MessageからLIFFページを開くボタン」であり、LIFFページ内の「QRを読む」ボタン（クライアントJS）とは別物
- ボタンのラベル文言は「QRを読む」で統一する（`docs/architecture.md`の記述と一致させる）
- `https://liff.line.me/{liff_id}`のURL形式はLINEプラットフォームの標準的なLIFF URL形式。末尾スラッシュ等を付けないこと
- `build_text_message`・`ReplyMessage::Text`系のロジックには手を加えないこと（ボタンが必要なのはQuest通知のみ）

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
