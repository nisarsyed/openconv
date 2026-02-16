use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use openconv_shared::error::OpenConvError;

/// Newtype wrapper for `OpenConvError` that implements `IntoResponse`.
///
/// Needed because of the orphan rule — neither the trait (`IntoResponse`)
/// nor the type (`OpenConvError`) is defined in this crate.
pub struct ServerError(pub OpenConvError);

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            OpenConvError::NotFound => (StatusCode::NOT_FOUND, self.0.to_string()),
            OpenConvError::Unauthorized => (StatusCode::UNAUTHORIZED, self.0.to_string()),
            OpenConvError::Forbidden => (StatusCode::FORBIDDEN, self.0.to_string()),
            OpenConvError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            OpenConvError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            OpenConvError::Crypto(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl From<OpenConvError> for ServerError {
    fn from(e: OpenConvError) -> Self {
        ServerError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_maps_to_404() {
        let response = ServerError(OpenConvError::NotFound).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_unauthorized_maps_to_401() {
        let response = ServerError(OpenConvError::Unauthorized).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_forbidden_maps_to_403() {
        let response = ServerError(OpenConvError::Forbidden).into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_validation_maps_to_400() {
        let response = ServerError(OpenConvError::Validation("bad input".into())).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_internal_maps_to_500() {
        let response =
            ServerError(OpenConvError::Internal("something broke".into())).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_error_responses_are_json_with_error_field() {
        let response = ServerError(OpenConvError::NotFound).into_response();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("error").is_some());
        assert_eq!(json["error"], "not found");
    }
}
