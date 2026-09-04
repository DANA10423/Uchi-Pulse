# 親機 Web状態表示 設計書

## 1. 目的

親機Web UIにおける状態表示の基本方針を定義する。

Web表示は親機が管理する情報を使用する。子機はWeb表示メッセージや対象者表示名を保持・生成しない。

---

## 2. 表示情報の取得元

| 表示情報 | 取得元 |
|---|---|
| デバイス名・分類 | 親機DB `devices` |
| 家族表示名 | 親機DB `families.display_name` |
| Action表示メッセージ | 親機DB `actions.web_message` |
| 通信状態 | 親機メモリ `DeviceState` |
| 入室可否 | 親機メモリ `DeviceState.room_access_status` |

Actionの対象者名を子機EVENTに表示文字列として持たせず、親機がFamily IDから表示名を解決する。

---

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

最終確認時刻や最終イベントは状態一覧へ常時表示しない。

---

## 4. Actionメッセージ

ActionのWeb表示メッセージは親機DBのAction定義に保持する。

初期値をシステム側で用意し、親機へのUSB CDC設定によって変更可能とする。

Web UIは子機から受信したAction IDを直接表示用文字列として解釈せず、親機のAction定義を参照する。

```text
子機EVENT
  Action ID
      ↓
親機Action定義
  ├─ 対象家族
  └─ web_message
      ↓
Web表示
```

---

## 5. 対象者表示

家族対象Actionでは `target_family_id` から家族マスタを参照し、`display_name` を表示に使用する。

ポスト投函等の共通対象Actionでは対象家族を持たないため、家族名を付加しない。

対象家族が必要かどうかはAction種別によって親機側で判定する。

---

## 6. 通知との分離

Web表示とスマートフォン通知は別処理とする。

Actionの `web_message` はWeb表示に使用する。スマートフォンへ通知するかどうか、誰へ通知するかはActionとは別の通知設定を参照する。

したがって、Web表示対象者とスマートフォン通知先が同一である必要はない。

---

## 7. 責務境界

子機:

- GPIO監視
- OFF→ON / ON→OFF検出
- Action ID発生
- UDP EVENT送信

親機:

- Action ID解釈
- 対象家族解決
- 表示名解決
- Webメッセージ取得
- 現在状態更新
- Web表示
- 通知設定評価

この責務を混在させない。

---

## 8. 関連仕様

- `docs/home_yuru_communication_design.md`
- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/cdc_communication_spec.md`