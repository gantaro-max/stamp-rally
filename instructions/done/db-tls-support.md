# 実装指示書: sqlxのTLSバックエンド有効化（本番DB接続の疎通確保）

## 背景・目的

Koyebへの本番デプロイ作業の一環で、TiDB Cloud（TLS必須接続）に対して`sqlx-cli`で`sqlx migrate run`を実行したところ、以下のエラーで失敗した。

```
error occurred while attempting to establish a TLS connection: TLS upgrade required by connect options
but SQLx was built without TLS support enabled
```

原因は`sqlx-cli`が`--no-default-features --features mysql`（TLSバックエンド未指定）でインストールされていたためで、`--features mysql,rustls`を付けて再インストールすることでCLI側は解消した。

しかし調査の結果、**アプリ本体の`Cargo.toml`の`sqlx`依存関係にも同様にTLSバックエンド機能が一切指定されておらず、`Cargo.lock`に`rustls`・`native-tls`系のクレートが1つも含まれていない**ことを確認した。つまりKoyebにデプロイした現状のバイナリでも、本番の`DATABASE_URL`（TLS必須のTiDB Serverless接続文字列）に接続しようとした瞬間、起動時に同じ`"SQLx was built without TLS support enabled"`エラーで異常終了する状態にある。

[SECURITY.md](../SECURITY.md)「本番DB（TiDB Serverless）の接続方針」および[docs/architecture.md](../docs/architecture.md)18節では「TLS必須接続」を既に設計方針として明記済みであり、これは新規の設計判断ではなく、その方針を実装が満たしていなかった不備の修正である。

## 実装対象ファイル

- `Cargo.toml` — `sqlx`依存の`features`にTLSバックエンドを追加
- `src/main.rs`（`mod tests`内） — 回帰テストを追加

## テストケース（TDDの起点）

- [ ] ケース1（回帰確認）: 到達不能なアドレス（例: `127.0.0.1:1`。接続は即座に拒否される）かつ`ssl-mode=REQUIRED`を指定した接続文字列（例: `mysql://user:pass@127.0.0.1:1/testdb?ssl-mode=REQUIRED`)に対して`sqlx::mysql::MySqlPoolOptions::connect`を試みたとき、返るエラーメッセージの文字列表現に`"without TLS support enabled"`が含まれないこと。
  - 接続自体はポート到達不能で失敗するのが正しい挙動（アサーションは「TLS未対応エラーではない、別の理由で失敗している」ことだけを確認する。実際のネットワーク到達性やTiDBへの疎通はCIでは検証できないため対象外とする）
  - 現状（Red）ではこのテストは失敗する（エラーメッセージに`"without TLS support enabled"`が含まれてしまうため）

## 実装仕様

### Cargo.toml

`sqlx`の`features`配列に`"tls-native-tls"`を追加する。

```toml
sqlx = { version = "0.8.6", features = ["mysql", "runtime-tokio", "macros", "chrono", "uuid", "tls-native-tls"] }
```

`tls-rustls`系ではなく`tls-native-tls`を選ぶ理由:

- 本アプリの`reqwest`（LINE Messaging API通信用）も明示的なTLSバックエンド指定をしておらず、デフォルトの`native-tls`（システムのOpenSSL）に依存している
- 本番用`Dockerfile`（[docs/architecture.md](../docs/architecture.md)18節）は実行イメージに`ca-certificates`を含めることを既に明記しており、これは`native-tls`がシステムの信頼ルート証明書ストアを参照するために必要なもの（コメントで「reqwestのTLS通信に必要」と記載済み）
- `sqlx`側も`tls-native-tls`を選べば、この既存のOpenSSL/`ca-certificates`の仕組みをそのまま再利用でき、`rustls`系の別のTLSスタックを新たに追加する必要がない（依存関係・バイナリサイズの増加を避ける）

### src/main.rs（テスト追加）

`mod tests`内に、上記テストケースに対応するテストを追加する。DBへの実接続を必要としないテストなので、既存の`#[sqlx::test]`パターンではなく`#[tokio::test]`を使う。

```rust
#[tokio::test]
async fn mysql_pool_supports_tls_connect_options() {
    let result = sqlx::mysql::MySqlPoolOptions::new()
        .connect("mysql://user:pass@127.0.0.1:1/testdb?ssl-mode=REQUIRED")
        .await;
    let err = result.expect_err("unreachable port should fail to connect");
    assert!(!err.to_string().contains("without TLS support enabled"));
}
```

（エラー型・APIの細部は実際の`sqlx` 0.8.6の型に合わせて調整してよい。重要なのはアサーションの意図。）

## 制約・注意事項

- 既存のローカル開発環境の動作（`.env`の`DATABASE_URL=mysql://gantaro:...@db:3306/stamprally`、TLS指定なしの非TLS接続）を壊さないこと。`tls-native-tls`を追加しても、接続文字列に`ssl-mode`等の指定がなければ従来通り動作するはずだが、`docker-compose up -d`でローカルDBを起動した状態で`cargo test`が通ることを確認すること
- 本番の実際のTiDB Serverlessへの疎通確認はCI/自動テストでは行えない。この指示書のテストケースは「TLSバックエンドがバイナリにコンパイルされているかどうか」の確認にとどめる。実際の本番接続確認はデプロイ後に手動で行う（このリポジトリの運用者側のタスク）
- `cargo build --release --locked`が通ることを確認する（本番用`Dockerfile`のビルドコマンドと同一）。`Cargo.lock`が更新されるので、変更をコミットに含めること

## 完了条件

- [ ] 上記テストケースについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] `Cargo.toml`へのfeature追加という最小限の実装でテストを通した（Green）
- [ ] `cargo test`が全体で通る（ローカルdocker-compose DBを起動した状態で）
- [ ] `cargo clippy -- -D warnings`が警告なく通る
- [ ] `cargo build --release --locked`が通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
