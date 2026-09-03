# 親機・子機間 通信仕様書

## 1. 目的

本仕様書は、Uchi-Pulse における親機（Raspberry Pi Zero W / Zero 2 W）と子機（Raspberry Pi Pico W / Pico 2 W）間の通信仕様を定義する。

家庭内Wi-Fi/LAN上で、複数子機から親機へのイベント通知、生存確認、および親機による通信状態判定を行う。

---

## 2. 基本仕様

| 項目 | 仕様 |
|---|---|
| ネットワーク | 家庭内Wi-Fi / LAN |
| トランスポート | UDP |
| データ形式 | JSON |
| 主通信方向 | 子機 → 親機 |
| 到達確認 | EVENTのみACKを使用 |
| 複数子機 | 対応 |
| 子機識別 | `device_id` |
| メッセージ識別 | `message_id` |
| EVENT重複判定 | `device_id + message_id` |
| 子機登録情報 | 親機SQLiteのデバイスマスタに永続化 |
| 通信状態 | 親機メモリ上で管理し、DBへ永続化しない |

---

## 3. 子機管理

### 3.1 親機起動時

親機は起動時にSQLiteのデバイスマスタから登録済みかつ有効な子機を取得し、通信状態管理の対象とする。

各子機の起動直後の状態は `INITIAL_WAIT`（未確認）とし、最終正常受信時刻は未設定とする。

親機起動時に子機一覧を空の状態から開始する方式は採用しない。

### 3.2 デバイスマスタと通信状態

子機の登録情報と現在の通信状態は分離する。

- デバイスマスタ: SQLiteへ永続化する。
- 通信状態: 親機稼働中のメモリ上で管理する。

Nodeから通信を受信したことだけを理由として、未登録Nodeを自動的にデバイスマスタへ登録することは本仕様では前提としない。

### 3.3 子機識別

子機は `device_id` によって識別する。

IPアドレスは子機そのものの識別子として使用しない。

### 3.4 親機起動時のブロードキャスト

親機起動時にHELLO要求を全子機へブロードキャストする処理は必須としない。

親機は登録済み子機を `INITIAL_WAIT` として管理し、子機から自発的に送信されるHELLO、HEARTBEAT、EVENT等の正常な通信を待つ。

---

## 4. 通信状態管理

### 4.1 状態

| 状態 | 意味 |
|---|---|
| `INITIAL_WAIT` | 登録済みだが、現在起動している親機がまだ正常な通信を確認していない |
| `ONLINE` | 正常な通信を確認済みで、オフライン判定時間内に通信がある |
| `OFFLINE` | 一度通信を確認した後、オフライン判定時間を超えて正常な通信がない |

### 4.2 ONLINE判定

親機が登録済み子機から通信仕様上有効なパケットを正常に受信した場合、その子機を `ONLINE` とする。

対象には以下を含む。

- `HELLO`
- `HEARTBEAT`
- `EVENT`

正常なパケットを受信するたびに最終正常受信時刻を更新する。

一度 `OFFLINE` となった子機も、正常なパケットを受信した時点で `ONLINE` へ復帰する。

### 4.3 HEARTBEAT

子機は一定間隔でHEARTBEATを送信する。

初期値は以下とする。

```text
heartbeat_interval_sec = 180
```

HEARTBEATにはACKを返さない。

### 4.4 OFFLINE判定

親機はONLINEの子機について、最後に正常なパケットを受信してから設定されたオフライン判定時間を超えた場合、その子機を `OFFLINE` とする。

初期値は以下とする。

```text
offline_timeout_sec = 210
```

`OFFLINE` はデバイスマスタからの削除を意味しない。

`INITIAL_WAIT` は親機起動後まだ一度も通信を確認していない状態であり、単純にOFFLINEと同一視しない。

### 4.5 状態遷移

```text
親機起動
   │
   ▼
INITIAL_WAIT
   │
   │ 正常な子機パケット受信
   ▼
 ONLINE
   │
   │ 最終正常受信からoffline_timeout_sec経過
   ▼
 OFFLINE
   │
   │ 正常な子機パケット受信
   └────────────────────→ ONLINE
```

---

## 5. JSON共通フォーマット

ACKを除く子機から親機へのメッセージは以下を基本とする。

```json
{
  "version": 1,
  "type": "EVENT",
  "device_id": "CHILD-001",
  "message_id": 12345,
  "data": {}
}
```

| フィールド | 型 | 必須 | 内容 |
|---|---|---|---|
| `version` | integer | 必須 | 通信プロトコルバージョン。初期値1 |
| `type` | string | 必須 | メッセージ種別 |
| `device_id` | string | 必須 | 子機固有識別子 |
| `message_id` | integer | 必須 | 子機が付与するメッセージ番号 |
| `data` | object | 必須 | メッセージ固有データ |

基本メッセージは `HELLO`、`HEARTBEAT`、`EVENT`、`ACK` とする。

---

## 6. message_id

`message_id` は子機ごとに管理する連番とする。

EVENT再送時には新しい `message_id` を割り当てず、最初に送信したEVENTと同じ値を使用する。

親機はEVENTについて以下の組み合わせで重複を判定する。

```text
device_id + message_id
```

データベース設計上の `event_id` は、このEVENT識別情報を保存する項目として扱う。

---

## 7. HELLO

HELLOは子機の起動時やWi-Fi再接続時などに、自身の存在を親機へ通知するために使用する。

HELLOはONLINE判定の必須条件ではない。EVENTまたはHEARTBEATを先に正常受信した場合も、登録済み子機の通信状態をONLINEとする。

例:

```json
{
  "version": 1,
  "type": "HELLO",
  "device_id": "CHILD-001",
  "message_id": 1,
  "data": {
    "name": "リビング",
    "firmware_version": "1.0.0"
  }
}
```

HELLOはACK対象外とする。

---

## 8. HEARTBEAT

HEARTBEATは、子機が正常に動作し親機と通信可能であることを通知する。

```json
{
  "version": 1,
  "type": "HEARTBEAT",
  "device_id": "CHILD-001",
  "message_id": 152,
  "data": {}
}
```

HEARTBEATはACK対象外とする。

正常受信した場合、親機は対象の登録済み子機をONLINEとし、最終正常受信時刻を更新する。

---

## 9. EVENT

EVENTは、子機で発生したボタン操作、センサー検知、状態変化等を親機へ通知する。

```json
{
  "version": 1,
  "type": "EVENT",
  "device_id": "CHILD-001",
  "message_id": 153,
  "data": {
    "event_type": "BUTTON",
    "channel": 1,
    "value": 1
  }
}
```

初期仕様の `event_type` は以下を想定する。

| event_type | 用途 |
|---|---|
| `BUTTON` | イベントボタン等の操作 |
| `SENSOR` | ポスト投函検知等のセンサーイベント |
| `STATE` | 状態変化通知 |

イベント固有の意味は物理GPIO番号ではなく論理 `channel` によって扱う。

正常なEVENTを受信した場合、親機は通信状態をONLINEへ更新し、イベント履歴をSQLiteへ保存したうえでACKを返す。

---

## 10. ACK

ACKはEVENTのみを対象とする。HELLOおよびHEARTBEATにはACKを返さない。

ACKは可能な限り単純なJSONとする。

```json
{
  "type": "ACK",
  "message_id": 153
}
```

`message_id` には受信したEVENTの `message_id` を設定する。

子機はACKの `message_id` が送信中EVENTと一致した場合、そのEVENTの送信を完了する。

---

## 11. EVENT再送

EVENT送信後のACK待ち時間の初期値は以下とする。

```text
ack_timeout_sec = 3
```

ACKを受信できなかった場合、同じEVENTを再送する。

再送回数の初期値は以下とする。

```text
event_retry_count = 3
```

再送時も同じ `message_id` を使用する。

---

## 12. EVENT重複処理

ACKが失われた場合など、親機が処理済みのEVENTを子機が再送する可能性がある。

親機は `device_id + message_id` によってEVENTの重複を判定する。

同一EVENTを再受信した場合は以下とする。

1. EVENT履歴を二重登録しない。
2. EVENTに伴う本処理を重複実行しない。
3. ACKは再度返信する。

SQLiteでは `(device_id, event_id)` の一意制約により履歴の二重登録を防止する。

---

## 13. 親機の受信処理概要

```text
UDP受信
  │
  ▼
JSON解析・妥当性確認
  │
  ▼
登録済みdevice_idの確認
  │
  ▼
正常受信として通信状態をONLINEへ更新
  │
  ▼
メッセージ種別判定
  │
  ├── HELLO ──────> 存在通知として処理
  ├── HEARTBEAT ──> 生存通知として処理
  └── EVENT ──────> 重複判定 → 履歴保存 → ACK
```

単にUDPパケットを受信しただけではONLINEとせず、通信仕様上有効なメッセージとして正常に解析できた場合に状態更新する。

---

## 14. 関連文書

- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/uchi-pulse-system-overview.md`
