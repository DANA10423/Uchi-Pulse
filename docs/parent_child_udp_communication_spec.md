# 親機・子機間 通信仕様書

## 1. 目的

本仕様書は、家庭内ゆるコミュニケーションツールにおける、親機（Raspberry Pi Zero W / Zero 2 W）と子機（Raspberry Pi Pico W / Pico 2 W）間の通信仕様を定義する。

本通信は家庭内Wi-Fi / LAN上で行い、複数子機から親機へのイベント通知、生存確認、オンライン・オフライン状態管理を対象とする。

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
| 重複判定 | `device_id + message_id` |
| 子機一覧の永続化 | 行わない |

---

## 3. 子機管理

### 3.1 親機起動時

親機は起動時に子機一覧を空の状態で開始する。

子機一覧は親機のメモリ上だけに保持し、ファイルやデータベースへの永続保存は行わない。

親機の電源OFFまたは再起動により、子機一覧は破棄される。

### 3.2 子機の自動登録

親機は、有効な `device_id` を持つ正常な子機パケットを受信した場合、子機一覧を確認する。

- 未登録の `device_id` なら、新しい子機として一覧へ追加する。
- 登録済みなら、既存の子機情報を更新する。
- 正常なパケットを受信した子機は `ONLINE` とする。

`HELLO` は子機の起動・参加通知として使用するが、子機登録の必須条件ではない。

したがって、未登録子機から `EVENT` や `HEARTBEAT` を受信した場合でも、その子機を登録して `ONLINE` とする。

### 3.3 子機識別

子機は `device_id` によって識別する。

IPアドレスは子機識別には使用しない。

DHCP等によって子機のIPアドレスが変化した場合でも、同じ `device_id` であれば同一子機として扱い、親機が保持するIPアドレス等の通信情報を更新する。

### 3.4 子機一覧の削除

親機動作中は、子機一覧からの自動削除を行わない。

通信不能になった子機は `OFFLINE` として一覧に残す。

親機の電源OFFまたは再起動時に一覧全体を破棄する。

---

## 4. ONLINE / OFFLINE管理

### 4.1 ONLINE判定

親機が子機から正常なパケットを受信した場合、その子機を `ONLINE` とする。

対象には少なくとも以下を含む。

- `HELLO`
- `HEARTBEAT`
- `EVENT`

正常な子機パケットを受信するたびに `last_seen` を更新する。

一度 `OFFLINE` となった子機でも、正常なパケットを受信した時点で即座に `ONLINE` へ復帰する。

### 4.2 HEARTBEAT

子機は一定間隔で `HEARTBEAT` を送信する。

初期値は以下とする。

```text
heartbeat_interval_sec = 180
```

すなわち3分間隔とする。

この値は設定変更可能なオプション値とする。

`HEARTBEAT` に対してACKは返さない。

### 4.3 OFFLINE判定

親機は最後に正常なパケットを受信した時刻 `last_seen` から一定時間経過した場合、その子機を `OFFLINE` とする。

初期値は以下とする。

```text
offline_timeout_sec = 210
```

すなわち3分30秒とする。

この値は設定変更可能なオプション値とする。

`OFFLINE` は子機の削除を意味しない。

### 4.4 状態遷移

```text
未登録
   │
   │ 正常な子機パケット受信
   ▼
 ONLINE
   │
   │ last_seen から offline_timeout_sec 経過
   ▼
 OFFLINE
   │
   │ 正常な子機パケット受信
   ▼
 ONLINE
```

---

## 5. JSON共通フォーマット

ACKを除く子機から親機へのメッセージは、以下の共通構造を基本とする。

```json
{
  "version": 1,
  "type": "EVENT",
  "device_id": "CHILD-001",
  "message_id": 12345,
  "data": {}
}
```

### 5.1 共通フィールド

| フィールド | 型 | 必須 | 内容 |
|---|---|---|---|
| `version` | integer | 必須 | 通信プロトコルバージョン。初期値は `1` |
| `type` | string | 必須 | メッセージ種別 |
| `device_id` | string | 必須 | 子機固有識別子 |
| `message_id` | integer | 必須 | 子機が付与するメッセージ番号 |
| `data` | object | 必須 | メッセージ固有データ |

### 5.2 メッセージ種別

初期仕様では以下を定義する。

| type | 用途 |
|---|---|
| `HELLO` | 子機の起動・参加通知 |
| `HEARTBEAT` | 生存確認 |
| `EVENT` | イベント通知 |
| `ACK` | EVENT受信確認 |

---

## 6. message_id

`message_id` は子機ごとに管理する連番とする。

例:

```text
HELLO       message_id = 1
HEARTBEAT   message_id = 2
HEARTBEAT   message_id = 3
EVENT       message_id = 4
HEARTBEAT   message_id = 5
EVENT       message_id = 6
```

再送時には新しい `message_id` を割り当てず、最初に送信したEVENTと同じ `message_id` を使用する。

親機は以下の組み合わせでEVENTの重複を判定する。

```text
device_id + message_id
```

---

## 7. HELLO

### 7.1 用途

子機の起動時やWi-Fiへの再接続時などに、子機が自身の存在を親機へ通知する。

ただし、`HELLO` は子機登録およびONLINE判定の必須条件ではない。

### 7.2 フォーマット

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

### 7.3 data

| フィールド | 型 | 内容 |
|---|---|---|
| `name` | string | 子機の表示名 |
| `firmware_version` | string | 子機ファームウェアバージョン |

IPアドレスはJSONデータに含めず、親機がUDPパケットの送信元情報から取得する。

`HELLO` はACK対象外とする。

---

## 8. HEARTBEAT

### 8.1 用途

子機が正常に動作し、親機と通信可能であることを通知する。

### 8.2 フォーマット

```json
{
  "version": 1,
  "type": "HEARTBEAT",
  "device_id": "CHILD-001",
  "message_id": 152,
  "data": {}
}
```

`HEARTBEAT` はACK対象外とする。

親機は正常受信時に以下を行う。

```text
last_seen = 現在時刻
status = ONLINE
```

---

## 9. EVENT

### 9.1 用途

子機で発生したボタン操作、センサー検知、状態変化等を親機へ通知する。

### 9.2 フォーマット

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

### 9.3 EVENTデータ

| フィールド | 型 | 必須 | 内容 |
|---|---|---|---|
| `event_type` | string | 必須 | イベント種別 |
| `channel` | integer | 必須 | 論理入力チャンネル |
| `value` | integer | 必須 | イベント値 |

### 9.4 event_type

初期仕様では以下を想定する。

| event_type | 用途 |
|---|---|
| `BUTTON` | イベントボタン等の操作 |
| `SENSOR` | ポスト投函検知等のセンサーイベント |
| `STATE` | 状態変化通知 |

イベント固有の意味は、物理GPIO番号ではなく論理 `channel` によって扱う。

例えば子機内部では、

```text
GPIO15 → channel 1 → EVENT_BUTTON
```

のように設定する。

通信プロトコルには物理GPIO番号を直接持ち込まない。

---

## 10. ACK

### 10.1 対象

ACKは `EVENT` のみを対象とする。

`HELLO` および `HEARTBEAT` にはACKを返さない。

### 10.2 フォーマット

ACKは可能な限り単純なJSONとする。

```json
{
  "type": "ACK",
  "message_id": 153
}
```

ACKには以下を含めない。

- `version`
- `device_id`
- `data`

`message_id` には、受信したEVENTの `message_id` をそのまま設定する。

### 10.3 ACK照合

子機は受信したACKについて、

```text
ACK.message_id == 送信中EVENT.message_id
```

を確認する。

一致した場合、そのEVENTは親機へ正常到達したものとして送信処理を完了する。

---

## 11. EVENT再送

### 11.1 ACK待ち時間

EVENT送信後のACK待ち時間の初期値は以下とする。

```text
ack_timeout_sec = 3
```

3秒以内に対象EVENTのACKを受信できなかった場合、EVENTを再送する。

この値は設定変更可能なオプション値とする。

### 11.2 再送回数

初期値は以下とする。

```text
event_retry_count = 3
```

「再送回数3回」は、初回送信とは別に最大3回再送することを意味する。

したがって最大送信回数は4回となる。

```text
初回送信
   ↓ 3秒
再送1
   ↓ 3秒
再送2
   ↓ 3秒
再送3
```

再送時も同じ `message_id` を使用する。

---

## 12. EVENT重複処理

ACKがネットワーク上で失われた場合、親機がすでに処理したEVENTを子機が再送する可能性がある。

親機は、

```text
device_id + message_id
```

によってEVENTの重複を判定する。

すでに処理済みのEVENTを再受信した場合、

1. EVENTの本処理は再実行しない。
2. ACKは再度返信する。

例:

```text
子機                         親機

EVENT #100 ────────────────>
                             EVENT処理
       <────────── ACK #100
              × ACK消失

3秒後

EVENT #100 ────────────────>
                             処理済みと判定
                             EVENT再処理なし
       <────────── ACK #100
```

---

## 13. 時刻情報

初期通信仕様では、子機から送信するJSONに `timestamp` を必須項目として設けない。

親機がパケットを正常受信した時刻を `received_at` / `last_seen` として管理する。

将来、子機側で正確な時刻を取得できる構成が必要になった場合は、オプションフィールドとして追加可能とする。

---

## 14. 親機側の子機管理情報

親機は少なくとも以下の情報をメモリ上に保持する。

| 項目 | 内容 |
|---|---|
| `device_id` | 子機固有ID |
| `name` | 子機表示名（取得できている場合） |
| `ip_address` | 最後に確認した送信元IP |
| `source_port` | 最後に確認したUDP送信元ポート |
| `status` | `ONLINE` / `OFFLINE` |
| `first_seen` | 親機起動後に最初に確認した時刻 |
| `last_seen` | 最後に正常なパケットを受信した時刻 |
| `firmware_version` | HELLOで取得したファームウェアバージョン |

---

## 15. 設定値

初期設定値を以下とする。

| 設定 | 初期値 | 変更可否 |
|---|---:|---|
| `ack_timeout_sec` | 3秒 | 可 |
| `event_retry_count` | 3回 | 可 |
| `heartbeat_interval_sec` | 180秒 | 可 |
| `offline_timeout_sec` | 210秒 | 可 |

---

## 16. 通信シーケンス例

### 16.1 通常起動

```text
子機                              親機
 │                                 │
 │──── HELLO ─────────────────────>│
 │                                 │ 登録 / ONLINE
 │                                 │
 │──── HEARTBEAT ─────────────────>│
 │                                 │ last_seen更新
 │                                 │
 │──── EVENT #153 ────────────────>│
 │                                 │ EVENT処理
 │<────────────── ACK #153 ───────│
 │                                 │
```

### 16.2 OFFLINEからの復帰

```text
子機                              親機
 │                                 │
 │       通信なし                  │
 │                                 │
 │                       OFFLINE判定
 │                                 │
 │──── EVENT #200 ────────────────>│
 │                                 │ 即ONLINE復帰
 │                                 │ EVENT処理
 │<────────────── ACK #200 ───────│
```

`HELLO` を受信しなくても、正常なEVENT等を受信すればONLINEへ復帰する。

### 16.3 親機再起動

```text
親機再起動
    ↓
子機一覧 = 空

子機からHEARTBEAT / EVENT / HELLOを受信
    ↓
device_id 未登録
    ↓
子機一覧へ自動追加
    ↓
ONLINE
```

---

## 17. 確定事項まとめ

本仕様では以下を確定事項とする。

1. 親機・子機間通信はUDPを使用する。
2. 通信データはJSON形式とする。
3. EVENTのみACKによる到達確認を行う。
4. EVENTのACK待ち時間初期値は3秒とする。
5. EVENTは初回送信に加えて最大3回再送する。
6. 子機は3分間隔を初期値としてHEARTBEATを送信する。
7. 親機は最終正常受信から3分30秒を初期値としてOFFLINE判定する。
8. 正常な子機パケットを受信すれば、その子機をONLINEとする。
9. HELLOは登録・ONLINE判定の必須条件ではない。
10. 未登録子機から正常なパケットを受信した場合は自動登録する。
11. OFFLINEとなった子機も正常なパケット受信時に即ONLINEへ復帰する。
12. 子機一覧は親機のメモリ上だけで保持し、親機電源OFF時に破棄する。
13. 子機は `device_id` で識別し、IPアドレスでは識別しない。
14. EVENTの重複は `device_id + message_id` で判定する。
15. 重複EVENTは再処理しないがACKは再送する。
16. ACKは `type` と `message_id` のみを持つ最小JSONとする。
17. 子機側timestampは初期仕様では必須としない。
