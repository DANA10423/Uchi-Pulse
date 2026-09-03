# AGENTS.md

このファイルは、Uchi-Pulse リポジトリで作業する ChatGPT / Codex 等のエージェント向けの指示書です。

## 1. プロジェクト概要

Uchi-Pulse は、家庭内の物理ボタン・センサーから発生したイベントを、Wi-Fi 経由で親機へ集約するシステムです。

- `apps/uchi-pulse-node`: Raspberry Pi Pico W / Pico 2 W 向け子機ファームウェア
- `apps/uchi-pulse-hub`: Raspberry Pi Zero W / Zero 2 W 向け親機プログラム
- `docs/`: システム設計・通信仕様
- `.github/DEVELOPMENT_WORKFLOW.md`: Issue / PR / Codex を含む開発運用ルール
- `.github/ISSUE_TEMPLATE/`: Issue作成テンプレート
- `.github/pull_request_template.md`: Pull Requestテンプレート

## 2. 正式な情報源

本リポジトリでは、情報の役割を以下のように分ける。

- 要求・作業状態: GitHub Issue
- 正式なシステム設計・通信仕様: `main` ブランチの `docs/`
- 正式なソースコード: `main` ブランチ
- 実装差分・レビュー記録: Pull Request
- 検討過程: Chat等。ただし重要な決定事項はIssueまたは`docs/`へ反映する。

Chatだけに存在する決定を正式仕様として扱わないこと。

## 3. 作業開始時のルール

新規機能、設計変更、改善、不具合修正は原則としてGitHub Issueを起点にする。

実装開始前に対象Issueを読み、少なくとも以下を確認すること。

1. 目的
2. 背景
3. 対象
4. 決定事項
5. 参照仕様
6. 実装内容
7. 非対象
8. 完了条件
9. テスト・確認
10. 関連Issue / PR

Issueの情報が不足して実装判断が必要になる場合、推測で仕様を追加しない。既存仕様・コードから確定できない事項は、未確定事項として報告すること。

Issue運用の詳細は `.github/DEVELOPMENT_WORKFLOW.md` を参照する。

## 4. 仕様の参照

変更対象に応じて、`main` 上の該当仕様を確認すること。

- 親機・子機UDP通信: `docs/parent_child_udp_communication_spec.md`
- CDC管理通信: `docs/cdc_communication_spec.md`
- システム全体: `docs/uchi-pulse-system-overview.md`
- 子機・家庭内コミュニケーション設計: `docs/home_yuru_communication_design.md`

Issueと設計文書が矛盾する場合、独断でどちらかへ合わせない。差分を明示し、必要なら設計判断をIssueで確定してから`docs/`を更新する。

通信仕様、設定形式、データ形式、共通インターフェース等を変更する場合は、正式実装を完了扱いする前に対応する設計文書を更新すること。

## 5. Issue運用

IssueはChat / Work / Codex間の共通の作業契約として扱う。

新しいIssueは原則として `.github/ISSUE_TEMPLATE/` のテンプレートを使用する。

- 機能追加・設計変更・改善: `task.yml`
- 不具合: `bug.yml`

大きな作業は必要に応じて親Issueと子Issueへ分割する。

作業中に新しい設計判断が確定した場合、重要な内容をChatだけに残さず、対象Issueまたは正式な設計文書へ反映する。

Issueの完了条件を満たしていない状態でDone扱いしない。

## 6. Branch / Pull Request

`main` は安定した正式状態を保つ。

通常の実装作業はIssue単位のブランチで行うことを基本とする。

例:

```text
feature/12-cdc-node
fix/15-event-ack
docs/18-cdc-protocol
```

一つのPRには原則として一つのIssueに対応する論理的変更を含め、関係のない変更を混在させない。

PR作成時は `.github/pull_request_template.md` を使用し、少なくとも以下を記録する。

- 対応Issue
- 参照仕様
- 変更内容
- 非対象
- 設計との整合性
- 実行したテストと結果
- 変更ファイル
- 未確認事項・残課題

設計変更を伴う場合は、必要な`docs/`更新を同じ変更単位で追跡可能にする。

## 7. 開発ルール

- Rust 2024 edition を使用する。
- 可能な限り、ロジックをテスト可能なライブラリ部分へ分離する。
- 通信JSONの既存フィールド名・メッセージ種別を、仕様変更なしに変更しない。
- Wi-Fi認証情報や実機固有の秘密情報をコミットしない。
- 実装の都合だけを理由に既存仕様を暗黙に変更しない。
- 仕様外の追加機能を独断で実装しない。
- 未実施のテストや実機確認を、実施済みとして報告しない。

## 8. CDC通信の共通方針

CDC通信を変更・実装する場合は、必ず `docs/cdc_communication_spec.md` と対象Issueを参照する。

- 親機・子機のCDCプロトコルは共通化する。
- JSONメッセージ、Command、Response、Error、Codec等の共通部分は共通クレートとして実装する方針とする。
- USB / OS / MCU固有のI/OとCommand Handlerはターゲット側へ分離する。
- 子機でも利用できるよう、共通クレートは可能な限り`no_std`対応を考慮する。
- CDCは設定、状態確認、診断、保守用のローカル管理インターフェースとする。
- ボタン等へ割り当てるマクロ／アクションの具体的内容はCDC通信仕様とは分離する。
- CDC設定から任意コードを実行させる方式は採用しない。

関連Issue:

- #1 親機（Raspberry Pi Zero）のCDC通信を実装する
- #2 子機（Pico W / Pico 2 W）のCDC通信を実装する
- #3 ボタン等に割り当てる共通マクロ処理を定義・実装する

## 9. UDP通信の重要ルール

- UDPのデフォルト待受は `0.0.0.0:5000`。
- 親機のOFFLINE判定初期値は210秒。
- EVENTの重複判定は必ず `device_id + message_id` を使用する。
- 重複EVENTは再処理しないがACKは再返信する。
- 子機EVENTはACK待ち3秒、初回送信とは別に最大3回再送する。
- 親機の子機一覧は現仕様ではメモリ上だけに保持し、永続化しない。
- GPIO番号をUDPイベントデータへ直接含めず、論理チャンネルを使用する。

UDP仕様を変更する場合は `docs/parent_child_udp_communication_spec.md` を先に確認すること。

## 10. ターゲット固有の注意

### 親機

- 対象: Raspberry Pi Zero W / Zero 2 W（Linux）。
- CDCではLinux USB Gadgetを利用する構成を前提とする。
- CDC I/OとHubの業務ロジックを疎結合に保つ。
- Web UI、親機から子機への通知・出力制御等は、対象Issueで明示されていない限り勝手に追加しない。

### 子機

- 対象: Raspberry Pi Pico W / Pico 2 W。
- `src/protocol.rs` は現行UDP wire format確認時の重要な実装参照先とする。
- 初期設定は現時点では `src/config.rs` のコンパイル時設定を使用している。
- USB CDC設定と設定永続化は関連Issueに従って実装する。
- Pico W / Pico 2 W双方への影響を考慮する。

## 11. テスト・確認

変更後は、変更範囲に応じて必要な確認を実施する。

基本確認:

```sh
cargo fmt --all -- --check
cargo test --workspace --offline
```

子機プロトコルライブラリ確認:

```sh
cargo check -p uchi-pulse-node --lib --features pico-w --target thumbv6m-none-eabi
cargo check -p uchi-pulse-node --lib --features pico-2-w --target thumbv8m.main-none-eabihf
```

子機ファームウェアの完全なビルドには、`apps/uchi-pulse-node/firmware/README.md` に記載されたCYW43439用バイナリが必要である。これらの`.bin`ファイルはリポジトリへ追加しない。

Issueに追加のテスト条件がある場合は、その条件も実施する。

実機確認が必要だが実行環境がない場合は、未確認事項として明記する。

## 12. 完了時の報告

作業完了時は、IssueまたはPRへ追跡できる形で以下を報告する。

- 対応Issue
- 変更したファイル
- 主な実装内容
- 設計文書の変更有無
- 実行した確認コマンド
- テスト結果
- 未確認事項（実機・Wi-Fi・外部バイナリ等）
- 残課題

「実装した」「テストした」「確認した」は、実際に実施した内容だけを記載すること。
