# 実装指示書: LIFFチェックイン結果表示後に「LINEチャットに戻る」ボタンを追加

## 背景・目的

本番運用の動作確認で、QRコードを読み込んでチェックインした後、参加者が次に何をすればいいか分かりにくいというフィードバックがあった。結果メッセージ（成功/クリア/拒否のいずれか）は表示されるが、そこからLINEチャットに戻る手段はLIFFブラウザを自分で閉じるしかない状態だった。

[docs/architecture.md 15節「LIFFページ（`GET /liff/checkin`）」](../docs/architecture.md#liffページget-liffcheckin)に、チェックイン結果表示後に「LINEチャットに戻る」ボタンを表示し、LIFF SDKの`liff.closeWindow()`でLIFFブラウザを閉じてトーク画面に戻る設計を追記済み。本指示書はこれに基づく実装指示。

## 実装対象ファイル

- `templates/liff/checkin.html` — 結果表示後に表示する「LINEチャットに戻る」ボタンを追加
- `src/handlers/liff.rs` — レンダリング結果を確認するテストを追加（Rustコード自体の変更は不要）

## テストケース（TDDの起点）

既存の`get_checkin_page_contains_liff_id`等と同じパターン（レンダリング結果のHTML文字列に対する`contains`アサーション）でテストする。

- [ ] ケース1: `GET /liff/checkin`のレスポンスHTMLに、「LINEチャットに戻る」という文言のボタン要素が含まれ、初期状態では非表示（例: `d-none`クラスやインラインスタイル`display: none`等、実装方法は問わないが「初期状態で見えない」ことがHTML/CSSの記述から判断できる形）になっていること
- [ ] ケース2: レスポンスHTMLの埋め込みJS内に、`liff.closeWindow()`を呼び出すコードが含まれていること
- [ ] ケース3: レスポンスHTMLの埋め込みJS内で、チェックイン結果（`status`が`next`/`cleared`/それ以外＝拒否のいずれか）を受け取った後に、ケース1のボタンを表示状態に切り替える処理（非表示クラス・スタイルの解除）が行われていること
- [ ] ケース4（回帰）: 既存の`get_checkin_page_contains_liff_id`・`get_checkin_page_contains_reason_specific_rejection_messages`・`get_checkin_page_keeps_success_messages`が引き続きパスすること

## 実装仕様

### templates/liff/checkin.html

`<button id="scan-button" ...>QRを読む</button>`の直後に、初期状態で非表示のボタンを追加する:

```html
<button id="close-button" type="button" class="btn btn-secondary w-100 mt-2 d-none">LINEチャットに戻る</button>
```

JS側で、3つの結果分岐（`next` / `cleared` / それ以外の拒否）のいずれに入った場合も、メッセージ設定に加えてこのボタンを表示状態にする。例:

```js
const closeButton = document.getElementById('close-button');

closeButton.addEventListener('click', () => {
    liff.closeWindow();
});

button.addEventListener('click', async () => {
    try {
        // ...既存のfetch処理...
        const body = await response.json();
        if (body.status === 'next') {
            message.textContent = '...';
        } else if (body.status === 'cleared') {
            message.textContent = '...';
        } else {
            // ...既存のreasonMessages処理...
        }
        closeButton.classList.remove('d-none');
    } catch (error) {
        message.textContent = 'QRの読み取りに失敗しました。もう一度お試しください。';
    }
});
```

（`closeButton.classList.remove('d-none')`は`try`ブロックの正常系の末尾、3分岐すべてに共通する位置に1回だけ書けばよい。`catch`節・LIFF初期化失敗時は表示しない）

## 制約・注意事項

- `catch`節（QRスキャン自体の失敗）・LIFF初期化失敗時には、このボタンを表示しないこと（`liff.closeWindow()`を呼べる状態と呼べない状態を区別する必要はないが、設計として「チェックインAPIの結果を受け取れた場合のみ」に限定する）
- 既存の「QRを読む」ボタン・その他のメッセージ文言（前PR #17で追加した理由別メッセージ含む）には手を加えないこと
- サーバー側（`src/handlers/liff.rs`の`checkin`関数・`checkin_page`関数）のロジックには一切手を加えないこと。今回はLIFFページのクライアント側表示のみの変更

## 完了条件

- [ ] 上記テストケースについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 最小限の実装（テンプレートのHTML/JS修正）でテストを通した（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
