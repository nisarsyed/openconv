# Integration Notes: Opus Review Feedback

## Suggestions INTEGRATED

### 1. Dependency Version Table (Issue #1)
**Why:** The thiserror version conflict is real — research says "1", spec says "2". Adding explicit version pins prevents implementer confusion. Using thiserror 2 since this is a new project.

### 2. DM Messages Design Fix (Issue #2)
**Why:** Migration 7 creating `channel_id` as NOT NULL then Migration 9 altering it is bad sequencing. Will restructure: Migration 7 creates messages with `channel_id` nullable from the start, add `dm_channel_id`, and the CHECK constraint. DM tables (Migration 9) create `dm_channels` and `dm_channel_members`. Also adding missing `(dm_channel_id, created_at)` index.

### 3. Remove FTS5 from Foundation (Issue #4)
**Why:** Critical bug — FTS5 `content_rowid` requires an INTEGER column, but `cached_messages` uses `id TEXT PK`. Also needs sync triggers that don't exist yet. Deferring to messaging split where it belongs.

### 4. SQLx Feature Flags for Shared Crate (Issue #6)
**Why:** The implementer will hit this immediately when writing database queries. Feature flags in shared crate (`sqlx` feature) are the cleanest approach.

### 5. config.toml Handling (Issue #7)
**Why:** Checked into source control with safe defaults. All secrets come from env vars. Clarify what goes where.

### 6. Cross-Platform Shutdown Signal (Issue #8)
**Why:** Windows doesn't have SIGTERM. Use `tokio::signal::ctrl_c()` as cross-platform base + conditional Unix signals.

### 7. Fix Double-Arc AppState (Issue #12)
**Why:** PgPool is already Arc internally. AppState should derive Clone and be passed directly.

### 8. Remove .sqlx/ from .gitignore (Issue #13)
**Why:** Directly contradicts the offline CI requirement. .sqlx/ must be committed.

### 9. Add cached_users Table (Issue #16)
**Why:** Without it, offline message rendering can't show sender names. Essential for a chat app.

### 10. Add DefaultBodyLimit (Issue #17)
**Why:** Axum's default 2MB limit will silently block file uploads (up to 25MB). Needs explicit configuration.

### 11. Add updated_at Trigger Function (Issue #18)
**Why:** Without triggers, every UPDATE must manually set updated_at. Error-prone. One trigger function applied to all relevant tables.

### 12. Document Migration Strategy as Irreversible (Issue #11)
**Why:** Conscious decision worth documenting. db-reset is the recovery path.

### 13. Add Docker Compose Dev-Only Note (Issue #10)
**Why:** Trivial to add, prevents credential confusion.

## Suggestions PARTIALLY INTEGRATED

### 14. SQLite Encryption Note (Issue #3)
**Why partially:** Valid concern for privacy-focused app, but encryption is crypto split (02) territory. Adding a forward reference note rather than implementation details.

### 15. TauRPC Version Pins (Issue #5)
**Why partially:** The user explicitly chose TauRPC in the interview. Adding version pins and compatibility notes, but not changing the technology choice.

### 16. React Version (Issue #14)
**Why partially:** Pinning to React 19 since it's well-established. Minor change.

## Suggestions NOT INTEGRATED

### Sessions/Auth Table (Issue #9)
**Why not:** Explicitly deferred to auth split (03) by design. The foundation defines data tables, not auth infrastructure. Split 03 adds its own migrations.

### Cross-Platform Build Prerequisites (Issue #15)
**Why not:** Better suited for a CONTRIBUTING.md or README, not the implementation plan. Platform prerequisites are documentation concerns, not code architecture decisions.
