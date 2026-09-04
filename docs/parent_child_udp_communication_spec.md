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
 ├─ 重複あり → 履歴・Action処理を再実行せずACK送信
 └─ 新規
     ↓
   Action取得・enabled / target / state changes検証
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

重複EVENTではActionの再検証・再実行を行わず、履歴を追加せずACKのみ送信する。

外部Slack/LINE通知の完了はACK条件に含めない。通知失敗によってACK、履歴、状態変更を失敗扱いにしない。

存在しないAction、無効化されたAction、または対象定義が不正なActionのEVENTは受理しない。これらはEVENT履歴へ保存せず、状態更新およびACK送信も行わない。

Actionの事前検証とEVENT履歴保存が完了した後にAction内部処理が失敗した場合は、同一EVENTの再送をAction再実行に利用しないためACKを返す。内部失敗は親機内部のエラーとして記録・処理し、重複EVENTでは履歴・Action処理を再実行せずACKのみ返す。

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

専用メッセージは、親機から子機へ次のJSONをブロードキャストする。

```json
{
  "type": "HELLO_REQUEST"
}
```

`HELLO_REQUEST` は親機起動時の探索要求専用であり、通常の4メッセージには含めない。子機が応答する場合は、子機から親機への通常の `HELLO`（`type` と `device_id`）を送信する。`HELLO_REQUEST` 自体はACK対象外である。

親機のデフォルト送信先は `255.255.255.255:5000` とし、起動時設定で変更可能とする。

子機はUDP `5000`番で `HELLO_REQUEST` を受信する。受信したデータグラムの送信元
`IpEndpoint` を、子機のRAM上の現在の親機エンドポイントとして保持し、通常の
`HELLO` をその送信元へ返す。以後の `HEARTBEAT` と `EVENT` は保持したエンドポイントへ送信する。
親機のエンドポイントはFlashへ保存しないため、親機の再起動またはIP変更後は新しい
`HELLO_REQUEST` の送信元で更新される。

子機が親機エンドポイントをまだ取得していない状態でEVENTが発生した場合、子機は
EVENTを送信せず、再生成不可能なEVENTを無制限に保持しない。親機発見後に発生した
EVENTから通常の再送・ACK処理を行う。

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
