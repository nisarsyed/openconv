use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::UserId;
use sqlx::Row;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use crate::validation::{escape_ilike, validate_display_name};

// ---------------------------------------------------------------------------
// Request / Response Types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
pub struct UserProfileResponse {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub public_key: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Serialize)]
pub struct PublicProfileResponse {
    pub id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub public_key: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchUsersQuery {
    pub q: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchUsersResponse {
    pub users: Vec<PublicProfileResponse>,
    pub total: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct UploadPreKeysRequest {
    pub pre_key_bundles: Vec<Vec<u8>>,
}

#[derive(Debug, serde::Serialize)]
pub struct PreKeyBundleResponse {
    pub key_data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/users/me — full profile including email.
pub async fn get_me(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<UserProfileResponse>, ServerError> {
    let row = sqlx::query(
        "SELECT id, email, display_name, avatar_url, public_key, created_at, updated_at FROM users WHERE id = $1",
    )
    .bind(auth_user.user_id.0)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(profile_from_row(&row)))
}

/// PATCH /api/users/me — update display_name and/or avatar_url.
pub async fn update_me(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfileResponse>, ServerError> {
    let display_name = req
        .display_name
        .as_deref()
        .map(validate_display_name)
        .transpose()?;

    if let Some(ref url) = req.avatar_url {
        if url.trim().is_empty() {
            return Err(OpenConvError::Validation("avatar_url must not be empty".into()).into());
        }
        if url.len() > 2048 {
            return Err(OpenConvError::Validation(
                "avatar_url must be 2048 characters or fewer".into(),
            )
            .into());
        }
    }

    if display_name.is_none() && req.avatar_url.is_none() {
        return get_me(State(state), auth_user).await;
    }

    let mut builder = sqlx::QueryBuilder::new("UPDATE users SET ");
    let mut has_set = false;

    if let Some(ref name) = display_name {
        builder.push("display_name = ");
        builder.push_bind(name.as_str());
        has_set = true;
    }

    if let Some(ref url) = req.avatar_url {
        if has_set {
            builder.push(", ");
        }
        builder.push("avatar_url = ");
        builder.push_bind(url.as_str());
    }

    builder.push(" WHERE id = ");
    builder.push_bind(auth_user.user_id.0);
    builder
        .push(" RETURNING id, email, display_name, avatar_url, public_key, created_at, updated_at");

    let row = builder
        .build()
        .fetch_one(&state.db)
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    Ok(Json(profile_from_row(&row)))
}

/// GET /api/users/:user_id — public profile (no email).
pub async fn get_user(
    State(state): State<AppState>,
    _auth_user: AuthUser,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<Json<PublicProfileResponse>, ServerError> {
    let row =
        sqlx::query("SELECT id, display_name, avatar_url, public_key FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?
            .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(public_profile_from_row(&row)))
}

/// GET /api/users/search — search by display_name (ILIKE).
pub async fn search_users(
    State(state): State<AppState>,
    _auth_user: AuthUser,
    Query(params): Query<SearchUsersQuery>,
) -> Result<Json<SearchUsersResponse>, ServerError> {
    let q = params.q.trim().to_string();
    if q.is_empty() {
        return Err(OpenConvError::Validation("q must not be empty".into()).into());
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    let offset = params.offset.unwrap_or(0).max(0);
    let escaped = escape_ilike(&q);
    let pattern = format!("%{escaped}%");
    let prefix_pattern = format!("{escaped}%");

    let rows = sqlx::query(
        "SELECT id, display_name, avatar_url, public_key FROM users \
         WHERE display_name ILIKE $1 OR public_key ILIKE $2 \
         ORDER BY display_name LIMIT $3 OFFSET $4",
    )
    .bind(&pattern)
    .bind(&prefix_pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE display_name ILIKE $1 OR public_key ILIKE $2",
    )
    .bind(&pattern)
    .bind(&prefix_pattern)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    Ok(Json(SearchUsersResponse {
        users: rows.iter().map(public_profile_from_row).collect(),
        total,
    }))
}

/// GET /api/users/:user_id/prekeys — fetch one unused pre-key bundle.
pub async fn get_prekeys(
    State(state): State<AppState>,
    _auth_user: AuthUser,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<Json<PreKeyBundleResponse>, ServerError> {
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    let row = sqlx::query(
        "SELECT id, key_data FROM pre_key_bundles \
         WHERE user_id = $1 AND is_used = false \
         LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    let bundle_id: uuid::Uuid = row.get("id");
    let key_data: Vec<u8> = row.get("key_data");

    sqlx::query("UPDATE pre_key_bundles SET is_used = true WHERE id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    tx.commit()
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    Ok(Json(PreKeyBundleResponse { key_data }))
}

const MAX_BUNDLE_SIZE: usize = 1024;

/// POST /api/users/me/prekeys — upload new pre-key bundles.
pub async fn upload_prekeys(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(req): Json<UploadPreKeysRequest>,
) -> Result<StatusCode, ServerError> {
    if req.pre_key_bundles.is_empty() {
        return Err(OpenConvError::Validation("pre_key_bundles must not be empty".into()).into());
    }
    if req.pre_key_bundles.len() > 100 {
        return Err(OpenConvError::Validation("pre_key_bundles must not exceed 100".into()).into());
    }
    if req
        .pre_key_bundles
        .iter()
        .any(|b| b.len() > MAX_BUNDLE_SIZE)
    {
        return Err(OpenConvError::Validation(format!(
            "each pre-key bundle must be {MAX_BUNDLE_SIZE} bytes or fewer"
        ))
        .into());
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    for bundle in &req.pre_key_bundles {
        let id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO pre_key_bundles (id, user_id, device_id, key_data, is_used) VALUES ($1, $2, $3, $4, false)",
        )
        .bind(id)
        .bind(auth_user.user_id.0)
        .bind(auth_user.device_id.0)
        .bind(bundle.as_slice())
        .execute(&mut *tx)
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;
    }

    tx.commit()
        .await
        .map_err(|e| ServerError(OpenConvError::Internal(e.to_string())))?;

    Ok(StatusCode::CREATED)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn profile_from_row(row: &sqlx::postgres::PgRow) -> UserProfileResponse {
    UserProfileResponse {
        id: UserId(row.get("id")),
        email: row.get("email"),
        display_name: row.get("display_name"),
        avatar_url: row.get("avatar_url"),
        public_key: row.get("public_key"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn public_profile_from_row(row: &sqlx::postgres::PgRow) -> PublicProfileResponse {
    PublicProfileResponse {
        id: UserId(row.get("id")),
        display_name: row.get("display_name"),
        avatar_url: row.get("avatar_url"),
        public_key: row.get("public_key"),
    }
}
