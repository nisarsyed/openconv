# Complete Specification: OpenConv Project Foundation

## Project Context

OpenConv is a privacy-focused, lightweight desktop chat application (Discord alternative) built with:
- **Desktop Client:** Tauri 2.x + React + TypeScript + Vite
- **Server:** Rust + Axum 0.8+
- **Client DB:** SQLite via rusqlite (exposed through Tauri IPC commands)
- **Server DB:** PostgreSQL via SQLx
- **Monorepo:** Cargo workspace + npm workspace in a single repository
- **E2E Encryption:** Signal protocol (handled in later split, but schema must accommodate)

This foundation split establishes the monorepo structure, scaffolding for both apps, database schemas, shared code, and dev tooling.

## Decisions

### From Deep-Project Interview
- Monorepo with shared Rust crate between client and server
- Tauri 2.x (not 1.x) with capabilities/permissions system
- Axum web framework with SQLx for PostgreSQL
- Discord-style chat model: servers/guilds → channels → messages
- Signal protocol E2E encryption (schema must support pre-key bundles)
- Keypair-based identity with email recovery

### From Deep-Plan Interview
- **ID strategy:** UUID v7 — time-sortable, no coordination needed
- **Client SQLite:** rusqlite via Tauri commands (Rust-side queries, better for crypto ops)
- **Server config:** dotenvy for env vars + TOML config file with env overrides
- **Task runner:** just (justfile)
- **Schema scope:** All tables defined in this split (users, guilds, channels, roles, messages, files, pre-keys)
- **Styling:** Tailwind CSS
- **IPC types:** TauRPC for auto-generated TypeScript types from Rust via Specta

### From Research
- SQLx 0.8 with compile-time query verification and built-in migration tooling
- Tower middleware ecosystem (tracing, CORS, auth)
- Tauri 2.x capabilities system replaces v1 allowlist
- TauRPC provides bidirectional type-safe IPC
- `sqlx prepare` for offline CI builds

## Monorepo Structure

```
openconv/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── package.json                  # npm workspace root (private: true)
├── justfile                      # Task runner
├── docker-compose.yml            # Local PostgreSQL
├── .env.example                  # Environment variable template
├── .gitignore
│
├── crates/
│   └── shared/                   # Shared Rust crate: openconv-shared
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── ids.rs            # Typed UUID IDs (UserId, GuildId, etc.)
│           ├── models.rs         # Shared domain models
│           ├── api.rs            # API request/response types
│           └── error.rs          # Shared error types
│
├── apps/
│   ├── server/                   # Axum server: openconv-server
│   │   ├── Cargo.toml
│   │   ├── config.toml           # Server configuration
│   │   ├── migrations/           # SQLx PostgreSQL migrations
│   │   └── src/
│   │       ├── main.rs           # Entry point, server startup
│   │       ├── config.rs         # Config loading (TOML + env)
│   │       ├── db.rs             # Database pool setup
│   │       ├── routes/           # Route modules
│   │       │   ├── mod.rs
│   │       │   └── health.rs     # Health check endpoint
│   │       ├── middleware/        # Tower middleware
│   │       │   ├── mod.rs
│   │       │   └── request_id.rs
│   │       └── error.rs          # Server error types → HTTP responses
│   │
│   └── desktop/                  # Tauri app: openconv-desktop
│       ├── src-tauri/
│       │   ├── Cargo.toml
│       │   ├── tauri.conf.json
│       │   ├── build.rs
│       │   ├── capabilities/
│       │   │   └── default.json
│       │   └── src/
│       │       ├── lib.rs        # Tauri app setup, plugin registration
│       │       ├── main.rs       # Desktop entry point
│       │       ├── db.rs         # SQLite setup via rusqlite
│       │       └── commands/     # IPC commands
│       │           ├── mod.rs
│       │           └── health.rs
│       ├── src/                  # React frontend
│       │   ├── main.tsx
│       │   ├── App.tsx
│       │   └── ...
│       ├── package.json
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── tailwind.config.ts
│       └── postcss.config.js
│
└── target/                       # Shared Cargo build output
```

## Cargo Workspace Configuration

Root `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "crates/shared",
    "apps/server",
    "apps/desktop/src-tauri",
]

[workspace.dependencies]
# Shared dependencies with consistent versions
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v7", "serde"] }
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
```

## npm Workspace Configuration

Root `package.json`:
```json
{
  "private": true,
  "workspaces": ["apps/desktop"]
}
```

## Database Schemas

### PostgreSQL (Server) — Full Schema

**Users:**
- id (UUID v7, PK), public_key (BYTEA, UNIQUE), email (VARCHAR, UNIQUE), display_name (VARCHAR), avatar_url (VARCHAR nullable), created_at (TIMESTAMPTZ), updated_at (TIMESTAMPTZ)

**Pre-Key Bundles:**
- id (UUID v7, PK), user_id (UUID, FK → users), identity_key (BYTEA), signed_pre_key (BYTEA), signed_pre_key_signature (BYTEA), one_time_pre_keys (BYTEA[]), created_at (TIMESTAMPTZ)

**Guilds:**
- id (UUID v7, PK), name (VARCHAR), owner_id (UUID, FK → users), icon_url (VARCHAR nullable), created_at (TIMESTAMPTZ), updated_at (TIMESTAMPTZ)

**Channels:**
- id (UUID v7, PK), guild_id (UUID, FK → guilds), name (VARCHAR), topic (VARCHAR nullable), channel_type (SMALLINT, default 0 = text), position (INT), created_at (TIMESTAMPTZ), updated_at (TIMESTAMPTZ)

**Roles:**
- id (UUID v7, PK), guild_id (UUID, FK → guilds), name (VARCHAR), permissions (BIGINT, bitfield), color (INT nullable), position (INT), created_at (TIMESTAMPTZ)

**Guild Members:**
- user_id (UUID, FK → users), guild_id (UUID, FK → guilds), nickname (VARCHAR nullable), joined_at (TIMESTAMPTZ), PRIMARY KEY (user_id, guild_id)

**Guild Member Roles:**
- user_id (UUID), guild_id (UUID), role_id (UUID, FK → roles), FOREIGN KEY (user_id, guild_id) REFERENCES guild_members, PRIMARY KEY (user_id, guild_id, role_id)

**Messages:**
- id (UUID v7, PK), channel_id (UUID, FK → channels), sender_id (UUID, FK → users), encrypted_content (BYTEA), nonce (BYTEA), edited_at (TIMESTAMPTZ nullable), deleted (BOOLEAN default false), created_at (TIMESTAMPTZ)
- INDEX on (channel_id, created_at)

**Files:**
- id (UUID v7, PK), uploader_id (UUID, FK → users), message_id (UUID, FK → messages, nullable), encrypted_blob_key (VARCHAR), file_name (VARCHAR), size_bytes (BIGINT), mime_type (VARCHAR), created_at (TIMESTAMPTZ)

**DM Channels:**
- id (UUID v7, PK), created_at (TIMESTAMPTZ)

**DM Channel Members:**
- dm_channel_id (UUID, FK → dm_channels), user_id (UUID, FK → users), PRIMARY KEY (dm_channel_id, user_id)

### SQLite (Client) — Local Cache Schema

**local_user:**
- id (TEXT PK), display_name (TEXT), email (TEXT), avatar_url (TEXT nullable)

**cached_guilds:**
- id (TEXT PK), name (TEXT), icon_url (TEXT nullable), owner_id (TEXT), updated_at (TEXT)

**cached_channels:**
- id (TEXT PK), guild_id (TEXT), name (TEXT), channel_type (INTEGER), position (INTEGER), updated_at (TEXT)

**cached_messages:**
- id (TEXT PK), channel_id (TEXT), sender_id (TEXT), content (TEXT, decrypted plaintext), created_at (TEXT), edited_at (TEXT nullable)
- INDEX on (channel_id, created_at)

**cached_files:**
- id (TEXT PK), message_id (TEXT), file_name (TEXT), size_bytes (INTEGER), mime_type (TEXT), local_path (TEXT nullable)

**sync_state:**
- channel_id (TEXT PK), last_message_id (TEXT), last_sync_at (TEXT)

## Shared Crate (openconv-shared)

### Typed IDs
Newtype wrappers around UUID v7 with serde support, Display, and construction helpers:
- `UserId`, `GuildId`, `ChannelId`, `MessageId`, `RoleId`, `FileId`, `DmChannelId`

### API Types
Request/response structs for server ↔ client communication, serializable with serde. These are the "contract" between server and client.

### Error Types
Shared error enum used by both server and client crates.

## Axum Server Scaffold

### Startup Flow
1. Load config (TOML file + env overrides via dotenvy)
2. Initialize tracing subscriber
3. Create PostgreSQL connection pool (SQLx)
4. Run pending migrations
5. Build Axum router with middleware stack
6. Bind to configured address
7. Serve with graceful shutdown handler

### Middleware Stack
1. TraceLayer (tower-http) — request/response tracing
2. CorsLayer (tower-http) — CORS headers
3. Request ID injection
4. (Auth middleware placeholder — implemented in split 03)

### Routes (Foundation)
- `GET /health/live` — liveness probe (returns 200)
- `GET /health/ready` — readiness probe (checks DB connection)

## Tauri App Scaffold

### Setup Flow
1. Initialize rusqlite database (create/open SQLite file in app data dir)
2. Run SQLite migrations
3. Register TauRPC router (Specta-based type-safe IPC)
4. Register Tauri commands
5. Configure system tray (placeholder)
6. Create main window

### IPC Commands (Foundation)
- Health/connectivity check commands
- SQLite query wrappers (exposed as typed TauRPC procedures)

### React Frontend (Foundation)
- Vite + React + TypeScript scaffold
- Tailwind CSS configured
- TauRPC client initialized
- Basic App component with routing placeholder
- Dark theme default CSS setup

### Capabilities
- `core:default`
- Custom command permissions for database access

## Dev Tooling

### justfile Targets
- `dev` — Start Tauri app in dev mode (Vite HMR + Tauri)
- `server` — Start Axum server locally
- `build` — Build both client and server in release mode
- `test` — Run all Rust tests + frontend tests
- `db-up` — Start PostgreSQL via Docker Compose
- `db-migrate` — Run SQLx migrations
- `db-reset` — Drop and recreate database
- `sqlx-prepare` — Generate offline query metadata for CI
- `lint` — Run clippy + eslint
- `fmt` — Run rustfmt + prettier

### Docker Compose
- PostgreSQL 15+ container with persistent volume
- Configured database name, user, password matching `.env.example`

### .env.example
```
DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
RUST_LOG=info,openconv=debug
```

## Testing Setup

### Rust
- Unit tests via `#[test]` and `#[tokio::test]` in each crate
- `sqlx::test` for server database tests (auto-creates test DB)
- Integration tests in `apps/server/tests/`

### Frontend
- Vitest for unit/component tests
- React Testing Library for React component tests
- Test config in `vitest.config.ts`

## Constraints
- Tauri 2.x (capabilities/permissions system, not v1 allowlist)
- Rust edition 2021+
- React 18+ with TypeScript strict mode
- PostgreSQL 15+
- Target platforms: macOS, Linux, Windows
- SQLx offline mode for CI (no DB required for cargo build)
