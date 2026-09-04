# Uchi-Pulse CDC通信仕様書

## 1. 目的
Uchi-PulseにおけるUSB CDC通信の共通仕様と、親機・子機それぞれの設定責務を定義する。

通常運用時の親機・子機間通信はWi-Fi/UDPを使用し、CDCはPC等から直接管理・設定するためのローカル通信路とする。

## 2. 共通通信仕様
- USB CDCを使用する。
- UTF-8 JSONを使用する。
- 1メッセージ1行、LF (`\n`) 終端とする。
- 要求・応答型とする。
- 初期プロトコルバージョンは `1` とする。
- Request/Response対応付けのため `request_id` を使用する。

### 2.1 Request
```json
{
  "version": 1,
  "request_id": "123",
  "command": "get_config",
  "params": {}
}
```

### 2.2 Success Response
```json
{
  "version": 1,
  "request_id": "123",
  "status": "ok",
  "data": {}
}
```

### 2.3 Error Response
```json
{
  "version": 1,
  "request_id": "123",
  "status": "error",
  "error": {
    "code": "INVALID_CONFIG",
    "message": "invalid configuration"
  }
}
```

## 3. 共通コマンド
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

## 4. 子機CDC設定

### 4.1 Input Event
- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

子機のAction入力割当は `GPIO + Input Event + Action ID` とする。

### 4.2 GPIO入力設定
- `gpio_inputs` は最大10件まで設定できる。
- GPIO番号は固定の2/3/4ではなく、Pico W / Pico 2 Wの外部端子として利用可能なGPIOから任意に指定できる。
- 対応GPIOは `0`〜`22`、`26`、`27`、`28` とする。
- `23`、`24`、`25`、`29` はオンボードCYW43 Wi-Fi接続で使用するため、子機入力には指定できない。
- 同一GPIOを `gpio_inputs` に重複登録してはならない。
- `input_mappings` が参照するGPIOは、必ず `gpio_inputs` に登録されていなければならない。

### 4.3 子機設定例
```json
{
  "device_id": "node-01",
  "gpio_inputs": [
    { "gpio": 0, "active_high": false, "debounce_ms": 30 },
    { "gpio": 5, "active_high": false, "debounce_ms": 30 },
    { "gpio": 28, "active_high": false, "debounce_ms": 30 }
  ],
  "input_mappings": [
    {
      "gpio": 5,
      "input_event": "CLICK",
      "action_id": 10,
      "enabled": true
    }
  ],
  "double_click_interval_ms": 400,
  "long_press_threshold_ms": 1000,
  "ack_timeout_ms": 60000,
  "event_retry_count": 3,
  "heartbeat_interval_sec": 180
}
```

`set_config.params` は上記設定オブジェクトを直接指定する。`get_config` の
`data` は同じ設定オブジェクトを返す。`gpio_inputs` と
`input_mappings` の各要素には、子機が対応するGPIO、Input Event、Action ID、
`enabled` を指定する。`set_config` は全設定項目を必須とし、保存後の次回起動時に
適用する。

- `double_click_interval_ms`: デフォルト400ms
- `long_press_threshold_ms`: デフォルト1000ms
- `debounce_ms`: デフォルト30ms（GPIO入力ごと）
- `ack_timeout_ms`: デフォルト60000ms
- `event_retry_count`: デフォルト3回
- `heartbeat_interval_sec`: デフォルト180秒
- 各設定は取得・変更・永続保存可能とする。
- `CLICK` と `DOUBLE_CLICK` は同一操作から重複発生させない。

子機はAction名、対象家族、状態変更、Web表示、通知設定、通知先を保持・解釈しない。

## 5. 親機CDC設定

親機CDCでは親機DBの設定を管理する。

### 5.1 Action本体
- `action_id`
- `action_name`
- `target_type`
- `target_family_id`
- `web_message`（NULL可）
- `enabled`

### 5.2 Action状態変更
状態変更はAction本体から分離し、`action_state_changes` として0..n件管理する。

- `action_id`
- `state_type`
- `state_value`

1 Actionで単一状態変更、複数状態変更、状態変更なしを扱える。

### 5.3 通知設定
`action_notification_settings`:
- `action_id`
- `notification_enabled`
- `notification_message`（NULL可）

`action_notification_targets` により通知先家族を0..n件設定できる。

Action対象家族と通知先家族は別概念とする。

### 5.4 親機設定JSON
親機の `get_config.data` および `set_config.params` は、次のオブジェクトとする。

```json
{
  "families": [
    { "family_id": 1, "display_name": "太郎", "enabled": true }
  ],
  "actions": [
    {
      "action_id": 10,
      "action_name": "ご飯通知",
      "target_type": "FAMILY",
      "target_family_id": 1,
      "web_message": "ご飯です",
      "enabled": true,
      "state_changes": [
        { "state_type": "MEAL_NOTICE", "state_value": "ON" }
      ],
      "notification_enabled": false,
      "notification_message": null,
      "notification_targets": []
    }
  ],
  "family_notification_destinations": [
    {
      "family_id": 1,
      "notification_type": "LINE",
      "destination": "line-user-id",
      "enabled": true
    }
  ]
}
```

`set_config`は、家族・Action・状態変更・通知設定・通知先を検証したうえで、親機の
SQLiteへ保存する。イベント履歴および子機の通信状態は設定JSONに含めない。

## 6. 設定反映
`set_config` は設定値を検証して永続保存する。原則として設定変更は次回起動時に反映する。

子機はFlash等、親機はSQLite等の永続ストレージへ保存する。

## 7. エラーコード
- `INVALID_JSON`
- `UNSUPPORTED_VERSION`
- `INVALID_COMMAND`
- `INVALID_PARAMETER`
- `INVALID_CONFIG`
- `SAVE_FAILED`
- `OPERATION_FAILED`
- `NOT_SUPPORTED`

## 8. 責務境界
```text
[子機 CDC]
GPIO + Input Event + Action ID
        ↓
UDP EVENT(Action ID)

============================

[親機]
Action ID
 ↓
Action本体
 ↓
action_state_changes 0..n
 ↓
Web表示（任意）
 ↓
通知設定（任意）
```

## 9. 関連仕様
- `docs/home_yuru_communication_design.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
