diff --git a/Cargo.lock b/Cargo.lock
index 62e7346..7535971 100644
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -2,6 +2,12 @@
 # It is not intended for manual editing.
 version = 4
 
+[[package]]
+name = "Inflector"
+version = "0.11.4"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "fe438c63458706e03479442743baae6c88256498e6431708f6dfc520a26515d3"
+
 [[package]]
 name = "adler2"
 version = "2.0.1"
@@ -2446,9 +2452,13 @@ dependencies = [
  "rusqlite",
  "serde",
  "serde_json",
+ "specta",
+ "specta-typescript",
  "tauri",
  "tauri-build",
+ "tauri-specta",
  "tracing",
+ "tracing-subscriber",
  "uuid",
 ]
 
@@ -2545,6 +2555,12 @@ dependencies = [
  "windows-link 0.2.1",
 ]
 
+[[package]]
+name = "paste"
+version = "1.0.15"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "57c0d7b74b563b49d38dae00a0c37d4d6de9b432382b2892f0574ddcae73fd0a"
+
 [[package]]
 name = "pem-rfc7468"
 version = "0.7.0"
@@ -3567,6 +3583,50 @@ dependencies = [
  "system-deps",
 ]
 
+[[package]]
+name = "specta"
+version = "2.0.0-rc.22"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "ab7f01e9310a820edd31c80fde3cae445295adde21a3f9416517d7d65015b971"
+dependencies = [
+ "paste",
+ "specta-macros",
+ "thiserror 1.0.69",
+]
+
+[[package]]
+name = "specta-macros"
+version = "2.0.0-rc.18"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "c0074b9e30ed84c6924eb63ad8d2fe71cdc82628525d84b1fcb1f2fd40676517"
+dependencies = [
+ "Inflector",
+ "proc-macro2",
+ "quote",
+ "syn 2.0.115",
+]
+
+[[package]]
+name = "specta-serde"
+version = "0.0.9"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "77216504061374659e7245eac53d30c7b3e5fe64b88da97c753e7184b0781e63"
+dependencies = [
+ "specta",
+ "thiserror 1.0.69",
+]
+
+[[package]]
+name = "specta-typescript"
+version = "0.0.9"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "3220a0c365e51e248ac98eab5a6a32f544ff6f961906f09d3ee10903a4f52b2d"
+dependencies = [
+ "specta",
+ "specta-serde",
+ "thiserror 1.0.69",
+]
+
 [[package]]
 name = "spin"
 version = "0.9.8"
@@ -3994,6 +4054,7 @@ dependencies = [
  "serde_json",
  "serde_repr",
  "serialize-to-javascript",
+ "specta",
  "swift-rs",
  "tauri-build",
  "tauri-macros",
@@ -4125,6 +4186,34 @@ dependencies = [
  "wry",
 ]
 
+[[package]]
+name = "tauri-specta"
+version = "2.0.0-rc.21"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "b23c0132dd3cf6064e5cd919b82b3f47780e9280e7b5910babfe139829b76655"
+dependencies = [
+ "heck 0.5.0",
+ "serde",
+ "serde_json",
+ "specta",
+ "specta-typescript",
+ "tauri",
+ "tauri-specta-macros",
+ "thiserror 2.0.18",
+]
+
+[[package]]
+name = "tauri-specta-macros"
+version = "2.0.0-rc.16"
+source = "registry+https://github.com/rust-lang/crates.io-index"
+checksum = "7a4aa93823e07859546aa796b8a5d608190cd8037a3a5dce3eb63d491c34bda8"
+dependencies = [
+ "heck 0.5.0",
+ "proc-macro2",
+ "quote",
+ "syn 2.0.115",
+]
+
 [[package]]
 name = "tauri-utils"
 version = "2.8.2"
diff --git a/apps/desktop/src-tauri/Cargo.toml b/apps/desktop/src-tauri/Cargo.toml
index f6590a7..2c71b3e 100644
--- a/apps/desktop/src-tauri/Cargo.toml
+++ b/apps/desktop/src-tauri/Cargo.toml
@@ -3,6 +3,14 @@ name = "openconv-desktop"
 version = "0.1.0"
 edition = "2021"
 
+[lib]
+name = "openconv_desktop_lib"
+crate-type = ["staticlib", "cdylib", "rlib"]
+
+[[bin]]
+name = "openconv-desktop"
+path = "src/main.rs"
+
 [dependencies]
 openconv-shared = { path = "../../../crates/shared" }
 serde = { workspace = true }
@@ -10,9 +18,12 @@ serde_json = { workspace = true }
 uuid = { workspace = true }
 chrono = { workspace = true }
 tracing = { workspace = true }
+tracing-subscriber = { workspace = true }
 rusqlite = { workspace = true }
 tauri = { version = "2", features = ["tray-icon"] }
-tauri-build = { version = "2", features = [] }
+specta = { version = "=2.0.0-rc.22", features = ["derive"] }
+tauri-specta = { version = "=2.0.0-rc.21", features = ["derive", "typescript"] }
+specta-typescript = "0.0.9"
 
 [build-dependencies]
 tauri-build = { version = "2", features = [] }
diff --git a/apps/desktop/src-tauri/capabilities/default.json b/apps/desktop/src-tauri/capabilities/default.json
new file mode 100644
index 0000000..6ff2257
--- /dev/null
+++ b/apps/desktop/src-tauri/capabilities/default.json
@@ -0,0 +1,9 @@
+{
+  "$schema": "../gen/schemas/desktop-schema.json",
+  "identifier": "default",
+  "description": "Default capabilities for the main window",
+  "windows": ["main"],
+  "permissions": [
+    "core:default"
+  ]
+}
diff --git a/apps/desktop/src-tauri/gen/schemas/capabilities.json b/apps/desktop/src-tauri/gen/schemas/capabilities.json
index 9e26dfe..6721f72 100644
--- a/apps/desktop/src-tauri/gen/schemas/capabilities.json
+++ b/apps/desktop/src-tauri/gen/schemas/capabilities.json
@@ -1 +1 @@
-{}
\ No newline at end of file
+{"default":{"identifier":"default","description":"Default capabilities for the main window","local":true,"windows":["main"],"permissions":["core:default"]}}
\ No newline at end of file
diff --git a/apps/desktop/src-tauri/icons/128x128.png b/apps/desktop/src-tauri/icons/128x128.png
new file mode 100644
index 0000000..f7c1ccd
Binary files /dev/null and b/apps/desktop/src-tauri/icons/128x128.png differ
diff --git a/apps/desktop/src-tauri/icons/128x128@2x.png b/apps/desktop/src-tauri/icons/128x128@2x.png
new file mode 100644
index 0000000..8f12086
Binary files /dev/null and b/apps/desktop/src-tauri/icons/128x128@2x.png differ
diff --git a/apps/desktop/src-tauri/icons/32x32.png b/apps/desktop/src-tauri/icons/32x32.png
new file mode 100644
index 0000000..956a6bc
Binary files /dev/null and b/apps/desktop/src-tauri/icons/32x32.png differ
diff --git a/apps/desktop/src-tauri/icons/icon.icns b/apps/desktop/src-tauri/icons/icon.icns
new file mode 100644
index 0000000..8f12086
Binary files /dev/null and b/apps/desktop/src-tauri/icons/icon.icns differ
diff --git a/apps/desktop/src-tauri/icons/icon.ico b/apps/desktop/src-tauri/icons/icon.ico
new file mode 100644
index 0000000..7e3fc9b
Binary files /dev/null and b/apps/desktop/src-tauri/icons/icon.ico differ
diff --git a/apps/desktop/src-tauri/icons/icon.png b/apps/desktop/src-tauri/icons/icon.png
new file mode 100644
index 0000000..8f12086
Binary files /dev/null and b/apps/desktop/src-tauri/icons/icon.png differ
diff --git a/apps/desktop/src-tauri/src/commands/health.rs b/apps/desktop/src-tauri/src/commands/health.rs
new file mode 100644
index 0000000..6fdab63
--- /dev/null
+++ b/apps/desktop/src-tauri/src/commands/health.rs
@@ -0,0 +1,47 @@
+use serde::{Deserialize, Serialize};
+use specta::Type;
+
+#[derive(Debug, Clone, Serialize, Deserialize, Type)]
+pub struct AppHealth {
+    pub version: String,
+    pub db_status: String,
+}
+
+/// Inner logic for health check, testable without Tauri state.
+pub fn health_check_inner(conn: &rusqlite::Connection) -> AppHealth {
+    let db_status = match conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0)) {
+        Ok(_) => "ok".to_string(),
+        Err(e) => e.to_string(),
+    };
+
+    AppHealth {
+        version: env!("CARGO_PKG_VERSION").to_string(),
+        db_status,
+    }
+}
+
+#[tauri::command]
+#[specta::specta]
+pub fn health_check(db: tauri::State<'_, crate::DbState>) -> Result<AppHealth, String> {
+    let conn = db.conn.lock().map_err(|e| e.to_string())?;
+    Ok(health_check_inner(&conn))
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn test_health_check_returns_app_health() {
+        let conn = crate::db::init_db_in_memory().expect("should create db");
+        let health = health_check_inner(&conn);
+        assert_eq!(health.db_status, "ok");
+    }
+
+    #[test]
+    fn test_health_check_includes_version() {
+        let conn = crate::db::init_db_in_memory().expect("should create db");
+        let health = health_check_inner(&conn);
+        assert!(!health.version.is_empty(), "version should not be empty");
+    }
+}
diff --git a/apps/desktop/src-tauri/src/commands/mod.rs b/apps/desktop/src-tauri/src/commands/mod.rs
new file mode 100644
index 0000000..43a7c76
--- /dev/null
+++ b/apps/desktop/src-tauri/src/commands/mod.rs
@@ -0,0 +1 @@
+pub mod health;
diff --git a/apps/desktop/src-tauri/src/db.rs b/apps/desktop/src-tauri/src/db.rs
new file mode 100644
index 0000000..62ecdf9
--- /dev/null
+++ b/apps/desktop/src-tauri/src/db.rs
@@ -0,0 +1,54 @@
+use rusqlite::{Connection, Result};
+
+fn configure_connection(conn: &Connection) -> Result<()> {
+    conn.execute_batch(
+        "PRAGMA journal_mode=WAL;
+         PRAGMA foreign_keys=ON;
+         PRAGMA busy_timeout=5000;",
+    )
+}
+
+pub fn init_db(path: &std::path::Path) -> Result<Connection> {
+    let conn = Connection::open(path)?;
+    configure_connection(&conn)?;
+    Ok(conn)
+}
+
+pub fn init_db_in_memory() -> Result<Connection> {
+    let conn = Connection::open_in_memory()?;
+    configure_connection(&conn)?;
+    Ok(conn)
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn test_init_db_in_memory() {
+        let conn = init_db_in_memory().expect("should create in-memory db");
+        let mode: String = conn
+            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
+            .expect("should query journal_mode");
+        // In-memory databases use "memory" journal mode regardless of WAL setting
+        assert!(
+            mode == "wal" || mode == "memory",
+            "unexpected journal_mode: {mode}"
+        );
+    }
+
+    #[test]
+    fn test_init_db_connection_is_functional() {
+        let conn = init_db_in_memory().expect("should create in-memory db");
+        let result: i64 = conn
+            .query_row("SELECT 1", [], |row| row.get(0))
+            .expect("should execute query");
+        assert_eq!(result, 1);
+
+        // Verify foreign keys are enabled
+        let fk: i64 = conn
+            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
+            .expect("should query foreign_keys");
+        assert_eq!(fk, 1, "foreign_keys should be enabled");
+    }
+}
diff --git a/apps/desktop/src-tauri/src/lib.rs b/apps/desktop/src-tauri/src/lib.rs
index 33131a6..9693737 100644
--- a/apps/desktop/src-tauri/src/lib.rs
+++ b/apps/desktop/src-tauri/src/lib.rs
@@ -1 +1,85 @@
-//! OpenConv Tauri application library.
+pub mod db;
+pub mod commands;
+
+pub struct DbState {
+    pub conn: std::sync::Mutex<rusqlite::Connection>,
+}
+
+impl DbState {
+    pub fn new(conn: rusqlite::Connection) -> Self {
+        Self {
+            conn: std::sync::Mutex::new(conn),
+        }
+    }
+}
+
+fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
+    use tauri::Manager;
+
+    let show_hide = tauri::menu::MenuItem::with_id(app, "show_hide", "Show/Hide", true, None::<&str>)?;
+    let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
+    let menu = tauri::menu::Menu::with_items(app, &[&show_hide, &quit])?;
+
+    tauri::tray::TrayIconBuilder::new()
+        .menu(&menu)
+        .on_menu_event(|app, event| {
+            match event.id().as_ref() {
+                "show_hide" => {
+                    if let Some(window) = app.get_webview_window("main") {
+                        if window.is_visible().unwrap_or(false) {
+                            let _ = window.hide();
+                        } else {
+                            let _ = window.show();
+                            let _ = window.set_focus();
+                        }
+                    }
+                }
+                "quit" => {
+                    app.exit(0);
+                }
+                _ => {}
+            }
+        })
+        .build(app)?;
+    Ok(())
+}
+
+pub fn run() {
+    use tauri::Manager;
+
+    tracing_subscriber::fmt::init();
+
+    let builder = tauri_specta::Builder::<tauri::Wry>::new()
+        .commands(tauri_specta::collect_commands![
+            commands::health::health_check,
+        ]);
+
+    #[cfg(debug_assertions)]
+    builder
+        .export(
+            specta_typescript::Typescript::default()
+                .bigint(specta_typescript::BigIntExportBehavior::Number),
+            "../src/bindings.ts",
+        )
+        .expect("failed to export typescript bindings");
+
+    tauri::Builder::default()
+        .invoke_handler(builder.invoke_handler())
+        .setup(move |app| {
+            builder.mount_events(app);
+
+            let app_data_dir = app.path().app_data_dir()?;
+            std::fs::create_dir_all(&app_data_dir)?;
+
+            let db_path = app_data_dir.join("openconv.db");
+            let conn = db::init_db(&db_path)
+                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
+            app.manage(DbState::new(conn));
+
+            setup_tray(app)?;
+
+            Ok(())
+        })
+        .run(tauri::generate_context!())
+        .expect("error while running tauri application");
+}
diff --git a/apps/desktop/src-tauri/src/main.rs b/apps/desktop/src-tauri/src/main.rs
index a8924dc..4dc7713 100644
--- a/apps/desktop/src-tauri/src/main.rs
+++ b/apps/desktop/src-tauri/src/main.rs
@@ -1,7 +1,5 @@
-//! OpenConv desktop entry point.
-
 #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
 
 fn main() {
-    println!("openconv-desktop placeholder");
+    openconv_desktop_lib::run();
 }
diff --git a/apps/desktop/src-tauri/tauri.conf.json b/apps/desktop/src-tauri/tauri.conf.json
index 8593c68..7a700ff 100644
--- a/apps/desktop/src-tauri/tauri.conf.json
+++ b/apps/desktop/src-tauri/tauri.conf.json
@@ -1,18 +1,36 @@
 {
   "productName": "OpenConv",
   "version": "0.1.0",
-  "identifier": "com.openconv.app",
+  "identifier": "com.openconv.desktop",
   "build": {
     "devUrl": "http://localhost:1420",
-    "frontendDist": "../src"
+    "frontendDist": "../dist",
+    "beforeDevCommand": "npm run dev",
+    "beforeBuildCommand": "npm run build"
   },
   "app": {
     "windows": [
       {
         "title": "OpenConv",
-        "width": 1024,
-        "height": 768
+        "width": 1200,
+        "height": 800,
+        "minWidth": 800,
+        "minHeight": 600
       }
+    ],
+    "security": {
+      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'"
+    }
+  },
+  "bundle": {
+    "active": true,
+    "targets": "all",
+    "icon": [
+      "icons/32x32.png",
+      "icons/128x128.png",
+      "icons/128x128@2x.png",
+      "icons/icon.icns",
+      "icons/icon.ico"
     ]
   }
 }
