# 親機 Web状態表示 設計書

## 1. 目的
親機Web UIにおける状態表示の基本方針を定義する。

Web表示は親機が管理する情報を使用する。子機はWeb表示メッセージや対象者表示名を保持・生成しない。

## 2. 表示情報の取得元
| 表示情報 | 取得元 |
|---|---|
| デバイス名・分類 | 親機DB `devices` |
| 家族表示名 | 親機DB `families.display_name` |
| Action表示メッセージ | 親機DB `actions.web_message` |
| Action対象家族 | 親機DB `actions.target_family_id` |
| 通信状態 | 親機メモリ `DeviceState` |
| 入室可否 | 親機メモリ `DeviceState.room_access_status` |

## 3. 状態一覧
通信状態と入室可否は別項目として表示する。

通信状態:
- 未確認 (`INITIAL_WAIT`)
- オンライン (`ONLINE`)
- オフライン (`OFFLINE`)

入室可否:
- 未設定 (`UNSET`)
- OK (`OK`)
- NG (`NG`)
- 会議中 (`MEETING`)

入室可否は家族用端末にのみ表示する。

ご飯通知、おやつ通知、HELP通知は一覧の常時表示列には追加しない。Actionメッセージ等の別表示で扱う。

最終確認時刻、最終イベント、IPアドレス、通信診断情報は常時表示しない。

## 4. Actionメッセージ
`actions.web_message` はWeb表示専用とする。

`web_message` はNULLを許容する。入室問い合わせ等の通知専用ActionではWeb表示を行わない設定を可能とする。

FAMILY Actionの `{target}` は `target_family_id` に対応する `families.display_name` で展開する。

```text
子機EVENT(Action ID)
 ↓
Action本体
 ├─ target_family_id
 └─ web_message
 ↓
web_message が定義されている場合のみWeb表示
```

## 5. 通知との分離
Web表示とスマートフォン通知は別処理とする。

- Web: `actions.web_message`
- 通知: `action_notification_settings.notification_message`

Web表示対象と通知対象が同一である必要はない。

## 6. 責務境界
子機:
- GPIO監視
- Input Event判定
- Action ID決定
- UDP EVENT送信

Input Event:
- `OFF_TO_ON`
- `ON_TO_OFF`
- `CLICK`
- `DOUBLE_CLICK`
- `LONG_PRESS`

親機:
- Action ID解釈
- 対象家族解決
- 状態変更適用
- Webメッセージ取得・展開
- Web表示
- 通知設定評価

Web UIはGPIO番号やInput Eventを解釈しない。

## 7. 関連仕様
- `docs/home_yuru_communication_design.md`
- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/cdc_communication_spec.md`
