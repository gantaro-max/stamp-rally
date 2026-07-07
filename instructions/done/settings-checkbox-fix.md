# 実装指示書: イベント設定画面のチェックボックス送信バグ修正

## 背景・目的

Claudeによる全体最終レビュー（`docs/requirements.md`全項目の要件充足確認）の過程で、`main`に既存のバグが見つかった。`POST /admin/settings`で、チェックボックス（`is_team_mode`・`require_answer_check`）にチェックを入れて送信すると、ハンドラー本体（CSRF検証・`event_service::update_settings`呼び出し）に到達する前に、Axumの`Form`抽出自体が失敗し `422 Unprocessable Entity` が返る。

### 原因

`src/handlers/admin.rs:26-33` の `SettingsForm` は次のようになっている。

```rust
#[derive(Debug, Deserialize)]
pub struct SettingsForm {
    #[serde(default)]
    is_team_mode: bool,
    #[serde(default)]
    require_answer_check: bool,
    csrf_token: String,
}
```

`#[serde(default)]` はフィールドが送信されなかった場合（＝チェックが外れている場合）に`false`として扱うためのもので、これ自体は正しい。しかし、HTMLの`<input type="checkbox">`（`templates/admin/settings.html:10,14`、`value`属性未指定）は、チェックが入っている場合に**値`"on"`を送信する**。`serde_urlencoded`の`bool`デシリアライザは`"true"`/`"false"`という文字列しか受け付けないため、`is_team_mode=on`のような値が来ると、そのフィールドのデシリアライズに失敗し、`Form<SettingsForm>`抽出全体がAxumの`FormRejection`（422）で失敗する。ハンドラー関数の中身（`csrf_service::verify_token`によるCSRF検証を含む）は一切実行されない。

結果として、**管理画面からチェックボックスをONにして設定を保存する操作が常に422エラーで失敗し、設定が一切更新できない**（本番でも同様の不具合が起きる）。

### 既存の失敗テスト（Red状態は既に存在）

`src/main.rs`内の以下2件のテストが、この不具合により現在失敗している。これらは新たに書く必要はなく、本修正のRed状態としてそのまま使用する。

- `tests::post_settings_with_checked_boxes_updates_flags`（`src/main.rs:1448`付近）: チェックボックスON送信で302リダイレクト・DB更新を期待しているが、現状422が返る
- `tests::post_settings_rejects_invalid_csrf_without_changing_db`（`src/main.rs:1562`付近）: 不正なCSRFトークンでの送信（チェックボックスONを含む）で403を期待しているが、現状422が返る（CSRF検証に到達する前に422になっているため）

## 実装対象ファイル

- `src/handlers/admin.rs` — `SettingsForm`のチェックボックスフィールドのデシリアライズ方法を修正

## テストケース（TDDの起点）

- [ ] ケースA（既存・Red確認のみ）: 上記2件の既存テストが、修正前に実際に失敗する（422が返る）ことを確認する。新規のRedテストを書く必要はない
- [ ] ケースB: 修正後、`post_settings_with_checked_boxes_updates_flags`が302を返し、DBの`is_team_mode`/`require_answer_check`が両方`true`に更新されることを確認する（Green）
- [ ] ケースC: 修正後、`post_settings_rejects_invalid_csrf_without_changing_db`が403を返し、DBが更新されないことを確認する（Green）
- [ ] ケースD（回帰確認）: 既存の`post_settings_without_checkbox_fields_sets_flags_false`（チェックなし送信で`false`になる）が、修正後も引き続きパスすることを確認する

## 実装仕様

### src/handlers/admin.rs

- `is_team_mode`・`require_answer_check`に、`"on"`（チェック時にHTMLが送信する値）を`true`として扱うカスタムデシリアライザを追加する。例:

```rust
fn deserialize_checkbox<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(value == "on" || value == "true")
}
```

- `SettingsForm`の両フィールドに `#[serde(default, deserialize_with = "deserialize_checkbox")]` を付与する（`default`と`deserialize_with`は併用可能で、フィールドが送信されない場合は`deserialize_with`は呼ばれず`default`が使われる。送信された場合のみ`deserialize_checkbox`が値の変換を行う）
- ハンドラー本体（CSRF検証・`update_settings`呼び出し）のロジックは変更しない

## 制約・注意事項

- スコープ外の機能追加・リファクタリングを行わないこと
- 他のフォーム（`rooms.rs`等）にはbool型のcheckboxフィールドは存在しないため、本修正は`admin.rs`の`SettingsForm`のみに閉じる（`grep -rn "serde(default)\]" -A1 src/handlers/*.rs`で確認済み）
- `cargo clippy` が警告なく通ること
- コミットは、既存の失敗テストを確認するステップと、実装で修正するステップが分かるように分割すること（例: 最初に現状のテスト失敗を確認するだけのコミットは不要だが、実装コミットのメッセージで「どのテストをGreenにするための修正か」が分かるようにする）

## 完了条件

- [ ] `post_settings_with_checked_boxes_updates_flags`・`post_settings_rejects_invalid_csrf_without_changing_db`の両方がGreenになった
- [ ] `post_settings_without_checkbox_fields_sets_flags_false`が引き続きGreenであることを確認した
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
