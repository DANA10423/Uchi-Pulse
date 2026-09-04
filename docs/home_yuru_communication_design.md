# 家庭内ゆるコミュニケーションツール プログラム設計書

## 1. 文書概要

### 1.1 目的
本システムは、家庭内で発生する簡単な連絡・状態共有を、物理入力を中心とした簡単な操作で行う「家庭内ゆるコミュニケーションツール」である。

子機からActionを発生させ、家庭内LAN上の親機がActionの意味解釈、状態、履歴、Web表示、通知を一元管理する。

### 1.2 基本方針
- 子機: Raspberry Pi Pico W / Pico 2 W。
- 親機: Raspberry Pi Zero W / Zero 2 W。
- 子機と親機は家庭内Wi-Fiで通信する。
- 子機設定にWeb UIは使用せず、USB CDCで設定する。
- 子機と親機の責務を明確に分離する。
- 子機はAction IDの意味、対象者、Web表示メッセージ、スマートフォン通知先を解釈しない。
- 親機はAction IDを解釈し、対象者、状態変更、Web表示、通知を管理する。

---

## 2. 子機設計

### 2.1 責務
子機はGPIO入力の状態変化を検出し、設定されたAction IDを親機へ通知する汎用I/Oノードとする。

ボタン、スイッチ、ポスト投函検出センサー等を機能別に区別せず、GPIOデジタル入力として扱う。

### 2.2 入力イベント
GPIO入力は状態そのものではなくエッジをAction発生の契機とする。

- OFF → ON
- ON → OFF

各GPIOの各エッジに独立してAction IDを割り当てられる。

```text
GPIO入力
   ↓
エッジ検出（OFF→ON / ON→OFF）
   ↓
設定されたAction IDを取得
   ↓
親機へEVENT送信
```

### 2.3 子機CDC設定項目
Action発生に関する子機設定は次の3項目を一組とする。

| 項目 | 内容 |
|---|---|
| GPIO | 監視対象GPIO番号 |
| edge | `OFF_TO_ON` または `ON_TO_OFF` |
| action_id | エッジ発生時に親機へ通知するAction ID |

例:

```text
GPIO 5 / OFF_TO_ON / Action ID 1
GPIO 5 / ON_TO_OFF / Action ID 2
```

子機はAction IDの意味を解釈しない。

### 2.4 子機に保持しない情報
次の情報は親機側の責務であり、子機のAction割当設定には保持しない。

- Action名・Action種別の意味
- Actionの対象家族
- 対象者の表示名
- Web表示メッセージ
- 親機内部状態の変更内容
- スマートフォン通知有無
- スマートフォン通知先
- LINE / Slack等の実送信先情報

---

## 3. 親機設計

### 3.1 責務
親機は子機から受信したAction IDを解釈し、Action定義に従って状態管理、履歴保存、Web表示を行う。スマートフォン通知はAction定義とは分離した通知設定に従って処理する。

### 3.2 初期Action
初期版では次のActionを対象とする。

| Action | 内容 |
|---|---|
| ご飯通知 | ご飯通知状態を設定 |
| ご飯通知クリア | ご飯通知状態を解除 |
| 入室OK | 入室可能状態を設定 |
| 入室NG | 入室不可状態を設定 |
| 会議中 | 会議中状態を設定 |
| ポスト投函 | ポスト投函状態を設定 |
| ポスト投函解除 | ポスト投函状態を解除 |

「会議中」の解除専用Actionは設けず、`入室OK` によって状態を変更する。

### 3.3 Action定義
Action定義は親機データベースに登録する。

Actionには少なくとも以下の概念を持たせる。

- Action ID
- Action種別
- Action名
- 対象家族（種別により必要な場合）
- Web表示メッセージ
- 状態変更内容

Web表示メッセージにはシステムのデフォルト値を用意し、親機のCDC設定で変更可能とする。

### 3.4 対象者
Action種別によって対象家族が必要かどうかを判定する。

- 家族に対するAction種別: `target_family_id` を使用する。
- 家族を対象としない共通Action種別: `target_family_id` を使用しない。

現時点でポスト投函系は共通対象として扱う。

ご飯通知系・入室系を含め、各Action種別が家族対象か共通対象かの最終分類はAction種別定義として管理する。DB上の `target_family_id` はNULLを許容し、Action種別のルールによって必須／不要を検証する。

「家」「共通」等を表すダミー家族レコードは作成しない。

対象者の表示名はActionへ文字列として保存せず、家族マスタの `display_name` を参照する。

### 3.5 通知設定
スマートフォン通知はAction定義から分離して管理する。

通知設定では少なくとも次を管理する。

- 対象Action ID
- 通知有無
- 通知先家族

Actionの「対象者」と通知の「通知先」は別概念とする。

LINE / Slack等の実際の送信先情報もActionには保持せず、家族に紐づく通知先情報として別管理する。

```text
Action
  ├─ action_id
  ├─ action_type
  ├─ target_family_id
  ├─ web_message
  └─ 状態変更
       ↓
通知設定
  ├─ action_id
  ├─ 通知有無
  └─ 通知先 family_id
       ↓
家族通知先
  ├─ family_id
  ├─ 通知方式（LINE / Slack等）
  └─ 実送信先情報
```

---

## 4. 子機・親機の責務境界

| 情報・処理 | 子機 | 親機 |
|---|---|---|
| GPIO監視 | ○ | × |
| OFF→ON / ON→OFF検出 | ○ | × |
| GPIO+edgeへのAction ID割当 | ○ | × |
| Action IDの意味解釈 | × | ○ |
| Action対象者 | × | ○ |
| 対象者表示名 | × | ○ |
| Web表示メッセージ | × | ○ |
| 状態変更 | × | ○ |
| イベント履歴 | × | ○ |
| スマホ通知有無 | × | ○ |
| 通知先家族 | × | ○ |
| LINE / Slack等の送信先 | × | ○ |

この責務境界を基本とし、子機側の設定と親機側のAction・通知設定を混在させない。

---

## 5. 処理概要

```text
[子機]
GPIO
  ↓
OFF→ON / ON→OFF
  ↓
Action ID
  ↓ UDP EVENT

---------------- 責務境界 ----------------

[親機]
Action ID
  ↓
Action定義参照
  ├─ Action種別
  ├─ 対象家族（必要な種別のみ）
  ├─ Web表示メッセージ
  └─ 状態変更
  ↓
履歴保存・メモリ状態更新・Web表示
  ↓
通知設定参照
  ├─ 通知有無
  └─ 通知先家族
  ↓
必要な場合のみLINE / Slack等へ通知
```

---

## 6. 関連仕様

- `docs/cdc_communication_spec.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-database-design.md`
- `docs/parent-overview-design.md`
- `docs/parent-web-status-design.md`

Actionの詳細実装方式、Action IDの具体的な採番、Action種別ごとの対象者要否、通知サービス固有仕様は詳細設計で定義する。