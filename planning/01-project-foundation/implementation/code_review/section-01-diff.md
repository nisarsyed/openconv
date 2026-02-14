diff --git a/.env.example b/.env.example
new file mode 100644
index 0000000..1dd8be1
--- /dev/null
+++ b/.env.example
@@ -0,0 +1,9 @@
+# PostgreSQL connection (must match docker-compose.yml credentials)
+DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv
+
+# Server configuration (overrides config.toml values)
+SERVER_HOST=0.0.0.0
+SERVER_PORT=3000
+
+# Logging
+RUST_LOG=info,openconv=debug
diff --git a/.gitignore b/.gitignore
new file mode 100644
index 0000000..5517f23
--- /dev/null
+++ b/.gitignore
@@ -0,0 +1,21 @@
+# Rust
+target/
+
+# Node
+node_modules/
+
+# Environment
+.env
+
+# SQLite databases
+*.db
+
+# OS files
+.DS_Store
+Thumbs.db
+
+# IDE
+.idea/
+.vscode/
+*.swp
+*.swo
diff --git a/Cargo.toml b/Cargo.toml
new file mode 100644
index 0000000..80e7a6e
--- /dev/null
+++ b/Cargo.toml
@@ -0,0 +1,24 @@
+[workspace]
+resolver = "2"
+members = [
+    "crates/shared",
+    "apps/server",
+    "apps/desktop/src-tauri",
+]
+
+[workspace.dependencies]
+serde = { version = "1", features = ["derive"] }
+serde_json = "1"
+uuid = { version = "1", features = ["v7", "serde"] }
+thiserror = "2"
+chrono = { version = "0.4", features = ["serde"] }
+tracing = "0.1"
+tracing-subscriber = { version = "0.3", features = ["env-filter"] }
+axum = { version = "0.8", features = ["macros"] }
+tokio = { version = "1", features = ["full"] }
+sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono"] }
+tower = "0.5"
+tower-http = { version = "0.6", features = ["trace", "cors", "limit"] }
+dotenvy = "0.15"
+toml = "0.8"
+rusqlite = { version = "0.32", features = ["bundled"] }
diff --git a/apps/desktop/package.json b/apps/desktop/package.json
new file mode 100644
index 0000000..e86a30a
--- /dev/null
+++ b/apps/desktop/package.json
@@ -0,0 +1,6 @@
+{
+  "name": "openconv-desktop",
+  "private": true,
+  "version": "0.1.0",
+  "scripts": {}
+}
diff --git a/apps/desktop/src-tauri/Cargo.toml b/apps/desktop/src-tauri/Cargo.toml
new file mode 100644
index 0000000..f6590a7
--- /dev/null
+++ b/apps/desktop/src-tauri/Cargo.toml
@@ -0,0 +1,18 @@
+[package]
+name = "openconv-desktop"
+version = "0.1.0"
+edition = "2021"
+
+[dependencies]
+openconv-shared = { path = "../../../crates/shared" }
+serde = { workspace = true }
+serde_json = { workspace = true }
+uuid = { workspace = true }
+chrono = { workspace = true }
+tracing = { workspace = true }
+rusqlite = { workspace = true }
+tauri = { version = "2", features = ["tray-icon"] }
+tauri-build = { version = "2", features = [] }
+
+[build-dependencies]
+tauri-build = { version = "2", features = [] }
diff --git a/apps/desktop/src-tauri/build.rs b/apps/desktop/src-tauri/build.rs
new file mode 100644
index 0000000..261851f
--- /dev/null
+++ b/apps/desktop/src-tauri/build.rs
@@ -0,0 +1,3 @@
+fn main() {
+    tauri_build::build();
+}
diff --git a/apps/desktop/src-tauri/src/lib.rs b/apps/desktop/src-tauri/src/lib.rs
new file mode 100644
index 0000000..33131a6
--- /dev/null
+++ b/apps/desktop/src-tauri/src/lib.rs
@@ -0,0 +1 @@
+//! OpenConv Tauri application library.
diff --git a/apps/desktop/src-tauri/src/main.rs b/apps/desktop/src-tauri/src/main.rs
new file mode 100644
index 0000000..a8924dc
--- /dev/null
+++ b/apps/desktop/src-tauri/src/main.rs
@@ -0,0 +1,7 @@
+//! OpenConv desktop entry point.
+
+#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
+
+fn main() {
+    println!("openconv-desktop placeholder");
+}
diff --git a/apps/desktop/src-tauri/tauri.conf.json b/apps/desktop/src-tauri/tauri.conf.json
new file mode 100644
index 0000000..b611332
--- /dev/null
+++ b/apps/desktop/src-tauri/tauri.conf.json
@@ -0,0 +1,19 @@
+{
+  "$schema": "https://raw.githubusercontent.com/nicegui/nicegui/main/nicegui/tauri/tauri.conf.json",
+  "productName": "OpenConv",
+  "version": "0.1.0",
+  "identifier": "com.openconv.app",
+  "build": {
+    "devUrl": "http://localhost:1420",
+    "frontendDist": "../src"
+  },
+  "app": {
+    "windows": [
+      {
+        "title": "OpenConv",
+        "width": 1024,
+        "height": 768
+      }
+    ]
+  }
+}
diff --git a/apps/server/Cargo.toml b/apps/server/Cargo.toml
new file mode 100644
index 0000000..2224fd1
--- /dev/null
+++ b/apps/server/Cargo.toml
@@ -0,0 +1,24 @@
+[package]
+name = "openconv-server"
+version = "0.1.0"
+edition = "2021"
+
+[[bin]]
+name = "openconv-server"
+path = "src/main.rs"
+
+[dependencies]
+openconv-shared = { path = "../../crates/shared", features = ["sqlx"] }
+serde = { workspace = true }
+serde_json = { workspace = true }
+uuid = { workspace = true }
+chrono = { workspace = true }
+tracing = { workspace = true }
+tracing-subscriber = { workspace = true }
+axum = { workspace = true }
+tokio = { workspace = true }
+sqlx = { workspace = true }
+tower = { workspace = true }
+tower-http = { workspace = true }
+dotenvy = { workspace = true }
+toml = { workspace = true }
diff --git a/apps/server/src/main.rs b/apps/server/src/main.rs
new file mode 100644
index 0000000..9173575
--- /dev/null
+++ b/apps/server/src/main.rs
@@ -0,0 +1,5 @@
+//! OpenConv Axum server entry point.
+
+fn main() {
+    println!("openconv-server placeholder");
+}
diff --git a/crates/shared/Cargo.toml b/crates/shared/Cargo.toml
new file mode 100644
index 0000000..5701b94
--- /dev/null
+++ b/crates/shared/Cargo.toml
@@ -0,0 +1,19 @@
+[package]
+name = "openconv-shared"
+version = "0.1.0"
+edition = "2021"
+
+[features]
+default = []
+sqlx = ["dep:sqlx"]
+
+[dependencies]
+serde = { workspace = true }
+serde_json = { workspace = true }
+uuid = { workspace = true }
+thiserror = { workspace = true }
+chrono = { workspace = true }
+
+[dependencies.sqlx]
+workspace = true
+optional = true
diff --git a/crates/shared/src/lib.rs b/crates/shared/src/lib.rs
new file mode 100644
index 0000000..a57683e
--- /dev/null
+++ b/crates/shared/src/lib.rs
@@ -0,0 +1 @@
+//! OpenConv shared library — types, IDs, and API contracts shared between server and client.
diff --git a/package.json b/package.json
new file mode 100644
index 0000000..0cebe0a
--- /dev/null
+++ b/package.json
@@ -0,0 +1,6 @@
+{
+  "private": true,
+  "workspaces": [
+    "apps/desktop"
+  ]
+}
