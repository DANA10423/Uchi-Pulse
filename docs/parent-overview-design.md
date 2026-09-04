# Uchi-Pulse 親機 概要設計書

## 1. 目的

親機（Raspberry Pi Zero W / Zero 2 W）は、子機とのUDP通信、Action解釈、状態管理、履歴保存、Web UI、スマートフォン通知を担当する中央管理装置である。

本書では親機の責務を定義する。子機GPIO監視および `GPIO + Input Event + Action ID` 割当は子機側の責務であり、親機内部処理とは分離する。

---

## 2. 親機の主要責務

- 子機管理
- UDP HELLO / HEARTBEAT / EVENT受信
- 子機通信状態管理
- Action IDの解釈
- Action対象家族の解決
- Actionによる現在状態更新
- イベント履歴保存
- Web表示情報提供
- Action通知設定の評価
- 通知機能への通知要求引き渡し
- 親機自身のUSB CDC設定・保守

---

## 3. 子機との責務境界

子機はGPIO入力からInput Eventを生成し、子機CDCで設定されたAction IDをUDP EVENTとして親機へ送る。

Input Eventの初期定義:

- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

```text
子機
GPIO + Input Event
    ↓
Action ID
    ↓ UDP EVENT

--------------------------

親機
Action ID
    ↓
Action定義を参照
    ↓
状態・表示・通知判断
```

Input Eventの検出・判定は子機側で完了する。親機はGPIO番号やInput EventをAction定義として保持・解釈しない。

---

## 4. Action管理

Action定義は親機DBで管理する。

基本Actionパターンは11種類とする。

- ご飯通知
- ご飯通知クリア
- おやつ通知
- おやつ通知クリア
- HELP通知
- HELP通知クリア
- 入室OK
- 入室NG
- 会議中
- ポスト投函
- ポスト投函解除

ご飯通知、おやつ通知、HELP通知はそれぞれ独立した `ON` / `OFF` 状態として扱う。

Actionには、Action ID、名称、`target_type`、`target_family_id`、Web表示メッセージ、状態変更内容を定義する。

- `FAMILY`: 対象家族を1人持つ。
- `COMMON`: 対象家族を持たない。

FAMILY Actionは対象家族ごとに別Action IDとして登録する。1つのAction IDに複数家族は紐づけない。

親機は受信したAction IDから対象家族を直接解決し、送信元device_idから対象者を推測しない。

---

## 5. 通知管理

スマートフォン通知設定はAction定義から分離する。

通知設定ではActionごとの通知有無と通知先家族を管理する。Action対象者と通知先は別概念である。

LINE / Slack等の外部サービスへの実送信処理は通知機能として分離する。

---

## 6. 状態管理

親機稼働中の現在状態はメモリ上で管理する。

- `device_id`
- `last_seen_at`
- 通信状態: `INITIAL_WAIT` / `ONLINE` / `OFFLINE`
- 入室可否: `UNSET` / `OK` / `NG` / `MEETING`
- ご飯通知: `ON` / `OFF`
- おやつ通知: `ON` / `OFF`
- HELP通知: `ON` / `OFF`

親機起動時はDBの有効デバイスを読み込み、通信状態を `INITIAL_WAIT` とする。復元対象状態はイベント履歴から最新値を復元する。

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --正常受信--> ONLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

---

## 7. データベース

SQLiteを使用する。

主な永続情報:

- デバイスマスタ
- 家族マスタ
- Action定義
- Action通知設定
- 通知先家族
- 家族の外部通知先設定
- EVENT履歴

通信状態等の現在値はDBの専用状態として保存しない。

---

## 8. Web UI

親機Web UIはDBのマスタ情報とメモリ上の現在状態を組み合わせて表示する。

ActionのWeb表示メッセージは親機DBのAction定義から取得する。FAMILY Actionの対象家族表示名はActionの `target_family_id` から家族マスタを参照して取得する。

通信状態と入室可否は別項目として表示する。入室可否は家族用端末にのみ表示する。

---

## 9. CDC

親機のUSB CDCは親機側設定・保守を行うために使用する。

子機の `GPIO + Input Event + Action ID` 割当は子機CDCで設定し、親機CDCのAction定義とは分離する。

---

## 10. 関連仕様

- `docs/home_yuru_communication_design.md`
- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/parent-web-status-design.md`
