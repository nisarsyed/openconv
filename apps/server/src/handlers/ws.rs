use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, State};
use axum::response::Response;
use axum::Json;
use fred::prelude::*;
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{DeviceId, UserId};
use serde::{Deserialize, Serialize};

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;
use crate::ws::connection::handle_connection;

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct WsQueryParams {
    pub ticket: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TicketResponse {
    pub ticket: String,
}

#[derive(Serialize, Deserialize)]
struct TicketData {
    user_id: UserId,
    device_id: DeviceId,
}

#[utoipa::path(post, path = "/api/ws/ticket", tag = "WebSocket", security(("bearer_auth" = [])), responses((status = 200, body = TicketResponse), (status = 401, body = crate::error::ErrorResponse)))]
/// POST /api/ws/ticket -- Issue a single-use WebSocket ticket.
pub async fn create_ws_ticket(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<TicketResponse>, ServerError> {
    let ticket_id = uuid::Uuid::new_v4().to_string();
    let key = format!("ws:ticket:{ticket_id}");

    let data = TicketData {
        user_id: auth.user_id,
        device_id: auth.device_id,
    };

    let value = serde_json::to_string(&data)
        .map_err(|e| ServerError(OpenConvError::Internal(format!("serialize ticket: {e}"))))?;

    state
        .redis
        .set::<(), _, _>(&key, value.as_str(), Some(Expiration::EX(30)), None, false)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to store ws ticket in redis");
            ServerError(OpenConvError::Internal("ticket storage failed".into()))
        })?;

    Ok(Json(TicketResponse { ticket: ticket_id }))
}

#[utoipa::path(get, path = "/ws", tag = "WebSocket", params(WsQueryParams), responses((status = 101, description = "WebSocket upgrade"), (status = 401, body = crate::error::ErrorResponse)))]
/// GET /ws?ticket=<uuid> -- Upgrade to WebSocket.
pub async fn ws_upgrade(
    State(state): State<AppState>,
    Query(params): Query<WsQueryParams>,
    ws: WebSocketUpgrade,
) -> Result<Response, ServerError> {
    // Validate ticket is a valid UUID
    if uuid::Uuid::parse_str(&params.ticket).is_err() {
        return Err(ServerError(OpenConvError::Unauthorized));
    }

    let key = format!("ws:ticket:{}", params.ticket);

    // Atomic get-and-delete (single-use)
    let data: Option<String> = state.redis.getdel(&key).await.map_err(|e| {
        tracing::error!(error = %e, "failed to consume ws ticket from redis");
        ServerError(OpenConvError::Internal("ticket validation failed".into()))
    })?;

    let data = data.ok_or(ServerError(OpenConvError::Unauthorized))?;

    let ticket: TicketData = serde_json::from_str(&data).map_err(|e| {
        tracing::error!(error = %e, "failed to parse ticket data");
        ServerError(OpenConvError::Internal("invalid ticket data".into()))
    })?;

    let user_id = ticket.user_id;
    let device_id = ticket.device_id;

    Ok(ws.on_upgrade(move |socket| handle_connection(socket, state, user_id, device_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_data_round_trip() {
        let data = TicketData {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: TicketData = serde_json::from_str(&json).unwrap();
        assert_eq!(data.user_id, back.user_id);
        assert_eq!(data.device_id, back.device_id);
    }

    #[test]
    fn ticket_response_serializes_correctly() {
        let resp = TicketResponse {
            ticket: "abc-123".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn ws_query_params_deserializes() {
        let json = r#"{"ticket": "some-uuid"}"#;
        let params: WsQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.ticket, "some-uuid");
    }

    #[test]
    fn invalid_uuid_is_rejected() {
        assert!(uuid::Uuid::parse_str("not-a-uuid").is_err());
    }

    #[test]
    fn valid_uuid_is_accepted() {
        let uuid = uuid::Uuid::new_v4().to_string();
        assert!(uuid::Uuid::parse_str(&uuid).is_ok());
    }
}
