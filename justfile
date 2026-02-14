# OpenConv Development Commands

default:
    @just --list

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
