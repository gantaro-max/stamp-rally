# 実装指示書: 管理画面での現在アップロード済み画像のプレビュー表示

## 背景・目的

部屋登録・編集画面（`/admin/rooms/*`）とイベント設定画面（`/admin/settings`）の画像アップロード欄（クエスト画像・スタンプ画像・スタンプカード台紙画像）は、いずれもファイル選択の`<input type="file">`のみで、現在何がアップロードされているか（設定済みかどうか、設定済みならどんな画像か）が画面上で分からないという課題があった。実際の運用で、この見えにくさが改善要望として挙がった。

基本設計は [docs/architecture.md 24節「管理画面での現在アップロード済み画像のプレビュー表示（今後の拡張）」](../docs/architecture.md#24-管理画面での現在アップロード済み画像のプレビュー表示今後の拡張)を参照。

**スコープ外（今回は対応しない）**: 画像の削除専用ボタン（未設定に戻す機能）、サムネイルのクリックでの拡大表示・モーダル表示。

## 実装対象ファイル

- `src/handlers/image.rs` — 相対URLを組み立てる純粋関数`public_image_url`を追加
- `src/handlers/rooms.rs` — `RoomListTemplate`を表示用アイテムのVecに変更、`RoomEditTemplate`/`RoomEditTemplateValues`に画像URLフィールドを追加
- `src/handlers/admin.rs` — `SettingsTemplate`に台紙画像URLフィールドを追加
- `templates/admin/rooms/list.html` — クエスト画像・スタンプ画像列を追加
- `templates/admin/rooms/edit.html` — 各ファイル入力欄の上にプレビューを追加
- `templates/admin/settings.html` — 台紙画像入力欄の上にプレビューを追加

`templates/admin/rooms/add.html`・`src/handlers/rooms.rs`の`add`/`add_form`関連コードは変更しない（新規登録時点では画像が存在しないため対象外）。

## テストケース（TDDの起点）

### `src/handlers/image.rs`

- [ ] ケース1: `public_image_url("abc-123")`が`"/public/image/abc-123"`を返す（正常系、純粋関数のユニットテスト。DB不要）

### `src/handlers/rooms.rs`

- [ ] ケース2: `image_url`・`stamp_image_url`がいずれも`Some("/public/image/xxx")`のとき、一覧テンプレート（`RoomListTemplate`または新設する表示用アイテムを使うテンプレート）のレンダリング結果に`<img src="/public/image/xxx"`という文字列が含まれる（既存の`room_templates_include_logout_csrf_token`と同様、DB不要のテンプレートレンダリングテストでよい）
- [ ] ケース3: `image_url`・`stamp_image_url`がいずれも`None`のとき、レンダリング結果に「未設定」という文字列が含まれる
- [ ] ケース4: `RoomEditTemplate`について、`image_url`が`Some(...)`のときはサムネイル`<img>`タグが、`None`のときは「未設定」がレンダリング結果に含まれる（`stamp_image_url`についても同様）
- [ ] ケース5（DB統合・正常系）: `#[sqlx::test]`で、`image_id`が設定された部屋を`room_repository::insert`等で作成し、`GET /admin/rooms/edit/{id}`（`edit_form`）の実際のレスポンスHTMLに、その部屋の画像の実際のUUIDを使った`/public/image/{uuid}`が含まれることを確認する。認証済みセッションでのルーターテストのセットアップは`src/main.rs`の`#[cfg(test)]`モジュール（`seed_authenticated_logout_session`周辺、`SessionManagerLayer`・`MemoryStore`の使い方）を参考にすること

### `src/handlers/admin.rs`

- [ ] ケース6: `SettingsTemplate`について、`stamp_card_background_image_url`が`Some(...)`のときはサムネイル`<img>`タグが、`None`のときは「未設定」がレンダリング結果に含まれる（DB不要のテンプレートレンダリングテスト）
- [ ] ケース7（DB統合・正常系）: `#[sqlx::test]`で、`stamp_card_background_image_id`が設定されたイベントに対して`GET /admin/settings`（`settings_form`）の実際のレスポンスHTMLに、実際のUUIDを使った`/public/image/{uuid}`が含まれることを確認する

### 回帰確認（新規テスト追加不要）

- [ ] ケース8: 既存の`room_templates_include_logout_csrf_token`がそのまま通ること（`RoomAddTemplate`は変更しないため無影響のはず）
- [ ] ケース9: `rooms::update`のバリデーションエラー時（`StampLabelInvalid`・`AnswerRequired`・`Image`）の再描画が、画像URLフィールド追加後も引き続き正しく動作すること（新規`#[sqlx::test]`を書いてもよいし、手動確認でもよい）

## 実装仕様

### `src/handlers/image.rs`

```rust
pub fn public_image_url(uuid: &str) -> String {
    format!("/public/image/{uuid}")
}
```

既存の`serve`ハンドラーや`stamp_card`ハンドラーの実装は変更しない。

### `src/handlers/rooms.rs`

`RoomListTemplate`を、`Room`をそのまま渡すのではなく表示用アイテムのVecに変更する。

```rust
struct RoomListItem {
    id: i32,
    room_name: String,
    image_url: Option<String>,
    stamp_image_url: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/rooms/list.html")]
struct RoomListTemplate {
    rooms: Vec<RoomListItem>,
    csrf_token: String,
}
```

`list`関数内で、`room_service::list`が返す`Vec<Room>`の各要素について、`image_id`・`stamp_image_id`が`Some`ならそれぞれ`room_image_repository::find_uuid_by_id`でUUIDを引き、`Some`が返れば`public_image_url`でURLに変換する（`find_uuid_by_id`が`None`を返す場合＝参照が壊れている想定外ケースは、素直に`None`として扱ってよい）。`RoomListItem`に詰め替えてテンプレートへ渡す。

`RoomEditTemplate`・`RoomEditTemplateValues`に以下を追加する。

```rust
struct RoomEditTemplate {
    // ...既存フィールド...
    image_url: Option<String>,
    stamp_image_url: Option<String>,
}

struct RoomEditTemplateValues {
    // ...既存フィールド...
    image_url: Option<String>,
    stamp_image_url: Option<String>,
}
```

`edit_form`関数内で、取得した`room`の`image_id`・`stamp_image_id`から同様にURLを解決し、`RoomEditTemplateValues`にセットする。

`update`関数のバリデーションエラー時の再描画でも同じ情報が必要になる。`update`関数の冒頭付近（`room_service::update`を呼ぶ前）で`room_service::get(&pool, id)`を呼び、その結果から`image_id`・`stamp_image_id`を解決したURLを`values`（`RoomEditTemplateValues`）にセットしてからエラー分岐に渡す（`room_service::update`が失敗する場合、画像の張り替えはまだ行われていないため、更新前の`room`が持つ画像参照＝現在有効な画像を指す）。

### `src/handlers/admin.rs`

`SettingsTemplate`に以下を追加する。

```rust
struct SettingsTemplate {
    // ...既存フィールド...
    stamp_card_background_image_url: Option<String>,
}
```

`settings_form`関数内で、`event.stamp_card_background_image_id`から同様にURLを解決してセットする。

### `templates/admin/rooms/list.html`

`<thead>`に「クエスト画像」「スタンプ画像」列を追加し、各行で以下のように表示する（Askamaの`{% if let Some(url) = ... %}`構文を使用）。

```html
<td>
  {% if let Some(url) = room.image_url %}
    <img src="{{ url }}" alt="クエスト画像" width="48" height="48" style="object-fit: cover;">
  {% else %}
    <span class="text-muted">未設定</span>
  {% endif %}
</td>
<td>
  {% if let Some(url) = room.stamp_image_url %}
    <img src="{{ url }}" alt="スタンプ画像" width="48" height="48" style="object-fit: cover;">
  {% else %}
    <span class="text-muted">未設定</span>
  {% endif %}
</td>
```

### `templates/admin/rooms/edit.html`

「画像」「スタンプ画像」の各`<div class="mb-3">`内、ファイル入力の直前にプレビューを追加する。

```html
<div class="mb-3">
  <label class="form-label">画像</label>
  {% if let Some(url) = image_url %}
    <div class="mb-2"><img src="{{ url }}" alt="現在のクエスト画像" style="max-width: 160px; max-height: 160px;"></div>
  {% else %}
    <div class="mb-2 text-muted">未設定</div>
  {% endif %}
  <input class="form-control" type="file" name="image" accept="image/png,image/jpeg">
</div>
```

「スタンプ画像」欄も同様のパターンで`stamp_image_url`を使う。

### `templates/admin/settings.html`

「スタンプカード台紙画像」欄も同様のパターンで`stamp_card_background_image_url`を使う。

## 制約・注意事項

- `/public/image/{uuid}`エンドポイント自体（`src/handlers/image.rs`の`serve`関数）は変更しない
- サムネイルの表示サイズはCSSインラインスタイルで最小限に留める（新しいCSSファイル・デザインシステムの拡張は不要）
- `RoomListTemplate`の構造変更に伴い、テンプレート内で`room.id`・`room.room_name`のような既存のフィールド参照が壊れないよう、`RoomListItem`に必要なフィールドを過不足なく含めること
- `room_service::get`の追加呼び出し（`update`関数内）は、既存の`room_service::update`が内部で行う可能性のある同等の取得と重複しても構わない（早すぎる最適化は不要。1リクエストあたり数クエリ増える程度は許容範囲）

## 完了条件

- [ ] 上記9件のテストケースについて、実装前に失敗するテストを書いたことを確認した（Red。ケース8・9は既存テストの回帰確認、または軽微な追加確認のため必須ではない）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ローカルの管理画面で、実際に画像をアップロード済みの部屋・イベントについて、一覧・編集・設定の各画面でサムネイルが表示されること、未設定の場合は「未設定」と表示されることを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
