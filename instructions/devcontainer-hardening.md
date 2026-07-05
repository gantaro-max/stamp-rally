# 実装指示書: devcontainer設定の堅牢化（シークレット管理・ヘルスチェック・ポート運用）

## 背景・目的

直近のコンテナ設定修正（コミット `6ae7db0`「コンテナ設定修正」）について、Claudeが複数エージェントでレビューした結果、以下の問題が見つかった。

1. `.devcontainer/compose.yaml` にDB接続情報（パスワード含む）が平文でハードコードされており、[CLAUDE.md](../CLAUDE.md)の「シークレットは環境変数で注入する」方針の抜け穴になっている
2. `DATABASE_URL` が `compose.yaml` と `.env` / `.env.example` の二重管理になっており、`.env` を変更してもコンテナには反映されない（`compose.yaml` 側の値が優先される）
3. `healthcheck` の `mysqladmin ping -u root -prootroot` が `MYSQL_ROOT_PASSWORD` の値をハードコードしており、どちらか一方だけ変更すると同期が崩れて壊れる
4. ホストポート（`127.0.0.1:3307`, `127.0.0.1:8099`）が固定されており、`container_name` を削除した意図（複数ワークツリーでの並行起動）と矛盾し、2つ目のワークツリーを起動するとポート競合で失敗する

これらを解消し、シークレットの一元管理（`.env` を唯一の情報源にする）と設定の堅牢性を確保する。

**本作業は `feature/devcontainer-hardening` ブランチで行い、完了後にPull Requestを作成すること（[AGENTS.md](../AGENTS.md)のブランチ運用ルールに従う。今回のレビュー対象となった前回のコミットが`main`に直接入ってしまった反省を踏まえ、必ずルール通りに進めること）。**

---

## 実装対象ファイル

- `.env.example` — MySQLブートストラップ用変数・ホストポート変数を追加
- `.devcontainer/compose.yaml` — シークレット参照方法・ヘルスチェック・ポート設定を修正
- `.devcontainer/Dockerfile` — ベースイメージタグの実在確認（必要であれば修正）

---

## テストケース（TDDの起点）について

本指示書はインフラ設定変更のみで、アプリケーションの振る舞い（ロジック）を追加・変更するものではない。[AGENTS.md](../AGENTS.md)の「実装対象に『テストケース』の記載がない場合（振る舞いを持たない変更）はTDDサイクルの対象外でよい」という規定に従い、Red-Green-Refactorサイクルの対象外とする。代わりに、下記「完了条件」の動作確認をもって検証する。

---

## 実装仕様

### .env.example

以下の変数を追加する（値は現状のローカル開発用デフォルトを踏襲する）。

```
# ローカルDB（.devcontainer/compose.yaml の db サービスが env_file 経由で読み込む）
MYSQL_ROOT_PASSWORD=rootroot
MYSQL_DATABASE=stamprally
MYSQL_USER=gantaro
MYSQL_PASSWORD=stamprally_dev_pass

# devcontainerのホストポート割当（複数ワークツリーを同時に開く場合はここを変更する）
DB_HOST_PORT=3307
APP_HOST_PORT=8099
```

既存の `DATABASE_URL` はこれらの値と一致させたまま残し、コメントで「`MYSQL_*` を変更したら `DATABASE_URL` も合わせて変更すること」を明記する。

### .devcontainer/compose.yaml

- `db` サービス:
  - `environment:` の直書きをやめ、`env_file: ../.env` に置き換える（`compose.yaml` からの相対パスでプロジェクトルートの `.env` を読み込む）
  - `healthcheck` を `CMD-SHELL` 形式にし、環境変数を参照する形に変更する
    ```yaml
    healthcheck:
      test: ["CMD-SHELL", "mysqladmin ping -h localhost -u root -p\"$MYSQL_ROOT_PASSWORD\""]
      interval: 5s
      timeout: 5s
      retries: 10
    ```
  - ポートを `"127.0.0.1:${DB_HOST_PORT:-3307}:3306"` に変更する
- `rust_app` サービス:
  - `environment: DATABASE_URL: ...` の直書きをやめ、`env_file: ../.env` に置き換える（これによりDATABASE_URLに加え、LINE関連のシークレット等も自動的にコンテナへ注入されるようになる）
  - ポートを `"127.0.0.1:${APP_HOST_PORT:-8099}:8000"` に変更する

### .devcontainer/Dockerfile

- `mcr.microsoft.com/devcontainers/rust:1.85-bookworm` というタグが実在するか確認する
- 実在しない場合は、Cargo.tomlの `edition = "2024"`（Rust 1.85以降が必要）を満たす実在のタグに置き換える

---

## 制約・注意事項

- `env_file` に指定する `.env` は既存どおり `.gitignore` の対象のままとし、`.env.example` のみを更新する（`.env` 自体をコミットしない）
- `docker compose config`（または同等の方法）でYAMLの構文・変数展開が正しく解釈されることを確認する
- 複数ワークツリーを同時に起動する場合の運用（`DB_HOST_PORT` / `APP_HOST_PORT` を別々の値に設定する）は、READMEやコメントで簡単に触れる程度でよく、自動割当の仕組みまでは不要

---

## 完了条件

- [ ] `.env.example` にMySQLブートストラップ変数（`MYSQL_ROOT_PASSWORD` 等）とポート変数（`DB_HOST_PORT`, `APP_HOST_PORT`）が追加されている
- [ ] `.devcontainer/compose.yaml` の `db` / `rust_app` 両サービスが `env_file: ../.env` を参照し、シークレットの直書きが無くなっている
- [ ] `healthcheck` が `MYSQL_ROOT_PASSWORD` 環境変数を参照する形になっている
- [ ] devcontainerを再ビルドし、`db` サービスのhealthcheckが通ることを確認した
- [ ] devcontainer内で `echo $DATABASE_URL` が `.env` の値と一致することを確認した
- [ ] `.devcontainer/Dockerfile` のベースイメージタグが実在することを確認した（必要であれば修正済み）
- [ ] `cargo build` が成功する
- [ ] 本作業を `feature/devcontainer-hardening` ブランチで行い、Pull Requestを作成した
