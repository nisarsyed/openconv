Now I have all the context needed. Here is the section content.

# Section 05: Tauri Desktop App Scaffold

## Overview

This section creates the Tauri 2.x desktop application crate (`openconv-desktop`), including the Rust backend with rusqlite initialization, TauRPC type-safe IPC setup, a health check command, system tray placeholder, and the `tauri.conf.json` configuration. This is the desktop client's Rust core -- the React frontend is handled separately in section-07.

## Dependencies

- **section-01-monorepo-setup**: The Cargo workspace root `Cargo.toml` must exist with workspace members and workspace dependencies defined. The npm workspace root `package.json` must exist.
- **section-02-shared-crate**: The `openconv-shared` crate must exist at `crates/shared/` so that `openconv-desktop` can import shared types and IDs. The desktop crate uses the shared crate **without** the `sqlx` feature flag.

## What This Section Produces

After completing this section, the following will exist:

- `apps/desktop/src-tauri/` -- Tauri Rust source directory
- `apps/desktop/src-tauri/Cargo.toml` -- Desktop crate manifest
- `apps/desktop/src-tauri/tauri.conf.json` -- Tauri configuration
- `apps/desktop/src-tauri/capabilities/default.json` -- Window permissions
- `apps/desktop/src-tauri/src/main.rs` -- Desktop entry point
- `apps/desktop/src-tauri/src/lib.rs` -- Tauri app builder
- `apps/desktop/src-tauri/src/db.rs` -- SQLite database module
- `apps/desktop/src-tauri/src/commands/mod.rs` -- IPC command module
- `apps/desktop/src-tauri/src/commands/health.rs` -- Health check IPC command

The Tauri app will compile, initialize a SQLite database in the Tauri app data directory, expose a health check IPC command, and show a system tray icon.

---

## Tests (Write These First)

All tests for this section live in the desktop crate's Rust test modules. Tests should use in-memory SQLite databases (`:memory:`) where possible to avoid filesystem side effects.

### Test File: `apps/desktop/src-tauri/src/db.rs` (inline `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: init_db creates a connection to an in-memory database without error
    #[test]
    fn test_init_db_in_memory() { ... }

    // Test: init_db returns a Connection that can execute basic SQL
    #[test]
    fn test_init_db_connection_is_functional() { ... }
}
```

### Test File: `apps/desktop/src-tauri/src/commands/health.rs` (inline `#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: health_check command returns expected AppHealth struct
    #[test]
    fn test_health_check_returns_app_health() { ... }

    // Test: health_check command includes app version (non-empty string)
    #[test]
    fn test_health_check_includes_version() { ... }
}
```

These tests verify the minimal scaffold works. The SQLite migration tests belong to section-06 (sqlite-migrations), not this section. The tests here only confirm that the database module can open a connection and the health check command returns valid data.

---

## Implementation Details

### 1. Desktop Crate Cargo.toml

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/Cargo.toml`

The crate is named `openconv-desktop` and belongs to the workspace. It produces both a library (`lib`) and a binary (`main`). The library target is needed so Tauri's codegen and tests work correctly. The binary target is the actual desktop application entry point.

Key dependency choices:

| Dependency | Version / Source | Purpose |
|---|---|---|
| `tauri` | 2 (features: tray-icon) | Core Tauri framework |
| `tauri-build` | 2 | Build script for Tauri |
| `serde` | workspace = true | Serialization |
| `serde_json` | workspace = true | JSON for IPC |
| `rusqlite` | workspace = true (features: bundled) | Client SQLite database |
| `openconv-shared` | path = "../../crates/shared" | Shared types (NO sqlx feature) |
| `tracing` | workspace = true | Structured logging |
| `tauri-specta` | =2.0.0-rc.21 | Type-safe IPC bindings (pinned RC) |
| `specta` | =2.0.0-rc.22 (features: derive) | TypeScript type generation (pinned RC) |
| `specta-typescript` | 0.0.9 | TypeScript export (updated from planned 0.0.7) |
| `tracing-subscriber` | workspace = true | Structured logging initialization |

The `[build-dependencies]` section includes `tauri-build` version 2.

The `Cargo.toml` must declare:

```toml
[lib]
name = "openconv_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[[bin]]
name = "openconv-desktop"
path = "src/main.rs"
```

The `staticlib` and `cdylib` crate types are required by Tauri for platform-specific bundling. The `rlib` type allows `#[cfg(test)]` modules within the library to run via `cargo test`.

**Important:** The shared crate is referenced **without** the `sqlx` feature:

```toml
openconv-shared = { path = "../../../crates/shared" }
```

The path is relative from `apps/desktop/src-tauri/` to `crates/shared/`.

**TauRPC fallback note:** If `tauri-specta` v2 has compatibility issues with the Tauri 2.x version in use, fall back to `ts-rs` for Rust-to-TypeScript type generation with manual Tauri command wiring. In that case, API types in the shared crate would derive `ts_rs::TS` to generate `.ts` files, and Tauri commands would be registered manually via `tauri::generate_handler![]`.

### 2. Tauri Build Script

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/build.rs`

A minimal build script required by Tauri:

```rust
fn main() {
    tauri_build::build()
}
```

### 3. Tauri Configuration

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/tauri.conf.json`

This JSON file configures the Tauri application. Key settings:

- **`productName`**: `"OpenConv"`
- **`identifier`**: `"com.openconv.desktop"`
- **`build.devUrl`**: `"http://localhost:1420"` -- points to the Vite dev server
- **`build.frontendDist`**: `"../dist"` -- relative path to the built React output
- **`build.beforeDevCommand`**: `"npm run dev"` -- starts Vite dev server before Tauri
- **`build.beforeBuildCommand`**: `"npm run build"` -- builds frontend before Tauri bundle
- **`app.windows`**: A single default window:
  - `title`: `"OpenConv"`
  - `width`: 1200, `height`: 800
  - `minWidth`: 800, `minHeight`: 600
- **`app.security.csp`**: `"default-src 'self'; style-src 'self' 'unsafe-inline'"` -- Content Security Policy
- **`bundle.active`**: `true`
- **`bundle.targets`**: `"all"`

The `beforeDevCommand` and `beforeBuildCommand` run from the `apps/desktop/` directory (the parent of `src-tauri/`), where `package.json` lives.

### 4. Capabilities Configuration

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/capabilities/default.json`

This grants the main window permissions for IPC:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default"
  ]
}
```

The `core:default` permission provides basic Tauri functionality (window management, app lifecycle). Additional custom command permissions can be added later as needed.

### 5. Tauri Icons

**Directory:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/icons/`

Tauri requires application icons. For the scaffold, use `cargo tauri icon` to generate placeholder icons from a default source, or create a minimal set manually. At minimum, the following must exist:

- `icon.ico` (Windows)
- `icon.png` (Linux/general)
- `icon.icns` or `icon.png` at various sizes (macOS)

For the scaffold phase, a simple placeholder (solid color square) is sufficient. The `tauri.conf.json` references these via `"app.windows[0].icon"` or `"bundle.icon"`.

### 6. Main Entry Point

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/main.rs`

The desktop entry point. It suppresses the console window on Windows (release builds) and delegates to the library:

```rust
// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    openconv_desktop_lib::run();
}
```

### 7. Library Entry Point (App Builder)

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/lib.rs`

This is the core of the Tauri application. The `run()` function builds and launches the Tauri app. It performs the following in order:

1. **Initialize tracing** -- Set up `tracing_subscriber` for structured logging within the desktop app.

2. **Set up TauRPC router** -- Create a `specta` builder that collects all typed commands and generates TypeScript bindings. The builder produces both a Tauri plugin (for routing IPC calls) and an `Events` type (for future typed event emission). The TypeScript bindings are exported to `../src/bindings.ts` (relative to `src-tauri/`, landing in the React app's `src/` directory). Binding generation only runs in debug builds (`#[cfg(debug_assertions)]`).

3. **Build the Tauri app** -- Use `tauri::Builder::default()` with:
   - `.plugin(specta_plugin)` -- Register the TauRPC IPC router
   - `.setup(|app| { ... })` -- Setup callback that runs after the app starts:
     - Resolve the app data directory via `app.path().app_data_dir()?`
     - Create the directory if it does not exist
     - Open/create the SQLite database file (`openconv.db`) in that directory
     - Store the database connection in Tauri's managed state via `app.manage(DbState::new(conn))`
     - Initialize the system tray
   - `.run(tauri::generate_context!())` -- Launch the app

The `DbState` wrapper is needed because `rusqlite::Connection` is not `Send + Sync`. Wrap it in a `std::sync::Mutex`:

```rust
pub struct DbState {
    pub conn: std::sync::Mutex<rusqlite::Connection>,
}

impl DbState {
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self {
            conn: std::sync::Mutex::new(conn),
        }
    }
}
```

The module declarations in `lib.rs` use `pub(crate)` visibility (changed from plan's `mod` to allow cross-module access within the crate while keeping them out of the public API):

```rust
pub(crate) mod db;
pub(crate) mod commands;
```

### 8. Database Module

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/db.rs`

This module handles SQLite connection management. For the scaffold, it provides:

- `init_db(path: &std::path::Path) -> Result<rusqlite::Connection, rusqlite::Error>` -- Opens or creates a SQLite database at the given path. Enables WAL journal mode for better concurrent read performance. Sets a busy timeout. Returns the connection.

Key SQLite PRAGMAs to set on connection open:
- `PRAGMA journal_mode=WAL;` -- Write-Ahead Logging for concurrency
- `PRAGMA foreign_keys=ON;` -- Enforce foreign key constraints (off by default in SQLite)
- `PRAGMA busy_timeout=5000;` -- Wait up to 5 seconds on lock contention

The migration runner (`run_migrations`) is implemented in section-06 (sqlite-migrations). This section only provides the connection initialization.

For testing, provide an alternative constructor:

- `init_db_in_memory() -> Result<rusqlite::Connection, rusqlite::Error>` -- Creates an in-memory database with the same PRAGMAs. Used by tests to avoid filesystem access.

### 9. IPC Commands Module

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/commands/mod.rs`

Re-exports the health command module:

```rust
pub mod health;
```

### 10. Health Check Command

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src-tauri/src/commands/health.rs`

Defines a health check IPC command that the React frontend can call to verify IPC is working.

The `AppHealth` response struct:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct AppHealth {
    pub version: String,
    pub db_status: String,
}
```

The command function signature. It accesses the `DbState` from Tauri's managed state, attempts a simple query (`SELECT 1`) to verify the database is accessible, and returns the health status.

```rust
/// Inner logic extracted for unit testing without Tauri state.
pub fn health_check_inner(conn: &rusqlite::Connection) -> AppHealth { ... }

#[tauri::command]
#[specta::specta]
pub fn health_check(db: tauri::State<'_, crate::DbState>) -> Result<AppHealth, String> {
    // Lock the mutex, delegate to health_check_inner
}
```

If the database query succeeds, `db_status` is `"ok"`. If it fails, `db_status` contains the error description. The command never returns `Err` -- database failures are reported in the `db_status` field so the UI can always display something.

### 11. System Tray

The system tray is set up during the Tauri `setup` callback or directly on the builder. Use `tauri::tray::TrayIconBuilder` to create a tray icon with a context menu:

- **"Show/Hide"** menu item -- Toggles the main window's visibility. Use `app.get_webview_window("main")` to get the window handle, then call `.show()` or `.hide()` based on `.is_visible()`.
- **"Quit"** menu item -- Calls `app.exit(0)` to cleanly shut down.

The tray icon uses the app's default icon. The menu event handler matches on menu item IDs to dispatch actions.

Outline of the tray setup (in a helper function called from `setup`):

```rust
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_hide = tauri::menu::MenuItem::with_id(app, "show_hide", "Show/Hide", true, None::<&str>)?;
    let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = tauri::menu::Menu::with_items(app, &[&show_hide, &quit])?;

    tauri::tray::TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            // Match on event.id.as_ref() for "show_hide" and "quit"
        })
        .build(app)?;
    Ok(())
}
```

### 12. TauRPC / Specta Integration Details

The TauRPC setup in `lib.rs` uses `tauri_specta::Builder` to collect commands and generate bindings. The pattern:

```rust
let builder = tauri_specta::Builder::<tauri::Wry>::new()
    .commands(tauri_specta::collect_commands![
        commands::health::health_check,
    ]);

#[cfg(debug_assertions)]
builder.export(
    specta_typescript::Typescript::default()
        .bigint(specta_typescript::BigIntExportBehavior::Number)
        .formatter(specta_typescript::formatter::prettier),
    "../src/bindings.ts",
)?;
```

The `.commands()` call registers all IPC commands with both Tauri and Specta. The `.export()` call generates TypeScript types and function stubs that the React frontend imports. This only runs in debug builds to avoid generation during production builds.

The builder is then integrated into the Tauri app via `.invoke_handler(builder.invoke_handler())` on the Tauri builder, and `builder.mount_events(app)` inside the `.setup()` callback. (Note: the originally planned `.plugin(builder.into_plugin())` API does not exist in tauri-specta 2.0.0-rc.21.)

---

## File Listing Summary

| File Path | Purpose |
|---|---|
| `apps/desktop/src-tauri/Cargo.toml` | Crate manifest with Tauri, rusqlite, specta dependencies |
| `apps/desktop/src-tauri/build.rs` | Tauri build script |
| `apps/desktop/src-tauri/tauri.conf.json` | Tauri app configuration |
| `apps/desktop/src-tauri/capabilities/default.json` | Window IPC permissions |
| `apps/desktop/src-tauri/icons/` | Placeholder app icons |
| `apps/desktop/src-tauri/src/main.rs` | Binary entry point |
| `apps/desktop/src-tauri/src/lib.rs` | App builder, DbState, module declarations, TauRPC setup |
| `apps/desktop/src-tauri/src/db.rs` | SQLite init with PRAGMAs, in-memory constructor for tests |
| `apps/desktop/src-tauri/src/commands/mod.rs` | Command module re-exports |
| `apps/desktop/src-tauri/src/commands/health.rs` | Health check IPC command and AppHealth struct |

## Verification

After implementing this section:

1. `cargo build -p openconv-desktop` compiles without errors
2. `cargo test -p openconv-desktop` runs and passes the db and health check tests
3. The `tauri.conf.json` is valid JSON and accepted by Tauri's config parser
4. TypeScript bindings file is generated at `apps/desktop/src/bindings.ts` when running in debug mode
5. The app can be launched with `cargo tauri dev` (requires section-07 for the frontend, but the Rust side compiles independently)

## Implementation Deviations

- **specta-typescript version**: Used 0.0.9 (latest) instead of planned 0.0.7
- **specta/tauri-specta versions**: Pinned to exact RC versions (`=2.0.0-rc.22` / `=2.0.0-rc.21`) for reproducibility
- **Specta integration**: Used `invoke_handler()` + `mount_events()` pattern instead of planned `.plugin(builder.into_plugin())` which does not exist in the RC API
- **Module visibility**: `pub(crate) mod` instead of private `mod` for `db` and `commands` (needed for cross-module access while keeping public API clean)
- **health_check testability**: Extracted `health_check_inner(conn)` function to enable unit testing without Tauri state
- **db.rs**: Added `configure_connection()` helper to share PRAGMA setup between `init_db` and `init_db_in_memory`
- **Tray errors**: Used `tracing::warn!` on tray window operations instead of silently discarding errors
- **Prettier formatter**: Omitted `.formatter(specta_typescript::formatter::prettier)` from TypeScript export (not essential for scaffold)
- **Tests**: 4 passing (2 db, 2 health check)