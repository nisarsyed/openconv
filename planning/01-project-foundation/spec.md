# 01 - Project Foundation

## Overview

Establish the monorepo structure, shared code, application scaffolding, and database schemas for OpenConv — a privacy-focused desktop chat application (Discord alternative) built with Tauri + React/TypeScript (client) and Rust/Axum (server).

This split produces the foundational infrastructure that all other splits build on.

## Original Requirements

See `planning/requirements.md` for the high-level project description.
See `planning/deep_project_interview.md` for full interview context.

## Key Decisions (from interview)

- **Monorepo:** Single repository with Cargo workspace + npm workspace
- **Client:** Tauri 2.x with React + TypeScript frontend
- **Server:** Rust with Axum web framework
- **Client DB:** SQLite (for offline support and local data)
- **Server DB:** PostgreSQL
- **Shared code:** Rust crate for types/models shared between client and server

## Scope

### Monorepo Structure
- Cargo workspace configuration with member crates:
  - `server/` — Axum server binary
  - `client/` (Tauri app) — Rust backend + React frontend
  - `shared/` — Shared Rust crate (types, models, error types)
- npm workspace for the React frontend
- Workspace-level tooling (rustfmt, clippy, eslint, prettier)

### Tauri App Scaffold
- Tauri 2.x project initialization
- React + TypeScript + Vite frontend setup
- Basic window configuration
- Tauri IPC command structure (patterns for Rust ↔ JS communication)
- System tray placeholder

### Axum Server Scaffold
- Axum project with basic routing structure
- Configuration management (env vars, config files)
- PostgreSQL connection pool (sqlx or diesel)
- Basic middleware stack (logging, CORS, request ID)
- Health check endpoint
- Graceful shutdown handling

### Database Schemas
- **PostgreSQL (server):**
  - Users table (id, public_key, email, display_name, avatar_url, created_at)
  - Pre-key bundles table (for Signal protocol key exchange)
  - Guilds table (id, name, owner_id, icon_url, created_at)
  - Channels table (id, guild_id, name, type, position, created_at)
  - Guild members table (user_id, guild_id, role_id, joined_at)
  - Roles table (id, guild_id, name, permissions, position)
  - Messages table (id, channel_id, sender_id, encrypted_content, created_at)
  - Files table (id, uploader_id, encrypted_blob_key, size, mime_type, created_at)
  - Migration tooling (sqlx-migrate or refinery)

- **SQLite (client):**
  - Local user profile and keys
  - Cached messages (decrypted, for offline access and search)
  - Cached guild/channel metadata
  - File cache metadata
  - Migration tooling

### Shared Crate
- Common data types (UserId, GuildId, ChannelId, MessageId — typed IDs)
- API request/response types (serde serializable)
- Error types
- Constants and configuration shared between client and server

### Build & Dev Tooling
- `cargo build` compiles both server and client Rust code
- `npm run dev` launches Tauri app with Vite hot reload
- `cargo run --bin server` starts the server
- Docker Compose for local PostgreSQL
- Basic Makefile or justfile for common commands

## Outputs

A working monorepo where:
1. `cargo build` compiles without errors
2. The Tauri app opens a window with a basic React page
3. The Axum server starts and responds to health checks
4. Both databases can be initialized with schemas
5. Shared types are importable from both server and client crates

## Dependencies

- **Depends on:** Nothing (this is the foundation)
- **Depended on by:** All other splits (02-06)

## Constraints

- Tauri 2.x (not 1.x) — uses the newer plugin system and permission model
- Rust edition 2021+
- React 18+ with TypeScript strict mode
- PostgreSQL 15+
- Target platforms: macOS, Linux, Windows (Tauri cross-compilation)
