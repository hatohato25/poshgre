# poshgre

A TUI PostgreSQL client for fast database exploration with fuzzy search, written in Rust.

![demo](https://github.com/hatohato25/poshgre/releases/download/v0.1.0/t-rec.gif)

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
brew trust --formula hatohato25/poshgre/poshgre  # Required if HOMEBREW_REQUIRE_TAP_TRUST is set
brew install poshgre
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

See the [Configuration docs](https://hatohato25.github.io/poshgre/docs.html#configuration) for the full settings reference and example configurations. A minimal annotated example is also available in `config.example.toml`.

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

See the [Key Bindings docs](https://hatohato25.github.io/poshgre/docs.html#key-bindings) for the full list of key bindings across the connection selector, SQL input, shell input, AI prompt input, and result viewer.

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
