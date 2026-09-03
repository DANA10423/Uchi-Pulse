# uchi-pulse-hub

Uchi Pulse の Raspberry Pi Zero / Zero 2 W 向け親機ベースプログラムです。
`docs/parent_child_udp_communication_spec.md` に基づく UDP/JSON の受信処理を実装しています。

実装済みの範囲:

- UDP待受（デフォルト `0.0.0.0:5000`）
- `HELLO` / `HEARTBEAT` / `EVENT` の受信
- 有効なパケットからの子機自動登録
- `device_id` を識別子とした送信元IP/ポート更新
- ONLINE/OFFLINE管理（デフォルト210秒）
- `device_id + message_id` によるEVENT重複排除
- EVENTのACK返信（重複EVENTにも再返信）
- 子機一覧とイベント履歴のメモリ保持（再起動時に破棄）

## 起動

```sh
cargo run -p uchi-pulse-hub
cargo run -p uchi-pulse-hub -- --bind 0.0.0.0:5000 --offline-timeout-sec 210
```

環境変数 `UCHI_PULSE_BIND` と `UCHI_PULSE_OFFLINE_TIMEOUT_SEC` でも設定できます。

現段階では Web UI、永続化、親機から子機への問い合わせ通知・出力制御は未実装です。
このプロジェクトは、それらを追加するための通信・状態管理のベースです。

## テスト

```sh
cargo test -p uchi-pulse-hub
```

