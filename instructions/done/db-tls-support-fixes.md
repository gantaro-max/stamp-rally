# 実装指示書: sqlx TLS対応 差し戻し修正（最終レビュー指摘対応）

## 背景・目的

`feature/db-tls-support`（PR #14）はCodexの一次実装を経てpush済みだが、Claudeによる最終レビュー（設計整合性・セキュリティ・要件充足・実装指示書・TDD遵守の5観点）のうち**TDD遵守**の観点で1件の問題が見つかった。マージ前にこのブランチ上で修正すること。

- **無検証テスト（常にパスするテスト）**: `src/main.rs`の`mysql_pool_supports_tls_connect_options`テスト（指示書 [instructions/db-tls-support.md](db-tls-support.md) のテストケース1に対応）は、`tls-native-tls`機能の有無に関わらず常にパスすることが、レビューでの実機検証（`Cargo.toml`からfeatureを一時的に外して実行）により確認された。原因は、接続先`127.0.0.1:1`への接続がTCP接続確立の段階で失敗し、sqlxがTLSネゴシエーションを試みる前にエラーを返してしまうため（`sqlx-core-0.8.6/src/net/tls/mod.rs`の"without TLS support enabled"チェックに到達しない）。これは指示書側のテスト設計の誤りであり、Codexの実装ミスではない。

  Codexはこの問題に実装時点で気づき、代替として`sqlx_dependency_explicitly_enables_native_tls`（`Cargo.toml`の文字列を直接assertするテスト）を追加していた。このテストは実機検証でRed/Green双方が正しく機能することを確認済みで、実質的な回帰保護にはなっている。ただし、機能しない`mysql_pool_supports_tls_connect_options`がテストスイートに残ったままなのは、「常にパスする＝何も検証していないテスト」であり、TDD運用（[AGENTS.md](../AGENTS.md)）の趣旨に反する。

この指示書のスコープは、この1件の是正のみ。他の変更（`Cargo.toml`のfeature選定・`sqlx_dependency_explicitly_enables_native_tls`の実装等）はそのままでよく、変更しないこと。

## 実装対象ファイル

- `src/main.rs` — 無検証な`mysql_pool_supports_tls_connect_options`テストを削除する

## テストケース（TDDの起点）

新たな失敗するテストを書く類の修正ではない（既存の壊れた・無意味なテストを取り除くだけの変更）。ただし取り除いた後も他のテストに影響がないことを確認すること。

- [ ] `mysql_pool_supports_tls_connect_options`を削除した後、`sqlx_dependency_explicitly_enables_native_tls`テストが引き続き存在し、`cargo test sqlx_dependency_explicitly_enables_native_tls -- --nocapture`が通ること
- [ ] `cargo test`（DB非依存のテストすべて）が引き続き通ること（回帰確認）

## 実装仕様

### src/main.rs

- `mod tests`内の`mysql_pool_supports_tls_connect_options`関数（`#[tokio::test]`、`MySqlPoolOptions::new().connect("mysql://user:pass@127.0.0.1:1/testdb?ssl-mode=REQUIRED")`を使うテスト）を丸ごと削除する
- このテストが使っていた`MySqlPoolOptions`のインポートが、削除後に他で使われなくなる場合は、`use`文も併せて削除し、未使用importの警告（`cargo clippy -- -D warnings`）が出ないようにする
- `sqlx_dependency_explicitly_enables_native_tls`テストはそのまま残す（変更不要）

## 制約・注意事項

- スコープは上記の1点のみ。`Cargo.toml`・`Cargo.lock`・`sqlx_dependency_explicitly_enables_native_tls`テストの実装には手を加えないこと
- より「本物らしい」TLSハンドシェイクを検証するテスト（例: ローカル開発用DBコンテナへ`ssl-mode=REQUIRED`で接続する等）への差し替えは、今回は行わない。ローカル開発DBの構成やCI環境の有無に依存する複雑なテストを新たに持ち込むより、機能しないテストを削除して`sqlx_dependency_explicitly_enables_native_tls`のみに絞る方針とする（過剰実装を避ける）
- 削除によって空になった`#[tokio::test]`関連のuse文やヘルパーが残らないよう整理すること

## 完了条件

- [ ] `mysql_pool_supports_tls_connect_options`を削除した（Refactor相当のコミット。新しい振る舞いを追加するものではないため、Red-Greenのサイクルは不要）
- [ ] `cargo test`が全体で通る（DB依存テストはこの環境の既知の制約により従来通り除外可）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] `cargo build --release --locked`が通る
- [ ] 同じブランチ（`feature/db-tls-support`）に追加コミットをpushし、既存PR #14に反映した
