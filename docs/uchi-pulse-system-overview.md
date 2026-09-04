# Uchi-Pulse システム概要

## 1. 概要

Uchi-Pulseは家庭内の簡単な連絡・状態共有を、物理入力、Web UI、スマートフォン通知によって行うシステムである。

子機はGPIO入力をAction IDへ変換して親機へ送信し、親機はAction IDの意味を解釈して状態管理、履歴、Web表示、外部通知を行う。

---

## 2. 構成

- 子機: Raspberry Pi Pico W / Pico 2 W
- 親機: Raspberry Pi Zero W / Zero 2 W
- 通常通信: 家庭内Wi-Fi / UDP
- 設定・保守: USB CDC
- 親機DB: SQLite
- 利用者UI: 親機Web UI
- 外部通知: LINE / Slack等を想定

---

## 3. 子機の責務

子機は汎用GPIOノードとして動作する。

GPIO入力はボタン・ポストセンサー等の用途で処理を分けず、デジタル入力として統一する。

Action発生条件はGPIOのエッジとする。

- OFF→ON
- ON→OFF

子機CDCでは各入力について次を設定する。

```text
GPIO + Edge + Action ID
```

子機はAction IDの意味を解釈しない。

---

## 4. 親機の責務

親機は受信したAction IDを親機DBのAction定義から解釈する。

親機が管理する情報:

- デバイス
- 家族と表示名
- Action定義
- Action対象家族
- Web表示メッセージ
- 現在状態
- EVENT履歴
- Action通知設定
- 通知先家族
- LINE / Slack等の実送信先

スマートフォン通知設定はAction定義から分離する。

---

## 5. 初期Action

- ご飯通知
- ご飯通知クリア
- 入室OK
- 入室NG
- 会議中
- ポスト投函
- ポスト投函解除

会議中状態からの解除は入室OKを使用する。

Action種別によって対象家族の要否を判定する。ご飯通知系・入室系は家族対象、ポスト投函系は共通対象とする。

---

## 6. 処理全体

```text
[子機]
GPIO
 ↓
Edge
 ↓
Action ID
 ↓ UDP EVENT

==============

[親機]
Action ID
 ↓
Action定義
 ├─ 種別
 ├─ 対象家族
 ├─ Webメッセージ
 └─ 状態変更
 ↓
履歴・現在状態・Web
 ↓
通知設定
 ├─ 通知有無
 └─ 通知先家族
 ↓
LINE / Slack等
```

---

## 7. 設計原則

- 子機と親機の設定責務を混在させない。
- 子機は物理入力からAction IDを生成することに集中する。
- Actionの意味は親機で一元管理する。
- 対象者表示名は家族マスタで管理する。
- 共通Action用のダミー家族は作らない。
- Web表示メッセージは親機Action定義で管理し、デフォルトを持たせて親機CDCで変更可能とする。
- スマートフォン通知はAction定義から分離する。
- Action対象者と通知先を別概念とする。
- 現在状態は親機メモリ、履歴は親機DBで管理する。

---

## 8. 関連文書

- `docs/home_yuru_communication_design.md`
- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/parent-web-status-design.md`