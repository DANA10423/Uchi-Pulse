# Uchi-Pulse 親機・子機 UDP通信仕様書

## 1. 目的

子機（Pico W / Pico 2 W）と親機（Zero W / Zero 2 W）の通常運用時UDP通信を定義する。

---

## 2. 基本方針

- JSONを使用する。
- 親機・子機は家庭内LAN上でUDP通信する。
- 子機はHELLO、HEARTBEAT、EVENTを送信する。
- EVENTはACK対象とする。
- ACK待ち時間はデフォルト60秒、設定変更可能とする。
- 再送回数はデフォルト3回とする。
- HEARTBEAT間隔はデフォルト3分、設定変更可能とする。
- オフライン判定時間はデフォルト3分、設定変更可能とする。

---

## 3. HELLO / HEARTBEAT

HELLOは子機起動時等の存在通知に使用する。HEARTBEATは生存確認に使用する。

正常なHELLO / HEARTBEAT受信は親機の通信状態をONLINEへ更新する根拠となる。

---

## 4. EVENT

### 4.1 Action ID

子機のGPIO入力から発生したEVENTでは、子機CDC設定に従ってAction IDを送信する。

子機側の入力割当は次の3要素である。

```text
GPIO + Input Event + Action ID
```

Input Eventの初期定義:

- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

Input Eventの検出・判定は子機側入力処理で行う。GPIO番号およびInput EventはAction IDを選択するための子機ローカル設定であり、UDP EVENTには含めない。

子機はAction IDの意味、対象家族、Web表示メッセージ、通知先を解釈しない。

### 4.2 EVENT形式

```json
{
  "type": "EVENT",
  "device_id": "node-01",
  "event_id": "...",
  "action_id": 10
}
```

`event_id` はEVENTの重複排除・ACK対応に使用するEVENT識別情報とする。

### 4.3 EVENT処理

```text
[子機]
GPIO監視
  ↓
Input Event判定
  ↓
Action ID取得
  ↓
EVENT送信
  ↓
ACK待ち / 必要に応じ再送

---------------- 責務境界 ----------------

[親機]
EVENT受信
  ↓
Action ID解釈
  ↓
履歴保存
  ↓
現在状態更新
  ↓
Web表示・通知設定評価
```

UDP層は `CLICK` / `DOUBLE_CLICK` / `LONG_PRESS` 等の判定ロジックを持たない。

---

## 5. ACK

EVENTに対するACKはできるだけ単純な形式とする。

同一EVENTを再受信した場合、親機はイベント履歴およびAction処理を二重実行しないが、子機が再送を停止できるようACKは返す。

---

## 6. 通信状態

HELLO、HEARTBEAT、EVENT等の正常なUDPメッセージを受信した場合、親機は対象子機をONLINEとして扱う。

親機起動直後の登録済み子機は `INITIAL_WAIT` とし、オフライン判定時間超過で `OFFLINE` へ遷移する。OFFLINE後も正常受信でONLINEへ復帰する。

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

---

## 7. Action定義との境界

本UDP仕様はAction IDを転送するが、Actionの意味は定義しない。

Action定義の詳細は `docs/home_yuru_communication_design.md` および `docs/parent-database-design.md` を参照する。

---

## 8. 関連仕様

- `docs/cdc_communication_spec.md`
- `docs/home_yuru_communication_design.md`
- `docs/parent-database-design.md`
- `docs/parent-overview-design.md`
