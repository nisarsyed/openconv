/// Shared error type used across server and client.
#[derive(Debug, thiserror::Error)]
pub enum OpenConvError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("validation error: {0}")]
    Validation(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("rate limited")]
    RateLimited,

    #[error("session compromised")]
    SessionCompromised,

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_display() {
        let err = OpenConvError::NotFound;
        assert_eq!(err.to_string(), "not found");
    }

    #[test]
    fn validation_contains_message() {
        let err = OpenConvError::Validation("bad input".into());
        assert_eq!(err.to_string(), "validation error: bad input");
    }

    #[test]
    fn internal_contains_message() {
        let err = OpenConvError::Internal("db down".into());
        assert_eq!(err.to_string(), "internal error: db down");
    }

    #[test]
    fn all_variants_impl_error() {
        let errors: Vec<Box<dyn std::error::Error>> = vec![
            Box::new(OpenConvError::NotFound),
            Box::new(OpenConvError::Unauthorized),
            Box::new(OpenConvError::Forbidden),
            Box::new(OpenConvError::Validation("x".into())),
            Box::new(OpenConvError::Internal("y".into())),
            Box::new(OpenConvError::Crypto("z".into())),
            Box::new(OpenConvError::RateLimited),
            Box::new(OpenConvError::SessionCompromised),
            Box::new(OpenConvError::ServiceUnavailable("redis down".into())),
        ];
        for e in &errors {
            let _ = e.to_string();
        }
    }

    #[test]
    fn rate_limited_display() {
        let err = OpenConvError::RateLimited;
        assert_eq!(err.to_string(), "rate limited");
    }

    #[test]
    fn session_compromised_display() {
        let err = OpenConvError::SessionCompromised;
        assert_eq!(err.to_string(), "session compromised");
    }

    #[test]
    fn service_unavailable_display() {
        let err = OpenConvError::ServiceUnavailable("redis down".into());
        assert_eq!(err.to_string(), "service unavailable: redis down");
    }
}
