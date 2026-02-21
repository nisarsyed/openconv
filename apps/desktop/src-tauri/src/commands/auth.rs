use tauri::State;

use crate::auth_service::{get_or_create_device_id, AppError, AuthResult, AuthState};
use crate::DbState;

#[tauri::command]
#[specta::specta]
pub async fn auth_register_start(
    email: String,
    display_name: String,
    state: State<'_, AuthState>,
) -> Result<(), AppError> {
    state
        .auth_service
        .register_start(email, display_name)
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_verify_email(
    email: String,
    code: String,
    state: State<'_, AuthState>,
) -> Result<String, AppError> {
    state.auth_service.register_verify(email, code).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_register_complete(
    registration_token: String,
    display_name: String,
    auth: State<'_, AuthState>,
    db: State<'_, DbState>,
) -> Result<AuthResult, AppError> {
    let (device_id, device_name) = {
        let conn = db.conn.lock().map_err(|e| AppError::new(e.to_string()))?;
        get_or_create_device_id(&conn)?
    };
    auth.auth_service
        .register_complete(registration_token, display_name, device_id, device_name)
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_login(
    auth: State<'_, AuthState>,
    db: State<'_, DbState>,
) -> Result<AuthResult, AppError> {
    let (device_id, device_name) = {
        let conn = db.conn.lock().map_err(|e| AppError::new(e.to_string()))?;
        get_or_create_device_id(&conn)?
    };
    auth.auth_service.login(device_id, device_name).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_refresh(state: State<'_, AuthState>) -> Result<(), AppError> {
    state.auth_service.refresh().await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_logout(state: State<'_, AuthState>) -> Result<(), AppError> {
    state.auth_service.logout().await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_recover_start(
    email: String,
    state: State<'_, AuthState>,
) -> Result<(), AppError> {
    state.auth_service.recover_start(email).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_recover_verify(
    email: String,
    code: String,
    state: State<'_, AuthState>,
) -> Result<String, AppError> {
    state.auth_service.recover_verify(email, code).await
}

#[tauri::command]
#[specta::specta]
pub async fn auth_recover_complete(
    recovery_token: String,
    auth: State<'_, AuthState>,
    db: State<'_, DbState>,
) -> Result<AuthResult, AppError> {
    let (device_id, device_name) = {
        let conn = db.conn.lock().map_err(|e| AppError::new(e.to_string()))?;
        get_or_create_device_id(&conn)?
    };
    auth.auth_service
        .recover_complete(recovery_token, device_id, device_name)
        .await
}

#[tauri::command]
#[specta::specta]
pub fn auth_check_identity(state: State<'_, AuthState>) -> Result<bool, AppError> {
    state.auth_service.check_identity()
}

#[tauri::command]
#[specta::specta]
pub fn auth_get_public_key(state: State<'_, AuthState>) -> Result<String, AppError> {
    state.auth_service.get_public_key()
}
