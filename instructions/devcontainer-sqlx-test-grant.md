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

1. `#[sqlx::test]` は、テスト関数ごとに使い捨てのデータベースを新規作成して、そこにマイグレーションを適用してから実行する。データベース名は sqlx-core 0.8.6 の `TestSupport::db_name` の既定実装が生成する `_sqlx_test_<テストパスのSHA-512をbase64url化した値>`（`-` は `_` に置換、全長63文字固定）であり、`sqlx-mysql` 側はこれをオーバーライドしていない
2. 使い捨てDBの管理台帳テーブル `_sqlx_test_databases` は、`DATABASE_URL` が指すデータベース（＝ `stamprally`）内に作成される。これはアプリ用ユーザーの既存権限で足りている
3. 一方、`create database _sqlx_test_...` / `drop database if exists _sqlx_test_...` を実行する権限が無い。MySQL公式イメージは `MYSQL_USER` / `MYSQL_DATABASE` から `GRANT ALL PRIVILEGES ON \`stamprally\`.*` しか付与しないため、`stamprally` 以外のデータベースを作成できない

現状は `#[sqlx::test]` を含む128件が常に失敗する状態であり、[AGENTS.md](../AGENTS.md) の一次セルフチェック節でも「既知の環境制約でセルフチェックの対象外」として扱われている。しかしこれは、

- リポジトリを clone した第三者が `cargo test` を実行すると 128件失敗するという、公開リポジトリとしての第一印象の問題
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

なお、この GRANT は既に**開発中の当該コンテナのDBには手動で適用済み**である。ただし手動適用はコンテナを作り直すと失われるため、本指示書で構成として永続化する。

**本作業は `feature/devcontainer-sqlx-test-grant` ブランチで行い、完了後にPull Requestを作成すること（[AGENTS.md](../AGENTS.md) のブランチ運用ルールに従う）。**

---

## 実装対象ファイル

- `.devcontainer/initdb/01-grant-sqlx-test.sh` — **新規**。DB初期化時に上記GRANTを適用するスクリプト
- `.devcontainer/compose.yaml` — `db` サービスに `initdb` ディレクトリのマウントを追加
- `README.md` — 「DB結合テストについて」の注記を、本対応後の実態に合わせて書き換える
- `AGENTS.md` — 「`sqlx::test` 系のDB接続エラーは既知の環境制約」とする記述を更新する

---

## テストケース（TDDの起点）について

本指示書はローカル開発環境の構成変更のみであり、アプリケーションの振る舞い（ハンドラー・サービス関数等）を追加・変更するものではない。[AGENTS.md](../AGENTS.md) の「実装対象に『テストケース』の記載がない場合（Cargo.tomlへの依存追加やマイグレーションDDLなど、振る舞いを持たない変更）はTDDサイクルの対象外でよい」という規定、および先行事例 [`instructions/done/devcontainer-hardening.md`](done/devcontainer-hardening.md) の扱いに従い、**Red-Green-Refactorサイクルの対象外**とする。

検証は下記「完了条件」の動作確認をもって行う。ただし本件は「既存の128件のテストが通るようになること」自体が検証であるため、**作業前に128件が失敗している状態を実際に確認してから着手すること**（Redに相当する状態の確認）。

---

## 実装仕様

### `.devcontainer/initdb/01-grant-sqlx-test.sh`（新規）

MySQL公式イメージのエントリポイントは、**データディレクトリが空の初回起動時に限り** `/docker-entrypoint-initdb.d/` 配下のファイルを名前順に実行する。`.sql` ファイルは環境変数展開が行われないため、`MYSQL_USER` の値を埋め込む必要がある本件では **`.sh` 形式にすること**。

エントリポイントは `docker_setup_db`（データベースと `MYSQL_USER` の作成）を済ませてから初期化スクリプトを実行するため、この時点で対象ユーザーは既に存在する。

```sh
#!/bin/sh
set -e

# #[sqlx::test] はテストごとに使い捨てDB `_sqlx_test_<hash>` を作成する。
# MySQL公式イメージは MYSQL_USER に MYSQL_DATABASE の権限しか与えないため、
# アプリ用ユーザーが使い捨てDBを作成・削除できるよう、名前パターンを限定して権限を付与する。
# 識別子内の `\_` は「リテラルのアンダースコア」の意味（エスケープしないと `_` は1文字ワイルドカード）。
mysql --protocol=socket -uroot -p"$MYSQL_ROOT_PASSWORD" <<-EOSQL
	GRANT ALL PRIVILEGES ON \`\_sqlx\_test\_%\`.* TO '${MYSQL_USER}'@'%';
	FLUSH PRIVILEGES;
EOSQL
```

実装上の注意:

- **エスケープを必ず実地で確認すること。** サーバに届く最終的なSQLが、上記「対策」節に示した `` GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* TO 'gantaro'@'%'; `` と文字単位で一致している必要がある。ヒアドキュメント内ではバックスラッシュが `` ` `` `$` `\` に対して特殊なため、`` \` `` はリテラルのバックティック、`\_` はリテラルの `\_` になる。適用後に `SHOW GRANTS FOR 'gantaro'@'%';` を実行し、次の3行が出ることで確認する
  ```
  GRANT USAGE ON *.* TO `gantaro`@`%`
  GRANT ALL PRIVILEGES ON `stamprally`.* TO `gantaro`@`%`
  GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* TO `gantaro`@`%`
  ```
- 実行ビットの有無に依存しないこと。MySQLのエントリポイントは `.sh` を、実行可能なら実行し、そうでなければ `.` でsourceする。どちらでも動くよう、`exit` や `return` に依存する書き方をしない
- ユーザー名・パスワードをスクリプト内に直書きしないこと（`$MYSQL_USER` / `$MYSQL_ROOT_PASSWORD` を参照する）。`env_file: ../.env` により両方ともコンテナ内で参照できる

### `.devcontainer/compose.yaml`

`db` サービスに読み取り専用マウントを1行追加する。他のサービス定義・`healthcheck`・ポート設定は変更しない。

```yaml
  db:
    image: mysql:8.0
    env_file:
      - ../.env
    ports:
      - "127.0.0.1:${DB_HOST_PORT:-3307}:3306"
    command: --default-authentication-plugin=mysql_native_password
    healthcheck:
      ...
    volumes:
      - db-data:/var/lib/mysql
      - ./initdb:/docker-entrypoint-initdb.d:ro   # ← 追加
```

### `README.md`

「ローカルでの起動」節にある `> **DB 結合テストについて**` の引用ブロックを差し替える。本対応後は、devcontainerを新規に作成すれば追加操作なしで `cargo test` が通るため、現在の「rootを指定して実行してください」という案内は不正確になる。

差し替え後の内容は以下の趣旨とすること（文面は調整してよい）。

- 212件のうち128件は `#[sqlx::test]` によるDB結合テストで、テストごとに使い捨てDB（`_sqlx_test_*`）を作成する
- devcontainerを新規作成した場合、`.devcontainer/initdb/` の初期化スクリプトが必要な権限を自動で付与するため、`cargo test` をそのまま実行できる
- **本対応より前に作成した既存のdevcontainerでは、DBのデータボリュームが既に初期化済みのため初期化スクリプトが実行されない。** その場合は下記「既存環境での適用」を一度だけ実施する

あわせて「既存環境での適用」として、ボリュームを作り直す方法（`docker compose down -v` 相当。**ローカルDBのデータは失われる**旨を明記する）と、それを避けたい場合に手動でGRANTを1回だけ流す方法の両方を記載すること。

### `AGENTS.md`

一次セルフチェック節の以下の記述を更新する（現在の80行目付近）。

> - `sqlx::test` 系のDB接続エラー（`docker`/`gh`/DBホスト名解決がこの開発環境に無いことに起因するもの）は既知の環境制約であり、セルフチェックの対象外である旨を報告に明記する

本対応により、**devcontainer内では `sqlx::test` が正常に実行できるようになる**ため、DB結合テストの結果はセルフチェックの対象に含めること。ただし、DBが起動していない環境（devcontainer外での実行等）で接続エラーになる場合が依然ありうるため、「DBに到達できない環境で実行した場合はその旨を報告に明記する」という趣旨に書き換える。`docker`/`gh` が無いことに起因する他の制約についての記述は残すこと。

---

## 制約・注意事項

- **本変更はローカル開発環境（devcontainer）専用である。** 本番DB（TiDB Serverless）の接続ユーザー・権限には一切影響しない。[SECURITY.md](../SECURITY.md)「本番DB（TiDB Serverless）の接続方針」は変更しないこと
- 付与する権限は `_sqlx_test_` で始まる名前のデータベースに限定すること。`GRANT ALL PRIVILEGES ON *.*` や、グローバルな `CREATE` / `DROP` を与えてはならない。アプリ用ユーザーが本番同様「自分のデータベースしか触れない」状態を保つことが、この方式を選んだ理由である
- 識別子パターンのアンダースコアは必ずエスケープすること。`_sqlx_test_%` と書くと `_` が1文字ワイルドカードとして解釈され、意図より広い範囲のデータベース名に一致してしまう
- `.env` / `.env.example` に新しい環境変数を追加する必要はない。既存の `MYSQL_USER` / `MYSQL_ROOT_PASSWORD` で足りる
- `.devcontainer/initdb/` 配下にはシークレットを一切置かないこと（このディレクトリはコミット対象になる）
- 初期化スクリプトは初回起動時にしか実行されない。この制約はスクリプト側では解決できないため、README に既存環境向けの手順を必ず記載すること
- `cargo test` の完走には数分かかる（実測 約220秒）。タイムアウトに注意すること

---

## 完了条件

- [ ] 着手前に、現状の `cargo test` で128件が失敗することを確認した
- [ ] `.devcontainer/initdb/01-grant-sqlx-test.sh` を追加し、`.devcontainer/compose.yaml` の `db` サービスに `./initdb:/docker-entrypoint-initdb.d:ro` のマウントを追加した
- [ ] データボリュームを削除してdevcontainerを作り直し、**初期化スクリプトが実際に実行される経路で** GRANT が適用されることを確認した
- [ ] `SHOW GRANTS FOR '<MYSQL_USER>'@'%';` の出力に `` GRANT ALL PRIVILEGES ON `\_sqlx\_test\_%`.* `` が含まれることを確認した
- [ ] `.env` の `DATABASE_URL`（アプリ用一般ユーザー）のまま `cargo test` を実行し、**212 passed / 0 failed** になることを確認した
- [ ] `cargo clippy` が警告なく通る
- [ ] `README.md` の「DB結合テストについて」の注記を、本対応後の実態（新規作成時は追加操作不要／既存環境では一度だけ適用が必要）に合わせて書き換えた
- [ ] `AGENTS.md` の「`sqlx::test` 系のDB接続エラーは既知の環境制約」とする記述を更新した
- [ ] 付与された権限が `_sqlx_test_` で始まるデータベースに限定されており、グローバル権限が増えていないことを `SHOW GRANTS` で確認した
- [ ] 本作業を `feature/devcontainer-sqlx-test-grant` ブランチで行い、Pull Requestを作成した
