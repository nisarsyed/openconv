# Implementation Plan: OpenConv Project Foundation

## 1. What We're Building

OpenConv is a privacy-focused desktop chat application — a Discord alternative — built as a Rust monorepo. This foundation split establishes:

1. **Cargo workspace monorepo** with three crates: shared library, Axum server, Tauri desktop client
2. **npm workspace** for the React/TypeScript frontend within the Tauri app
3. **PostgreSQL schema** for the server (all tables: users, guilds, channels, roles, messages, files, pre-keys, DMs)
4. **SQLite schema** for the client (local cache: user profile, guilds, channels, messages, files, sync state)
5. **Axum server scaffold** with SQLx, health check endpoints, middleware, graceful shutdown
6. **Tauri 2.x desktop app scaffold** with React + Vite + Tailwind CSS, rusqlite via IPC, TauRPC for type-safe commands
7. **Dev tooling**: justfile, Docker Compose for PostgreSQL, linting, formatting

The end state: `cargo build` compiles everything, `just dev` launches the Tauri app with hot reload, `just server` starts Axum serving health checks, and both databases initialize with schemas.

## 2. Monorepo Setup

### 2.1 Cargo Workspace

The workspace root `Cargo.toml` declares three members and shared dependency versions:

```
openconv/
├── Cargo.toml          # [workspace] with members and [workspace.dependencies]
├── Cargo.lock
├── crates/
│   └── shared/         # openconv-shared
├── apps/
│   ├── server/         # openconv-server
│   └── desktop/
│       └── src-tauri/  # openconv-desktop
└── target/
```

**Workspace dependencies** ensure consistent versions across crates. Each member's `Cargo.toml` references these via `workspace = true`.

**Pinned workspace dependency versions:**

| Crate | Version | Notes |
|-------|---------|-------|
| `serde` | 1 (features: derive) | Serialization framework |
| `serde_json` | 1 | JSON serialization |
| `uuid` | 1 (features: v7, serde) | Time-sortable UUIDs |
| `thiserror` | 2 | Error derive macros (v2, not v1) |
| `chrono` | 0.4 (features: serde) | Date/time handling |
| `tracing` | 0.1 | Structured logging |
| `tracing-subscriber` | 0.3 (features: env-filter) | Log output |
| `axum` | 0.8 (features: macros) | Web framework |
| `tokio` | 1 (features: full) | Async runtime |
| `sqlx` | 0.8 (features: runtime-tokio, postgres, uuid, chrono) | Database |
| `tower` | 0.5 | Middleware framework |
| `tower-http` | 0.6 (features: trace, cors, limit) | HTTP middleware |
| `dotenvy` | 0.15 | Env file loading |
| `toml` | 0.8 | Config file parsing |
| `rusqlite` | 0.32 (features: bundled) | Client SQLite |

### 2.2 npm Workspace

Root `package.json` with `"private": true` and `"workspaces": ["apps/desktop"]`. The desktop app's `package.json` lives at `apps/desktop/package.json` and contains React, Vite, Tailwind, and TauRPC dependencies.

### 2.3 Git Configuration

`.gitignore` excludes: `target/`, `node_modules/`, `.env`, `*.db`. Include `.env.example` as a template.

**Important:** `.sqlx/` is NOT gitignored — it must be committed for offline CI builds (`SQLX_OFFLINE=true`). Regenerate via `just sqlx-prepare` whenever SQL queries change.

## 3. Shared Crate (openconv-shared)

### 3.1 Typed IDs

Create newtype wrappers around `uuid::Uuid` for each entity: `UserId`, `GuildId`, `ChannelId`, `MessageId`, `RoleId`, `FileId`, `DmChannelId`. Each wrapper derives `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize` and has a `new()` method generating UUID v7. Implement `Display` (formats as UUID string) and `FromStr`.

Use a macro to reduce boilerplate — a single `define_id!` macro that generates the newtype, all derives, and the constructor for each ID type.

### 3.2 API Types

Request/response structs for the REST API contract. These are data-only (no methods beyond serde). Group by domain:

**Auth types:** `RegisterRequest { public_key, email, display_name }`, `RegisterResponse { user_id, token }`, `LoginChallengeRequest { public_key }`, `LoginChallengeResponse { challenge }`, `LoginVerifyRequest { public_key, signature }`, `LoginVerifyResponse { token }`

**Guild types:** `CreateGuildRequest { name }`, `GuildResponse { id, name, owner_id, icon_url, created_at }`, `GuildListResponse { guilds: Vec<GuildResponse> }`

**Channel types:** `CreateChannelRequest { name, channel_type }`, `ChannelResponse { id, guild_id, name, channel_type, position }`

**Message types:** `SendMessageRequest { encrypted_content, nonce }`, `MessageResponse { id, channel_id, sender_id, encrypted_content, nonce, created_at }`

**User types:** `UserProfileResponse { id, display_name, avatar_url }`

These types live in submodules under `src/api/` in the shared crate. Both server and client import them.

### 3.3 Error Types

A shared `OpenConvError` enum with variants: `NotFound`, `Unauthorized`, `Forbidden`, `Validation(String)`, `Internal(String)`. Uses `thiserror` for `Display` and `Error` derives. The server maps these to HTTP status codes; the client uses them for error handling.

### 3.4 SQLx Integration via Feature Flags

The shared crate needs to work with both the server (SQLx/PostgreSQL) and client (rusqlite). Add an optional `sqlx` feature flag:

- When `sqlx` feature is enabled, typed IDs derive `sqlx::Type`, `sqlx::Encode`, `sqlx::Decode`
- The `define_id!` macro conditionally adds these derives via `#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]`
- The server crate enables this feature: `openconv-shared = { path = "../../crates/shared", features = ["sqlx"] }`
- The desktop crate uses the shared crate without the `sqlx` feature

This avoids coupling the shared crate to PostgreSQL while letting the server use typed IDs directly in queries.

### 3.5 Constants

Shared configuration constants: `MAX_FILE_SIZE_BYTES` (25MB), `MAX_DISPLAY_NAME_LENGTH`, `MAX_CHANNEL_NAME_LENGTH`, `MAX_GUILD_NAME_LENGTH`, `MAX_MESSAGE_SIZE_BYTES`.

## 4. Axum Server Scaffold

### 4.1 Configuration

A `ServerConfig` struct loaded from `config.toml` with environment variable overrides. Fields: `host`, `port`, `database_url`, `max_db_connections`, `cors_origins`, `log_level`.

Loading order:
1. Read `config.toml` (using the `toml` crate)
2. Override any field that has a matching environment variable (e.g., `DATABASE_URL` overrides `database_url`)
3. The `dotenvy` crate loads `.env` into the environment before config parsing

`config.toml` is checked into source control with safe development defaults (localhost, default port, etc.). All secrets and machine-specific values come from environment variables (`.env` file). This means a developer can clone and run immediately without creating config files — only `.env` is needed.

### 4.2 Database Setup

Use SQLx with the `PgPool` connection pool. Create the pool at startup with configured `max_connections`. Run pending migrations via `sqlx::migrate!()` (embeds migrations at compile time).

### 4.3 Application State

An `AppState` struct holding `PgPool` and `ServerConfig`. Passed to all handlers via Axum's `State` extractor.

`AppState` derives `Clone` and is passed directly to Axum — no manual `Arc` wrapping needed. `PgPool` is already `Arc` internally, and `ServerConfig` can be wrapped in `Arc<ServerConfig>` as a field of `AppState`. Axum handles the state sharing.

### 4.4 Router & Middleware

Build the Axum router with a middleware stack using Tower's `ServiceBuilder`:

1. **TraceLayer** from `tower-http` — logs method, URI, status, and latency for every request
2. **CorsLayer** from `tower-http` — configured from `cors_origins` in config (permissive in dev)
3. **DefaultBodyLimit** from `tower-http` — set to 2MB default for API endpoints. File upload routes in later splits will override with a higher per-route limit (up to `MAX_FILE_SIZE_BYTES` = 25MB).
4. **Request ID** — custom middleware using `from_fn` that generates a UUID for each request and adds it to response headers and the tracing span

Routes for this split:
- `GET /health/live` → returns 200 with `{"status": "ok"}`
- `GET /health/ready` → queries PostgreSQL (`SELECT 1`), returns 200 if connected, 503 if not

### 4.5 Error Handling

Implement `IntoResponse` for the shared `OpenConvError` type, mapping each variant to an HTTP status code and JSON error body `{"error": "message"}`.

### 4.6 Startup & Shutdown

The `main.rs` entry point:
1. Calls `dotenvy::dotenv().ok()`
2. Initializes `tracing_subscriber` with env filter
3. Loads `ServerConfig`
4. Creates `PgPool` and runs migrations
5. Builds router with state
6. Binds `TcpListener` on configured host:port
7. Calls `axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await`

`shutdown_signal()` uses `tokio::signal::ctrl_c()` as the cross-platform base (works on macOS, Linux, Windows). On Unix, additionally listen for SIGTERM via `#[cfg(unix)]` conditional compilation with `tokio::signal::unix::signal(SignalKind::terminate())`. The function resolves when either signal is received.

## 5. PostgreSQL Migrations

Create SQLx migrations in `apps/server/migrations/` in sequential order. Each migration is a `.sql` file. Migrations are forward-only (no down/rollback files). Recovery from a bad migration is via `just db-reset` (drop and recreate the database).

**Migration 1: Utility functions and users table**
- Create a reusable `set_updated_at()` trigger function that sets `updated_at = NOW()` on any row update
- Create the users table with all columns as specified in the schema section of the spec
- UNIQUE constraint on `public_key` and `email`
- Index on `email`
- Apply `set_updated_at` trigger to users table

**Migration 2: Create pre-key bundles table**
- Foreign key to users
- Index on `user_id`

**Migration 3: Create guilds table**
- Foreign key from `owner_id` to users
- Index on `owner_id`
- Apply `set_updated_at` trigger

**Migration 4: Create channels table**
- Foreign key to guilds with CASCADE DELETE
- Unique constraint on `(guild_id, name)`
- Index on `guild_id`
- Apply `set_updated_at` trigger

**Migration 5: Create roles table**
- Foreign key to guilds with CASCADE DELETE
- Three default roles per guild will be inserted by application code (not migration)

**Migration 6: Create guild_members and guild_member_roles tables**
- Composite primary key `(user_id, guild_id)` on guild_members
- Triple primary key `(user_id, guild_id, role_id)` on guild_member_roles
- Foreign keys with CASCADE DELETE

**Migration 7: Create DM tables**
- `dm_channels` table with id (UUID v7, PK) and created_at
- `dm_channel_members` junction table with composite PK `(dm_channel_id, user_id)`
- Foreign keys to dm_channels and users with CASCADE DELETE

**Migration 8: Create messages table (unified for guild channels and DMs)**
- `channel_id` (UUID, FK → channels, **nullable**)
- `dm_channel_id` (UUID, FK → dm_channels, **nullable**)
- `sender_id` (UUID, FK → users)
- `encrypted_content`, `nonce`, `edited_at`, `deleted`, `created_at`
- **CHECK constraint:** exactly one of `channel_id` or `dm_channel_id` is non-null
- Composite index on `(channel_id, created_at)` for efficient guild message history
- Composite index on `(dm_channel_id, created_at)` for efficient DM message history
- Soft delete via `deleted` boolean

**Design decision on DMs and messages:** Rather than having DM messages in a separate table, messaging is unified. A message belongs to either a `channel_id` (guild channel) or a `dm_channel_id` (DM), never both. The CHECK constraint enforces this at the database level. DM tables are created before messages (Migration 7) so the foreign key to `dm_channels` can be established in Migration 8.

**Migration 9: Create files table**
- Foreign keys to users and messages (nullable — files can exist without messages during upload)
- Index on `message_id`

## 6. Tauri Desktop App Scaffold

### 6.1 Tauri 2.x Setup

Initialize with `cargo tauri init` in the desktop app directory. The `tauri.conf.json` configures:
- App identifier: `com.openconv.desktop`
- Window: title "OpenConv", default size 1200x800, min size 800x600
- Dev server URL: `http://localhost:1420` (Vite dev server)
- Build: `npm run build` for frontend, dist directory `../dist`

### 6.2 Capabilities Configuration

Create `src-tauri/capabilities/default.json` granting the main window:
- `core:default` — basic Tauri functionality
- Custom command permissions for database access and app commands

### 6.3 Rust Backend (src-tauri/src/)

**lib.rs** — Main Tauri app builder:
1. Initialize rusqlite database (open/create SQLite file in Tauri's app data directory via `app.path().app_data_dir()`)
2. Run SQLite migrations (embedded in Rust code as string constants)
3. Set up TauRPC router (Specta-based type generation)
4. Register all IPC command handlers
5. Register system tray (placeholder: show/hide window, quit)
6. Build and return the Tauri app

**main.rs** — Desktop entry point, calls `app_lib::run()`.

**db.rs** — SQLite database module:
- `init_db(path: &Path) -> rusqlite::Connection` — opens or creates the database, runs migrations
- Migration functions that create all client-side tables (local_user, cached_users, cached_guilds, cached_channels, cached_messages, cached_files, sync_state)
- Connection wrapper that can be shared via Tauri's managed state

**commands/mod.rs** — IPC commands exposed to the frontend:
- `health_check` — returns app version and database status
- Database commands will be added by later splits

### 6.4 TauRPC Setup

Add `tauri-specta` (v2) and `specta` (v2) as dependencies. These versions are compatible with Tauri 2.x. Define the TauRPC router in `lib.rs`:
1. Create Specta-based router with typed procedures
2. Generate TypeScript bindings (output to `src/bindings.ts` in the React app)
3. Register the router with the Tauri builder

The generated TypeScript types keep the frontend in sync with Rust commands automatically. Run type generation as part of the build step.

**Fallback note:** If `tauri-specta` v2 has compatibility issues with the Tauri 2.x version used, fall back to `ts-rs` for Rust-to-TypeScript type generation with manual Tauri command wiring. The API types in the shared crate would derive `ts_rs::TS` to generate `.ts` files.

### 6.5 System Tray (Placeholder)

Register a system tray icon with a basic context menu:
- "Show/Hide Window" — toggles main window visibility
- "Quit" — exits the application

Use `tauri::tray::TrayIconBuilder` with `on_menu_event` handler.

## 7. React Frontend Scaffold

### 7.1 Vite + React + TypeScript

Standard Vite setup with `@vitejs/plugin-react`. `vite.config.ts` sets port 1420, strict port, and ignores `src-tauri/` in the file watcher. TypeScript configured in strict mode.

### 7.2 Tailwind CSS

Install `tailwindcss`, `postcss`, `autoprefixer`. Configure `tailwind.config.ts` to scan `./src/**/*.{ts,tsx}`. Set up a base CSS file importing Tailwind's layers. Configure dark mode as `class` strategy with dark as the default.

### 7.3 Application Shell

**main.tsx** — React root with StrictMode.

**App.tsx** — Minimal app component that:
1. Imports TauRPC-generated bindings
2. Calls the health check command on mount to verify IPC works
3. Renders a placeholder layout with the OpenConv title and a status indicator
4. Applies dark theme by default (Tailwind `dark` class on root element)

### 7.4 Project Dependencies

React 19, React DOM, React Router (for future routing), and TauRPC client library. Dev dependencies: TypeScript, Vite, Tailwind CSS v4, ESLint, Prettier.

## 8. SQLite Client Migrations

Embed migrations in Rust code (not SQL files, since rusqlite doesn't have a migration runner like SQLx). Create a `run_migrations(conn: &Connection)` function that:

1. Creates a `_migrations` table if it doesn't exist (version INT, applied_at TEXT)
2. Checks which migrations have been applied
3. Runs pending migrations in order
4. Records each applied migration

Migrations create all tables defined in the client SQLite schema: `local_user`, `cached_users`, `cached_guilds`, `cached_channels`, `cached_messages`, `cached_files`, `sync_state`.

**cached_users table** (new): Stores display names and avatar URLs for known users (not just the current user). Fields: `id TEXT PK`, `display_name TEXT`, `avatar_url TEXT nullable`, `updated_at TEXT`. This is essential for offline message rendering — without it, the UI can't resolve `sender_id` to a display name when offline.

**FTS5 is deferred** to the messaging split (06). The FTS5 virtual table requires an INTEGER rowid column, but `cached_messages` uses `id TEXT PK`. It also requires INSERT/UPDATE/DELETE triggers to stay in sync. Both concerns are better addressed when the messaging infrastructure is built.

**SQLite encryption note:** The `cached_messages` table stores decrypted plaintext for offline access. For a privacy-focused application, the SQLite database file should be encrypted at rest. The crypto split (02) will evaluate SQLCipher (requires `rusqlite` with `bundled-sqlcipher` feature instead of `bundled`) or application-level encryption. The foundation establishes the table structure; encryption wrapping is a crypto split concern.

## 9. Dev Tooling

### 9.1 justfile

Define targets for all common operations:

```just
# Development
dev          # cd apps/desktop && npm run tauri dev
server       # cargo run --bin openconv-server
build        # cargo build --release

# Database
db-up        # docker compose up -d postgres
db-down      # docker compose down
db-migrate   # sqlx migrate run --source apps/server/migrations
db-reset     # sqlx database drop && sqlx database create && just db-migrate
sqlx-prepare # cargo sqlx prepare --workspace

# Testing
test         # cargo test --workspace && cd apps/desktop && npm test
test-rust    # cargo test --workspace
test-js      # cd apps/desktop && npm test

# Linting
lint         # cargo clippy --workspace -- -D warnings && cd apps/desktop && npm run lint
fmt          # cargo fmt --all && cd apps/desktop && npm run fmt
fmt-check    # cargo fmt --all --check && cd apps/desktop && npm run fmt:check
```

### 9.2 Docker Compose

A `docker-compose.yml` at the repo root with a PostgreSQL 15 service:
- Container name: `openconv-postgres`
- Port: 5432 mapped to host
- Environment: `POSTGRES_DB=openconv`, `POSTGRES_USER=openconv`, `POSTGRES_PASSWORD=openconv`
- Volume: `pgdata` for persistence

**Dev-only credentials:** These hardcoded credentials are for local development only and must never be used in any deployed environment. The Docker Compose credentials and `.env.example` `DATABASE_URL` must stay in sync — changing one requires updating the other.

### 9.3 Environment Template

`.env.example` with all required environment variables and their default/example values. Copy to `.env` for local development.

## 10. Testing Foundation

### 10.1 Rust Tests

**Shared crate tests:**
- Unit tests for typed ID creation, serialization/deserialization roundtrips
- Unit tests for API type serialization

**Server tests:**
- Integration test for health check endpoints using `axum::test` (test client)
- Database test using `sqlx::test` attribute (creates temporary test database, runs migrations)

**Desktop tests:**
- Unit tests for SQLite migration logic
- Unit tests for database initialization

### 10.2 Frontend Tests

Configure Vitest with `vitest.config.ts`. Set up React Testing Library. Create a single smoke test that renders the App component and verifies it mounts without crashing.

Add test scripts to `apps/desktop/package.json`: `"test": "vitest run"`, `"test:watch": "vitest"`.

## 11. Build Verification Criteria

The foundation is complete when:

1. `cargo build --workspace` compiles all three crates with zero errors and zero warnings (clippy clean)
2. `just db-up && just db-migrate` creates all PostgreSQL tables
3. `just server` starts Axum, `curl http://localhost:3000/health/live` returns 200
4. `just dev` launches Tauri app, opens a window showing the placeholder UI
5. The Tauri app successfully calls the health check IPC command and displays the result
6. `just test` passes all Rust and JavaScript tests
7. SQLite database is created in the app data directory with all tables
8. TauRPC generates TypeScript bindings that compile without errors
9. `just lint` passes with no warnings
