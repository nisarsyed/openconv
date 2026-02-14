# Code Review Interview: Section 04 - PostgreSQL Migrations

**Date:** 2026-02-14

## Auto-fixes

### 1. Strengthen error type assertions in constraint tests
**Finding:** Tests use `assert!(result.is_err())` without checking specific PostgreSQL error codes.
**Action:** FIX - Downcast errors to check constraint name/code for unique (23505), FK (23503), and check (23514) violations.

### 2. Remove redundant index on users.email
**Finding:** `idx_users_email` duplicates the UNIQUE constraint's automatic index.
**Action:** FIX - Remove the explicit CREATE INDEX statement.

## User Decisions

### 3. Missing test coverage for 5 areas
**Finding:** No tests for pre_key_bundles, roles, guild_member_roles, sender_id non-cascade, files ON DELETE SET NULL.
**User Decision:** Defer to section 09 (testing section). Keep to plan's test list for now.
**Action:** DEFER

## Let Go

- #2 (pg_sleep fragility) - works in practice with sqlx::test
- #9 (no channel_type CHECK) - plan doesn't specify, can add later
- #10 (NOW() vs clock_timestamp()) - plan specifies NOW()
