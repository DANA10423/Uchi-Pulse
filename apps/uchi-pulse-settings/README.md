# uchi-pulse-settings

Uchi-Pulse親機のSQLite設定を、エンドユーザー向けGUIで管理する独立デスクトップアプリです。

## 起動

ワークスペースのルートで実行します。

```sh
cargo run -p uchi-pulse-settings
```

起動後に親機のSQLiteファイルを指定して「開く」を押してください。SQL入力やUSB CDC接続は必要ありません。

GUIから次の情報を確認・更新できます。

- 子機
- 家族
- Action
- Actionの状態変更
- Actionの通知設定と通知先家族
- 家族ごとの外部通知先
- イベント履歴の確認

親機がDBを使用中の状態で更新せず、親機を停止してから保存してください。保存後は親機を再起動してください。
