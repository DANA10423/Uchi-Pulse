# Uchi-Pulse システム概要

## 1. 概要
Uchi-Pulseは家庭内の簡単な連絡・状態共有を、物理入力、Web UI、スマートフォン通知によって行うシステムである。

子機はGPIO入力からInput Eventを判定してAction IDへ変換し、親機はAction IDを解釈して状態管理、履歴、Web表示、外部通知を行う。

## 2. 構成
- 子機: Raspberry Pi Pico W / Pico 2 W
- 親機: Raspberry Pi Zero W / Zero 2 W
- 通常通信: 家庭内Wi-Fi / UDP
- 設定・保守: USB CDC
- 親機DB: SQLite
- 利用者UI: 親機Web UI
- 外部通知: LINE / Slack等

## 3. 子機の責務
Input Event:
- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

子機設定:
```text
GPIO + Input Event + Action ID
```

- `double_click_interval_ms`: 400ms
- `long_press_threshold_ms`: 1000ms

子機はAction IDの意味を解釈しない。

## 4. 親機の責務
親機が管理する情報:
- デバイス
- 家族
- Action本体
- Action対象家族
- Action状態変更 0..n
- Web表示メッセージ
- 現在状態
- EVENT履歴
- Action通知設定
- 通知メッセージ
- 通知先家族
- LINE / Slack等の実送信先

## 5. Actionモデル
Action本体と状態変更を分離する。

```text
Action
 ├─ target_family_id 0..1
 ├─ state changes 0..n
 ├─ web_message 任意
 └─ notification 任意
```

初期版の基本Actionパターンは12種類とする。

- ご飯通知
- ご飯通知クリア
- おやつ通知
- おやつ通知クリア
- 食事通知クリア
- HELP通知
- HELP通知クリア
- 入室OK
- 入室NG
- 会議中
- ポスト投函
- ポスト投函解除

`食事通知クリア` は `MEAL_NOTICE` と `SNACK_NOTICE` を同時にOFFへ変更する。

FAMILY Actionは対象家族ごとに別Action IDとし、COMMON Actionは対象家族を持たない。

## 6. 通知専用Action
状態を変更しない通知専用Actionを作成可能とする。

入室問い合わせでは:
- 対象家族を1人持つ
- 状態変更は0件
- 入室可否は変更しない
- Web表示は任意
- 通知メッセージをActionごとに設定可能
- 通知先家族は複数設定可能
- 同一対象家族向けに複数Actionを作成可能

## 7. 処理全体
```text
[子機]
GPIO
 ↓
Input Event
 ↓
Action ID
 ↓ UDP EVENT

==============

[親機]
Action ID
 ↓
Action本体
 ↓
状態変更 0..n
 ↓
履歴 / Web
 ↓
通知設定
 ├─ notification_message
 └─ 通知先家族 0..n
 ↓
通知機能
 ↓
LINE / Slack等
```

## 8. 設計原則
- 子機と親機の責務を混在させない。
- 子機はAction IDを不透明な識別子として扱う。
- Actionと状態変更を分離する。
- 1 Actionは0..n件の状態変更を持てる。
- 状態変更なしの通知専用Actionを許容する。
- FAMILY Actionは対象家族ごとに別Action IDを持つ。
- Action対象家族と通知先家族は別概念とする。
- Webメッセージと通知メッセージを分離する。
- 現在状態は親機メモリ、履歴は親機DBで管理する。

## 9. 関連文書
- `docs/home_yuru_communication_design.md`
- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/parent-web-status-design.md`
