# uchi-pulse-node

Uchi Pulse の Raspberry Pi Pico W / Pico 2 W 向け子機ファームウェアです。
PHASE 6/7/8では、GPIO入力から共通Input Eventを判定し、設定されたAction IDを正式なUDP EVENTとして親機へ送信します。また、USB CDCで子機設定を取得・保存できます。

- デバウンス付きGPIO入力変化検出
- `OFF_TO_ON` / `ON_TO_OFF` / `CLICK` / `DOUBLE_CLICK` / `LONG_PRESS`
- `GPIO + Input Event -> Action ID` のデータ駆動マッピング
- GPIO入力は最大10個まで設定可能
- GPIO 2/3/4固定ではなく、Pico W / Pico 2 Wの外部利用可能GPIO（0〜22、26〜28）から選択可能
- ホスト上でテスト可能な入力ロジックとUDPプロトコル処理
- `boot_id + sequence` によるEVENT識別子
- EVENT ACK待機（60秒）と同一EVENTの最大3回再送
- 親機の `HELLO_REQUEST` ブロードキャストによる親機自動探索
- 発見した親機エンドポイントへの180秒間隔のHEARTBEAT
- USB CDCのLF区切りJSON（`get_config` / `set_config` / `factory_reset` / `reboot`）
- 設定のvalidation、Flash保存、次回起動時の復元

GPIO 23/24/25/29 はオンボードCYW43 Wi-Fi接続で使用するため、GPIO入力設定の対象外です。
UDPは正式な共通wire formatを使用します。GPIO番号、Input Event、Actionの業務的意味はUDPへ含めません。

## ビルド

先に `firmware/README.md` の CYW43439 ファームウェアを配置してください。
初回起動時は `src/config.rs` の既定SSID・パスワード（`change-me`）が使われます。
実運用のWi-Fi/IP設定はUSB CDCの `set_config` で設定してください。

```sh
# Pico W (RP2040)
cargo build-pico-w

# Pico 2 W (RP2350)
cargo build-pico-2-w
```

デバッグプローブで書き込む場合は `cargo run-pico-w` / `cargo run-pico-2-w` を使います。
BOOTSEL/UF2 書き込みを使う場合は生成された ELF を `elf2uf2-rs` 等で変換してください。

## 現在の設定方式

Wi-Fi SSID・パスワード、IP取得方式（`DHCP` / `STATIC`）、固定IPv4設定、GPIO・Input Mapping・
ジェスチャー時間・UDP送信タイミングはUSB CDCから変更でき、最終4 KiBの設定保存領域へ保存されます。
保存設定は次回起動時に反映されます。DHCPが既定値です。

固定IPv4を使う場合の `set_config.params.network` は次の形式です。

```json
{
  "mode": "STATIC",
  "static_ipv4": {
    "ip_address": "192.168.1.50",
    "prefix_length": 24,
    "gateway": "192.168.1.1",
    "dns": "192.168.1.1"
  }
}
```

親機のIPアドレスは保存しません。親機起動時にUDP `5000`番へ送信される
`{"type":"HELLO_REQUEST"}` を受信すると、送信元エンドポイントを記憶し、通常の `HELLO`、
`HEARTBEAT`、`EVENT` の送信先として使用します。親機が未発見の間に発生したEVENTは送信せず、
ログへ記録します。

GPIO入力は最大10個で、各入力のGPIO番号は外部利用可能GPIOから任意に選択できます。Input Mappingには
業務名を持たせず、Action IDだけを保持します。

設定保存領域は2 MiBイメージの最終4 KiBを使用し、リンカースクリプトからコード領域を除外しています。
Pico W / Pico 2 WのFlash HALアダプタは `src/storage.rs` の共通ストレージ抽象化へ接続されます。

## テスト

入力判定とUDP JSON/ACK相関はホスト上でテストできます。

```sh
cargo test -p uchi-pulse-node
```
