diff --git a/config.toml b/config.toml
new file mode 100644
index 0000000..5f748cb
--- /dev/null
+++ b/config.toml
@@ -0,0 +1,10 @@
+# OpenConv Server Configuration
+# Development defaults -- safe for local use only.
+# Environment variables override these values (e.g., DATABASE_URL).
+
+host = "127.0.0.1"
+port = 3000
+database_url = "postgres://openconv:openconv@localhost:5432/openconv"
+max_db_connections = 5
+cors_origins = ["http://localhost:1420"]
+log_level = "debug"
diff --git a/docker-compose.yml b/docker-compose.yml
new file mode 100644
index 0000000..4608d4c
--- /dev/null
+++ b/docker-compose.yml
@@ -0,0 +1,20 @@
+services:
+  postgres:
+    image: postgres:15
+    container_name: openconv-postgres
+    ports:
+      - "5432:5432"
+    environment:
+      POSTGRES_DB: openconv
+      POSTGRES_USER: openconv
+      POSTGRES_PASSWORD: openconv
+    volumes:
+      - pgdata:/var/lib/postgresql/data
+    healthcheck:
+      test: ["CMD-SHELL", "pg_isready -U openconv"]
+      interval: 5s
+      timeout: 5s
+      retries: 5
+
+volumes:
+  pgdata:
diff --git a/justfile b/justfile
new file mode 100644
index 0000000..8e2b3f8
--- /dev/null
+++ b/justfile
@@ -0,0 +1,63 @@
+# OpenConv Development Commands
+
+# Launch Tauri desktop app with hot reload
+dev:
+    cd apps/desktop && npm run tauri dev
+
+# Start the Axum server
+server:
+    cargo run --bin openconv-server
+
+# Build all crates in release mode
+build:
+    cargo build --release
+
+# Start PostgreSQL container
+db-up:
+    docker compose up -d postgres
+
+# Stop and remove containers
+db-down:
+    docker compose down
+
+# Run pending PostgreSQL migrations
+db-migrate:
+    sqlx migrate run --source apps/server/migrations
+
+# Drop, recreate, and migrate the database
+db-reset:
+    sqlx database drop -y
+    sqlx database create
+    just db-migrate
+
+# Generate SQLx offline query data for CI
+sqlx-prepare:
+    cargo sqlx prepare --workspace
+
+# Run all tests (Rust + JavaScript)
+test:
+    cargo test --workspace
+    cd apps/desktop && npm test
+
+# Run Rust tests only
+test-rust:
+    cargo test --workspace
+
+# Run JavaScript tests only
+test-js:
+    cd apps/desktop && npm test
+
+# Lint all code (Clippy + ESLint)
+lint:
+    cargo clippy --workspace -- -D warnings
+    cd apps/desktop && npm run lint
+
+# Format all code
+fmt:
+    cargo fmt --all
+    cd apps/desktop && npm run fmt
+
+# Check formatting without modifying files
+fmt-check:
+    cargo fmt --all --check
+    cd apps/desktop && npm run fmt:check
diff --git a/planning/01-project-foundation/implementation/deep_implement_config.json b/planning/01-project-foundation/implementation/deep_implement_config.json
index 0228535..f9fccb3 100644
--- a/planning/01-project-foundation/implementation/deep_implement_config.json
+++ b/planning/01-project-foundation/implementation/deep_implement_config.json
@@ -41,6 +41,10 @@
     "section-06-sqlite-migrations": {
       "status": "complete",
       "commit_hash": "badca67"
+    },
+    "section-07-react-frontend": {
+      "status": "complete",
+      "commit_hash": "47747e6"
     }
   },
   "pre_commit": {
