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

### 4.2 子機設定例
```json
{
  "input_mappings": [
    {
      "gpio": 5,
      "input_event": "CLICK",
      "action_id": 10
    }
  ],
  "double_click_interval_ms": 400,
  "long_press_threshold_ms": 1000
}
```

- `double_click_interval_ms`: デフォルト400ms
- `long_press_threshold_ms`: デフォルト1000ms
- 両設定は取得・変更・永続保存可能とする。
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
