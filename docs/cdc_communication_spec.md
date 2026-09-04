# Uchi-Pulse CDC通信仕様書

## 1. 目的

本仕様書はUchi-PulseにおけるUSB CDC通信の共通仕様と、親機・子機それぞれの設定責務を定義する。

通常運用時の親機・子機間通信はWi-Fi/UDPを使用し、CDCはPC等から直接管理・設定するためのローカル通信路とする。

---

## 2. 対象

- 親機: Raspberry Pi Zero W / Zero 2 W
- 子機: Raspberry Pi Pico W / Pico 2 W

通信プロトコルの基本形式は共通化するが、設定内容は親機と子機で異なる。

---

## 3. 共通通信仕様

- USB CDCを使用する。
- UTF-8 JSONを使用する。
- 1メッセージ1行、LF (`\n`) 終端とする。
- 原則として要求・応答型とする。
- 初期プロトコルバージョンは `1` とする。

---

## 4. 共通コマンド

| command | 内容 |
|---|---|
| `get_info` | ターゲット情報取得 |
| `get_config` | 保存設定取得 |
| `set_config` | 設定変更・保存 |
| `get_status` | 現在状態取得 |
| `get_inputs` | 入力状態取得 |
| `get_outputs` | 出力状態取得 |
| `reboot` | 再起動 |
| `factory_reset` | 保存設定初期化 |

---

## 5. 子機CDC設定

### 5.1 責務

子機CDCでは、GPIO入力からどのAction IDを発生させるかを設定する。

入力条件は `Edge` ではなく、より一般化した `Input Event` として扱う。

### 5.2 Input Event

初期版では以下を定義する。

- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

`OFF_TO_ON` / `ON_TO_OFF` はGPIO状態変化を表す。

`CLICK` / `DOUBLE_CLICK` / `LONG_PRESS` は子機が入力状態と時間を基に判定する論理イベントである。

### 5.3 Action入力割当

1つの入力割当は次の3項目から構成する。

| 項目 | 内容 |
|---|---|
| `gpio` | GPIO番号 |
| `input_event` | Input Event |
| `action_id` | 発生させるAction ID |

例:

```json
{
  "gpio": 5,
  "input_event": "DOUBLE_CLICK",
  "action_id": 11
}
```

同一GPIOの異なるInput Eventに別Action IDを設定できる。

### 5.4 操作判定設定

`CLICK` / `DOUBLE_CLICK` / `LONG_PRESS` を判定するため、子機設定に以下を持つ。

| 項目 | 単位 | デフォルト | 内容 |
|---|---:|---:|---|
| `double_click_interval_ms` | ms | 400 | 1回目のクリック後、2回目のクリックを待つ時間 |
| `long_press_threshold_ms` | ms | 1000 | 押下継続を長押しと判定する時間 |

両項目は子機CDCの `get_config` / `set_config` で取得・変更でき、子機の永続設定として保存する。

設定例:

```json
{
  "double_click_interval_ms": 400,
  "long_press_threshold_ms": 1000
}
```

`CLICK` は、1回目のクリック後 `double_click_interval_ms` 内に2回目のクリックが成立しなかった時点で確定する。2回目が成立した場合は `DOUBLE_CLICK` とし、同じ操作について `CLICK` を重複発生させない。

`LONG_PRESS` は押下状態が `long_press_threshold_ms` 以上継続した場合に成立する。

### 5.5 子機CDCで設定しない情報

以下は親機側の情報であり、子機へ設定しない。

- Action名・意味
- `target_type`
- Action対象家族
- 対象者表示名
- Web表示メッセージ
- 親機状態変更内容
- スマートフォン通知有無
- 通知先家族
- LINE / Slack等の実送信先

子機はAction IDを不透明な識別子として扱う。

---

## 6. 親機CDC設定

親機CDCでは、親機DBに保持するAction定義や関連設定を管理対象とする。

Action定義はAction IDごとに以下を保持する。

- `action_name`
- `target_type` (`FAMILY` / `COMMON`)
- `target_family_id`
- `web_message`
- `state_type`
- `state_value`
- `enabled`

`FAMILY` Actionでは対象家族ごとに別Action IDを登録する。`COMMON` Actionでは `target_family_id = NULL` とする。

スマートフォン通知設定はAction定義とは分離する。

---

## 7. 親機と子機の設定境界

```text
[子機 CDC]
GPIO
 + Input Event
 + Action ID
 + 操作判定時間
      ↓
通常運用時にUDP EVENTとしてAction ID送信

================ 責務境界 ================

[親機]
Action ID
      ↓
Action定義
 + target_type
 + target_family_id
 + Web表示
 + 状態変更
      ↓
通知設定
```

Input Eventの判定は子機で完了し、親機やUDP層へInput Eventそのものを送る必要はない。

---

## 8. 設定反映

`set_config` は設定値を検証して永続保存する。原則として設定変更は次回起動時に反映する。

子機ではFlash等へ保存し、親機ではLinux上の永続ストレージ（SQLite等）へ保存する。

---

## 9. エラー

初期エラーコード:

- `INVALID_JSON`
- `UNSUPPORTED_VERSION`
- `INVALID_COMMAND`
- `INVALID_PARAMETER`
- `INVALID_CONFIG`
- `SAVE_FAILED`
- `OPERATION_FAILED`
- `NOT_SUPPORTED`

子機ではGPIO番号、`input_event`、Action ID、`double_click_interval_ms`、`long_press_threshold_ms` 等を検証対象とする。

---

## 10. 関連仕様

- `docs/home_yuru_communication_design.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
