# 実装指示書: LINE Bot基盤 差し戻し修正（最終レビュー指摘対応）

## 背景・目的

`feature/line-bot-core`（PR #5、LINE Webhook基盤・ゲーム進行ロジック）はCodexの一次レビューを経て実装済みだが、Claudeによる最終レビュー（設計整合性・セキュリティ・要件充足・実装指示書・TDD遵守の5観点）で以下2件の問題が見つかった。マージ前にこのブランチ上で修正すること。

1. **`.env.example`に`PUBLIC_BASE_URL`が無い**: `src/main.rs`は起動時に`PUBLIC_BASE_URL`が未設定だと`process::exit(1)`する仕様だが、`.env.example`にはこの変数が記載されていない。`cp .env.example .env`しただけでは起動できない（Claude側の設計ドキュメント更新時に`.env.example`への追記が`main`へのコミット漏れとなっており、それがそのままこのブランチにも反映されていなかったことが原因。設計上の指示自体は[instructions/line-bot-core.md](line-bot-core.md)・[docs/architecture.md](../docs/architecture.md) 14節の通りで変わらない）
2. **TDD規約違反**（[AGENTS.md](../AGENTS.md)のRed-Green-Refactor必須ルールに反する）:
   - コミット`3face4a`（`feat: wire line webhook and public image routes`）で、[instructions/line-bot-core.md](line-bot-core.md)のケース37〜39（`POST /callback`正常系・`GET /public/image/{uuid}`の200/404）に対応するテストを、実装と**同一コミット**で追加している。先行する「対象コードが無く実際に失敗するテスト（Red）」のコミットが存在しない
   - コミット`af6ca8f`（`test: cover webhook and image integration`）は、ラベルに反して新規テストを1件も追加していない。差分の実体は`cargo fmt`相当の再フォーマットのみ（改行・インデント調整、`#[cfg(test)]`属性の位置調整等）であり、"test:"コミットとして意味のある失敗を経ていない

この指示書のスコープは上記2件の修正のみ。他の機能追加・リファクタリングは行わないこと。

---

## 実装対象ファイル

- `.env.example` — `PUBLIC_BASE_URL`の行を追加
- `src/handlers/line_webhook.rs` / `src/handlers/image.rs`（テストモジュール） — ケース37〜39を正しいRed→Green分離コミットに書き直す

---

## 修正内容（TDDの起点）

### `.env.example`

- [ ] ケースA: `LINE_CHANNEL_ACCESS_TOKEN`の行の後に、以下を追記する（`docs/architecture.md` 14節・`instructions/line-bot-core.md`の記載と同一内容）:
  ```
  # 画像URL等を組み立てる際に前置する公開ベースURL（末尾スラッシュなし）
  PUBLIC_BASE_URL=http://localhost:8000
  ```
- テスト対象外の変更（設定ファイルの追記のみ）。ただし単独のコミット（例: `docs: add PUBLIC_BASE_URL to .env.example`）として記録すること

### ケース37〜39のRed→Green再構成

現在ケース37〜39のテスト（`callback_with_valid_text_message_updates_game_state`、`public_image_returns_stored_image`、`public_image_returns_not_found_for_missing_uuid`）は`3face4a`で実装と同時に追加されている。これを以下の手順でやり直す。

- [ ] ケースB（Red）: 一旦、該当3テストのみを含むコミットを作成し、対象のルート・ハンドラー実装が無い（またはスタブ状態の）状態で実際にコンパイルエラーまたはテスト失敗になることを確認してからコミットする
  - 過去のコミット`3face4a`を丸ごとrevertし直す必要はない。「テストが実装より先に失敗した」という履歴をこれから作り直せればよいので、実務的には次のいずれかの方法でよい:
    - (a) 一時的に対象実装を`todo!()`やコメントアウトに戻し、3テストが失敗する状態でコミットしてから、実装を復元するコミットを続ける
    - (b) 新しいテストケースを1件追加する形で改めてRed→Greenを踏み直す（例えば、現状カバーされていないエッジケースを追加テストとして先に書き、失敗を確認してから対応する実装を足す。この場合、既存のケース37〜39自体は「後付けだった」事実を今回のコミットメッセージに明記し、新規に追加するテストで正しいTDDサイクルを1件示す）
  - どちらの方法を取るかはCodexの判断に委ねるが、**最終的なgit履歴上で「対象コードが存在しない/失敗する状態のテスト単独コミット」→「それを通す最小実装のコミット」の順序が明確に確認できること**が必須条件
- [ ] ケースC（Green）: ケースBのテストを通す最小限の実装（または実装の復元）を行い、個別コミットにする
- [ ] ケースD: `af6ca8f`相当の整形差分は、"test:"ではなく`refactor:`または`style:`など内容に即したコミットメッセージにする（新規テストを含まないコミットを"test:"と名乗らないこと）

---

## 制約・注意事項

- 既存のテストケース1〜36・38〜39（ケース37以外、および今回作り直す37以外）を壊さないこと。`cargo test`が全体で通ること
- スコープ外の機能追加・リファクタリングを行わないこと
- `docs/architecture.md`・`docs/api.md`・`instructions/line-bot-core.md`の記載と実装の対応関係（既に最終レビューで確認済み）を変えないこと
- `cargo clippy`が警告なく通ること
- コミットメッセージ（`test:`/`feat:`/`refactor:`等のプレフィックス）と実際の差分内容を一致させること。整形のみの変更を"test:"と名乗らないこと

---

## 完了条件

- [ ] `.env.example`に`PUBLIC_BASE_URL`が追記されている
- [ ] ケース37〜39について、Red（意味のある失敗）→Green（最小実装）の順序がgit履歴から確認できる
- [ ] `cargo test`が全体で通る（既存のケース1〜39を含む）
- [ ] `cargo clippy`が警告なく通る
- [ ] `git log`上で、各コミットのメッセージ（`test:`/`feat:`/`refactor:`）と実際の変更内容が一致していることを確認した
