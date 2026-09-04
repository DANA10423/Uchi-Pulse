# uchi-pulse-hub

Uchi Pulse の Raspberry Pi Zero / Zero 2 W 向け親機ベースプログラムです。
`docs/parent_child_udp_communication_spec.md` に基づく UDP/JSON の受信処理を実装しています。

実装済みの範囲:

- UDP待受（デフォルト `0.0.0.0:5000`）
- `HELLO` / `HEARTBEAT` / `EVENT` の受信
- SQLiteに登録された `enabled = 1` の子機だけを処理
- ONLINE/OFFLINE管理（デフォルト180秒、メモリ上のみ）
- `(device_id, event_id)` によるEVENT重複排除
- EVENTのACK返信（重複EVENTにも再返信）
- EVENT履歴のSQLite保存
- 起動時の `HELLO_REQUEST` ブロードキャスト

## 起動

```sh
cargo run -p uchi-pulse-hub
cargo run -p uchi-pulse-hub -- --bind 0.0.0.0:5000 --db ./uchi-pulse.db --offline-timeout-sec 180
```

起動時の設定は `--bind`、`--db`、`--hello-request-addr`、`--offline-timeout-sec` で指定できます。対応する環境変数は `UCHI_PULSE_BIND`、`UCHI_PULSE_DB`、`UCHI_PULSE_HELLO_REQUEST_ADDR`、`UCHI_PULSE_OFFLINE_TIMEOUT_SEC` です。

現段階ではAction実行エンジン、Web UI、外部通知、子機側の新UDP通信処理は未実装です。

## テスト

```sh
cargo test -p uchi-pulse-hub
```
