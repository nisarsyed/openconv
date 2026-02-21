use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use axum_extra::extract::Multipart;
use object_store::path::Path as StorePath;
use object_store::{ObjectStore, PutPayload};
use openconv_shared::api::file::{FileMetaResponse, FileResponse};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{DmChannelId, FileId, GuildId, UserId};
use openconv_shared::permissions::Permissions;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::extractors::channel_member::ChannelMember;
use crate::state::AppState;

const MAX_FILE_NAME_LEN: usize = 255;
const MAX_MIME_TYPE_LEN: usize = 127;
const MAX_ENCRYPTED_BLOB_KEY_LEN: usize = 4096;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

/// Sanitize a user-provided filename for safe use in Content-Disposition.
/// Strips path separators and control characters.
fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\' && *c != '"')
        .collect::<String>()
        .trim()
        .to_string()
}

/// Validate that a MIME type looks reasonable (type/subtype, ASCII, no control chars).
fn validate_mime_type(mime: &str) -> Result<(), ServerError> {
    if mime.len() > MAX_MIME_TYPE_LEN {
        return Err(ServerError(OpenConvError::Validation(
            "mime_type too long".into(),
        )));
    }
    let parts: Vec<&str> = mime.splitn(2, '/').collect();
    if parts.len() != 2
        || parts[0].is_empty()
        || parts[1].is_empty()
        || mime.chars().any(|c| c.is_control())
    {
        return Err(ServerError(OpenConvError::Validation(
            "invalid mime_type format".into(),
        )));
    }
    Ok(())
}

// ─── Shared multipart parsing ──────────────────────────────

struct ParsedUpload {
    file_bytes: Vec<u8>,
    file_name: String,
    mime_type: String,
    encrypted_blob_key: String,
}

/// Parse multipart fields for file upload. Validates sizes and field lengths.
async fn parse_upload_multipart(
    multipart: &mut Multipart,
    max_size: u64,
) -> Result<ParsedUpload, ServerError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut encrypted_blob_key: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let data = field.bytes().await.map_err(|_| {
                    ServerError(OpenConvError::Validation("failed to read file data".into()))
                })?;
                if data.len() as u64 > max_size {
                    return Err(payload_too_large(max_size));
                }
                file_bytes = Some(data.to_vec());
            }
            "file_name" => {
                let val = field.text().await.map_err(|_| {
                    ServerError(OpenConvError::Validation("failed to read file_name".into()))
                })?;
                if val.len() > MAX_FILE_NAME_LEN {
                    return Err(ServerError(OpenConvError::Validation(
                        "file_name too long".into(),
                    )));
                }
                let sanitized = sanitize_file_name(&val);
                if sanitized.is_empty() {
                    return Err(ServerError(OpenConvError::Validation(
                        "file_name is empty after sanitization".into(),
                    )));
                }
                file_name = Some(sanitized);
            }
            "mime_type" => {
                let val = field.text().await.map_err(|_| {
                    ServerError(OpenConvError::Validation("failed to read mime_type".into()))
                })?;
                validate_mime_type(&val)?;
                mime_type = Some(val);
            }
            "encrypted_blob_key" => {
                let val = field.text().await.map_err(|_| {
                    ServerError(OpenConvError::Validation(
                        "failed to read encrypted_blob_key".into(),
                    ))
                })?;
                if val.len() > MAX_ENCRYPTED_BLOB_KEY_LEN {
                    return Err(ServerError(OpenConvError::Validation(
                        "encrypted_blob_key too long".into(),
                    )));
                }
                encrypted_blob_key = Some(val);
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes
        .ok_or_else(|| ServerError(OpenConvError::Validation("missing file field".into())))?;
    let file_name = file_name
        .ok_or_else(|| ServerError(OpenConvError::Validation("missing file_name field".into())))?;
    let mime_type = mime_type
        .ok_or_else(|| ServerError(OpenConvError::Validation("missing mime_type field".into())))?;
    let encrypted_blob_key = encrypted_blob_key.ok_or_else(|| {
        ServerError(OpenConvError::Validation(
            "missing encrypted_blob_key field".into(),
        ))
    })?;

    Ok(ParsedUpload {
        file_bytes,
        file_name,
        mime_type,
        encrypted_blob_key,
    })
}

/// Store file bytes in object store, insert DB record, and return the response.
/// If DB insert fails, deletes the blob from the store (best-effort cleanup).
async fn store_and_insert(
    state: &AppState,
    uploader_id: UserId,
    storage_path: &str,
    parsed: ParsedUpload,
) -> Result<(StatusCode, Json<FileResponse>), ServerError> {
    let size_bytes = parsed.file_bytes.len() as i64;

    let store_path = StorePath::from(storage_path);
    state
        .object_store
        .put(&store_path, PutPayload::from(parsed.file_bytes))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "object store put failed");
            ServerError(OpenConvError::Internal("file storage error".into()))
        })?;

    let row = sqlx::query_as::<_, FileRow>(
        "INSERT INTO files (uploader_id, file_name, mime_type, size_bytes, storage_path, encrypted_blob_key) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, file_name, mime_type, size_bytes, created_at",
    )
    .bind(uploader_id)
    .bind(&parsed.file_name)
    .bind(&parsed.mime_type)
    .bind(size_bytes)
    .bind(storage_path)
    .bind(&parsed.encrypted_blob_key)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        // Best-effort cleanup: delete the blob we just wrote
        let store = state.object_store.clone();
        let path = store_path.clone();
        tokio::spawn(async move {
            if let Err(del_err) = store.delete(&path).await {
                tracing::error!(error = %del_err, "failed to clean up orphan blob after DB insert failure");
            }
        });
        db_err(e)
    })?;

    Ok((
        StatusCode::CREATED,
        Json(FileResponse {
            id: row.id,
            file_name: row.file_name,
            mime_type: row.mime_type,
            size_bytes: row.size_bytes,
            created_at: row.created_at,
        }),
    ))
}

/// Pre-check Content-Length header to reject obviously oversized uploads early.
fn check_content_length(
    headers: &axum::http::HeaderMap,
    max_size: u64,
) -> Result<(), ServerError> {
    if let Some(content_length) = headers.get(header::CONTENT_LENGTH) {
        if let Ok(len_str) = content_length.to_str() {
            if let Ok(len) = len_str.parse::<u64>() {
                if len > max_size {
                    return Err(payload_too_large(max_size));
                }
            }
        }
    }
    Ok(())
}

/// Return a 413 Payload Too Large response.
fn payload_too_large(max_size: u64) -> ServerError {
    ServerError(OpenConvError::PayloadTooLarge(format!(
        "file exceeds maximum size of {} bytes",
        max_size
    )))
}

// ─── Upload ─────────────────────────────────────────────────

/// POST /api/channels/:channel_id/files
/// Upload an encrypted file to a guild channel.
pub async fn upload(
    State(state): State<AppState>,
    channel_member: ChannelMember,
    headers: axum::http::HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<FileResponse>), ServerError> {
    channel_member.require(Permissions::ATTACH_FILES)?;

    let max_size = state.config.file_storage.max_file_size_bytes;

    // Pre-check Content-Length header to reject obviously oversized uploads early
    check_content_length(&headers, max_size)?;

    let parsed = parse_upload_multipart(&mut multipart, max_size).await?;

    let storage_uuid = uuid::Uuid::now_v7();
    let storage_path = format!(
        "guilds/{}/{}/{}",
        channel_member.guild_id, channel_member.channel_id, storage_uuid
    );

    store_and_insert(&state, channel_member.user_id, &storage_path, parsed).await
}

/// POST /api/dm-channels/:dm_channel_id/files
/// Upload an encrypted file to a DM channel.
pub async fn upload_dm(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(dm_channel_id): Path<DmChannelId>,
    headers: axum::http::HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<FileResponse>), ServerError> {
    // Verify DM membership
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dm_channel_members WHERE dm_channel_id = $1 AND user_id = $2)",
    )
    .bind(dm_channel_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    if !is_member {
        return Err(ServerError(OpenConvError::Forbidden));
    }

    let max_size = state.config.file_storage.max_file_size_bytes;

    // Pre-check Content-Length
    check_content_length(&headers, max_size)?;

    let parsed = parse_upload_multipart(&mut multipart, max_size).await?;

    let storage_uuid = uuid::Uuid::now_v7();
    let storage_path = format!("dm/{}/{}", dm_channel_id, storage_uuid);

    store_and_insert(&state, auth.user_id, &storage_path, parsed).await
}

// ─── Download ───────────────────────────────────────────────

/// GET /api/files/:file_id
/// Download an encrypted file.
pub async fn download(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(file_id): Path<FileId>,
) -> Result<Response, ServerError> {
    let file = sqlx::query_as::<_, FullFileRow>(
        "SELECT id, uploader_id, file_name, mime_type, size_bytes, storage_path, created_at \
         FROM files WHERE id = $1",
    )
    .bind(file_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    verify_file_access(&state.db, auth.user_id, &file.storage_path).await?;

    let store_path = StorePath::from(file.storage_path.as_str());
    let result = state.object_store.get(&store_path).await.map_err(|e| {
        tracing::error!(error = %e, "object store get failed");
        ServerError(OpenConvError::Internal("file storage error".into()))
    })?;

    let bytes = result.bytes().await.map_err(|e| {
        tracing::error!(error = %e, "object store read failed");
        ServerError(OpenConvError::Internal("file storage error".into()))
    })?;

    // Always serve as octet-stream to prevent browser execution of content
    let content_disposition = format!(
        "attachment; filename=\"{}\"",
        sanitize_file_name(&file.file_name)
    );

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .header(header::CONTENT_LENGTH, file.size_bytes.to_string())
        .body(Body::from(bytes))
        .map_err(|_| ServerError(OpenConvError::Internal("response build error".into())))?;

    Ok(response)
}

// ─── Metadata ───────────────────────────────────────────────

/// GET /api/files/:file_id/meta
/// Get file metadata without downloading.
pub async fn meta(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(file_id): Path<FileId>,
) -> Result<Json<FileMetaResponse>, ServerError> {
    let file = sqlx::query_as::<_, FullFileRow>(
        "SELECT id, uploader_id, file_name, mime_type, size_bytes, storage_path, created_at \
         FROM files WHERE id = $1",
    )
    .bind(file_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    verify_file_access(&state.db, auth.user_id, &file.storage_path).await?;

    Ok(Json(FileMetaResponse {
        id: file.id,
        file_name: file.file_name,
        mime_type: file.mime_type,
        size_bytes: file.size_bytes,
        uploader_id: file.uploader_id,
        created_at: file.created_at,
    }))
}

// ─── Access verification ────────────────────────────────────

/// Verify the user has access to the file by checking channel/guild membership
/// based on the storage path pattern.
async fn verify_file_access(
    db: &sqlx::PgPool,
    user_id: UserId,
    storage_path: &str,
) -> Result<(), ServerError> {
    if let Some(rest) = storage_path.strip_prefix("guilds/") {
        // Format: guilds/{guild_id}/{channel_id}/{uuid}
        let guild_id_str = rest
            .split('/')
            .next()
            .ok_or(ServerError(OpenConvError::Internal(
                "malformed storage path".into(),
            )))?;
        let guild_id: GuildId = guild_id_str
            .parse()
            .map_err(|_| ServerError(OpenConvError::Internal("malformed storage path".into())))?;

        let is_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(db_err)?;

        if !is_member {
            return Err(ServerError(OpenConvError::Forbidden));
        }
    } else if let Some(rest) = storage_path.strip_prefix("dm/") {
        // Format: dm/{dm_channel_id}/{uuid}
        let dm_id_str = rest
            .split('/')
            .next()
            .ok_or(ServerError(OpenConvError::Internal(
                "malformed storage path".into(),
            )))?;
        let dm_channel_id: DmChannelId = dm_id_str
            .parse()
            .map_err(|_| ServerError(OpenConvError::Internal("malformed storage path".into())))?;

        let is_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dm_channel_members WHERE dm_channel_id = $1 AND user_id = $2)",
        )
        .bind(dm_channel_id)
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(db_err)?;

        if !is_member {
            return Err(ServerError(OpenConvError::Forbidden));
        }
    } else {
        return Err(ServerError(OpenConvError::Internal(
            "unknown storage path format".into(),
        )));
    }

    Ok(())
}

// ─── Route builders ─────────────────────────────────────────

/// Routes for guild channel file upload.
/// Mounted at /api/channels/:channel_id/files
pub fn guild_file_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/", axum::routing::post(upload))
}

/// Routes for DM channel file upload.
/// Mounted at /api/dm-channels/:dm_channel_id/files
pub fn dm_file_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/", axum::routing::post(upload_dm))
}

/// Routes for file download and metadata.
/// Mounted at /api/files
pub fn file_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/{file_id}", axum::routing::get(download))
        .route("/{file_id}/meta", axum::routing::get(meta))
}

// ─── Internal row types ─────────────────────────────────────

#[derive(sqlx::FromRow)]
struct FileRow {
    id: FileId,
    file_name: String,
    mime_type: String,
    size_bytes: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct FullFileRow {
    id: FileId,
    uploader_id: UserId,
    file_name: String,
    mime_type: String,
    size_bytes: i64,
    storage_path: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guild_file_routes_build_without_panic() {
        let _ = guild_file_routes();
    }

    #[test]
    fn dm_file_routes_build_without_panic() {
        let _ = dm_file_routes();
    }

    #[test]
    fn file_routes_build_without_panic() {
        let _ = file_routes();
    }

    #[test]
    fn guild_storage_path_format() {
        let guild_id = uuid::Uuid::nil();
        let channel_id = uuid::Uuid::nil();
        let storage_uuid = uuid::Uuid::nil();
        let path = format!("guilds/{guild_id}/{channel_id}/{storage_uuid}");
        assert!(path.starts_with("guilds/"));
        assert_eq!(path.split('/').count(), 4);
    }

    #[test]
    fn dm_storage_path_format() {
        let dm_id = uuid::Uuid::nil();
        let storage_uuid = uuid::Uuid::nil();
        let path = format!("dm/{dm_id}/{storage_uuid}");
        assert!(path.starts_with("dm/"));
        assert_eq!(path.split('/').count(), 3);
    }

    #[test]
    fn sanitize_strips_control_chars_and_path_separators() {
        assert_eq!(sanitize_file_name("hello.txt"), "hello.txt");
        assert_eq!(sanitize_file_name("../etc/passwd"), "..etcpasswd");
        assert_eq!(sanitize_file_name("file\0name.txt"), "filename.txt");
        assert_eq!(sanitize_file_name("file\"name.txt"), "filename.txt");
        assert_eq!(sanitize_file_name("normal file.pdf"), "normal file.pdf");
    }

    #[test]
    fn validate_mime_type_accepts_valid() {
        assert!(validate_mime_type("application/octet-stream").is_ok());
        assert!(validate_mime_type("image/png").is_ok());
        assert!(validate_mime_type("text/plain").is_ok());
    }

    #[test]
    fn validate_mime_type_rejects_invalid() {
        assert!(validate_mime_type("notamime").is_err());
        assert!(validate_mime_type("").is_err());
        assert!(validate_mime_type("/noprefix").is_err());
        assert!(validate_mime_type("nosuffix/").is_err());
        assert!(validate_mime_type(&"a".repeat(200)).is_err());
    }
}
