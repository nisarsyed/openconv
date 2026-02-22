use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use tower::{Layer, Service};

use crate::jwt::JwtService;

/// Tower layer that applies per-IP rate limiting using Redis.
#[derive(Clone)]
pub struct RateLimitLayer {
    redis: fred::clients::Pool,
    max_requests: u32,
    window_seconds: u64,
    endpoint_prefix: String,
}

impl RateLimitLayer {
    pub fn new(
        redis: fred::clients::Pool,
        max_requests: u32,
        window_seconds: u64,
        endpoint_prefix: String,
    ) -> Self {
        Self {
            redis,
            max_requests,
            window_seconds,
            endpoint_prefix,
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            redis: self.redis.clone(),
            max_requests: self.max_requests,
            window_seconds: self.window_seconds,
            endpoint_prefix: self.endpoint_prefix.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    redis: fred::clients::Pool,
    max_requests: u32,
    window_seconds: u64,
    endpoint_prefix: String,
}

pub struct RateLimitError {
    pub retry_after_seconds: u64,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        let mut response = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "rate limit exceeded" })),
        )
            .into_response();
        response.headers_mut().insert(
            "Retry-After",
            self.retry_after_seconds
                .to_string()
                .parse()
                .expect("valid header value"),
        );
        response
    }
}

fn extract_client_ip<B>(req: &Request<B>) -> String {
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<std::net::SocketAddr>>() {
        return addr.ip().to_string();
    }
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(first) = s.split(',').next() {
                return first.trim().to_string();
            }
        }
    }
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(s) = real_ip.to_str() {
            return s.trim().to_string();
        }
    }
    "unknown".to_string()
}

/// Lua script that atomically increments a key and sets expiration.
/// Returns the new count. Sets EXPIRE only on the first increment (count == 1).
const RATE_LIMIT_SCRIPT: &str = r#"
local count = redis.call('INCR', KEYS[1])
if count == 1 then
    redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return count
"#;

/// Check rate limit against Redis. Returns Ok(()) if within limit,
/// Err(retry_after_seconds) if exceeded. Fails open on Redis errors.
async fn check_redis_rate_limit(
    redis: &fred::clients::Pool,
    key: &str,
    max_requests: u32,
    window_seconds: u64,
) -> Result<(), u64> {
    use fred::interfaces::{ClientLike, KeysInterface, LuaInterface};

    // Fail open if pool is not connected
    if !redis.is_connected() {
        tracing::warn!(key, "rate limiter: Redis not connected, failing open");
        return Ok(());
    }

    // Atomic INCR + conditional EXPIRE via Lua script
    let count: i64 = match redis
        .eval(
            RATE_LIMIT_SCRIPT,
            vec![key.to_string()],
            vec![window_seconds.to_string()],
        )
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, key, "rate limiter: Redis eval failed, failing open");
            return Ok(());
        }
    };

    if count > max_requests as i64 {
        let ttl: i64 = redis.ttl(key).await.unwrap_or(window_seconds as i64);
        return Err(ttl.max(1) as u64);
    }

    Ok(())
}

impl<S> Service<Request<Body>> for RateLimitService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let redis = self.redis.clone();
        let max = self.max_requests;
        let window = self.window_seconds;
        let prefix = self.endpoint_prefix.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let ip = extract_client_ip(&req);
            let key = format!("rl:ip:{ip}:{prefix}");

            match check_redis_rate_limit(&redis, &key, max, window).await {
                Ok(()) => inner.call(req).await,
                Err(retry_after) => Ok(RateLimitError {
                    retry_after_seconds: retry_after,
                }
                .into_response()),
            }
        })
    }
}

/// Check per-public-key rate limit. Returns Ok(()) if within limit,
/// Err(RateLimitError) if exceeded.
pub async fn check_key_rate_limit(
    redis: &fred::clients::Pool,
    public_key: &str,
    endpoint: &str,
    max_requests: u32,
    window_seconds: u64,
) -> Result<(), RateLimitError> {
    let key = format!("rl:pk:{public_key}:{endpoint}");
    check_redis_rate_limit(redis, &key, max_requests, window_seconds)
        .await
        .map_err(|retry_after| RateLimitError {
            retry_after_seconds: retry_after,
        })
}

/// Check per-email rate limit. Returns Ok(()) if within limit,
/// Err(RateLimitError) if exceeded.
pub async fn check_email_rate_limit(
    redis: &fred::clients::Pool,
    email: &str,
    max_requests: u32,
    window_seconds: u64,
) -> Result<(), RateLimitError> {
    let key = format!("rl:email:{email}");
    check_redis_rate_limit(redis, &key, max_requests, window_seconds)
        .await
        .map_err(|retry_after| RateLimitError {
            retry_after_seconds: retry_after,
        })
}

// ---------------------------------------------------------------------------
// Per-user rate limiter (authenticated endpoints)
// ---------------------------------------------------------------------------

/// Tower layer that applies per-user rate limiting using Redis.
/// Falls back to per-IP rate limiting if no valid JWT is present.
#[derive(Clone)]
pub struct UserRateLimitLayer {
    redis: fred::clients::Pool,
    jwt: Arc<JwtService>,
    max_requests: u32,
    window_seconds: u64,
    endpoint_prefix: String,
}

impl UserRateLimitLayer {
    pub fn new(
        redis: fred::clients::Pool,
        jwt: Arc<JwtService>,
        max_requests: u32,
        window_seconds: u64,
        endpoint_prefix: String,
    ) -> Self {
        Self {
            redis,
            jwt,
            max_requests,
            window_seconds,
            endpoint_prefix,
        }
    }
}

impl<S> Layer<S> for UserRateLimitLayer {
    type Service = UserRateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        UserRateLimitService {
            inner,
            redis: self.redis.clone(),
            jwt: self.jwt.clone(),
            max_requests: self.max_requests,
            window_seconds: self.window_seconds,
            endpoint_prefix: self.endpoint_prefix.clone(),
        }
    }
}

#[derive(Clone)]
pub struct UserRateLimitService<S> {
    inner: S,
    redis: fred::clients::Pool,
    jwt: Arc<JwtService>,
    max_requests: u32,
    window_seconds: u64,
    endpoint_prefix: String,
}

/// Extract user ID from the Authorization Bearer token without performing
/// full authentication. Returns None if the header is missing, malformed,
/// or the JWT is invalid.
fn extract_user_id_from_jwt<B>(req: &Request<B>, jwt: &JwtService) -> Option<String> {
    let auth_header = req.headers().get("authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let claims = jwt.validate_access_token(token).ok()?;
    Some(claims.sub)
}

impl<S> Service<Request<Body>> for UserRateLimitService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let redis = self.redis.clone();
        let jwt = self.jwt.clone();
        let max = self.max_requests;
        let window = self.window_seconds;
        let prefix = self.endpoint_prefix.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let key = if let Some(user_id) = extract_user_id_from_jwt(&req, &jwt) {
                format!("rl:user:{user_id}:{prefix}")
            } else {
                let ip = extract_client_ip(&req);
                format!("rl:ip:{ip}:{prefix}")
            };

            match check_redis_rate_limit(&redis, &key, max, window).await {
                Ok(()) => inner.call(req).await,
                Err(retry_after) => Ok(RateLimitError {
                    retry_after_seconds: retry_after,
                }
                .into_response()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn get_test_redis() -> Option<fred::clients::Pool> {
        use fred::interfaces::ClientLike;
        let config = fred::types::config::Config::from_url("redis://localhost:6379").ok()?;
        let pool = fred::clients::Pool::new(config, None, None, None, 1).ok()?;
        let _ = pool.init().await.ok()?;
        pool.wait_for_connect().await.ok()?;
        Some(pool)
    }

    async fn cleanup_redis_key(redis: &fred::clients::Pool, key: &str) {
        use fred::interfaces::KeysInterface;
        let _: i64 = redis.del(key).await.unwrap_or_default();
    }

    fn test_app(redis: fred::clients::Pool, max_requests: u32, window_seconds: u64) -> Router {
        let handler = || async { "ok" };
        Router::new()
            .route("/test", get(handler))
            .layer(RateLimitLayer::new(
                redis,
                max_requests,
                window_seconds,
                "test".to_string(),
            ))
    }

    #[tokio::test]
    async fn first_request_within_limit_returns_200() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        cleanup_redis_key(&redis, "rl:ip:10.0.0.1:test").await;

        let app = test_app(redis.clone(), 10, 60);
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "10.0.0.1")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        cleanup_redis_key(&redis, "rl:ip:10.0.0.1:test").await;
    }

    #[tokio::test]
    async fn requests_exceeding_limit_return_429_with_retry_after() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        cleanup_redis_key(&redis, "rl:ip:10.0.0.2:test").await;

        let max_requests = 3u32;

        // Send requests up to the limit
        for _ in 0..max_requests {
            let request = Request::builder()
                .uri("/test")
                .header("X-Forwarded-For", "10.0.0.2")
                .body(Body::empty())
                .unwrap();
            let app_clone = test_app(redis.clone(), max_requests, 60);
            let response = app_clone.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // One more should be rejected
        let app = test_app(redis.clone(), max_requests, 60);
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "10.0.0.2")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key("Retry-After"));
        let retry_after: u64 = response
            .headers()
            .get("Retry-After")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(retry_after > 0);

        cleanup_redis_key(&redis, "rl:ip:10.0.0.2:test").await;
    }

    #[tokio::test]
    async fn rate_limit_counters_expire_after_window() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        cleanup_redis_key(&redis, "rl:ip:10.0.0.3:test").await;

        let max_requests = 2u32;
        let window_seconds = 1u64; // 1-second window for fast test

        // Fill up the limit
        for _ in 0..max_requests {
            let app = test_app(redis.clone(), max_requests, window_seconds);
            let request = Request::builder()
                .uri("/test")
                .header("X-Forwarded-For", "10.0.0.3")
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // Wait for window to expire
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Should be allowed again
        let app = test_app(redis.clone(), max_requests, window_seconds);
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "10.0.0.3")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        cleanup_redis_key(&redis, "rl:ip:10.0.0.3:test").await;
    }

    #[tokio::test]
    async fn different_ips_have_independent_counters() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        cleanup_redis_key(&redis, "rl:ip:10.0.0.4:test").await;
        cleanup_redis_key(&redis, "rl:ip:10.0.0.5:test").await;

        let max_requests = 2u32;

        // Fill up limit for IP 10.0.0.4
        for _ in 0..max_requests {
            let app = test_app(redis.clone(), max_requests, 60);
            let request = Request::builder()
                .uri("/test")
                .header("X-Forwarded-For", "10.0.0.4")
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // IP 10.0.0.5 should still be allowed
        let app = test_app(redis.clone(), max_requests, 60);
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "10.0.0.5")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        cleanup_redis_key(&redis, "rl:ip:10.0.0.4:test").await;
        cleanup_redis_key(&redis, "rl:ip:10.0.0.5:test").await;
    }

    #[tokio::test]
    async fn rate_limit_fails_open_when_redis_unavailable() {
        // Create a pool pointing to a non-existent Redis
        let config = fred::types::config::Config::from_url("redis://localhost:59999").unwrap();
        let pool = fred::clients::Pool::new(config, None, None, None, 1).unwrap();
        // Don't init -- pool is not connected

        let app = test_app(pool, 1, 60);
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "10.0.0.99")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        // Should fail open and return 200
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- UserRateLimitLayer tests ---

    fn test_jwt_service() -> Arc<crate::jwt::JwtService> {
        let config = crate::config::JwtConfig {
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIONXw0UoRsRapn/WATSl25Hsej6hTuwsf+olF9npjjSs\n-----END PRIVATE KEY-----".to_string(),
            public_key_pem: "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA9eB0735gPgffPc6aheXCqzsXb4ylG7Yi6I0yUIb2vZ4=\n-----END PUBLIC KEY-----".to_string(),
            access_token_ttl_seconds: 300,
            refresh_token_ttl_seconds: 604800,
        };
        Arc::new(crate::jwt::JwtService::new(&config).unwrap())
    }

    fn user_test_app(
        redis: fred::clients::Pool,
        jwt: Arc<crate::jwt::JwtService>,
        max_requests: u32,
        window_seconds: u64,
    ) -> Router {
        let handler = || async { "ok" };
        Router::new()
            .route("/test", get(handler))
            .layer(UserRateLimitLayer::new(
                redis,
                jwt,
                max_requests,
                window_seconds,
                "utest".to_string(),
            ))
    }

    #[tokio::test]
    async fn user_rate_limit_uses_user_key_with_valid_jwt() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        let jwt = test_jwt_service();
        let user_id = openconv_shared::ids::UserId::new();
        let device_id = openconv_shared::ids::DeviceId::new();
        let token = jwt.issue_access_token(&user_id, &device_id).unwrap();

        let key = format!("rl:user:{user_id}:utest");
        cleanup_redis_key(&redis, &key).await;

        let app = user_test_app(redis.clone(), jwt, 10, 60);
        let request = Request::builder()
            .uri("/test")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify the user key was incremented (not the IP key)
        use fred::interfaces::KeysInterface;
        let exists: bool = redis.exists(&key).await.unwrap();
        assert!(exists, "expected user rate limit key to exist");

        cleanup_redis_key(&redis, &key).await;
    }

    #[tokio::test]
    async fn user_rate_limit_falls_back_to_ip_without_jwt() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        let jwt = test_jwt_service();
        let ip_key = "rl:ip:10.0.0.50:utest";
        cleanup_redis_key(&redis, ip_key).await;

        let app = user_test_app(redis.clone(), jwt, 10, 60);
        let request = Request::builder()
            .uri("/test")
            .header("X-Forwarded-For", "10.0.0.50")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify the IP key was used
        use fred::interfaces::KeysInterface;
        let exists: bool = redis.exists(ip_key).await.unwrap();
        assert!(exists, "expected IP rate limit key to exist as fallback");

        cleanup_redis_key(&redis, ip_key).await;
    }

    #[tokio::test]
    async fn user_rate_limit_returns_429_when_exceeded() {
        let Some(redis) = get_test_redis().await else {
            eprintln!("skipping: Redis not available");
            return;
        };
        let jwt = test_jwt_service();
        let user_id = openconv_shared::ids::UserId::new();
        let device_id = openconv_shared::ids::DeviceId::new();
        let token = jwt.issue_access_token(&user_id, &device_id).unwrap();

        let key = format!("rl:user:{user_id}:utest");
        cleanup_redis_key(&redis, &key).await;

        let max_requests = 2u32;

        // Fill up the limit
        for _ in 0..max_requests {
            let app = user_test_app(redis.clone(), jwt.clone(), max_requests, 60);
            let request = Request::builder()
                .uri("/test")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        // One more should be rejected
        let app = user_test_app(redis.clone(), jwt.clone(), max_requests, 60);
        let request = Request::builder()
            .uri("/test")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key("Retry-After"));

        cleanup_redis_key(&redis, &key).await;
    }
}
