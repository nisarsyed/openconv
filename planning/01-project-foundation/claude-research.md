# Research Findings: Project Foundation

## Tauri 2.x Project Setup & Cargo Workspace Monorepo

### Project Structure
Tauri 2.x projects have two parts: frontend (JS/TS at project root) and backend (Rust in `src-tauri/`). The `src-tauri/` directory contains `tauri.conf.json`, `src/lib.rs` (mobile entry point), `src/main.rs` (desktop entry point), `build.rs`, `capabilities/` directory, and `icons/`.

### Recommended Monorepo Layout
```
project-root/
├── Cargo.toml (workspace root)
├── package.json (npm workspace root)
├── crates/
│   └── shared/         # Shared Rust crate
├── apps/
│   ├── desktop/        # Tauri app
│   │   ├── src-tauri/
│   │   ├── src/        # React frontend
│   │   └── package.json
│   └── server/         # Axum server
└── target/             # Shared build output
```

All workspace members share a single `Cargo.lock` and build into a unified `target/` directory. The `src-tauri` folder can be a workspace member. npm workspaces (v7+) use a `workspaces` field in root `package.json` with `"private": true`.

### Tauri 2.x CLI
- `tauri init` generates initial `tauri.conf.json`
- CLI available as `cargo-tauri` (Rust) or `@tauri-apps/cli` (Node)
- Invoked as `cargo tauri` or `npx tauri`

### Key Gotchas
- `src-tauri` folder name is flexible — Tauri locates project via `tauri.conf.json`
- In a workspace, `target/` directory is at workspace root
- Tauri CLI must be invoked from the directory containing `tauri.conf.json`

---

## Axum Server with SQLx and PostgreSQL

### SQLx vs Diesel — Recommendation: SQLx
- **SQLx**: Pure async/await, compile-time SQL verification via `query!` macro, raw SQL control, native async support ideal for Axum
- **Diesel**: More mature (v2.0), type-safe query builder DSL, slightly faster in pure ORM mode
- **For async Axum stack → use SQLx**

### Core Dependencies (Axum 0.8+)
```toml
axum = { version = "0.8", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }
serde = { version = "1", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror = "1"
dotenvy = "0.15"
```

### Connection Pooling
```rust
let pool = PgPoolOptions::new()
    .max_connections(50)
    .connect(&env::var("DATABASE_URL")?)
    .await?;
```
Share via Axum's `State` extractor with `AppState { db: PgPool }`.

### Migration Tooling — Recommendation: SQLx built-in
- `sqlx migrate add <name>`, `sqlx migrate run`, `sqlx migrate revert`
- Migrations stored in `./migrations/` directory
- Can be embedded at compile-time with `migrate!` macro
- `sqlx prepare` generates `.sqlx/` metadata for offline CI builds (`SQLX_OFFLINE=true`)

### Middleware Stack (Tower)
- **Tracing**: `tower-http::TraceLayer` + `tracing-subscriber`
- **CORS**: `tower-http::cors::CorsLayer`
- **Request ID**: `axum-trace-id` crate
- **Auth**: `axum::middleware::from_fn_with_state`

### Graceful Shutdown
```rust
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```
Handle both SIGINT (Ctrl+C) and SIGTERM via `tokio::signal`.

### Health Checks
Separate liveness (simple OK) and readiness (check dependencies) endpoints.

### Error Handling
Use `thiserror` for domain errors implementing `IntoResponse`. Map to appropriate HTTP status codes.

---

## Tauri 2.x IPC Patterns and React + Vite Integration

### Command System (v2 IPC)
Commands are `#[tauri::command]` Rust functions invoked from JS via `invoke()` from `@tauri-apps/api/core`.

Key features:
- Args passed as JSON with camelCase keys, auto-deserialized
- Return values must implement `serde::Serialize`
- Async commands supported (can't use borrowed types like `&str`)
- Commands can access `WebviewWindow`, `AppHandle`, `State<T>`
- **Streaming data** via `tauri::ipc::Channel<T>`

### v1 → v2 Changes
- `@tauri-apps/api/tauri` → `@tauri-apps/api/core`
- `tauri > allowlist` replaced by **capabilities and permissions system**
- Capabilities = ACLs for fine-grained command access
- All potentially dangerous commands blocked by default

### Capabilities/Permissions System
```json
{
  "identifier": "main-capability",
  "windows": ["main"],
  "permissions": ["core:default", "sql:allow-execute", "sql:allow-load"]
}
```
Capabilities defined in `src-tauri/capabilities/` directory.

### React + Vite Setup
Standard Vite config with `clearScreen: false`, port 1420, `strictPort: true`, ignoring `src-tauri/` in watcher. Tauri loads dev server URL in webview.

### Type-Safe IPC
[TauRPC](https://github.com/MatsDK/TauRPC) provides type-safe bidirectional IPC with auto-generated TypeScript types from Rust using Specta.

### Hot Reload
`npm run tauri dev` starts Vite dev server + Tauri app. Frontend changes trigger HMR. Rust changes trigger rebuild and restart.

### System Tray (v2)
Enable `tray-icon` feature in Cargo.toml. API available from both Rust (`TrayIconBuilder`) and JS (`TrayIcon` from `@tauri-apps/api/tray`).

### SQLite Integration
**Option 1: tauri-plugin-sql** — Plugin with migration support, JS-side query API, permissions-based.
```rust
.plugin(tauri_plugin_sql::Builder::default()
    .add_migrations("sqlite:mydb.db", migrations)
    .build())
```

**Option 2: rusqlite directly** — More control, exposed via Tauri commands.

---

## Testing Considerations (New Project)

Since this is a new project, testing setup should be established from the start:

### Rust Testing
- Built-in `#[test]` and `#[tokio::test]` for unit tests
- `sqlx::test` macro for database-dependent tests (auto-creates test databases)
- Integration tests in `tests/` directory per crate

### Frontend Testing
- **Vitest** for React component and unit testing (native Vite integration)
- **React Testing Library** for component testing
- **Playwright** or **Cypress** for E2E testing (if needed later)

### Recommended Setup
- Each Rust crate has its own unit tests
- Server crate has integration tests using `sqlx::test`
- Frontend uses Vitest + React Testing Library
- Justfile/Makefile targets for running all tests

---

## Sources
- [Tauri v2 Project Structure](https://v2.tauri.app/start/project-structure/)
- [Tauri v2 Calling Rust](https://v2.tauri.app/develop/calling-rust/)
- [Tauri v2 Capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri v2 System Tray](https://v2.tauri.app/learn/system-tray/)
- [Tauri SQL Plugin](https://v2.tauri.app/plugin/sql/)
- [Tauri Migration Guide v1→v2](https://v2.tauri.app/start/migrate/from-tauri-1/)
- [Axum + SQLx + PostgreSQL Guide](https://www.ruststepbystep.com/integrating-axum-with-postgresql-and-sqlx-a-step-by-step-guide/)
- [SQLx vs Diesel](https://diesel.rs/compare_diesel.html)
- [SQLx GitHub](https://github.com/launchbadge/sqlx)
- [TauRPC](https://github.com/MatsDK/TauRPC)
