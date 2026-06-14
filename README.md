# poshgre

A TUI PostgreSQL client for fast database exploration with fuzzy search, written in Rust.

## Features

- **Bastion Support**: Connect to PostgreSQL through SSH bastion servers with per-connection or shared bastion configuration
- **TUI Interface**: Interactive terminal UI powered by ratatui
- **Fuzzy Finder**: Quick connection and table selection with skim (Rust-native fzf)
- **TOML Configuration**: Manage multiple connections in a single config file
- **Connection Pooling**: Configurable connection pool with global defaults and per-connection overrides
- **Shell Input**: Run shell commands from within poshgre (executes on bastion when connected via bastion)
- **AI Prompt**: Generate SQL from natural language using the Anthropic Claude API (requires `ANTHROPIC_API_KEY`)
- **Read-only Mode**: Prevent accidental writes on sensitive connections
- **Secure**: Memory-zeroed password handling, TLS/SSL support, config file permission checks

## Installation

### Homebrew

```bash
brew tap hatohato25/poshgre
brew install posh
```

### Linux / WSL (Windows Subsystem for Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/hatohato25/poshgre/main/install.sh | bash
```

### From Source

```bash
cargo build --release
cp target/release/posh /usr/local/bin/
```

## Requirements

- Rust 1.75.0 or later (for building from source)
- PostgreSQL 13+

## Configuration

Create a configuration file at `~/.config/poshgre/config.toml` and restrict its permissions:

```bash
chmod 600 ~/.config/poshgre/config.toml
```

poshgre warns if the file permissions are not `600`.

### Configuration Reference

#### `[settings]` — Application settings (optional)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `language` | string | `"en"` | Display language: `"en"` or `"ja"` |
| `anthropic_api_key` | string | — | Anthropic API key for the AI prompt feature. Can also be set via the `ANTHROPIC_API_KEY` environment variable. If not set, the AI prompt area is hidden. |
| `claude_model` | string | `"claude-3-5-haiku-20241022"` | Claude model to use for SQL generation. Can also be set via the `CLAUDE_MODEL` environment variable. |

#### `[[connections]]` — Connection entry (one or more required)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | string | — | Connection name (required) |
| `bastion` | see below | omitted | Bastion configuration |
| `readonly` | bool | `false` | Prevent write operations when `true` |

**`bastion` field behavior:**

| Value | Effect |
|-------|--------|
| Omitted or `false` | Direct connection (no bastion) |
| `true` | Use `[default_bastion]` settings |
| `[connections.bastion]` table | Use per-connection bastion settings |

#### `[connections.bastion]` — Per-connection bastion settings

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `host` | string | — | Bastion server hostname or IP (required) |
| `port` | integer | `22` | SSH port |
| `user` | string | — | SSH username (required) |
| `key_path` | string | — | Path to SSH private key; omit to use SSH agent |

#### `[connections.postgres]` — PostgreSQL settings

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `host` | string | — | PostgreSQL hostname or IP (required) |
| `port` | integer | `5432` | PostgreSQL port |
| `database` | string | — | Database name (required) |
| `user` | string | — | PostgreSQL username (required) |
| `password` | string | — | PostgreSQL password (required) |
| `timeout` | integer | `30` | Connection timeout in seconds |
| `ssl_mode` | string | `"require"` | TLS/SSL mode: `"disable"`, `"require"`, `"verify-ca"`, or `"verify-full"` |

#### `[connections.postgres.pool]` — Connection pool settings (optional)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `max_connections` | integer | `10` | Maximum number of connections |
| `idle_timeout` | integer | `300` | Idle connection timeout in seconds |

#### `[default_bastion]` — Shared bastion settings (optional)

Applied to all connections with `bastion = true`. Fields are the same as `[connections.bastion]`.

#### `[default_postgres_pool]` — Shared pool settings (optional)

Applied to all connections that do not specify their own pool settings. Per-connection settings take precedence. Fields are the same as `[connections.postgres.pool]`.

### Configuration Examples

#### Example 1: Direct connection (local development)

```toml
[[connections]]
name = "local-dev"

[connections.postgres]
host = "localhost"
port = 5432
database = "your_database"
user = "postgres"
password = "your_password"
ssl_mode = "disable"  # acceptable for local development
```

#### Example 2: Shared bastion (production environments)

Use `[default_bastion]` when multiple connections share the same bastion server.

```toml
[default_bastion]
host = "bastion.example.com"
port = 22
user = "your_ssh_user"
# Omit key_path to use your SSH agent (e.g., 1Password SSH agent)

[default_postgres_pool]
max_connections = 20
idle_timeout = 600

[[connections]]
name = "production"
bastion = true  # uses [default_bastion]

[connections.postgres]
host = "postgres.internal.example.com"
port = 5432
database = "production_db"
user = "app_user"
password = "secure_password"
timeout = 60
ssl_mode = "require"

[[connections]]
name = "staging"
bastion = true  # uses [default_bastion]
readonly = true

[connections.postgres]
host = "postgres-staging.internal.example.com"
port = 5432
database = "staging_db"
user = "app_user"
password = "staging_password"
ssl_mode = "require"
```

#### Example 3: Per-connection bastion

Use `[connections.bastion]` when each connection has a different bastion server.

```toml
[[connections]]
name = "region-a"

[connections.bastion]
host = "bastion-a.example.com"
port = 22
user = "your_ssh_user"
key_path = "~/.ssh/id_rsa"

[connections.postgres]
host = "postgres-a.internal.example.com"
port = 5432
database = "db_a"
user = "app_user"
password = "password_a"
ssl_mode = "require"

[[connections]]
name = "region-b"

[connections.bastion]
host = "bastion-b.example.com"
port = 2222
user = "your_ssh_user"
key_path = "~/.ssh/id_ed25519"

[connections.postgres]
host = "postgres-b.internal.example.com"
port = 5432
database = "db_b"
user = "app_user"
password = "password_b"
ssl_mode = "require"
```

See `config.example.toml` for a complete annotated example.

## Usage

```bash
posh                          # Start with default config (~/.config/poshgre/config.toml)
posh --config /path/to.toml   # Specify a config file
posh --verbose                # Enable debug logging
posh --readonly               # Start in read-only mode (overrides per-connection settings)
posh --lang ja                # Use Japanese display language
```

Config file search order when `--config` is not specified:
1. `~/.config/poshgre/config.toml`
2. `./config.toml`

## Key Bindings

### Connection Selection

Connection selection is handled by skim (Rust-native fzf). Standard fzf key bindings apply.

| Key | Action |
|-----|--------|
| Type to filter | Incremental search |
| `Up` / `Down` | Move cursor |
| `Enter` | Select connection |
| `ESC` / `Ctrl+C` | Cancel / quit |

### SQL Input

SQL input is handled directly by poshgre. Press `Tab` (when the completion popup is closed) to cycle focus: SQL Input → Shell Input → AI Prompt Input → SQL Input. When no `ANTHROPIC_API_KEY` is configured, the AI Prompt area is hidden and `Tab` cycles between SQL Input and Shell Input only.

#### Execution and Completion

| Key | Action |
|-----|--------|
| `Enter` | Execute SQL |
| `Tab` (completion visible) / `Down` | Next completion candidate |
| `Shift+Tab` / `Up` | Previous completion candidate |
| `Tab` (completion hidden) | Switch focus to Shell input area |
| `Ctrl+T` | List tables in current schema |
| `Ctrl+S` | Column selection mode (table → column picker) |

#### Aliases

| Alias | Action |
|-------|--------|
| `ss` | List schemas |
| `st` | List tables in current schema |
| `sc` | Column selection mode |

#### Editing

| Key | Action |
|-----|--------|
| `Ctrl+A` | Select all |
| `Ctrl+C` | Copy selection / quit (no selection) |
| `Ctrl+V` | Paste from clipboard |
| `Ctrl+X` | Cut selection |
| `Ctrl+K` | Delete from cursor to end of line |
| `Ctrl+U` | Delete from start of line to cursor |
| `Ctrl+W` | Delete previous word |
| `Ctrl+Y` | Paste from kill buffer |
| `Ctrl+E` | Move cursor to end of line |
| `Home` / `End` | Move cursor to start / end of line |
| `Alt+←` / `Alt+→` | Move cursor one word left / right |
| `Shift+←` / `Shift+→` | Extend selection left / right |
| `Alt+Shift+←` / `Alt+Shift+→` | Extend selection one word left / right |

#### Navigation and Other

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate SQL history |
| `ESC` | Clear input / close completion popup |
| `q` (empty input) | Quit |

### Shell Input

The Shell input area is a separate input field below the SQL input. Press `Tab` (when the completion popup is not visible) from SQL Input to move focus to Shell Input, then `Tab` again to advance to AI Prompt Input (if configured), or back to SQL Input.

When `Enter` is pressed in the Shell input area, poshgre suspends the TUI, runs the command, displays the output, waits for `Enter`, then resumes the TUI. For direct connections the command runs locally; for bastion connections it runs remotely over SSH.

| Key | Action |
|-----|--------|
| `Enter` | Execute shell command |
| `Tab` | Advance focus to AI Prompt Input (or SQL Input if AI Prompt is hidden) |
| `Up` / `Down` | Navigate shell history |
| `Ctrl+A` / `Home` | Move cursor to start of line |
| `Ctrl+E` / `End` | Move cursor to end of line |
| `Ctrl+K` | Delete from cursor to end of line |
| `Ctrl+U` | Delete from start of line to cursor |
| `Ctrl+W` / `Alt+Backspace` | Delete previous word |
| `Alt+←` / `Alt+b` | Move cursor one word left |
| `Alt+→` / `Alt+f` | Move cursor one word right |
| `Alt+Delete` | Delete next word (forward) |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character after cursor |
| `Esc` | Clear shell input |
| `Ctrl+C` | Quit |

### AI Prompt Input

The AI Prompt input area is displayed below Shell Input when `anthropic_api_key` is set (either in `[settings]` or via the `ANTHROPIC_API_KEY` environment variable). Enter a natural language description and press `Enter` to generate SQL using the Anthropic Claude API. The generated SQL is placed directly into the SQL input area. Only the Anthropic API is supported at this time.

While the agent is processing, a spinner appears in the area title and further input is ignored until the response arrives.

| Key | Action |
|-----|--------|
| `Enter` | Send prompt to Claude and generate SQL |
| `Tab` | Return focus to SQL Input |
| `Shift+Tab` | Return focus to Shell Input |
| `Esc` | Return focus to SQL Input |
| `Ctrl+C` | Quit |

### Result Viewer

Result display is handled by skim (Rust-native fzf). Standard fzf key bindings apply.

| Key | Action |
|-----|--------|
| Type to filter | Incremental search |
| `Up` / `Down` | Scroll through results |
| `Enter` | Select record (generates WHERE template) |
| `ESC` / `Ctrl+C` | Return to SQL input |

## Local Testing with Docker

The repository ships with a `docker-compose.yml` and initialization scripts under `docker/init/` that spin up a PostgreSQL 16 instance pre-loaded with sample data across multiple schemas.

### Available schemas

All schemas reside in a single database (`testdb`). Switch schemas with `SET search_path TO <schema>` or use the `ss` alias.

| Schema | Description |
|--------|-------------|
| `public` | Basic users / products / orders |
| `ecommerce` | Customers, items, transactions, reviews |
| `blog` | Authors, posts, comments, tags |
| `analytics` | Events (~1M rows), sessions, page views |
| `inventory` | Warehouses, products, stock levels, shipments |
| `hr` | Departments, employees, projects, time entries |

### Steps

**1. Start the PostgreSQL container**

```bash
docker compose up -d
```

Wait until the container reports healthy (the init scripts run automatically on first start; the `analytics.events` table takes a minute to populate ~1M rows):

```bash
docker compose ps          # STATUS should show "healthy"
```

**2. Create a config file**

```bash
mkdir -p ~/.config/poshgre
cp config.example.toml ~/.config/poshgre/config.toml
chmod 600 ~/.config/poshgre/config.toml
```

Then edit `~/.config/poshgre/config.toml` so the `local-dev` connection points to the Docker instance:

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

**3. Launch poshgre**

```bash
posh
```

Select `local-dev` from the connection picker and start exploring.

### Stopping and cleaning up

```bash
docker compose down          # stop containers, keep volume
docker compose down -v       # stop containers and remove volume
```

## Development

```bash
# Run all tests
cargo test

# Run integration tests (requires Docker or Podman)
cargo test --test integration_test
```

## License

MIT
