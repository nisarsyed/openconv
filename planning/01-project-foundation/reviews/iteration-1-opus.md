# Opus Review

**Model:** claude-opus-4
**Generated:** 2026-02-14T00:00:00Z

---

# Plan Review: OpenConv Project Foundation

## Overall Assessment

This is a well-structured foundation plan with clear scope and reasonable technology choices. The monorepo layout, shared crate design, and dev tooling are all sensible. However, there are several footguns, missing considerations, and ambiguities that could cause problems during implementation or in later splits.

---

## 1. Dependency Version Inconsistencies

**Section 2.1 (Cargo Workspace) and the research document**

The research document lists `thiserror = "1"`, but the claude-spec.md specifies `thiserror = "2"`. The plan itself does not pin the `thiserror` version. `thiserror` 2.x has breaking changes from 1.x (different derive syntax, `#[error(transparent)]` behavior changes). Since the project is new, `thiserror = "2"` is the better choice, but the research doc is stale.

Similarly, no versions are specified for `axum`, `tokio`, `sqlx`, `tower`, `tower-http`, `tauri`, or `rusqlite` in the plan itself.

**Recommendation:** Add a concrete dependency version table to the plan, or at minimum reference the spec's version list and resolve the `thiserror` conflict.

---

## 2. DM Messages Design -- Incomplete and Risky

**Section 5 (PostgreSQL Migrations), Migration 9**

The plan proposes adding a nullable `dm_channel_id` to the messages table and using a CHECK constraint. Problems:

1. **Migration 7 already creates the messages table** with `channel_id` as a non-nullable FK. Migration 9 would need to ALTER to make `channel_id` nullable and add `dm_channel_id`. This ordering is not stated.

2. **Foreign key integrity is unresolvable.** PostgreSQL CHECK constraints cannot enforce cross-table referential integrity.

3. **Query complexity.** Indexes on `(channel_id, created_at)` won't cover DM queries; you also need `(dm_channel_id, created_at)`.

**Recommendation:** Either rewrite Migration 7 to make `channel_id` nullable from the start (with the CHECK constraint), or defer DM message storage design to a later split. Also add the missing index on `(dm_channel_id, created_at)`.

---

## 3. SQLite Client Schema -- Security Concern

**Section 8 (SQLite Client Migrations)**

The client SQLite schema stores `cached_messages` with a `content` column as "decrypted plaintext." For a privacy-focused application, this is a significant concern.

The plan does not mention:
- Whether the SQLite database file itself will be encrypted (e.g., SQLCipher)
- Whether the app data directory has any OS-level protection
- What happens if someone copies the `.db` file

**Recommendation:** Add a note about SQLite encryption strategy. This affects the `rusqlite` dependency choice since SQLCipher requires a different build configuration.

---

## 4. FTS5 Virtual Table -- Premature and Fragile

**Section 8**

The plan creates an FTS5 virtual table linked to `cached_messages` via `content=cached_messages, content_rowid=rowid`. Problems:

1. **Content-sync FTS tables require triggers** to stay in sync. No triggers are mentioned.

2. **rowid assumption.** The `cached_messages` table uses `id TEXT PK`, which means it is not a rowid-alias table. The FTS5 `content_rowid` parameter needs an INTEGER column.

3. **Creating an empty structure that requires complex supporting infrastructure is premature.**

**Recommendation:** Remove the FTS5 table from the foundation split. Create it in split 06 when the message caching infrastructure is actually built.

---

## 5. TauRPC/Specta -- Maturity and Version Risk

**Section 6.4 (TauRPC Setup)**

TauRPC is a relatively small community project with limited maintenance activity. The plan does not address:
- Specific versions of `tauri-specta` and `specta`
- Whether Tauri 2.x is fully supported
- Fallback strategy if TauRPC breaks or becomes unmaintained

**Recommendation:** Verify compatibility with Tauri 2.x. Consider whether `ts-rs` for Rust-to-TS generation would be more sustainable. At minimum, add a note about the risk and a fallback plan.

---

## 6. Missing `openconv-shared` Dependency on `sqlx` Types

**Section 3 (Shared Crate)**

The shared crate defines typed IDs. However, the plan does not address how these types integrate with SQLx's `FromRow` and `Type` derives needed for database queries on the server side.

**Recommendation:** Add a subsection specifying the strategy. Feature flags (`sqlx` feature in the shared crate) are the cleanest approach.

---

## 7. `config.toml` in Source Control vs. Generated

**Section 4.1 (Configuration)**

No `config.toml.example` or equivalent exists. An implementer cloning the repo will not know what `config.toml` should contain.

**Recommendation:** Either add `config.toml.example` or make `config.toml` checked-in with safe defaults and document that all secrets come from env vars.

---

## 8. Graceful Shutdown -- Windows Signal Handling

**Section 4.6**

The plan says `shutdown_signal()` handles SIGINT or SIGTERM "(Unix)." Windows does not have SIGTERM.

**Recommendation:** Use `tokio::signal::ctrl_c()` as the cross-platform base, and conditionally add Unix signal handling via `#[cfg(unix)]`.

---

## 9. Missing Sessions/Auth Token Table

**Section 5, Migration 1**

No column for storing session tokens or refresh tokens. The auth split will need to add token/session storage.

**Recommendation:** Either add a `sessions` table to the foundation schema or explicitly note that it is deferred to split 03.

---

## 10. Docker Compose -- Hardcoded Credentials

**Section 9.2**

The Docker Compose config uses `POSTGRES_PASSWORD=openconv`. Credentials and `.env.example` `DATABASE_URL` must match.

**Recommendation:** Add a comment noting these are dev-only credentials and that changing them requires updating `.env.example`.

---

## 11. Missing Migration Rollback Strategy

**Section 5**

No down migrations mentioned. Without them, `just db-reset` is the only recovery path.

**Recommendation:** State whether migrations are reversible or irreversible. Document the recovery path.

---

## 12. `Arc<AppState>` -- Double Arc

**Section 4.3**

Axum's `State` extractor already handles sharing. Wrapping in `Arc<AppState>` yourself means double-Arc.

**Recommendation:** `AppState` should derive `Clone` and be passed directly, since `PgPool` is already `Arc` internally.

---

## 13. `.sqlx/` Gitignore Contradicts Offline CI

**Section 2.3 and 9.1**

The `.gitignore` excludes `.sqlx/`, but the spec says "SQLx offline mode for CI (no DB required for cargo build)," which requires committing `.sqlx/`.

**Recommendation:** Remove `.sqlx/` from `.gitignore`. Add a note that `.sqlx/` should be committed and regenerated via `just sqlx-prepare`.

---

## 14. React Version Ambiguity

**Section 7.4**

"React 18+" is ambiguous. React 19 has breaking changes.

**Recommendation:** Pin to a specific major version and verify Tauri + TauRPC compatibility.

---

## 15. Missing Cross-Platform Build Considerations

**Section 6.1**

No mention of Windows WebView2, Linux webkit2gtk/libappindicator prerequisites, or justfile Windows compatibility.

**Recommendation:** Add a "Prerequisites" subsection listing platform-specific dependencies.

---

## 16. No `cached_users` Table

**Section 8**

No user cache table beyond `local_user` (which is only the current user). The UI needs sender display names for offline rendering.

**Recommendation:** Add a `cached_users` table or denormalize `sender_display_name` in `cached_messages`.

---

## 17. No Request Body Size Limits

**Section 4.4**

No `DefaultBodyLimit` configuration. File uploads up to 25MB will fail with Axum's default 2MB limit.

**Recommendation:** Add `DefaultBodyLimit` configuration to the middleware stack.

---

## 18. Missing `updated_at` Auto-Update Mechanism

**Section 5**

Multiple tables have `updated_at` but no trigger functions to automatically update them.

**Recommendation:** Add a reusable `set_updated_at()` trigger function and apply to all tables with `updated_at` columns.

---

## Summary of Priority Issues

| Priority | Issue | Section |
|----------|-------|---------|
| **High** | `.sqlx/` gitignore contradicts offline CI requirement | 2.3, 9.1 |
| **High** | FTS5 virtual table will fail due to rowid/TEXT PK mismatch | 8 |
| **High** | DM messages design requires altering Migration 7 retroactively | 5, Mig 9 |
| **High** | Decrypted plaintext stored unencrypted on disk | 8 |
| **Medium** | Double-Arc footgun in AppState | 4.3 |
| **Medium** | Missing SQLx trait integration strategy for shared types | 3 |
| **Medium** | TauRPC maturity and Tauri 2.x compatibility unverified | 6.4 |
| **Medium** | No `cached_users` table for offline display name resolution | 8 |
| **Medium** | No `updated_at` trigger functions | 5 |
| **Medium** | `thiserror` version conflict between research and spec | 2.1 |
| **Low** | Windows signal handling not addressed | 4.6 |
| **Low** | Missing body size limit configuration | 4.4 |
| **Low** | React version ambiguity | 7.4 |
| **Low** | No migration rollback strategy documented | 5 |
| **Low** | Cross-platform build prerequisites not documented | 6.1 |
