Now I have all the context I need. Let me generate the section content for `section-01-monorepo-setup`.

# Section 1: Monorepo Setup

## Overview

This section creates the skeleton that all other sections build into. It establishes the Cargo workspace (three Rust crates), the npm workspace (React frontend), Git configuration, and environment template files. No application logic is implemented here -- only project structure and dependency declarations.

**What this section produces:**
- Root `Cargo.toml` with workspace members and shared dependency versions
- Root `package.json` for npm workspace
- `.gitignore` with correct exclusions
- `.env.example` with all required environment variables
- Empty directory structure for all crates and the desktop frontend
- Minimal `Cargo.toml` files for each workspace member (enough to compile)
- Minimal `src/lib.rs` or `src/main.rs` stubs in each crate (enough for `cargo build --workspace` to succeed)

**Dependencies:** None. This is the first section with no prerequisites.

**Blocked by this section:** All other sections (02 through 08) depend on this skeleton existing.

---

## Tests

There are no direct unit tests for this section. Monorepo setup is workspace configuration verified by build tooling:

- `cargo build --workspace` compiles successfully and discovers all three crates
- `cargo test --workspace` discovers test modules from all three crates (even if no tests exist yet)
- `npm install` at the repository root resolves the workspace correctly

These are verification commands to run after completing the section, not test code to write.

---

## Implementation

### Step 1: Create the Directory Structure

Create the following directories from the repository root (`/Users/nisar/personal/projects/openconv/`):

```
openconv/
├── crates/
│   └── shared/
│       └── src/
├── apps/
│   ├── server/
│   │   ├── src/
│   │   └── migrations/
│   └── desktop/
│       ├── src-tauri/
│       │   ├── src/
│       │   └── capabilities/
│       └── src/
```

All `src/` directories will contain minimal stub files to allow compilation.

### Step 2: Root Cargo.toml (Workspace)

**File:** `/Users/nisar/personal/projects/openconv/Cargo.toml`

This is the Cargo workspace root. It declares three member crates and a `[workspace.dependencies]` table that pins shared dependency versions. Each member crate references these via `dependency_name.workspace = true` in its own `Cargo.toml`.

```toml
[workspace]
resolver = "2"
members = [
    "crates/shared",
    "apps/server",
    "apps/desktop/src-tauri",
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v7", "serde"] }
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
axum = { version = "0.8", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors", "limit"] }
dotenvy = "0.15"
toml = "0.8"
rusqlite = { version = "0.32", features = ["bundled"] }
```

**Key design decisions:**
- `resolver = "2"` is required for Cargo workspace feature unification.
- `thiserror = "2"` (not v1) -- this is a new project and v2 is current.
- `uuid` has `v7` feature for time-sortable IDs and `serde` feature for serialization.
- `sqlx` includes `runtime-tokio`, `postgres`, `uuid`, and `chrono` features for full server-side integration.
- `rusqlite` uses `bundled` feature to compile SQLite from source (avoids system SQLite version issues).

### Step 3: Shared Crate Cargo.toml

**File:** `/Users/nisar/personal/projects/openconv/crates/shared/Cargo.toml`

```toml
[package]
name = "openconv-shared"
version = "0.1.0"
edition = "2021"

[features]
default = []
sqlx = ["dep:sqlx"]

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }

[dependencies.sqlx]
workspace = true
optional = true
```

**Key design decisions:**
- The `sqlx` feature is optional. When enabled (by the server crate), typed IDs derive `sqlx::Type`. When disabled (by the desktop crate), no SQLx dependency is pulled in.
- All common dependencies use `workspace = true` to inherit versions from the root.

**Stub file:** `/Users/nisar/personal/projects/openconv/crates/shared/src/lib.rs`

```rust
//! OpenConv shared library — types, IDs, and API contracts shared between server and client.
```

This is intentionally a near-empty file. Section 02 (shared-crate) populates it with modules.

### Step 4: Server Crate Cargo.toml

**File:** `/Users/nisar/personal/projects/openconv/apps/server/Cargo.toml`

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
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
axum = { workspace = true }
tokio = { workspace = true }
sqlx = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
dotenvy = { workspace = true }
toml = { workspace = true }
```

**Key design decisions:**
- This crate imports `openconv-shared` with the `sqlx` feature enabled, so typed IDs can be used directly in SQLx queries.
- It is a binary crate (`[[bin]]`), not a library.

**Stub file:** `/Users/nisar/personal/projects/openconv/apps/server/src/main.rs`

```rust
//! OpenConv Axum server entry point.

fn main() {
    println!("openconv-server placeholder");
}
```

This placeholder will be replaced by section 03 (server-scaffold) with the full async Axum startup sequence. It exists only so `cargo build --workspace` succeeds.

### Step 5: Desktop (Tauri) Crate Cargo.toml

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/Cargo.toml`

```toml
[package]
name = "openconv-desktop"
version = "0.1.0"
edition = "2021"

[dependencies]
openconv-shared = { path = "../../../crates/shared" }
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }
rusqlite = { workspace = true }
tauri = { version = "2", features = ["tray-icon"] }
tauri-build = { version = "2", features = [] }

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

**Key design decisions:**
- This crate imports `openconv-shared` WITHOUT the `sqlx` feature -- the desktop client uses rusqlite, not SQLx.
- `tauri` and `tauri-build` are pinned to v2. These are NOT in `[workspace.dependencies]` because only the desktop crate uses them, and Tauri's build-dependency setup is crate-specific.
- The `tray-icon` feature enables system tray functionality.

**Stub files:**

`/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/main.rs`
```rust
//! OpenConv desktop entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    println!("openconv-desktop placeholder");
}
```

`/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/lib.rs`
```rust
//! OpenConv Tauri application library.
```

`/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/build.rs`
```rust
fn main() {
    tauri_build::build();
}
```

The `build.rs` file is required by Tauri -- without it, the crate will not compile. The `#![cfg_attr(...)]` attribute on `main.rs` hides the console window on Windows release builds, which is standard for GUI applications.

Section 05 (tauri-scaffold) replaces these stubs with the full Tauri app builder, IPC commands, and system tray setup.

### Step 5b: Minimal tauri.conf.json (Added During Implementation)

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/tauri.conf.json`

**Deviation from plan:** This file was not in the original plan (Section 05 was supposed to create it), but `tauri_build::build()` in `build.rs` requires it to exist for `cargo build --workspace` to succeed. A minimal stub was added to satisfy the Section 01 verification requirement. Section 05 will overwrite this with the full configuration.

Also added:
- `.gitkeep` files in `apps/server/migrations/`, `apps/desktop/src-tauri/capabilities/` to preserve empty directories in Git
- `apps/desktop/src/index.html` placeholder since `frontendDist` points to `../src`

### Step 6: Root package.json (npm Workspace)

**File:** `/Users/nisar/personal/projects/openconv/package.json`

```json
{
  "private": true,
  "workspaces": [
    "apps/desktop"
  ]
}
```

This declares the npm workspace. `"private": true` prevents accidental publishing to npm. The single workspace member is the desktop frontend at `apps/desktop/`.

### Step 7: Desktop Frontend package.json (Stub)

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/package.json`

Create a minimal `package.json` that section 07 (react-frontend) will expand with full React/Vite/Tailwind dependencies. For now, it must be valid JSON so `npm install` at the root works:

```json
{
  "name": "openconv-desktop",
  "private": true,
  "version": "0.1.0",
  "scripts": {}
}
```

Section 07 adds all dependencies (`react`, `vite`, `tailwindcss`, etc.) and scripts (`dev`, `build`, `test`, `lint`, `fmt`).

### Step 8: .gitignore

**File:** `/Users/nisar/personal/projects/openconv/.gitignore`

```gitignore
# Rust
target/

# Node
node_modules/

# Environment
.env

# SQLite databases
*.db

# OS files
.DS_Store
Thumbs.db

# IDE
.idea/
.vscode/
*.swp
*.swo
```

**Critical note:** `.sqlx/` is intentionally NOT listed here. The `.sqlx/` directory contains offline query metadata generated by `cargo sqlx prepare` and must be committed to the repository. Without it, CI builds fail because `SQLX_OFFLINE=true` relies on these cached query results instead of a live database connection.

### Step 9: .env.example

**File:** `/Users/nisar/personal/projects/openconv/.env.example`

```dotenv
# PostgreSQL connection (must match docker-compose.yml credentials)
DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv

# Server configuration (overrides config.toml values)
SERVER_HOST=0.0.0.0
SERVER_PORT=3000

# Logging
RUST_LOG=info,openconv=debug
```

This file is checked into version control as a template. Developers copy it to `.env` for local development. The credentials here are for local Docker Compose development only and must never be used in any deployed environment.

The `DATABASE_URL` must stay in sync with the Docker Compose PostgreSQL service configured in section 08 (dev-tooling). If one changes, the other must be updated to match.

---

## Verification

After completing all steps, run these commands from the repository root to verify:

1. **Cargo workspace compiles:** `cargo build --workspace` -- all three crates should compile with zero errors. Warnings are acceptable at this stage since the stubs are minimal, but clippy should be clean: `cargo clippy --workspace -- -D warnings`.

2. **npm workspace resolves:** `npm install` at the repo root should complete without errors and create `node_modules/` with workspace symlinks.

3. **Cargo discovers all members:** `cargo metadata --no-deps --format-version 1 | grep -o '"name":"openconv-[^"]*"'` should show `openconv-shared`, `openconv-server`, and `openconv-desktop`.

4. **Git exclusions are correct:** Confirm `.env` is ignored but `.env.example` is not. Confirm `target/` and `node_modules/` are ignored. Confirm `.sqlx/` is NOT ignored.

---

## Notes for Subsequent Sections

- **Section 02 (shared-crate)** populates `/Users/nisar/personal/projects/openconv/crates/shared/src/` with the `define_id!` macro, typed IDs, API types, error types, and constants.
- **Section 03 (server-scaffold)** replaces the server `main.rs` stub with the full Axum startup sequence and adds `config.rs`, `db.rs`, `routes/`, `middleware/`, and `error.rs` modules.
- **Section 05 (tauri-scaffold)** replaces the desktop stubs with the full Tauri app builder, adds `tauri.conf.json`, capabilities, `db.rs`, and IPC commands.
- **Section 07 (react-frontend)** expands the desktop `package.json` and creates the Vite/React/Tailwind scaffold under `/Users/nisar/personal/projects/openconv/apps/desktop/src/`.
- **Section 08 (dev-tooling)** creates the `justfile`, `docker-compose.yml`, and `config.toml` at the repository root.