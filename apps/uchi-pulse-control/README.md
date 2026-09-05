# uchi-pulse-control

Uchi-Pulseの子機・親機をUSB CDC経由で操作し、親機SQLiteの設定をGUIで管理できるRust製デスクトップアプリです。

## 起動

ワークスペースのルートで実行します。

```sh
cargo run -p uchi-pulse-control
```

画面上部で対象（子機／親機）とCDCポートを選び、「接続」を押してください。macOSでは
`/dev/cu.usbmodem...`、Linuxでは`/dev/ttyACM...`、Windowsでは`COM...`として表示されます。

## 操作

- 情報・設定・状態・入力・出力の取得
- 子機または親機のJSON設定の編集と保存
- 設定初期化、子機再起動
- 送受信JSONの通信ログ表示
- 親機SQLiteの設定確認・編集

初期化と再起動は確認ダイアログを表示します。CDC通信は
`docs/cdc_communication_spec.md`のバージョン1、LF終端JSONに従います。

日本語表示のため、起動時にmacOS・Windows・Linuxの日本語フォント候補を検索して
eguiへ登録します。採用されたフォントは画面上部に表示されます。

## 注意

子機側でまだ実装されていないコマンドは、画面上に`NOT_SUPPORTED`として表示されます。
`set_config`は仕様上すべての設定項目が必須です。

## 親機CDC

親機プログラムをRaspberry Pi上でUSB Gadgetのシリアルデバイスに接続して起動します。
通常は以下のように`/dev/ttyGS0`を指定します。

```sh
uchi-pulse-hub --db /var/lib/uchi-pulse/uchi-pulse.db --cdc-device /dev/ttyGS0
```

環境変数`UCHI_PULSE_CDC_DEVICE`でも指定できます。親機の設定画面では、家族、Action、
Action状態変更、通知設定、通知先を取得・編集・保存できます。イベント履歴と子機の
通信状態は設定JSONの対象外です。

## 親機SQLiteの設定

画面上部の対象で「SQLite」を選び、親機起動時に指定したDBファイルを開きます。
例:

```text
./uchi-pulse.db
```

GUIからSQLを入力せずに、次の設定を編集できます。

- 子機の登録・有効/無効・表示名
- 家族の登録・有効/無効・表示名
- Actionの対象、対象家族、Web表示メッセージ
- Actionの状態変更（複数設定）
- Actionの通知有効化、通知メッセージ、通知先家族
- イベント履歴の確認

設定変更前に親機を停止し、DBのバックアップを取得してください。保存後は親機を再起動してください。
