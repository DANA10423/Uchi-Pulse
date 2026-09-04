# Uchi-Pulse 親機・子機 UDP通信仕様書

## 1. 目的
子機（Pico W / Pico 2 W）と親機（Zero W / Zero 2 W）の通常運用時UDP通信を定義する。

## 2. 基本方針
- JSONを使用する。
- メッセージ種別は `HELLO` / `HEARTBEAT` / `EVENT` / `ACK` とする。
- EVENTのみACK対象とする。
- ACK待ち時間はデフォルト60秒、設定変更可能とする。
- EVENT再送回数はデフォルト3回とする。
- HEARTBEAT間隔はデフォルト3分、設定変更可能とする。
- オフライン判定時間はデフォルト3分、設定変更可能とする。

## 3. メッセージ形式

### 3.1 HELLO
```json
{
  "type": "HELLO",
  "device_id": "node-01"
}
```

### 3.2 HEARTBEAT
```json
{
  "type": "HEARTBEAT",
  "device_id": "node-01"
}
```

### 3.3 EVENT
```json
{
  "type": "EVENT",
  "device_id": "node-01",
  "event_id": "boot-id-00000001",
  "action_id": 10
}
```

### 3.4 ACK
```json
{
  "type": "ACK",
  "device_id": "node-01",
  "event_id": "boot-id-00000001"
}
```

ACKの `device_id` はACK対象EVENTを送信した子機IDを使用する。

## 4. EVENT識別
`event_id` はEVENT重複排除とACK対応に使用する。

要件:
- 同一EVENTの再送では同じ `event_id` を使用する。
- 子機再起動をまたいでも重複判定に支障がない一意性を持たせる。
- 初期実装では `boot_id + sequence` 方式を採用する。
- `sequence` は起動中に単調増加する。

具体的な `boot_id` の生成方法は子機実装内で決定するが、同一子機の連続した起動で容易に衝突しない値とする。

## 5. 子機入力との境界
子機側入力割当:
```text
GPIO + Input Event + Action ID
```

Input Event:
- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

GPIO番号とInput EventはUDP EVENTへ含めない。UDP層はAction IDのみ受け取る。

## 6. EVENT受信・ACK処理
新規EVENT:
```text
EVENT受信
 ↓
JSON / device_id / event_id 検証
 ↓
重複チェック
 ↓
EVENT履歴保存
 ↓
Action処理
 ├─ 状態変更 0..n
 └─ Web表示処理
 ↓
ACK送信
 ↓
外部通知処理
```

重複EVENT:
```text
EVENT受信
 ↓
重複確認
 ↓
履歴・Action処理は再実行しない
 ↓
ACK送信
```

外部Slack/LINE通知の完了はACK条件に含めない。通知失敗によってACK、履歴、状態変更を失敗扱いにしない。

## 7. 通信状態
正常なHELLO / HEARTBEAT / EVENTを受信した場合、対象子機をONLINEとして扱う。

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --正常受信--> ONLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

親機起動時は登録済み有効子機を `INITIAL_WAIT` とする。

## 8. 親機起動時の探索
親機起動時は、子機へHELLO送信を促すためのブロードキャスト要求を送信できる構成とする。

ブロードキャスト要求の専用メッセージ形式は親機起動処理の詳細実装時に定義する。子機から親機への通常メッセージ形式 `HELLO` / `HEARTBEAT` / `EVENT` / `ACK` には影響させない。

## 9. Action定義との境界
UDP層はActionの意味を解釈しない。

```text
子機入力処理
GPIO + Input Event -> Action ID
        ↓
UDP EVENT(Action ID)
        ↓
親機Action処理
Action本体 + 状態変更0..n + Web + 通知
```

## 10. 関連仕様
- `docs/cdc_communication_spec.md`
- `docs/home_yuru_communication_design.md`
- `docs/parent-database-design.md`
- `docs/parent-overview-design.md`
