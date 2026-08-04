# poshgre

[English](README.md) | **日本語**

fuzzy search でデータベースを高速に探索できる、Rust製のTUI PostgreSQLクライアントです。

![demo](https://github.com/hatohato25/poshgre/releases/download/v0.1.0/t-rec.gif)

## 特徴

- **bastion対応**: SSH bastion サーバー経由でPostgreSQLに接続。接続ごとの設定と共有bastion設定の両方に対応
- **TUIインターフェース**: ratatui によるインタラクティブなターミナルUI
- **Fuzzy Finder**: skim（Rust製のfzfクローン）による高速な接続先・テーブル選択
- **TOML設定**: 複数の接続先を単一の設定ファイルで管理
- **コネクションプーリング**: グローバルなデフォルト値と接続ごとの上書きが可能な設定式コネクションプール
- **Shell入力**: poshgre 内からシェルコマンドを実行（bastion経由で接続している場合はbastion上で実行）
- **AI Prompt**: Anthropic Claude API を使って自然言語からSQLを生成（`ANTHROPIC_API_KEY` が必要）
- **読み取り専用モード**: 重要な接続先での誤った書き込みを防止
- **セキュア**: メモリzeroizeによるパスワード管理、TLS/SSL対応、設定ファイルのパーミッションチェック

## インストール

### Homebrew

```bash
brew tap hatohato25/poshgre
brew trust --formula hatohato25/poshgre/poshgre  # HOMEBREW_REQUIRE_TAP_TRUST が設定されている場合に必要
brew install poshgre
```

### Linux / WSL (Windows Subsystem for Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/hatohato25/poshgre/main/install.sh | bash
```

### ソースからビルド

```bash
cargo build --release
cp target/release/posh /usr/local/bin/
```

## 動作要件

- Rust 1.75.0 以降（ソースからビルドする場合）
- PostgreSQL 13以降

## 設定

`~/.config/poshgre/config.toml` に設定ファイルを作成し、パーミッションを制限してください。

```bash
chmod 600 ~/.config/poshgre/config.toml
```

パーミッションが `600` でない場合、poshgre は警告を表示します。

設定項目の完全なリファレンスと設定例については [Configuration docs](https://hatohato25.github.io/poshgre/docs.html#configuration) を参照してください。コメント付きの最小限の例は `config.example.toml` にもあります。

## 使い方

```bash
posh                          # Start with default config (~/.config/poshgre/config.toml)
posh --config /path/to.toml   # Specify a config file
posh --verbose                # Enable debug logging
posh --readonly               # Start in read-only mode (overrides per-connection settings)
posh --lang ja                # Use Japanese display language
```

`--config` を指定しない場合の設定ファイル探索順序:
1. `~/.config/poshgre/config.toml`
2. `./config.toml`

## キーバインド

接続セレクタ、SQL入力、Shell入力、AI Prompt入力、結果ビューアの各画面におけるキーバインドの一覧は [Key Bindings docs](https://hatohato25.github.io/poshgre/docs.html#key-bindings) を参照してください。

## Dockerによるローカルテスト

このリポジトリには `docker-compose.yml` と `docker/init/` 配下の初期化スクリプトが含まれており、複数のスキーマにサンプルデータを投入済みのPostgreSQL 16インスタンスを起動できます。

### 利用可能なスキーマ

すべてのスキーマは単一のデータベース（`testdb`）内にあります。スキーマの切り替えは `SET search_path TO <schema>` または `ss` エイリアスで行えます。

| スキーマ | 説明 |
|--------|-------------|
| `public` | 基本的な users / products / orders |
| `ecommerce` | 顧客、商品、取引、レビュー |
| `blog` | 著者、投稿、コメント、タグ |
| `analytics` | イベント（約100万行）、セッション、ページビュー |
| `inventory` | 倉庫、商品、在庫数、出荷 |
| `hr` | 部署、従業員、プロジェクト、勤怠エントリ |

### 手順

**1. PostgreSQLコンテナを起動する**

```bash
docker compose up -d
```

コンテナが healthy になるまで待ちます（初期化スクリプトは初回起動時に自動実行されます。`analytics.events` テーブルへの約100万行の投入には1分ほどかかります）。

```bash
docker compose ps          # STATUS should show "healthy"
```

**2. 設定ファイルを作成する**

```bash
mkdir -p ~/.config/poshgre
cp config.example.toml ~/.config/poshgre/config.toml
chmod 600 ~/.config/poshgre/config.toml
```

続いて `~/.config/poshgre/config.toml` を編集し、`local-dev` 接続がDockerインスタンスを指すようにします。

```toml
[[connections]]
name = "local-dev"

[connections.postgres]
host     = "127.0.0.1"
port     = 15432          # mapped port in docker-compose.yml
database = "testdb"
user     = "testuser"
password = "testpass"
ssl_mode = "disable"
```

**3. poshgre を起動する**

```bash
posh
```

接続ピッカーから `local-dev` を選択して、探索を始めましょう。

### 停止とクリーンアップ

```bash
docker compose down          # stop containers, keep volume
docker compose down -v       # stop containers and remove volume
```

## 開発

```bash
# Run all tests
cargo test

# Run integration tests (requires Docker or Podman)
cargo test --test integration_test
```

## ライセンス

MIT
