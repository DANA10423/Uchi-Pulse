# 家庭内ゆるコミュニケーションツール プログラム設計書

## 1. 文書概要

Uchi-Pulseは、家庭内で発生する簡単な連絡・状態共有を物理入力中心の簡単な操作で行う「家庭内ゆるコミュニケーションツール」である。

- 子機: Raspberry Pi Pico W / Pico 2 W
- 親機: Raspberry Pi Zero W / Zero 2 W
- 通常通信: 家庭内Wi-Fi / UDP
- 設定・保守: USB CDC

子機はAction IDの意味を解釈せず、親機がAction IDから対象者、状態変更、Web表示、通知を解釈する。

---

## 2. 子機設計

GPIO入力から `OFF_TO_ON` / `ON_TO_OFF` / `CLICK` / `DOUBLE_CLICK` / `LONG_PRESS` のInput Eventを生成し、`GPIO + Input Event + Action ID` の設定に従ってAction IDをUDP EVENTで送信する。

操作判定設定:

| 設定項目 | デフォルト |
|---|---:|
| `double_click_interval_ms` | 400 ms |
| `long_press_threshold_ms` | 1000 ms |

両設定はUSB CDCで取得・変更・永続保存できる。

---

## 3. 親機Action設計

### 3.1 Action本体

Action本体は以下を持つ。

- `action_id`
- `action_name`
- `target_type`
- `target_family_id`
- `web_message`（任意）
- `enabled`

`target_type` は以下の3種類とする。

| target_type | 意味 | target_family_id |
|---|---|---|
| `FAMILY` | 特定の家族1人 | 必須 |
| `ALL_FAMILIES` | 有効な家族全員 | `NULL` |
| `COMMON` | 家族に属さない共通状態 | `NULL` |

`ALL_FAMILIES` はAction実行時点で `families.enabled = 1` の家族全員へ展開する。家族の追加・無効化後もAction定義は変更せず、その時点の有効家族を対象とする。

特定家族向けの `FAMILY` Actionは対象家族ごとに別Action IDを登録する。`ALL_FAMILIES` Actionは1つのAction IDで有効家族全員を対象にできる。`COMMON` Actionは対象家族を持たない。

### 3.2 状態変更の分離

Actionは状態を必ず変更するものとはしない。状態変更を `action_state_changes` としてAction本体から分離し、1つのActionに0件以上の状態変更を定義できる。

```text
Action
  ├─ 対象: 特定家族 / 有効家族全員 / 共通
  ├─ 状態変更 0..n
  ├─ Web表示 任意
  └─ スマートフォン通知 任意
```

これにより、通常Action、複数状態を変更するAction、全員対象Action、通知専用Actionを同一モデルで扱う。

### 3.3 基本Actionパターン

初期版では次の12種類を基本Actionパターンとする。

| Action | target_type | 状態変更 |
|---|---|---|
| ご飯通知 | `FAMILY` / `ALL_FAMILIES` | `MEAL_NOTICE=ON` |
| ご飯通知クリア | `FAMILY` / `ALL_FAMILIES` | `MEAL_NOTICE=OFF` |
| おやつ通知 | `FAMILY` / `ALL_FAMILIES` | `SNACK_NOTICE=ON` |
| おやつ通知クリア | `FAMILY` / `ALL_FAMILIES` | `SNACK_NOTICE=OFF` |
| 食事通知クリア | `FAMILY` / `ALL_FAMILIES` | `MEAL_NOTICE=OFF`, `SNACK_NOTICE=OFF` |
| HELP通知 | `FAMILY` / `ALL_FAMILIES` | `HELP_NOTICE=ON` |
| HELP通知クリア | `FAMILY` / `ALL_FAMILIES` | `HELP_NOTICE=OFF` |
| 入室OK | `FAMILY` | `ENTRY_PERMISSION=OK` |
| 入室NG | `FAMILY` | `ENTRY_PERMISSION=NG` |
| 会議中 | `FAMILY` | `ENTRY_PERMISSION=MEETING` |
| ポスト投函 | `COMMON` | `MAILBOX=ON` |
| ポスト投函解除 | `COMMON` | `MAILBOX=OFF` |

「食事通知クリア」は対象となる家族のご飯通知とおやつ通知を同時に解除する。`ALL_FAMILIES` の場合は、有効な家族全員について両状態を解除する。

---

## 4. 入室問い合わせAction

部屋の外にいる家族から、部屋にいる対象家族へ「入室してよいか」等を問い合わせる用途として、状態変更なしの通知専用Actionを作成できる。

入室問い合わせActionの基本ルール:

- `target_type = FAMILY`
- 対象家族ごとに別Action ID
- 状態変更は0件
- `ENTRY_PERMISSION` は変更しない
- スマートフォン通知を有効化できる
- 通知先家族は複数設定可能
- 通知メッセージをActionごとに設定可能
- 同一対象者に複数の問い合わせActionを作成可能

例:

```text
父への入室問い合わせ
  状態変更: なし
  通知先: 父
  通知メッセージ: 今、部屋に入ってもいい？

父へ少し話したい
  状態変更: なし
  通知先: 父
  通知メッセージ: 少し話したいです
```

問い合わせを受けた対象家族が、自分の端末から `入室OK` / `入室NG` / `会議中` Actionを実行することで入室可否状態を変更する。

入室問い合わせは基本Actionパターンの固定種類として数を限定せず、必要な数だけ作成可能な通知専用Actionとして扱う。

---

## 5. 通知設計

Actionの対象家族と通知先家族は別概念とする。

通知設定はActionごとに以下を持つ。

- 通知有無
- 通知メッセージ
- 通知先家族 0..n

`ALL_FAMILIES` はActionの状態変更対象を表すものであり、スマートフォン通知先を自動的に全員へ変更するものではない。通知先は `action_notification_targets` で独立して設定する。

通知メッセージはActionごとに設定可能とする。LINE / Slack等の実際の送信先は家族ごとの通知先設定から解決する。

外部通知失敗はUDP ACK、EVENT履歴保存、状態変更に影響させない。

---

## 6. 処理概要

```text
[子機]
GPIO
 ↓
Input Event
 ↓
Action ID
 ↓ UDP EVENT

---------------- 責務境界 ----------------

[親機]
Action ID
 ↓
Action本体取得
 ↓
target_type解決
 ├─ FAMILY       → target_family_id の家族1人
 ├─ ALL_FAMILIES → families.enabled = 1 の家族全員
 └─ COMMON       → 家族対象なし
 ↓
EVENT履歴保存
 ↓
状態変更 0..n件を対象へ適用
 ↓
Web表示（定義されている場合）
 ↓
通知設定評価
 ↓
通知メッセージ + 通知先家族
 ↓
LINE / Slack等の通知機能
```

---

## 7. 設計原則

- 子機はAction IDを不透明な識別子として扱う。
- Actionの意味と対象解決は親機で管理する。
- `FAMILY` は特定家族1人を対象とし、対象家族ごとに別Action IDを持つ。
- `ALL_FAMILIES` は実行時点の有効家族全員を対象とし、`target_family_id` は持たない。
- `COMMON` は家族単位ではない共通状態を対象とする。
- Actionと状態変更を分離する。
- 1 Actionは0..n件の状態変更を持つ。
- 状態変更なしの通知専用Actionを許容する。
- Action対象家族と通知先家族を分離する。
- `ALL_FAMILIES` と通知先全員は同義ではない。
- 通知メッセージはActionごとに設定できる。
- 入室問い合わせActionは複数作成でき、入室可否状態を直接変更しない。

---

## 8. 関連仕様

- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/parent-overview-design.md`
- `docs/parent-web-status-design.md`
