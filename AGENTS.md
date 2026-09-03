# AGENTS.md

このファイルは、Uchi Pulse リポジトリで作業するエージェント向けの指示書です。

## プロジェクト概要

Uchi Pulse は、家庭内の物理ボタン・センサーから発生したイベントを、Wi-Fi 経由で親機へ集約するシステムです。

- `apps/uchi-pulse-node`: Raspberry Pi Pico W / Pico 2 W 向け子機ファームウェア
- `apps/uchi-pulse-hub`: Raspberry Pi Zero / Zero 2 W 向け親機プログラム
- `docs/`: システム設計・通信仕様
- `.github/DEVELOPMENT_WORKFLOW.md`: Issue / PR / Codexを含む開発運用ルール

## 作業開始時のルール

新規機能、設計変更、改善、不具合修正は原則としてGitHub Issueを起点にする。

実装を開始する前に、対象Issueから少なくとも以下を確認すること。

1. 目的
2. 対象
3. 決定事項
4. 参照仕様
5. 実装内容
6. 非対象
7. 完了条件
8. テスト・確認

Issueの内容とmain上の設計文書が矛盾する場合は、独断で仕様を変更せず差分を報告すること。

Issue運用の詳細は `.github/DEVELOPMENT_WORKFLOW.md` を参照する。

## 仕様の参照順

変更対象に応じて、main上の該当仕様を確認すること。

- 親機・子機UDP通信: `docs/parent_child_udp_communication_spec.md`
- CDC管理通信: `docs/cdc_communication_spec.md`
- システム全体: `docs/uchi-pulse-system-overview.md`
- 子機・家庭内コミュニケーション設計: `docs/home_yuru_communication_design.md`

仕様と実装が異なる場合、独断で仕様を変更せず、必要な差分をIssueまたは結果報告へ記載すること。

## 開発ルール

- Rust 2024 edition を使用する。
- 変更後は `cargo fmt --all -- --check` を実行する。
- 可能な限り、ロジックをテスト可能なライブラリ部分へ分離する。
- 通信JSONのフィールド名・大文字のメッセージ種別を変更しない。
- EVENTの重複判定は必ず `device_id + message_id` を使う。
- 重複EVENTは再処理しないが、ACKは再返信する。
- 親機の子機一覧は現仕様ではメモリ上だけに保持し、永続化しない。
- Wi-Fi認証情報や実機固有設定をコミットしない。
- 関係のないIssueの変更を同じPRへ混在させない。
- 設計変更が発生した場合は、必要な`docs/`文書も追跡可能な形で更新する。

## テスト・確認コマンド

```sh
cargo fmt --all -- --check
cargo test --workspace --offline

# 子機のプロトコルライブラリ確認
cargo check -p uchi-pulse-node --lib --features pico-w --target thumbv6m-none-eabi
cargo check -p uchi-pulse-node --lib --features pico-2-w --target thumbv8m.main-none-eabihf
```

子機ファームウェアの完全なビルドには、`apps/uchi-pulse-node/firmware/README.md` に記載された CYW43439 用バイナリが必要です。これらの `.bin` ファイルはリポジトリへ追加しません。

## 実装上の注意

### 親機

- UDP のデフォルト待受は `0.0.0.0:5000`。
- OFFLINE 判定の初期値は 210 秒。
- Web UI、永続化、親機から子機への通知・出力制御は拡張対象。
- CDC通信を変更する場合は `docs/cdc_communication_spec.md` と対象Issueを参照する。

### 子機

- `src/protocol.rs` が親機と共有すべきUDP wire format の基準。
- GPIO番号を通信データへ直接含めず、論理チャンネルを送信する。
- EVENT は ACK 待ち 3 秒、初回送信とは別に最大 3 回再送する。
- 初期設定は現在 `src/config.rs` のコンパイル時設定。USB CDC 設定は未実装。
- CDC通信を変更する場合は `docs/cdc_communication_spec.md` と対象Issueを参照する。

## Pull Request

PR作成時は `.github/pull_request_template.md` を使用し、対応Issue、参照仕様、変更内容、実行テスト、未確認事項を明記する。

## 変更時の報告

完了時は、以下を簡潔に報告する。

- 対応Issue
- 変更したファイル
- 主な実装内容
- 実行した確認コマンドと結果
- 未確認事項（実機・Wi-Fi・外部バイナリなど）
- 残課題
