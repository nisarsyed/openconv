use std::collections::HashSet;

use openconv_shared::api::auth::{DeviceInfo as SharedDeviceInfo, DevicesListResponse};
use openconv_shared::api::guild::GuildMemberResponse;
use openconv_shared::api::ws::RecipientPayload as SharedRecipientPayload;
use openconv_shared::ids::UserId;
use tauri::{AppHandle, Manager};

use crate::auth_service::{self, AppError};
use crate::crypto_service::{self, CryptoState};

/// A member-device pair ready for the encryption pipeline.
///
/// Carries both the shared UUID-based IDs (for building the WS `RecipientPayload`)
/// and the mapped signal device ID (for `crypto_service::DeviceInfo`).
#[derive(Debug, Clone)]
pub struct MemberDevice {
    pub user_id: UserId,
    pub device_id: openconv_shared::ids::DeviceId,
    pub signal_device_id: u32,
}

impl MemberDevice {
    /// Convert to the `crypto_service::DeviceInfo` expected by the encryption methods.
    pub fn to_crypto_device_info(&self) -> crypto_service::DeviceInfo {
        crypto_service::DeviceInfo {
            user_id: self.user_id.to_string(),
            device_id: self.signal_device_id,
        }
    }
}

/// Fetch all member-device pairs for a guild channel.
///
/// 1. `GET /api/guilds/{guild_id}/members` → member list
/// 2. For each member (except self), `GET /api/users/{user_id}/devices` → device list
/// 3. Map each device to a `MemberDevice` with its signal device ID
pub async fn fetch_channel_member_devices(
    http_client: &reqwest::Client,
    api_base_url: &str,
    access_token: &str,
    guild_id: &str,
    self_user_id: &str,
) -> Result<Vec<MemberDevice>, AppError> {
    let members: Vec<GuildMemberResponse> = http_client
        .get(format!("{api_base_url}/api/guilds/{guild_id}/members"))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::new(format!("failed to fetch guild members: {e}")))?
        .json()
        .await?;

    let mut result = Vec::new();
    for member in &members {
        if member.user_id.to_string() == self_user_id {
            continue;
        }

        let devices =
            fetch_user_devices(http_client, api_base_url, access_token, &member.user_id).await?;

        for device in &devices {
            result.push(MemberDevice {
                user_id: member.user_id,
                device_id: device.id,
                signal_device_id: device.signal_device_id,
            });
        }
    }

    Ok(result)
}

/// Fetch all member-device pairs for a DM channel's recipients.
pub async fn fetch_dm_recipient_devices(
    http_client: &reqwest::Client,
    api_base_url: &str,
    access_token: &str,
    participant_ids: &[String],
    self_user_id: &str,
) -> Result<Vec<MemberDevice>, AppError> {
    let mut result = Vec::new();

    for participant_id in participant_ids {
        if participant_id == self_user_id {
            continue;
        }

        let user_id: UserId = participant_id
            .parse()
            .map_err(|_| AppError::new(format!("invalid participant user_id: {participant_id}")))?;

        let devices = fetch_user_devices(http_client, api_base_url, access_token, &user_id).await?;

        for device in &devices {
            result.push(MemberDevice {
                user_id,
                device_id: device.id,
                signal_device_id: device.signal_device_id,
            });
        }
    }

    Ok(result)
}

/// Fetch all devices for a single user.
async fn fetch_user_devices(
    http_client: &reqwest::Client,
    api_base_url: &str,
    access_token: &str,
    user_id: &UserId,
) -> Result<Vec<SharedDeviceInfo>, AppError> {
    let resp: DevicesListResponse = http_client
        .get(format!("{api_base_url}/api/users/{}/devices", user_id.0))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::new(format!("failed to fetch devices for {user_id}: {e}")))?
        .json()
        .await?;

    Ok(resp.devices)
}

/// Fetch a pre-key bundle for a user from the server.
///
/// Returns the raw bundle bytes suitable for passing to
/// `CryptoService::ensure_session`.
pub async fn fetch_prekey_bundle(
    http_client: &reqwest::Client,
    api_base_url: &str,
    access_token: &str,
    user_id: &UserId,
) -> Result<Vec<u8>, AppError> {
    #[derive(serde::Deserialize)]
    struct PreKeyBundleResponse {
        key_data: Vec<u8>,
    }

    let resp: PreKeyBundleResponse = http_client
        .get(format!("{api_base_url}/api/users/{}/prekeys", user_id.0))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::new(format!("failed to fetch prekey bundle for {user_id}: {e}")))?
        .json()
        .await?;

    Ok(resp.key_data)
}

// ─── Full encryption pipeline ────────────────────────────────

/// Encrypt plaintext for a guild channel's members.
///
/// Fetches member devices, establishes Signal sessions, encrypts per-device,
/// and returns shared `RecipientPayload` entries ready for the WS message.
pub async fn encrypt_for_channel(
    app: &AppHandle,
    guild_id: &str,
    sender_id: &str,
    plaintext: &[u8],
) -> Result<Vec<SharedRecipientPayload>, AppError> {
    let (http_client, api_base_url, access_token) = prepare_http_context().await?;

    let member_devices = fetch_channel_member_devices(
        &http_client,
        &api_base_url,
        &access_token,
        guild_id,
        sender_id,
    )
    .await?;

    if member_devices.is_empty() {
        return Err(AppError::new(
            "no recipients: channel has no other members with devices",
        ));
    }

    establish_sessions_and_encrypt(
        app,
        &http_client,
        &api_base_url,
        &access_token,
        &member_devices,
        plaintext,
    )
    .await
}

/// Encrypt plaintext for a DM channel's recipients.
pub async fn encrypt_for_dm(
    app: &AppHandle,
    participant_ids: &[String],
    sender_id: &str,
    plaintext: &[u8],
) -> Result<Vec<SharedRecipientPayload>, AppError> {
    let (http_client, api_base_url, access_token) = prepare_http_context().await?;

    let member_devices = fetch_dm_recipient_devices(
        &http_client,
        &api_base_url,
        &access_token,
        participant_ids,
        sender_id,
    )
    .await?;

    if member_devices.is_empty() {
        return Err(AppError::new(
            "no recipients: DM channel has no other participants with devices",
        ));
    }

    establish_sessions_and_encrypt(
        app,
        &http_client,
        &api_base_url,
        &access_token,
        &member_devices,
        plaintext,
    )
    .await
}

/// Get HTTP context (client, base URL, access token) for API calls.
pub async fn prepare_http_context() -> Result<(reqwest::Client, String, String), AppError> {
    let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
        .await
        .map_err(|e| AppError::new(format!("internal error: {e}")))?
        .map_err(|_| AppError::new("not authenticated"))?;

    let api_base_url =
        std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let http_client = reqwest::Client::new();

    Ok((http_client, api_base_url, access_token))
}

/// Core pipeline: deduplicate devices, fetch pre-key bundles, ensure sessions,
/// encrypt, and convert to shared RecipientPayload.
async fn establish_sessions_and_encrypt(
    app: &AppHandle,
    http_client: &reqwest::Client,
    api_base_url: &str,
    access_token: &str,
    member_devices: &[MemberDevice],
    plaintext: &[u8],
) -> Result<Vec<SharedRecipientPayload>, AppError> {
    // Deduplicate by (user_id, signal_device_id) to avoid ratchet skew
    let mut seen = HashSet::new();
    let unique_devices: Vec<_> = member_devices
        .iter()
        .filter(|md| seen.insert((md.user_id, md.signal_device_id)))
        .cloned()
        .collect();

    // Fetch pre-key bundles for each unique target
    let mut bundles = Vec::with_capacity(unique_devices.len());
    for md in &unique_devices {
        let bundle =
            fetch_prekey_bundle(http_client, api_base_url, access_token, &md.user_id).await?;
        bundles.push(bundle);
    }

    // Establish sessions and encrypt (blocking crypto operations)
    let crypto_devices: Vec<_> = unique_devices
        .iter()
        .map(|md| md.to_crypto_device_info())
        .collect();
    let plaintext_owned = plaintext.to_vec();
    let app_clone = app.clone();

    let crypto_payloads = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        for (i, device) in crypto_devices.iter().enumerate() {
            crypto_state.crypto_service.ensure_session(
                &device.user_id,
                device.device_id,
                Some(&bundles[i]),
            )?;
        }
        crypto_state
            .crypto_service
            .encrypt_for_channel(&crypto_devices, &plaintext_owned)
    })
    .await
    .map_err(|e| AppError::new(format!("internal error: {e}")))??;

    // Convert local RecipientPayload → shared RecipientPayload (with UUID IDs)
    let recipients: Vec<SharedRecipientPayload> = crypto_payloads
        .into_iter()
        .zip(unique_devices.iter())
        .map(|(cp, md)| SharedRecipientPayload {
            user_id: md.user_id,
            device_id: md.device_id,
            ciphertext: cp.ciphertext,
            message_type: cp.message_type,
        })
        .collect();

    Ok(recipients)
}
