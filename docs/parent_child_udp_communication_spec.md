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

## 3. HELLO

子機起動時等に親機へ存在を通知する。親機起動時は子機へHELLO送信を促すブロードキャストを行えるものとする。

正常なHELLO受信は親機の通信状態をONLINEへ更新する根拠となる。

---

## 4. HEARTBEAT

子機の生存確認に使用する。

正常なHEARTBEAT受信は親機の通信状態をONLINEへ更新する根拠となる。

---

## 5. EVENT

### 5.1 Action ID

子機のGPIO入力から発生したEVENTでは、子機CDC設定に従ってAction IDを送信する。

子機側の入力割当は次の3要素である。

```text
GPIO + Edge + Action ID
```

`Edge` は `OFF_TO_ON` / `ON_TO_OFF` のいずれかとする。

GPIOおよびEdgeは子機がAction IDを選択するための入力設定であり、Action IDの意味定義そのものではない。

子機はAction IDの意味、対象家族、Web表示メッセージ、通知先を解釈しない。

親機は受信したAction IDを親機DBのAction定義と照合し、状態変更・Web表示・通知判断を行う。

### 5.2 EVENT概念形式

```json
{
  "type": "EVENT",
  "device_id": "node-01",
  "event_id": "...",
  "action_id": 1
}
```

`event_id` はEVENTの重複排除・ACK対応に使用するEVENT識別情報とする。具体的なID生成方式は実装詳細で定義する。

### 5.3 EVENT処理

```text
[子機]
GPIOエッジ検出
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
Web表示
  ↓
通知設定評価
  ↓
必要な場合のみ通知機能へ通知要求
```

---

## 6. ACK

EVENTに対するACKはできるだけ単純な形式とする。

ACKは対象EVENTの識別情報を返し、子機がどのEVENTに対するACKか判定できるようにする。

同一EVENTを再受信した場合、親機はイベント履歴およびAction処理を二重実行しないが、子機が再送を停止できるようACKは返す。

---

## 7. 通信状態

HELLO、HEARTBEAT、EVENT等の正常なUDPメッセージを受信した場合、親機は対象子機をONLINEとして扱う。

親機起動直後の登録済み子機は、通信確認前は `INITIAL_WAIT`（未確認）として扱う。

`INITIAL_WAIT` のままオフライン判定時間を超えた場合は `OFFLINE` へ遷移する。

一度OFFLINEになった子機でも正常なメッセージを受信すればONLINEへ復帰する。

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

---

## 8. Action定義との境界

本UDP仕様はAction IDを転送するが、Actionの意味は定義しない。

Action定義は親機側で管理し、初期Actionとして以下を使用する。

- ご飯通知
- ご飯通知クリア
- 入室OK
- 入室NG
- 会議中
- ポスト投函
- ポスト投函解除

詳細は `docs/home_yuru_communication_design.md` および `docs/parent-database-design.md` を参照する。

---

## 9. 関連仕様

- `docs/cdc_communication_spec.md`
- `docs/home_yuru_communication_design.md`
- `docs/parent-database-design.md`
- `docs/parent-overview-design.md`