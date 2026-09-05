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
