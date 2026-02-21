use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use fred::interfaces::KeysInterface;
use openconv_shared::api::auth::{
    DeviceInfo, DevicesListResponse, LoginChallengeRequest, LoginChallengeResponse,
    LoginVerifyRequest, LoginVerifyResponse, RecoverCompleteRequest, RecoverCompleteResponse,
    RecoverStartRequest, RecoverStartResponse, RecoverVerifyRequest, RecoverVerifyResponse,
    RefreshRequest, RefreshResponse, RegisterCompleteRequest, RegisterResponse,
    RegisterStartRequest, RegisterStartResponse, RegisterVerifyRequest, RegisterVerifyResponse,
};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{DeviceId, UserId};
use rand::Rng;
use subtle::ConstantTimeEq;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use crate::validation::validate_display_name;

fn validate_email(email: &str) -> Result<(), ServerError> {
    let email = email.trim();
    if email.is_empty() {
        return Err(OpenConvError::Validation("email is required".into()).into());
    }
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(OpenConvError::Validation("invalid email format".into()).into());
    }
    if !parts[1].contains('.') {
        return Err(OpenConvError::Validation("invalid email format".into()).into());
    }
    Ok(())
}

fn validate_verification_code(code: &str) -> Result<(), ServerError> {
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(OpenConvError::Validation("invalid code".into()).into());
    }
    Ok(())
}

/// Redis storage format for verification codes.
#[derive(serde::Serialize, serde::Deserialize)]
struct VerificationData {
    code: String,
    display_name: String,
    attempts_remaining: u32,
}

/// Lua script for atomic verification code check.
/// Returns: [result_code, display_name_or_empty]
///   result_code:
///     1  = code matched, key deleted
///     0  = code mismatch, attempts decremented
///     -1 = key not found / expired
///     -2 = attempts exhausted, key deleted
const VERIFY_CODE_SCRIPT: &str = r#"
local key = KEYS[1]
local submitted_code = ARGV[1]

local data = redis.call('GET', key)
if not data then
    return {-1, ""}
end

local decoded = cjson.decode(data)
local attempts = tonumber(decoded.attempts_remaining)

if attempts <= 0 then
    redis.call('DEL', key)
    return {-2, ""}
end

if submitted_code == decoded.code then
    redis.call('DEL', key)
    return {1, decoded.display_name}
end

decoded.attempts_remaining = attempts - 1
local ttl = redis.call('TTL', key)
if ttl > 0 then
    redis.call('SET', key, cjson.encode(decoded), 'EX', ttl)
end

return {0, ""}
"#;

pub async fn register_start(
    State(state): State<AppState>,
    Json(req): Json<RegisterStartRequest>,
) -> Result<Json<RegisterStartResponse>, ServerError> {
    validate_email(&req.email)?;
    let display_name = validate_display_name(&req.display_name)?;

    let email = req.email.trim().to_lowercase();

    // Per-email rate limiting
    crate::middleware::rate_limit::check_email_rate_limit(
        &state.redis,
        &email,
        state.config.rate_limit.email_per_address_per_hour,
        3600,
    )
    .await
    .map_err(|_| OpenConvError::RateLimited)?;

    // Check if email already exists — always return the same response (privacy-first)
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&state.db)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    if !exists {
        let code = format!("{:06}", rand::rng().random_range(0..1_000_000u32));

        let data = VerificationData {
            code: code.clone(),
            display_name,
            attempts_remaining: 5,
        };
        let json_data = serde_json::to_string(&data)
            .map_err(|e| OpenConvError::Internal(format!("serialization error: {e}")))?;

        let key = format!("verify:{email}");
        state
            .redis
            .set::<(), _, _>(
                &key,
                json_data.as_str(),
                Some(fred::types::Expiration::EX(600)),
                None,
                false,
            )
            .await
            .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

        if let Err(e) = state.email.send_verification_code(&email, &code).await {
            tracing::error!(error = %e, "failed to send verification email");
        }
    }

    Ok(Json(RegisterStartResponse {
        message: "Verification code sent".into(),
    }))
}

pub async fn register_verify(
    State(state): State<AppState>,
    Json(req): Json<RegisterVerifyRequest>,
) -> Result<Json<RegisterVerifyResponse>, ServerError> {
    validate_email(&req.email)?;
    validate_verification_code(&req.code)?;

    let email = req.email.trim().to_lowercase();
    let key = format!("verify:{email}");

    // Atomic verification via Lua script
    use fred::interfaces::LuaInterface;
    let result: Vec<fred::types::Value> = state
        .redis
        .eval(VERIFY_CODE_SCRIPT, vec![key], vec![req.code.clone()])
        .await
        .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

    if result.len() < 2 {
        return Err(OpenConvError::Internal("unexpected redis response".into()).into());
    }

    let result_code: i64 = match &result[0] {
        fred::types::Value::Integer(n) => *n,
        _ => return Err(OpenConvError::Internal("unexpected redis response type".into()).into()),
    };

    match result_code {
        1 => {
            // Code matched — extract display_name from Lua response
            let display_name = match &result[1] {
                fred::types::Value::String(s) => s.to_string(),
                fred::types::Value::Bytes(b) => String::from_utf8_lossy(b).to_string(),
                _ => String::new(),
            };

            let token = state.jwt.issue_registration_token(&email, &display_name)?;

            Ok(Json(RegisterVerifyResponse {
                registration_token: token,
            }))
        }
        0 => Err(OpenConvError::Validation("invalid code".into()).into()),
        -1 => Err(OpenConvError::Validation("code expired or not found".into()).into()),
        -2 => Err(OpenConvError::Validation("code expired, request a new one".into()).into()),
        _ => Err(OpenConvError::Internal("unexpected verification result".into()).into()),
    }
}

pub async fn register_complete(
    State(state): State<AppState>,
    Json(req): Json<RegisterCompleteRequest>,
) -> Result<Json<RegisterResponse>, ServerError> {
    // 1. Validate registration token
    let claims = state
        .jwt
        .validate_registration_token(&req.registration_token)?;

    // 2. Validate public key
    let pk_bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.public_key)
        .map_err(|_| OpenConvError::Validation("invalid public key encoding".into()))?;

    if pk_bytes.len() != 33 {
        return Err(
            OpenConvError::Validation("public key must be 33 bytes when decoded".into()).into(),
        );
    }

    libsignal_protocol::PublicKey::deserialize(&pk_bytes)
        .map_err(|_| OpenConvError::Validation("invalid public key format".into()))?;

    // Decode pre-key bundle
    let pre_key_data = base64::engine::general_purpose::STANDARD
        .decode(&req.pre_key_bundle)
        .map_err(|_| OpenConvError::Validation("invalid pre-key bundle encoding".into()))?;

    // 3. Begin transaction
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction start failed: {e}")))?;

    let user_id = UserId::new();
    let pre_key_id = uuid::Uuid::now_v7();

    // Insert user
    let insert_result = sqlx::query(
        "INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id.0)
    .bind(&req.public_key)
    .bind(&claims.email)
    .bind(&claims.display_name)
    .execute(&mut *tx)
    .await;

    if let Err(e) = insert_result {
        if let Some(db_err) = e.as_database_error() {
            if db_err.is_unique_violation() {
                return Err(OpenConvError::Conflict("account already exists".into()).into());
            }
        }
        return Err(OpenConvError::Internal(format!("database error: {e}")).into());
    }

    // Insert device
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(req.device_id.0)
    .bind(user_id.0)
    .bind(&req.device_name)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // Insert pre-key bundle
    sqlx::query(
        "INSERT INTO pre_key_bundles (id, user_id, device_id, key_data, is_used) VALUES ($1, $2, $3, $4, false)",
    )
    .bind(pre_key_id)
    .bind(user_id.0)
    .bind(req.device_id.0)
    .bind(&pre_key_data)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // 4. Generate token family and issue tokens
    let family = uuid::Uuid::now_v7().to_string();
    let access_token = state.jwt.issue_access_token(&user_id, &req.device_id)?;
    let (refresh_token, jti_str) =
        state
            .jwt
            .issue_refresh_token(&user_id, &req.device_id, &family)?;

    let jti: uuid::Uuid = jti_str
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid jti format".into()))?;
    let family_uuid: uuid::Uuid = family
        .parse()
        .expect("family UUID was just generated from Uuid::now_v7()");
    let expires_at = chrono::Utc::now() + state.jwt.refresh_ttl();

    // Store refresh token record
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(jti)
    .bind(user_id.0)
    .bind(req.device_id.0)
    .bind(family_uuid)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // Commit transaction
    tx.commit()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction commit failed: {e}")))?;

    Ok(Json(RegisterResponse {
        user_id,
        access_token,
        refresh_token,
        device_id: req.device_id,
    }))
}

/// Redis storage format for login challenges.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredChallenge {
    challenge: String,
    exists: bool,
}

pub async fn challenge(
    State(state): State<AppState>,
    Json(req): Json<LoginChallengeRequest>,
) -> Result<Json<LoginChallengeResponse>, ServerError> {
    // Validate public key format before using as Redis key
    let pk_bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.public_key)
        .map_err(|_| OpenConvError::Validation("invalid public key encoding".into()))?;
    if pk_bytes.len() != 33 {
        return Err(
            OpenConvError::Validation("public key must be 33 bytes when decoded".into()).into(),
        );
    }

    // Per-public-key rate limiting
    crate::middleware::rate_limit::check_key_rate_limit(
        &state.redis,
        &req.public_key,
        "challenge",
        state.config.rate_limit.challenge_per_key_per_minute,
        60,
    )
    .await
    .map_err(|_| OpenConvError::RateLimited)?;

    // Generate 32 bytes of cryptographic randomness
    let challenge_bytes: [u8; 32] = rand::rng().random();
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);

    // Check if user exists — always return a challenge regardless (privacy-first)
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE public_key = $1)")
            .bind(&req.public_key)
            .fetch_one(&state.db)
            .await
            .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // Store challenge in Redis with 60s TTL
    let stored = StoredChallenge {
        challenge: challenge_b64.clone(),
        exists,
    };
    let json_data = serde_json::to_string(&stored)
        .map_err(|e| OpenConvError::Internal(format!("serialization error: {e}")))?;

    let key = format!("challenge:{}", req.public_key);
    state
        .redis
        .set::<(), _, _>(
            &key,
            json_data.as_str(),
            Some(fred::types::Expiration::EX(60)),
            None,
            false,
        )
        .await
        .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

    Ok(Json(LoginChallengeResponse {
        challenge: challenge_b64,
    }))
}

pub async fn login_verify(
    State(state): State<AppState>,
    Json(req): Json<LoginVerifyRequest>,
) -> Result<Json<LoginVerifyResponse>, ServerError> {
    // 1. Atomic fetch-and-delete challenge from Redis
    let key = format!("challenge:{}", req.public_key);
    let stored_json: Option<String> = state
        .redis
        .getdel(&key)
        .await
        .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

    let stored_json = stored_json.ok_or(OpenConvError::Unauthorized)?;
    let stored: StoredChallenge = serde_json::from_str(&stored_json)
        .map_err(|_| OpenConvError::Internal("corrupt challenge data".into()))?;

    // 2. Check exists flag — blind challenge means user doesn't exist
    if !stored.exists {
        return Err(OpenConvError::Unauthorized.into());
    }

    // 3. Parse public key using shared crypto_verify module
    let public_key = crate::crypto_verify::parse_public_key(&req.public_key)
        .map_err(|_| OpenConvError::Unauthorized)?;

    // 4. Decode signature
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.signature)
        .map_err(|_| OpenConvError::Unauthorized)?;

    // 5. Decode challenge from stored data
    let challenge_bytes = base64::engine::general_purpose::STANDARD
        .decode(&stored.challenge)
        .map_err(|_| OpenConvError::Internal("corrupt challenge data".into()))?;

    // 6. Verify signature using shared crypto_verify module
    if !crate::crypto_verify::verify_challenge_signature(&public_key, &challenge_bytes, &sig_bytes)
    {
        return Err(OpenConvError::Unauthorized.into());
    }

    // 7. Look up user by public_key
    let user_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE public_key = $1")
        .bind(&req.public_key)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?
        .ok_or(OpenConvError::Unauthorized)?;

    let user_id = UserId(user_id);

    // 8. Begin transaction for device upsert + refresh token storage
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction start failed: {e}")))?;

    // Upsert device record — scoped to current user via WHERE clause
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) \
         VALUES ($1, $2, $3, NOW(), NOW()) \
         ON CONFLICT (id) DO UPDATE SET last_active = NOW(), device_name = EXCLUDED.device_name \
         WHERE devices.user_id = $2",
    )
    .bind(req.device_id.0)
    .bind(user_id.0)
    .bind(&req.device_name)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // 9. Issue tokens
    let family = uuid::Uuid::now_v7().to_string();
    let access_token = state.jwt.issue_access_token(&user_id, &req.device_id)?;
    let (refresh_token, jti_str) =
        state
            .jwt
            .issue_refresh_token(&user_id, &req.device_id, &family)?;

    // 10. Store refresh token in database
    let jti: uuid::Uuid = jti_str
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid jti format".into()))?;
    let family_uuid: uuid::Uuid = family
        .parse()
        .expect("family UUID was just generated from Uuid::now_v7()");
    let expires_at = chrono::Utc::now() + state.jwt.refresh_ttl();

    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) \
         VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(jti)
    .bind(user_id.0)
    .bind(req.device_id.0)
    .bind(family_uuid)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // Commit transaction
    tx.commit()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction commit failed: {e}")))?;

    Ok(Json(LoginVerifyResponse {
        access_token,
        refresh_token,
        user_id,
        device_id: req.device_id,
    }))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, ServerError> {
    // 1. Validate the refresh JWT
    let claims = state.jwt.validate_refresh_token(&body.refresh_token)?;

    // 2. Parse claims
    let jti: uuid::Uuid = claims
        .jti
        .parse()
        .map_err(|_| OpenConvError::Unauthorized)?;
    let family_uuid: uuid::Uuid = claims
        .family
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid family format".into()))?;
    let user_id: UserId = claims
        .sub
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid user_id in token".into()))?;
    let device_id: DeviceId = claims
        .device_id
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid device_id in token".into()))?;

    // 3. Begin transaction to prevent race conditions
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction start failed: {e}")))?;

    // 4. Look up the token record by jti (with FOR UPDATE to lock the row)
    let record: Option<(bool,)> =
        sqlx::query_as("SELECT is_used FROM refresh_tokens WHERE jti = $1 FOR UPDATE")
            .bind(jti)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    let (is_used,) = record.ok_or(OpenConvError::Unauthorized)?;

    // 5. Check for token reuse (breach detection)
    if is_used {
        // Invalidate entire token family
        sqlx::query("UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE family = $1")
            .bind(family_uuid)
            .execute(&mut *tx)
            .await
            .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| OpenConvError::Internal(format!("transaction commit failed: {e}")))?;

        return Err(OpenConvError::SessionCompromised.into());
    }

    // 6. Mark the current token as used
    sqlx::query("UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE jti = $1")
        .bind(jti)
        .execute(&mut *tx)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // 7. Issue new token pair in the same family
    let access_token = state.jwt.issue_access_token(&user_id, &device_id)?;
    let (refresh_token, new_jti_str) =
        state
            .jwt
            .issue_refresh_token(&user_id, &device_id, &claims.family)?;

    let new_jti: uuid::Uuid = new_jti_str
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid jti format".into()))?;
    let expires_at = chrono::Utc::now() + state.jwt.refresh_ttl();

    // 8. Store the new refresh token record
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(new_jti)
    .bind(user_id.0)
    .bind(device_id.0)
    .bind(family_uuid)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // 9. Update device last_active
    sqlx::query("UPDATE devices SET last_active = NOW() WHERE id = $1")
        .bind(device_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction commit failed: {e}")))?;

    Ok(Json(RefreshResponse {
        access_token,
        refresh_token,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, ServerError> {
    sqlx::query(
        "UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE user_id = $1 AND device_id = $2 AND is_used = false",
    )
    .bind(auth.user_id.0)
    .bind(auth.device_id.0)
    .execute(&state.db)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    Ok(StatusCode::OK)
}

pub async fn logout_all(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, ServerError> {
    sqlx::query(
        "UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE user_id = $1 AND is_used = false",
    )
    .bind(auth.user_id.0)
    .execute(&state.db)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    Ok(StatusCode::OK)
}

pub async fn list_devices(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<DevicesListResponse>, ServerError> {
    type DeviceRow = (uuid::Uuid, String, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>);
    let rows: Vec<DeviceRow> =
        sqlx::query_as(
            "SELECT id, device_name, last_active, created_at FROM devices WHERE user_id = $1 ORDER BY last_active DESC",
        )
        .bind(auth.user_id.0)
        .fetch_all(&state.db)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    let devices = rows
        .into_iter()
        .map(|(id, device_name, last_active, created_at)| DeviceInfo {
            id: DeviceId(id),
            device_name,
            last_active,
            created_at,
        })
        .collect();

    Ok(Json(DevicesListResponse { devices }))
}

pub async fn revoke_device(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<DeviceId>,
) -> Result<StatusCode, ServerError> {
    // Look up the device
    let owner: Option<(uuid::Uuid,)> = sqlx::query_as("SELECT user_id FROM devices WHERE id = $1")
        .bind(device_id.0)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    let (owner_id,) = owner.ok_or(OpenConvError::NotFound)?;

    if owner_id != auth.user_id.0 {
        return Err(OpenConvError::Forbidden.into());
    }

    // Soft-invalidate all refresh tokens for this device (audit trail)
    sqlx::query(
        "UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE device_id = $1 AND is_used = false",
    )
    .bind(device_id.0)
    .execute(&state.db)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // Delete the device (refresh_tokens cascade-delete via FK)
    sqlx::query("DELETE FROM devices WHERE id = $1")
        .bind(device_id.0)
        .execute(&state.db)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Account Recovery
// ---------------------------------------------------------------------------

/// Redis storage format for recovery codes.
#[derive(serde::Serialize, serde::Deserialize)]
struct RecoveryData {
    code: String,
    attempts_remaining: u32,
}

/// Lua script for atomic recovery code attempt decrement (mismatch path).
/// Returns: [result_code, ""]
///   result_code:
///     0  = decremented successfully
///     -1 = key not found / expired
///     -2 = attempts exhausted, key deleted
const RECOVER_DECREMENT_SCRIPT: &str = r#"
local key = KEYS[1]

local data = redis.call('GET', key)
if not data then
    return {-1, ""}
end

local decoded = cjson.decode(data)
local attempts = tonumber(decoded.attempts_remaining)

if attempts <= 0 then
    redis.call('DEL', key)
    return {-2, ""}
end

decoded.attempts_remaining = attempts - 1
if decoded.attempts_remaining <= 0 then
    redis.call('DEL', key)
    return {-2, ""}
end
local ttl = redis.call('TTL', key)
if ttl > 0 then
    redis.call('SET', key, cjson.encode(decoded), 'EX', ttl)
end

return {0, ""}
"#;

pub async fn recover_start(
    State(state): State<AppState>,
    Json(req): Json<RecoverStartRequest>,
) -> Result<Json<RecoverStartResponse>, ServerError> {
    validate_email(&req.email)?;

    let email = req.email.trim().to_lowercase();

    // Per-email rate limiting
    crate::middleware::rate_limit::check_email_rate_limit(
        &state.redis,
        &email,
        state.config.rate_limit.email_per_address_per_hour,
        3600,
    )
    .await
    .map_err(|_| OpenConvError::RateLimited)?;

    // Always generate code and write to Redis to prevent timing-based email enumeration.
    // Only send the actual email if the user exists.
    let code = format!("{:06}", rand::rng().random_range(0..1_000_000u32));

    let data = RecoveryData {
        code: code.clone(),
        attempts_remaining: 5,
    };
    let json_data = serde_json::to_string(&data)
        .map_err(|e| OpenConvError::Internal(format!("serialization error: {e}")))?;

    let key = format!("recover:{email}");
    state
        .redis
        .set::<(), _, _>(
            &key,
            json_data.as_str(),
            Some(fred::types::Expiration::EX(600)),
            None,
            false,
        )
        .await
        .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&state.db)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    if exists {
        if let Err(e) = state.email.send_recovery_code(&email, &code).await {
            tracing::error!(error = %e, "failed to send recovery email");
        }
    }

    Ok(Json(RecoverStartResponse {
        message: "Recovery code sent".into(),
    }))
}

pub async fn recover_verify(
    State(state): State<AppState>,
    Json(req): Json<RecoverVerifyRequest>,
) -> Result<Json<RecoverVerifyResponse>, ServerError> {
    validate_email(&req.email)?;
    validate_verification_code(&req.code)?;

    let email = req.email.trim().to_lowercase();
    let key = format!("recover:{email}");

    // Fetch stored data from Redis for constant-time comparison in Rust
    let stored: Option<String> = state
        .redis
        .get(&key)
        .await
        .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

    let stored = match stored {
        Some(s) => s,
        None => return Err(OpenConvError::Validation("invalid or expired code".into()).into()),
    };

    let data: RecoveryData = serde_json::from_str(&stored)
        .map_err(|e| OpenConvError::Internal(format!("deserialization error: {e}")))?;

    if data.attempts_remaining == 0 {
        state
            .redis
            .del::<(), _>(&key)
            .await
            .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;
        return Err(OpenConvError::Validation("code expired, request a new one".into()).into());
    }

    // Constant-time comparison to prevent timing attacks
    let codes_match: bool = data.code.as_bytes().ct_eq(req.code.as_bytes()).into();

    if codes_match {
        // Delete the consumed key
        state
            .redis
            .del::<(), _>(&key)
            .await
            .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

        // Look up user_id by email
        let user_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?
            .ok_or_else(|| OpenConvError::Validation("invalid or expired code".into()))?;

        let uid = UserId(user_id);
        let token = state.jwt.issue_recovery_token(&email, &uid)?;

        Ok(Json(RecoverVerifyResponse {
            recovery_token: token,
        }))
    } else {
        // Atomically decrement attempts via Lua script
        use fred::interfaces::LuaInterface;
        let result: Vec<fred::types::Value> = state
            .redis
            .eval(RECOVER_DECREMENT_SCRIPT, vec![key], Vec::<String>::new())
            .await
            .map_err(|e| OpenConvError::Internal(format!("redis error: {e}")))?;

        let result_code: i64 = if result.is_empty() {
            0
        } else {
            match &result[0] {
                fred::types::Value::Integer(n) => *n,
                _ => 0,
            }
        };

        if result_code == -2 {
            Err(OpenConvError::Validation("code expired, request a new one".into()).into())
        } else {
            Err(OpenConvError::Validation("invalid or expired code".into()).into())
        }
    }
}

pub async fn recover_complete(
    State(state): State<AppState>,
    Json(req): Json<RecoverCompleteRequest>,
) -> Result<Json<RecoverCompleteResponse>, ServerError> {
    // 1. Validate the recovery token
    let claims = state.jwt.validate_recovery_token(&req.recovery_token)?;

    let user_id: UserId = claims
        .user_id
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid user_id in recovery token".into()))?;

    // 2. Validate new public key
    let pk_bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.new_public_key)
        .map_err(|_| OpenConvError::Validation("invalid public key encoding".into()))?;

    if pk_bytes.len() != 33 {
        return Err(
            OpenConvError::Validation("public key must be 33 bytes when decoded".into()).into(),
        );
    }

    libsignal_protocol::PublicKey::deserialize(&pk_bytes)
        .map_err(|_| OpenConvError::Validation("invalid public key format".into()))?;

    // 3. Decode pre-key bundle
    let pre_key_data = base64::engine::general_purpose::STANDARD
        .decode(&req.new_pre_key_bundle)
        .map_err(|_| OpenConvError::Validation("invalid pre-key bundle encoding".into()))?;

    // 4. Begin transaction for atomic identity replacement
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction start failed: {e}")))?;

    // a. Update public key and set public_key_changed_at
    let rows = sqlx::query(
        "UPDATE users SET public_key = $1, public_key_changed_at = NOW() WHERE id = $2",
    )
    .bind(&req.new_public_key)
    .bind(user_id.0)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    if rows.rows_affected() == 0 {
        return Err(OpenConvError::NotFound.into());
    }

    // b. Delete all existing refresh tokens for this user
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // c. Delete all existing pre-key bundles for this user
    sqlx::query("DELETE FROM pre_key_bundles WHERE user_id = $1")
        .bind(user_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // d. Delete all existing devices for this user
    sqlx::query("DELETE FROM devices WHERE user_id = $1")
        .bind(user_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // e. Create new device
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(req.device_id.0)
    .bind(user_id.0)
    .bind(&req.device_name)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // f. Store new pre-key bundle
    let pre_key_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO pre_key_bundles (id, user_id, device_id, key_data, is_used) VALUES ($1, $2, $3, $4, false)",
    )
    .bind(pre_key_id)
    .bind(user_id.0)
    .bind(req.device_id.0)
    .bind(&pre_key_data)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // 5. Issue new tokens
    let family = uuid::Uuid::now_v7().to_string();
    let access_token = state.jwt.issue_access_token(&user_id, &req.device_id)?;
    let (refresh_token, jti_str) =
        state
            .jwt
            .issue_refresh_token(&user_id, &req.device_id, &family)?;

    let jti: uuid::Uuid = jti_str
        .parse()
        .map_err(|_| OpenConvError::Internal("invalid jti format".into()))?;
    let family_uuid: uuid::Uuid = family
        .parse()
        .expect("family UUID was just generated from Uuid::now_v7()");
    let expires_at = chrono::Utc::now() + state.jwt.refresh_ttl();

    // Store refresh token record
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(jti)
    .bind(user_id.0)
    .bind(req.device_id.0)
    .bind(family_uuid)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| OpenConvError::Internal(format!("database error: {e}")))?;

    // 6. Commit transaction
    tx.commit()
        .await
        .map_err(|e| OpenConvError::Internal(format!("transaction commit failed: {e}")))?;

    Ok(Json(RecoverCompleteResponse {
        user_id,
        access_token,
        refresh_token,
        device_id: req.device_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_email_accepts_valid_email() {
        assert!(validate_email("user@example.com").is_ok());
    }

    #[test]
    fn validate_email_rejects_missing_at() {
        assert!(validate_email("userexample.com").is_err());
    }

    #[test]
    fn validate_email_rejects_missing_domain_dot() {
        assert!(validate_email("user@example").is_err());
    }

    #[test]
    fn validate_email_rejects_empty() {
        assert!(validate_email("").is_err());
    }

    #[test]
    fn validate_email_rejects_double_at() {
        assert!(validate_email("user@@example.com").is_err());
    }

    #[test]
    fn validate_display_name_accepts_valid() {
        let result = validate_display_name("Alice").unwrap();
        assert_eq!(result, "Alice");
    }

    #[test]
    fn validate_display_name_trims_whitespace() {
        let result = validate_display_name("  Alice  ").unwrap();
        assert_eq!(result, "Alice");
    }

    #[test]
    fn validate_display_name_rejects_empty() {
        assert!(validate_display_name("").is_err());
    }

    #[test]
    fn validate_display_name_rejects_whitespace_only() {
        assert!(validate_display_name("   ").is_err());
    }

    #[test]
    fn validate_display_name_rejects_over_64_chars() {
        let long_name = "a".repeat(65);
        assert!(validate_display_name(&long_name).is_err());
    }

    #[test]
    fn validate_display_name_accepts_exactly_64_chars() {
        let name = "a".repeat(64);
        assert!(validate_display_name(&name).is_ok());
    }

    #[test]
    fn validate_display_name_rejects_control_characters() {
        assert!(validate_display_name("Alice\x00Bob").is_err());
        assert!(validate_display_name("Alice\nBob").is_err());
    }

    #[test]
    fn validate_display_name_counts_chars_not_bytes() {
        // 64 CJK characters = 192 bytes but should be accepted
        let name = "\u{4e00}".repeat(64);
        assert!(validate_display_name(&name).is_ok());
        // 65 CJK characters should be rejected
        let name = "\u{4e00}".repeat(65);
        assert!(validate_display_name(&name).is_err());
    }

    #[test]
    fn validate_verification_code_accepts_valid() {
        assert!(validate_verification_code("123456").is_ok());
        assert!(validate_verification_code("000000").is_ok());
    }

    #[test]
    fn validate_verification_code_rejects_wrong_length() {
        assert!(validate_verification_code("12345").is_err());
        assert!(validate_verification_code("1234567").is_err());
    }

    #[test]
    fn validate_verification_code_rejects_non_numeric() {
        assert!(validate_verification_code("12345a").is_err());
        assert!(validate_verification_code("abcdef").is_err());
    }

    #[test]
    fn stored_challenge_roundtrip() {
        let data = StoredChallenge {
            challenge: "dGVzdA==".into(),
            exists: true,
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: StoredChallenge = serde_json::from_str(&json).unwrap();
        assert_eq!(back.challenge, "dGVzdA==");
        assert!(back.exists);
    }

    #[test]
    fn verification_data_roundtrip() {
        let data = VerificationData {
            code: "123456".into(),
            display_name: "Test User".into(),
            attempts_remaining: 5,
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: VerificationData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, "123456");
        assert_eq!(back.display_name, "Test User");
        assert_eq!(back.attempts_remaining, 5);
    }
}
