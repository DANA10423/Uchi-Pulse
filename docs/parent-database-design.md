# 親機 データベース設計書

## 1. 目的

本書は、Uchi-Pulse 親機で使用するデータベースの基本設計を定義する。

親機では SQLite を採用し、以下の情報を永続化する。

- デバイスのマスタ情報
- 子機から UDP で受信したイベント履歴

なお、オンライン／オフラインなどの接続状態はデータベースでは管理せず、親機稼働中のメモリ上で管理する。

---

## 2. データベース方式

| 項目 | 内容 |
|---|---|
| DBMS | SQLite |
| 用途 | 親機におけるデバイス情報・イベント履歴の永続管理 |
| 接続状態管理 | 対象外。メモリ上で管理する |
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
6. ACKが必要なイベントについては、重複受信であっても通信仕様に従ってACKを返す。

---

## 7. 接続状態の扱い

オンライン／オフラインなどの接続状態はデータベース項目として保持しない。

接続状態は、親機稼働中に以下のような情報をメモリ上で管理する。

- 現在認識している子機
- 最終受信時刻
- オンライン／オフライン判定状態

親機を再起動した場合、この接続状態は初期化されてよい。

一方、子機から受信したイベントについては `events` テーブルへ永続保存するため、親機再起動後も履歴として参照可能とする。

---

## 8. 設計方針

本データベースは、家庭内用途でのシンプルな運用を前提とし、必要最小限の構成とする。

- デバイスの固定情報は `devices` に集約する。
- イベント履歴は `events` に記録する。
- 一時的な接続状態をDBへ持ち込まない。
- UDP再送によるイベントの二重登録を防止する。
- 将来の機能追加が必要になった場合は、既存テーブルへ不要な状態項目を追加するのではなく、用途に応じて履歴・設定テーブル等を追加する。
