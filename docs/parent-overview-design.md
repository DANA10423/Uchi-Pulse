# Uchi-Pulse 親機 概要設計書

## 1. 目的
親機（Raspberry Pi Zero W / Zero 2 W）は、子機とのUDP通信、Action解釈、状態管理、履歴保存、Web UI、スマートフォン通知を担当する中央管理装置である。

子機GPIO監視および `GPIO + Input Event + Action ID` 割当は子機側の責務とする。

## 2. 親機の主要責務
- 子機管理
- UDP HELLO / HEARTBEAT / EVENT受信
- 子機通信状態管理
- Action ID解釈
- Action対象解決（特定家族 / 有効家族全員 / 共通）
- Action状態変更 0..n件の適用
- EVENT履歴保存
- Web表示情報提供
- Action通知設定評価
- 通知機能への通知要求
- 親機USB CDC設定・保守

## 3. 子機との責務境界
Input Event:
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

----------------

親機
Action ID
    ↓
Action本体
    ↓
target_type解決
    ↓
状態変更 0..n
    ↓
Web / 通知
```

親機はGPIO番号やInput Eventを解釈しない。子機はAction対象も解釈しない。

## 4. Action管理
Actionは親機DBで管理する。

### 4.1 Action本体
- `action_id`
- `action_name`
- `target_type`
- `target_family_id`
- `web_message`（任意）
- `enabled`

`target_type` は次の3種類とする。

- `FAMILY`: 特定家族1人。`target_family_id` 必須。
- `ALL_FAMILIES`: Action実行時点の有効家族全員。`target_family_id = NULL`。
- `COMMON`: 家族に属さない共通状態。`target_family_id = NULL`。

`ALL_FAMILIES` は実行時点で `families.enabled = 1` の家族全員へ展開する。対象家族一覧をActionへ固定保持しないため、家族追加・無効化時に全員向けActionの再設定は不要とする。

状態変更は `action_state_changes` に分離し、1 Actionに0..n件定義する。

### 4.2 基本Actionパターン
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

`食事通知クリア` は `MEAL_NOTICE=OFF` と `SNACK_NOTICE=OFF` を同時に適用する。

対象種別の基本方針:
- ご飯・おやつ・食事通知クリア・HELP系: `FAMILY` または `ALL_FAMILIES`
- 入室OK / 入室NG / 会議中: `FAMILY`
- ポスト投函 / ポスト投函解除: `COMMON`

FAMILY Actionは対象家族ごとに別Action IDとする。ALL_FAMILIES Actionは1つのAction IDで有効家族全員を対象にする。COMMON Actionは対象家族を持たない。

## 5. 通知専用Action
状態変更を持たないActionを許容する。

入室問い合わせは代表的な通知専用Actionであり、以下とする。
- `target_type = FAMILY`
- `target_family_id` = 問い合わせ対象家族
- `action_state_changes` = 0件
- `web_message` = NULL可
- 通知有効化可能
- 通知先家族は複数設定可能
- 通知メッセージはActionごとに設定可能
- 同一対象家族に複数作成可能

入室問い合わせ自体では `ENTRY_PERMISSION` を変更しない。

## 6. 通知管理
スマートフォン通知設定はAction本体から分離する。

- `notification_enabled`
- `notification_message`
- 通知先家族 0..n

Action対象家族と通知先家族は別概念とする。`ALL_FAMILIES` はAction対象を意味し、通知先全員を意味しない。通知先はActionごとに独立して設定する。

## 7. 状態管理
親機稼働中の現在状態はメモリ上で管理する。

- `device_id`
- `last_seen_at`
- 通信状態: `INITIAL_WAIT` / `ONLINE` / `OFFLINE`
- 入室可否: `UNSET` / `OK` / `NG` / `MEETING`
- `MEAL_NOTICE`: `ON` / `OFF`
- `SNACK_NOTICE`: `ON` / `OFF`
- `HELP_NOTICE`: `ON` / `OFF`

親機起動時は有効デバイスを `INITIAL_WAIT` で初期化し、復元対象状態はEVENT履歴から復元する。

`ALL_FAMILIES` Actionによる家族単位の状態変更は、Action実行時点の有効家族全員へ個別に反映する。

## 8. Web UI
一覧の常時表示は以下を基本とする。

家族用端末:
- 表示名
- 通信状態
- 入室可否

設備用端末:
- 表示名
- 通信状態

ご飯・おやつ・HELP状態を一覧の常時列として追加しない。Action表示等で扱う。

## 9. CDC
親機CDCではAction本体、対象種別、状態変更、家族、Web表示、通知設定を管理対象とする。

`target_type` は `FAMILY` / `ALL_FAMILIES` / `COMMON` を扱い、`target_family_id` との組み合わせを検証する。

子機の `GPIO + Input Event + Action ID` は子機CDCの責務とする。

## 10. 関連仕様
- `docs/home_yuru_communication_design.md`
- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/parent-web-status-design.md`
