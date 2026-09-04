# Uchi-Pulse Codex プログラム作成 作業指示書

作成日: 2026-09-04\
対象リポジトリ: `DANA10423/Uchi-Pulse`\
対象ブランチ: `main`

## 1. 目的

Uchi-Pulse の現在確定している設計書および GitHub Issue
を正本として、Codex が段階的に実装を進めるための作業指示書である。

本作業では、一度に全機能を実装しない。依存関係に従って PHASE
ごとに実装し、各 PHASE
でテスト・レビュー・設計との整合確認を行ってから次へ進むこと。

## 2. 最重要ルール

1.  **実装前にリポジトリ全体を確認すること。**
2.  `docs/` の設計書と対象 Issue を必ず読んでから実装すること。
3.  仕様が不明な場合は推測して実装しないこと。
4.  設計書と Issue
    に矛盾がある場合は実装を停止し、矛盾点を報告すること。
5.  Issue に書かれていない新機能を独自判断で追加しないこと。
6.  Rust
    の内部構造は適切に設計してよいが、外部仕様を勝手に変更しないこと。
7.  各機能の責務境界を守ること。
8.  各 PHASE で自動テストを追加すること。
9.  既存テストを壊さないこと。
10. PHASE
    完了時に、変更ファイル、実装内容、テスト結果、残課題を報告すること。
11. Git commit / push / Issue close
    は、明示的な指示がない限り勝手に行わないこと。

## 3. 実装前に必ず確認する資料

最低限、以下を確認すること。

-   `docs/uchi-pulse-system-overview.md`
-   `docs/home_yuru_communication_design.md`
-   `docs/cdc_communication_spec.md`
-   `docs/parent_child_udp_communication_spec.md`
-   `docs/parent-overview-design.md`
-   `docs/parent-database-design.md`
-   `docs/parent-web-status-design.md`

GitHub Issue:

-   #1 親機（Raspberry Pi Zero）のCDC通信を実装する
-   #2 子機（Pico W / Pico 2 W）のCDC通信を実装する
-   #3 GPIO入力に割り当てるActionを定義・実装する
-   #4 親機・子機間のUDP通信処理を実装する
-   #5 親機: SQLiteデータベース実装
-   #6 親機: 子機通信状態管理機能の実装
-   #8 親機Web状態一覧画面を実装する
-   #10 親機からSlack／LINEへ家族向け通知を送信する機能を実装する

Issue #7「簡易スケジュール機能」は初期実装の必須対象外とする。

## 4. 実装上の基本アーキテクチャ

### 子機

Raspberry Pi Pico W / Pico 2 W を対象とする。

主責務:

-   GPIO入力監視
-   Input Event 判定
-   GPIO + Input Event → Action ID の対応
-   USB CDC による設定
-   UDP HELLO / HEARTBEAT / EVENT
-   EVENT ACK待ち・再送
-   Action ID の意味は解釈しない

Input Event:

-   `OFF_TO_ON`
-   `ON_TO_OFF`
-   `CLICK`
-   `DOUBLE_CLICK`
-   `LONG_PRESS`

初期値:

-   `double_click_interval_ms = 400`
-   `long_press_threshold_ms = 1000`

両方とも CDC から変更・永続化可能とする。

### 親機

Raspberry Pi Zero W / Zero 2 W を対象とする。

主責務:

-   UDP受信
-   子機通信状態管理
-   EVENT重複排除
-   EVENT履歴保存
-   Action ID 解釈
-   状態変更
-   Web表示用情報生成
-   外部通知要求生成
-   SQLite永続化
-   USB CDC 管理
-   Web UI
-   Slack / LINE Provider

## 5. Action設計の重要事項

子機は Action ID のみを扱う。

親機側 Action 本体:

-   `action_id`
-   `action_name`
-   `target_type`
-   `target_family_id`
-   `web_message`
-   `enabled`

状態変更は Action 本体から分離し、`action_state_changes` に 0..n
件保持する。

この構造により以下を扱う。

-   単一状態変更
-   複数状態一括変更
-   状態変更なしの通知専用Action

初期基本パターンは12種類:

1.  ご飯通知
2.  ご飯通知クリア
3.  おやつ通知
4.  おやつ通知クリア
5.  食事通知クリア
6.  HELP通知
7.  HELP通知クリア
8.  入室OK
9.  入室NG
10. 会議中
11. ポスト投函
12. ポスト投函解除

`食事通知クリア` は以下2状態を同時に変更する。

-   `MEAL_NOTICE = OFF`
-   `SNACK_NOTICE = OFF`

FAMILY Action は対象家族ごとに別 Action ID を持つ。

### 入室問い合わせ

入室問い合わせは通知専用Actionとして扱う。

-   `target_type = FAMILY`
-   `action_state_changes = 0件`
-   `ENTRY_PERMISSION` を変更しない
-   通知メッセージをActionごとに設定可能
-   通知先家族を複数指定可能
-   同一対象家族へ複数種類作成可能

問い合わせを受けた家族が別途「入室OK / 入室NG /
会議中」を実行して状態を変更する。

------------------------------------------------------------------------

# PHASE 0 --- 現状調査と実装計画

## 作業

リポジトリ全体を調査し、以下を報告する。

-   Cargo workspace 構成
-   既存 crate / application
-   親機コードの有無
-   子機コードの有無
-   共通コードの有無
-   テスト構成
-   CI構成
-   各 Issue の現在の実装状況
-   設計書と実装の差分
-   PHASE 1以降で新規作成・変更予定のファイル

## 完了条件

**この PHASE では原則として実装しない。**

調査結果と具体的な実装計画を提示し、重大な仕様不足や矛盾がないことを確認する。

------------------------------------------------------------------------

# PHASE 1 --- 共通データモデル・プロトコル基盤

対象: #1 #2 #3 #4 の共通部分

## 実装内容

CDC / UDP / Action で共有できるデータモデルを整理する。

最低限:

-   CDC Request / Response
-   UDP `HELLO`
-   UDP `HEARTBEAT`
-   UDP `EVENT`
-   UDP `ACK`
-   `event_id`
-   `action_id`
-   Input Event enum
-   Action関連型
-   通信設定値

UDP EVENT:

``` json
{
  "type": "EVENT",
  "device_id": "node-01",
  "event_id": "boot-id-00000001",
  "action_id": 10
}
```

UDP層では GPIO / Input Event / Action の意味を解釈しない。

## テスト

-   JSON serialize / deserialize
-   正常値
-   必須フィールド欠落
-   不正メッセージ種別
-   event_id保持
-   Action ID保持

## 完了条件

共通モデルが親機・子機の両方から利用でき、設計書のJSON仕様と一致すること。

------------------------------------------------------------------------

# PHASE 2 --- 親機SQLite基盤

対象: Issue #5

## 実装内容

`docs/parent-database-design.md` に従い SQLite を実装する。

最低限のテーブル:

-   `devices`
-   `families`
-   `actions`
-   `action_state_changes`
-   `action_notification_settings`
-   `action_notification_targets`
-   `family_notification_destinations`
-   `events`

重要事項:

-   Action と状態変更を分離する
-   1 Action → 0..n state changes
-   EVENT重複登録を防止する
-   通信状態はDBへ保存しない
-   最終正常受信時刻はDBへ保存しない

## テスト

-   DB初期化
-   CRUD
-   FK制約
-   0件状態変更Action
-   複数状態変更Action
-   EVENT重複登録防止
-   FAMILY Action
-   通知先複数設定

------------------------------------------------------------------------

# PHASE 3 --- 親機メモリ状態管理

対象: Issue #6

## 状態

-   `INITIAL_WAIT`
-   `ONLINE`
-   `OFFLINE`

## 実装内容

親機起動時:

1.  DBから `enabled = 1` のデバイスを取得
2.  状態管理対象を構築
3.  全対象を `INITIAL_WAIT`
4.  最終正常受信時刻は未設定

正常な以下の受信で `ONLINE`:

-   HELLO
-   HEARTBEAT
-   EVENT

OFFLINE判定:

-   初期値 180秒
-   `INITIAL_WAIT → OFFLINE`
-   `ONLINE → OFFLINE`
-   `OFFLINE → ONLINE` 復帰可能

通信状態はSQLiteへ永続化しない。

## テスト

全状態遷移をテストする。

------------------------------------------------------------------------

# PHASE 4 --- 親機UDP通信

対象: Issue #4

## 実装内容

UDP / JSON:

-   HELLO
-   HEARTBEAT
-   EVENT
-   ACK

EVENTのみACK対象。

初期設定:

-   ACK待ち: 60秒
-   再送: 3回
-   HEARTBEAT: 180秒
-   OFFLINE判定: 180秒

親機 EVENT 処理順:

``` text
EVENT受信
 ↓
JSON/device_id/event_id検証
 ↓
重複チェック
 ↓
EVENT履歴保存
 ↓
Action処理
 ↓
ACK送信
 ↓
外部通知処理
```

重複EVENTでは:

-   履歴再登録しない
-   Action再実行しない
-   ACKは返す

外部通知完了をACK条件にしない。

親機起動時には、子機へHELLO送信を促すブロードキャスト要求を送信可能な構造とする。

## テスト

-   各メッセージ受信
-   不正JSON
-   未登録device
-   EVENT重複
-   ACK
-   状態管理連携
-   Action呼び出し順序

------------------------------------------------------------------------

# PHASE 5 --- Action実行エンジン

対象: Issue #3

## 実装内容

EVENT の `action_id` から親機が Action を取得し処理する。

処理:

1.  Action取得
2.  enabled確認
3.  target解決
4.  `action_state_changes` 取得
5.  0..n件の状態変更適用
6.  `web_message` 処理
7.  通知設定評価
8.  通知要求生成

必ず以下をテストする。

-   ご飯通知
-   ご飯通知クリア
-   おやつ通知
-   おやつ通知クリア
-   食事通知クリア
-   HELP通知
-   HELP通知クリア
-   入室OK
-   入室NG
-   会議中
-   ポスト投函
-   ポスト投函解除
-   状態変更なしの入室問い合わせ

入室問い合わせでは `ENTRY_PERMISSION`
が変化しないことを明示的にテストする。

------------------------------------------------------------------------

# PHASE 6 --- 子機GPIO / Input Event / Action ID

対象: Issue #2 #3

## 実装内容

GPIO入力から以下を判定する。

-   OFF_TO_ON
-   ON_TO_OFF
-   CLICK
-   DOUBLE_CLICK
-   LONG_PRESS

設定:

``` text
GPIO + Input Event + Action ID
```

CLICK と DOUBLE_CLICK は同じ操作から重複発生させない。

初期値:

-   double click = 400ms
-   long press = 1000ms

判定後は Action ID を UDP EVENT生成処理へ渡す。

子機は Action の意味を解釈してはならない。

## テスト

ハードウェア非依存の判定ロジックを可能な限り分離して単体テストする。

------------------------------------------------------------------------

# PHASE 7 --- 子機UDP通信

対象: Issue #4

## 実装内容

-   HELLO
-   HEARTBEAT
-   EVENT
-   ACK受信
-   ACKタイムアウト
-   EVENT再送
-   `boot_id + sequence` event_id

同一EVENT再送では同じ event_id を使用する。

`boot_id` は連続起動で容易に衝突しない方式とする。

## テスト

-   event_id生成
-   sequence増加
-   再送時ID維持
-   ACK受信
-   タイムアウト
-   最大再送回数

------------------------------------------------------------------------

# PHASE 8 --- CDC共通処理と子機CDC

対象: Issue #2

## 仕様

UTF-8 JSON + LF終端。

Request:

-   `version`
-   `request_id`
-   `command`
-   `params`

Response:

-   `status = ok/error`
-   Requestと同じ `request_id`

基本コマンド:

-   `get_info`
-   `get_config`
-   `set_config`
-   `get_status`
-   `get_inputs`
-   `get_outputs`
-   `reboot`
-   `factory_reset`

子機では以下をCDCから設定・永続化可能とする。

-   GPIO + Input Event + Action ID
-   double_click_interval_ms
-   long_press_threshold_ms

再起動後に復元する。

共通CDC部分は可能な限り `no_std` 対応を維持する。

------------------------------------------------------------------------

# PHASE 9 --- 親機CDC

対象: Issue #1

## 実装内容

Linux USB Gadget / CDC I/O と共通CDCプロトコルを接続する。

親機CDC管理対象:

-   Action本体
-   Action状態変更
-   家族情報
-   Web表示設定
-   通知設定

親機CDCでは子機の GPIO + Input Event + Action ID 設定を扱わない。

UDP処理との疎結合を維持する。

------------------------------------------------------------------------

# PHASE 10 --- 親機Web状態一覧

対象: Issue #8

## 家族用端末

表示:

-   表示名
-   通信状態
-   入室可否

通信状態:

-   未確認
-   オンライン
-   オフライン

入室可否:

-   未設定
-   OK
-   NG
-   会議中

## 設備用端末

表示:

-   表示名
-   通信状態

入室可否は表示しない。

## 常時一覧に表示しない

-   ご飯通知
-   おやつ通知
-   HELP通知
-   最終正常受信時刻
-   最新イベント
-   IPアドレス
-   通信診断情報

Web表示には `actions.web_message` を使用する。

`web_message = NULL` はWeb非表示。

通知用 `notification_message` と混同しない。

GPIO番号や Input Event をWeb UIで解釈しない。

復元対象状態はEVENT履歴から復元する。

------------------------------------------------------------------------

# PHASE 11 --- Slack / LINE 外部通知

対象: Issue #10

## 責務

Action処理側:

-   通知要否判定
-   notification_message取得・展開
-   通知先 family_id 決定

Notification Service:

-   family_id → 実送信先解決
-   Provider形式変換
-   HTTP送信
-   APIエラー
-   timeout
-   認証情報管理

Providerは Slack / LINE 固有処理を分離する。

重要:

-   LINE Notifyは使用しない
-   現行公式LINE APIを使用する
-   Slackも公式通知手段を使用する
-   秘密情報をソース・ログへ出さない
-   外部通知失敗でUDP ACKを失敗させない
-   EVENT履歴や状態更新をロールバックしない
-   永続通知キュー・自動再送は初期必須要件外

少なくとも Slack または LINE の一方で実送信可能な状態まで実装する。

------------------------------------------------------------------------

# PHASE 12 --- 結合試験

以下のEnd-to-End経路を確認する。

``` text
GPIO
 ↓
Input Event
 ↓
Action ID
 ↓
UDP EVENT
 ↓
親機受信
 ↓
重複排除
 ↓
EVENT履歴
 ↓
Action実行
 ↓
状態更新
 ↓
ACK
 ↓
Web表示 / 外部通知
```

確認シナリオ:

-   ご飯通知
-   食事通知クリア
-   HELP
-   入室OK / NG / 会議中
-   入室問い合わせ
-   ポスト投函
-   EVENT再送
-   重複EVENT
-   子機OFFLINE
-   OFFLINEからONLINE復帰
-   親機再起動後の状態復元
-   外部通知失敗時も基本処理継続

------------------------------------------------------------------------

# PHASE 13 --- 最終品質確認

実行可能な範囲ですべて実施する。

-   `cargo fmt --check`
-   `cargo clippy`
-   `cargo test`
-   target別build/check
-   Web側lint/test/build
-   未使用コード確認
-   panic / unwrap の妥当性確認
-   ログへの秘密情報出力確認
-   設計書との最終差分確認

必要な場合のみREADMEや開発者向けドキュメントを更新する。

------------------------------------------------------------------------

## 6. PHASEごとのCodex報告形式

各PHASE終了時、以下の形式で報告すること。

``` markdown
## PHASE X 完了報告

### 実装した内容
- ...

### 変更ファイル
- ...

### 追加・変更したテスト
- ...

### テスト結果
- cargo test: ...
- cargo clippy: ...
- その他: ...

### 設計書 / Issueとの整合
- ...

### 未解決事項
- なし
```

未解決事項がある場合は「なし」とせず具体的に記載し、独断で次PHASEへ進まない。

## 7. 実装時の禁止事項

-   Action IDを固定値としてコードへ埋め込んで意味を決定しない
-   子機にAction名や意味を持たせない
-   UDP層にAction業務ロジックを入れない
-   Web UIにGPIO/Input Event解釈を入れない
-   `web_message` と `notification_message` を共用しない
-   通信状態をSQLiteへ永続化しない
-   外部通知完了をEVENT ACK条件にしない
-   LINE Notifyを使用しない
-   秘密情報をGit管理しない
-   設計外機能を勝手に追加しない

## 8. Codexへの開始指示

まず **PHASE 0だけ** を実施してください。

リポジトリ、`docs/`、Issue
#1〜#10（#9は誤作成、#7は初期必須対象外）を確認し、現在の実装状況と設計との差分を調査してください。

この段階では大規模な実装変更を開始しないでください。

PHASE 0完了報告として、以下を提示してください。

1.  現在のリポジトリ構成
2.  既存実装の概要
3.  各対象Issueの実装状況
4.  設計書との不整合
5.  不明仕様
6.  PHASE 1で変更・追加予定のファイル
7.  PHASE構成の修正が必要ならその理由

重大な問題がなければ、PHASE
1へ進める状態かどうかを最後に明示してください。
