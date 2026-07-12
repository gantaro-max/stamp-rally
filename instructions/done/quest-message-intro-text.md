# 実装指示書: クエスト通知に状況に応じた「つなぎの文」を追加する

## 背景・目的

本番運用のフィードバックで、部屋の案内メッセージがいきなり部屋名・クエスト文から始まり、文脈（初めての部屋なのか、前の部屋をクリアしたばかりなのか）が分かりにくいという指摘があった。[docs/architecture.md 13節「`line_client` と Flex Message」](../docs/architecture.md#13-line_clientとflex-message)に、状況ごとの「つなぎの文」をクエスト通知のFlex Messageに追加する設計を追記済み。本指示書はこれに基づく実装指示。

## 実装対象ファイル

- `src/services/game_service.rs` — `ReplyMessage::Quest`に`intro`フィールドを追加し、`quest_reply_for_room`が呼び出し元から`intro`を受け取るように変更。3つの呼び出し箇所それぞれで状況に応じた文言を渡す
- `src/services/line_client.rs` — `build_quest_flex_message`が`intro`を受け取り、Flex Messageの`body`先頭に表示するように変更
- 上記2ファイルの既存テストの更新（`body.contents`のインデックスがずれるため）

## テストケース（TDDの起点）

### game_service

- [ ] ケース1: 参加登録（名前入力）直後に返される`ReplyMessage::Quest`の`intro`が`"最初の部屋は"`であること
- [ ] ケース2: 既に部屋が割り当てられた状態（未クリア）で「開始」を再送信したときに返される`ReplyMessage::Quest`の`intro`が`"現在向かっている部屋は"`であること
- [ ] ケース3: QRチェックインが成功し次の部屋が割り当てられたときに返される`CheckinOutcome::NextQuest(ReplyMessage::Quest{intro, ..})`の`intro`が、`"【{直前にクリアした部屋の room_name}】クリアおめでとうございます。次の部屋は"`という形式になっていること（部屋名部分は実際にクリアした部屋の名前に置き換わること。例えば部屋名が「図書室」なら`"【図書室】クリアおめでとうございます。次の部屋は"`）

### line_client

- [ ] ケース4: `build_quest_flex_message`に`intro`引数を渡すと、`contents.body.contents`の先頭要素がその`intro`の値をテキストとして持ち、`size: "sm"`・`color: "#888888"`・`wrap: true`になっていること
- [ ] ケース5（回帰）: 既存の`build_quest_flex_message_includes_hero_when_image_url_is_present`・`_omits_hero_when_image_url_is_absent`テストを、`body.contents`のインデックスが1つずつ後ろにずれる（`intro`が0番目、部屋名が1番目、`separator`が2番目、クエスト文が3番目）ことに合わせて更新し、引き続きパスすること
- [ ] ケース6（回帰）: `to_line_message`が`ReplyMessage::Quest`の`intro`を`build_quest_flex_message`にそのまま渡すこと

## 実装仕様

### src/services/game_service.rs

`ReplyMessage`の`Quest`バリアントに`intro`を追加:

```rust
pub enum ReplyMessage {
    Text(String),
    Quest {
        intro: String,
        room_name: String,
        quest_text: String,
        image_url: Option<String>,
    },
    Cleared { elapsed: String },
}
```

`quest_reply_for_room`に`intro`引数を追加:

```rust
async fn quest_reply_for_room(
    pool: &MySqlPool,
    public_base_url: &str,
    room: &room_repository::Room,
    intro: impl Into<String>,
) -> Result<ReplyMessage, GameServiceError> {
    // ...既存のimage_url組み立てはそのまま...
    Ok(ReplyMessage::Quest {
        intro: intro.into(),
        room_name: room.room_name.clone(),
        quest_text: room.quest_text.clone(),
        image_url,
    })
}
```

3つの呼び出し箇所を変更する:

1. `handle_text_message`内、名前入力を受けて最初の部屋を割り当てる箇所（`quest_reply_for_room(pool, public_base_url, &room)`を呼んでいる箇所）:
   ```rust
   quest_reply_for_room(pool, public_base_url, &room, "最初の部屋は").await
   ```
2. `quest_reply_for_player`関数内（「開始」再送信時の現在の部屋の再送）:
   ```rust
   quest_reply_for_room(pool, public_base_url, &room, "現在向かっている部屋は").await
   ```
3. `checkin`関数内、次の部屋を割り当てる箇所。この時点で変数`room`（`find_by_qr_uuid`で取得した、今回チェックインした＝クリアした部屋）がまだスコープ内にあるので、その`room_name`を使う:
   ```rust
   let intro = format!("【{}】クリアおめでとうございます。次の部屋は", room.room_name);
   let reply = quest_reply_for_room(pool, public_base_url, &next_room, intro).await?;
   ```

### src/services/line_client.rs

`build_quest_flex_message`のシグネチャに`intro`を追加し、`body.contents`の先頭に挿入する:

```rust
pub fn build_quest_flex_message(
    intro: &str,
    room_name: &str,
    quest_text: &str,
    image_url: Option<&str>,
    liff_id: &str,
) -> Value {
    // body.contents を以下の順に変更
    // [
    //   {"type": "text", "text": intro, "size": "sm", "color": "#888888", "wrap": true},
    //   {"type": "text", "text": room_name, "weight": "bold", "size": "xl", "wrap": true},
    //   {"type": "separator"},
    //   {"type": "text", "text": quest_text, "wrap": true, "size": "md", "color": "#555555"}
    // ]
    // header・hero・footerは既存のまま変更しない
}
```

引数の順序は既存の呼び出し箇所（`to_line_message`）に合わせて調整してよいが、`intro`を追加する形にする。

`to_line_message`のQuestバリアントの分岐:

```rust
ReplyMessage::Quest { intro, room_name, quest_text, image_url } =>
    build_quest_flex_message(intro, room_name, quest_text, image_url.as_deref(), liff_id),
```

## 制約・注意事項

- `header`（「次のクエスト」ラベル・背景色`#2E7D32`）・`hero`（画像）・`footer`（QRを読むボタン）は変更しないこと。変更するのは`body`の先頭に1要素追加する点のみ
- クリア演出（`ReplyMessage::Cleared`・`build_cleared_flex_message`）には手を加えないこと。今回の対象はクエスト通知（`ReplyMessage::Quest`）のみ
- 3つの文言（「最初の部屋は」「【部屋名】クリアおめでとうございます。次の部屋は」「現在向かっている部屋は」）は指示書・`docs/architecture.md`の記載と完全に一致させること
- ケース3で参照する「直前にクリアした部屋」は、`checkin`関数内で既にQR照合のために取得済みの`room`変数（`find_by_qr_uuid`の結果）をそのまま使えばよく、追加のDB問い合わせは不要

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
