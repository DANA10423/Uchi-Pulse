# Uchi-Pulse CDC通信仕様書

## 1. 目的

本仕様書はUchi-PulseにおけるUSB CDC通信の共通仕様と、親機・子機それぞれの設定責務を定義する。

通常運用時の親機・子機間通信はWi-Fi/UDPを使用し、CDCはPC等から直接管理・設定するためのローカル通信路とする。

---

## 2. 対象

- 親機: Raspberry Pi Zero W / Zero 2 W
- 子機: Raspberry Pi Pico W / Pico 2 W

通信プロトコルの基本形式は共通化するが、設定内容は親機と子機で異なる。両者の設定項目を混在させない。

---

## 3. 共通通信仕様

- USB CDCを使用する。
- UTF-8 JSONを使用する。
- 1メッセージ1行、LF (`\n`) 終端とする。
- 原則として要求・応答型とする。
- 初期プロトコルバージョンは `1` とする。
- UDP EVENTで使用するACK、再送、重複排除はCDCでは使用しない。

基本要求:

```json
{"version":1,"command":"get_status"}
```

成功応答:

```json
{"version":1,"result":"ok","data":{}}
```

エラー応答:

```json
{"version":1,"result":"error","error":{"code":"INVALID_PARAMETER","message":"invalid parameter"}}
```

---

## 4. 共通コマンド

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

コマンド名は共通とするが、`get_config` / `set_config` のデータ内容は親機と子機で異なる。

---

## 5. 子機CDC設定

### 5.1 責務

子機CDCでは、GPIO入力からどのAction IDを発生させるかを設定する。

ボタン、スイッチ、ポスト投函センサー等の物理用途による区別は行わず、GPIOデジタル入力のエッジとして扱う。

### 5.2 Action入力割当

1つの入力割当は次の3項目から構成する。

| 項目 | 内容 |
|---|---|
| `gpio` | GPIO番号 |
| `edge` | `OFF_TO_ON` / `ON_TO_OFF` |
| `action_id` | 発生させるAction ID |

同じGPIOについて `OFF_TO_ON` と `ON_TO_OFF` に別のAction IDを設定できる。

### 5.3 子機CDCで設定しない情報

以下は親機側の情報であり、子機へ設定しない。

- Action名・意味
- `target_type`
- Action対象家族
- 対象者表示名
- Web表示メッセージ
- 親機状態変更内容
- スマートフォン通知有無
- 通知先家族
- LINE / Slack等の実送信先

子機はAction IDを不透明な識別子として扱う。

---

## 6. 親機CDC設定

### 6.1 責務

親機CDCでは、親機DBに保持するAction定義や関連設定を管理対象とする。

### 6.2 Action定義

親機側ではAction IDごとに次の情報を管理する。

- `action_name`
- `target_type` (`FAMILY` / `COMMON`)
- `target_family_id`
- `web_message`
- `state_type`
- `state_value`
- `enabled`

`FAMILY` Actionでは `target_family_id` を必須とし、対象家族ごとに別Action IDを登録する。

同じAction内容でも対象家族が異なる場合は別Action IDとする。

```text
Action ID 4 = 父 / 入室NG
Action ID 5 = 母 / 入室NG
```

1つのAction IDに複数の対象家族は登録しない。

`COMMON` Actionでは `target_family_id = NULL` とする。

Web表示メッセージにはデフォルト値を用意し、親機CDCで変更可能とする。

### 6.3 家族情報

対象者の表示名はActionに直接保存せず、家族マスタで管理する。

- `family_id`
- `display_name`
- `enabled`

FAMILY Actionは `target_family_id` で家族マスタを参照する。

### 6.4 通知設定

スマートフォン通知設定はAction定義とは分離する。

- 対象Action ID
- 通知有無
- 通知先 `family_id`

Actionの対象家族とスマートフォン通知先家族は別概念とする。

LINE / Slack等の実送信先情報もActionには含めず、家族に紐づく通知先設定として管理する。

---

## 7. 親機と子機の設定境界

```text
[子機 CDC]
GPIO
 + Edge
 + Action ID
      ↓
通常運用時にUDP EVENTとしてAction ID送信

================ 責務境界 ================

[親機]
Action ID
      ↓
Action定義
 + target_type
 + target_family_id
 + Web表示
 + 状態変更
      ↓
通知設定
 + 通知有無
 + 通知先家族
```

この境界を維持し、親機用設定を子機設定データへ含めない。また子機のGPIO割当を親機Action定義へ含めない。

---

## 8. 設定反映

`set_config` は設定値を検証して永続保存する。原則として設定変更は次回起動時に反映する。

子機ではFlash等へ保存し、親機ではLinux上の永続ストレージ（SQLite等）へ保存する。

---

## 9. エラー

初期エラーコード:

- `INVALID_JSON`
- `UNSUPPORTED_VERSION`
- `INVALID_COMMAND`
- `INVALID_PARAMETER`
- `INVALID_CONFIG`
- `SAVE_FAILED`
- `OPERATION_FAILED`
- `NOT_SUPPORTED`

親機では `target_type` と `target_family_id` の整合性も設定検証対象とする。子機ではGPIO番号、`edge`、Action IDの形式等を検証対象とする。

---

## 10. 関連仕様

- `docs/home_yuru_communication_design.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`

Action IDはAction定義ごとに一意に割り当て、FAMILY Actionは対象家族ごとに別Action IDとする。