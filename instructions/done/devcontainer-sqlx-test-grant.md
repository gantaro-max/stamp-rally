# 実装指示書: devcontainerでの `sqlx::test` 実行に必要なDB権限の付与

## 背景・目的

ポートフォリオとしてリポジトリを一般公開するにあたり、`cargo test` の初期状態での挙動を確認したところ、**212件中128件が失敗する**ことが判明した。

```
thread 'tests::room_qr_returns_png' panicked at sqlx-core-0.8.6/src/testing/mod.rs:226:14:
failed to connect to setup test database: Database(MySqlDatabaseError {
  code: Some("42000"), number: 1044,
  message: "Access denied for user 'gantaro'@'%' to database
            '_sqlx_test_pZBqqrY7MaV7epbcztdI8EHhXLqdzOjJgqN3L6pCHKuNSO5LVlLt'" })
```

### 原因（調査済み）

コードの不具合ではなく、**ローカル開発DBの権限設定の不足**である。

1. `#[sqlx::test]` は、テスト関数ごとに使い捨てのデータベースを新規作成し、そこにマイグレーションを適用してから実行する。データベース名は sqlx-core 0.8.6 の `TestSupport::db_name` の既定実装が生成する `_sqlx_test_<テストパスのSHA-512をbase64url化した値>`（`-` は `_` に置換、全長63文字固定）であり、`sqlx-mysql` 側はこれをオーバーライドしていない
2. 使い捨てDBの管理台帳テーブル `_sqlx_test_databases` は、`DATABASE_URL` が指すデータベース（＝ `stamprally`）内に作成される。これはアプリ用ユーザーの既存権限で足りている
3. 一方、`create database _sqlx_test_...` / `drop database if exists _sqlx_test_...` を実行する権限が無い。MySQL公式イメージは `MYSQL_USER` / `MYSQL_DATABASE` から `` GRANT ALL PRIVILEGES ON `stamprally`.* `` しか付与しないため、`stamprally` 以外のデータベースを作成できない

現状は `#[sqlx::test]` を含む128件が常に失敗する状態であり、[AGENTS.md](../AGENTS.md) の一次セルフチェック節でも「既知の環境制約でセルフチェックの対象外」として扱われている。しかしこれは、

- リポジトリを clone した第三者が `cargo test` を実行すると128件失敗するという、公開リポジトリとしての第一印象の問題
- Codex自身がDB結合テストの結果を確認できず、TDDのRed/Greenを128件分について実地で検証できないという、開発プロセス上の実害

の両方を生んでいる。本対応でこれを解消する。

### 対策（検証済み）

アプリ用ユーザーに対し、`_sqlx_test_` で始まる名前のデータベースに限定した権限を付与する。

```sql
GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* TO 'gantaro'@'%';
```

**この GRANT を適用したうえで、`.env` の `DATABASE_URL`（アプリ用一般ユーザー）のまま `cargo test` を実行し、212件すべての成功を確認済みである。**

```
test result: ok. 212 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

本指示書では、この GRANT を **devcontainer 作成時に自動適用される構成**として永続化する。

---

## ⚠️ 着手前に必ず読むこと（偽グリーンの罠）

**上記の調査の過程で、この GRANT は既に現在の開発コンテナのDBへ手動適用されている。**

そのため、**何も実装しない状態で `cargo test` を実行すると 212件すべて成功してしまう。** これは自分の実装が正しいことの証拠には一切ならない。この状態のまま着手すると、実装が間違っていても・そもそも実装しなくても「グリーン」に見える。

したがって、**着手の最初の手順として、手動適用された GRANT を明示的に取り消し、128件が失敗する状態を再現すること**（後述「検証手順」の手順2）。この取り消しを行わずに得られた成功結果は無効であり、完了条件を満たしたとみなしてはならない。

**本作業は `feature/devcontainer-sqlx-test-grant` ブランチで行うこと（[AGENTS.md](../AGENTS.md) のブランチ運用ルールに従う）。ただしPull Requestの作成は行わない — 後述「完了条件」を参照。**

---

## 実装対象ファイル

- `.devcontainer/Dockerfile` — MySQLクライアントを追加する
- `.devcontainer/grant-sqlx-test.sh` — **新規**。GRANTを冪等に適用するスクリプト
- `.devcontainer/devcontainer.json` — `postCreateCommand` から上記スクリプトを呼ぶ
- `README.md` — 「DB結合テストについて」の注記を、本対応後の実態に合わせて書き換える
- `AGENTS.md` — 「`sqlx::test` 系のDB接続エラーは既知の環境制約」とする記述を更新する

---

## テストケース（TDDの起点）について

本指示書はローカル開発環境の構成変更のみであり、アプリケーションの振る舞い（ハンドラー・サービス関数等）を追加・変更するものではない。[AGENTS.md](../AGENTS.md) の「実装対象に『テストケース』の記載がない場合（Cargo.tomlへの依存追加やマイグレーションDDLなど、振る舞いを持たない変更）はTDDサイクルの対象外でよい」という規定、および先行事例 [`instructions/done/devcontainer-hardening.md`](done/devcontainer-hardening.md) の扱いに従い、**新規テストコードの追加は不要**とする。

ただし本件は「既存の128件のテストが通るようになること」自体が検証対象であるため、下記「検証手順」に沿って **Red（128件失敗）→ Green（212件成功）を実地で確認すること**。

---

## 検証手順（この順序で行うこと）

### 手順1: MySQLクライアントを現セッションに導入する

`.devcontainer/Dockerfile` への追加はコンテナ再ビルド時にしか反映されないため、**いま検証するために**アドホックに導入する。

```bash
sudo apt-get update && sudo apt-get install -y --no-install-recommends default-mysql-client
```

（この導入自体は成果物ではない。永続化は手順4の Dockerfile 変更で行う）

### 手順2: 手動適用された GRANT を取り消し、Red を再現する

```bash
MYSQL_PWD="$MYSQL_ROOT_PASSWORD" mysql -h db -u root <<'EOSQL'
REVOKE ALL PRIVILEGES ON `\_sqlx\_test\_%`.* FROM 'gantaro'@'%';
FLUSH PRIVILEGES;
EOSQL
```

取り消し後、`SHOW GRANTS FOR 'gantaro'@'%';` の出力が次の2行だけになることを確認する。

```
GRANT USAGE ON *.* TO `gantaro`@`%`
GRANT ALL PRIVILEGES ON `stamprally`.* TO `gantaro`@`%`
```

続けて `cargo test` を実行し、**128件が失敗する**ことを確認する（全体で約220秒かかる。時間を惜しむ場合は `cargo test -- room_qr_returns_png` 等の単体指定で `Access denied ... to database '_sqlx_test_...'` が再現することの確認でもよいが、その場合は手順5で必ず全件実行すること）。

**ここで失敗が再現しない場合は、REVOKE が効いていない。先に進まず原因を調べること。**

### 手順3〜4: 実装（下記「実装仕様」参照）

### 手順5: Green を確認する

実装したスクリプトを、`postCreateCommand` から呼ばれるのと同じ形で手動実行する。

```bash
.devcontainer/grant-sqlx-test.sh
```

`SHOW GRANTS FOR 'gantaro'@'%';` に次の行が現れることを確認する。

```
GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* TO `gantaro`@`%`
```

そのうえで、`.env` の `DATABASE_URL`（アプリ用一般ユーザー）のまま `cargo test` を実行し、**212 passed / 0 failed** を確認する。

### 手順6: 冪等性を確認する

`.devcontainer/grant-sqlx-test.sh` を連続2回実行し、2回目もエラーにならず、`SHOW GRANTS` の結果が変わらないことを確認する。

---

## 実装仕様

### `.devcontainer/Dockerfile`

MySQLクライアントを追加する。ベースイメージは Debian bookworm のため、パッケージ名は `default-mysql-client`（MariaDBクライアント。`compose.yaml` が `--default-authentication-plugin=mysql_native_password` を指定しているためMySQL 8.0に接続できる）。

既存の `cargo install sqlx-cli ...` の行は変更しない。apt のキャッシュはイメージに残さないこと。

```dockerfile
RUN apt-get update \
    && apt-get install -y --no-install-recommends default-mysql-client \
    && rm -rf /var/lib/apt/lists/*
```

### `.devcontainer/grant-sqlx-test.sh`（新規）

`MYSQL_USER` / `MYSQL_ROOT_PASSWORD` は、`compose.yaml` の `env_file: ../.env` により `rust_app` コンテナの環境変数として既に参照可能である（確認済み）。スクリプト内に値を直書きしないこと。

```sh
#!/usr/bin/env bash
# #[sqlx::test] はテスト関数ごとに使い捨てDB `_sqlx_test_<hash>` を作成・削除する。
# MySQL公式イメージは MYSQL_USER に MYSQL_DATABASE 分の権限しか付与しないため、
# 名前パターンを `_sqlx_test_` 始まりに限定して作成・削除権限を追加で付与する。
# 識別子内の `\_` は「リテラルのアンダースコア」の意味（エスケープしないと `_` は1文字ワイルドカード）。
set -euo pipefail

: "${MYSQL_USER:?compose.yaml の env_file 経由で .env が読み込まれているか確認してください}"
: "${MYSQL_ROOT_PASSWORD:?compose.yaml の env_file 経由で .env が読み込まれているか確認してください}"

DB_HOST="${DB_HOST:-db}"

for i in $(seq 1 30); do
  if MYSQL_PWD="$MYSQL_ROOT_PASSWORD" mysql -h "$DB_HOST" -u root -e 'SELECT 1' >/dev/null 2>&1; then
    break
  fi
  if [ "$i" -eq 30 ]; then
    echo "grant-sqlx-test: DB($DB_HOST) に接続できませんでした" >&2
    exit 1
  fi
  sleep 2
done

MYSQL_PWD="$MYSQL_ROOT_PASSWORD" mysql -h "$DB_HOST" -u root <<EOSQL
GRANT ALL PRIVILEGES ON \`\_sqlx\_test\_%\`.* TO '${MYSQL_USER}'@'%';
FLUSH PRIVILEGES;
EOSQL

echo "grant-sqlx-test: '${MYSQL_USER}'@'%' に _sqlx_test_* データベースの権限を付与しました"
```

実装上の注意:

- **エスケープを必ず実地で確認すること。** サーバに届く最終的なSQLが `` GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* TO 'gantaro'@'%'; `` と文字単位で一致している必要がある。クォートしないヒアドキュメントでは、`` \` `` はリテラルのバックティック、`\_` はリテラルの `\_`、`${MYSQL_USER}` は展開、という扱いになる。手順5の `SHOW GRANTS` 出力で確認する
- パスワードはコマンドライン引数（`-p...`）ではなく `MYSQL_PWD` 環境変数で渡すこと（`ps` への露出と、クライアントが出す警告を避けるため）
- ファイルに実行ビットを立てること（`git update-index --chmod=+x` 等でモードもコミットに含める）。`postCreateCommand` から直接実行するため
- DBの起動待ちループを入れること。`depends_on: service_healthy` により通常は起動済みだが、`postCreateCommand` の実行タイミングに依存しない作りにする

### `.devcontainer/devcontainer.json`

既存の `postCreateCommand` の末尾にスクリプト実行を追加する。他のキーは変更しない。

```json
"postCreateCommand": "sudo chown -R vscode /usr/local/cargo/registry /app/target && (test -f .env || cp .env.example .env) && .devcontainer/grant-sqlx-test.sh",
```

### `README.md`

「ローカルでの起動」節にある `> **DB 結合テストについて**` の引用ブロックを差し替える。本対応後は devcontainer を使う限り追加操作が不要になるため、現在の「rootを指定して実行してください」という案内は不正確になる。

差し替え後は以下の趣旨とすること（文面は調整してよい）。

- 212件のうち128件は `#[sqlx::test]` によるDB結合テストで、テストごとに使い捨てDB（`_sqlx_test_*`）を作成する
- devcontainer を使う場合、`postCreateCommand` が必要な権限を自動で付与するため `cargo test` をそのまま実行できる
- devcontainer を使わず自前でDBを用意する場合は、`DATABASE_URL` のユーザーに `_sqlx_test_*` という名前のデータベースを作成・削除できる権限が必要である

**既存のdevcontainerを使い続けている場合にコンテナのリビルドが必要になる旨も1行添えること**（MySQLクライアントの導入と `postCreateCommand` の変更が反映されるのはリビルド後のため）。

### `AGENTS.md`

一次セルフチェック節の以下の記述を更新する（現在の80行目付近）。

> - `sqlx::test` 系のDB接続エラー（`docker`/`gh`/DBホスト名解決がこの開発環境に無いことに起因するもの）は既知の環境制約であり、セルフチェックの対象外である旨を報告に明記する

本対応により **devcontainer内では `sqlx::test` が正常に実行できるようになる**ため、DB結合テストの結果はセルフチェックの対象に含めること。ただしDBに到達できない環境で実行した場合は依然として失敗しうるため、「DBに到達できない環境で実行した場合はその旨を報告に明記する」という趣旨に書き換える。`docker`/`gh` が無いことに起因する他の制約についての記述は残すこと。

---

## 制約・注意事項

- **本変更はローカル開発環境（devcontainer）専用である。** 本番DB（TiDB Serverless）の接続ユーザー・権限には一切影響しない。[SECURITY.md](../SECURITY.md)「本番DB（TiDB Serverless）の接続方針」は変更しないこと
- 付与する権限は `_sqlx_test_` で始まる名前のデータベースに限定すること。`GRANT ALL PRIVILEGES ON *.*` や、グローバルな `CREATE` / `DROP` を与えてはならない。アプリ用ユーザーが本番同様「自分のデータベースしか触れない」状態を保つことが、この方式を選んだ理由である
- 識別子パターンのアンダースコアは必ずエスケープすること。`_sqlx_test_%` と書くと `_` が1文字ワイルドカードとして解釈され、意図より広い範囲のデータベース名に一致してしまう
- `.env` / `.env.example` に新しい環境変数を追加する必要はない。既存の `MYSQL_USER` / `MYSQL_ROOT_PASSWORD` で足りる
- `.devcontainer/` 配下にシークレットを一切書かないこと（このディレクトリはコミット対象）
- `.devcontainer/Dockerfile` の変更はコンテナ再ビルド後にしか反映されない。**Codexはコンテナを再ビルドできない（devcontainer内に `docker` コマンドが無い）ため、Dockerfile変更が実際に効くことの確認は本作業の範囲外**とし、報告にその旨を明記すること。現セッションでの検証は手順1のアドホック導入で代替する
- `cargo test` の完走には数分かかる（実測 約220秒）。タイムアウトに注意すること

---

## 完了条件

- [ ] 手順1〜2を実施し、**GRANTを取り消した状態で128件が失敗すること**を実際に確認した（偽グリーンの罠を回避したことの証明。この確認をしていない場合、以降の成功結果は無効）
- [ ] `.devcontainer/Dockerfile` に `default-mysql-client` の導入を追加した（aptキャッシュを残していない）
- [ ] `.devcontainer/grant-sqlx-test.sh` を追加し、実行ビットが立った状態でコミットした
- [ ] `.devcontainer/devcontainer.json` の `postCreateCommand` からスクリプトが呼ばれるようにした
- [ ] `.devcontainer/grant-sqlx-test.sh` を手動実行し、`SHOW GRANTS FOR '<MYSQL_USER>'@'%';` の出力に `` GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* `` が含まれることを確認した
- [ ] 付与された権限が `_sqlx_test_` で始まるデータベースに限定されており、グローバル権限（`*.*`）が `USAGE` のままであることを `SHOW GRANTS` で確認した
- [ ] `.env` の `DATABASE_URL`（アプリ用一般ユーザー）のまま `cargo test` を実行し、**212 passed / 0 failed** になることを確認した
- [ ] スクリプトを連続2回実行しても失敗せず、結果が変わらないこと（冪等性）を確認した
- [ ] `cargo clippy` が警告なく通る
- [ ] `README.md` の「DB結合テストについて」の注記を、本対応後の実態に合わせて書き換えた（既存devcontainerのリビルドが必要な旨を含む）
- [ ] `AGENTS.md` の「`sqlx::test` 系のDB接続エラーは既知の環境制約」とする記述を更新した
- [ ] `feature/devcontainer-sqlx-test-grant` ブランチで作業し、**リモートへpushした**
- [ ] **Pull Requestは作成しない。** この開発環境には `gh` コマンドが無く、[AGENTS.md](../AGENTS.md) ワークフロー手順7の `gh pr create` は実行できない。pushまでを行い、ブランチ名・変更概要・検証結果（手順2のRedと手順5のGreenの実際の出力）を報告すること。PR作成はユーザーがGitHubのWeb UIで行う
