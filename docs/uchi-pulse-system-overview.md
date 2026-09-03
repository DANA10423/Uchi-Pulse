# Uchi Pulse システム概要設計書

## 1. 目的

本書は、家庭内での「ゆるいコミュニケーション」を実現する Uchi Pulse システムの概要設計を定義する。

Uchi Pulse は、家族ごと・場所ごとに配置した子機（Node）から、ボタン操作やセンサー入力などのイベントを家庭内ネットワーク上の親機（Hub）へ通知し、家庭内の小さな「合図」や「気配」を共有するシステムである。

---

## 2. システム構成

| プロジェクト | 役割 | 主な実行環境 |
|---|---|---|
| `uchi-pulse-hub` | 親機。Node管理、イベント受信、状態管理、履歴保存 | Raspberry Pi Zero W / Zero 2 W |
| `uchi-pulse-node` | 子機。ボタン、センサー、GPIO入出力、イベント送信 | Raspberry Pi Pico W / Pico 2 W |
| `uchi-pulse-config` | Node初期設定ツール | PC |
| `uchi-pulse-core` | 将来必要になった場合の共通データ型・処理 | 共通ライブラリ |

```text
                         家庭内ネットワーク
                              Wi-Fi
                                │
                    ┌─────────────────────┐
                    │   Uchi Pulse Hub    │
                    │ Raspberry Pi Zero   │
                    │   W / Zero 2 W      │
                    └──────────┬──────────┘
                               │
                 ┌─────────────┼─────────────┐
                 │             │             │
              Wi-Fi          Wi-Fi         Wi-Fi
                 │             │             │
        ┌────────▼──────┐ ┌───▼──────────┐ ┌▼─────────────┐
        │ Pulse Node #1 │ │ Pulse Node #2│ │ Pulse Node #N│
        │ Pico W/2 W    │ │ Pico W/2 W   │ │ Pico W/2 W   │
        └───────────────┘ └──────────────┘ └──────────────┘
```

---

## 3. Uchi Pulse Hub

### 3.1 役割

`uchi-pulse-hub` はシステムの親機として以下を担当する。

- 登録済みNodeの管理
- NodeからのUDP通信受信
- Nodeの現在の通信状態管理
- EVENT受信履歴のSQLite保存
- EVENTに対するACK応答
- 将来的なWeb表示・通知機能への情報提供

### 3.2 Node登録情報

Nodeの登録情報は親機のSQLiteデータベースにデバイスマスタとして永続化する。

親機起動時は、デバイスマスタに登録されている有効なNodeを読み込み、通信状態管理の対象とする。

Nodeから通信を受信したことだけを理由として、未登録Nodeを自動的にデバイスマスタへ登録することは本概要では前提としない。

### 3.3 通信状態

通信状態そのものはデータベースへ保存せず、親機稼働中のメモリ上で管理する。

状態は以下の3種類とする。

| 状態 | 表示 | 意味 |
|---|---|---|
| `INITIAL_WAIT` | 未確認 | 親機起動後、そのNodeからまだ正常な通信を確認していない |
| `ONLINE` | オンライン | 正常な通信を確認し、通信可能と判断している |
| `OFFLINE` | オフライン | 一度通信を確認した後、一定時間正常な通信がない |

親機起動時、登録済みの有効なNodeは `INITIAL_WAIT` とする。

HELLO、HEARTBEAT、EVENT等の正常な通信を受信すると `ONLINE` とする。一度 `OFFLINE` となったNodeも、正常な通信を再受信すれば `ONLINE` へ復帰する。

---

## 4. Uchi Pulse Node

`uchi-pulse-node` は家庭内の各場所または家族向けに設置する子機である。

主な責務は以下とする。

- イベントボタン入力
- センサー入力
- GPIO入力・出力
- HubへのEVENT通知
- HubへのHELLO・HEARTBEAT送信
- 設定情報の保持

Nodeの基本ハードウェアとして Raspberry Pi Pico W / Pico 2 W を想定する。

イベントボタンはNodeの基本構成として必須とし、具体的な機能はNodeごとの設定によって決定する。

Node自身にはWeb設定画面を持たせず、PC上の設定ツールから初期設定する。

---

## 5. 通信概要

HubとNodeは家庭内Wi-Fi/LAN上でUDP通信を行い、アプリケーションデータにはJSONを使用する。

| 種別 | 方向 | 用途 | ACK |
|---|---|---|---|
| `HELLO` | Node → Hub | Nodeの存在通知 | なし |
| `HEARTBEAT` | Node → Hub | 生存通知 | なし |
| `EVENT` | Node → Hub | ボタン・センサー等のイベント通知 | あり |
| `ACK` | Hub → Node | EVENT受信確認 | - |

HELLOはONLINE判定の必須条件ではない。通信仕様上有効なHELLO、HEARTBEAT、EVENTのいずれを受信した場合も、その登録済みNodeをONLINEとして扱う。

親機起動時にNodeへHELLO要求をブロードキャストする処理は必須としない。Nodeからの自発的な通信を待つ。

詳細は `docs/parent_child_udp_communication_spec.md` を参照する。

---

## 6. EVENTとACK

EVENTのみACK対象とする。

NodeはEVENT送信後にACKを待ち、ACKを受信できない場合は通信仕様に従って再送する。

Hubは同一EVENTを再受信しても履歴を二重登録せず、ACKは再度返信する。

EVENT受信履歴はSQLiteへ保存する。データベース仕様は `docs/parent-database-design.md` を参照する。

---

## 7. HEARTBEATとオフライン判定

Nodeは一定間隔でHEARTBEATを送信する。

Hubは最後に正常な通信を確認してから所定のオフライン判定時間を超えたONLINEのNodeをOFFLINEとする。

OFFLINEは登録削除を意味しない。デバイスマスタは維持され、正常な通信を再受信した時点でONLINEへ復帰する。

`INITIAL_WAIT` は親機起動後にまだ通信確認できていない状態であり、OFFLINEとは区別する。

---

## 8. Hubの基本ライフサイクル

```text
電源ON
  │
  ▼
初期化
  │
  ▼
SQLiteオープン／初期化
  │
  ▼
デバイスマスタから有効なNodeを取得
  │
  ▼
各Nodeを INITIAL_WAIT（未確認）として状態管理へ登録
  │
  ▼
UDP待受開始
  │
  ├── HELLO受信 ──────> ONLINE更新
  ├── HEARTBEAT受信 ──> ONLINE更新
  └── EVENT受信 ──────> ONLINE更新 → 履歴保存 → ACK
  │
  └── 状態監視 ───────> 一定時間通信なし → OFFLINE
```

---

## 9. データ管理方針

```text
SQLite（永続情報）
  ├─ devices : 登録済みNodeのマスタ情報
  └─ events  : EVENT受信履歴

メモリ（一時情報）
  └─ Node通信状態
       ├─ INITIAL_WAIT
       ├─ ONLINE
       └─ OFFLINE
```

DBは「登録情報と過去の事実」、メモリは「現在の通信状態」を管理する。

---

## 10. 設計上の基本方針

- 家庭内用途を前提とし、必要以上に複雑な通信制御を導入しない。
- Nodeは共通ハードウェアと設定によって複数用途へ対応する。
- Node同士の直接通信ではなく、Hub中心の構成とする。
- 一時的な通信断を許容し、EVENTはACKと再送で到達性を補う。
- 通信状態とデバイス登録情報を分離する。
- 親機再起動後もデバイス登録情報はSQLiteから復元し、通信状態のみ未確認から再判定する。

---

## 11. 関連文書

- `docs/parent-overview-design.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/cdc_communication_spec.md`
