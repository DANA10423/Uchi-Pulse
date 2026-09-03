# uchi-pulse-node

Uchi Pulse の Raspberry Pi Pico W / Pico 2 W 向け子機ファームウェアです。
`docs/parent_child_udp_communication_spec.md` の UDP/JSON 仕様に合わせ、次を実装しています。

- 起動時の `HELLO`
- 3 分間隔の `HEARTBEAT`
- GPIO入力を論理チャンネルへ変換した `EVENT`
- `EVENT` の ACK 待ち（3秒）と最大3回再送
- `device_id + message_id` による ACK 照合
- 設定可能な Hub アドレス、Wi-Fi、GPIO論理割り当ての定義

## ビルド

先に `firmware/README.md` の CYW43439 ファームウェアを配置し、`wifi_ssid`、
`wifi_password`、`hub_ipv4` を `src/config.rs` で変更してください。

```sh
# Pico W (RP2040)
cargo build-pico-w

# Pico 2 W (RP2350)
cargo build-pico-2-w
```

デバッグプローブで書き込む場合は `cargo run-pico-w` / `cargo run-pico-2-w` を使います。
BOOTSEL/UF2 書き込みを使う場合は生成された ELF を `elf2uf2-rs` 等で変換してください。

## 現在の設定方式

GPIO・Wi-Fi・Hub設定は、初期実装では `src/config.rs` のコンパイル時設定です。
設計書で定義されている USB CDC 初期設定は、次の拡張ポイントとして残しています。
通信フォーマットは `src/protocol.rs` に分離しているため、USB CDC の設定保存を追加しても
UDP通信層を変更せずに対応できます。

## テスト

通信JSONはホスト上でテストできます。

```sh
cargo test -p uchi-pulse-node
```

