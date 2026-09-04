# Uchi-Pulse 親機 Raspberry Pi Zero 実行環境構築手順書

## 1. 目的

本書は、Uchi-Pulse 親機として使用する Raspberry Pi Zero W / Zero 2 W について、OSの書き込みから初期設定、SSH接続、Uchi-Pulse親機プログラムの配置、自動起動、ログ確認までの実行環境構築手順を定義する。

本書は親機の実機環境を再現可能にするための運用手順書であり、Uchi-Pulseの通信仕様・データベース仕様・Web仕様そのものは各設計書を正本とする。

## 2. 対象機種

対象機種は以下とする。

- Raspberry Pi Zero 2 W
- Raspberry Pi Zero W

標準の開発・実機確認対象は **Raspberry Pi Zero 2 W** とする。

### 2.1 推奨OS

#### Raspberry Pi Zero 2 W

- Raspberry Pi OS Lite 64-bit
- デスクトップ環境なし

Zero 2 Wは64-bit Arm Cortex-A53を搭載し、Raspberry Pi OS 64-bitの対応機種である。

#### Raspberry Pi Zero W

- Raspberry Pi OS Lite 32-bit
- デスクトップ環境なし

初代Zero Wを使用する場合は32-bit環境を使用する。

OSの具体的なリリース番号は固定せず、セットアップ時点でRaspberry Pi公式が提供する安定版Raspberry Pi OS Liteを使用する。

参考:

- https://www.raspberrypi.com/software/operating-systems/
- https://www.raspberrypi.com/documentation/computers/os.html

## 3. 前提機材

以下を準備する。

- Raspberry Pi Zero 2 W または Zero W
- microSDカード
  - 8GB以上を最低目安とする
  - 実運用では16GB以上を推奨する
- 安定した電源
- Wi-Fiアクセスポイント
- Raspberry Pi Imagerを実行できるPC
  - 開発時はmacOSを想定
- microSDカードをPCへ接続するためのカードリーダー

親機は基本的にディスプレイ・キーボードを接続しない **headless運用** とする。

## 4. Raspberry Pi Imagerの準備

PCへRaspberry Pi Imagerをインストールする。

公式:

- https://www.raspberrypi.com/software/

Raspberry Pi Imagerを起動し、以下を選択する。

### Zero 2 W

- Device: Raspberry Pi Zero 2 W
- OS: Raspberry Pi OS Lite (64-bit)
- Storage: 使用するmicroSDカード

### Zero W

- Device: Raspberry Pi Zero W
- OS: Raspberry Pi OS Lite (32-bit)
- Storage: 使用するmicroSDカード

書き込み対象のmicroSDカードを誤らないこと。

## 5. Imagerでの初期設定

OS書き込み前にImagerのOSカスタマイズ画面で初期設定を行う。

### 5.1 ホスト名

推奨:

```text
uchi-pulse-hub
```

複数の親機を扱う場合は重複しない名称に変更する。

例:

```text
uchi-pulse-hub-01
```

### 5.2 ユーザー

専用のLinuxユーザーを設定する。

例:

```text
uchi
```

パスワードは十分に強いものを設定する。

パスワードをリポジトリ、設計書、Issue、ログへ記録しない。

### 5.3 Wi-Fi

家庭内Wi-FiのSSIDとパスワードを設定する。

国コードは実際の使用地域に合わせる。

日本国内で使用する場合:

```text
JP
```

Uchi-Pulseの子機と親機は、相互にUDP通信可能な同一LANまたは通信可能なネットワークへ接続する。

### 5.4 SSH

SSHを有効化する。

開発時は、可能であれば公開鍵認証を使用する。

初期構築でパスワード認証を使用する場合も、外部インターネットへSSHポートを公開しない。

### 5.5 タイムゾーン

日本国内では以下を使用する。

```text
Asia/Tokyo
```

## 6. microSDへの書き込み

ImagerでOSを書き込む。

書き込み後の検証まで正常終了したことを確認し、microSDを安全に取り外す。

microSDをRaspberry Pi Zeroへ挿入する。

## 7. 初回起動

Raspberry Pi Zeroへ電源を接続する。

初回起動時はOSの初期処理とWi-Fi接続が行われる。

PCからホスト名でSSH接続を試す。

例:

```bash
ssh uchi@uchi-pulse-hub.local
```

`.local`で名前解決できない場合は、ルーターの管理画面等でZeroへ割り当てられたIPアドレスを確認し、IPアドレスを指定する。

例:

```bash
ssh uchi@192.168.1.100
```

初回接続時はSSHホスト鍵を確認したうえで接続する。

## 8. OS基本確認

SSH接続後、OSとCPUアーキテクチャを確認する。

```bash
cat /etc/os-release
uname -a
uname -m
```

想定例:

Zero 2 W 64-bit:

```text
aarch64
```

Zero W 32-bitでは32-bit Arm環境であることを確認する。

## 9. OS更新

初回セットアップ時にパッケージ情報とOSパッケージを更新する。

```bash
sudo apt update
sudo apt full-upgrade -y
```

更新後に再起動が必要な場合:

```bash
sudo reboot
```

再起動後、SSHで再接続する。

## 10. 時刻同期確認

親機ではEVENT履歴等で時刻を使用するため、OS時刻が正しいことを確認する。

```bash
timedatectl
```

以下を確認する。

- Time zoneが意図した地域である
- System clock synchronizedが有効である
- NTP serviceが有効である

日本国内の例:

```text
Time zone: Asia/Tokyo
```

## 11. ネットワーク確認

IPアドレスを確認する。

```bash
hostname -I
```

ネットワークインターフェースを確認する。

```bash
ip addr
```

デフォルトルートを確認する。

```bash
ip route
```

Uchi-Pulseでは、子機から親機を安定して参照できるようにする必要がある。

親機IPアドレスの固定方法はネットワーク環境によって異なるため、本書ではOS側で固定値を直接設定することを必須としない。

家庭用ルーターを使用する場合は、DHCP予約により `uchi-pulse-hub` へ同じIPアドレスを割り当てる方式を推奨する。

## 12. Uchi-Pulse実行ディレクトリ

親機プログラムは、アプリケーション本体・設定・可変データを分離する。

標準構成は以下とする。

```text
/opt/uchi-pulse/
└── bin/
    └── uchi-pulse-hub

/etc/uchi-pulse/
└── （将来の設定ファイル）

/var/lib/uchi-pulse/
└── uchi-pulse.db
```

役割:

- `/opt/uchi-pulse/bin/`: 実行ファイル
- `/etc/uchi-pulse/`: 永続設定
- `/var/lib/uchi-pulse/`: SQLite DB等の可変データ

ログは原則としてsystemd/journaldへ出力し、独自ログファイルを必須としない。

## 13. 実行用ユーザー

初期開発段階ではImagerで作成したユーザーから手動実行してもよい。

常時運用段階では、Uchi-Pulse専用のシステムユーザーで実行する構成を推奨する。

例:

```bash
sudo useradd --system --home /var/lib/uchi-pulse --shell /usr/sbin/nologin uchi-pulse
```

ディレクトリを作成する。

```bash
sudo mkdir -p /opt/uchi-pulse/bin
sudo mkdir -p /etc/uchi-pulse
sudo mkdir -p /var/lib/uchi-pulse
```

所有者を設定する。

```bash
sudo chown -R uchi-pulse:uchi-pulse /var/lib/uchi-pulse
```

実行ファイルについてはroot管理とし、サービスユーザーから書き換えられないようにする。

```bash
sudo chown -R root:root /opt/uchi-pulse
sudo chmod 755 /opt/uchi-pulse
sudo chmod 755 /opt/uchi-pulse/bin
```

## 14. 親機バイナリの準備

Uchi-Pulse親機はRustで実装する。

通常運用ではZero上でソースコードから毎回コンパイルせず、開発PCまたはCIで対象アーキテクチャ向けに作成した実行ファイルを配置する方式を基本とする。

対象CPUアーキテクチャが異なるため、macOS用にビルドした実行ファイルをそのままZeroへコピーして実行することはできない。

Zero 2 W 64-bitとZero W 32-bitでは対象アーキテクチャが異なるため、それぞれに対応したビルド成果物を使用する。

具体的なRustクロスビルド手順・ターゲットは、親機プログラムのビルド方式確定時に別途定義する。

## 15. バイナリ配置

開発PCから作成済みバイナリを転送する例:

```bash
scp ./uchi-pulse-hub uchi@uchi-pulse-hub.local:/tmp/uchi-pulse-hub
```

ZeroへSSH接続し、正式配置する。

```bash
sudo install -o root -g root -m 755 /tmp/uchi-pulse-hub /opt/uchi-pulse/bin/uchi-pulse-hub
```

配置確認:

```bash
ls -l /opt/uchi-pulse/bin/uchi-pulse-hub
```

## 16. 手動起動確認

systemd登録前に、まず手動で親機プログラムが起動することを確認する。

実際のCLI引数は親機実装の確定内容に従う。

DBパスを引数または設定で指定できる実装となった場合の概念例:

```bash
/opt/uchi-pulse/bin/uchi-pulse-hub --db /var/lib/uchi-pulse/uchi-pulse.db
```

上記はCLI仕様が確定するまで **例示** とし、正式なコマンド仕様とはしない。

確認事項:

- プロセスが正常起動する
- SQLite DBを作成・オープンできる
- マイグレーションが正常終了する
- UDP待受を開始できる
- 登録済み有効デバイスがINITIAL_WAITで初期化される
- 異常終了しない

## 17. systemdによる自動起動

親機プログラムの起動方法が確定した後、systemdサービスとして登録する。

サービスファイル:

```text
/etc/systemd/system/uchi-pulse-hub.service
```

基本形:

```ini
[Unit]
Description=Uchi-Pulse Hub
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=uchi-pulse
Group=uchi-pulse
ExecStart=/opt/uchi-pulse/bin/uchi-pulse-hub
WorkingDirectory=/var/lib/uchi-pulse
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

DBパス等の正式な起動引数が決定した場合は `ExecStart` に反映する。

サービスファイルを作成・変更した後:

```bash
sudo systemctl daemon-reload
```

自動起動を有効化する。

```bash
sudo systemctl enable uchi-pulse-hub
```

手動起動:

```bash
sudo systemctl start uchi-pulse-hub
```

状態確認:

```bash
systemctl status uchi-pulse-hub
```

停止:

```bash
sudo systemctl stop uchi-pulse-hub
```

再起動:

```bash
sudo systemctl restart uchi-pulse-hub
```

## 18. ログ確認

systemd/journaldのログを確認する。

直近のログ:

```bash
journalctl -u uchi-pulse-hub -n 100 --no-pager
```

リアルタイム確認:

```bash
journalctl -u uchi-pulse-hub -f
```

現在の起動以降:

```bash
journalctl -u uchi-pulse-hub -b --no-pager
```

ログへ以下を出力しないこと。

- Wi-Fiパスワード
- Slack/LINEのアクセストークン等
- Webhook秘密情報
- その他認証情報

## 19. 再起動試験

実行環境構築後、Zeroを再起動する。

```bash
sudo reboot
```

再接続後、以下を確認する。

```bash
systemctl status uchi-pulse-hub
```

確認事項:

- Uchi-Pulse Hubが自動起動している
- SQLite DBが再利用されている
- DB登録済み有効デバイスから状態管理が再構築されている
- 通信状態がINITIAL_WAITから開始する
- 過去のONLINE/OFFLINE状態をSQLiteから復元していない

## 20. UDP実機確認

親機UDP実装完了後、Pico W / Pico 2 Wとの実通信を確認する。

確認項目:

- HELLOを受信できる
- HEARTBEATを受信できる
- EVENTを受信できる
- EVENTにACKを返せる
- 同一EVENT再送を重複排除できる
- 重複EVENTにもACKを再送できる
- 正常通信で通信状態がONLINEになる
- 通信停止時にOFFLINEへ遷移する
- OFFLINE後の正常通信でONLINEへ復帰する

具体的なUDPフィールド、タイムアウト、再送回数は `docs/parent_child_udp_communication_spec.md` を正本とする。

## 21. Web実機確認

親機Web機能実装後、同一LAN内のPCまたはスマートフォンからWeb画面へアクセスする。

確認項目:

- 親機Webサーバーへ接続できる
- 家族用端末の通信状態が表示される
- 家族用端末の入室可否が表示される
- 設備用端末に通信状態が表示される
- 未確認 / オンライン / オフラインが正しく反映される

具体的な表示仕様は `docs/parent-web-status-design.md` を正本とする。

## 22. 外部通知実機確認

Slack / LINE通知実装後に確認する。

- ZeroからインターネットへHTTPS通信できる
- 設定された通知だけが送信される
- 通知失敗でUDP ACK処理が失敗しない
- 認証情報がログへ出ない

通知仕様は関連Issueおよび親機設計書を正本とする。

## 23. USB CDC確認

親機CDC実装後、USB Gadget/CDC構成を確認する。

USB Gadgetに必要なOS設定は実装方式確定後に本書へ追加する。

CDCプロトコル自体は `docs/cdc_communication_spec.md` を正本とする。

## 24. バックアップ

最低限バックアップ対象とするもの:

- `/var/lib/uchi-pulse/uchi-pulse.db`
- `/etc/uchi-pulse/` 配下の永続設定

実行ファイルはリポジトリまたはビルド成果物から再配置可能とし、バックアップの必須対象とはしない。

DBバックアップはサービス停止中、またはSQLiteの整合性を保証できる方法で取得する。

## 25. アップデート手順

親機プログラム更新時の基本手順:

```text
新バイナリ作成
  ↓
Zeroへ転送
  ↓
サービス停止
  ↓
必要ならDBバックアップ
  ↓
バイナリ差し替え
  ↓
サービス起動
  ↓
status / journal確認
  ↓
基本通信確認
```

バイナリ差し替え前に既存バージョンを一時退避できる運用としてもよい。

## 26. 初期構築チェックリスト

- [ ] 対象機種を確認した
- [ ] Zero 2 WではRaspberry Pi OS Lite 64-bitを選択した
- [ ] Zero WではRaspberry Pi OS Lite 32-bitを選択した
- [ ] hostnameを設定した
- [ ] Wi-Fiを設定した
- [ ] SSHを有効化した
- [ ] タイムゾーンを設定した
- [ ] OSを書き込んだ
- [ ] 初回起動した
- [ ] SSH接続できた
- [ ] OSを更新した
- [ ] 時刻同期を確認した
- [ ] ネットワークを確認した
- [ ] `/opt/uchi-pulse/bin` を準備した
- [ ] `/etc/uchi-pulse` を準備した
- [ ] `/var/lib/uchi-pulse` を準備した
- [ ] 親機バイナリを配置した
- [ ] 手動起動を確認した
- [ ] systemdサービスを登録した
- [ ] 自動起動を確認した
- [ ] journalログを確認した
- [ ] 再起動後も自動起動した

## 27. 実装フェーズとの関係

PC上の単体テスト・統合テストと、Zero実機試験を区別する。

```text
PC / CI
  ↓
Rustロジックテスト
SQLiteテスト
UDP処理テスト
Web処理テスト
  ↓
対象アーキテクチャ向けビルド
  ↓
Raspberry Pi Zero実機
  ↓
OS / systemd / SQLite / UDP / Web確認
  ↓
Pico実機との結合試験
```

PC上ですべてのテストが成功しても、Zero実機での動作確認を最終完了条件から除外しない。

## 28. 今後追記する項目

以下は関連実装方式確定後に本書へ追記する。

- Rustクロスビルドの正式ターゲットと手順
- 配布バイナリの作成方法
- Hubの正式CLI / 設定ファイル仕様
- UDP待受ポートのOS側確認手順
- Web待受アドレス・ポートの確認手順
- USB Gadget / CDCのOS設定
- Slack / LINE認証情報の安全な配置方法
- バックアップ・リストアの正式運用
- Zero W / Zero 2 Wそれぞれの実機試験結果

## 29. 関連設計書

- `docs/uchi-pulse-system-overview.md`
- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-web-status-design.md`
- `docs/cdc_communication_spec.md`
