# 実装指示書: Koyebデプロイ設定（本番用Dockerfile・PORT対応）

## 背景・目的

本番デプロイ先をRenderからKoyebに変更した。設計は [docs/architecture.md 18節](../docs/architecture.md#18-デプロイ構成koyeb) にまとめている。この設計を実際の変更に落とし込む。

- Koyebの無料枠Webサービスはリソース制約があるため、開発用の`.devcontainer/Dockerfile`とは別に、軽量な本番専用`Dockerfile`をマルチステージビルドで用意する
- Koyebなど多くのPaaSは、アプリがリッスンすべきポート番号を`PORT`環境変数で伝える方式を採るため、現状ポート8000固定になっている起動処理を`PORT`環境変数対応に変更する

## 実装対象ファイル

- `Dockerfile`（新規、リポジトリルート） — 本番用マルチステージビルド
- `src/main.rs` — リッスンポートを`PORT`環境変数から決定するよう変更（[main.rs:115](../src/main.rs#L115)付近）

## テストケース（TDDの起点）

### 1. `PORT`環境変数対応（振る舞いのある変更 — TDD対象）

- [ ] ケース1: `PORT`環境変数が数値文字列で設定されている場合、その値がリッスンポートとして使われる
- [ ] ケース2: `PORT`環境変数が未設定の場合、デフォルトの8000が使われる（現状の挙動を維持する回帰確認）
- [ ] ケース3: `PORT`環境変数が数値としてパースできない値（空文字列・非数値文字列等）の場合、デフォルトの8000にフォールバックする（起動時にpanicやプロセス終了をしない。他の必須環境変数（`DATABASE_URL`等）とは異なり、`PORT`は任意設定でありローカル開発（未設定）でも動く必要があるため）

ポート決定ロジックを`main`関数から切り出した小さな純粋関数（例: `fn resolve_port(value: Option<&str>) -> u16`）として実装し、上記3ケースを通常の`#[test]`（DB非依存）で検証すること。`main`関数内では`env::var("PORT").ok()`の結果をこの関数に渡すだけにする。

### 2. Dockerfile（振る舞いを持たない変更 — [AGENTS.md](../AGENTS.md)98節によりTDD対象外）

- [ ] ケースA: `docker build .` がリポジトリルートで成功する（Docker Engineが利用可能な環境で確認する。利用できない場合はケースBで代替する）
- [ ] ケースB（Dockerが使えない環境の代替確認）: `Dockerfile`のビルドステージと同一のベースイメージ・コマンド（`cargo build --release`）がローカルで成功することを確認し、実行ステージにコピーする成果物のパス（`target/release/stamp_rally`）が実際に存在することを確認する
- [ ] ケースC: ビルドしたイメージを起動し（`docker run`）、`docs/architecture.md`18節に列挙した環境変数一式をダミー値で与えた状態で`GET /health`が200を返すことを確認する（Dockerが使えない環境では、同等の確認を`cargo run --release`で行ったことを完了条件のコミットメッセージ等に明記する）

## 実装仕様

### src/main.rs

- `main`関数の冒頭付近（他の環境変数読み込みと合わせて）で、`PORT`環境変数を読む
- 新規関数（`main.rs`内のプライベート関数でよい）:
  ```rust
  fn resolve_port(value: Option<&str>) -> u16 {
      const DEFAULT_PORT: u16 = 8000;
      value.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_PORT)
  }
  ```
- 呼び出し側: `let port = resolve_port(env::var("PORT").ok().as_deref());`
- [main.rs:115](../src/main.rs#L115)の`SocketAddr::from(([0, 0, 0, 0], 8000))`を`SocketAddr::from(([0, 0, 0, 0], port))`に変更する

### Dockerfile（新規、リポジトリルート）

マルチステージビルドとする。イメージ名・タグの指定はKoyeb側の設定に依存するためこの指示書の対象外。

- ビルドステージ: `rust:1-bookworm`（`.devcontainer/Dockerfile`のベースと近いバージョン。`Cargo.toml`の`edition = "2024"`が要求する最低Rustバージョンを満たすタグを使うこと）を使い、`cargo build --release`でリリースビルドする
- 実行ステージ: `debian:bookworm-slim`をベースに、以下のみをコピーする
  - ビルドステージで生成された`target/release/stamp_rally`バイナリ
  - `ca-certificates`パッケージ（`reqwest`のTLS通信・LINE API/TiDB Serverlessへの接続に必要。`apt-get install -y ca-certificates && apt-get clean`で導入し、レイヤーを最小化する）
- `templates/`ディレクトリはAskamaがビルド時にコンパイル済みバイナリへ埋め込むため、実行ステージにコピーする必要はない（コピーしても害はないが不要。埋め込みであることは`grep -rn "askama" src/`等で実装済みの`#[derive(Template)]`の使い方から確認できる）
- `EXPOSE`するポート番号は固定値（8000）を書いてよいが、実際のリッスンポートは`PORT`環境変数で上書き可能である（Koyeb側の設定と一致させる）
- `ENTRYPOINT`または`CMD`で`./stamp_rally`（実行ステージ内の配置パスに合わせる）を起動する

## 制約・注意事項

- `.devcontainer/Dockerfile` や `.devcontainer/compose.yaml` など、ローカル開発用の構成は一切変更しないこと
- 新規`Dockerfile`にシークレットの実際の値を書かないこと（`ARG`/`ENV`でビルド時に埋め込まない。すべて実行時の環境変数として注入される前提）
- `PORT`以外の環境変数（`DATABASE_URL`等）の必須チェック挙動（未設定ならプロセス終了）は変更しないこと。`PORT`だけがローカル開発で未設定になり得る任意設定という扱い
- 本指示書のスコープは上記2ファイルの変更のみ。他のリファクタリングを行わないこと

## 完了条件

- [ ] `resolve_port`の3ケースがテストで検証され、実装前に失敗するテストを書いたことを確認した（Red→Green→Refactor）
- [ ] `main.rs`が`PORT`環境変数を読み、未設定・パース失敗時は8000にフォールバックする
- [ ] `Dockerfile`が新規追加され、マルチステージビルドで軽量な実行イメージになっている
- [ ] ビルド確認（ケースA/BいずれかとケースC）を実施し、その方法をコミットメッセージまたはPR説明に明記した
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
