diff --git a/Cargo.lock b/Cargo.lock
index 7120647..62e7346 100644
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -2463,6 +2463,7 @@ dependencies = [
  "serde",
  "serde_json",
  "sqlx",
+ "thiserror 2.0.18",
  "tokio",
  "toml 0.8.23",
  "tower",
diff --git a/Cargo.toml b/Cargo.toml
index 80e7a6e..0a19466 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -9,7 +9,7 @@ members = [
 [workspace.dependencies]
 serde = { version = "1", features = ["derive"] }
 serde_json = "1"
-uuid = { version = "1", features = ["v7", "serde"] }
+uuid = { version = "1", features = ["v4", "v7", "serde"] }
 thiserror = "2"
 chrono = { version = "0.4", features = ["serde"] }
 tracing = "0.1"
diff --git a/apps/server/Cargo.toml b/apps/server/Cargo.toml
index 2224fd1..0a8ad93 100644
--- a/apps/server/Cargo.toml
+++ b/apps/server/Cargo.toml
@@ -22,3 +22,8 @@ tower = { workspace = true }
 tower-http = { workspace = true }
 dotenvy = { workspace = true }
 toml = { workspace = true }
+thiserror = { workspace = true }
+
+[dev-dependencies]
+serde_json = { workspace = true }
+uuid = { workspace = true }
diff --git a/apps/server/config.toml b/apps/server/config.toml
new file mode 100644
index 0000000..89cabd1
--- /dev/null
+++ b/apps/server/config.toml
@@ -0,0 +1,6 @@
+host = "127.0.0.1"
+port = 3000
+database_url = "postgresql://openconv:openconv@localhost:5432/openconv"
+max_db_connections = 5
+cors_origins = ["http://localhost:1420"]
+log_level = "debug"
diff --git a/apps/server/src/config.rs b/apps/server/src/config.rs
new file mode 100644
index 0000000..7129310
--- /dev/null
+++ b/apps/server/src/config.rs
@@ -0,0 +1,149 @@
+use serde::Deserialize;
+
+/// Server configuration loaded from config.toml with env var overrides.
+#[derive(Debug, Clone, Deserialize)]
+pub struct ServerConfig {
+    /// Host to bind to. Default: "127.0.0.1"
+    #[serde(default = "default_host")]
+    pub host: String,
+    /// Port to listen on. Default: 3000
+    #[serde(default = "default_port")]
+    pub port: u16,
+    /// PostgreSQL connection string
+    pub database_url: String,
+    /// Maximum database pool connections. Default: 5
+    #[serde(default = "default_max_db_connections")]
+    pub max_db_connections: u32,
+    /// Allowed CORS origins. Default: ["http://localhost:1420"]
+    #[serde(default = "default_cors_origins")]
+    pub cors_origins: Vec<String>,
+    /// Tracing log level. Default: "info"
+    #[serde(default = "default_log_level")]
+    pub log_level: String,
+}
+
+fn default_host() -> String {
+    "127.0.0.1".to_string()
+}
+fn default_port() -> u16 {
+    3000
+}
+fn default_max_db_connections() -> u32 {
+    5
+}
+fn default_cors_origins() -> Vec<String> {
+    vec!["http://localhost:1420".to_string()]
+}
+fn default_log_level() -> String {
+    "info".to_string()
+}
+
+impl Default for ServerConfig {
+    fn default() -> Self {
+        Self {
+            host: default_host(),
+            port: default_port(),
+            database_url: String::new(),
+            max_db_connections: default_max_db_connections(),
+            cors_origins: default_cors_origins(),
+            log_level: default_log_level(),
+        }
+    }
+}
+
+impl ServerConfig {
+    /// Load configuration from TOML file with environment variable overrides.
+    ///
+    /// Reads `config.toml` from CWD (or path in `CONFIG_PATH` env var),
+    /// then overrides individual fields from env vars.
+    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
+        let path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
+        let contents = std::fs::read_to_string(&path)?;
+        Self::from_toml_str(&contents)
+    }
+
+    /// Load configuration from a TOML string, then apply env var overrides.
+    pub fn from_toml_str(toml_str: &str) -> Result<Self, Box<dyn std::error::Error>> {
+        let mut config: ServerConfig = toml::from_str(toml_str)?;
+        config.apply_env_overrides();
+        Ok(config)
+    }
+
+    /// Apply environment variable overrides to the config.
+    pub fn apply_env_overrides(&mut self) {
+        if let Ok(val) = std::env::var("HOST") {
+            self.host = val;
+        }
+        if let Ok(val) = std::env::var("PORT") {
+            if let Ok(port) = val.parse() {
+                self.port = port;
+            }
+        }
+        if let Ok(val) = std::env::var("DATABASE_URL") {
+            self.database_url = val;
+        }
+        if let Ok(val) = std::env::var("MAX_DB_CONNECTIONS") {
+            if let Ok(max) = val.parse() {
+                self.max_db_connections = max;
+            }
+        }
+        if let Ok(val) = std::env::var("LOG_LEVEL") {
+            self.log_level = val;
+        }
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn test_config_loads_from_valid_toml_string() {
+        let toml = r#"
+            host = "0.0.0.0"
+            port = 8080
+            database_url = "postgresql://user:pass@localhost/db"
+            max_db_connections = 10
+            cors_origins = ["http://localhost:3000"]
+            log_level = "debug"
+        "#;
+        let config = ServerConfig::from_toml_str(toml).unwrap();
+        assert_eq!(config.host, "0.0.0.0");
+        assert_eq!(config.port, 8080);
+        assert_eq!(config.database_url, "postgresql://user:pass@localhost/db");
+        assert_eq!(config.max_db_connections, 10);
+        assert_eq!(config.cors_origins, vec!["http://localhost:3000"]);
+        assert_eq!(config.log_level, "debug");
+    }
+
+    #[test]
+    fn test_config_applies_env_var_overrides() {
+        let toml = r#"
+            database_url = "postgresql://original@localhost/db"
+        "#;
+        std::env::set_var("DATABASE_URL", "postgresql://overridden@localhost/db");
+        let config = ServerConfig::from_toml_str(toml).unwrap();
+        assert_eq!(config.database_url, "postgresql://overridden@localhost/db");
+        std::env::remove_var("DATABASE_URL");
+    }
+
+    #[test]
+    fn test_config_has_correct_defaults_for_omitted_fields() {
+        let toml = r#"
+            database_url = "postgresql://localhost/db"
+        "#;
+        let config = ServerConfig::from_toml_str(toml).unwrap();
+        assert_eq!(config.host, "127.0.0.1");
+        assert_eq!(config.port, 3000);
+        assert_eq!(config.max_db_connections, 5);
+        assert_eq!(config.cors_origins, vec!["http://localhost:1420"]);
+        assert_eq!(config.log_level, "info");
+    }
+
+    #[test]
+    fn test_config_fails_on_malformed_toml() {
+        let toml = "this is not valid = [[[toml";
+        let result = ServerConfig::from_toml_str(toml);
+        assert!(result.is_err());
+    }
+}
diff --git a/apps/server/src/error.rs b/apps/server/src/error.rs
new file mode 100644
index 0000000..5b7d0e3
--- /dev/null
+++ b/apps/server/src/error.rs
@@ -0,0 +1,73 @@
+use axum::http::StatusCode;
+use axum::response::{IntoResponse, Response};
+use axum::Json;
+use openconv_shared::error::OpenConvError;
+
+/// Newtype wrapper for `OpenConvError` that implements `IntoResponse`.
+///
+/// Needed because of the orphan rule — neither the trait (`IntoResponse`)
+/// nor the type (`OpenConvError`) is defined in this crate.
+pub struct ServerError(pub OpenConvError);
+
+impl IntoResponse for ServerError {
+    fn into_response(self) -> Response {
+        let (status, message) = match &self.0 {
+            OpenConvError::NotFound => (StatusCode::NOT_FOUND, self.0.to_string()),
+            OpenConvError::Unauthorized => (StatusCode::UNAUTHORIZED, self.0.to_string()),
+            OpenConvError::Forbidden => (StatusCode::FORBIDDEN, self.0.to_string()),
+            OpenConvError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
+            OpenConvError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
+        };
+        (status, Json(serde_json::json!({ "error": message }))).into_response()
+    }
+}
+
+impl From<OpenConvError> for ServerError {
+    fn from(e: OpenConvError) -> Self {
+        ServerError(e)
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn test_not_found_maps_to_404() {
+        let response = ServerError(OpenConvError::NotFound).into_response();
+        assert_eq!(response.status(), StatusCode::NOT_FOUND);
+    }
+
+    #[test]
+    fn test_unauthorized_maps_to_401() {
+        let response = ServerError(OpenConvError::Unauthorized).into_response();
+        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
+    }
+
+    #[test]
+    fn test_forbidden_maps_to_403() {
+        let response = ServerError(OpenConvError::Forbidden).into_response();
+        assert_eq!(response.status(), StatusCode::FORBIDDEN);
+    }
+
+    #[test]
+    fn test_validation_maps_to_400() {
+        let response = ServerError(OpenConvError::Validation("bad input".into())).into_response();
+        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
+    }
+
+    #[test]
+    fn test_internal_maps_to_500() {
+        let response = ServerError(OpenConvError::Internal("something broke".into())).into_response();
+        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
+    }
+
+    #[tokio::test]
+    async fn test_error_responses_are_json_with_error_field() {
+        let response = ServerError(OpenConvError::NotFound).into_response();
+        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
+        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
+        assert!(json.get("error").is_some());
+        assert_eq!(json["error"], "not found");
+    }
+}
diff --git a/apps/server/src/handlers/health.rs b/apps/server/src/handlers/health.rs
new file mode 100644
index 0000000..6099f6c
--- /dev/null
+++ b/apps/server/src/handlers/health.rs
@@ -0,0 +1,25 @@
+use axum::extract::State;
+use axum::http::StatusCode;
+use axum::response::IntoResponse;
+use axum::Json;
+
+use crate::state::AppState;
+
+/// GET /health/live — returns 200 unconditionally.
+/// Used by load balancers to check if the process is alive.
+pub async fn liveness() -> impl IntoResponse {
+    Json(serde_json::json!({ "status": "ok" }))
+}
+
+/// GET /health/ready — queries the database to verify connectivity.
+/// Returns 200 on success, 503 on failure.
+pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
+    match sqlx::query("SELECT 1").execute(&state.db).await {
+        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
+        Err(_) => (
+            StatusCode::SERVICE_UNAVAILABLE,
+            Json(serde_json::json!({ "status": "unavailable" })),
+        )
+            .into_response(),
+    }
+}
diff --git a/apps/server/src/handlers/mod.rs b/apps/server/src/handlers/mod.rs
new file mode 100644
index 0000000..43a7c76
--- /dev/null
+++ b/apps/server/src/handlers/mod.rs
@@ -0,0 +1 @@
+pub mod health;
diff --git a/apps/server/src/lib.rs b/apps/server/src/lib.rs
new file mode 100644
index 0000000..9e3b501
--- /dev/null
+++ b/apps/server/src/lib.rs
@@ -0,0 +1,6 @@
+pub mod config;
+pub mod error;
+pub mod handlers;
+pub mod router;
+pub mod shutdown;
+pub mod state;
diff --git a/apps/server/src/main.rs b/apps/server/src/main.rs
index 9173575..cc5c0b7 100644
--- a/apps/server/src/main.rs
+++ b/apps/server/src/main.rs
@@ -1,5 +1,44 @@
-//! OpenConv Axum server entry point.
+use std::sync::Arc;
 
-fn main() {
-    println!("openconv-server placeholder");
+use tracing_subscriber::EnvFilter;
+
+use openconv_server::config::ServerConfig;
+use openconv_server::router::build_router;
+use openconv_server::shutdown::shutdown_signal;
+use openconv_server::state::AppState;
+
+#[tokio::main]
+async fn main() -> Result<(), Box<dyn std::error::Error>> {
+    dotenvy::dotenv().ok();
+
+    let config = ServerConfig::load()?;
+
+    tracing_subscriber::fmt()
+        .with_env_filter(
+            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
+        )
+        .init();
+
+    let pool = sqlx::postgres::PgPoolOptions::new()
+        .max_connections(config.max_db_connections)
+        .connect(&config.database_url)
+        .await?;
+
+    sqlx::migrate!().run(&pool).await?;
+
+    let addr = format!("{}:{}", config.host, config.port);
+    let state = AppState {
+        db: pool,
+        config: Arc::new(config),
+    };
+    let app = build_router(state);
+
+    let listener = tokio::net::TcpListener::bind(&addr).await?;
+    tracing::info!("Server listening on {addr}");
+
+    axum::serve(listener, app)
+        .with_graceful_shutdown(shutdown_signal())
+        .await?;
+
+    Ok(())
 }
diff --git a/apps/server/src/router.rs b/apps/server/src/router.rs
new file mode 100644
index 0000000..b0bc6cc
--- /dev/null
+++ b/apps/server/src/router.rs
@@ -0,0 +1,55 @@
+use axum::http::HeaderValue;
+use axum::middleware;
+use axum::routing::get;
+use tower_http::cors::{AllowOrigin, CorsLayer};
+use axum::extract::DefaultBodyLimit;
+use tower_http::trace::TraceLayer;
+
+use crate::handlers;
+use crate::state::AppState;
+
+/// Builds the application router with all middleware and routes.
+pub fn build_router(state: AppState) -> axum::Router {
+    let origins: Vec<HeaderValue> = state
+        .config
+        .cors_origins
+        .iter()
+        .filter_map(|o| o.parse().ok())
+        .collect();
+
+    let cors = CorsLayer::new()
+        .allow_origin(AllowOrigin::list(origins))
+        .allow_methods([
+            axum::http::Method::GET,
+            axum::http::Method::POST,
+            axum::http::Method::PUT,
+            axum::http::Method::DELETE,
+            axum::http::Method::OPTIONS,
+        ])
+        .allow_headers([
+            axum::http::header::CONTENT_TYPE,
+            axum::http::header::AUTHORIZATION,
+        ]);
+
+    axum::Router::new()
+        .route("/health/live", get(handlers::health::liveness))
+        .route("/health/ready", get(handlers::health::readiness))
+        .layer(middleware::from_fn(request_id_middleware))
+        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
+        .layer(cors)
+        .layer(TraceLayer::new_for_http())
+        .with_state(state)
+}
+
+async fn request_id_middleware(
+    request: axum::extract::Request,
+    next: middleware::Next,
+) -> axum::response::Response {
+    let request_id = uuid::Uuid::new_v4().to_string();
+    let mut response = next.run(request).await;
+    response.headers_mut().insert(
+        "x-request-id",
+        HeaderValue::from_str(&request_id).unwrap(),
+    );
+    response
+}
diff --git a/apps/server/src/shutdown.rs b/apps/server/src/shutdown.rs
new file mode 100644
index 0000000..d2ae49b
--- /dev/null
+++ b/apps/server/src/shutdown.rs
@@ -0,0 +1,30 @@
+/// Returns a future that resolves when a shutdown signal is received.
+///
+/// Listens for Ctrl+C on all platforms. On Unix, also listens for SIGTERM.
+pub async fn shutdown_signal() {
+    let ctrl_c = async {
+        tokio::signal::ctrl_c()
+            .await
+            .expect("failed to install Ctrl+C handler");
+    };
+
+    #[cfg(unix)]
+    let terminate = async {
+        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
+            .expect("failed to install SIGTERM handler")
+            .recv()
+            .await;
+    };
+
+    #[cfg(not(unix))]
+    let terminate = std::future::pending::<()>();
+
+    tokio::select! {
+        _ = ctrl_c => {
+            tracing::info!("Received Ctrl+C, starting graceful shutdown");
+        }
+        _ = terminate => {
+            tracing::info!("Received SIGTERM, starting graceful shutdown");
+        }
+    }
+}
diff --git a/apps/server/src/state.rs b/apps/server/src/state.rs
new file mode 100644
index 0000000..6d5713a
--- /dev/null
+++ b/apps/server/src/state.rs
@@ -0,0 +1,24 @@
+use std::sync::Arc;
+
+use crate::config::ServerConfig;
+
+/// Shared application state passed to all handlers via Axum's State extractor.
+///
+/// `PgPool` is internally Arc-wrapped. `ServerConfig` is wrapped in `Arc`
+/// so cloning `AppState` is cheap.
+#[derive(Clone)]
+pub struct AppState {
+    pub db: sqlx::PgPool,
+    pub config: Arc<ServerConfig>,
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn test_app_state_implements_clone() {
+        fn assert_clone<T: Clone>() {}
+        assert_clone::<AppState>();
+    }
+}
diff --git a/apps/server/tests/health.rs b/apps/server/tests/health.rs
new file mode 100644
index 0000000..8629004
--- /dev/null
+++ b/apps/server/tests/health.rs
@@ -0,0 +1,99 @@
+use axum::body::Body;
+use axum::http::{Request, StatusCode};
+use tower::ServiceExt; // for `oneshot`
+
+use openconv_server::config::ServerConfig;
+use openconv_server::router::build_router;
+use openconv_server::state::AppState;
+
+/// Helper to build a test router without a real database.
+/// Uses a pool that points to an invalid URL — fine for liveness checks
+/// and middleware tests that don't hit the DB.
+fn test_app() -> axum::Router {
+    // Create a PgPool with a dummy URL — it won't actually connect
+    // since liveness doesn't query the DB.
+    let pool = sqlx::PgPool::connect_lazy("postgresql://fake@localhost/fake").unwrap();
+    let config = ServerConfig {
+        database_url: "postgresql://fake@localhost/fake".to_string(),
+        ..Default::default()
+    };
+    let state = AppState {
+        db: pool,
+        config: std::sync::Arc::new(config),
+    };
+    build_router(state)
+}
+
+#[tokio::test]
+async fn test_health_live_returns_200_with_status_ok() {
+    let app = test_app();
+    let request = Request::builder()
+        .uri("/health/live")
+        .body(Body::empty())
+        .unwrap();
+
+    let response = app.oneshot(request).await.unwrap();
+    assert_eq!(response.status(), StatusCode::OK);
+
+    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
+    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
+    assert_eq!(json["status"], "ok");
+}
+
+#[tokio::test]
+async fn test_health_ready_returns_503_when_db_unreachable() {
+    let app = test_app();
+    let request = Request::builder()
+        .uri("/health/ready")
+        .body(Body::empty())
+        .unwrap();
+
+    let response = app.oneshot(request).await.unwrap();
+    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
+}
+
+#[tokio::test]
+async fn test_requests_include_x_request_id_header() {
+    let app = test_app();
+    let request = Request::builder()
+        .uri("/health/live")
+        .body(Body::empty())
+        .unwrap();
+
+    let response = app.oneshot(request).await.unwrap();
+    let request_id = response.headers().get("x-request-id");
+    assert!(request_id.is_some(), "Response should include x-request-id header");
+    // Verify the value is a valid UUID
+    let id_str = request_id.unwrap().to_str().unwrap();
+    uuid::Uuid::parse_str(id_str).expect("x-request-id should be a valid UUID");
+}
+
+#[tokio::test]
+async fn test_cors_headers_present() {
+    let app = test_app();
+    let request = Request::builder()
+        .method("OPTIONS")
+        .uri("/health/live")
+        .header("Origin", "http://localhost:1420")
+        .header("Access-Control-Request-Method", "GET")
+        .body(Body::empty())
+        .unwrap();
+
+    let response = app.oneshot(request).await.unwrap();
+    assert!(
+        response.headers().get("access-control-allow-origin").is_some(),
+        "Response should include Access-Control-Allow-Origin header"
+    );
+}
+
+#[tokio::test]
+async fn test_unknown_routes_return_404() {
+    let app = test_app();
+    let request = Request::builder()
+        .uri("/nonexistent")
+        .body(Body::empty())
+        .unwrap();
+
+    let response = app.oneshot(request).await.unwrap();
+    assert_eq!(response.status(), StatusCode::NOT_FOUND);
+}
