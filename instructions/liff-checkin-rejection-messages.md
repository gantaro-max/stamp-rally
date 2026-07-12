# 実装指示書: LIFFチェックイン失敗時のメッセージを理由別に出し分ける

## 背景・目的

本番運用の動作確認で、LIFFページ（`/liff/checkin`）でのチェックイン失敗時に、理由（`reason`）によらず単一の汎用メッセージ「チェックインできませんでした。LINEチャットの案内を確認してください。」しか表示されないことが分かった。特に`wrong_room`（案内されていない部屋のQRを読んだ場合）で、参加者が「なぜ失敗したのか」「次に何をすればいいのか」が分からず混乱する。

サーバー側（`POST /liff/checkin`）は既に`{"status": "rejected", "reason": "wrong_room"}`のように理由を区別して返している（[src/handlers/liff.rs](../src/handlers/liff.rs)の`rejection_response`関数）。この`reason`をLIFFページのクライアント側JS（[templates/liff/checkin.html](../templates/liff/checkin.html)）で読み取り、理由ごとにメッセージを出し分けるよう変更する。

設計の詳細は[docs/architecture.md 15節「レスポンス設計」](../docs/architecture.md#レスポンス設計)のメッセージ対応表を参照。

## 実装対象ファイル

- `templates/liff/checkin.html` — チェックイン失敗時のメッセージ表示ロジックを、`reason`ごとの出し分けに変更
- `src/handlers/liff.rs` — レンダリング結果を確認するテストを追加（Rustコード自体の変更は不要）

## テストケース（TDDの起点）

このテンプレートはAskamaで静的にレンダリングされ（`{{ liff_id }}`部分のみ動的）、JSの分岐ロジック自体は静的なテキストとしてHTMLに埋め込まれる。そのため、既存の`get_checkin_page_contains_liff_id`と同じパターン（レンダリング結果のHTML文字列に対する`contains`アサーション）でテストする。

- [ ] ケース1: `GET /liff/checkin`のレスポンスHTMLに、`docs/architecture.md`15節の対応表にある6つのメッセージ文言（`wrong_room`用「このQRコードはご案内している部屋のものではありません。LINEチャットで案内されている部屋をご確認ください。」を含む全6件）がすべて含まれること
- [ ] ケース2: レスポンスHTMLの埋め込みJS内に、`reason`の値（`wrong_room`, `already_finished`, `not_registered`, `answer_not_verified`, `room_not_found`, `invalid_id_token`）ごとの分岐（`if`/`switch`等、実装方法は問わない）が存在すること（ケース1と合わせて、単に6文言を羅列しているだけでなく実際に`reason`で出し分けるロジックになっていることを確認する）
- [ ] ケース3（回帰）: 既存の`get_checkin_page_contains_liff_id`（`liff_id`がHTMLに含まれること）が引き続きパスすること
- [ ] ケース4（回帰）: `status === 'next'` / `status === 'cleared'`のときのメッセージ（「チェックインしました。次の案内はLINEチャットを確認してください。」「クリアしました。LINEチャットを確認してください。」）は変更しないこと

## 実装仕様

### templates/liff/checkin.html

現在の以下の部分:

```js
} else {
    message.textContent = 'チェックインできませんでした。LINEチャットの案内を確認してください。';
}
```

を、`body.reason`で分岐する形に変更する。例:

```js
} else {
    const reasonMessages = {
        wrong_room: 'このQRコードはご案内している部屋のものではありません。LINEチャットで案内されている部屋をご確認ください。',
        already_finished: '既に全部屋クリア済みです。',
        not_registered: '参加登録が完了していません。LINEで「開始」と送信してください。',
        answer_not_verified: '先にLINEで正解を送信してから、QRコードを読み込んでください。',
        room_not_found: '無効なQRコードです。もう一度お試しください。',
        invalid_id_token: '認証に失敗しました。時間をおいてもう一度お試しください。'
    };
    message.textContent = reasonMessages[body.reason] || 'チェックインできませんでした。LINEチャットの案内を確認してください。';
}
```

（`||`のフォールバックは、`reason`が将来追加される・想定外の値が来た場合に空表示にならないための保険。既存の汎用メッセージをそのままフォールバック文言として流用する）

文言は`docs/architecture.md`15節の対応表と一字一句一致させること。

### src/handlers/liff.rs

`mod tests`内の`get_checkin_page_contains_liff_id`テスト、または新規テスト関数に、上記テストケース1・2に対応するアサーションを追加する。レンダリング結果は既存テストと同じ`GET /liff/checkin`のレスポンスボディ文字列から取得できる。

## 制約・注意事項

- サーバー側（`src/handlers/liff.rs`の`checkin`関数・`rejection_response`関数・`game_service`）のロジックには一切手を加えないこと。今回はLIFFページのクライアント側表示のみの変更
- `status === 'next'` / `'cleared'`時のメッセージ、QRスキャン失敗時（`catch`節）のメッセージ、LIFF初期化失敗時のメッセージは変更しないこと
- 文言は指示書・`docs/architecture.md`の対応表と完全に一致させること（表記ゆれを作らない）

## 完了条件

- [ ] 上記テストケースについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 最小限の実装（テンプレートのJS修正）でテストを通した（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
