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

ボタン、スイッチ、ポスト投函検出センサー等を機能別に区別せずGPIOデジタル入力として扱う。

Action発生条件は次のEdgeとする。

- `OFF_TO_ON`
- `ON_TO_OFF`

各GPIOの各Edgeに独立してAction IDを割り当てられる。

### 2.2 子機CDC設定

| 項目 | 内容 |
|---|---|
| `gpio` | 監視対象GPIO番号 |
| `edge` | `OFF_TO_ON` / `ON_TO_OFF` |
| `action_id` | 発生時に送信するAction ID |

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

Actionの対象範囲は `target_type` で明示する。

- `FAMILY`: 家族対象。`target_family_id` 必須。
- `COMMON`: 共通対象。`target_family_id` はNULL。

### 3.2 Action IDと対象家族

家族対象Actionは対象家族ごとに別Action IDを登録する。

同じAction内容であっても対象家族が異なる場合は別Actionとして扱う。

```text
Action ID 4 = 父 / 入室NG
Action ID 5 = 母 / 入室NG
```

1つのAction IDが持つ `target_family_id` は最大1件とする。複数家族を1つのAction IDに紐づけない。

親機は受信したAction IDからAction内容と対象家族を直接解決する。送信元 `device_id` からAction対象者を推測しない。

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

FAMILYの5種類は必要な家族ごとにAction定義を作成するため、Action ID数が家族数に応じて増えることを許容する。COMMONの2種類は対象家族を持たない。

「会議中」の解除専用Actionは設けず、対象家族の `入室OK` Actionを使用する。

### 3.4 対象家族とメッセージ

`FAMILY` Actionでは `target_family_id` を必須とし、家族マスタの `display_name` を参照する。

`COMMON` Actionでは `target_family_id = NULL` とする。「家」「共通」等のダミー家族は作成しない。

`web_message` 内の `{target}` は親機が対象家族の `display_name` で展開する。

Web表示メッセージにはデフォルト値を用意し、親機CDCで変更可能とする。

### 3.5 通知設定

スマートフォン通知はAction定義から分離する。

- 対象Action ID
- 通知有無
- 通知先家族

Actionの対象家族と通知先家族は別概念とする。LINE / Slack等の実送信先も家族に紐づく別設定として管理する。

---

## 4. 子機・親機の責務境界

| 情報・処理 | 子機 | 親機 |
|---|---|---|
| GPIO監視 | ○ | × |
| Edge検出 | ○ | × |
| GPIO+EdgeへのAction ID割当 | ○ | × |
| Action IDの意味解釈 | × | ○ |
| `target_type` / 対象家族 | × | ○ |
| 対象者表示名 | × | ○ |
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
OFF_TO_ON / ON_TO_OFF
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

基本Actionパターン、対象範囲、状態種別・状態値は本書の定義を基準とする。実際のAction IDはAction定義ごとに一意に割り当てる。