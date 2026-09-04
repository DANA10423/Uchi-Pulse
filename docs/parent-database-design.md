# 親機 データベース設計書

## 1. 目的

本書はUchi-Pulse親機で使用するSQLiteデータベースの基本設計を定義する。

親機は、デバイス情報、家族情報、Action定義、通知設定、イベント履歴を永続管理する。GPIO番号・入力エッジ・GPIOへのAction ID割当は子機側設定であり、親機DBのAction定義には含めない。

通信状態および入室可否等の現在値は親機稼働中のメモリ上で管理し、必要な状態はイベント履歴から復元する。

---

## 2. 責務境界

### 2.1 子機側で保持する設定

- GPIO番号
- Edge（`OFF_TO_ON` / `ON_TO_OFF`）
- Action ID

子機はAction IDの意味を解釈しない。

### 2.2 親機DBで保持する情報

- Action定義
- 家族と表示名
- Actionの対象範囲・対象家族
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

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `family_id` | INTEGER PK | NO | 家族内部ID |
| `display_name` | TEXT | NO | Web等に表示する名前 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

Actionには表示名文字列を直接保存せず `family_id` を参照する。「家」「共通」等のダミー家族は作成しない。

---

## 6. actions

### 6.1 項目

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER PK | NO | 子機から通知される一意のAction ID |
| `action_name` | TEXT | NO | 管理・表示用名称 |
| `target_type` | TEXT | NO | `FAMILY` / `COMMON` |
| `target_family_id` | INTEGER FK | YES | FAMILYの場合の対象家族 |
| `web_message` | TEXT | NO | Web表示メッセージ |
| `state_type` | TEXT | NO | 変更対象状態 |
| `state_value` | TEXT | NO | 設定する状態値 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

### 6.2 target_type

- `FAMILY`: 家族対象。`target_family_id` 必須。
- `COMMON`: 共通対象。`target_family_id` はNULL。

ポスト投函系のみ `COMMON` とし、それ以外の基本Actionパターンは `FAMILY` とする。

### 6.3 FAMILY Actionの登録方式

FAMILY Actionは対象家族ごとに別Action IDを登録する。

```text
Action ID 4 = 父 / 入室NG
Action ID 5 = 母 / 入室NG
```

設計ルール:

- 1つのAction IDが持つ `target_family_id` は最大1件。
- 同じAction内容でも対象家族が異なれば別Action IDとする。
- Action ID数が家族数に応じて増えることを許容する。
- 親機は送信元 `device_id` から対象家族を推測しない。
- EVENTの `action_id` から取得したActionレコードの `target_family_id` を対象者として使用する。

### 6.4 基本Actionパターン

初期版では以下7種類を基本パターンとして定義する。これらは固定Action IDではない。

| Action | target_type | state_type | state_value | デフォルト `web_message` |
|---|---|---|---|---|
| ご飯通知 | `FAMILY` | `MEAL_NOTICE` | `ON` | `{target}：ご飯です` |
| ご飯通知クリア | `FAMILY` | `MEAL_NOTICE` | `OFF` | `{target}：ご飯通知を解除しました` |
| 入室OK | `FAMILY` | `ENTRY_PERMISSION` | `OK` | `{target}：入室OK` |
| 入室NG | `FAMILY` | `ENTRY_PERMISSION` | `NG` | `{target}：入室NG` |
| 会議中 | `FAMILY` | `ENTRY_PERMISSION` | `MEETING` | `{target}：会議中` |
| ポスト投函 | `COMMON` | `MAILBOX` | `ON` | `ポストに投函がありました` |
| ポスト投函解除 | `COMMON` | `MAILBOX` | `OFF` | `ポストの投函状態を解除しました` |

FAMILYの5種類は必要な家族ごとにActionレコードを登録する。COMMONの2種類は `target_family_id = NULL` とする。

会議中の解除専用Actionは設けず、対象家族の入室OK Actionで状態を変更する。

`{target}` は `target_family_id` に対応する `families.display_name` で親機が展開する。

---

## 7. Action通知設定

スマートフォン通知設定はAction定義から分離する。Actionの `target_family_id` と通知先家族は別概念である。

### 7.1 action_notification_settings

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER PK/FK | NO | 対象Action |
| `notification_enabled` | INTEGER | NO | 1=通知、0=通知しない |

### 7.2 action_notification_targets

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER FK | NO | 対象Action |
| `family_id` | INTEGER FK | NO | 通知先家族 |

主キーは `(action_id, family_id)` とする。

---

## 8. family_notification_destinations

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

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `id` | INTEGER PK | NO | 自動採番履歴ID |
| `received_at` | TEXT | NO | 親機受信日時 |
| `device_id` | TEXT | NO | 送信元子機 |
| `event_id` | TEXT | NO | UDP EVENTの一意ID |
| `payload` | TEXT | NO | 受信JSON本文 |

`(device_id, event_id)` を一意とし、UDP再送による二重登録を防止する。

Action IDはUDP EVENTの `action_id` として受信する。

---

## 10. Action受信時の親機処理

```text
UDP EVENT受信
    ↓
Action ID取得
    ↓
actionsをaction_idで1件取得
    ├─ target_type
    ├─ target_family_id
    ├─ web_message
    ├─ state_type
    └─ state_value
    ↓
FAMILYの場合はtarget_family_idを対象者として使用
    ↓
{target} を家族表示名で展開
    ↓
eventsへ履歴保存
    ↓
メモリ上の必要な状態を更新
    ↓
通知設定を評価
    ↓
必要な場合は通知機能へ通知要求
```

---

## 11. 現在状態とDB

通信状態や入室可否等の現在値そのものは専用DB状態として保持せず、親機メモリ上で管理する。

親機起動時は `devices` の有効な子機から `DeviceState` を生成し、通信状態を `INITIAL_WAIT` とする。入室可否は `events` に保存された最新の入室可否Action履歴から復元する。

`DeviceState` の基本情報:

- `device_id`
- `last_seen_at`
- `status`: `INITIAL_WAIT` / `ONLINE` / `OFFLINE`
- `room_access_status`: `UNSET` / `OK` / `NG` / `MEETING`

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --正常受信--> ONLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

通信状態が変化しても入室可否は自動変更しない。

---

## 12. 設計方針まとめ

- 子機のGPIO・Edge・Action ID割当と親機のAction定義を混在させない。
- 子機はAction IDの意味を解釈しない。
- 7種類は基本Actionパターンであり固定Action IDではない。
- FAMILY Actionは対象家族ごとに別Action IDを持つ。
- 1つのAction IDには対象家族を最大1人だけ持たせる。
- 親機はAction IDから対象家族を直接解決し、device_idから対象者を推測しない。
- Web表示メッセージはAction定義に持ち、`{target}` を家族表示名で展開できる。
- スマートフォン通知設定はAction定義から分離する。
- Action対象家族と通知先家族を別概念として扱う。
- 現在状態はメモリで管理し、必要な状態はイベント履歴から復元する。