# 親機 データベース設計書

## 1. 目的

本書はUchi-Pulse親機で使用するSQLiteデータベースの基本設計を定義する。

親機はデバイス、家族、Action、Actionによる状態変更、通知設定、イベント履歴を永続管理する。GPIO番号・Input Event・GPIOへのAction ID割当は子機側設定であり、親機DBには含めない。

Actionは状態を必ず変更するものとは限定しない。1つのActionは0件以上の状態変更を持つことができ、状態変更を行わずスマートフォン通知のみを行うActionも定義できる。

---

## 2. 責務境界

### 2.1 子機側で保持する設定

- GPIO番号
- Input Event（`OFF_TO_ON` / `ON_TO_OFF` / `CLICK` / `DOUBLE_CLICK` / `LONG_PRESS`）
- Action ID

子機はAction IDの意味を解釈しない。

### 2.2 親機DBで保持する情報

- Action定義
- 家族と表示名
- Actionの対象範囲・対象家族
- Web表示メッセージ
- Actionによる0件以上の状態変更
- Actionごとのスマートフォン通知有無
- Actionごとの通知メッセージ
- 通知先家族
- 家族ごとのLINE / Slack等の実送信先設定
- イベント履歴

---

## 3. テーブル一覧

| テーブル | 用途 |
|---|---|
| `devices` | 子機マスタ |
| `families` | 家族マスタ |
| `actions` | Action本体 |
| `action_state_changes` | Actionによる状態変更（0..n件） |
| `action_notification_settings` | Action単位の通知有無・通知メッセージ |
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

---

## 5. families

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `family_id` | INTEGER PK | NO | 家族内部ID |
| `display_name` | TEXT | NO | Web等に表示する名前 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

---

## 6. actions

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER PK | NO | 子機から通知される一意のAction ID |
| `action_name` | TEXT | NO | 管理・表示用名称 |
| `target_type` | TEXT | NO | `FAMILY` / `COMMON` |
| `target_family_id` | INTEGER FK | YES | FAMILYの場合の対象家族 |
| `web_message` | TEXT | YES | Web表示メッセージ。不要なActionではNULL可 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

`state_type` / `state_value` は `actions` から分離し、`action_state_changes` で管理する。

- `FAMILY`: 家族対象。`target_family_id` 必須。
- `COMMON`: 共通対象。`target_family_id` はNULL。
- FAMILY Actionは対象家族ごとに別Action IDを登録する。
- 親機は送信元 `device_id` から対象家族を推測しない。

---

## 7. action_state_changes

1つのActionから0件以上の状態変更を定義する。

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER FK | NO | 対象Action |
| `state_type` | TEXT | NO | 変更対象状態 |
| `state_value` | TEXT | NO | 設定する状態値 |

主キーは `(action_id, state_type)` とする。

例:

```text
ご飯通知
  └─ MEAL_NOTICE = ON

食事通知クリア
  ├─ MEAL_NOTICE = OFF
  └─ SNACK_NOTICE = OFF

入室問い合わせ
  └─ 状態変更なし
```

これにより、単一状態変更、複数状態変更、状態変更なしのActionを同じモデルで扱う。

---

## 8. 基本Actionパターン

初期版では以下12種類を基本Actionパターンとする。固定Action IDではない。

| Action | target_type | 状態変更 | デフォルトWebメッセージ |
|---|---|---|---|
| ご飯通知 | `FAMILY` | `MEAL_NOTICE=ON` | `{target}：ご飯です` |
| ご飯通知クリア | `FAMILY` | `MEAL_NOTICE=OFF` | `{target}：ご飯通知を解除しました` |
| おやつ通知 | `FAMILY` | `SNACK_NOTICE=ON` | `{target}：おやつです` |
| おやつ通知クリア | `FAMILY` | `SNACK_NOTICE=OFF` | `{target}：おやつ通知を解除しました` |
| 食事通知クリア | `FAMILY` | `MEAL_NOTICE=OFF`, `SNACK_NOTICE=OFF` | `{target}：食事通知を解除しました` |
| HELP通知 | `FAMILY` | `HELP_NOTICE=ON` | `{target}：HELPです` |
| HELP通知クリア | `FAMILY` | `HELP_NOTICE=OFF` | `{target}：HELP通知を解除しました` |
| 入室OK | `FAMILY` | `ENTRY_PERMISSION=OK` | `{target}：入室OK` |
| 入室NG | `FAMILY` | `ENTRY_PERMISSION=NG` | `{target}：入室NG` |
| 会議中 | `FAMILY` | `ENTRY_PERMISSION=MEETING` | `{target}：会議中` |
| ポスト投函 | `COMMON` | `MAILBOX=ON` | `ポストに投函がありました` |
| ポスト投函解除 | `COMMON` | `MAILBOX=OFF` | `ポストの投函状態を解除しました` |

FAMILYの10種類は必要な家族ごとにActionを作成する。COMMONの2種類は対象家族を持たない。

---

## 9. 通知専用Action / 入室問い合わせ

状態を変更せず、スマートフォン通知だけを目的とするActionを作成できる。

代表用途として、部屋の外にいる家族が、部屋にいる対象家族へ入室してよいか問い合わせるActionを想定する。

- `target_type = FAMILY`
- `target_family_id` = 問い合わせ対象家族
- `action_state_changes` = 0件
- Web表示は不要なら `web_message = NULL`
- スマートフォン通知を有効化できる
- 通知先は1人以上の家族を設定できる
- 通知メッセージはActionごとに設定できる
- 同じ対象家族に対して複数の問い合わせActionを作成できる

例:

```text
Action: 父への入室問い合わせ
状態変更: なし
通知先: 父
通知メッセージ: 今、部屋に入ってもいい？

Action: 父へ「少し話したい」
状態変更: なし
通知先: 父
通知メッセージ: 少し話したいです
```

問い合わせAction自体は `ENTRY_PERMISSION` を変更しない。対象家族が別の `入室OK` / `入室NG` / `会議中` Actionを操作した場合に入室可否状態が変化する。

---

## 10. Action通知設定

### 10.1 action_notification_settings

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER PK/FK | NO | 対象Action |
| `notification_enabled` | INTEGER | NO | 1=通知、0=通知しない |
| `notification_message` | TEXT | YES | Action固有の通知メッセージ |

`notification_message` はActionごとに設定可能とする。`{target}` を含む場合は `families.display_name` で展開できる。

### 10.2 action_notification_targets

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `action_id` | INTEGER FK | NO | 対象Action |
| `family_id` | INTEGER FK | NO | 通知先家族 |

主キーは `(action_id, family_id)` とする。Action対象家族と通知先家族は別概念であり、通知先は複数設定できる。

---

## 11. family_notification_destinations

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `id` | INTEGER PK | NO | 内部ID |
| `family_id` | INTEGER FK | NO | 家族 |
| `notification_type` | TEXT | NO | LINE / Slack等 |
| `destination` | TEXT | NO | サービス上の実送信先情報 |
| `enabled` | INTEGER | NO | 1=有効、0=無効 |

---

## 12. events

| 項目 | 型 | NULL | 内容 |
|---|---|---|---|
| `id` | INTEGER PK | NO | 自動採番履歴ID |
| `received_at` | TEXT | NO | 親機受信日時 |
| `device_id` | TEXT | NO | 送信元子機 |
| `event_id` | TEXT | NO | UDP EVENTの一意ID |
| `payload` | TEXT | NO | 受信JSON本文 |

`(device_id, event_id)` を一意とし、UDP再送による二重登録を防止する。

---

## 13. Action受信時の親機処理

```text
UDP EVENT受信
    ↓
Action ID取得
    ↓
actions取得
    ↓
eventsへ履歴保存
    ↓
action_state_changesを0..n件取得
    ↓
定義された状態をメモリへ反映
    ↓
Web表示が定義されていれば表示情報へ反映
    ↓
通知設定を評価
    ↓
有効ならnotification_messageと通知先家族を通知機能へ渡す
```

外部通知失敗はUDP ACK、履歴保存、状態更新を失敗させない。

---

## 14. 現在状態とDB

通信状態、入室可否、ご飯通知、おやつ通知、HELP通知等の現在値は親機メモリ上で管理し、必要な状態はイベント履歴から復元する。

- `ENTRY_PERMISSION`: `UNSET` / `OK` / `NG` / `MEETING`
- `MEAL_NOTICE`: `ON` / `OFF`
- `SNACK_NOTICE`: `ON` / `OFF`
- `HELP_NOTICE`: `ON` / `OFF`

食事通知クリアでは `MEAL_NOTICE` と `SNACK_NOTICE` を同時に `OFF` にする。

---

## 15. 設計方針まとめ

- Actionと状態変更を分離する。
- 1 Actionは0..n件の状態変更を持てる。
- 状態変更なしの通知専用Actionを許容する。
- FAMILY Actionは対象家族ごとに別Action IDを持つ。
- Action対象家族と通知先家族を別概念として扱う。
- 通知先は複数設定できる。
- 通知メッセージはActionごとに設定できる。
- 入室問い合わせActionは状態を変更しない。
- 食事通知クリアはご飯通知とおやつ通知を同時に解除する。
