# Code Review Interview: Section 03 - Server Scaffold

**Date:** 2026-02-14

## Auto-fixes (no discussion needed)

### 1. Add `migrate` feature to workspace sqlx dependency
**Finding:** CRITICAL - `sqlx::migrate!()` in main.rs requires the `migrate` feature which is missing from the workspace `Cargo.toml`.
**Action:** FIX - Add `"migrate"` to sqlx features in workspace Cargo.toml.

### 2. Remove duplicate dev-dependencies
**Finding:** HIGH - `serde_json` and `uuid` are listed in both `[dependencies]` and `[dev-dependencies]`.
**Action:** FIX - Remove the `[dev-dependencies]` section entries since they're already in `[dependencies]`.

### 3. Add ignored test stub for `test_health_ready_returns_200_when_db_connected`
**Finding:** HIGH - Missing planned test. Requires a live database (available after section 04).
**Action:** FIX - Add `#[ignore]` test stub with a TODO note.

### 4. Add request ID to tracing span
**Finding:** MEDIUM - Plan says to optionally add request ID to tracing span. Implementation doesn't.
**Action:** FIX - Add `tracing::Span::current().record("request_id", ...)` in the middleware.

### 5. Remove unused `chrono` dependency
**Finding:** LOW - `chrono` is in `[dependencies]` but not used in any server source file.
**Action:** FIX - Remove `chrono` from server Cargo.toml.

## User Decisions

### 6. Fail on invalid env var parse (PORT, MAX_DB_CONNECTIONS)
**Finding:** MEDIUM - `apply_env_overrides` silently ignores invalid env var values.
**Question:** Should invalid env var values warn or fail?
**User Decision:** Fail with error. Make `apply_env_overrides` return `Result` and propagate parse errors.
**Action:** FIX - Change `apply_env_overrides` to return `Result<(), Box<dyn std::error::Error>>`.

## Let Go (no changes)

- #4 (inconsistent error messages) - matches the plan's specification
- #7 (env var test thread safety) - known Rust limitation, not worth adding serial_test dep now
- #8 (.keep vs .gitkeep) - created in section 01, functionally identical
- #10 (no explicit [lib] section) - Cargo auto-detects, not needed
