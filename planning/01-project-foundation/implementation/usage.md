# OpenConv Foundation - Usage Guide

## Quick Start

### Prerequisites

- Rust toolchain (stable) with `clippy` and `rustfmt`
- Node.js (18+) and npm
- Docker (for PostgreSQL)
- `just` command runner: `cargo install just`
- `sqlx-cli`: `cargo install sqlx-cli --no-default-features --features postgres`

### Initial Setup

```bash
# Clone and enter the project
cd openconv

# Install JS dependencies
npm install

# Start PostgreSQL
just db-up

# Run migrations
just db-migrate

# Verify everything works
just test
```

### Development Commands

```bash
# Launch Tauri desktop app with hot reload
just dev

# Start the Axum server standalone
just server

# Run all tests (Rust + JavaScript)
just test

# Run only Rust tests
just test-rust

# Run only JavaScript tests
just test-js

# Lint all code (Clippy + ESLint)
just lint

# Format all code
just fmt

# Check formatting without modifying
just fmt-check

# Database operations
just db-up          # Start PostgreSQL container
just db-down        # Stop containers
just db-migrate     # Run pending migrations
just db-reset       # Drop, recreate, and migrate
just sqlx-prepare   # Generate offline query data
```

## Project Structure

```
openconv/
├── Cargo.toml                 # Workspace root
├── package.json               # npm workspace root
├── justfile                   # Development commands
├── docker-compose.yml         # PostgreSQL service
├── config.toml                # Server configuration
├── .env                       # DATABASE_URL
│
├── crates/
│   └── shared/                # openconv-shared crate
│       └── src/
│           ├── ids.rs         # Typed UUID v7 IDs (UserId, GuildId, etc.)
│           ├── api/           # API request/response types
│           ├── error.rs       # Shared error types
│           └── constants.rs   # Limits and constants
│
├── apps/
│   ├── server/                # openconv-server crate (Axum)
│   │   ├── src/
│   │   │   ├── config.rs      # TOML config with env overrides
│   │   │   ├── router.rs      # Routes + middleware (CORS, request ID)
│   │   │   ├── handlers/      # Health endpoints
│   │   │   ├── state.rs       # AppState (PgPool + config)
│   │   │   └── error.rs       # HTTP error mapping
│   │   ├── migrations/        # 9 PostgreSQL migrations
│   │   └── tests/             # Integration tests (health, migrations)
│   │
│   └── desktop/               # Tauri 2.x desktop app
│       ├── src/               # React frontend (Vite + Tailwind v4)
│       │   ├── App.tsx        # Main component with health check
│       │   └── __tests__/     # Vitest tests
│       ├── src-tauri/         # openconv-desktop crate
│       │   └── src/
│       │       ├── db.rs      # SQLite with WAL + 7 cache tables
│       │       └── commands/  # Tauri IPC commands (health check)
│       └── vitest.config.ts   # Test configuration
```

## API Endpoints

### Health

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health/live` | Liveness check - returns `{"status": "ok"}` |
| GET | `/health/ready` | Readiness check - verifies database connectivity |

### Middleware

All responses include:
- `x-request-id` header (UUID v4)
- CORS headers for configured origins (default: `http://localhost:1420`)

## Database Schema

### PostgreSQL (Server)

9 tables created by migrations:
- `users` - User accounts with public keys
- `pre_key_bundles` - Key exchange bundles
- `guilds` - Guild/server entities
- `channels` - Text channels within guilds
- `roles` - Permission roles
- `guild_members` - Guild membership
- `dm_channels` - Direct message channels
- `messages` - Encrypted messages (guild or DM)
- `files` - File attachments

### SQLite (Desktop Client)

7 cache tables + migrations table:
- `local_user` - Authenticated user session
- `cached_users` - User profile cache
- `cached_guilds` - Guild cache
- `cached_channels` - Channel cache
- `cached_messages` - Message cache (indexed by channel + created_at)
- `cached_files` - File metadata cache
- `sync_state` - Per-channel sync cursor

## Test Summary

| Crate | Tests | Type |
|-------|-------|------|
| openconv-shared | 28 | Unit (IDs, API types, errors, constants) |
| openconv-server (unit) | 11 | Unit (config, state, error mapping) |
| openconv-server (integration) | 22 | Integration (health endpoints, PG migrations) |
| openconv-desktop | 15 | Unit (SQLite db, migrations, health command) |
| React frontend | 5 | Component (App smoke tests) |
| **Total** | **81** | |

## Configuration

### Server (`config.toml`)

```toml
host = "127.0.0.1"
port = 3000
database_url = "postgresql://openconv:openconv@localhost:5432/openconv"
max_db_connections = 5
cors_origins = ["http://localhost:1420"]
log_level = "info"
```

All fields can be overridden via environment variables: `HOST`, `PORT`, `DATABASE_URL`, `MAX_DB_CONNECTIONS`, `LOG_LEVEL`.
