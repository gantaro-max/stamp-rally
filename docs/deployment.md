# デプロイ手順 — StampRallyBot

デプロイ構成の設計判断（ホスティング先の選定・ビルド方式・DB・環境変数・セッション等）は [architecture.md 18節](architecture.md#18-デプロイ構成render) を参照。ここでは実際にデプロイする際の作業手順のみをまとめる。

---

## 1. 外部サービスの準備（初回のみ）

1. **LINE Official Account Manager**（https://manager.line.biz）で、本アプリ専用のLINE公式アカウントを新規作成する（他アプリ・他イベントの公式アカウントとは別に作成する。名前・アイコンは本イベント用に設定する）
2. 作成した公式アカウントの設定からMessaging APIを有効化する（2024年9月4日の仕様変更により、LINE Developersコンソールから直接Messaging APIチャネルを新規作成することはできなくなっており、必ずLINE公式アカウント側から有効化する手順を踏む。プロバイダは既存のものを選択できる）
3. 有効化後、LINE DevelopersコンソールにそのMessaging APIチャネルが表示されるので、そこから`Channel secret`（`LINE_CHANNEL_SECRET`）・`Channel access token`（`LINE_CHANNEL_ACCESS_TOKEN`）を発行する
4. 同じプロバイダの配下に、LIFFアプリ登録用の**LINEログインチャネル**を新規作成する（Messaging APIチャネルへのLIFF直接追加は廃止されているため、必ず別チャネルとして作成する。LINEミニアプリチャネルという選択肢もあるが、審査は不要になったものの開発/審査/本番の3チャネル構成で運用がやや煩雑なため、非公開の単発イベント用途である本アプリではLINEログインチャネルを選ぶ）
5. このLINEログインチャネルのLIFFタブでLIFFアプリを追加し、`LIFF_ID` を発行する（エンドポイントURLは「2. Renderでの初回セットアップ」でRenderのURLが確定してから設定する）
6. このLINEログインチャネル自体のチャネルIDを `LINE_LOGIN_CHANNEL_ID` として控える（Messaging APIチャネルのIDとは別物）
7. 既存のTiDB Cloudアカウント・クラスタ内に、このアプリ専用のデータベース（`stamprally`）と専用DBユーザーを作成する（既存の別アプリのデータベースとは分離する。詳細は [SECURITY.md](../SECURITY.md)「本番DB（TiDB Serverless）の接続方針」）。発行された接続文字列を控える（`DATABASE_URL`に設定する値）

---

## 2. Renderでの初回セットアップ

1. Renderで新規Web Serviceを作成し、このリポジトリを接続する。ランタイムは**Docker**を選択する（リポジトリルートの本番用`Dockerfile`が使われる。`.devcontainer/Dockerfile`とは別物）
2. アプリは`PORT`環境変数のポートでリッスンする（未設定時は8000にフォールバック）。Renderは`PORT`を自動で渡すため、通常は追加設定不要
3. Health Check Pathに `/health` を設定する
4. インスタンス数が1（オートスケールしない構成）になっていることを確認する（セッションをプロセス内メモリで保持する設計のため）
5. Environment Variables に、上記手順1で控えた値と `PUBLIC_BASE_URL`（Renderが割り当てたURL）・`ADMIN_PASSWORD`（初期管理者パスワード）を設定する
6. `DATABASE_URL` に手元から `sqlx migrate run` でマイグレーション適用済みのTiDB Serverless接続文字列を設定する（マイグレーションの適用方法は次節）
7. デプロイを実行し、`https://<Renderドメイン>/health` が200を返すことを確認する
8. LINE DevelopersコンソールのWebhook URLを `https://<Renderドメイン>/callback` に、LIFFのエンドポイントURLを `https://<Renderドメイン>/liff/checkin` に設定する

> 無料枠では約15分アクセスがないとインスタンスがスリープする。スリープ中に届いたWebhookは、コールドスタートの待ち時間の分だけ応答が遅れる。参加者が最初のメッセージを送る前に、管理画面などにアクセスして起動させておくと体験が安定する（設計上の扱いは [architecture.md 18節](architecture.md#18-デプロイ構成render)）。

---

## 3. スキーマ変更を含むリリース時の手順

本番DBへの自動マイグレーションは行わない。スキーマ変更（`migrations/`への追加）を含むリリースでは、コードをデプロイする前に手元で以下を実行する。

```bash
DATABASE_URL=<本番のTiDB Serverless接続文字列> sqlx migrate run
```

適用結果を確認してから、通常どおりコードをRenderにデプロイする。
