No prior sections exist yet. I have all the context I need. Now I will produce the section content.

# Section 03: Server Scaffold

## Overview

This section creates the Axum HTTP server binary crate (`openconv-server`) located at `/Users/nisar/personal/projects/openconv/apps/server/`. It includes configuration loading, application state, a router with middleware, health check endpoints, error handling, and graceful shutdown.

**Dependencies:** This section assumes Section 01 (monorepo setup) and Section 02 (shared crate) are complete. Specifically:
- The workspace root `Cargo.toml` at `/Users/nisar/personal/projects/openconv/Cargo.toml` already declares `apps/server` as a member and has workspace dependencies defined.
- The `openconv-shared` crate at `/Users/nisar/personal/projects/openconv/crates/shared/` exists with typed IDs, API types, `OpenConvError`, and constants.

**Blocks:** Section 04 (PostgreSQL migrations) builds on the server crate by adding migration SQL files.

## Files to Create

| File Path | Purpose |
|-----------|---------|
| `/Users/nisar/personal/projects/openconv/apps/server/Cargo.toml` | Crate manifest for the server binary |
| `/Users/nisar/personal/projects/openconv/apps/server/src/main.rs` | Entry point: env loading, tracing, config, pool, router, serve |
| `/Users/nisar/personal/projects/openconv/apps/server/src/config.rs` | `ServerConfig` struct with TOML + env var loading |
| `/Users/nisar/personal/projects/openconv/apps/server/src/state.rs` | `AppState` struct holding `PgPool` and `Arc<ServerConfig>` |
| `/Users/nisar/personal/projects/openconv/apps/server/src/router.rs` | Axum router builder with middleware stack |
| `/Users/nisar/personal/projects/openconv/apps/server/src/handlers/mod.rs` | Handler module declarations |
| `/Users/nisar/personal/projects/openconv/apps/server/src/handlers/health.rs` | Health check endpoint handlers |
| `/Users/nisar/personal/projects/openconv/apps/server/src/error.rs` | `IntoResponse` impl for `OpenConvError` |
| `/Users/nisar/personal/projects/openconv/apps/server/src/shutdown.rs` | Graceful shutdown signal listener |
| `/Users/nisar/personal/projects/openconv/apps/server/src/lib.rs` | Re-exports for integration testing |
| `/Users/nisar/personal/projects/openconv/apps/server/config.toml` | Default development config file |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/.keep` | Placeholder for SQLx migrations directory |

## Tests First

Write tests before implementation. These tests live in two locations: inline `#[cfg(test)]` modules within each source file, and integration tests at `/Users/nisar/personal/projects/openconv/apps/server/tests/`.

### 4.1 Configuration Tests

File: `/Users/nisar/personal/projects/openconv/apps/server/src/config.rs` (inline `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loads_from_valid_toml_string() {
        // Parse a complete valid TOML string into ServerConfig
        // Assert all fields match expected values
    }

    #[test]
    fn test_config_applies_env_var_overrides() {
        // Set DATABASE_URL env var
        // Parse config from TOML, then apply overrides
        // Assert database_url field matches the env var, not the TOML value
        // Clean up: remove the env var after test
    }

    #[test]
    fn test_config_has_correct_defaults_for_omitted_fields() {
        // Parse a minimal TOML string (only required fields)
        // Assert optional fields have sensible defaults (host = "127.0.0.1", port = 3000, etc.)
    }

    #[test]
    fn test_config_fails_on_malformed_toml() {
        // Attempt to parse malformed TOML
        // Assert error is returned
    }
}
```

### 4.3 Application State Tests

File: `/Users/nisar/personal/projects/openconv/apps/server/src/state.rs` (inline `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_implements_clone() {
        // Verify AppState: Clone bound at compile time
        fn assert_clone<T: Clone>() {}
        assert_clone::<AppState>();
    }
}
```

### 4.4 Router and Health Check Tests

File: `/Users/nisar/personal/projects/openconv/apps/server/tests/health.rs`

These are integration tests that spin up the Axum app using `axum::body::Body` and `tower::ServiceExt` (the `oneshot` pattern) without needing a live server or database for the liveness check.

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`

#[tokio::test]
async fn test_health_live_returns_200_with_status_ok() {
    // Build the router (without database — use a mock or test pool)
    // Send GET /health/live
    // Assert status 200
    // Assert body contains {"status": "ok"}
}

#[tokio::test]
async fn test_health_ready_returns_200_when_db_connected() {
    // Requires a real PgPool (use sqlx::test or a test database)
    // Send GET /health/ready
    // Assert status 200
}

#[tokio::test]
async fn test_health_ready_returns_503_when_db_unreachable() {
    // Build router with a PgPool pointing to an invalid database
    // Send GET /health/ready
    // Assert status 503
}

#[tokio::test]
async fn test_requests_include_x_request_id_header() {
    // Send any request
    // Assert response headers contain "x-request-id"
    // Assert the value is a valid UUID
}

#[tokio::test]
async fn test_cors_headers_present() {
    // Send an OPTIONS preflight request
    // Assert Access-Control-Allow-Origin header is present
}

#[tokio::test]
async fn test_unknown_routes_return_404() {
    // Send GET /nonexistent
    // Assert status 404
}
```

### 4.5 Error Handling Tests

File: `/Users/nisar/personal/projects/openconv/apps/server/src/error.rs` (inline `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn test_not_found_maps_to_404() {
        let response = OpenConvError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_unauthorized_maps_to_401() {
        let response = OpenConvError::Unauthorized.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_forbidden_maps_to_403() {
        let response = OpenConvError::Forbidden.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_validation_maps_to_400() {
        let response = OpenConvError::Validation("bad input".into()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_internal_maps_to_500() {
        let response = OpenConvError::Internal("something broke".into()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_error_responses_are_json_with_error_field() {
        // Convert an OpenConvError to response
        // Read the body as bytes, parse as JSON
        // Assert the JSON object has an "error" key with the expected message string
    }
}
```

## Implementation Details

### Crate Manifest

File: `/Users/nisar/personal/projects/openconv/apps/server/Cargo.toml`

The server crate is a binary crate named `openconv-server`. It depends on the shared crate with the `sqlx` feature enabled.

```toml
[package]
name = "openconv-server"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "openconv-server"
path = "src/main.rs"

[dependencies]
openconv-shared = { path = "../../crates/shared", features = ["sqlx"] }
axum = { workspace = true }
tokio = { workspace = true }
sqlx = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
uuid = { workspace = true }
dotenvy = { workspace = true }
toml = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
# For integration tests using tower::ServiceExt::oneshot
```

All dependency versions come from the workspace-level `[workspace.dependencies]` table (set up in Section 01).

### Configuration (`config.rs`)

The `ServerConfig` struct is deserialized from TOML, then environment variables are applied as overrides.

```rust
/// Server configuration loaded from config.toml with env var overrides.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ServerConfig {
    /// Host to bind to. Default: "127.0.0.1"
    pub host: String,
    /// Port to listen on. Default: 3000
    pub port: u16,
    /// PostgreSQL connection string
    pub database_url: String,
    /// Maximum database pool connections. Default: 5
    pub max_db_connections: u32,
    /// Allowed CORS origins. Default: ["http://localhost:1420"]
    pub cors_origins: Vec<String>,
    /// Tracing log level. Default: "info"
    pub log_level: String,
}
```

Provide a `Default` impl with safe development values. Implement a `load()` method that:

1. Reads `config.toml` from the current working directory (or a path specified by `CONFIG_PATH` env var) using `std::fs::read_to_string` and `toml::from_str`.
2. Applies environment variable overrides: check for `HOST`, `PORT`, `DATABASE_URL`, `MAX_DB_CONNECTIONS`, `LOG_LEVEL`. If the env var is set, overwrite the corresponding field.
3. Returns `Result<ServerConfig, Box<dyn std::error::Error>>`.

The `config.toml` file uses sensible development defaults and is safe to commit. Secrets (like `DATABASE_URL`) should come from `.env` via environment variable override.

### Default `config.toml`

File: `/Users/nisar/personal/projects/openconv/apps/server/config.toml`

```toml
host = "127.0.0.1"
port = 3000
database_url = "postgresql://openconv:openconv@localhost:5432/openconv"
max_db_connections = 5
cors_origins = ["http://localhost:1420"]
log_level = "debug"
```

### Application State (`state.rs`)

```rust
/// Shared application state passed to all handlers via Axum's State extractor.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: std::sync::Arc<ServerConfig>,
}
```

`AppState` derives `Clone`. `PgPool` is internally `Arc`-wrapped already. `ServerConfig` is wrapped in `Arc` so cloning `AppState` is cheap. No manual `Arc<AppState>` wrapping is needed -- Axum handles state sharing via `with_state()`.

### Router and Middleware (`router.rs`)

Build the Axum router with a layered middleware stack.

```rust
/// Builds the application router with all middleware and routes.
pub fn build_router(state: AppState) -> axum::Router {
    // ...
}
```

The middleware stack (applied via `tower::ServiceBuilder` or Axum's `.layer()`) in order:

1. **TraceLayer** (`tower_http::trace::TraceLayer`) -- logs HTTP method, URI, status code, and latency for every request. Uses `tracing` for structured logging output.

2. **CorsLayer** (`tower_http::cors::CorsLayer`) -- configured from `state.config.cors_origins`. Parse each origin string into `HeaderValue` and set via `allow_origins()`. Allow common methods (GET, POST, PUT, DELETE, OPTIONS) and headers (Content-Type, Authorization).

3. **DefaultBodyLimit** (`tower_http::limit::DefaultBodyLimit`) -- set to 2MB (`2 * 1024 * 1024` bytes). Individual routes that need higher limits (e.g., file upload, added in later sections) will override this per-route.

4. **Request ID Middleware** -- a custom middleware using `axum::middleware::from_fn` that:
   - Generates a `uuid::Uuid::new_v4()` for each request
   - Inserts it into response headers as `x-request-id`
   - Optionally adds it to the current tracing span

Routes registered:

```rust
axum::Router::new()
    .route("/health/live", get(handlers::health::liveness))
    .route("/health/ready", get(handlers::health::readiness))
    // middleware layers applied here
    .with_state(state)
```

### Health Check Handlers (`handlers/health.rs`)

Two endpoints:

```rust
/// GET /health/live
/// Returns 200 {"status": "ok"} unconditionally.
/// Used by load balancers to check if the process is alive.
pub async fn liveness() -> impl IntoResponse {
    // Return Json({"status": "ok"}) with 200
}

/// GET /health/ready
/// Queries the database with SELECT 1 to verify connectivity.
/// Returns 200 {"status": "ok"} on success, 503 {"status": "unavailable"} on failure.
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    // Execute `sqlx::query("SELECT 1").execute(&state.db).await`
    // On success: 200 with {"status": "ok"}
    // On failure: 503 with {"status": "unavailable"}
}
```

### Error Handling (`error.rs`)

Implement `axum::response::IntoResponse` for `openconv_shared::error::OpenConvError`. This allows handlers to return `Result<T, OpenConvError>` and Axum will automatically convert errors to HTTP responses.

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use openconv_shared::error::OpenConvError;

impl IntoResponse for OpenConvError {
    fn into_response(self) -> Response {
        // Map variant to (StatusCode, message) tuple
        // NotFound       -> 404
        // Unauthorized   -> 401
        // Forbidden      -> 403
        // Validation(m)  -> 400, message = m
        // Internal(m)    -> 500, message = m
        //
        // Return (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}
```

Note: Since `OpenConvError` is defined in the shared crate and `IntoResponse` is from axum, the orphan rule means this impl must go in the server crate (which owns neither trait nor type). To work around this, create a newtype wrapper `ServerError(OpenConvError)` that implements `IntoResponse`, or implement it via a trait extension. The simplest approach is a newtype:

```rust
pub struct ServerError(pub OpenConvError);

impl IntoResponse for ServerError { ... }

impl From<OpenConvError> for ServerError {
    fn from(e: OpenConvError) -> Self { ServerError(e) }
}
```

Then handler return types use `Result<impl IntoResponse, ServerError>`.

### Graceful Shutdown (`shutdown.rs`)

```rust
/// Returns a future that resolves when a shutdown signal is received.
/// Listens for Ctrl+C on all platforms. On Unix, also listens for SIGTERM.
pub async fn shutdown_signal() {
    // tokio::signal::ctrl_c() as the base
    // On Unix: also tokio::signal::unix::signal(SignalKind::terminate())
    // Use tokio::select! to resolve on whichever comes first
    // Log a tracing::info! message when signal received
}
```

Use `#[cfg(unix)]` for the SIGTERM branch. On non-Unix platforms, only Ctrl+C is used.

### Entry Point (`main.rs`)

The `main` function orchestrates startup in this order:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load .env file (dotenvy::dotenv().ok() -- ok() ignores missing .env)
    // 2. Initialize tracing subscriber with EnvFilter
    // 3. Load ServerConfig
    // 4. Create PgPool with max_connections from config
    // 5. Run SQLx migrations: sqlx::migrate!().run(&pool).await
    // 6. Build AppState { db: pool, config: Arc::new(config) }
    // 7. Build router via router::build_router(state)
    // 8. Bind TcpListener on host:port
    // 9. Log "Server listening on {host}:{port}"
    // 10. axum::serve(listener, app).with_graceful_shutdown(shutdown::shutdown_signal()).await
}
```

Step 5 (`sqlx::migrate!()`) will produce a compile error until at least one migration file exists in `apps/server/migrations/`. To allow this section to compile independently, create an empty `migrations/` directory with a `.keep` file. The `sqlx::migrate!()` macro will find zero migrations and succeed (it is a no-op when there are no migrations). Section 04 adds the actual migration SQL files.

### Library Re-exports (`lib.rs`)

File: `/Users/nisar/personal/projects/openconv/apps/server/src/lib.rs`

Re-export internal modules so integration tests can import them:

```rust
pub mod config;
pub mod error;
pub mod handlers;
pub mod router;
pub mod shutdown;
pub mod state;
```

This is necessary because integration tests (in `tests/`) can only access items through the crate's public API (`use openconv_server::router::build_router`).

## Implementation Notes

- **SQLx offline mode:** The server uses `sqlx::migrate!()` which embeds migrations at compile time. If building without a live database, set `SQLX_OFFLINE=true` and ensure `.sqlx/` directory is populated (via `just sqlx-prepare`). For this section, since there are no SQL queries (only migrations), offline mode is not yet a concern.

- **No handlers beyond health checks.** All other endpoints (auth, guilds, channels, messages, etc.) are added in later feature splits. This section establishes the server skeleton.

- **The `migrations/` directory** must exist for `sqlx::migrate!()` to compile. Create it with a `.keep` file. Section 04 will populate it with actual migration files.

- **Test database for integration tests:** The health check integration tests that need a real database should use the `#[sqlx::test]` attribute, which automatically creates a temporary database, runs migrations, and cleans up after. For tests that do not need the database (liveness check, request ID header, unknown routes), build a router with a test `AppState` using a pool from a test database or skip the readiness-dependent tests until Section 04 is complete.

## Code Review Changes

- Added `migrate` feature to workspace `sqlx` dependency (required for `sqlx::migrate!()` macro)
- Added `v4` feature to workspace `uuid` dependency (required for `Uuid::new_v4()` in request ID middleware)
- Changed `DefaultBodyLimit` import from `tower_http::limit` to `axum::extract` (API change in axum 0.8)
- Made `apply_env_overrides` return `Result` — invalid env var values (e.g., `PORT=abc`) now fail loudly instead of being silently ignored
- Added `#[ignore]` test stub for `test_health_ready_returns_200_when_db_connected` (requires live DB, deferred to section 04)
- Added request ID recording to tracing span in the request ID middleware
- Removed unused `chrono` dependency from server crate
- Removed duplicate `[dev-dependencies]` entries (`serde_json`, `uuid` already in `[dependencies]`)

## Test Summary

- **Unit tests (11):** config (4), error (6), state (1)
- **Integration tests (6):** health live (1), health ready 503 (1), health ready 200 (1, ignored), request ID (1), CORS (1), 404 (1)