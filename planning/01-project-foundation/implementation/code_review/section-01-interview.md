# Code Review Interview: Section 01 - Monorepo Setup

**Date:** 2026-02-14

## Interview Items

### tauri.conf.json added out of scope
**Decision:** Keep minimal stub. Section 05 will overwrite with full config. The plan has an internal inconsistency (requires `cargo build --workspace` to pass but doesn't provide `tauri.conf.json` which Tauri v2 mandates). Minimal stub resolves this.

## Auto-Fixes

### 1. Add .gitkeep files for empty directories
The plan requires `apps/server/migrations/`, `apps/desktop/src-tauri/capabilities/`, and `apps/desktop/src/` to exist. Adding `.gitkeep` placeholders so Git tracks them.

### 2. Fix $schema URL in tauri.conf.json
The `$schema` referenced a NiceGUI project URL (copy-paste artifact). Fixing to omit the schema entirely since this is a minimal stub that Section 05 will replace.

### 3. Add index.html placeholder in apps/desktop/src/
The `frontendDist` in tauri.conf.json points to `../src` which needs at least an `index.html` to avoid potential build issues.

## Let Go

- No thiserror in desktop crate (plan-consistent, deferred to Section 05)
- main.rs + lib.rs without [lib] in Cargo.toml (standard Tauri pattern, plan-consistent)
