# Uchi-Pulse 親機 Raspberry Pi Zero 実行環境構築手順書

## 1. 目的

本書は、Uchi-Pulse 親機として使用する Raspberry Pi Zero W / Zero 2 W について、OSの書き込みから初期設定、Wi-Fi接続、SSH接続、Rustクロスビルド、親機プログラムの配置、systemd自動起動、ログ確認、実機試験までの実行環境構築手順を定義する。

本書は親機の実機環境を再現可能にするための運用手順書である。通信仕様・データベース仕様・Web仕様等は各設計書を正本とする。

クロスビルドおよびデプロイ方式は、`DANA10423/intrusion-suite` の `intrusion-gateway` で実績のある Zig + cargo-zigbuild、scp/ssh、systemd の構成を参考にする。

## 2. 対象機種とOS

標準の開発・実機確認対象は **Raspberry Pi Zero 2 W** とする。

| 機種 | 推奨OS | Rustターゲット |
| --- | --- | --- |
| Raspberry Pi Zero 2 W | Raspberry Pi OS Lite 64-bit | `aarch64-unknown-linux-gnu` |
| Raspberry Pi Zero W | Raspberry Pi OS Lite 32-bit | `arm-unknown-linux-gnueabihf` |

OSはデスクトップ環境なしのLite版を使用する。具体的なリリース番号は固定せず、セットアップ時点でRaspberry Pi公式が提供する安定版を使用する。

## 3. 前提機材

- Raspberry Pi Zero 2 W または Zero W
- microSDカード（8GB以上、実運用16GB以上推奨）
- 安定した電源
- Wi-Fiアクセスポイント
- Raspberry Pi Imagerを実行できる開発PC（macOSを想定）
- microSDカードリーダー

親機は基本的にディスプレイ・キーボードを接続しないheadless運用とする。

## 4. Raspberry Pi OSの書き込み

Raspberry Pi Imagerを使用する。

- Zero 2 W: Raspberry Pi OS Lite (64-bit)
- Zero W: Raspberry Pi OS Lite (32-bit)

OSカスタマイズで以下を設定する。

- hostname: `uchi-pulse-hub` を推奨
- Linuxユーザー: 例 `uchi`
- Wi-Fi SSID / password
- Wi-Fi country: 日本国内では `JP`
- SSH: 有効
- timezone: 日本国内では `Asia/Tokyo`

可能であればSSHは公開鍵認証を使用する。パスワード、Wi-Fi認証情報等をリポジトリへ保存しない。

### 4.1 Wi-Fi初期設定

headless運用では、初回起動前にRaspberry Pi ImagerのOSカスタマイズでWi-Fiを設定することを基本とする。

設定する項目:

- SSID
- Wi-Fiパスワード
- WLAN country

日本国内ではWLAN countryを `JP` とする。

Wi-Fi認証情報を設計書、Issue、シェルスクリプト、Git管理対象ファイルへ記載しない。

## 5. 初回起動とSSH接続

microSDをZeroへ挿入して起動する。Imagerで設定したWi-Fiへ接続されるまで待ち、開発PCから接続する。

```bash
ssh uchi@uchi-pulse-hub.local
```

`.local`で解決できない場合はルーター等でIPアドレスを確認して接続する。

```bash
ssh uchi@192.168.1.100
```

SSH接続できない場合は、まずZeroがWi-Fiへ接続できているかをルーターの接続端末一覧等で確認する。

## 6. OS・CPU確認

```bash
cat /etc/os-release
uname -a
uname -m
```

Zero 2 W + 64-bit OSでは `aarch64` であることを確認する。Zero Wでは32-bit Arm環境であることを確認する。

## 7. OS更新

```bash
sudo apt update
sudo apt full-upgrade -y
sudo reboot
```

再起動後、SSHで再接続する。

## 8. 時刻同期確認

```bash
timedatectl
```

Time zone、System clock synchronized、NTP serviceを確認する。EVENT履歴等の時刻に影響するため、時刻同期が正常でない状態で本運用を開始しない。

## 9. Wi-Fi・ネットワーク設定と確認

Raspberry Pi OS Bookworm以降ではNetworkManagerが標準のネットワーク設定ツールであるため、本書では `nmcli` を使用する。

### 9.1 Wi-Fiデバイス確認

```bash
nmcli device
```

内蔵Wi-Fiが通常 `wlan0` として認識され、Wi-Fiデバイスとして表示されることを確認する。

Wi-Fi無線の状態を確認する。

```bash
nmcli radio wifi
```

`disabled` の場合は有効化する。

```bash
sudo nmcli radio wifi on
```

### 9.2 WLAN countryの確認・変更

WLAN countryを変更する必要がある場合は `raspi-config` を使用する。

```bash
sudo raspi-config
```

`Localisation Options` → `WLAN Country` から使用地域を設定する。

日本国内では `JP` を選択する。

### 9.3 周辺Wi-Fiの検索

```bash
nmcli dev wifi list
```

接続するSSIDが一覧に表示されることを確認する。

### 9.4 Wi-Fiへ接続

パスワードをコマンドラインへ直接記述するとシェル履歴等へ残る可能性があるため、対話入力を使用する。

```bash
sudo nmcli --ask dev wifi connect "<SSID>"
```

表示されたプロンプトでWi-Fiパスワードを入力する。

接続後に確認する。

```bash
nmcli dev wifi list
```

対象SSIDの `IN-USE` に `*` が表示されていることを確認する。

接続状態は次でも確認できる。

```bash
nmcli connection show --active
```

### 9.5 Wi-Fi接続先を変更する場合

Zeroを別のWi-Fiへ移動する場合も、周辺SSIDを確認してから同じ方法で接続する。

```bash
nmcli dev wifi list
sudo nmcli --ask dev wifi connect "<NEW_SSID>"
```

SSH経由で接続先を変更すると、Wi-Fi切替時に現在のSSH接続が切断される可能性がある。新しいネットワークへ接続後、新しいIPアドレスまたは `.local` ホスト名で再接続する。

現在登録されている接続設定は以下で確認する。

```bash
nmcli connection show
```

不要な接続設定を削除する場合は、対象の接続名を十分確認したうえで実行する。

```bash
sudo nmcli connection delete "<CONNECTION_NAME>"
```

### 9.6 隠しSSIDへ接続する場合

必要な場合のみ以下を使用する。

```bash
sudo nmcli --ask dev wifi connect "<SSID>" hidden yes
```

### 9.7 IPアドレス・経路確認

```bash
hostname -I
ip addr
ip route
```

以下を確認する。

- `wlan0` にIPアドレスが割り当てられている
- デフォルトルートが存在する
- 開発PCからSSH接続できる
- 子機と親機が相互にUDP通信可能なネットワークに存在する

子機から親機を安定して参照できる必要がある。家庭用ルーターではDHCP予約により親機へ同じIPアドレスを割り当てる方式を推奨する。

### 9.8 再起動後のWi-Fi確認

設定後、一度再起動して自動再接続を確認する。

```bash
sudo reboot
```

再起動後、SSHで再接続し、以下を確認する。

```bash
nmcli connection show --active
hostname -I
ip route
```

Wi-Fiへ自動接続し、想定したネットワーク経由で通信できることを確認する。

## 10. Zero側ディレクトリ構成

標準構成を以下とする。

```text
/opt/uchi-pulse/
└── bin/
    └── uchi-pulse-hub

/etc/uchi-pulse/
└── uchi-pulse-hub.env

/var/lib/uchi-pulse/
└── uchi-pulse.db
```

- `/opt/uchi-pulse/bin/`: 実行ファイル
- `/etc/uchi-pulse/`: 永続設定・環境設定
- `/var/lib/uchi-pulse/`: SQLite DB等の可変データ
- ログ: 原則systemd/journald

## 11. 実行用ユーザー

常時運用では専用システムユーザーを使用する。

```bash
sudo useradd --system --home /var/lib/uchi-pulse --shell /usr/sbin/nologin uchi-pulse
sudo mkdir -p /opt/uchi-pulse/bin /etc/uchi-pulse /var/lib/uchi-pulse
sudo chown -R uchi-pulse:uchi-pulse /var/lib/uchi-pulse
sudo chown -R root:root /opt/uchi-pulse /etc/uchi-pulse
sudo chmod 755 /opt/uchi-pulse /opt/uchi-pulse/bin
sudo chmod 700 /etc/uchi-pulse
```

実行ファイルはroot管理とし、サービスユーザーから書き換えられないようにする。

## 12. クロスビルド方針

Zero上で毎回Rustコンパイルを行わず、開発PCまたはCIで対象アーキテクチャ向けバイナリを作成してZeroへ配置する。

`intrusion-suite/apps/intrusion-gateway/scripts/build-pi.sh` で使用している方式を参考に、Uchi-Pulseでも **Zig + cargo-zigbuild** を標準クロスビルド方式とする。

macOS用バイナリをZeroへコピーしても実行できないため、必ず対象Linuxターゲット向けにビルドする。

## 13. 開発Macへのクロスビルド環境導入

Zigをインストールする。

```bash
brew install zig
```

cargo-zigbuildをインストールする。

```bash
cargo install cargo-zigbuild --locked
```

Rustターゲットを追加する。

Zero 2 W 64-bit:

```bash
rustup target add aarch64-unknown-linux-gnu
```

Zero W 32-bit:

```bash
rustup target add arm-unknown-linux-gnueabihf
```

確認:

```bash
zig version
cargo zigbuild --version
rustup target list --installed
```

## 14. 手動クロスビルド

リポジトリルートから実行する。

### Zero 2 W / 64-bit

```bash
cargo zigbuild -p uchi-pulse-hub --release --target aarch64-unknown-linux-gnu
```

成果物:

```text
target/aarch64-unknown-linux-gnu/release/uchi-pulse-hub
```

### Zero W / 32-bit

```bash
cargo zigbuild -p uchi-pulse-hub --release --target arm-unknown-linux-gnueabihf
```

成果物:

```text
target/arm-unknown-linux-gnueabihf/release/uchi-pulse-hub
```

ビルド後は `file` コマンド等で成果物のアーキテクチャを確認してから配置する。

## 15. Uchi-Pulse用ビルドスクリプト方針

将来的に以下をリポジトリへ配置する。

```text
apps/uchi-pulse-hub/
├── scripts/
│   ├── build-pi.sh
│   ├── deploy.sh
│   └── install-service.sh
├── systemd/
│   └── uchi-pulse-hub.service
└── config/
    └── uchi-pulse-hub.env.example
```

`build-pi.sh` は `intrusion-suite` と同様に `PI_TARGET` でターゲットを上書き可能にする。

標準値は標準実機であるZero 2 W向けを想定する。

概念例:

```bash
target="${PI_TARGET:-aarch64-unknown-linux-gnu}"
cargo zigbuild -p uchi-pulse-hub --release --target "$target"
```

Zero W向け:

```bash
PI_TARGET=arm-unknown-linux-gnueabihf ./apps/uchi-pulse-hub/scripts/build-pi.sh
```

## 16. バイナリ配置

手動配置例:

```bash
scp target/aarch64-unknown-linux-gnu/release/uchi-pulse-hub \
  uchi@uchi-pulse-hub.local:/tmp/uchi-pulse-hub
```

Zero側で正式配置する。

```bash
sudo install -o root -g root -m 0755 \
  /tmp/uchi-pulse-hub /opt/uchi-pulse/bin/uchi-pulse-hub
```

## 17. deploy.shの方針

`intrusion-suite` の `deploy.sh` と同様に、開発PCから以下を一括実行できる構成とする。

1. 対象ターゲットのreleaseバイナリ存在確認
2. `scp` でZeroの一時領域へ転送
3. `ssh` で正式配置
4. systemdサービス再起動

設定値は環境変数で変更可能にする。

```text
PI_TARGET
PI_HOST
PI_TARGET_DIR
PI_SERVICE
```

Uchi-Pulseでの想定値:

```text
PI_HOST=uchi@uchi-pulse-hub.local
PI_TARGET_DIR=/opt/uchi-pulse/bin
PI_SERVICE=uchi-pulse-hub.service
```

本番設定や認証情報をdeployスクリプトへ直接記述しない。

## 18. 環境設定ファイル

認証情報や環境依存設定は、必要に応じてZero側の以下へ配置する。

```text
/etc/uchi-pulse/uchi-pulse-hub.env
```

所有者はroot、権限は0600を基本とする。

```bash
sudo chown root:root /etc/uchi-pulse/uchi-pulse-hub.env
sudo chmod 600 /etc/uchi-pulse/uchi-pulse-hub.env
```

Slack/LINE等の秘密情報をGitへコミットしない。リポジトリには値を含まない `.env.example` のみ配置可能とする。

## 19. 手動起動確認

systemd登録前に手動起動を確認する。

CLI仕様確定前の概念例:

```bash
/opt/uchi-pulse/bin/uchi-pulse-hub --db /var/lib/uchi-pulse/uchi-pulse.db
```

確認事項:

- プロセスが正常起動する
- SQLite DBを作成・オープンできる
- マイグレーションが正常終了する
- UDP待受を開始できる
- 登録済み有効デバイスがINITIAL_WAITで初期化される
- 異常終了しない

## 20. systemdサービス

`intrusion-suite` のサービス構成を参考に、ネットワーク起動待ち、異常終了時再起動、権限制限を行う。

`/etc/systemd/system/uchi-pulse-hub.service` の基本形:

```ini
[Unit]
Description=Uchi-Pulse Hub
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=uchi-pulse
Group=uchi-pulse
WorkingDirectory=/var/lib/uchi-pulse
EnvironmentFile=-/etc/uchi-pulse/uchi-pulse-hub.env
ExecStart=/opt/uchi-pulse/bin/uchi-pulse-hub
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
LimitNOFILE=4096

[Install]
WantedBy=multi-user.target
```

SQLite DBへ書き込み可能となるよう、`/var/lib/uchi-pulse` の所有権を `uchi-pulse` とする。`ProtectSystem=strict` 採用時も書き込み対象ディレクトリの権限・systemd sandbox設定を実機で検証し、必要なら `ReadWritePaths=/var/lib/uchi-pulse` を追加する。

登録:

```bash
sudo systemctl daemon-reload
sudo systemctl enable uchi-pulse-hub
sudo systemctl start uchi-pulse-hub
systemctl status uchi-pulse-hub
```

## 21. install-service.shの方針

`intrusion-suite` と同様に、開発PCからsystemdユニットと環境設定テンプレートをZeroへ転送し、以下を行うスクリプトを用意する。

- `/etc/uchi-pulse` 作成
- `/var/lib/uchi-pulse` 作成
- `/opt/uchi-pulse/bin` 作成
- systemdユニット配置
- 初回のみ環境設定ファイル生成
- `systemctl daemon-reload`
- `systemctl enable uchi-pulse-hub`

既存の実環境設定ファイルを再インストール時に上書きしない。

## 22. ログ確認

```bash
journalctl -u uchi-pulse-hub -n 100 --no-pager
journalctl -u uchi-pulse-hub -f
journalctl -u uchi-pulse-hub -b --no-pager
```

Wi-Fiパスワード、Slack/LINEトークン、Webhook等の秘密情報をログへ出力しない。

## 23. 再起動試験

```bash
sudo reboot
```

再接続後:

```bash
systemctl status uchi-pulse-hub
```

以下を確認する。

- Hubが自動起動している
- SQLite DBが再利用される
- 有効デバイスから状態管理が再構築される
- 通信状態はINITIAL_WAITから開始する
- 過去のONLINE/OFFLINE状態をSQLiteから復元しない

## 24. UDP実機確認

Pico W / Pico 2 Wとの実通信で以下を確認する。

- HELLO / HEARTBEAT / EVENT受信
- EVENT ACK
- EVENT重複排除
- 重複EVENTへのACK再送
- INITIAL_WAIT / ONLINE / OFFLINE遷移
- OFFLINEからONLINEへの復帰

具体的仕様は `docs/parent_child_udp_communication_spec.md` を正本とする。

## 25. Web・通知・CDC実機確認

各機能実装後にZero実機で確認する。

Web:
- 同一LANから接続できる
- 通信状態・入室可否が仕様どおり表示される

Slack / LINE:
- ZeroからHTTPS通信できる
- 通知失敗がUDP ACKを失敗させない
- 秘密情報をログへ出さない

USB CDC:
- 親機CDC実装後、USB Gadget/CDC構成を確認する
- OS側設定は実装方式確定後に本書へ追記する

## 26. バックアップ

最低限の対象:

- `/var/lib/uchi-pulse/uchi-pulse.db`
- `/etc/uchi-pulse/` 配下の永続設定

DBバックアップはサービス停止中、またはSQLiteの整合性を保証できる方法で取得する。

## 27. アップデート手順

基本フロー:

```text
Macでテスト
  ↓
Zero向けcargo zigbuild
  ↓
Zeroへ転送
  ↓
必要ならDBバックアップ
  ↓
バイナリ差し替え
  ↓
systemd再起動
  ↓
status / journal確認
  ↓
基本通信確認
```

`deploy.sh` 実装後は転送・差し替え・再起動を自動化する。

## 28. 初期構築チェックリスト

- [ ] 対象機種とOSを確認した
- [ ] hostname / Wi-Fi / SSH / timezoneを設定した
- [ ] OSを書き込み初回起動した
- [ ] Wi-Fiへ接続できた
- [ ] `nmcli` で接続SSIDを確認した
- [ ] IPアドレスとデフォルトルートを確認した
- [ ] 再起動後もWi-Fiへ自動接続した
- [ ] SSH接続できた
- [ ] OS更新・時刻同期を確認した
- [ ] Zero側ディレクトリと実行ユーザーを準備した
- [ ] MacへZigを導入した
- [ ] Macへcargo-zigbuildを導入した
- [ ] 対象Rustターゲットを導入した
- [ ] 対象機種向けクロスビルドに成功した
- [ ] バイナリのアーキテクチャを確認した
- [ ] Zeroへバイナリを配置した
- [ ] 手動起動を確認した
- [ ] systemdサービスを登録した
- [ ] journalログを確認した
- [ ] Zero再起動後も自動起動した
- [ ] 実装済み機能の実機試験を実施した

## 29. PCテストと実機試験の関係

```text
Mac / CI
  ↓
Rustロジックテスト
SQLiteテスト
UDP/Web等のテスト
  ↓
Zig + cargo-zigbuild
  ↓
対象アーキテクチャ向けLinuxバイナリ
  ↓
Raspberry Pi Zero実機
  ↓
OS / systemd / SQLite / UDP / Web確認
  ↓
Pico実機との結合試験
```

PC上ですべてのテストが成功しても、Zero実機での動作確認を最終完了条件から除外しない。

## 30. 今後追記・実装する項目

- `apps/uchi-pulse-hub/scripts/build-pi.sh` の実装
- `apps/uchi-pulse-hub/scripts/deploy.sh` の実装
- `apps/uchi-pulse-hub/scripts/install-service.sh` の実装
- `apps/uchi-pulse-hub/systemd/uchi-pulse-hub.service` の実装
- `apps/uchi-pulse-hub/config/uchi-pulse-hub.env.example` の実装
- Hub正式CLI / 設定ファイル仕様
- UDP/Web待受ポートのOS側確認手順
- USB Gadget / CDCのOS設定
- Slack / LINE認証情報の正式な配置方法
- バックアップ・リストアの正式運用
- Zero W / Zero 2 Wそれぞれの実機試験結果

## 31. 参考実装・参考資料

クロスビルド・デプロイ・systemd構成の参考:

- `DANA10423/intrusion-suite/apps/intrusion-gateway/scripts/build-pi.sh`
- `DANA10423/intrusion-suite/apps/intrusion-gateway/scripts/deploy.sh`
- `DANA10423/intrusion-suite/apps/intrusion-gateway/scripts/install-service.sh`
- `DANA10423/intrusion-suite/apps/intrusion-gateway/systemd/intrusion-gateway.service`

Wi-Fi設定はRaspberry Pi公式ドキュメントのNetworkManager / `nmcli` 手順に従う。

Uchi-Pulseではディレクトリ、ユーザー、サービス名、バイナリ名をUchi-Pulse用に変更し、設計上の責務を維持する。

## 32. 関連設計書

- `docs/uchi-pulse-system-overview.md`
- `docs/parent-overview-design.md`
- `docs/parent-database-design.md`
- `docs/parent_child_udp_communication_spec.md`
- `docs/parent-web-status-design.md`
- `docs/cdc_communication_spec.md`