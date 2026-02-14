Section 08 has not been written yet, but I have the full plan and TDD file. I now have all the context needed. Here is the section content:

# Section 09: Testing Infrastructure and Initial Tests

## Overview

This section establishes the complete testing infrastructure for the OpenConv project and verifies that all tests across all crates pass together. It is the final section in the foundation split -- it depends on all prior sections (01 through 08) being complete. Rather than creating new application code, this section focuses on:

1. Verifying that all Rust tests across all three crates pass via `cargo test --workspace`
2. Verifying that frontend tests pass via `npm test` in `apps/desktop`
3. Ensuring the `just test` target works end-to-end
4. Adding any missing test infrastructure, test helpers, or cross-crate integration tests not covered by individual sections
5. Validating build verification criteria for the entire foundation

This section does **not** duplicate tests already specified in sections 02-08. Those sections define their own tests inline. This section adds the **glue** -- meta-tests, integration test infrastructure, and verification that everything works as a unified whole.

## Dependencies

| Section | What It Must Provide |
|---------|---------------------|
| section-01-monorepo-setup | Cargo workspace with three members, npm workspace |
| section-02-shared-crate | `openconv-shared` crate with ID types, API types, error types, constants, and inline unit tests |
| section-03-server-scaffold | `openconv-server` crate with config, router, health endpoints, error handling, and inline/integration tests |
| section-04-postgres-migrations | 9 SQL migration files and `tests/migrations.rs` integration tests in the server crate |
| section-05-tauri-scaffold | `openconv-desktop` crate with db module, health command, and inline unit tests |
| section-06-sqlite-migrations | Migration runner and all client tables in `db.rs`, with inline tests |
| section-07-react-frontend | React app with Vitest config, setup file, and App component smoke test |
| section-08-dev-tooling | `justfile` with `test` target, `docker-compose.yml`, `config.toml` |

## Files to Create/Modify

| File Path | Action | Purpose |
|-----------|--------|---------|
| `/Users/nisar/personal/projects/openconv/apps/server/tests/health.rs` | Verify/Complete | Integration tests for health endpoints (may already be stubbed in section 03) |
| `/Users/nisar/personal/projects/openconv/apps/server/tests/migrations.rs` | Verify/Complete | Migration integration tests (created in section 04) |

No new source files are created by this section. The work is ensuring all existing tests compile and pass, and filling in any test stubs left as `todo!()` by prior sections.

---

## Tests

### Part 1: Shared Crate Tests (Verification)

These tests were defined in section 02 and live in inline `#[cfg(test)]` modules within the shared crate. This section verifies they are fully implemented (not `todo!()`). The tests are enumerated here for completeness.

**File:** `/Users/nisar/personal/projects/openconv/crates/shared/src/ids.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // Test: UserId::new() generates a valid UUID v7
    #[test]
    fn user_id_new_creates_valid_uuid() {
        // Call UserId::new(), assert inner UUID is not nil
    }

    // Test: UserId serializes to a UUID string via serde_json
    #[test]
    fn user_id_serializes_to_uuid_string() {
        // serde_json::to_string(&id) should produce a quoted UUID string
    }

    // Test: UserId deserializes from a UUID string via serde_json
    #[test]
    fn user_id_deserializes_from_uuid_string() {
        // serde_json::from_str with a valid UUID string should succeed
    }

    // Test: UserId roundtrip: serialize then deserialize produces the same value
    #[test]
    fn user_id_roundtrip_serde() {
        // Serialize then deserialize, assert equality
    }

    // Test: UserId Display formats as UUID string
    #[test]
    fn user_id_display_formats_as_uuid() {
        // format!("{}", id) should produce a valid UUID string
    }

    // Test: UserId FromStr parses a valid UUID string
    #[test]
    fn user_id_from_str_valid() {
        // UserId::from_str("valid-uuid-string") should succeed
    }

    // Test: UserId FromStr rejects an invalid string
    #[test]
    fn user_id_from_str_invalid() {
        // UserId::from_str("not-a-uuid") should return Err
    }

    // Test: Two calls to UserId::new() produce different IDs
    #[test]
    fn user_id_new_produces_unique_ids() {
        // Assert UserId::new() != UserId::new()
    }

    // Test: UserId::new() produces time-sortable IDs
    #[test]
    fn user_id_new_is_time_sortable() {
        // Create id1, then id2; assert id2 > id1 lexicographically
    }

    // Representative tests for other ID types via define_id! macro
    #[test]
    fn guild_id_roundtrip_serde() { /* ... */ }

    #[test]
    fn channel_id_roundtrip_serde() { /* ... */ }

    #[test]
    fn message_id_roundtrip_serde() { /* ... */ }

    #[test]
    fn role_id_roundtrip_serde() { /* ... */ }

    #[test]
    fn file_id_roundtrip_serde() { /* ... */ }

    #[test]
    fn dm_channel_id_roundtrip_serde() { /* ... */ }
}
```

**File:** `/Users/nisar/personal/projects/openconv/crates/shared/src/api/mod.rs` (or distributed across API submodules)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: RegisterRequest serializes to JSON with expected field names
    #[test]
    fn register_request_serializes() { /* ... */ }

    // Test: RegisterRequest deserializes from JSON
    #[test]
    fn register_request_deserializes() { /* ... */ }

    // Test: GuildResponse roundtrip serialization
    #[test]
    fn guild_response_roundtrip() { /* ... */ }

    // Test: MessageResponse includes all fields in JSON output
    #[test]
    fn message_response_includes_all_fields() { /* ... */ }

    // Test: CreateGuildRequest with minimal fields serializes correctly
    #[test]
    fn create_guild_request_minimal() { /* ... */ }

    // Test: ChannelResponse deserializes channel_type as expected type
    #[test]
    fn channel_response_channel_type() { /* ... */ }
}
```

**File:** `/Users/nisar/personal/projects/openconv/crates/shared/src/error.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: OpenConvError::NotFound displays correctly
    #[test]
    fn not_found_display() {
        // assert_eq!(OpenConvError::NotFound.to_string(), "not found")
    }

    // Test: OpenConvError::Validation contains the message
    #[test]
    fn validation_contains_message() {
        // assert that .to_string() includes the provided message
    }

    // Test: OpenConvError::Internal contains the message
    #[test]
    fn internal_contains_message() {
        // assert that .to_string() includes the provided message
    }

    // Test: All error variants implement std::error::Error
    #[test]
    fn all_variants_impl_error() {
        // Compile-time check: attempt to use each variant as &dyn std::error::Error
        fn assert_error<T: std::error::Error>() {}
        assert_error::<OpenConvError>();
    }
}
```

**File:** `/Users/nisar/personal/projects/openconv/crates/shared/src/constants.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: MAX_FILE_SIZE_BYTES equals 25 * 1024 * 1024
    #[test]
    fn max_file_size_is_25mb() {
        assert_eq!(MAX_FILE_SIZE_BYTES, 25 * 1024 * 1024);
    }

    // Test: All length constants are > 0
    #[test]
    fn all_length_constants_positive() {
        assert!(MAX_DISPLAY_NAME_LENGTH > 0);
        assert!(MAX_CHANNEL_NAME_LENGTH > 0);
        assert!(MAX_GUILD_NAME_LENGTH > 0);
        assert!(MAX_MESSAGE_SIZE_BYTES > 0);
    }
}
```

### Part 2: Server Scaffold Tests (Verification)

These tests were defined in section 03. Verify they are fully implemented.

**File:** `/Users/nisar/personal/projects/openconv/apps/server/src/config.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loads_from_valid_toml_string() { /* ... */ }

    #[test]
    fn test_config_applies_env_var_overrides() { /* ... */ }

    #[test]
    fn test_config_has_correct_defaults_for_omitted_fields() { /* ... */ }

    #[test]
    fn test_config_fails_on_malformed_toml() { /* ... */ }
}
```

**File:** `/Users/nisar/personal/projects/openconv/apps/server/src/state.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_implements_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<AppState>();
    }
}
```

**File:** `/Users/nisar/personal/projects/openconv/apps/server/src/error.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn test_not_found_maps_to_404() {
        // OpenConvError::NotFound (wrapped in ServerError) -> 404
    }

    #[test]
    fn test_unauthorized_maps_to_401() { /* ... */ }

    #[test]
    fn test_forbidden_maps_to_403() { /* ... */ }

    #[test]
    fn test_validation_maps_to_400() { /* ... */ }

    #[test]
    fn test_internal_maps_to_500() { /* ... */ }

    #[tokio::test]
    async fn test_error_responses_are_json_with_error_field() {
        // Convert a ServerError to response, read body bytes, parse as JSON
        // Assert the JSON has an "error" key
    }
}
```

### Part 3: Server Integration Tests (Health Endpoints)

These tests were defined in section 03 and live in the integration test file. They test the router as a whole using `tower::ServiceExt::oneshot`.

**File:** `/Users/nisar/personal/projects/openconv/apps/server/tests/health.rs`

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for oneshot

// NOTE: Tests that do not require a database create a router with a test AppState.
// For tests that need a real PgPool, use #[sqlx::test] which auto-creates a temp database.

#[tokio::test]
async fn test_health_live_returns_200_with_status_ok() {
    // Build the router with a test AppState (requires a valid PgPool)
    // Send GET /health/live via oneshot
    // Assert status 200
    // Parse body as JSON, assert {"status": "ok"}
}

#[sqlx::test]
async fn test_health_ready_returns_200_when_db_connected(pool: sqlx::PgPool) {
    // Build router with real pool from sqlx::test
    // Send GET /health/ready
    // Assert status 200
}

#[tokio::test]
async fn test_health_ready_returns_503_when_db_unreachable() {
    // Build router with a PgPool pointing to an invalid/closed database
    // Send GET /health/ready
    // Assert status 503
}

#[tokio::test]
async fn test_requests_include_x_request_id_header() {
    // Send GET /health/live
    // Assert response has "x-request-id" header
    // Assert the header value parses as a valid UUID
}

#[tokio::test]
async fn test_cors_headers_present() {
    // Send an OPTIONS request with Origin header
    // Assert Access-Control-Allow-Origin is present in response
}

#[tokio::test]
async fn test_unknown_routes_return_404() {
    // Send GET /nonexistent
    // Assert status 404
}
```

**Important implementation note for health integration tests:** Tests that need a PgPool but are not marked `#[sqlx::test]` need to construct a pool manually. This can be done by reading `DATABASE_URL` from the environment and connecting. However, the preferred approach is to use `#[sqlx::test]` wherever a pool is needed. For tests like the liveness check that do not exercise the database, a real pool is still required to construct `AppState`. The recommended pattern is:

```rust
// Helper function used across health integration tests
async fn build_test_app(pool: sqlx::PgPool) -> axum::Router {
    let config = openconv_server::config::ServerConfig::default();
    let state = openconv_server::state::AppState {
        db: pool,
        config: std::sync::Arc::new(config),
    };
    openconv_server::router::build_router(state)
}
```

For the 503 test (unreachable database), create a pool with an invalid connection string. `sqlx::PgPoolOptions::new().max_connections(1).connect("postgres://invalid:invalid@localhost:1/nonexistent")` will produce a pool that fails on queries.

### Part 4: PostgreSQL Migration Tests (Verification)

These tests were defined in section 04. They live at the path below and use `#[sqlx::test]` to automatically run all migrations on a temporary test database before each test.

**File:** `/Users/nisar/personal/projects/openconv/apps/server/tests/migrations.rs`

```rust
use sqlx::PgPool;

#[sqlx::test]
async fn all_migrations_apply_successfully(pool: PgPool) {
    // If we reach here, all migrations ran. Verify with SELECT 1.
}

#[sqlx::test]
async fn users_table_insert_and_select(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn users_table_unique_public_key(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn users_table_unique_email(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn users_updated_at_trigger(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn guilds_fk_owner_id_rejects_nonexistent_user(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn guilds_updated_at_trigger(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn channels_cascade_delete_on_guild_delete(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn channels_unique_guild_id_name(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn messages_check_rejects_both_null(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn messages_check_rejects_both_set(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn messages_accepts_channel_id_only(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn messages_accepts_dm_channel_id_only(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn guild_members_no_duplicate_membership(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn dm_channel_members_no_duplicate_membership(pool: PgPool) { /* ... */ }

#[sqlx::test]
async fn files_allows_null_message_id(pool: PgPool) { /* ... */ }
```

### Part 5: Desktop (Tauri) Tests (Verification)

These tests were defined in sections 05 and 06. They live inline in the desktop crate.

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/commands/health.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_returns_app_health() {
        // Construct an AppHealth or call the health_check function with a test db
    }

    #[test]
    fn test_health_check_includes_version() {
        // Assert version string is non-empty
    }
}
```

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/db.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_init_db_in_memory() { /* ... */ }

    #[test]
    fn test_init_db_connection_is_functional() { /* ... */ }

    #[test]
    fn test_run_migrations_creates_migrations_table() { /* ... */ }

    #[test]
    fn test_run_migrations_creates_all_tables() { /* ... */ }

    #[test]
    fn test_run_migrations_idempotent() { /* ... */ }

    #[test]
    fn test_migrations_table_records_versions() { /* ... */ }

    #[test]
    fn test_local_user_table_columns() { /* ... */ }

    #[test]
    fn test_cached_users_table_columns() { /* ... */ }

    #[test]
    fn test_cached_guilds_table_columns() { /* ... */ }

    #[test]
    fn test_cached_channels_table_columns() { /* ... */ }

    #[test]
    fn test_cached_messages_table_columns_and_index() { /* ... */ }

    #[test]
    fn test_cached_files_table_columns() { /* ... */ }

    #[test]
    fn test_sync_state_primary_key() { /* ... */ }
}
```

### Part 6: Frontend Tests (Verification)

These tests were defined in section 07 and use Vitest with React Testing Library.

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src/__tests__/App.test.tsx`

```typescript
import { render, screen } from "@testing-library/react";
import App from "../App";

// Test: App component renders without crashing (smoke test)
test("App renders without crashing", () => {
  // render(<App />);
  // If no error is thrown, the test passes
});

// Test: App component renders the OpenConv title text
test("App renders OpenConv title", () => {
  // render(<App />);
  // expect(screen.getByText(/OpenConv/i)).toBeInTheDocument();
});

// Test: App component mounts in dark mode
test("App has dark mode class", () => {
  // render(<App />);
  // Check that the root element or document has 'dark' class
});

// Test: App component displays a status indicator element
test("App displays status indicator", () => {
  // render(<App />);
  // expect(screen.getByTestId("status-indicator")).toBeInTheDocument();
});
```

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src/__tests__/setup.ts`

The test setup file must:
1. Import `@testing-library/jest-dom/vitest` for DOM matchers
2. Mock the Tauri IPC layer (`@tauri-apps/api`) to prevent real IPC calls during testing
3. Mock the TauRPC bindings import (`../bindings`) to provide a stub health check function

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/vitest.config.ts`

```typescript
/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/__tests__/setup.ts"],
  },
});
```

### Part 7: Meta-Tests and Cross-Crate Verification

These are not traditional unit tests but rather verification steps that confirm the entire testing infrastructure works as a unified system.

**Meta-test 1: `cargo test --workspace` discovers and runs tests from all three crates**

Run `cargo test --workspace` from the repository root. Verify the output shows test results from:
- `openconv-shared` (ID tests, API type tests, error tests, constants tests)
- `openconv-server` (config tests, state tests, error mapping tests, health integration tests, migration integration tests)
- `openconv-desktop` (db init tests, migration tests, health command tests)

All tests must pass. If any test is still a `todo!()`, this section's job is to implement it.

**Meta-test 2: `npm test` in `apps/desktop` runs Vitest and passes**

Run `cd apps/desktop && npm test` (which executes `vitest run`). Verify the output shows:
- The App smoke tests passing
- Exit code 0

**Meta-test 3: `just test` runs both Rust and JS tests successfully**

The `justfile` `test` target (defined in section 08) should execute both test suites:

```just
test:
    cargo test --workspace
    cd apps/desktop && npm test
```

Run `just test` from the repository root. Verify both suites pass with exit code 0.

---

## Implementation Guide

### Step 1: Verify all test stubs are implemented

Go through every test file across all three crates and the frontend. Replace any remaining `todo!()` or empty test bodies with working implementations. Each test should:

- Construct the necessary test data
- Call the function or construct the type under test
- Assert the expected outcome

The test stubs from sections 02-07 provide the intent (what to test). This section fills in any that were left as stubs.

### Step 2: Resolve compilation issues across the workspace

Run `cargo build --workspace` and fix any compilation errors. Common issues at this stage:

- **Missing imports in integration test files.** Integration tests (in `tests/`) are separate compilation units. They need explicit `use` statements for types from the crate being tested. For example, `tests/health.rs` needs `use openconv_server::router::build_router;` which requires that `router` is `pub` in the server crate's `lib.rs`.

- **SQLx offline data.** If building without a live database, ensure `.sqlx/` is populated by running `just sqlx-prepare` with the database running. The `.sqlx/` directory must be committed to source control.

- **Feature flag mismatches.** The shared crate must compile both with and without the `sqlx` feature. Running `cargo build --workspace` exercises both paths since the server enables `sqlx` and the desktop does not.

### Step 3: Run all Rust tests

```bash
cargo test --workspace
```

This discovers and runs all `#[test]` and `#[tokio::test]` functions across all crates. The `#[sqlx::test]` tests in the server crate require a running PostgreSQL instance. Ensure:

1. Docker Compose PostgreSQL is running: `just db-up`
2. `DATABASE_URL` is set (via `.env` file): `DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv`
3. The `.env` file is loaded by `dotenvy` or exported in the shell

If any tests fail, fix the underlying code in the appropriate section's files. Do not modify test expectations to make tests pass -- fix the implementation.

### Step 4: Run frontend tests

```bash
cd /Users/nisar/personal/projects/openconv/apps/desktop && npm test
```

This runs `vitest run`. Ensure:
1. `npm install` has been run in the workspace root (installs all dependencies)
2. The Vitest config at `apps/desktop/vitest.config.ts` is correct
3. The test setup file at `apps/desktop/src/__tests__/setup.ts` properly mocks Tauri IPC

### Step 5: Run `just test`

```bash
just test
```

This executes both Rust and frontend test suites. Verify it exits with code 0.

### Step 6: Verify build with zero warnings

```bash
cargo build --workspace 2>&1 | grep -c "warning"
cargo clippy --workspace -- -D warnings
```

The foundation is clean when clippy produces zero warnings. Fix any clippy lints in the appropriate source files.

---

## Test Environment Prerequisites

For the full test suite to pass, the following must be available:

| Prerequisite | How to Set Up |
|-------------|---------------|
| PostgreSQL 15 running locally | `just db-up` (uses Docker Compose from section 08) |
| `DATABASE_URL` environment variable | Set in `.env` file: `DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv` |
| Node.js and npm | Required for frontend tests; must be installed on the host |
| `npm install` completed | Run `npm install` from the workspace root |
| Rust toolchain with clippy | Standard `rustup` installation |
| Docker | Required for `just db-up` to start PostgreSQL |

---

## Build Verification Criteria

The foundation is complete when ALL of the following pass:

1. `cargo build --workspace` compiles all three crates with zero errors and zero warnings
2. `cargo clippy --workspace -- -D warnings` produces no warnings
3. `just db-up && just db-migrate` creates all PostgreSQL tables
4. `just server` starts Axum; `curl http://localhost:3000/health/live` returns `{"status":"ok"}` with HTTP 200
5. `just dev` launches the Tauri window with the dark-themed placeholder UI
6. The Tauri app calls the health check IPC command and the status indicator shows connected
7. `cargo test --workspace` passes all Rust tests (shared, server, desktop)
8. `cd apps/desktop && npm test` passes all frontend tests
9. `just test` passes both Rust and JavaScript test suites
10. The SQLite database is created in the Tauri app data directory with all 7 tables plus the `_migrations` table
11. TauRPC TypeScript bindings at `apps/desktop/src/bindings.ts` compile without errors
12. `just lint` passes with no warnings
13. `cargo fmt --all --check` passes (all Rust code is formatted)

Items 3-6 and 10-11 are manual verification steps (not automated tests). Items 1-2, 7-9, 12-13 are automated and should all pass from a single `just test && just lint && cargo fmt --all --check` invocation.