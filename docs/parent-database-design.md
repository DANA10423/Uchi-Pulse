# 親機 データベース設計書

## 1. 目的

本書は、Uchi-Pulse 親機で使用するデータベースの基本設計を定義する。

親機では SQLite を採用し、以下の情報を永続化する。

- デバイスのマスタ情報
- 子機から UDP で受信したイベント履歴

オンライン／オフラインなどの接続状態そのものはデータベースへ保存せず、親機稼働中のメモリ上で管理する。

ただし、親機起動時は `devices` デバイスマスタを読み込み、登録済みの有効な子機についてメモリ上の接続状態を `INITIAL_WAIT`（未確認）として初期化する。

---

## 2. データベース方式

| 項目 | 内容 |
|---|---|
| DBMS | SQLite |
| 用途 | 親機におけるデバイス情報・イベント履歴の永続管理 |
| 接続状態管理 | 状態そのものはDBへ保存せず、メモリ上で管理する |
| 起動時状態初期化 | `devices` の有効デバイスを読み込み、`INITIAL_WAIT` としてメモリ状態を生成する |
| 日時形式 | ISO 8601 形式の TEXT |

---

## 3. テーブル一覧

| テーブル名 | 用途 |
|---|---|
| `devices` | デバイスマスタ |
| `events` | UDPイベント受信履歴 |

---

## 4. devices テーブル

### 4.1 目的

子機の固定・準固定情報を管理する。

接続状態や最終受信時刻などの一時的な通信状態は保持しない。

親機起動時には、本テーブルに登録されている有効な子機を接続状態管理の初期化元として使用する。

### 4.2 項目定義

| 項目名 | SQLite型 | NULL | キー | 内容 |
|---|---|---|---|---|
| `device_id` | TEXT | NO | PK | 子機を一意に識別するID |
| `name` | TEXT | NO |  | 子機の表示名 |
| `device_type` | TEXT | NO |  | デバイス種別 |
| `registered_at` | TEXT | NO |  | 登録日時。ISO 8601形式 |
| `updated_at` | TEXT | NO |  | 最終更新日時。ISO 8601形式 |
| `enabled` | INTEGER | NO |  | 有効／無効。`1 = 有効`、`0 = 無効` |

### 4.3 制約

- `device_id` を主キーとする。
- `enabled` は `0` または `1` とする。
- 接続状態を示す `online`、`offline`、`last_seen` 等の項目は持たない。
- 親機起動時の接続状態管理対象は、原則として `enabled = 1` のデバイスとする。

### 4.4 CREATE TABLE 例

```sql
CREATE TABLE devices (
    device_id      TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    device_type    TEXT NOT NULL,
    registered_at  TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    enabled        INTEGER NOT NULL CHECK (enabled IN (0, 1))
);
```

---

## 5. events テーブル

### 5.1 目的

子機から UDP で受信したイベントを履歴として保存する。

保存対象は、受信したイベントの識別情報、親機での受信日時、および UDP で受信した JSON 本文とする。

接続状態は本テーブルでは管理しない。

### 5.2 項目定義

| 項目名 | SQLite型 | NULL | キー | 内容 |
|---|---|---|---|---|
| `id` | INTEGER | NO | PK | 親機DB内部の履歴ID。自動採番 |
| `received_at` | TEXT | NO |  | 親機がUDPデータを受信した日時。ISO 8601形式 |
| `device_id` | TEXT | NO |  | 送信元デバイスID |
| `event_id` | TEXT | NO |  | 子機が付与したイベントID |
| `payload` | TEXT | NO |  | UDPで受信したJSON本文 |

### 5.3 制約

- `id` を主キーとし、自動採番する。
- `(device_id, event_id)` を一意とする。
- UDP再送で同一イベントを再受信した場合は、同じイベントを二重登録しない。
- `payload` には、親機側で再構築したJSONではなく、UDPで実際に受信したJSON本文を保存する。

### 5.4 CREATE TABLE 例

```sql
CREATE TABLE events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    received_at  TEXT NOT NULL,
    device_id    TEXT NOT NULL,
    event_id     TEXT NOT NULL,
    payload      TEXT NOT NULL,
    UNIQUE (device_id, event_id)
);
```

---

## 6. イベント登録時の基本動作

子機からイベントを受信した場合、親機は以下の考え方で処理する。

1. UDPデータを受信する。
2. JSONを解析し、`device_id` と `event_id` を取得する。
3. 親機側で `received_at` を付与する。
4. `events` テーブルへ登録する。
5. `(device_id, event_id)` が既に存在する場合は再登録しない。
6. EVENTについては、重複受信であっても通信仕様に従ってACKを返す。
7. 正常なUDPメッセージを受信した場合は、DB登録処理とは別にメモリ上の接続状態を更新する。

---

## 7. 接続状態の扱い

### 7.1 基本方針

接続状態はデータベースへ永続化せず、親機稼働中のメモリ上で管理する。

データベースとメモリの責務は以下のように分離する。

- `devices`: 親機が管理対象として登録している子機
- `events`: 受信済みイベントの履歴
- メモリ上の `DeviceState`: 現在の通信状態

### 7.2 DeviceState

メモリ上では、デバイスごとに最低限以下の情報を管理する。

| 項目 | 内容 |
|---|---|
| `device_id` | 子機ID |
| `last_seen_at` | 最後に正常なUDPメッセージを受信した日時。初期接続待ち中は未設定 |
| `status` | `INITIAL_WAIT` / `ONLINE` / `OFFLINE` |

### 7.3 状態の意味

| 状態 | Web表示例 | 意味 |
|---|---|---|
| `INITIAL_WAIT` | 未確認 | 親機起動後、その子機からまだ一度も正常な通信を確認していない |
| `ONLINE` | オンライン | 正常な通信を確認済みで、オフライン判定時間内に通信がある |
| `OFFLINE` | オフライン | 一度通信を確認したが、その後オフライン判定時間を超えて正常な通信がない |

`INITIAL_WAIT` は、DBへ未登録という意味ではない。DBには登録済みだが、現在起動している親機がまだ通信を確認していない状態を表す。

### 7.4 親機起動時の初期化

親機起動時は以下の順序で状態管理を初期化する。

1. SQLiteをオープンし、必要なテーブルを初期化する。
2. `devices` から `enabled = 1` のデバイスを取得する。
3. 各デバイスについてメモリ上に `DeviceState` を生成する。
4. `status = INITIAL_WAIT` とする。
5. `last_seen_at` は未設定とする。
6. UDP受信を開始する。

接続状態自体をDBから復元することはしない。

### 7.5 正常受信時の状態更新

`HELLO`、`EVENT`、`HEARTBEAT` など、通信仕様上有効なUDPメッセージを受信した場合は、対象デバイスの状態を以下のように更新する。

- `last_seen_at = 現在時刻`
- `status = ONLINE`

そのため、以下の遷移を許可する。

```text
INITIAL_WAIT --正常受信--> ONLINE
OFFLINE      --正常受信--> ONLINE
ONLINE       --正常受信--> ONLINE
```

### 7.6 オフライン判定

定期的に `last_seen_at` を確認し、設定されたオフライン判定時間を超えて正常な通信を受信していない場合は `OFFLINE` とする。

```text
ONLINE --オフライン判定時間超過--> OFFLINE
```

`INITIAL_WAIT` は、親機起動後まだ一度も通信を確認していない状態であるため、単純に `OFFLINE` と同一視しない。

OFFLINEになったデバイスの `DeviceState` は削除せず保持し、再び正常な通信を受信した場合は `ONLINE` に復帰させる。

### 7.7 Web表示との関係

Web画面では、DBに登録されたデバイスと現在のメモリ状態を組み合わせて表示する。

これにより、親機起動直後でも登録済みデバイス一覧を表示でき、通信確認前のデバイスを「登録なし」ではなく「未確認」として表現できる。

---

## 8. 設計方針

本データベースは、家庭内用途でのシンプルな運用を前提とし、必要最小限の構成とする。

- デバイスの固定情報は `devices` に集約する。
- イベント履歴は `events` に記録する。
- 一時的な接続状態をDBへ持ち込まない。
- 親機起動時はDBのデバイスマスタを接続状態管理の初期化元として利用する。
- 登録済みデバイスの初期状態は `INITIAL_WAIT`（未確認）とする。
- 正常な通信を受信したデバイスは `ONLINE` とする。
- 一度ONLINEになった後、一定時間通信がない場合は `OFFLINE` とする。
- UDP再送によるイベントの二重登録を防止する。
- 将来の機能追加が必要になった場合は、既存テーブルへ不要な状態項目を追加するのではなく、用途に応じて履歴・設定テーブル等を追加する。
