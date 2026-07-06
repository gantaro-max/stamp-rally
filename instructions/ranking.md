# 実装指示書: ランキング画面（Slice D）

## 背景・目的

これまでの機能（管理者認証・部屋管理・LINE Bot基盤・LIFFチェックイン・イベント設定）により、参加登録からゴール判定（`players.finished_at`の記録）までのゲームループと、管理者による事前設定は完成した。本スライスでは、運営が現在の順位を確認できる `/admin/ranking` を実装し、一連の機能を完結させる。

[docs/architecture.md](../docs/architecture.md) の17節（本指示書と合わせて追記済み）で決定した方針に基づき、以下を実装する。

- `player_repository::find_all_by_event`：対象イベントの全参加者取得
- `ranking_service`：クリア済み参加者を所要時間（`finished_at - started_at`）の短い順に並べ、未クリア参加者は順位を付けず別掲する
- `GET /admin/ranking`：ランキング画面

---

## 実装対象ファイル

- `src/main.rs` — `admin_router` に `/ranking` ルーターを追加
- `src/handlers/admin.rs` — `ranking` ハンドラーを追加
- `src/services/mod.rs` — `ranking_service` の公開
- `src/services/ranking_service.rs`（新規） — ランキング組み立てロジック
- `src/repository/player_repository.rs` — `find_all_by_event` を追加
- `templates/admin/ranking.html`（新規） — ランキング画面
- `templates/admin/_base.html` — ナビゲーションに「ランキング」リンクを追加

---

## テストケース（TDDの起点）

[AGENTS.md](../AGENTS.md) のTDD規約に従い、以下の順にRed-Green-Refactorを回す。DBに依存するテストは `sqlx::test` を、DBに依存しない純粋関数は通常の `#[test]` を使うこと。

### player_repository（追加分、`sqlx::test`）

- [ ] ケース1: `find_all_by_event` が、指定した `event_id` に属する参加者をすべて返す

### ranking_service::build_ranking（DB非依存の純粋関数。`player_repository::Player` の値を直接組み立ててテストする。DB接続不要）

- [ ] ケース2: クリア済みの参加者が2名（所要時間が異なる）いる場合、所要時間が短い方が1位、長い方が2位になる
- [ ] ケース3: 未クリアの参加者（`finished_at = None`）は順位が付かず、`unfinished` に含まれる。複数いる場合は `started_at` の昇順に並ぶ
- [ ] ケース4: クリア済みが0名で未クリアのみの場合でも、`ranked` が空・`unfinished` に全員が正しく入る
- [ ] ケース5: 所要時間が1時間以上の参加者がいる場合、`elapsed_display` が `H:MM:SS` 形式になる（1時間未満は `M:SS` 形式）
- [ ] ケース6: 所要時間が同一（同着）の参加者が2名いても、例外を起こさず連番の順位（1位・2位）が振られる

### ranking_service::get_ranking（`sqlx::test`。薄いラッパーの疎通確認のみでよい）

- [ ] ケース7: DBに登録した参加者（クリア済み1名・未クリア1名）に対して `get_ranking` を呼ぶと、`build_ranking` を直接呼んだ場合と同じ内容の `RankingView` が返る

### ハンドラー（`sqlx::test` / 結合テスト）

- [ ] ケース8: 未ログイン状態で `GET /admin/ranking` にアクセスすると302で `/auth/login` へリダイレクトされる
- [ ] ケース9: ログイン済みでクリア済み参加者が1名以上いる状態で `GET /admin/ranking` にアクセスすると200が返り、レスポンスに参加者名と順位・所要時間が含まれる
- [ ] ケース10: ログイン済みで未クリアの参加者がいる状態で `GET /admin/ranking` にアクセスすると200が返り、その参加者名が「圏外」（未クリア）セクションに表示され、順位は付いていない

---

## 実装仕様

### src/repository/player_repository.rs（追加分）

- `find_all_by_event(pool: &MySqlPool, event_id: i32) -> Result<Vec<Player>, sqlx::Error>`（既存の `find_by_line_user_and_event` 等と同様、手動で `Row::try_get` する。`SELECT * FROM players WHERE event_id = ?` 相当）

### src/services/ranking_service.rs

- `pub struct RankedEntry { pub rank: usize, pub player_name: String, pub elapsed_display: String }`
- `pub struct UnfinishedEntry { pub player_name: String }`
- `pub struct RankingView { pub ranked: Vec<RankedEntry>, pub unfinished: Vec<UnfinishedEntry> }`
- `pub fn build_ranking(players: Vec<player_repository::Player>) -> RankingView`
  - `finished_at` が `Some` の参加者を `finished_at - started_at`（`chrono::TimeDelta`）の昇順にソートし、1位から順に `rank` を振って `RankedEntry` にする
  - `finished_at` が `None` の参加者は `started_at` の昇順にソートして `UnfinishedEntry` にする
  - 所要時間の表示形式は非公開のヘルパー関数（例: `fn format_elapsed(duration: chrono::TimeDelta) -> String`）で組み立てる。総分数が60未満なら `M:SS`、60以上なら `H:MM:SS`（分・秒は2桁ゼロ埋め）
- `pub enum RankingError { Database(sqlx::Error) }`（`From<sqlx::Error>`を実装）
- `pub async fn get_ranking(pool: &MySqlPool, event_id: i32) -> Result<RankingView, RankingError>` — `player_repository::find_all_by_event` の結果を `build_ranking` に渡すだけの薄いラッパー

### src/handlers/admin.rs（追加分）

- `pub async fn ranking(State(pool): State<MySqlPool>) -> impl IntoResponse`
  - `event_service::current(&pool)` で現在のイベントを取得する（Slice Cで追加済みの `event_service` を利用する。`event_repository::find_singleton` を新たに直接呼ばないこと）
  - `ranking_service::get_ranking(&pool, event.id)` の結果を `templates/admin/ranking.html` にレンダリングする
- ルーティングは既存の `rooms`/`settings` と同じく `admin_router`（`require_admin` 配下）に追加する。状態変更を伴わないGETのみのため、CSRFトークンの発行・検証は不要

### templates/admin/ranking.html

- `_base.html` を継承する
- 「クリア済み」テーブル：`rank` / `player_name` / `elapsed_display` の列を持つ。`ranking.ranked` が空なら「クリア済みの参加者はいません」等を表示する
- 「未クリア（圏外）」セクション：`player_name` の一覧のみ（順位は表示しない）。`ranking.unfinished` が空なら「全員クリア済みです」等を表示する

### templates/admin/_base.html

- 既存の「部屋管理」「設定」リンクの並びに「ランキング」（`/admin/ranking`）へのリンクを追加する

---

## 制約・注意事項

- 既存の `/health`・管理者認証・部屋管理・LINE Bot基盤・LIFFチェックイン・イベント設定の挙動とテストを壊さないこと
- ランキングの並び順は「所要時間（`finished_at - started_at`）の短い順」であり、`finished_at`の絶対時刻順ではないこと（[architecture.md](../docs/architecture.md) 17節）
- 未クリアの参加者を非表示にせず、順位無しの別セクションに表示すること（要件の「圏外表示」）
- 自動更新（WebSocket・ポーリング等）は実装しないこと。ページ読み込みのたびにDBの最新状態を反映していれば「リアルタイム」要件を満たすものとする
- `event_service::current`（Slice Cで追加済み）を利用し、`event_repository::find_singleton` を新たに直接呼び出さないこと
- `build_ranking` はDB・`sqlx`に依存しない純粋関数として実装し、`get_ranking`と分離すること（DB非依存のテストで大部分のロジックを検証できるようにするため）
- `docs/api.md` に記載のパス・メソッドと一致させること
- `cargo clippy --all-targets -- -D warnings` が警告なく通ること

---

## 完了条件

- [ ] 上記テストケースすべてについて、実装前に失敗するテストを書いたことを確認した（Red）
- [ ] 各テストを通す最小限の実装を行った（Green）
- [ ] リファクタリング後もすべてのテストが通ることを確認した（Refactor）
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy --all-targets -- -D warnings` が警告なく通る
- [ ] `cargo run` で `/admin/ranking` にアクセスし、クリア済み参加者が順位付きで、未クリア参加者が圏外セクションに表示されることを手動で確認した
