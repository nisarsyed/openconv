# TDD Plan: OpenConv Project Foundation

This document defines what tests to write BEFORE implementing each section of the plan. It mirrors the structure of `claude-plan.md`.

## Testing Stack

**Rust:**
- Built-in `#[test]` for unit tests
- `#[tokio::test]` for async tests
- `sqlx::test` for database-dependent server tests (auto-creates temp databases)
- `rusqlite` in-memory databases for client SQLite tests

**Frontend:**
- Vitest for unit and component tests
- React Testing Library for component rendering tests

## 2. Monorepo Setup

No direct tests — this section is workspace configuration. Verified by `cargo build --workspace` compiling successfully and `cargo test --workspace` discovering tests from all three crates.

## 3. Shared Crate (openconv-shared)

### 3.1 Typed IDs

```rust
// Test: UserId::new() generates a valid UUID v7
// Test: UserId serializes to a UUID string via serde_json
// Test: UserId deserializes from a UUID string via serde_json
// Test: UserId roundtrip: serialize then deserialize produces the same value
// Test: UserId Display formats as UUID string
// Test: UserId FromStr parses a valid UUID string
// Test: UserId FromStr rejects an invalid string
// Test: Two calls to UserId::new() produce different IDs
// Test: UserId::new() produces time-sortable IDs (second call > first call lexicographically)
// Repeat the above pattern for all other ID types: GuildId, ChannelId, MessageId, RoleId, FileId, DmChannelId
```

### 3.2 API Types

```rust
// Test: RegisterRequest serializes to JSON with expected field names
// Test: RegisterRequest deserializes from JSON
// Test: GuildResponse roundtrip serialization
// Test: MessageResponse includes all fields in JSON output
// Test: CreateGuildRequest with minimal fields serializes correctly
// Test: ChannelResponse deserializes channel_type as expected type
```

### 3.3 Error Types

```rust
// Test: OpenConvError::NotFound displays correctly
// Test: OpenConvError::Validation contains the message
// Test: OpenConvError::Internal contains the message
// Test: All error variants implement std::error::Error
```

### 3.4 SQLx Integration via Feature Flags

```rust
// Test (with sqlx feature): UserId can be used as a SQLx parameter type
// Test (without sqlx feature): Shared crate compiles without sqlx dependency
// Note: Feature flag compilation is tested by building the server crate (with sqlx)
// and the desktop crate (without sqlx) in the same workspace
```

### 3.5 Constants

```rust
// Test: MAX_FILE_SIZE_BYTES equals 25 * 1024 * 1024
// Test: All length constants are > 0
```

## 4. Axum Server Scaffold

### 4.1 Configuration

```rust
// Test: ServerConfig loads from a valid TOML string
// Test: ServerConfig applies environment variable overrides (DATABASE_URL)
// Test: ServerConfig has correct default values when fields omitted from TOML
// Test: ServerConfig fails gracefully when TOML is malformed
```

### 4.2 Database Setup

```rust
// Test (sqlx::test): PgPool connects successfully to the test database
// Test (sqlx::test): Migrations run without errors on a fresh database
// Test (sqlx::test): Running migrations twice is idempotent
```

### 4.3 Application State

```rust
// Test: AppState implements Clone
// Test: AppState can be constructed from PgPool and ServerConfig
```

### 4.4 Router & Middleware

```rust
// Test: GET /health/live returns 200 with {"status": "ok"}
// Test: GET /health/ready returns 200 when database is connected
// Test: GET /health/ready returns 503 when database is unreachable
// Test: Requests include X-Request-Id in response headers
// Test: CORS headers are present in responses
// Test: Unknown routes return 404
```

### 4.5 Error Handling

```rust
// Test: OpenConvError::NotFound maps to HTTP 404
// Test: OpenConvError::Unauthorized maps to HTTP 401
// Test: OpenConvError::Forbidden maps to HTTP 403
// Test: OpenConvError::Validation maps to HTTP 400
// Test: OpenConvError::Internal maps to HTTP 500
// Test: Error responses are JSON with {"error": "message"} format
```

### 4.6 Startup & Shutdown

```rust
// Test: shutdown_signal resolves on ctrl_c signal (unit test with tokio signal simulation)
// Note: Full startup/shutdown is an integration concern tested via health check endpoints
```

## 5. PostgreSQL Migrations

```rust
// Test (sqlx::test): All migrations apply successfully to a fresh database
// Test (sqlx::test): Users table has expected columns (insert and select a row)
// Test (sqlx::test): Users table enforces UNIQUE on public_key
// Test (sqlx::test): Users table enforces UNIQUE on email
// Test (sqlx::test): Guilds table FK on owner_id rejects nonexistent user
// Test (sqlx::test): Channels table CASCADE deletes when guild is deleted
// Test (sqlx::test): Channels table enforces UNIQUE on (guild_id, name)
// Test (sqlx::test): Messages table CHECK constraint rejects null channel_id AND null dm_channel_id
// Test (sqlx::test): Messages table CHECK constraint rejects both channel_id AND dm_channel_id set
// Test (sqlx::test): Messages table accepts channel_id set, dm_channel_id null
// Test (sqlx::test): Messages table accepts dm_channel_id set, channel_id null
// Test (sqlx::test): Guild members composite PK prevents duplicate membership
// Test (sqlx::test): DM channel members composite PK prevents duplicate membership
// Test (sqlx::test): Files table allows null message_id
// Test (sqlx::test): updated_at trigger auto-updates on user row modification
// Test (sqlx::test): updated_at trigger auto-updates on guild row modification
```

## 6. Tauri Desktop App Scaffold

### 6.1-6.2 Tauri Setup & Capabilities

```
// No unit tests — verified by Tauri app compiling and launching
// Capabilities tested by IPC commands working from the frontend
```

### 6.3 Rust Backend

```rust
// Test: health_check command returns expected AppHealth struct
// Test: health_check command includes app version
```

### 6.4 TauRPC Setup

```
// Verified by: TypeScript bindings file generates without errors
// Verified by: Frontend code compiles using the generated types
```

### 6.5 System Tray

```
// No unit tests — system tray is a platform integration
// Verified by: Tauri app shows tray icon when launched
```

## 7. React Frontend Scaffold

### 7.1-7.2 Vite + React + Tailwind

```typescript
// Test: App component renders without crashing (smoke test)
// Test: App component renders the OpenConv title text
```

### 7.3 Application Shell

```typescript
// Test: App component mounts in dark mode (has 'dark' class on root)
// Test: App component displays a status indicator element
```

### 7.4 Project Dependencies

```
// Verified by: npm install succeeds with no peer dependency errors
// Verified by: npm run build produces output in dist/
```

## 8. SQLite Client Migrations

```rust
// Test: run_migrations creates _migrations table on fresh in-memory database
// Test: run_migrations creates all expected tables (local_user, cached_users, cached_guilds, cached_channels, cached_messages, cached_files, sync_state)
// Test: run_migrations is idempotent (running twice doesn't error)
// Test: _migrations table records applied migrations with version numbers
// Test: local_user table has expected columns (insert and query)
// Test: cached_users table has expected columns (insert and query)
// Test: cached_messages table has expected columns and index on (channel_id, created_at)
// Test: cached_guilds table has expected columns
// Test: sync_state table has channel_id as primary key
```

## 9. Dev Tooling

### 9.1 justfile

```
// No unit tests — verified by running each just target
// Integration check: `just --list` shows all expected targets
```

### 9.2 Docker Compose

```
// No unit tests — verified by `docker compose up -d` starting PostgreSQL
// Integration check: `docker compose ps` shows healthy container
```

## 10. Testing Foundation

```
// Meta-test: `cargo test --workspace` discovers and runs tests from all three crates
// Meta-test: `npm test` in apps/desktop runs Vitest and passes
// Meta-test: `just test` runs both Rust and JS tests successfully
```

## 11. Build Verification Criteria

These are end-to-end verification steps, not unit tests:

```
// Verify: cargo build --workspace compiles with zero errors and zero warnings
// Verify: just db-up && just db-migrate creates all PostgreSQL tables
// Verify: just server starts, curl /health/live returns 200
// Verify: just dev launches Tauri window with placeholder UI
// Verify: Tauri health check IPC command works from frontend
// Verify: just test passes all tests
// Verify: SQLite database created with all tables in app data dir
// Verify: TauRPC TypeScript bindings compile
// Verify: just lint passes with no warnings
```
