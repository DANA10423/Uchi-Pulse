# uchi-pulse-node

Uchi Pulse の Raspberry Pi Pico W / Pico 2 W 向け子機ファームウェアです。
PHASE 6/7/8では、GPIO入力から共通Input Eventを判定し、設定されたAction IDを正式なUDP EVENTとして親機へ送信します。また、USB CDCで子機設定を取得・保存できます。

- デバウンス付きGPIO入力変化検出
- `OFF_TO_ON` / `ON_TO_OFF` / `CLICK` / `DOUBLE_CLICK` / `LONG_PRESS`
- `GPIO + Input Event -> Action ID` のデータ駆動マッピング
- ホスト上でテスト可能な入力ロジックとUDPプロトコル処理
- `boot_id + sequence` によるEVENT識別子
- EVENT ACK待機（60秒）と同一EVENTの最大3回再送
- 180秒間隔のHEARTBEAT
- USB CDCのLF区切りJSON（`get_config` / `set_config` / `factory_reset` / `reboot`）
- 設定のvalidation、Flash保存、次回起動時の復元

UDPは正式な共通wire formatを使用します。GPIO番号、Input Event、Actionの業務的意味はUDPへ含めません。

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

Wi-Fi・Hub接続先は現時点では `src/config.rs` のコンパイル時設定です。GPIO・Input Mapping・
ジェスチャー時間・UDP送信タイミングはUSB CDCから変更でき、最終4 KiBの設定保存領域へ保存されます。
保存設定は次回起動時に反映されます。Input Mappingには業務名を持たせず、Action IDだけを保持します。

設定保存領域は2 MiBイメージの最終4 KiBを使用し、リンカースクリプトからコード領域を除外しています。
Pico W / Pico 2 WのFlash HALアダプタは `src/storage.rs` の共通ストレージ抽象化へ接続されます。

## テスト

入力判定とUDP JSON/ACK相関はホスト上でテストできます。

```sh
cargo test -p uchi-pulse-node
```
