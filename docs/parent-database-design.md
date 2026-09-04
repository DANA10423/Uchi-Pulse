# 親機 データベース設計書

## 1. 目的

本書はUchi-Pulse親機で使用するSQLiteデータベースの基本設計を定義する。

親機は、デバイス情報、家族情報、Action定義、通知設定、イベント履歴を永続管理する。GPIO番号・入力エッジ・GPIOへのAction ID割当は子機側設定であり、親機DBのAction定義には含めない。

通信状態および入室可否等の現在値は親機稼働中のメモリ上で管理し、必要な状態はイベント履歴から復元する。

---

## 2. 責務境界

### 2.1 子機側で保持する設定

子機はUSB CDCで次の入力割当を設定する。

- GPIO番号
- エッジ（`OFF_TO_ON` / `ON_TO_OFF`）
- Action ID

子機はAction IDの意味を解釈しない。

### 2.2 親機DBで保持する情報

親機DBはAction IDの意味と、そのActionに関連する設定を管理する。

- Action定義
- 家族と表示名
- Actionの対象家族
- Web表示メッセージ
- 状態変更の意味
- Actionごとのスマートフォン通知有無
- 通知先家族
- 家族ごとのLINE / Slack等の実送信先設定
- イベント履歴

外部サービスへの実際の通知送信処理は通知機能が担当し、DB層の責務には含めない。

---

## 3. テーブル一覧

| テーブル | 用途 |
|---|---|
| `devices` | 子機マスタ |
| `families` | 家族マスタ |
| `actions` | Action定義 |
| `action_notification_settings` | Action単位の通知有無 |
| `action_notification_targets` | Actionの通知先家族 |
| `family_notification_destinations` | 家族ごとの外部通知送信先設定 |
| `events` | UDP EVENT受信履歴 |

---

## 4. devices

子機の固定・準固定情報を管理する。

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `device_id` | TEXT PK | NO | 子機ID |
| `name` | TEXT | NO | 表示名 |
| `device_type` | TEXT | NO | 利用上の分類 |
| `registered_at` | TEXT | NO | 登録日時 |
| `updated_at` | TEXT | NO | 更新日時 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

`device_type` は表示・管理上の分類であり、GPIOやAction処理を分岐させるためには使用しない。

---

## 5. families

### 5.1 目的

Actionの対象者およびスマートフォン通知先として参照する家族を管理する。

### 5.2 項目

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `family_id` | INTEGER PK | NO | 家族内部ID |
| `display_name` | TEXT | NO | Web等に表示する名前 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

Actionには表示名文字列を直接保存せず `family_id` を保持し、表示時に `display_name` を参照する。

「家」「共通」等のダミー家族は作成しない。

---

## 6. actions

### 6.1 目的

子機から通知されたAction IDを親機が解釈するための定義を管理する。

### 6.2 初期Action

- ご飯通知
- ご飯通知クリア
- 入室OK
- 入室NG
- 会議中
- ポスト投函
- ポスト投函解除

会議中の解除専用Actionは設けず、入室OKで状態を変更する。

### 6.3 項目

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER PK | NO | 子機から通知されるAction ID |
| `action_type` | TEXT | NO | Action種別 |
| `action_name` | TEXT | NO | 管理・表示用名称 |
| `target_family_id` | INTEGER FK | YES | 対象家族。Action種別により必須 |
| `web_message` | TEXT | NO | Web表示メッセージ |
| `state_type` | TEXT | NO | 変更対象状態 |
| `state_value` | TEXT | NO | 設定する状態値 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

### 6.4 対象者ルール

対象家族が必要かどうかは `action_type` によって判定する。

- 家族対象として定義されたAction種別では `target_family_id` を必須とする。
- 共通対象として定義されたAction種別では `target_family_id` を使用しない（NULL）。
- 現時点でポスト投函系は共通対象として扱う。
- その他の各Action種別の家族対象／共通対象の分類はAction種別定義として管理する。

DBでは `target_family_id` のNULLを許容し、登録・変更時にAction種別のルールに従って整合性を検証する。

### 6.5 Web表示メッセージ

`web_message` にはActionごとのデフォルト値を用意する。親機へのUSB CDC設定により変更可能とする。

---

## 7. Action通知設定

### 7.1 基本方針

スマートフォン通知設定はAction定義から分離する。

Actionは「何が起きたか、誰についてのActionか、状態をどう変えるか」を定義する。通知設定は「そのActionをスマートフォンへ通知するか、誰へ通知するか」を定義する。

Actionの `target_family_id` と通知先家族は別概念である。

### 7.2 action_notification_settings

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER PK/FK | NO | 対象Action |
| `notification_enabled` | INTEGER | NO | 1=通知、0=通知しない |

### 7.3 action_notification_targets

通知対象が複数家族となることを許容する。

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER FK | NO | 対象Action |
| `family_id` | INTEGER FK | NO | 通知先家族 |

主キーは `(action_id, family_id)` とする。

---

## 8. family_notification_destinations

### 8.1 目的

家族ごとの外部通知先設定を管理する。LINE / Slack等のサービス固有送信先情報をAction定義から分離する。

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `id` | INTEGER PK | NO | 内部ID |
| `family_id` | INTEGER FK | NO | 家族 |
| `notification_type` | TEXT | NO | LINE / Slack等 |
| `destination` | TEXT | NO | サービス上の実送信先情報 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

通知サービス固有の認証情報・秘密情報の保存方式は通知機能の詳細設計で別途定義する。

---

## 9. events

子機からUDPで受信したEVENTを履歴として保存する。

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `id` | INTEGER PK | NO | 自動採番履歴ID |
| `received_at` | TEXT | NO | 親機受信日時 |
| `device_id` | TEXT | NO | 送信元子機 |
| `event_id` | TEXT | NO | UDP EVENTの一意ID |
| `payload` | TEXT | NO | 受信JSON本文 |

`(device_id, event_id)` を一意とし、UDP再送による二重登録を防止する。

Action IDはUDP EVENTの `action_id` として受信し、受信JSON本文に含まれる。EVENT通信形式の詳細は `docs/parent_child_udp_communication_spec.md` で定義する。

---

## 10. Action受信時の親機処理

```text
UDP EVENT受信
    ↓
Action ID取得
    ↓
actions参照
    ├─ Action種別
    ├─ 対象家族（必要な種別のみ）
    ├─ Web表示メッセージ
    └─ 状態変更内容
    ↓
eventsへ履歴保存
    ↓
メモリ上の必要な状態を更新
    ↓
action_notification_settings参照
    ↓
通知ONの場合
    ↓
action_notification_targetsから通知先家族取得
    ↓
通知機能へ通知要求を引き渡す
```

実際のLINE / Slack等の送信先解決および外部サービスへの送信処理は通知機能の責務とする。

---

## 11. 現在状態とDB

通信状態や入室可否等の現在値そのものは専用DB状態として保持せず、親機メモリ上で管理する。

親機起動時は `devices` の有効な子機から `DeviceState` を生成し、通信状態を `INITIAL_WAIT` とする。入室可否は `events` に保存された最新の入室可否Action履歴から復元する。

`DeviceState` の基本情報:

- `device_id`
- `last_seen_at`
- `status`: `INITIAL_WAIT` / `ONLINE` / `OFFLINE`
- `room_access_status`: `UNSET` / `OK` / `NG` / `MEETING`

状態遷移は次を基本とする。

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --正常受信--> ONLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

正常受信とは、通信仕様上有効なHELLO / HEARTBEAT / EVENT等を正常に解析・処理できた場合をいう。

通信状態が変化しても入室可否は自動変更しない。

---

## 12. 設計方針まとめ

- 子機のGPIO・エッジ・Action ID割当と、親機のAction定義を混在させない。
- 子機はAction IDの意味を解釈しない。
- Action定義は親機DBで管理する。
- Action対象者の表示名は家族マスタを参照する。
- Action種別によって対象家族の必須／不要を判定する。
- 共通Action用のダミー家族は作らない。
- Web表示メッセージはAction定義に持ち、デフォルト値を用意して親機CDCで変更可能とする。
- スマートフォン通知設定はAction定義から分離する。
- Action対象者と通知先家族を別概念として扱う。
- LINE / Slack等の実送信は通知機能へ分離する。
- イベント履歴は `events` に保存する。
- 現在状態はメモリで管理し、必要な状態はイベント履歴から復元する。