use std::path::PathBuf;
use std::sync::Mutex;

use base64::Engine;
use openconv_crypto::{identity, master_key, prekeys, storage::CryptoStore};
use openconv_shared::api::auth::*;
use openconv_shared::ids::DeviceId;
use reqwest::Client;
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Error & Result types
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, specta::Type)]
pub struct AppError {
    pub message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<openconv_crypto::error::CryptoError> for AppError {
    fn from(e: openconv_crypto::error::CryptoError) -> Self {
        Self::new(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::new("request timed out")
        } else if e.is_connect() {
            Self::new("could not connect to server")
        } else {
            Self::new("network request failed")
        }
    }
}

impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self {
        Self::new(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(e.to_string())
    }
}

impl From<base64::DecodeError> for AppError {
    fn from(e: base64::DecodeError) -> Self {
        Self::new(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new(e.to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct AuthResult {
    pub user_id: String,
    pub public_key: String,
    pub device_id: String,
}

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

pub struct AuthState {
    pub auth_service: AuthService,
}

// ---------------------------------------------------------------------------
// Token storage (OS keychain)
// ---------------------------------------------------------------------------

const KEYRING_SERVICE: &str = "com.openconv.auth";

fn store_tokens(access_token: &str, refresh_token: &str) -> Result<(), AppError> {
    keyring::Entry::new(KEYRING_SERVICE, "access_token")?
        .set_password(access_token)?;
    keyring::Entry::new(KEYRING_SERVICE, "refresh_token")?
        .set_password(refresh_token)?;
    Ok(())
}

fn get_access_token() -> Result<String, AppError> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, "access_token")?.get_password()?)
}

fn get_refresh_token() -> Result<String, AppError> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, "refresh_token")?.get_password()?)
}

fn clear_tokens() -> Result<(), AppError> {
    let _ = keyring::Entry::new(KEYRING_SERVICE, "access_token")
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new(KEYRING_SERVICE, "refresh_token")
        .and_then(|e| e.delete_credential());
    Ok(())
}

// ---------------------------------------------------------------------------
// Device ID management (cache DB)
// ---------------------------------------------------------------------------

fn default_device_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .into_owned()
}

pub fn get_or_create_device_id(
    conn: &Connection,
) -> Result<(DeviceId, String), AppError> {
    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT id, device_name FROM local_device LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((id_str, name)) = existing {
        let id: DeviceId = id_str
            .parse()
            .map_err(|_| AppError::new("invalid stored device_id"))?;
        return Ok((id, name));
    }

    let id = DeviceId::new();
    let name = default_device_name();
    conn.execute(
        "INSERT INTO local_device (id, device_name) VALUES (?1, ?2)",
        rusqlite::params![id.to_string(), &name],
    )?;
    Ok((id, name))
}

// ---------------------------------------------------------------------------
// AuthService
// ---------------------------------------------------------------------------

/// Parse a user-safe error message from a non-success HTTP response.
async fn error_from_response(resp: reqwest::Response, context: &str) -> AppError {
    #[derive(serde::Deserialize)]
    struct ServerError {
        error: String,
    }
    let status = resp.status();
    if let Ok(body) = resp.json::<ServerError>().await {
        AppError::new(body.error)
    } else {
        AppError::new(format!("{context} (HTTP {status})"))
    }
}

pub struct AuthService {
    crypto_conn: Mutex<Connection>,
    api_base_url: String,
    http_client: Client,
}

impl AuthService {
    /// Create a new AuthService. Opens and encrypts the crypto DB.
    pub fn new(crypto_db_path: PathBuf, api_base_url: String) -> Result<Self, AppError> {
        let conn = Connection::open(&crypto_db_path)
            .map_err(|e| AppError::new(format!("failed to open crypto DB: {e}")))?;

        let mk = master_key::init_master_key_from_keychain()
            .map_err(|e| AppError::new(format!("failed to init master key: {e}")))?;
        let db_key = master_key::derive_db_encryption_key(&mk)
            .map_err(|e| AppError::new(format!("failed to derive DB key: {e}")))?;
        master_key::apply_encryption_key(&conn, &db_key)
            .map_err(|e| AppError::new(format!("failed to apply encryption: {e}")))?;

        CryptoStore::new(&conn)
            .run_migrations()
            .map_err(|e| AppError::new(format!("failed to run crypto migrations: {e}")))?;

        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| AppError::new(format!("failed to create HTTP client: {e}")))?;

        Ok(Self {
            crypto_conn: Mutex::new(conn),
            api_base_url,
            http_client,
        })
    }

    /// Create an AuthService for testing (no encryption).
    #[cfg(test)]
    pub fn new_for_testing(api_base_url: String) -> Self {
        let conn = Connection::open_in_memory().unwrap();
        CryptoStore::new(&conn).run_migrations().unwrap();
        Self {
            crypto_conn: Mutex::new(conn),
            api_base_url,
            http_client: Client::new(),
        }
    }

    fn lock_crypto(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.crypto_conn
            .lock()
            .map_err(|e| AppError::new(format!("crypto DB lock poisoned: {e}")))
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}{path}", self.api_base_url)
    }

    // -- Registration flow --------------------------------------------------

    pub async fn register_start(
        &self,
        email: String,
        display_name: String,
    ) -> Result<(), AppError> {
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/register/start"))
            .json(&RegisterStartRequest {
                email,
                display_name,
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "registration start failed").await);
        }
        Ok(())
    }

    pub async fn register_verify(
        &self,
        email: String,
        code: String,
    ) -> Result<String, AppError> {
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/register/verify"))
            .json(&RegisterVerifyRequest { email, code })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "verification failed").await);
        }

        let data: RegisterVerifyResponse = resp.json().await?;
        Ok(data.registration_token)
    }

    pub async fn register_complete(
        &self,
        registration_token: String,
        display_name: String,
        device_id: DeviceId,
        device_name: String,
    ) -> Result<AuthResult, AppError> {
        let b64 = base64::engine::general_purpose::STANDARD;

        // Sync: generate identity keypair + pre-key bundle
        let (public_key, bundle_b64) = {
            let conn = self.lock_crypto()?;
            // Clear any stale identity from a previously failed registration attempt
            conn.execute("DELETE FROM crypto_identity_keys", [])
                .map_err(|e| AppError::new(format!("failed to clear old identity: {e}")))?;
            identity::generate_identity(&conn)?;
            let pk = identity::get_public_key_string(&conn)?;
            // Use display_name as a placeholder user_id for bundle generation
            // (the real user_id is assigned server-side)
            let bundle = prekeys::generate_pre_key_bundle(&conn, &display_name)?;
            let bundle_json = serde_json::to_vec(&bundle)?;
            (pk, b64.encode(&bundle_json))
        };

        // Async: complete registration
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/register/complete"))
            .json(&RegisterCompleteRequest {
                registration_token,
                public_key: public_key.clone(),
                pre_key_bundle: bundle_b64,
                device_id,
                device_name,
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "registration complete failed").await);
        }

        let data: RegisterResponse = resp.json().await?;
        store_tokens(&data.access_token, &data.refresh_token)?;

        Ok(AuthResult {
            user_id: data.user_id.to_string(),
            public_key,
            device_id: data.device_id.to_string(),
        })
    }

    // -- Login flow ---------------------------------------------------------

    pub async fn login(
        &self,
        device_id: DeviceId,
        device_name: String,
    ) -> Result<AuthResult, AppError> {
        let b64 = base64::engine::general_purpose::STANDARD;

        // Sync: get public key
        let public_key = {
            let conn = self.lock_crypto()?;
            identity::get_public_key_string(&conn)?
        };

        // Async: request challenge
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/challenge"))
            .json(&LoginChallengeRequest {
                public_key: public_key.clone(),
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "challenge request failed").await);
        }

        let challenge_resp: LoginChallengeResponse = resp.json().await?;
        let challenge_bytes = b64.decode(&challenge_resp.challenge)?;

        // Sync: sign challenge
        let signature = {
            let conn = self.lock_crypto()?;
            let sig_bytes = identity::sign_challenge(&conn, &challenge_bytes)?;
            b64.encode(&sig_bytes)
        };

        // Async: verify signature
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/verify"))
            .json(&LoginVerifyRequest {
                public_key: public_key.clone(),
                signature,
                device_id,
                device_name,
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "login verification failed").await);
        }

        let data: LoginVerifyResponse = resp.json().await?;
        store_tokens(&data.access_token, &data.refresh_token)?;

        Ok(AuthResult {
            user_id: data.user_id.to_string(),
            public_key,
            device_id: data.device_id.to_string(),
        })
    }

    // -- Token refresh ------------------------------------------------------

    pub async fn refresh(&self) -> Result<(), AppError> {
        let refresh_token = get_refresh_token()?;

        let resp = self
            .http_client
            .post(self.api_url("/api/auth/refresh"))
            .json(&RefreshRequest { refresh_token })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "token refresh failed").await);
        }

        let data: RefreshResponse = resp.json().await?;
        store_tokens(&data.access_token, &data.refresh_token)?;
        Ok(())
    }

    // -- Logout -------------------------------------------------------------

    pub async fn logout(&self) -> Result<(), AppError> {
        let access_token = get_access_token().ok();

        if let Some(token) = access_token {
            let _ = self
                .http_client
                .post(self.api_url("/api/auth/logout"))
                .bearer_auth(&token)
                .send()
                .await;
        }

        clear_tokens()?;
        Ok(())
    }

    // -- Recovery flow ------------------------------------------------------

    pub async fn recover_start(&self, email: String) -> Result<(), AppError> {
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/recover/start"))
            .json(&RecoverStartRequest { email })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "recovery start failed").await);
        }
        Ok(())
    }

    pub async fn recover_verify(
        &self,
        email: String,
        code: String,
    ) -> Result<String, AppError> {
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/recover/verify"))
            .json(&RecoverVerifyRequest { email, code })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "recovery verify failed").await);
        }

        let data: RecoverVerifyResponse = resp.json().await?;
        Ok(data.recovery_token)
    }

    pub async fn recover_complete(
        &self,
        recovery_token: String,
        device_id: DeviceId,
        device_name: String,
    ) -> Result<AuthResult, AppError> {
        let b64 = base64::engine::general_purpose::STANDARD;

        // Sync: generate new identity keypair (overwrite old)
        let (new_public_key, new_bundle_b64) = {
            let conn = self.lock_crypto()?;
            // Delete old identity to allow regeneration
            conn.execute("DELETE FROM crypto_identity_keys", [])
                .map_err(|e| AppError::new(format!("failed to clear old identity: {e}")))?;
            identity::generate_identity(&conn)?;
            let pk = identity::get_public_key_string(&conn)?;
            let bundle = prekeys::generate_pre_key_bundle(&conn, "recovery")?;
            let bundle_json = serde_json::to_vec(&bundle)?;
            (pk, b64.encode(&bundle_json))
        };

        // Async: complete recovery
        let resp = self
            .http_client
            .post(self.api_url("/api/auth/recover/complete"))
            .json(&RecoverCompleteRequest {
                recovery_token,
                new_public_key: new_public_key.clone(),
                new_pre_key_bundle: new_bundle_b64,
                device_id,
                device_name,
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(error_from_response(resp, "recovery complete failed").await);
        }

        let data: RecoverCompleteResponse = resp.json().await?;
        store_tokens(&data.access_token, &data.refresh_token)?;

        Ok(AuthResult {
            user_id: data.user_id.to_string(),
            public_key: new_public_key,
            device_id: data.device_id.to_string(),
        })
    }

    // -- Identity queries (sync) --------------------------------------------

    pub fn check_identity(&self) -> Result<bool, AppError> {
        let conn = self.lock_crypto()?;
        match identity::get_public_key_string(&conn) {
            Ok(_) => Ok(true),
            Err(openconv_crypto::error::CryptoError::IdentityNotInitialized) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_public_key(&self) -> Result<String, AppError> {
        let conn = self.lock_crypto()?;
        Ok(identity::get_public_key_string(&conn)?)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_device_id_created_and_persisted() {
        let conn = test_cache_db();
        let (id1, name1) = get_or_create_device_id(&conn).unwrap();
        let (id2, name2) = get_or_create_device_id(&conn).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(name1, name2);
    }

    #[test]
    fn test_device_id_different_across_dbs() {
        let conn1 = test_cache_db();
        let conn2 = test_cache_db();
        let (id1, _) = get_or_create_device_id(&conn1).unwrap();
        let (id2, _) = get_or_create_device_id(&conn2).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_device_name_not_empty() {
        let conn = test_cache_db();
        let (_, name) = get_or_create_device_id(&conn).unwrap();
        assert!(!name.is_empty());
    }

    #[test]
    fn test_check_identity_no_identity() {
        let svc = AuthService::new_for_testing("http://localhost:0".to_string());
        assert!(!svc.check_identity().unwrap());
    }

    #[test]
    fn test_check_identity_after_generate() {
        let svc = AuthService::new_for_testing("http://localhost:0".to_string());
        {
            let conn = svc.lock_crypto().unwrap();
            identity::generate_identity(&conn).unwrap();
        }
        assert!(svc.check_identity().unwrap());
    }

    #[test]
    fn test_get_public_key_no_identity() {
        let svc = AuthService::new_for_testing("http://localhost:0".to_string());
        assert!(svc.get_public_key().is_err());
    }

    #[test]
    fn test_get_public_key_after_generate() {
        let svc = AuthService::new_for_testing("http://localhost:0".to_string());
        {
            let conn = svc.lock_crypto().unwrap();
            identity::generate_identity(&conn).unwrap();
        }
        let pk = svc.get_public_key().unwrap();
        assert!(!pk.is_empty());
    }

    #[test]
    fn test_app_error_serializes() {
        let err = AppError::new("test error");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("test error"));
    }

    #[test]
    fn test_auth_result_roundtrip() {
        let result = AuthResult {
            user_id: "u1".into(),
            public_key: "pk1".into(),
            device_id: "d1".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: AuthResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, "u1");
        assert_eq!(back.public_key, "pk1");
        assert_eq!(back.device_id, "d1");
    }
}
