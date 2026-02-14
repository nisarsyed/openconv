diff --git a/apps/desktop/src-tauri/src/db.rs b/apps/desktop/src-tauri/src/db.rs
index 36aa21c..ce1d40a 100644
--- a/apps/desktop/src-tauri/src/db.rs
+++ b/apps/desktop/src-tauri/src/db.rs
@@ -83,21 +83,17 @@ pub fn run_migrations(conn: &Connection) -> Result<()> {
         );",
     )?;
 
-    let current_version: i32 = conn
-        .query_row(
-            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
-            [],
-            |row| row.get(0),
-        )?;
+    let current_version: i32 = conn.query_row(
+        "SELECT COALESCE(MAX(version), 0) FROM _migrations",
+        [],
+        |row| row.get(0),
+    )?;
 
     for &(version, sql) in MIGRATIONS {
         if version > current_version {
             let tx = conn.unchecked_transaction()?;
             tx.execute_batch(sql)?;
-            tx.execute(
-                "INSERT INTO _migrations (version) VALUES (?1)",
-                [version],
-            )?;
+            tx.execute("INSERT INTO _migrations (version) VALUES (?1)", [version])?;
             tx.commit()?;
         }
     }
@@ -112,6 +108,7 @@ pub fn init_db(path: &std::path::Path) -> Result<Connection> {
     Ok(conn)
 }
 
+#[cfg(test)]
 pub fn init_db_in_memory() -> Result<Connection> {
     let conn = Connection::open_in_memory()?;
     configure_connection(&conn)?;
@@ -244,14 +241,39 @@ mod tests {
         conn.execute(
             "INSERT INTO local_user (id, public_key, email, display_name, avatar_url, token)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
-            ["user1", "pubkey123", "test@example.com", "Test User", "https://img.test/a.png", "token123"],
+            [
+                "user1",
+                "pubkey123",
+                "test@example.com",
+                "Test User",
+                "https://img.test/a.png",
+                "token123",
+            ],
         )
         .expect("should insert");
 
-        let (id, pk, email, name, avatar, token): (String, String, String, String, Option<String>, String) = conn
-            .query_row("SELECT id, public_key, email, display_name, avatar_url, token FROM local_user", [], |row| {
-                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
-            })
+        let (id, pk, email, name, avatar, token): (
+            String,
+            String,
+            String,
+            String,
+            Option<String>,
+            String,
+        ) = conn
+            .query_row(
+                "SELECT id, public_key, email, display_name, avatar_url, token FROM local_user",
+                [],
+                |row| {
+                    Ok((
+                        row.get(0)?,
+                        row.get(1)?,
+                        row.get(2)?,
+                        row.get(3)?,
+                        row.get(4)?,
+                        row.get(5)?,
+                    ))
+                },
+            )
             .expect("should query");
 
         assert_eq!(id, "user1");
@@ -272,9 +294,11 @@ mod tests {
         .expect("should insert");
 
         let (id, name, avatar): (String, String, Option<String>) = conn
-            .query_row("SELECT id, display_name, avatar_url FROM cached_users", [], |row| {
-                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
-            })
+            .query_row(
+                "SELECT id, display_name, avatar_url FROM cached_users",
+                [],
+                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
+            )
             .expect("should query");
 
         assert_eq!(id, "u1");
@@ -288,14 +312,22 @@ mod tests {
         conn.execute(
             "INSERT INTO cached_guilds (id, name, owner_id, icon_url, joined_at)
              VALUES (?1, ?2, ?3, ?4, ?5)",
-            ["g1", "Test Guild", "owner1", "https://img.test/icon.png", "2024-01-01T00:00:00"],
+            [
+                "g1",
+                "Test Guild",
+                "owner1",
+                "https://img.test/icon.png",
+                "2024-01-01T00:00:00",
+            ],
         )
         .expect("should insert");
 
         let (id, name, owner, icon): (String, String, String, Option<String>) = conn
-            .query_row("SELECT id, name, owner_id, icon_url FROM cached_guilds", [], |row| {
-                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
-            })
+            .query_row(
+                "SELECT id, name, owner_id, icon_url FROM cached_guilds",
+                [],
+                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
+            )
             .expect("should query");
 
         assert_eq!(id, "g1");
@@ -318,7 +350,15 @@ mod tests {
             .query_row(
                 "SELECT id, guild_id, name, channel_type, position FROM cached_channels",
                 [],
-                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
+                |row| {
+                    Ok((
+                        row.get(0)?,
+                        row.get(1)?,
+                        row.get(2)?,
+                        row.get(3)?,
+                        row.get(4)?,
+                    ))
+                },
             )
             .expect("should query");
 
@@ -335,15 +375,30 @@ mod tests {
         conn.execute(
             "INSERT INTO cached_messages (id, channel_id, sender_id, content, nonce, created_at)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
-            ["m1", "c1", "u1", "hello world", "nonce123", "2024-01-01T00:00:00"],
+            [
+                "m1",
+                "c1",
+                "u1",
+                "hello world",
+                "nonce123",
+                "2024-01-01T00:00:00",
+            ],
         )
         .expect("should insert");
 
-        let (id, ch, sender, content, nonce): (String, String, String, String, Option<String>) = conn
-            .query_row(
+        let (id, ch, sender, content, nonce): (String, String, String, String, Option<String>) =
+            conn.query_row(
                 "SELECT id, channel_id, sender_id, content, nonce FROM cached_messages",
                 [],
-                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
+                |row| {
+                    Ok((
+                        row.get(0)?,
+                        row.get(1)?,
+                        row.get(2)?,
+                        row.get(3)?,
+                        row.get(4)?,
+                    ))
+                },
             )
             .expect("should query");
 
@@ -361,7 +416,10 @@ mod tests {
                 |row| row.get(0),
             )
             .expect("should query for index");
-        assert!(idx_exists, "index idx_cached_messages_channel_created should exist");
+        assert!(
+            idx_exists,
+            "index idx_cached_messages_channel_created should exist"
+        );
     }
 
     #[test]
@@ -370,7 +428,14 @@ mod tests {
         conn.execute(
             "INSERT INTO cached_files (id, message_id, file_name, file_size, mime_type, local_path)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
-            ["f1", "m1", "photo.jpg", "1024", "image/jpeg", "/tmp/photo.jpg"],
+            [
+                "f1",
+                "m1",
+                "photo.jpg",
+                "1024",
+                "image/jpeg",
+                "/tmp/photo.jpg",
+            ],
         )
         .expect("should insert");
 
diff --git a/apps/desktop/src-tauri/src/lib.rs b/apps/desktop/src-tauri/src/lib.rs
index cc5c324..7395d09 100644
--- a/apps/desktop/src-tauri/src/lib.rs
+++ b/apps/desktop/src-tauri/src/lib.rs
@@ -1,5 +1,5 @@
-pub(crate) mod db;
 pub(crate) mod commands;
+pub(crate) mod db;
 
 pub struct DbState {
     pub conn: std::sync::Mutex<rusqlite::Connection>,
@@ -16,35 +16,34 @@ impl DbState {
 fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
     use tauri::Manager;
 
-    let show_hide = tauri::menu::MenuItem::with_id(app, "show_hide", "Show/Hide", true, None::<&str>)?;
+    let show_hide =
+        tauri::menu::MenuItem::with_id(app, "show_hide", "Show/Hide", true, None::<&str>)?;
     let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
     let menu = tauri::menu::Menu::with_items(app, &[&show_hide, &quit])?;
 
     tauri::tray::TrayIconBuilder::new()
         .menu(&menu)
-        .on_menu_event(|app, event| {
-            match event.id().as_ref() {
-                "show_hide" => {
-                    if let Some(window) = app.get_webview_window("main") {
-                        if window.is_visible().unwrap_or(false) {
-                            if let Err(e) = window.hide() {
-                                tracing::warn!("Failed to hide window: {e}");
-                            }
-                        } else {
-                            if let Err(e) = window.show() {
-                                tracing::warn!("Failed to show window: {e}");
-                            }
-                            if let Err(e) = window.set_focus() {
-                                tracing::warn!("Failed to focus window: {e}");
-                            }
+        .on_menu_event(|app, event| match event.id().as_ref() {
+            "show_hide" => {
+                if let Some(window) = app.get_webview_window("main") {
+                    if window.is_visible().unwrap_or(false) {
+                        if let Err(e) = window.hide() {
+                            tracing::warn!("Failed to hide window: {e}");
+                        }
+                    } else {
+                        if let Err(e) = window.show() {
+                            tracing::warn!("Failed to show window: {e}");
+                        }
+                        if let Err(e) = window.set_focus() {
+                            tracing::warn!("Failed to focus window: {e}");
                         }
                     }
                 }
-                "quit" => {
-                    app.exit(0);
-                }
-                _ => {}
             }
+            "quit" => {
+                app.exit(0);
+            }
+            _ => {}
         })
         .build(app)?;
     Ok(())
@@ -55,8 +54,8 @@ pub fn run() {
 
     tracing_subscriber::fmt::init();
 
-    let builder = tauri_specta::Builder::<tauri::Wry>::new()
-        .commands(tauri_specta::collect_commands![
+    let builder =
+        tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
             commands::health::health_check,
         ]);
 
@@ -78,8 +77,8 @@ pub fn run() {
             std::fs::create_dir_all(&app_data_dir)?;
 
             let db_path = app_data_dir.join("openconv.db");
-            let conn = db::init_db(&db_path)
-                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
+            let conn =
+                db::init_db(&db_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
             app.manage(DbState::new(conn));
 
             setup_tray(app)?;
diff --git a/apps/server/src/error.rs b/apps/server/src/error.rs
index 5b7d0e3..98f04de 100644
--- a/apps/server/src/error.rs
+++ b/apps/server/src/error.rs
@@ -58,14 +58,17 @@ mod tests {
 
     #[test]
     fn test_internal_maps_to_500() {
-        let response = ServerError(OpenConvError::Internal("something broke".into())).into_response();
+        let response =
+            ServerError(OpenConvError::Internal("something broke".into())).into_response();
         assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
     }
 
     #[tokio::test]
     async fn test_error_responses_are_json_with_error_field() {
         let response = ServerError(OpenConvError::NotFound).into_response();
-        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
+        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
+            .await
+            .unwrap();
         let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
         assert!(json.get("error").is_some());
         assert_eq!(json["error"], "not found");
diff --git a/apps/server/src/router.rs b/apps/server/src/router.rs
index 3cb358d..cc8aecb 100644
--- a/apps/server/src/router.rs
+++ b/apps/server/src/router.rs
@@ -1,8 +1,8 @@
+use axum::extract::DefaultBodyLimit;
 use axum::http::HeaderValue;
 use axum::middleware;
 use axum::routing::get;
 use tower_http::cors::{AllowOrigin, CorsLayer};
-use axum::extract::DefaultBodyLimit;
 use tower_http::trace::TraceLayer;
 
 use crate::handlers;
@@ -46,11 +46,10 @@ async fn request_id_middleware(
     next: middleware::Next,
 ) -> axum::response::Response {
     let request_id = uuid::Uuid::new_v4().to_string();
-    tracing::Span::current().record("request_id", &request_id.as_str());
+    tracing::Span::current().record("request_id", request_id.as_str());
     let mut response = next.run(request).await;
-    response.headers_mut().insert(
-        "x-request-id",
-        HeaderValue::from_str(&request_id).unwrap(),
-    );
+    response
+        .headers_mut()
+        .insert("x-request-id", HeaderValue::from_str(&request_id).unwrap());
     response
 }
diff --git a/apps/server/tests/health.rs b/apps/server/tests/health.rs
index a0a8a50..12c1749 100644
--- a/apps/server/tests/health.rs
+++ b/apps/server/tests/health.rs
@@ -35,17 +35,31 @@ async fn test_health_live_returns_200_with_status_ok() {
     let response = app.oneshot(request).await.unwrap();
     assert_eq!(response.status(), StatusCode::OK);
 
-    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
+    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
+        .await
+        .unwrap();
     let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
     assert_eq!(json["status"], "ok");
 }
 
-// TODO: Requires a live PgPool — enable after section-04 adds migrations.
-#[tokio::test]
-#[ignore]
-async fn test_health_ready_returns_200_when_db_connected() {
-    // Needs a real database connection. Use #[sqlx::test] once migrations exist.
-    todo!("implement with real database after section-04");
+#[sqlx::test]
+async fn test_health_ready_returns_200_when_db_connected(pool: sqlx::PgPool) {
+    let config = ServerConfig {
+        database_url: String::new(),
+        ..Default::default()
+    };
+    let state = AppState {
+        db: pool,
+        config: std::sync::Arc::new(config),
+    };
+    let app = build_router(state);
+    let request = Request::builder()
+        .uri("/health/ready")
+        .body(Body::empty())
+        .unwrap();
+
+    let response = app.oneshot(request).await.unwrap();
+    assert_eq!(response.status(), StatusCode::OK);
 }
 
 #[tokio::test]
@@ -70,7 +84,10 @@ async fn test_requests_include_x_request_id_header() {
 
     let response = app.oneshot(request).await.unwrap();
     let request_id = response.headers().get("x-request-id");
-    assert!(request_id.is_some(), "Response should include x-request-id header");
+    assert!(
+        request_id.is_some(),
+        "Response should include x-request-id header"
+    );
     // Verify the value is a valid UUID
     let id_str = request_id.unwrap().to_str().unwrap();
     uuid::Uuid::parse_str(id_str).expect("x-request-id should be a valid UUID");
@@ -89,7 +106,10 @@ async fn test_cors_headers_present() {
 
     let response = app.oneshot(request).await.unwrap();
     assert!(
-        response.headers().get("access-control-allow-origin").is_some(),
+        response
+            .headers()
+            .get("access-control-allow-origin")
+            .is_some(),
         "Response should include Access-Control-Allow-Origin header"
     );
 }
diff --git a/apps/server/tests/migrations.rs b/apps/server/tests/migrations.rs
index d987065..ae8e2d8 100644
--- a/apps/server/tests/migrations.rs
+++ b/apps/server/tests/migrations.rs
@@ -23,24 +23,21 @@ async fn all_migrations_apply_successfully(pool: PgPool) {
 #[sqlx::test]
 async fn users_table_insert_and_select(pool: PgPool) {
     let id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(id)
-    .bind("pk_test_user_1")
-    .bind("test@example.com")
-    .bind("Test User")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(id)
+        .bind("pk_test_user_1")
+        .bind("test@example.com")
+        .bind("Test User")
+        .execute(&pool)
+        .await
+        .unwrap();
 
-    let row: (uuid::Uuid, String, String, String) = sqlx::query_as(
-        "SELECT id, public_key, email, display_name FROM users WHERE id = $1",
-    )
-    .bind(id)
-    .fetch_one(&pool)
-    .await
-    .unwrap();
+    let row: (uuid::Uuid, String, String, String) =
+        sqlx::query_as("SELECT id, public_key, email, display_name FROM users WHERE id = $1")
+            .bind(id)
+            .fetch_one(&pool)
+            .await
+            .unwrap();
 
     assert_eq!(row.0, id);
     assert_eq!(row.1, "pk_test_user_1");
@@ -51,25 +48,22 @@ async fn users_table_insert_and_select(pool: PgPool) {
 /// Users table enforces UNIQUE constraint on public_key.
 #[sqlx::test]
 async fn users_table_unique_public_key(pool: PgPool) {
-    sqlx::query(
-        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
-    )
-    .bind("same_pk")
-    .bind("user1@example.com")
-    .bind("User 1")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
+        .bind("same_pk")
+        .bind("user1@example.com")
+        .bind("User 1")
+        .execute(&pool)
+        .await
+        .unwrap();
 
-    let err = sqlx::query(
-        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
-    )
-    .bind("same_pk")
-    .bind("user2@example.com")
-    .bind("User 2")
-    .execute(&pool)
-    .await
-    .unwrap_err();
+    let err =
+        sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
+            .bind("same_pk")
+            .bind("user2@example.com")
+            .bind("User 2")
+            .execute(&pool)
+            .await
+            .unwrap_err();
 
     assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
 }
@@ -77,25 +71,22 @@ async fn users_table_unique_public_key(pool: PgPool) {
 /// Users table enforces UNIQUE constraint on email.
 #[sqlx::test]
 async fn users_table_unique_email(pool: PgPool) {
-    sqlx::query(
-        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
-    )
-    .bind("pk_1")
-    .bind("same@example.com")
-    .bind("User 1")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
+        .bind("pk_1")
+        .bind("same@example.com")
+        .bind("User 1")
+        .execute(&pool)
+        .await
+        .unwrap();
 
-    let err = sqlx::query(
-        "INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)",
-    )
-    .bind("pk_2")
-    .bind("same@example.com")
-    .bind("User 2")
-    .execute(&pool)
-    .await
-    .unwrap_err();
+    let err =
+        sqlx::query("INSERT INTO users (public_key, email, display_name) VALUES ($1, $2, $3)")
+            .bind("pk_2")
+            .bind("same@example.com")
+            .bind("User 2")
+            .execute(&pool)
+            .await
+            .unwrap_err();
 
     assert_eq!(pg_error_code(&err).as_deref(), Some(PG_UNIQUE_VIOLATION));
 }
@@ -104,16 +95,14 @@ async fn users_table_unique_email(pool: PgPool) {
 #[sqlx::test]
 async fn users_updated_at_trigger(pool: PgPool) {
     let id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(id)
-    .bind("pk_trigger_test")
-    .bind("trigger@example.com")
-    .bind("Before")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(id)
+        .bind("pk_trigger_test")
+        .bind("trigger@example.com")
+        .bind("Before")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let before: (chrono::DateTime<chrono::Utc>,) =
         sqlx::query_as("SELECT updated_at FROM users WHERE id = $1")
@@ -149,14 +138,12 @@ async fn users_updated_at_trigger(pool: PgPool) {
 #[sqlx::test]
 async fn guilds_fk_owner_id_rejects_nonexistent_user(pool: PgPool) {
     let fake_user = uuid::Uuid::new_v4();
-    let err = sqlx::query(
-        "INSERT INTO guilds (name, owner_id) VALUES ($1, $2)",
-    )
-    .bind("Test Guild")
-    .bind(fake_user)
-    .execute(&pool)
-    .await
-    .unwrap_err();
+    let err = sqlx::query("INSERT INTO guilds (name, owner_id) VALUES ($1, $2)")
+        .bind("Test Guild")
+        .bind(fake_user)
+        .execute(&pool)
+        .await
+        .unwrap_err();
 
     assert_eq!(pg_error_code(&err).as_deref(), Some(PG_FK_VIOLATION));
 }
@@ -165,16 +152,14 @@ async fn guilds_fk_owner_id_rejects_nonexistent_user(pool: PgPool) {
 #[sqlx::test]
 async fn guilds_updated_at_trigger(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_guild_trigger")
-    .bind("guild_trigger@example.com")
-    .bind("Owner")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_guild_trigger")
+        .bind("guild_trigger@example.com")
+        .bind("Owner")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let guild_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
@@ -218,16 +203,14 @@ async fn guilds_updated_at_trigger(pool: PgPool) {
 #[sqlx::test]
 async fn channels_cascade_delete_on_guild_delete(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_cascade_test")
-    .bind("cascade@example.com")
-    .bind("Cascade Tester")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_cascade_test")
+        .bind("cascade@example.com")
+        .bind("Cascade Tester")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let guild_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
@@ -253,12 +236,11 @@ async fn channels_cascade_delete_on_guild_delete(pool: PgPool) {
         .await
         .unwrap();
 
-    let count: (i64,) =
-        sqlx::query_as("SELECT COUNT(*) FROM channels WHERE id = $1")
-            .bind(channel_id)
-            .fetch_one(&pool)
-            .await
-            .unwrap();
+    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels WHERE id = $1")
+        .bind(channel_id)
+        .fetch_one(&pool)
+        .await
+        .unwrap();
 
     assert_eq!(count.0, 0, "channel should be cascade deleted");
 }
@@ -267,16 +249,14 @@ async fn channels_cascade_delete_on_guild_delete(pool: PgPool) {
 #[sqlx::test]
 async fn channels_unique_guild_id_name(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_chan_unique")
-    .bind("chan_unique@example.com")
-    .bind("Chan Unique")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_chan_unique")
+        .bind("chan_unique@example.com")
+        .bind("Chan Unique")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let guild_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
@@ -308,16 +288,14 @@ async fn channels_unique_guild_id_name(pool: PgPool) {
 #[sqlx::test]
 async fn messages_check_rejects_both_null(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_msg_null")
-    .bind("msg_null@example.com")
-    .bind("Msg Null")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_msg_null")
+        .bind("msg_null@example.com")
+        .bind("Msg Null")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let err = sqlx::query(
         "INSERT INTO messages (sender_id, encrypted_content, nonce) VALUES ($1, $2, $3)",
@@ -336,16 +314,14 @@ async fn messages_check_rejects_both_null(pool: PgPool) {
 #[sqlx::test]
 async fn messages_check_rejects_both_set(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_msg_both")
-    .bind("msg_both@example.com")
-    .bind("Msg Both")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_msg_both")
+        .bind("msg_both@example.com")
+        .bind("Msg Both")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let guild_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
@@ -391,16 +367,14 @@ async fn messages_check_rejects_both_set(pool: PgPool) {
 #[sqlx::test]
 async fn messages_accepts_channel_id_only(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_msg_chan")
-    .bind("msg_chan@example.com")
-    .bind("Msg Chan")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_msg_chan")
+        .bind("msg_chan@example.com")
+        .bind("Msg Chan")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let guild_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
@@ -436,16 +410,14 @@ async fn messages_accepts_channel_id_only(pool: PgPool) {
 #[sqlx::test]
 async fn messages_accepts_dm_channel_id_only(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_msg_dm")
-    .bind("msg_dm@example.com")
-    .bind("Msg DM")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_msg_dm")
+        .bind("msg_dm@example.com")
+        .bind("Msg DM")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let dm_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
@@ -477,16 +449,14 @@ async fn messages_accepts_dm_channel_id_only(pool: PgPool) {
 #[sqlx::test]
 async fn guild_members_no_duplicate_membership(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_gm_dup")
-    .bind("gm_dup@example.com")
-    .bind("GM Dup")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_gm_dup")
+        .bind("gm_dup@example.com")
+        .bind("GM Dup")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let guild_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
@@ -518,16 +488,14 @@ async fn guild_members_no_duplicate_membership(pool: PgPool) {
 #[sqlx::test]
 async fn dm_channel_members_no_duplicate_membership(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_dm_dup")
-    .bind("dm_dup@example.com")
-    .bind("DM Dup")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_dm_dup")
+        .bind("dm_dup@example.com")
+        .bind("DM Dup")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     let dm_id = uuid::Uuid::new_v4();
     sqlx::query("INSERT INTO dm_channels (id) VALUES ($1)")
@@ -558,16 +526,14 @@ async fn dm_channel_members_no_duplicate_membership(pool: PgPool) {
 #[sqlx::test]
 async fn files_allows_null_message_id(pool: PgPool) {
     let user_id = uuid::Uuid::new_v4();
-    sqlx::query(
-        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
-    )
-    .bind(user_id)
-    .bind("pk_file_null")
-    .bind("file_null@example.com")
-    .bind("File Null")
-    .execute(&pool)
-    .await
-    .unwrap();
+    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
+        .bind(user_id)
+        .bind("pk_file_null")
+        .bind("file_null@example.com")
+        .bind("File Null")
+        .execute(&pool)
+        .await
+        .unwrap();
 
     sqlx::query(
         "INSERT INTO files (uploader_id, file_name, mime_type, size_bytes, storage_path, encrypted_blob_key) VALUES ($1, $2, $3, $4, $5, $6)",
diff --git a/crates/shared/src/api/mod.rs b/crates/shared/src/api/mod.rs
index 24b48e1..ecffce0 100644
--- a/crates/shared/src/api/mod.rs
+++ b/crates/shared/src/api/mod.rs
@@ -1,5 +1,5 @@
 pub mod auth;
-pub mod guild;
 pub mod channel;
+pub mod guild;
 pub mod message;
 pub mod user;
diff --git a/crates/shared/src/lib.rs b/crates/shared/src/lib.rs
index 6c7d94e..d9e8a45 100644
--- a/crates/shared/src/lib.rs
+++ b/crates/shared/src/lib.rs
@@ -1,6 +1,6 @@
 //! OpenConv shared library — types, IDs, and API contracts shared between server and client.
 
-pub mod ids;
 pub mod api;
-pub mod error;
 pub mod constants;
+pub mod error;
+pub mod ids;
diff --git a/planning/01-project-foundation/implementation/deep_implement_config.json b/planning/01-project-foundation/implementation/deep_implement_config.json
index f9fccb3..ab083ff 100644
--- a/planning/01-project-foundation/implementation/deep_implement_config.json
+++ b/planning/01-project-foundation/implementation/deep_implement_config.json
@@ -45,6 +45,10 @@
     "section-07-react-frontend": {
       "status": "complete",
       "commit_hash": "47747e6"
+    },
+    "section-08-dev-tooling": {
+      "status": "complete",
+      "commit_hash": "e93ccb7"
     }
   },
   "pre_commit": {
