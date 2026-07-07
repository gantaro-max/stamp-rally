# 実装指示書: 軽微な指摘対応（CSRF定数時間比較・QR一覧表示・SRI属性）

## 背景・目的

Claudeによる全体最終レビュー（設計整合性・セキュリティ・要件充足・実装指示書・TDD遵守の5観点）で見つかった、ブロッキングではない軽微な指摘のうち、以下3件を対応する。いずれも独立した小さな変更であり、関連はないが、まとめて1つの指示書・1つのブランチで扱う。

1. **CSRFトークン比較のタイミング攻撃対策**（セキュリティレビュー Low指摘）: `csrf_service::verify_token` のトークン比較が通常の文字列比較（`==`）になっている。同じコードベースの `line_client::verify_signature` はLINE署名検証に定数時間比較（`constant_time_eq`）を使っているのに対し、CSRFトークン側は未使用という一貫性の欠如がある。実害は低い（ネットワーク越しのタイミング攻撃は一般に困難）が、防御多層化のため統一する。
2. **QR一覧画面のUX改善**（要件充足レビュー 軽微指摘）: `docs/requirements.md`の「QRコードを部屋ごとに自動生成し、管理画面で表示」という要件自体は満たしているが、`templates/admin/rooms/list.html`では一覧にQR画像が直接表示されず、「QR表示」リンクで別ページ（PNG直接返却）に遷移する形になっている。一覧上でQRコードをサムネイル表示し、視認性を上げる。
3. **Bootstrap CDNへのSRI属性追加**（セキュリティレビュー Info指摘）: `templates/liff/checkin.html`がBootstrap CSSをCDN（jsDelivr）から読み込んでいるが、Subresource Integrity（`integrity`属性）が設定されておらず、CDN側が改ざんされた場合のサプライチェーンリスクがある。なお、同ファイルが読み込むLIFF SDK（`https://static.line-scdn.net/liff/edge/2/sdk.js`）はLINE公式の「エッジ版・随時更新される」URLであり、内容が固定されないためSRI適用が構造的に困難。本対応はBootstrap CSSのみを対象とする。

## 実装対象ファイル

- `src/services/csrf_service.rs` — トークン比較を定数時間比較に変更
- `templates/admin/rooms/list.html` — QRコードのサムネイル表示を追加
- `templates/liff/checkin.html` — Bootstrap CSSの`<link>`にSRI属性を追加

## テストケース（TDDの起点）

### 1. CSRF定数時間比較

タイミング攻撃耐性そのものを自動テストで検証するのは現実的ではない（実行時間の統計的検証はCIで安定しない）。以下の機能的な回帰確認に留める。

- [ ] ケースA（回帰確認）: 既存の `csrf_service::tests::issues_and_verifies_session_csrf_token`（`src/services/csrf_service.rs`）が、比較ロジック変更後も引き続きパスすることを確認する。新規テストは不要（値が一致する場合はtrue、長さ違い・値違い・空文字列はfalseという既存の期待値は変えない）

### 2. QR一覧表示

- [ ] ケースB（回帰確認）: 既存の `tests::authenticated_session_can_view_rooms`（`src/main.rs`）が、テンプレート変更後も引き続きパスすることを確認する（このテストは部屋名の文字列が含まれることしか検証していないため、テンプレートの表示形式変更では壊れないはず）
- [ ] ケースC（任意・推奨）: 一覧のレスポンスHTMLに、各部屋のQR画像を指す`<img>`タグ（`src="/admin/rooms/{id}/qr"`)が含まれることを検証する新規テストを追加する

### 3. SRI属性

自動テストでの検証は困難（CDNへの実際のリクエストが必要なため）。以下の確認で代替する。

- [ ] ケースD: 変更後、実際にブラウザ（`cargo run`でアプリを起動し`/liff/checkin`を表示するか、`curl`でHTML内容を確認）でBootstrapのスタイルが正しく適用されていること（`integrity`属性のハッシュ値が実際のCDN配信物と一致しており、ブラウザがブロックしていないこと）を目視確認する。自動テストのケースとしては、`integrity`属性がHTMLに含まれることを文字列として検証する軽量なテストを追加してもよい（必須ではない）

## 実装仕様

### src/services/csrf_service.rs

- `line_client.rs`にある`constant_time_eq`（長さが異なれば即`false`、同じ長さならXORの累積で全バイトを比較する実装）と同等のロジックを、`csrf_service.rs`内にプライベート関数として追加する（`line_client`の関数を`pub`にして相互参照させるのではなく、同じ小さな関数をこのモジュールにも複製する。既存のコードベースが同種の関数を各モジュールに閉じて持たせる方針と一貫させるため）
- `verify_token`内の`token == submitted`という比較を、上記の定数時間比較関数を使った比較に置き換える。文字列なので`.as_bytes()`で`&[u8]`に変換してから比較する
- 空文字列チェック（`submitted.is_empty()`で早期`false`）はそのまま維持する

### templates/admin/rooms/list.html

- QR列に、既存の「QR表示」リンクに加えて（または置き換えて）、サムネイル画像を追加する:

```html
<td>
  <a href="/admin/rooms/{{ room.id }}/qr"><img src="/admin/rooms/{{ room.id }}/qr" alt="QRコード" width="60" height="60"></a>
</td>
```

- `docs/operator-guide.md`5節「QRコードの準備とスタッフへの配布」の手順（一覧から「QRコードを表示」をクリックして印刷用ページへ遷移する）と矛盾しないよう、QRコードへのリンク自体は維持すること（サムネイル画像をそのリンクで囲む形にする）

### templates/liff/checkin.html

- Bootstrap CSSの`<link>`タグに`integrity`・`crossorigin="anonymous"`属性を追加する:

```html
<link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet" integrity="sha384-<実際の正しいハッシュ値>" crossorigin="anonymous">
```

- **重要**: `integrity`のハッシュ値は必ず実際に取得・計算して設定すること。誤ったハッシュ値を設定すると、ブラウザがCSSの読み込みをブロックし、ページのスタイルが完全に崩れる（現状の「SRIなし」より悪い状態になる）。取得方法の例:
  - `curl -s https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css | openssl dgst -sha384 -binary | openssl base64 -A` を実行し、出力に`sha384-`を前置する
  - または jsDelivrの公式ページ（`https://www.jsdelivr.com/package/npm/bootstrap`でバージョン5.3.3を選択）に表示される、公式配布の`<link>`タグをそのまま使う
- 設定後、`cargo run`でアプリを起動し、`/liff/checkin`をブラウザで開いてBootstrapのスタイルが正しく適用されていること（崩れていないこと）を必ず目視確認すること。この目視確認をせずに完了報告しないこと
- LIFF SDKの`<script>`タグ（`static.line-scdn.net`）にはSRIを追加しない（LINE公式のエッジ版URLで内容が固定されないため）

## 制約・注意事項

- スコープ外の機能追加・リファクタリングを行わないこと
- `cargo clippy` が警告なく通ること
- 3件は独立した変更なので、コミットは項目ごとに分けること（例: `fix: use constant-time comparison for csrf token`, `feat: show qr thumbnail in room list`, `fix: add sri integrity to bootstrap cdn link`）。TDD的な観点で自動テストが書けない項目（3件目）は、コミットメッセージと目視確認の実施記録で代替する

## 完了条件

- [ ] `csrf_service`のトークン比較が定数時間比較になり、既存テストが引き続き通る
- [ ] QR一覧画面にサムネイル画像が表示され、既存の「QRコードを表示」導線（`docs/operator-guide.md`の手順）が維持されている
- [ ] Bootstrap CDNの`<link>`に正しい`integrity`属性が設定され、実際にブラウザで`/liff/checkin`のスタイルが正しく表示されることを目視確認した
- [ ] `cargo test` が全体で通る
- [ ] `cargo clippy` が警告なく通る
- [ ] ブランチをリモートにpushし、Pull Requestを作成した
