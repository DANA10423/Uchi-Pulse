# Uchi-Pulse 親機 概要設計書

## 1. 目的

親機（Raspberry Pi Zero W / Zero 2 W）は、子機とのUDP通信、Action解釈、状態管理、履歴保存、Web UI、スマートフォン通知を担当する中央管理装置である。

本書では親機の責務を定義する。子機GPIO監視およびGPIO+エッジへのAction ID割当は子機側の責務であり、本書の親機内部処理とは分離する。

---

## 2. 親機の主要責務

- 子機管理
- UDP HELLO / HEARTBEAT / EVENT受信
- 子機通信状態管理
- Action IDの解釈
- Action対象家族の解決
- Actionによる現在状態更新
- イベント履歴保存
- Web表示情報提供
- Action通知設定の評価
- 通知機能への通知要求引き渡し
- 親機自身のUSB CDC設定・保守

LINE / Slack等の外部サービスへの実送信処理は通知機能として分離する。

---

## 3. 子機との責務境界

子機はGPIOのOFF→ON / ON→OFFを検出し、子機CDCで設定されたAction IDをUDP EVENTとして親機へ送る。

```text
子機
GPIO + Edge
    ↓
Action ID
    ↓ UDP EVENT

--------------------------

親機
Action ID
    ↓
Action定義を参照
    ↓
状態・表示・通知判断
```

親機は、どのGPIO・どのエッジからActionが発生したかをAction定義の意味として保持しない。GPIO割当は子機設定である。

---

## 4. Action管理

Action定義は親機DBで管理する。

初期Action:

- ご飯通知
- ご飯通知クリア
- 入室OK
- 入室NG
- 会議中
- ポスト投函
- ポスト投函解除

ActionにはAction種別、名称、必要に応じた対象家族、Web表示メッセージ、状態変更内容を定義する。

対象家族が必要かどうかはAction種別で判定する。家族対象として定義されたAction種別では対象家族を必須とし、共通対象として定義されたAction種別では対象家族を使用しない。現時点でポスト投函系は共通対象とする。共通対象のためのダミー家族は作成しない。

対象家族の表示名は家族マスタから取得する。

---

## 5. 通知管理

スマートフォン通知設定はAction定義から分離する。

通知設定では、Actionごとの通知有無と通知先家族を管理する。Action対象者と通知先は別概念である。

家族ごとのLINE / Slack等の実送信先設定も別管理する。

```text
Action発生
  ↓
通知設定
  ├─ OFF → 終了
  └─ ON
       ↓
     通知先家族
       ↓
     通知機能へ通知要求
       ↓
     LINE / Slack等
```

---

## 6. 状態管理

親機稼働中の現在状態はメモリ上で管理する。

デバイスごとの基本状態:

- `device_id`
- `last_seen_at`
- 通信状態: `INITIAL_WAIT` / `ONLINE` / `OFFLINE`
- 入室可否: `UNSET` / `OK` / `NG` / `MEETING`

親機起動時はDBの有効デバイスを読み込み、通信状態を `INITIAL_WAIT` とする。入室可否はイベント履歴から最新値を復元する。

通信状態の基本遷移:

```text
INITIAL_WAIT --正常受信--> ONLINE
INITIAL_WAIT --オフライン判定時間超過--> OFFLINE
ONLINE       --正常受信--> ONLINE
ONLINE       --オフライン判定時間超過--> OFFLINE
OFFLINE      --正常受信--> ONLINE
```

正常受信とは通信仕様上有効なHELLO / HEARTBEAT / EVENT等を正常に解析・処理できた場合をいう。

---

## 7. データベース

SQLiteを使用する。

主な永続情報:

- デバイスマスタ
- 家族マスタ
- Action定義
- Action通知設定
- 通知先家族
- 家族の外部通知先設定
- EVENT履歴

通信状態等の現在値はDBの専用状態として保存しない。

詳細は `docs/parent-database-design.md` を参照する。

---

## 8. Web UI

親機Web UIはDBのマスタ情報とメモリ上の現在状態を組み合わせて表示する。

ActionのWeb表示メッセージは親機DBのAction定義から取得する。対象家族がある場合の表示名は家族マスタから取得する。

通信状態と入室可否は別項目として表示する。入室可否は家族用端末にのみ表示する。

詳細は `docs/parent-web-status-design.md` を参照する。

---

## 9. CDC

親機のUSB CDCは親機側設定・保守を行うために使用する。

親機CDCで扱うAction関連設定には、Action定義、家族情報、Web表示メッセージ、通知設定等が含まれる。

子機のGPIO+エッジ+Action ID割当は子機CDCで設定するものであり、親機CDCのAction定義とは分離する。

詳細は `docs/cdc_communication_spec.md` を参照する。

---

## 10. 関連仕様

- `docs/home_yuru_communication_design.md`
- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/parent-web-status-design.md`