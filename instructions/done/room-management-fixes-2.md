# 実装指示書: 部屋管理機能 差し戻し修正（再指示・第2版）

## 背景・目的

`instructions/done/room-management-fixes.md`（PR #4マージ後の最終レビュー指摘対応）は `instructions/done/` にアーカイブされ完了扱いになっていたが、Claudeによる再調査の結果、記載されていた3件の是正がいずれも `main` に反映されていないことが判明した。

- 指摘1（層構造違反）の対応コミットは作業者のローカル環境にのみ存在し、GitHub上のどのブランチ・PRにも push されていなかった
- 指摘2（画像張り替えのinsert/delete順序バグ）・指摘3（未使用の`Display`/`Error`実装）については、対応するコミット自体が存在しなかった

作業環境を途中で切り替えた際に、pushされていないローカルコミットが引き継がれなかったことが原因と推測される。本指示書は `instructions/done/room-management-fixes.md` の内容を全面的に再指示するものであり、`main` の現状（コミット `1420b76` 時点）を起点として、新しいブランチで対応すること。**ローカルに残っている可能性のある過去の `feature/room-management` ブランチの続きから作業しないこと。** 前回同様の見落としを防ぐため、必ず `main` の最新から新規ブランチを切ること。

## ブランチ

- `main` から `feature/room-management-fixes-2` を作成して作業すること（[AGENTS.md](../AGENTS.md) のブランチ運用に従う。ブランチ名は本指示書のファイル名 `room-management-fixes-2` と対応させる）
- 作業完了後は必ず `git push` してPull Requestを作成すること。ローカルにコミットを残したままにしない（前回の見落としの再発防止）

## 実装対象ファイル

- `src/handlers/rooms.rs` — `event_repository` への直接依存を除去し、`room_service` 経由に統一
- `src/services/room_service.rs` — `current_event` 関数の追加、`update` の画像張り替え順序の修正、`RoomError` の `Display`/`Error` 実装の扱い

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、Red→Green→Refactorを個別コミットで回すこと。

### room_service（`sqlx::test`）

- [ ] ケースA: `room_service::current_event(&pool)` を呼ぶと、シングルトンの `Event` が返る
- [ ] ケースB: 既存の `update_replaces_existing_image` テスト（`src/services/room_service.rs:342`）が、insert/delete順序変更後も引き続きパスすることを確認する（新旧の `image_id` が異なり、旧画像行数0・新画像行数1になる現在のアサーションのまま）
- [ ] ケースC（任意）: `update` の途中で新規画像insertが失敗した場合に、既存の `image_id` が変更されず古い画像がそのまま残ることを検証するテスト。モックが大掛かりになる場合は無理に導入せず、実装のコードレビューでの確認に留めてよい

### ハンドラー（既存の結合テスト）

- [ ] ケースD: `src/handlers/rooms.rs` の既存テスト（`GET/POST /admin/rooms` 系一式）が、`room_service` 経由に変更した後も全てパスすることを確認する（新規テスト追加は不要、既存テストの回帰確認）

### RoomError

- [ ] ケースE: `Display`/`Error` 実装（`src/services/room_service.rs:44-56`）を削除する場合、削除後も既存のテストが全てパスすることを確認する。実際に呼び出す用途がある場合のみ実装を残し、その場合は呼び出し箇所とそれを検証する失敗するテストを先に書いてから実装すること

## 実装仕様

### src/services/room_service.rs

- `current_event(pool: &MySqlPool) -> Result<event_repository::Event, RoomError>` を追加する。内部で `event_repository::find_singleton` を呼び、`None` の場合は `RoomError::NotFound` を返す
- `update`（現在 `src/services/room_service.rs:112-162`）内の画像張り替え処理（`132-148行目`）を次の順序に変更する:
  1. `image_service::process_upload` で新しい画像を検証・加工する
  2. **先に** `room_image_repository::insert` で新しい画像を保存し、新しい `image_id` を得る
  3. 挿入成功後に、既存の `image_id`（あれば）を `room_image_repository::delete` する
  4. `room_repository::update` には新しい `image_id` を渡す
  - 現状は `134-136行目` で先に `delete`、`137-145行目` で後から `insert` という逆順になっており、insertが失敗すると `rooms.image_id` が削除済みの行を指したまま残ってしまう
- `RoomError` の `Display`/`Error` 実装（`44-56行目`）: 実際に文字列表現を必要とする呼び出し箇所が現状ないため、**未使用コードとして削除する**。ログ出力等の実用途を追加するならコードとテストをセットにし、単なる整理ではなく機能追加として別コミットにすること

### src/handlers/rooms.rs

- 以下5箇所（`list`:27行目, `add_form`:58行目, `add`:140行目, `edit_form`:261行目, `update`:335行目）にある `crate::repository::event_repository::find_singleton(&pool).await` の直接呼び出しを、すべて `room_service::current_event(&pool).await` に置き換える
- エラーハンドリング（`None`/`Err` 時に500を返す挙動）は現状を維持する

## 制約・注意事項

- スコープ外の機能追加・リファクタリングを行わないこと
- `cargo clippy` が警告なく通ること
- コミットは「どのRedがどのGreenに対応するか」がgit履歴から明確に分かるように分割すること。「refactorを名乗りつつ機能を追加する」コミットは作らないこと
- 作業完了後は必ずリモートにpushし、PRを作成すること。ローカルにのみコミットが残る状態で完了報告をしないこと

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] `git log` 上で、Red→Green→Refactorのサイクルおよび各コミットのメッセージと実際の変更内容が一致していることを確認した
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
