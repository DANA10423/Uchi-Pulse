# 家庭内ゆるコミュニケーションツール プログラム設計書

## 1. 文書概要

本システムは、家庭内で発生する簡単な連絡・状態共有を物理入力中心の簡単な操作で行う「家庭内ゆるコミュニケーションツール」である。

- 子機: Raspberry Pi Pico W / Pico 2 W
- 親機: Raspberry Pi Zero W / Zero 2 W
- 通常通信: 家庭内Wi-Fi / UDP
- 設定・保守: USB CDC

子機はAction IDの意味を解釈せず、親機がAction IDを解釈して対象者、状態変更、Web表示、通知を管理する。

---

## 2. 子機設計

### 2.1 入力イベント

ボタン、スイッチ、ポスト投函検出センサー等を機能別に区別せずGPIO入力として扱う。

GPIO入力から生成するイベントを `Input Event` と呼ぶ。初期版では以下を扱う。

- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

`OFF_TO_ON` / `ON_TO_OFF` はGPIOの状態変化そのものを表す。

`CLICK` / `DOUBLE_CLICK` / `LONG_PRESS` はボタン操作を子機側で判定して生成する論理イベントである。

各GPIOの各Input Eventに独立してAction IDを割り当てられる。

操作判定時間は子機設定として保持する。

| 設定項目 | デフォルト | 内容 |
|---|---:|---|
| `double_click_interval_ms` | 400 ms | 1回目のクリック後、2回目のクリックを待つ時間 |
| `long_press_threshold_ms` | 1000 ms | 押下継続を長押しと判定する時間 |

両設定値はUSB CDCの `get_config` / `set_config` で取得・変更可能とし、永続保存する。

`CLICK` は、1回目のクリック後 `double_click_interval_ms` 内に2回目が成立しなかった時点で確定する。2回目が成立した場合は `DOUBLE_CLICK` とし、同一操作から `CLICK` と `DOUBLE_CLICK` を重複発生させない。

`LONG_PRESS` は押下状態が `long_press_threshold_ms` 以上継続した場合に成立する。

### 2.2 子機CDC設定

Action入力割当:

| 項目 | 内容 |
|---|---|
| `gpio` | 監視対象GPIO番号 |
| `input_event` | `OFF_TO_ON` / `ON_TO_OFF` / `CLICK` / `DOUBLE_CLICK` / `LONG_PRESS` |
| `action_id` | 発生時に送信するAction ID |

例:

```text
GPIO 5 / CLICK        / Action ID 10
GPIO 5 / DOUBLE_CLICK / Action ID 11
GPIO 5 / LONG_PRESS   / Action ID 12
GPIO 8 / OFF_TO_ON    / Action ID 20
GPIO 8 / ON_TO_OFF    / Action ID 21
```

操作判定設定例:

```json
{
  "double_click_interval_ms": 400,
  "long_press_threshold_ms": 1000
}
```

子機はAction名、対象家族、Web表示メッセージ、状態変更内容、通知設定、LINE / Slack等の送信先を保持・解釈しない。

---

## 3. 親機Action設計

### 3.1 Action定義

親機DBのAction定義は以下を持つ。

- `action_id`
- `action_name`
- `target_type`
- `target_family_id`
- `web_message`
- `state_type`
- `state_value`
- `enabled`

- `FAMILY`: 家族対象。`target_family_id` 必須。
- `COMMON`: 共通対象。`target_family_id` はNULL。

### 3.2 Action IDと対象家族

家族対象Actionは対象家族ごとに別Action IDを登録する。

同じAction内容でも対象家族が異なる場合は別Actionとして扱う。

1つのAction IDが持つ `target_family_id` は最大1件とする。

親機は受信したAction IDからAction内容と対象家族を直接解決し、送信元 `device_id` からAction対象者を推測しない。

### 3.3 基本Actionパターン

初期版では次の7種類を基本Actionパターンとして用意する。

| Action | target_type | state_type | state_value | デフォルトWebメッセージ |
|---|---|---|---|---|
| ご飯通知 | `FAMILY` | `MEAL_NOTICE` | `ON` | `{target}：ご飯です` |
| ご飯通知クリア | `FAMILY` | `MEAL_NOTICE` | `OFF` | `{target}：ご飯通知を解除しました` |
| 入室OK | `FAMILY` | `ENTRY_PERMISSION` | `OK` | `{target}：入室OK` |
| 入室NG | `FAMILY` | `ENTRY_PERMISSION` | `NG` | `{target}：入室NG` |
| 会議中 | `FAMILY` | `ENTRY_PERMISSION` | `MEETING` | `{target}：会議中` |
| ポスト投函 | `COMMON` | `MAILBOX` | `ON` | `ポストに投函がありました` |
| ポスト投函解除 | `COMMON` | `MAILBOX` | `OFF` | `ポストの投函状態を解除しました` |

この7種類は固定Action IDではない。

FAMILYの5種類は必要な家族ごとにAction定義を作成する。COMMONの2種類は対象家族を持たない。

「会議中」の解除専用Actionは設けず、対象家族の `入室OK` Actionを使用する。

### 3.4 対象家族とメッセージ

`FAMILY` Actionでは `target_family_id` を必須とし、家族マスタの `display_name` を参照する。

`COMMON` Actionでは `target_family_id = NULL` とする。

`web_message` 内の `{target}` は親機が対象家族の `display_name` で展開する。

### 3.5 通知設定

スマートフォン通知はAction定義から分離する。

- 対象Action ID
- 通知有無
- 通知先家族

Actionの対象家族と通知先家族は別概念とする。

---

## 4. 子機・親機の責務境界

| 情報・処理 | 子機 | 親機 |
|---|---|---|
| GPIO監視 | ○ | × |
| Input Event生成 | ○ | × |
| CLICK / DOUBLE_CLICK / LONG_PRESS判定 | ○ | × |
| 操作判定時間の設定・保持 | ○ | × |
| GPIO+Input EventへのAction ID割当 | ○ | × |
| Action IDの意味解釈 | × | ○ |
| `target_type` / 対象家族 | × | ○ |
| Web表示メッセージ | × | ○ |
| 状態変更 | × | ○ |
| EVENT履歴 | × | ○ |
| 通知有無・通知先家族 | × | ○ |

---

## 5. 処理概要

```text
[子機]
GPIO
  ↓
Input Event判定
  ├─ OFF_TO_ON
  ├─ ON_TO_OFF
  ├─ CLICK
  ├─ DOUBLE_CLICK
  └─ LONG_PRESS
  ↓
Action ID
  ↓ UDP EVENT

---------------- 責務境界 ----------------

[親機]
Action ID
  ↓
Action定義参照
  ├─ target_type
  ├─ target_family_id
  ├─ web_message
  ├─ state_type
  └─ state_value
  ↓
履歴保存・メモリ状態更新・Web表示
  ↓
通知設定参照
  ↓
必要な場合は通知機能へ通知要求
```

---

## 6. 関連仕様

- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/parent-overview-design.md`
- `docs/parent-web-status-design.md`
