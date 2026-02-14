# Code Review: Section 03 - Server Scaffold

Several issues found ranging from a likely build-breaking bug to missing tests and minor correctness concerns.

1. CRITICAL - Missing `migrate` feature on sqlx (likely compile failure):
   The workspace `Cargo.toml` defines sqlx as:
   `sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono"] }`
   The `migrate` feature is not listed. However, `main.rs` calls `sqlx::migrate!().run(&pool).await?;`. The `sqlx::migrate!()` macro requires the `migrate` feature to be enabled on the sqlx crate. Without it, this will fail to compile. This needs to be added to the workspace dependency features list.

2. HIGH - Duplicate dev-dependencies already in regular dependencies:
   In `apps/server/Cargo.toml`, `serde_json` and `uuid` are already listed under `[dependencies]`, but they are redundantly repeated under `[dev-dependencies]`. The `[dev-dependencies]` entries are entirely unnecessary and should be removed.

3. HIGH - Missing integration test `test_health_ready_returns_200_when_db_connected`:
   The plan explicitly requires a test named `test_health_ready_returns_200_when_db_connected` that uses a real PgPool. This test is absent from the integration tests. The test skeleton should at minimum be present as an `#[ignore]`-annotated test stub.

4. MEDIUM - Inconsistent error message for `NotFound` and `Unauthorized` variants in ServerError:
   For `NotFound`, `Unauthorized`, and `Forbidden`, the code uses `self.0.to_string()` to get the message. But for `Validation(msg)` and `Internal(msg)`, it uses `msg.clone()` directly. This means inconsistent message formats: `{"error": "not found"}` vs `{"error": "bad input"}` instead of "validation error: bad input". The plan says Validation -> message = m, so the implementation matches the plan, but note the inconsistency.

5. MEDIUM - `apply_env_overrides` silently swallows parse errors for PORT and MAX_DB_CONNECTIONS:
   If `PORT=not_a_number`, the code silently ignores the invalid value. This is a footgun — the operator sets an env var expecting it to take effect, but a typo causes it to be silently ignored.

6. MEDIUM - `request_id_middleware` does not add the request ID to the tracing span:
   The plan states the request ID middleware should "Optionally adds it to the current tracing span." The implementation generates the UUID and inserts it into response headers but never records it in a tracing span.

7. LOW - Env var test is not thread-safe:
   `test_config_applies_env_var_overrides` calls `std::env::set_var` and `std::env::remove_var`. Environment variable mutation is inherently not thread-safe and can produce flaky results.

8. LOW - Migration directory file naming inconsistency:
   The plan specifies `migrations/.keep` but the actual file is `migrations/.gitkeep`.

9. LOW - `chrono` dependency is listed but not used anywhere in the server crate's source files. Dead weight.

10. LOW - No `[lib]` target in `apps/server/Cargo.toml`:
    The crate has both `src/lib.rs` and `src/main.rs`. Cargo auto-detects this, but being explicit with a `[lib]` section would be clearer.
