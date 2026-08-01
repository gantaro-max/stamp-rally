# 実装指示書: クリア時にもスタンプカード画像を送信する

## 背景・目的

`CheckinOutcome::Cleared`（全部屋クリア時）に送るメッセージは、これまでクリア専用のFlex Message（`build_cleared_flex_message`、クリアタイム等の文言のみ）1通だけだった。一方、クエスト案内時（`ReplyMessage::Quest`）は既にクエストFlex Message＋スタンプカード画像の2通構成になっている。

実際にプレイして確認したところ、ゴール時だけスタンプカード画像が付随せず、全スタンプが揃った状態を視覚的に見せる演出が欠けていたため、「集めきった」達成感が伝わりにくいという課題があった。クエスト案内時と同様、クリア時にも全スタンプが揃ったスタンプカード画像を添えるように変更する。

基本設計は [docs/architecture.md 23節「追記: クリア時にもスタンプカード画像を送信する（今後の拡張）」](../docs/architecture.md#追記-クリア時にもスタンプカード画像を送信する今後の拡張)を参照。要件は[docs/requirements.md](../docs/requirements.md)「ゴール」項を参照。

**スコープ外（今回は対応しない）**: クリア専用Flex Message（`build_cleared_flex_message`）自体の文言・レイアウト変更。今回はスタンプカード画像を追加で送るだけで、既存のクリア演出自体は変更しない。

## 実装対象ファイル

- `src/services/game_service.rs` — `ReplyMessage::Cleared`に`stamp_card_url: String`を追加。`cleared_reply`に`public_base_url: &str`引数を追加し、`stamp_card_url()`ヘルパー（既存）でURLを組み立てる。呼び出し元`checkin`関数内2箇所の`cleared_reply(&player)`呼び出しに`public_base_url`を渡す
- `src/services/line_client.rs` — `to_line_messages`の`ReplyMessage::Cleared`ケースを、`build_cleared_flex_message`と`build_stamp_status_image_message`の2件を返すよう変更

他のファイルは変更しない（`handlers::line_webhook`は`ReplyMessage::Cleared`を直接パターンマッチしておらず、`checkin`・`to_line_messages`の戻り値をそのまま使っているため無変更で動作する）。

## テストケース（TDDの起点）

### `src/services/game_service.rs`

- [ ] ケース1（既存テスト拡張）: `checkin_last_room_marks_finished_and_returns_cleared`で、返る`ReplyMessage::Cleared { elapsed, stamp_card_url }`の`stamp_card_url`が、対象プレイヤーの`stamp_card_token`から組み立てたURL（`{PUBLIC_BASE_URL}/public/stamp-card/{token}`の形。既存の`quest_reply_for_room`関連テストと同じ組み立て方）と一致することを確認する（正常系）

### `src/services/line_client.rs`

- [ ] ケース2（既存テスト拡張）: `to_line_messages_builds_cleared_flex_message`で、`ReplyMessage::Cleared { elapsed, stamp_card_url }`を渡したとき、返るメッセージ配列が`vec![build_cleared_flex_message(elapsed), build_stamp_status_image_message(stamp_card_url)]`と一致する（要素数2、1件目がクリアFlex Message、2件目が渡した`stamp_card_url`を使った画像メッセージ）ことを確認する（正常系）
- [ ] ケース3（回帰）: `build_cleared_flex_message`自体の出力（`build_cleared_flex_message_includes_elapsed_time`）は無変更で通ること（新規テスト追加不要、既存テストがそのまま通ることの確認で足りる）
- [ ] ケース4（回帰）: `ReplyMessage::Quest`・`ReplyMessage::StampStatus`・`ReplyMessage::Text`のケースは無変更で通ること（新規テスト追加不要、既存テストがそのまま通ることの確認で足りる）

## 実装仕様

### `src/services/game_service.rs`

`ReplyMessage::Cleared`に`stamp_card_url`フィールドを追加する（`ReplyMessage::Quest`と同じ形）。

```rust
Cleared {
    elapsed: String,
    stamp_card_url: String,
},
```

`cleared_reply`関数に`public_base_url: &str`引数を追加し、既存の`stamp_card_url(public_base_url, &player.stamp_card_token)`ヘルパー（本ファイルに既存）でURLを組み立てる。

```rust
fn cleared_reply(public_base_url: &str, player: &player_repository::Player) -> ReplyMessage {
    let elapsed = crate::services::ranking_service::format_elapsed(
        chrono::Utc::now().naive_utc() - player.started_at,
    );
    ReplyMessage::Cleared {
        elapsed,
        stamp_card_url: stamp_card_url(public_base_url, &player.stamp_card_token),
    }
}
```

`checkin`関数内の2箇所の呼び出し（`Ok(CheckinOutcome::Cleared(cleared_reply(&player)))`）を、いずれも`cleared_reply(public_base_url, &player)`に変更する（`checkin`関数は既に`public_base_url: &str`を引数に持っているので、それをそのまま渡すだけでよい）。

### `src/services/line_client.rs`

`to_line_messages`の`ReplyMessage::Cleared`ケースを変更する。

```rust
ReplyMessage::Cleared { elapsed, stamp_card_url } => vec![
    build_cleared_flex_message(elapsed),
    build_stamp_status_image_message(stamp_card_url),
],
```

`build_cleared_flex_message`・`build_stamp_status_image_message`関数自体の中身は変更しない。

## 制約・注意事項

- クリア専用Flex Message（`build_cleared_flex_message`）の文言・レイアウトは変更しない
- LINE Reply APIは1回のリプライで最大5メッセージまで送信可能という制約がある。今回追加後もクリア時は2メッセージ（Flex Message＋画像）であり、既存の`Quest`（2メッセージ）と同じ件数のため問題ない
- `stamp_card_url`の組み立ては新規ロジックを書かず、既存の`stamp_card_url()`ヘルパー関数を再利用すること
- ゴール到達時点で全部屋訪問済みのため、返る`/public/stamp-card/{token}`の画像は自動的に全マス埋まった状態になる（`stamp_card_service`側の変更は不要）

## 完了条件

- [ ] 上記4テストケースについて、実装前に失敗するテストを書いたことを確認した（Red。ケース3・4は既存テストの回帰確認のため、既存テストの内容を変更せずそのまま通ることの確認でよい）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ローカルの管理画面・LINE Bot経由で実際に全部屋をクリアし、クリアメッセージに続けてスタンプカード画像（全マス埋まった状態）が届くことを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
