# 実装指示書: スタンプ・スタンプカードのカスタム画像設定（PR A: DB・管理画面）

## 背景・目的

管理者から「スタンプ自体・スタンプカード台紙自体を画像でカスタマイズしたい」という要望があった。今回は2段階のPRで実装する最初のPR（PR A）で、以下のみを対象とする。

1. 部屋ごとの「スタンプ表示名」（必須・最大4文字。スタンプカード上でその部屋のスタンプに表示する短い文字列。長い部屋名でもスタンプ上で読みやすくするため、`room_name`とは別に管理者が入力する）
2. 部屋ごとの「スタンプ画像」（任意。設定すればその部屋専用のスタンプ画像を使う）
3. イベント全体の「スタンプカード台紙画像」（任意。設定すればスタンプカード画像全体の背景に使う）

**今回のPRでは、DBスキーマと管理画面（部屋登録・編集フォーム、イベント設定画面）への入力欄追加のみを行う。`stamp_card_service::render_png`（実際にスタンプカード画像へ反映する処理）は今回は変更しない。** 反映処理は次のPR（PR B）で別途指示書を作成する。

基本設計は [docs/architecture.md 7節「部屋（チェックポイント）管理の実装方針」](../docs/architecture.md#7-部屋チェックポイント管理の実装方針)・[16節「イベント設定画面」](../docs/architecture.md#16-イベント設定画面adminsettingsslice-c)・[23節「追記: 部屋ごとのカスタムスタンプ画像・カード台紙画像への対応」](../docs/architecture.md#stamp_card_servicerender_png)を参照。DB変更は [docs/database.md](../docs/database.md) の`rooms`・`events`テーブル定義、APIは [docs/api.md](../docs/api.md) の管理画面セクションを参照。

## 実装対象ファイル

- `migrations/0004_stamp_customization.sql`（新規） — `rooms.stamp_label`・`rooms.stamp_image_id`・`events.stamp_card_background_image_id`の追加
- `src/repository/room_repository.rs` — `Room`に`stamp_label`・`stamp_image_id`を追加、`insert`/`update`のシグネチャ変更、SELECT文の列リスト更新
- `src/repository/event_repository.rs` — `Event`に`stamp_card_background_image_id`を追加、`update_settings`のシグネチャ変更、SELECT文の列リスト更新
- `src/services/room_service.rs` — `CreateRoomInput`/`UpdateRoomInput`に`stamp_label: String`・`stamp_image_bytes: Option<Vec<u8>>`を追加。スタンプ表示名のバリデーション（1〜4文字）、スタンプ画像の保存・張り替え・削除（既存の`image_bytes`と同じパターン）
- `src/services/event_service.rs` — `SettingsInput`に`stamp_card_background_image_bytes: Option<Vec<u8>>`を追加。台紙画像の保存・張り替え（既存の部屋画像と同じパターン）
- `src/handlers/rooms.rs` — `parse_room_multipart`に`stamp_label`・`stamp_image`フィールドを追加。`add`/`update`ハンドラーの入力組み立てを更新。バリデーションエラー時のフォーム再表示に対応
- `src/handlers/admin.rs` — `settings_form`/`update_settings`を`Form`から`Multipart`に変更し、台紙画像アップロードに対応
- `templates/admin/rooms/add.html` / `templates/admin/rooms/edit.html` — スタンプ表示名の入力欄（`maxlength="4"`・`required`）とスタンプ画像のファイル入力欄を追加
- `templates/admin/settings.html` — `<form>`に`enctype="multipart/form-data"`を追加し、台紙画像のファイル入力欄を追加

## テストケース（TDDの起点）

### `src/repository/room_repository.rs`

- [ ] ケース1: `insert`に`stamp_label`・`stamp_image_id`を渡して作成した部屋を`find_by_id`で取得すると、両方の値がそのまま返る
- [ ] ケース2: `update`で`stamp_label`・`stamp_image_id`を更新できる

### `src/repository/event_repository.rs`

- [ ] ケース3: `update_settings`に`stamp_card_background_image_id`を渡して更新すると、`find_singleton`で取得したイベントにその値が反映されている

### `src/services/room_service.rs`

- [ ] ケース4: `stamp_label`が空文字列の場合、`create`が`RoomError::StampLabelInvalid`を返し、部屋は作成されない
- [ ] ケース5: `stamp_label`が5文字以上（例: `"12345"`）の場合、`create`が`RoomError::StampLabelInvalid`を返す（文字数は`chars().count()`で数える。マルチバイト文字を考慮するため）
- [ ] ケース6: `stamp_label`が1〜4文字（例: `"図書"`）なら`create`が成功し、保存された部屋の`stamp_label`が一致する
- [ ] ケース7: `stamp_image_bytes`を指定して`create`すると、`room_images`に新しい行が作られ、部屋の`stamp_image_id`がそのIDを指す
- [ ] ケース8: 既存のスタンプ画像がある部屋を、新しい`stamp_image_bytes`で`update`すると、新しい`room_images`行が作られて`stamp_image_id`が張り替わり、更新前の`room_images`行は削除される（`image_id`の既存の張り替えロジックと同じ順序: 新規挿入 → FK張り替え → 旧行削除）
- [ ] ケース9: `stamp_image_bytes`を指定せずに`update`すると、既存の`stamp_image_id`はそのまま維持される
- [ ] ケース10（回帰）: スタンプ画像を持つ部屋を`delete`すると、対応する`room_images`行も削除される（既存の`image_id`削除ロジックと同様、`stamp_image_id`についても行う）

### `src/services/event_service.rs`

- [ ] ケース11: `stamp_card_background_image_bytes`を指定して`update_settings`すると、`room_images`に新しい行が作られ、イベントの`stamp_card_background_image_id`がそのIDを指す
- [ ] ケース12: 既存の台紙画像がある状態で新しい`stamp_card_background_image_bytes`を指定して`update_settings`すると、新しい行に張り替わり、旧`room_images`行は削除される
- [ ] ケース13: `stamp_card_background_image_bytes`を指定せずに`update_settings`すると、既存の`stamp_card_background_image_id`はそのまま維持される

### `src/handlers/rooms.rs`

- [ ] ケース14: `stamp_label`・`stamp_image`を含むmultipartで`POST /admin/rooms/add`すると、部屋が作成され`/admin/rooms`にリダイレクトされる
- [ ] ケース15: `stamp_label`を含まない（または空の）multipartで`POST /admin/rooms/add`すると、200でフォームが再表示され、スタンプ表示名のエラーメッセージが含まれる（既存の`AnswerRequired`のエラー表示パターンと同様）

### `src/handlers/admin.rs`

- [ ] ケース16（回帰）: 画像を含まないmultipartで`POST /admin/settings`しても、既存の`is_team_mode`・`require_answer_check`の更新が引き続き成功する（`Form`から`Multipart`への変更による既存機能の回帰がないこと）
- [ ] ケース17: 台紙画像を含むmultipartで`POST /admin/settings`すると、設定が更新されリダイレクトされる

## 実装仕様

### `migrations/0004_stamp_customization.sql`

TiDBでは1つの`ALTER TABLE`文で列追加とインデックス/外部キー追加を同時に行うと失敗することがある（`docs/architecture.md` 23節の追記・過去の障害を参照）。そのため各変更を個別の`ALTER TABLE`文に分割する。

```sql
ALTER TABLE rooms
    ADD COLUMN stamp_label VARCHAR(4) NULL;

ALTER TABLE rooms
    ADD COLUMN stamp_image_id INT NULL;

ALTER TABLE rooms
    ADD KEY idx_rooms_stamp_image_id (stamp_image_id);

ALTER TABLE rooms
    ADD CONSTRAINT fk_rooms_stamp_image_id
        FOREIGN KEY (stamp_image_id) REFERENCES room_images (id)
        ON DELETE SET NULL;

ALTER TABLE events
    ADD COLUMN stamp_card_background_image_id INT NULL;

ALTER TABLE events
    ADD KEY idx_events_stamp_card_background_image_id (stamp_card_background_image_id);

ALTER TABLE events
    ADD CONSTRAINT fk_events_stamp_card_background_image_id
        FOREIGN KEY (stamp_card_background_image_id) REFERENCES room_images (id)
        ON DELETE SET NULL;
```

いずれもNULL許容のカラムなので、既存行がある状態で流してもNOT NULL制約違反は起こらない。バックフィルは不要（`stamp_label`が`NULL`の既存部屋は、PR Bで`stamp_card_service`側が`room_name`からのフォールバックで扱う。`docs/architecture.md` 7節参照）。

### `src/repository/room_repository.rs`

- `Room`構造体に`pub stamp_label: Option<String>,`・`pub stamp_image_id: Option<i32>,`を追加する
- `insert`・`update`のシグネチャに`stamp_label: Option<&str>`・`stamp_image_id: Option<i32>`を追加し、SQL文の列リスト・バインドにも追加する（既存の`image_id`と同じ形で列挙するだけでよい）
- `find_all`・`find_by_id`のSELECT文の列リストに`stamp_label, stamp_image_id`を追加し、`Room`組み立て箇所にも追加する

### `src/repository/event_repository.rs`

- `Event`構造体に`pub stamp_card_background_image_id: Option<i32>,`を追加する
- `update_settings`のシグネチャに`stamp_card_background_image_id: Option<i32>`を追加し、`UPDATE events SET ..., stamp_card_background_image_id = ? WHERE id = ?`に反映する
- `find_singleton`のSELECT文の列リストに`stamp_card_background_image_id`を追加し、`Event`組み立て箇所にも追加する

### `src/services/room_service.rs`

- `RoomError`に`StampLabelInvalid`を追加する
- `CreateRoomInput`・`UpdateRoomInput`に`pub stamp_label: String,`・`pub stamp_image_bytes: Option<Vec<u8>>,`を追加する
- `create`・`update`の冒頭付近（`answer`の必須チェックと同様の位置）で以下のバリデーションを行う:
  ```rust
  let stamp_label_len = input.stamp_label.chars().count();
  if stamp_label_len == 0 || stamp_label_len > 4 {
      return Err(RoomError::StampLabelInvalid);
  }
  ```
- スタンプ画像の保存・張り替えは、既存の`image_bytes` → `image_id`の処理（`image_service::process_upload` → `room_image_repository::insert` → 必要なら旧`room_image_repository::delete`）と全く同じ順序・ロジックを`stamp_image_bytes` → `stamp_image_id`についても行う（`create`では単純に挿入するだけ、`update`では既存の`existing.stamp_image_id`を見て張り替え/維持を判断する）
- `delete`関数で、`existing.image_id`の削除に加えて`existing.stamp_image_id`が`Some`ならその`room_images`行も削除する

### `src/services/event_service.rs`

- `SettingsInput`に`pub stamp_card_background_image_bytes: Option<Vec<u8>>,`を追加する
- `update_settings`で、`stamp_card_background_image_bytes`が`Some`の場合は`room_service`の画像張り替えロジックと同じ順序（新規挿入 → `event_repository::update_settings`でFK張り替え → 旧`room_images`行があれば削除）を行う。`room_image_repository`・`image_service`は`room_service`と同様にインポートして使う
- 画像が指定されない場合は、既存の`event.stamp_card_background_image_id`をそのまま`update_settings`に渡す（変更しない）

### `src/handlers/rooms.rs`

- `RoomMultipartForm`に`stamp_label: String`・`stamp_image_bytes: Option<Vec<u8>>`を追加し、`parse_room_multipart`のフィールド解析ループに`"stamp_label"`（テキスト）・`"stamp_image"`（ファイル、既存の`"image"`と同じ扱い）の分岐を追加する
- `AddTemplateValues`（および編集フォーム側の同等の構造体）に`stamp_label: String`を追加し、バリデーションエラーで再表示する際も入力値が消えないようにする（既存の`room_name`・`quest_text`と同様）
- `add`・`update`ハンドラーで、`room_service::CreateRoomInput`/`UpdateRoomInput`組み立てに`stamp_label`・`stamp_image_bytes`を追加する
- `RoomError::StampLabelInvalid`に対応するエラーメッセージ（例: `"スタンプ表示名は1〜4文字で入力してください"`）でフォームを再表示する分岐を、既存の`RoomError::AnswerRequired`の分岐と同じ形で追加する

### `src/handlers/admin.rs`

- `settings_form`・`update_settings`の引数を`Form<SettingsForm>`から`Multipart`に変更する。パース処理は`src/handlers/rooms.rs`の`parse_room_multipart`と同様の形（`multipart.next_field()`のループ）で、`is_team_mode`・`require_answer_check`（チェックボックス、値`"on"`の有無で判定）・`csrf_token`・`stamp_card_background_image`（ファイル）を読み取る
- `SettingsForm`のチェックボックス処理（`"on"`/`"true"`のカスタムデシリアライザ）は、`Form`抽出前提の実装だったため、`Multipart`の手動パースに合わせて「フィールドが存在すれば`true`」というロジックに書き換える（HTMLの`<input type="checkbox">`は未チェック時にフィールド自体が送られない仕様は変わらないため、実質的な判定内容は同じでよい）

### `templates/admin/rooms/add.html` / `templates/admin/rooms/edit.html`

`画像`欄の直後に以下を追加する（`edit.html`も同様の構成にする）:

```html
<div class="mb-3"><label class="form-label">スタンプ表示名</label><input class="form-control" type="text" name="stamp_label" value="{{ stamp_label }}" maxlength="4" required></div>
<div class="mb-3"><label class="form-label">スタンプ画像（任意）</label><input class="form-control" type="file" name="stamp_image" accept="image/png,image/jpeg"></div>
```

### `templates/admin/settings.html`

`<form>`タグに`enctype="multipart/form-data"`を追加し、保存ボタンの手前に以下を追加する:

```html
<div class="mb-3"><label class="form-label">スタンプカード台紙画像（任意）</label><input class="form-control" type="file" name="stamp_card_background_image" accept="image/png,image/jpeg"></div>
```

## 制約・注意事項

- 今回は`stamp_card_service::render_png`・`GET /public/stamp-card/{token}`ハンドラーを変更しない。DBに保存された`stamp_label`・`stamp_image_id`・`stamp_card_background_image_id`は、この時点ではまだスタンプカード画像に反映されない（PR Bで対応）。この点をPRの説明に明記すること
- 画像アップロードの検証（マジックバイト・サイズ上限・リサイズ）は既存の`image_service::process_upload`をそのまま再利用し、新しい検証ロジックは追加しない
- `stamp_label`の文字数チェックは`chars().count()`（Unicodeのスカラー値単位）で行う。バイト数（`len()`）で数えないこと（日本語は1文字が複数バイトになるため）
- `rooms.stamp_image_id`・`events.stamp_card_background_image_id`のいずれも`room_images`を参照する（新しいテーブルは作らない）
- 既存の部屋・イベントデータ（`stamp_label`が`NULL`の行）に対する一括更新・バックフィルは行わない

## 完了条件

- [ ] 上記17テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] 管理画面で実際に部屋登録・編集・イベント設定を操作し、スタンプ表示名・スタンプ画像・台紙画像が保存されることを目視確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
