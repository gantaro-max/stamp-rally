# 実装指示書: 部屋管理機能 差し戻し修正（最終レビュー指摘対応）

> **注記（2026-07-07 Claude再調査）**: 本指示書は `done/` にアーカイブされていたが、記載された3件の是正はいずれも `main` に反映されていないことが判明した。作業環境の切り替え時にpush漏れがあったと推測される。再指示書 [instructions/room-management-fixes-2.md](../room-management-fixes-2.md) を新規作成したので、今後はそちらを参照すること。本ファイルは経緯記録として残す。

## 背景・目的

`feature/room-management`（部屋管理CRUD・画像アップロード・QRコード表示）はCodexの一次レビューを経て実装済みだが、Claudeによる最終レビュー（設計整合性・セキュリティ・要件充足・実装指示書・TDD遵守の5観点）で以下3件の問題が見つかった。マージ前にこのブランチ上で修正すること。

1. **設計整合性違反**: `src/handlers/rooms.rs` の複数のハンドラーが `room_service` を経由せず `event_repository::find_singleton` を直接呼んでいる。`docs/architecture.md` の Handler → Service → Repository という責務分離に反しており、`room_service::create`/`update` 内部でも同じイベント行を再取得する二重取得になっている。
2. **TDD規約違反**: コミット `fc9efcb`（メッセージは `refactor: implement room error display`）で `RoomError` に `std::fmt::Display` / `std::error::Error` の実装が追加されているが、対応する失敗するテスト（Red）が存在せず、実装後も `src/handlers/rooms.rs` はどこからも `Display`（`{}`／`.to_string()`）を呼んでいない。Refactorコミットに紛れた未検証・未使用コードであり、[AGENTS.md](../AGENTS.md) のTDD規約（Refactorでは新しい振る舞いを追加しない）に反する。
3. **データ整合性のバグ**（セキュリティレビューで発見）: `room_service::update` が画像張り替え時に「新しい画像をinsertする前に古い画像をdeleteする」順序になっている。`room_image_repository::insert` がここで失敗すると、`rooms.image_id` は削除済みの古い行を指したまま残り、画像が失われる。

この指示書のスコープは上記3件の修正のみ。他の機能追加・リファクタリングは行わないこと。

---

## 実装対象ファイル

- `src/handlers/rooms.rs` — `event_repository` への直接依存を除去し、`room_service` 経由に統一
- `src/services/room_service.rs` — ハンドラーがイベント情報を取得するための関数を追加し、`update` の画像張り替え順序を修正
- `src/services/room_service.rs`（`RoomError` 定義箇所） — 未使用の `Display`/`Error` 実装の扱いを決定

---

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、Red→Green→Refactorを個別コミットで回すこと。

### room_service（`sqlx::test`）

- [ ] ケースA: `current_event(pool)` を呼ぶと、シングルトンの `Event` が返る（既存の `event_repository::find_singleton` の薄いラッパーであることをテストで確認する）
- [ ] ケースB: 画像を持つ部屋を新しい画像で更新するとき、`room_image_repository::insert` が失敗するケースをシミュレートできない場合は、少なくとも「新しい画像がinsertされた後でなければ古い画像がdeleteされない」という順序をコードレビューで確認可能な形にする。加えて、既存のケース13（画像張り替えで新旧行数が0/1になる）のテストが、修正後の順序でも引き続きパスすることを確認する
- [ ] ケースC（任意）: `update` の途中で新規画像insertが失敗した場合に、既存の `image_id` が変更されず古い画像がそのまま残ることを検証するテスト（`room_image_repository::insert` を差し替え可能にするのが大掛かりになる場合は、実装のコードレビューでの確認に留めてよい。無理に複雑なモックを導入しないこと）

### ハンドラー（既存の結合テスト）

- [ ] ケースD: 既存のケース15〜22（`GET /admin/rooms` 系の全ハンドラーテスト）が、`room_service` 経由に変更した後も全てパスすることを確認する（新規テスト追加は不要、既存テストの回帰確認）

### RoomError

- [ ] ケースE: `Display`/`Error` 実装を削除する場合は、削除後も既存のテストが全てパスすることを確認する。実装を残す場合は、実際に呼び出す箇所（例: ログ出力）を追加した上で、その呼び出しを検証する失敗するテストを先に書いてから実装すること

---

## 実装仕様

### src/services/room_service.rs

- `current_event(pool: &MySqlPool) -> Result<crate::repository::event_repository::Event, RoomError>` を追加する。`event_repository::find_singleton` を呼び、`None` の場合は `RoomError::NotFound`（またはそれに準ずる適切なバリアント）を返す。シングルトン運用（1建物1イベント）なので `Option` をここで剥がしてハンドラー側の `let Some(event) = ... else { 500 }` という定型コードを解消する
- `list` の呼び出しに必要な `event_id` は、ハンドラーから渡すのではなく `current_event` を呼んだ結果から得るようにハンドラー側を書き換える（`list` 自体のシグネチャは変更しなくてよい）
- `update` 内の画像張り替え処理を次の順序に変更する:
  1. `image_service::process_upload` で新しい画像を検証・加工する
  2. **先に** `room_image_repository::insert` で新しい画像を保存し、新しい `image_id` を得る
  3. 挿入が成功した**後で**、既存の `image_id`（あれば）を `room_image_repository::delete` する
  4. `room_repository::update` には新しい `image_id` を渡す
  - これにより、新しい画像のinsertが失敗した場合は既存の画像がそのまま残り、`rooms.image_id` が無効な行を指すことがなくなる
- `RoomError` の `Display`/`Error` 実装（`fc9efcb` で追加）について: このプロジェクトで実際に文字列表現が必要な箇所（ログ出力等）が無いなら、**実装を削除する**こと（未使用コードを残さない）。もしCodexの側でログ出力等の実用途があると判断するなら、その呼び出しコードとテストをセットで追加し、単なる「refactor」ではなく機能追加として別コミットで記録すること

### src/handlers/rooms.rs

- `list`, `add_form`, `add`, `edit_form`, `update` 内の `crate::repository::event_repository::find_singleton(&pool).await` の直接呼び出しを、すべて `room_service::current_event(&pool).await` に置き換える
- エラーハンドリング（`None`/`Err` 時に500を返す挙動）は現状を維持する

---

## 制約・注意事項

- 既存のテストケース1〜22（`instructions/room-management.md` 記載）を壊さないこと
- スコープ外の機能追加・リファクタリングを行わないこと（例: `ImageError::DimensionsTooLarge` は前回実装済みで指示書外の追加だが、セキュリティ強化として妥当と判断済みのため今回は変更不要）
- `cargo clippy` が警告なく通ること
- コミットは「どのRedがどのGreenに対応するか」が git 履歴から明確に分かるように分割すること。今回のような「refactorを名乗りつつ機能を追加する」コミットは作らないこと

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る（既存のケース1〜22を含む）
- [ ] `cargo clippy` が警告なく通る
- [ ] `git log` 上で、Red→Green→Refactorのサイクルおよび各コミットのメッセージと実際の変更内容が一致していることを確認した
