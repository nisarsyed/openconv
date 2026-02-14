<!-- PROJECT_CONFIG
runtime: rust-cargo
test_command: cargo test --workspace
END_PROJECT_CONFIG -->

<!-- SECTION_MANIFEST
section-01-monorepo-setup
section-02-shared-crate
section-03-server-scaffold
section-04-postgres-migrations
section-05-tauri-scaffold
section-06-sqlite-migrations
section-07-react-frontend
section-08-dev-tooling
section-09-testing
END_MANIFEST -->

# Implementation Sections Index

## Project Notes

This is a multi-language project (Rust + TypeScript). The primary build system is Cargo workspace, with an npm workspace nested inside for the React frontend. The `test_command` above covers Rust tests only; frontend tests run separately via `cd apps/desktop && npm test`.

## Dependency Graph

| Section | Depends On | Blocks | Parallelizable With |
|---------|------------|--------|---------------------|
| section-01-monorepo-setup | - | 02, 03, 04, 05, 06, 07, 08 | - |
| section-02-shared-crate | 01 | 03, 04, 05, 06 | - |
| section-03-server-scaffold | 02 | 04 | 05 |
| section-04-postgres-migrations | 03 | 09 | 06, 07 |
| section-05-tauri-scaffold | 02 | 06, 07 | 03 |
| section-06-sqlite-migrations | 05 | 09 | 04, 07 |
| section-07-react-frontend | 05 | 09 | 04, 06 |
| section-08-dev-tooling | 01 | 09 | 03, 04, 05, 06, 07 |
| section-09-testing | 04, 06, 07, 08 | - | - |

## Execution Order (Batches)

1. **Batch 1:** section-01-monorepo-setup (no dependencies)
2. **Batch 2:** section-02-shared-crate (after 01)
3. **Batch 3:** section-03-server-scaffold, section-05-tauri-scaffold, section-08-dev-tooling (parallel after 02/01)
4. **Batch 4:** section-04-postgres-migrations, section-06-sqlite-migrations, section-07-react-frontend (parallel after 03/05)
5. **Batch 5:** section-09-testing (after all implementation sections)

## Section Summaries

### section-01-monorepo-setup
Cargo workspace root `Cargo.toml` with workspace dependencies table, npm workspace root `package.json`, `.gitignore`, `.env.example`, directory structure creation. This section creates the skeleton that all other sections build into.

### section-02-shared-crate
The `openconv-shared` crate: `define_id!` macro and all typed ID newtypes (UserId, GuildId, etc.), API request/response types grouped by domain, shared `OpenConvError` enum with `thiserror`, constants module, and SQLx feature flag for conditional database trait derives.

### section-03-server-scaffold
Axum server binary crate: `ServerConfig` struct with TOML + env loading, `AppState` (Clone, no Arc wrapping), router with TraceLayer/CorsLayer/DefaultBodyLimit/RequestId middleware, health check endpoints (`/health/live`, `/health/ready`), `IntoResponse` for `OpenConvError`, and cross-platform graceful shutdown.

### section-04-postgres-migrations
All 9 SQLx migrations: `set_updated_at()` trigger function, users, pre-key bundles, guilds, channels, roles, guild_members/guild_member_roles, DM tables, unified messages table (nullable channel_id + dm_channel_id with CHECK constraint), files table.

### section-05-tauri-scaffold
Tauri 2.x desktop app: `tauri.conf.json`, capabilities/permissions, `lib.rs` app builder with rusqlite init and TauRPC setup, `main.rs` entry point, `db.rs` SQLite module with connection management, `commands/health.rs` IPC command, system tray placeholder.

### section-06-sqlite-migrations
Client-side SQLite migration runner embedded in Rust: `_migrations` tracking table, all client tables (local_user, cached_users, cached_guilds, cached_channels, cached_messages, cached_files, sync_state). No FTS5 (deferred to messaging split).

### section-07-react-frontend
Vite + React 19 + TypeScript scaffold: `vite.config.ts`, `tsconfig.json`, Tailwind CSS v4 setup, `main.tsx`, `App.tsx` with dark theme and health check IPC call, `package.json` with all dependencies and scripts.

### section-08-dev-tooling
`justfile` with all targets (dev, server, build, db-up/down/migrate/reset, sqlx-prepare, test, lint, fmt), `docker-compose.yml` with PostgreSQL 15 service, `config.toml` with safe dev defaults.

### section-09-testing
Testing infrastructure and initial tests across all crates: shared crate unit tests (ID roundtrips, API type serialization, error types), server integration tests (health endpoints via axum::test, database tests via sqlx::test, migration validation), desktop unit tests (SQLite migrations, db init), frontend smoke test (Vitest + React Testing Library).
