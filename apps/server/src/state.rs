use std::sync::Arc;

use crate::config::ServerConfig;
use crate::email::EmailService;
use crate::jwt::JwtService;

/// Shared application state passed to all handlers via Axum's State extractor.
///
/// `PgPool` is internally Arc-wrapped. `ServerConfig` is wrapped in `Arc`
/// so cloning `AppState` is cheap.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: Arc<ServerConfig>,
    pub redis: fred::clients::Pool,
    pub jwt: Arc<JwtService>,
    pub email: Arc<dyn EmailService>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_implements_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<AppState>();
    }
}
