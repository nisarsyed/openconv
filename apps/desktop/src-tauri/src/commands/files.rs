use std::path::PathBuf;
use std::sync::Mutex;

use openconv_shared::api::ws::ClientMessage;
use openconv_shared::ids::{ChannelId, DmChannelId, MessageId};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth_service::AppError;
use crate::cache::dm_channels;
use crate::cache::messages::{self, CachedMessage};
use crate::cache::search;
use crate::cache::CacheDb;
use crate::commands::device_directory;
use crate::crypto_service::CryptoState;
use crate::ws::handlers::{MSG_STATUS_PENDING, MSG_STATUS_QUEUED};
use crate::ws::WsState;

use super::messaging::MessageRateLimiter;

/// Maximum file size: 25 MB.
const MAX_FILE_SIZE: u64 = 25 * 1024 * 1024;

/// Maximum thumbnail dimension (width or height).
const THUMBNAIL_MAX_DIM: u32 = 200;

/// File metadata returned to the frontend after a successful send or download.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    pub file_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub mime_type: String,
    pub local_path: Option<String>,
    pub thumbnail_path: Option<String>,
}

/// JSON payload embedded in message plaintext for file messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMessagePayload {
    pub r#type: String,
    pub file_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub mime_type: String,
    pub file_key_base64: String,
}

/// Detect MIME type from file extension.
fn detect_mime_type(path: &std::path::Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png".into(),
        Some("jpg" | "jpeg") => "image/jpeg".into(),
        Some("gif") => "image/gif".into(),
        Some("webp") => "image/webp".into(),
        Some("svg") => "image/svg+xml".into(),
        Some("pdf") => "application/pdf".into(),
        Some("txt") => "text/plain".into(),
        Some("json") => "application/json".into(),
        Some("zip") => "application/zip".into(),
        Some("mp4") => "video/mp4".into(),
        Some("mp3") => "audio/mpeg".into(),
        Some("wav") => "audio/wav".into(),
        Some("doc") => "application/msword".into(),
        Some("docx") => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into()
        }
        _ => "application/octet-stream".into(),
    }
}

/// Check if a MIME type is an image type we can generate thumbnails for.
fn is_thumbnail_supported(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

/// Validate that a file path exists and is within the size limit.
fn validate_file(path: &std::path::Path) -> Result<u64, AppError> {
    let metadata =
        std::fs::metadata(path).map_err(|e| AppError::new(format!("file not found: {e}")))?;

    if !metadata.is_file() {
        return Err(AppError::new("path is not a file".to_string()));
    }

    let size = metadata.len();
    if size > MAX_FILE_SIZE {
        return Err(AppError::new(format!(
            "file exceeds maximum size of 25MB (got {} bytes)",
            size
        )));
    }

    Ok(size)
}

/// Generate a JPEG thumbnail for an image file.
///
/// Uses the `image` crate with memory limits to prevent OOM on large images.
/// Resizes preserving aspect ratio using Lanczos3 filter.
fn generate_thumbnail_sync(
    source_path: &std::path::Path,
    output_path: &std::path::Path,
    max_dimension: u32,
) -> Result<(), AppError> {
    use image::ImageReader;

    let reader = ImageReader::open(source_path)
        .map_err(|e| AppError::new(format!("failed to open image: {e}")))?
        .with_guessed_format()
        .map_err(|e| AppError::new(format!("failed to guess image format: {e}")))?;

    // Set memory limits to prevent OOM
    let mut reader = reader;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(100 * 1024 * 1024); // 100MB decoded max
    reader.limits(limits);

    let img = reader
        .decode()
        .map_err(|e| AppError::new(format!("failed to decode image: {e}")))?;

    let thumbnail = img.resize(
        max_dimension,
        max_dimension,
        image::imageops::FilterType::Lanczos3,
    );

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::new(format!("failed to create thumbnail dir: {e}")))?;
    }

    thumbnail
        .save_with_format(output_path, image::ImageFormat::Jpeg)
        .map_err(|e| AppError::new(format!("failed to save thumbnail: {e}")))?;

    Ok(())
}

/// Get the app's attachments directory.
fn attachments_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new(format!("failed to get app data dir: {e}")))?;
    Ok(data_dir.join("attachments"))
}

/// Get the app's thumbnails directory.
fn thumbnails_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new(format!("failed to get app data dir: {e}")))?;
    Ok(data_dir.join("thumbnails"))
}

/// Shared file upload pipeline used by both `send_file` and `send_dm_file`.
///
/// Pipeline:
/// 1. Validate file exists and is < 25MB
/// 2. Read file bytes from disk (async)
/// 3. Encrypt with AES-256-GCM via CryptoService::encrypt_file (spawn_blocking)
/// 4. Upload encrypted blob to server via REST (empty encrypted_blob_key -- key
///    distribution happens via the E2E message, not sent to server)
/// 5. Build file metadata JSON as message plaintext (includes base64 file key)
/// 6. Insert optimistic message into local cache
/// 7. Send via WebSocket
/// 8. Return file metadata
async fn upload_and_send_file(
    app: &AppHandle,
    channel_id: Option<&str>,
    dm_channel_id: Option<&str>,
    file_path: &str,
    ws_state: &WsState,
    cache_db: &CacheDb,
    rate_limiter: &Mutex<MessageRateLimiter>,
) -> Result<FileMetadata, AppError> {
    let path = PathBuf::from(file_path);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mime_type = detect_mime_type(&path);

    // 1. Rate limit check
    {
        let mut limiter = rate_limiter
            .lock()
            .map_err(|_| AppError::new("rate limiter lock poisoned"))?;
        limiter.check_and_record()?;
    }

    // 2. Validate file
    let file_size = validate_file(&path)?;

    // 3. Read file bytes (async to avoid blocking runtime)
    let file_bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::new(format!("failed to read file: {e}")))?;

    // 4. Encrypt via spawn_blocking
    let app_clone = app.clone();
    let (encrypted_blob, file_key) = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        crypto_state.crypto_service.encrypt_file(&file_bytes)
    })
    .await
    .map_err(|e| AppError::new(format!("spawn_blocking failed: {e}")))?
    .map_err(|e| AppError::new(e.message))?;

    // 5. Get access token for upload
    let access_token = tokio::task::spawn_blocking(crate::auth_service::get_access_token)
        .await
        .map_err(|e| AppError::new(format!("spawn_blocking: {e}")))?
        .map_err(|e| AppError::new(format!("auth: {e}")))?;

    let api_base_url =
        std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    // 6. Upload encrypted blob -- do NOT send the real key to the server.
    //    Key distribution happens via per-device E2E encryption in the WS message.
    let upload_url = if let Some(ch_id) = channel_id {
        format!("{api_base_url}/api/channels/{ch_id}/files")
    } else if let Some(dm_id) = dm_channel_id {
        format!("{api_base_url}/api/dm-channels/{dm_id}/files")
    } else {
        return Err(AppError::new("no channel or dm_channel specified"));
    };

    let client = reqwest::Client::new();
    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(encrypted_blob)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")
                .map_err(|e| AppError::new(format!("mime: {e}")))?,
        )
        .text("file_name", file_name.clone())
        .text("mime_type", mime_type.clone())
        .text("encrypted_blob_key", "e2e"); // Placeholder -- key sent via E2E message

    let resp = client
        .post(&upload_url)
        .bearer_auth(&access_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| AppError::new(format!("upload failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::new(format!("upload failed ({status}): {body}")));
    }

    #[derive(Deserialize)]
    struct FileResponse {
        id: String,
    }

    let file_resp: FileResponse = resp
        .json()
        .await
        .map_err(|e| AppError::new(format!("parse response: {e}")))?;

    // 7. Build file metadata JSON as message plaintext
    let file_key_base64 = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&*file_key)
    };

    let payload = FileMessagePayload {
        r#type: "file".into(),
        file_id: file_resp.id.clone(),
        file_name: file_name.clone(),
        file_size,
        mime_type: mime_type.clone(),
        file_key_base64,
    };
    let plaintext = serde_json::to_string(&payload)
        .map_err(|e| AppError::new(format!("serialize file metadata: {e}")))?;

    // 8. Get current user ID
    let sender_id = {
        let uid = ws_state.current_user_id.read().await;
        uid.map(|id| id.to_string()).unwrap_or_default()
    };

    // 9. Generate message ID and client nonce
    let message_id = MessageId::new();
    let msg_id_str = message_id.to_string();
    let client_nonce = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // 10. Optimistic insert into local cache
    let cached = CachedMessage {
        id: msg_id_str.clone(),
        channel_id: channel_id.map(String::from),
        dm_channel_id: dm_channel_id.map(String::from),
        sender_id: sender_id.clone(),
        sender_device_id: None,
        plaintext: Some(plaintext.clone()),
        ciphertext: None,
        message_type: Some("file".into()),
        created_at: now_ms,
        edited_at: None,
        decrypted_at: Some(now_ms),
        status: MSG_STATUS_PENDING.into(),
    };

    {
        let conn = cache_db.lock()?;
        messages::insert_message(&conn, &cached)?;
        // Index the file name in FTS for search
        search::index_message(&conn, &msg_id_str, &file_name)?;

        // Insert file record in cache
        crate::cache::files::insert_file(
            &conn,
            &crate::cache::files::CachedFile {
                id: file_resp.id.clone(),
                message_id: Some(msg_id_str.clone()),
                file_name: file_name.clone(),
                file_size: file_size as i64,
                mime_type: Some(mime_type.clone()),
                local_path: Some(file_path.to_string()),
                thumbnail_path: None,
                created_at: now_ms,
            },
        )?;
    }

    // 11. Store nonce for dedup matching when server echoes back
    {
        let mut nonces = ws_state.pending_nonces.write().await;
        nonces.insert(client_nonce.clone(), msg_id_str.clone());
    }

    // 12. Check connectivity — only encrypt if we can send
    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        // 13. Encrypt the file message plaintext (contains file key) per-device
        let recipients = if let Some(ch_id) = channel_id {
            // For channel files: look up guild_id from channel_cache
            let guild_id = {
                let conn = cache_db.lock()?;
                conn.query_row(
                    "SELECT guild_id FROM channel_cache WHERE id = ?1",
                    [ch_id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|_| AppError::new("channel not found in cache — cannot resolve guild"))?
            };
            device_directory::encrypt_for_channel(app, &guild_id, &sender_id, plaintext.as_bytes())
                .await?
        } else if let Some(dm_id) = dm_channel_id {
            // For DM files: look up participant_ids from dm_channel_cache
            let participant_ids = {
                let conn = cache_db.lock()?;
                let dm = dm_channels::get_dm_channel(&conn, dm_id)?
                    .ok_or_else(|| AppError::new("DM channel not found in cache"))?;
                dm.participant_ids
            };
            device_directory::encrypt_for_dm(
                app,
                &participant_ids,
                &sender_id,
                plaintext.as_bytes(),
            )
            .await?
        } else {
            return Err(AppError::new("no channel or dm_channel specified"));
        };

        let ws_msg = ClientMessage::SendMessage {
            channel_id: channel_id
                .map(|id| id.parse::<ChannelId>())
                .transpose()
                .map_err(|_| AppError::new("invalid channel_id"))?,
            dm_channel_id: dm_channel_id
                .map(|id| id.parse::<DmChannelId>())
                .transpose()
                .map_err(|_| AppError::new("invalid dm_channel_id"))?,
            recipients,
            client_nonce: Some(client_nonce),
        };

        tx.send(ws_msg)
            .map_err(|_| AppError::new("failed to send file message: WebSocket channel closed"))?;
    } else {
        // Not connected -- enqueue plaintext for offline delivery.
        // Encryption will happen on retry when connectivity is restored.
        let conn = cache_db.lock()?;
        messages::update_message_status(&conn, &msg_id_str, MSG_STATUS_QUEUED)?;
        crate::cache::queue::enqueue_message(
            &conn,
            &msg_id_str,
            channel_id,
            dm_channel_id,
            &plaintext,
            now_ms,
        )?;
    }

    Ok(FileMetadata {
        file_id: file_resp.id,
        file_name,
        file_size,
        mime_type,
        local_path: Some(file_path.to_string()),
        thumbnail_path: None,
    })
}

/// Send an encrypted file attachment to a channel.
#[tauri::command]
#[specta::specta]
pub async fn send_file(
    app: AppHandle,
    channel_id: String,
    file_path: String,
    ws_state: State<'_, WsState>,
    cache_db: State<'_, CacheDb>,
    rate_limiter: State<'_, Mutex<MessageRateLimiter>>,
) -> Result<FileMetadata, AppError> {
    upload_and_send_file(
        &app,
        Some(&channel_id),
        None,
        &file_path,
        ws_state.inner(),
        cache_db.inner(),
        rate_limiter.inner(),
    )
    .await
}

/// Send an encrypted file attachment to a DM channel.
#[tauri::command]
#[specta::specta]
pub async fn send_dm_file(
    app: AppHandle,
    dm_channel_id: String,
    file_path: String,
    ws_state: State<'_, WsState>,
    cache_db: State<'_, CacheDb>,
    rate_limiter: State<'_, Mutex<MessageRateLimiter>>,
) -> Result<FileMetadata, AppError> {
    upload_and_send_file(
        &app,
        None,
        Some(&dm_channel_id),
        &file_path,
        ws_state.inner(),
        cache_db.inner(),
        rate_limiter.inner(),
    )
    .await
}

/// Download and decrypt a file attachment.
///
/// 1. Download encrypted blob from server
/// 2. Decrypt blob with file key via CryptoService::decrypt_file
/// 3. Store decrypted file in $APPDATA/attachments/
/// 4. Generate thumbnail for images
/// 5. Update cache and emit event
#[tauri::command]
#[specta::specta]
pub async fn download_file(
    app: AppHandle,
    file_id: String,
    file_name: String,
    file_key_base64: String,
    mime_type: String,
) -> Result<FileMetadata, AppError> {
    let access_token = tokio::task::spawn_blocking(crate::auth_service::get_access_token)
        .await
        .map_err(|e| AppError::new(format!("spawn_blocking: {e}")))?
        .map_err(|e| AppError::new(format!("auth: {e}")))?;

    let api_base_url =
        std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    // Download encrypted blob
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{api_base_url}/api/files/{file_id}"))
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| AppError::new(format!("download failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::new(format!("download failed ({status}): {body}")));
    }

    let encrypted_bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::new(format!("read response: {e}")))?
        .to_vec();

    // Decode the file key
    let file_key = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&file_key_base64)
            .map_err(|e| AppError::new(format!("invalid file key: {e}")))?
    };

    // Decrypt via spawn_blocking
    let app_clone = app.clone();
    let decrypted = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        crypto_state
            .crypto_service
            .decrypt_file(&encrypted_bytes, &file_key)
    })
    .await
    .map_err(|e| AppError::new(format!("spawn_blocking failed: {e}")))?
    .map_err(|e| AppError::new(e.message))?;

    // Determine file extension from name
    let ext = PathBuf::from(&file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_string();

    // Store decrypted file (async I/O)
    let attach_dir = attachments_dir(&app)?;
    tokio::fs::create_dir_all(&attach_dir)
        .await
        .map_err(|e| AppError::new(format!("create attachments dir: {e}")))?;

    let local_path = attach_dir.join(format!("{file_id}.{ext}"));
    tokio::fs::write(&local_path, &decrypted)
        .await
        .map_err(|e| AppError::new(format!("write file: {e}")))?;

    // Generate thumbnail for images
    let thumbnail_path = if is_thumbnail_supported(&mime_type) {
        let thumb_dir = thumbnails_dir(&app)?;
        tokio::fs::create_dir_all(&thumb_dir)
            .await
            .map_err(|e| AppError::new(format!("create thumbnails dir: {e}")))?;

        let thumb_path = thumb_dir.join(format!("{file_id}_thumb.jpg"));
        let local_clone = local_path.clone();
        let thumb_clone = thumb_path.clone();

        tokio::task::spawn_blocking(move || {
            generate_thumbnail_sync(&local_clone, &thumb_clone, THUMBNAIL_MAX_DIM)
        })
        .await
        .map_err(|e| AppError::new(format!("spawn_blocking thumbnail: {e}")))?
        .map_err(|e| AppError::new(e.message))?;

        Some(thumb_path.to_string_lossy().to_string())
    } else {
        None
    };

    let file_size = decrypted.len() as u64;
    let local_path_str = local_path.to_string_lossy().to_string();

    // Update cache with local paths
    {
        let cache_db = app.state::<CacheDb>();
        let conn = cache_db
            .lock()
            .map_err(|_| AppError::new("cache lock poisoned".to_string()))?;
        crate::cache::files::update_local_paths(
            &conn,
            &file_id,
            &local_path_str,
            thumbnail_path.as_deref(),
        )
        .map_err(|e| AppError::new(format!("cache update: {e}")))?;
    }

    // Emit event so frontend can render the attachment
    let _ = app.emit(
        "ws:file_ready",
        serde_json::json!({
            "fileId": file_id,
            "localPath": local_path_str,
            "thumbnailPath": thumbnail_path,
        }),
    );

    Ok(FileMetadata {
        file_id,
        file_name,
        file_size,
        mime_type,
        local_path: Some(local_path_str),
        thumbnail_path,
    })
}

/// Generate a JPEG thumbnail for an image file.
#[tauri::command]
#[specta::specta]
pub async fn generate_thumbnail(
    app: AppHandle,
    file_id: String,
    source_path: String,
) -> Result<String, AppError> {
    let source = PathBuf::from(&source_path);
    let mime = detect_mime_type(&source);

    if !is_thumbnail_supported(&mime) {
        return Err(AppError::new(
            "unsupported image type for thumbnail generation".to_string(),
        ));
    }

    let thumb_dir = thumbnails_dir(&app)?;
    tokio::fs::create_dir_all(&thumb_dir)
        .await
        .map_err(|e| AppError::new(format!("create thumbnails dir: {e}")))?;

    let output_path = thumb_dir.join(format!("{file_id}_thumb.jpg"));
    let output_clone = output_path.clone();

    tokio::task::spawn_blocking(move || {
        generate_thumbnail_sync(&source, &output_clone, THUMBNAIL_MAX_DIM)
    })
    .await
    .map_err(|e| AppError::new(format!("spawn_blocking: {e}")))?
    .map_err(|e| AppError::new(e.message))?;

    Ok(output_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_file tests ---

    #[test]
    fn validate_file_rejects_nonexistent_path() {
        let result = validate_file(std::path::Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("file not found"));
    }

    #[test]
    fn validate_file_rejects_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");

        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_FILE_SIZE + 1).unwrap();

        let result = validate_file(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("exceeds maximum size"));
    }

    #[test]
    fn validate_file_accepts_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.txt");
        std::fs::write(&path, "hello world").unwrap();

        let size = validate_file(&path).unwrap();
        assert_eq!(size, 11);
    }

    #[test]
    fn validate_file_rejects_directory() {
        let dir = tempfile::tempdir().unwrap();
        let result = validate_file(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("not a file"));
    }

    // --- detect_mime_type tests ---

    #[test]
    fn detect_mime_type_recognizes_common_types() {
        assert_eq!(
            detect_mime_type(std::path::Path::new("photo.png")),
            "image/png"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("photo.jpg")),
            "image/jpeg"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("photo.JPEG")),
            "image/jpeg"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("anim.gif")),
            "image/gif"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("image.webp")),
            "image/webp"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("doc.pdf")),
            "application/pdf"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("readme.txt")),
            "text/plain"
        );
    }

    #[test]
    fn detect_mime_type_distinguishes_doc_from_docx() {
        assert_eq!(
            detect_mime_type(std::path::Path::new("file.doc")),
            "application/msword"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("file.docx")),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
    }

    #[test]
    fn detect_mime_type_defaults_to_octet_stream() {
        assert_eq!(
            detect_mime_type(std::path::Path::new("file.xyz")),
            "application/octet-stream"
        );
        assert_eq!(
            detect_mime_type(std::path::Path::new("noext")),
            "application/octet-stream"
        );
    }

    // --- is_thumbnail_supported tests ---

    #[test]
    fn thumbnail_supported_for_image_types() {
        assert!(is_thumbnail_supported("image/png"));
        assert!(is_thumbnail_supported("image/jpeg"));
        assert!(is_thumbnail_supported("image/gif"));
        assert!(is_thumbnail_supported("image/webp"));
    }

    #[test]
    fn thumbnail_not_supported_for_non_image_types() {
        assert!(!is_thumbnail_supported("application/pdf"));
        assert!(!is_thumbnail_supported("text/plain"));
        assert!(!is_thumbnail_supported("image/svg+xml"));
        assert!(!is_thumbnail_supported("application/octet-stream"));
    }

    // --- generate_thumbnail_sync tests ---

    #[test]
    fn thumbnail_generates_jpeg_from_png() {
        let dir = tempfile::tempdir().unwrap();

        let img = image::RgbImage::from_fn(100, 50, |_, _| image::Rgb([255, 0, 0]));
        let source = dir.path().join("test.png");
        img.save(&source).unwrap();

        let output = dir.path().join("thumb.jpg");
        generate_thumbnail_sync(&source, &output, 200).unwrap();

        assert!(output.exists());

        let thumb = image::open(&output).unwrap();
        assert!(thumb.width() <= 200);
        assert!(thumb.height() <= 200);
    }

    #[test]
    fn thumbnail_preserves_aspect_ratio() {
        let dir = tempfile::tempdir().unwrap();

        let img = image::RgbImage::from_fn(1000, 500, |_, _| image::Rgb([0, 128, 255]));
        let source = dir.path().join("wide.png");
        img.save(&source).unwrap();

        let output = dir.path().join("thumb.jpg");
        generate_thumbnail_sync(&source, &output, 200).unwrap();

        let thumb = image::open(&output).unwrap();
        assert_eq!(thumb.width(), 200);
        assert_eq!(thumb.height(), 100);
    }

    #[test]
    fn thumbnail_rejects_non_image() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("readme.txt");
        std::fs::write(&source, "not an image").unwrap();

        let output = dir.path().join("thumb.jpg");
        let result = generate_thumbnail_sync(&source, &output, 200);
        assert!(result.is_err());
    }

    #[test]
    fn thumbnail_creates_output_directory() {
        let dir = tempfile::tempdir().unwrap();

        let img = image::RgbImage::from_fn(50, 50, |_, _| image::Rgb([0, 255, 0]));
        let source = dir.path().join("test.png");
        img.save(&source).unwrap();

        let output = dir.path().join("nested").join("dir").join("thumb.jpg");
        generate_thumbnail_sync(&source, &output, 200).unwrap();
        assert!(output.exists());
    }

    #[test]
    fn thumbnail_small_image_generates_output() {
        let dir = tempfile::tempdir().unwrap();

        let img = image::RgbImage::from_fn(20, 10, |_, _| image::Rgb([128, 128, 128]));
        let source = dir.path().join("tiny.png");
        img.save(&source).unwrap();

        let output = dir.path().join("thumb.jpg");
        generate_thumbnail_sync(&source, &output, 200).unwrap();

        assert!(output.exists());
        let thumb = image::open(&output).unwrap();
        assert!(thumb.width() > 0);
        assert!(thumb.height() > 0);
    }

    // --- FileMessagePayload tests ---

    #[test]
    fn file_message_payload_round_trip() {
        let payload = FileMessagePayload {
            r#type: "file".into(),
            file_id: "f-123".into(),
            file_name: "test.png".into(),
            file_size: 1024,
            mime_type: "image/png".into(),
            file_key_base64: "dGVzdGtleQ==".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"file\""));
        assert!(json.contains("\"fileId\":\"f-123\""));

        let back: FileMessagePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.file_id, "f-123");
        assert_eq!(back.file_key_base64, "dGVzdGtleQ==");
    }
}
