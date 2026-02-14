The project has not been started yet -- all files need to be created from scratch. Now I have all the context needed to write the section.

# Section 08: Dev Tooling

## Overview

This section creates the development tooling infrastructure for the OpenConv project: a `justfile` with all common development targets, a `docker-compose.yml` for running PostgreSQL locally, and a `config.toml` with safe development defaults for the server.

## Dependencies

- **section-01-monorepo-setup** must be completed first (provides the workspace root structure, `.gitignore`, `.env.example`, and directory layout)

This section can be implemented in parallel with sections 03 (server scaffold), 04 (Postgres migrations), 05 (Tauri scaffold), 06 (SQLite migrations), and 07 (React frontend).

## Tests

Dev tooling does not have traditional unit tests. Verification is done by running the tools themselves.

From the TDD plan:

```
// justfile
// No unit tests -- verified by running each just target
// Integration check: `just --list` shows all expected targets

// Docker Compose
// No unit tests -- verified by `docker compose up -d` starting PostgreSQL
// Integration check: `docker compose ps` shows healthy container
```

**Manual verification checklist after implementation:**

1. `just --list` shows all expected targets (dev, server, build, db-up, db-down, db-migrate, db-reset, sqlx-prepare, test, test-rust, test-js, lint, fmt, fmt-check)
2. `docker compose up -d` starts a PostgreSQL 15 container named `openconv-postgres`
3. `docker compose ps` shows the container as healthy/running
4. The `DATABASE_URL` in `.env.example` matches the credentials in `docker-compose.yml`
5. `config.toml` values provide safe localhost-only defaults

## Files to Create

| File Path | Purpose |
|-----------|---------|
| `/Users/nisar/personal/projects/openconv/justfile` | Task runner with all dev targets |
| `/Users/nisar/personal/projects/openconv/docker-compose.yml` | PostgreSQL 15 service for local dev |
| `/Users/nisar/personal/projects/openconv/config.toml` | Server configuration with dev defaults |

## File to Modify

| File Path | Change |
|-----------|--------|
| `.env.example` | Verified DATABASE_URL matches docker-compose credentials. Also changed SERVER_HOST from 0.0.0.0 to 127.0.0.1 for localhost-only safety (code review fix). |

## Deviations from Plan

1. **Default justfile recipe added**: `default` recipe runs `just --list` so bare `just` shows available targets (code review fix).
2. **SERVER_HOST fixed**: .env.example had SERVER_HOST=0.0.0.0 from section-01, contradicting the plan's safety requirement for localhost-only binding. Changed to 127.0.0.1 (code review fix).
3. **Note on existing apps/server/config.toml**: A config.toml already exists at `apps/server/config.toml` from section-03. The root `config.toml` created here is used when running from the repo root (e.g., `just server`). Uses `postgres://` scheme (both `postgres://` and `postgresql://` are valid).

---

## Implementation Details

### 1. justfile

**File:** `/Users/nisar/personal/projects/openconv/justfile`

The `justfile` uses the [just](https://github.com/casey/just) command runner. It defines targets for all common development operations, organized into logical groups. The file should use `just` syntax (not `make` syntax -- notably, `just` uses plain indentation, not tab-only indentation, and variables use `:=` assignment).

The justfile should define the following targets with their exact commands:

**Development targets:**

- `dev` -- Changes into `apps/desktop` and runs `npm run tauri dev`. This launches the Tauri app with Vite hot-reload for the frontend and Cargo watch for the Rust backend.
- `server` -- Runs `cargo run --bin openconv-server`. This starts the Axum server directly.
- `build` -- Runs `cargo build --release`. Builds all workspace crates in release mode.

**Database targets:**

- `db-up` -- Runs `docker compose up -d postgres`. Starts just the PostgreSQL container in detached mode.
- `db-down` -- Runs `docker compose down`. Stops and removes containers.
- `db-migrate` -- Runs `sqlx migrate run --source apps/server/migrations`. Applies pending PostgreSQL migrations. Requires the `sqlx-cli` tool to be installed (`cargo install sqlx-cli`).
- `db-reset` -- Runs `sqlx database drop -y`, then `sqlx database create`, then calls `just db-migrate`. Fully resets the database (drops, recreates, migrates). The `-y` flag skips the confirmation prompt.
- `sqlx-prepare` -- Runs `cargo sqlx prepare --workspace`. Generates the `.sqlx/` offline query data that gets committed to source control for CI builds.

**Testing targets:**

- `test` -- Runs `cargo test --workspace` followed by `cd apps/desktop && npm test`. Executes both Rust and JavaScript test suites.
- `test-rust` -- Runs `cargo test --workspace`. Rust tests only.
- `test-js` -- Changes into `apps/desktop` and runs `npm test`. JavaScript/Vitest tests only.

**Linting and formatting targets:**

- `lint` -- Runs `cargo clippy --workspace -- -D warnings` followed by `cd apps/desktop && npm run lint`. Runs Clippy (treating warnings as errors) and ESLint.
- `fmt` -- Runs `cargo fmt --all` followed by `cd apps/desktop && npm run fmt`. Formats all Rust and TypeScript/JavaScript code.
- `fmt-check` -- Runs `cargo fmt --all --check` followed by `cd apps/desktop && npm run fmt:check`. Checks formatting without modifying files (useful in CI).

**Recommended justfile structure:**

```just
# OpenConv Development Commands

# Launch Tauri desktop app with hot reload
dev:
    cd apps/desktop && npm run tauri dev

# Start the Axum server
server:
    cargo run --bin openconv-server

# Build all crates in release mode
build:
    cargo build --release

# Start PostgreSQL container
db-up:
    docker compose up -d postgres

# Stop and remove containers
db-down:
    docker compose down

# Run pending PostgreSQL migrations
db-migrate:
    sqlx migrate run --source apps/server/migrations

# Drop, recreate, and migrate the database
db-reset:
    sqlx database drop -y
    sqlx database create
    just db-migrate

# Generate SQLx offline query data for CI
sqlx-prepare:
    cargo sqlx prepare --workspace

# Run all tests (Rust + JavaScript)
test:
    cargo test --workspace
    cd apps/desktop && npm test

# Run Rust tests only
test-rust:
    cargo test --workspace

# Run JavaScript tests only
test-js:
    cd apps/desktop && npm test

# Lint all code (Clippy + ESLint)
lint:
    cargo clippy --workspace -- -D warnings
    cd apps/desktop && npm run lint

# Format all code
fmt:
    cargo fmt --all
    cd apps/desktop && npm run fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt --all --check
    cd apps/desktop && npm run fmt:check
```

**Note on `just` prerequisites:** The `db-reset` target calls `just db-migrate` rather than duplicating the migration command. This ensures the migration source path is defined in one place only. Each line in a just recipe runs in its own shell by default, so `cd` does not persist across lines -- use `cd dir && command` on a single line when needed.

---

### 2. Docker Compose

**File:** `/Users/nisar/personal/projects/openconv/docker-compose.yml`

A Docker Compose file at the repository root defining a single PostgreSQL 15 service for local development.

Key configuration:

- **Image:** `postgres:15` (pinned major version for stability)
- **Container name:** `openconv-postgres` (fixed name for easy reference in scripts and docs)
- **Port mapping:** `5432:5432` (host:container, standard PostgreSQL port)
- **Environment variables:**
  - `POSTGRES_DB=openconv` -- the database name
  - `POSTGRES_USER=openconv` -- the database user
  - `POSTGRES_PASSWORD=openconv` -- the database password
- **Volume:** A named volume `pgdata` mounted to `/var/lib/postgresql/data` for data persistence across container restarts
- **Healthcheck:** Use `pg_isready -U openconv` to determine when PostgreSQL is accepting connections

**IMPORTANT: Dev-only credentials.** The credentials `openconv/openconv` are exclusively for local development and must never be used in any deployed environment. The `DATABASE_URL` in `.env.example` must stay in sync with these credentials. If either is changed, the other must be updated to match.

```yaml
services:
  postgres:
    image: postgres:15
    container_name: openconv-postgres
    ports:
      - "5432:5432"
    environment:
      POSTGRES_DB: openconv
      POSTGRES_USER: openconv
      POSTGRES_PASSWORD: openconv
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U openconv"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

---

### 3. Server Configuration File

**File:** `/Users/nisar/personal/projects/openconv/config.toml`

This file provides safe development defaults for the Axum server. It is checked into source control. All secrets and machine-specific values come from environment variables (via `.env` file) which override the values in this file.

The loading order (implemented in section-03-server-scaffold) is:
1. Read `config.toml`
2. Override any field that has a matching environment variable (e.g., `DATABASE_URL` overrides `database_url`)
3. `dotenvy` loads `.env` into the environment before config parsing

**Config fields and their dev defaults:**

- `host` = `"127.0.0.1"` -- bind to localhost only (not `0.0.0.0`) for safety
- `port` = `3000` -- standard dev port for the API server
- `database_url` = `"postgres://openconv:openconv@localhost:5432/openconv"` -- matches Docker Compose credentials exactly
- `max_db_connections` = `5` -- conservative pool size for local dev
- `cors_origins` = `["http://localhost:1420"]` -- allows requests from the Vite dev server (Tauri frontend). Permissive in dev; production will restrict this.
- `log_level` = `"debug"` -- verbose logging for development

```toml
# OpenConv Server Configuration
# Development defaults -- safe for local use only.
# Environment variables override these values (e.g., DATABASE_URL).

host = "127.0.0.1"
port = 3000
database_url = "postgres://openconv:openconv@localhost:5432/openconv"
max_db_connections = 5
cors_origins = ["http://localhost:1420"]
log_level = "debug"
```

---

### 4. Verify .env.example Consistency

**File:** `/Users/nisar/personal/projects/openconv/.env.example`

This file is created by section-01-monorepo-setup. After implementing this section, verify that the `DATABASE_URL` in `.env.example` matches the Docker Compose credentials:

```
DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv
```

The database URL uses the same `openconv` user, `openconv` password, `localhost:5432` host/port, and `openconv` database name as defined in `docker-compose.yml`. If section-01 has already been implemented and the values differ, update `.env.example` to match.

---

## Verification Checklist

After implementing all files, verify the following:

1. **justfile targets listed:** Run `just --list` and confirm all 14 targets appear (dev, server, build, db-up, db-down, db-migrate, db-reset, sqlx-prepare, test, test-rust, test-js, lint, fmt, fmt-check).

2. **Docker Compose starts PostgreSQL:** Run `just db-up` (or `docker compose up -d postgres`). Then run `docker compose ps` and confirm the `openconv-postgres` container is running and healthy.

3. **Database is accessible:** After `just db-up`, run `psql postgres://openconv:openconv@localhost:5432/openconv -c "SELECT 1"` to confirm connectivity. (Alternatively, `docker exec openconv-postgres psql -U openconv -c "SELECT 1"`.)

4. **Credential sync:** Confirm that `docker-compose.yml` credentials, `config.toml` `database_url`, and `.env.example` `DATABASE_URL` all contain the same user/password/host/port/database values.

5. **Config.toml is safe:** Confirm `host` is `127.0.0.1` (not `0.0.0.0`), port is `3000`, and `log_level` is `debug`.