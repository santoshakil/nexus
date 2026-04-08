use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("api error: {0}")]
    Api(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("platform not available: {0}")]
    PlatformNotAvailable(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AgentError {
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth(msg.into())
    }

    pub fn api(msg: impl Into<String>) -> Self {
        Self::Api(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    pub fn session(msg: impl Into<String>) -> Self {
        Self::Session(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    pub fn platform_not_available(msg: impl Into<String>) -> Self {
        Self::PlatformNotAvailable(msg.into())
    }

    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<&'static str>,
    pub retryable: bool,
}

impl From<&AgentError> for ErrorResponse {
    fn from(err: &AgentError) -> Self {
        let (code, suggestion, retryable) = match err {
            AgentError::Auth(_) => (
                "AUTH_ERROR",
                Some("Run `nexus auth telegram` or check GMAIL_APP_PASSWORD env var"),
                false,
            ),
            AgentError::Api(_) => ("API_ERROR", None, false),
            AgentError::Network(_) => (
                "NETWORK_ERROR",
                Some("Check internet connection and try again"),
                true,
            ),
            AgentError::Session(_) => (
                "SESSION_ERROR",
                Some("Session expired. Re-run `nexus auth telegram`"),
                false,
            ),
            AgentError::NotFound(_) => (
                "NOT_FOUND",
                Some("Use list_channels to find valid channel names/IDs"),
                false,
            ),
            AgentError::InvalidInput(_) => ("INVALID_INPUT", None, false),
            AgentError::PlatformNotAvailable(_) => (
                "PLATFORM_NOT_AVAILABLE",
                Some("Platform not configured. Set required env vars and restart"),
                false,
            ),
            AgentError::NotImplemented(_) => (
                "NOT_IMPLEMENTED",
                Some("This feature is not yet available"),
                false,
            ),
            AgentError::Internal(_) => ("INTERNAL_ERROR", Some("Unexpected error"), true),
        };
        Self {
            code,
            message: err.to_string(),
            suggestion,
            retryable,
        }
    }
}

impl ErrorResponse {
    pub fn to_compact(&self) -> String {
        let mut parts = vec![format!("[{}] {}", self.code, self.message)];
        if let Some(s) = self.suggestion {
            parts.push(format!("Suggestion: {s}"));
        }
        if self.retryable {
            parts.push("(retryable)".to_string());
        }
        parts.join(" | ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        assert_eq!(
            AgentError::auth("bad token").to_string(),
            "authentication failed: bad token"
        );
        assert_eq!(
            AgentError::not_found("chat xyz").to_string(),
            "not found: chat xyz"
        );
        assert_eq!(
            AgentError::network("timeout").to_string(),
            "network error: timeout"
        );
    }

    #[test]
    fn error_response_mapping() {
        let err = AgentError::auth("expired");
        let resp = ErrorResponse::from(&err);
        assert_eq!(resp.code, "AUTH_ERROR");
        assert!(!resp.retryable);
        assert!(resp.suggestion.is_some());
    }

    #[test]
    fn network_error_is_retryable() {
        let err = AgentError::network("connection reset");
        let resp = ErrorResponse::from(&err);
        assert!(resp.retryable);
        assert_eq!(resp.code, "NETWORK_ERROR");
    }

    #[test]
    fn internal_error_is_retryable() {
        let err = AgentError::internal("panic");
        let resp = ErrorResponse::from(&err);
        assert!(resp.retryable);
        assert_eq!(resp.code, "INTERNAL_ERROR");
    }

    #[test]
    fn non_retryable_errors() {
        for err in [
            AgentError::api("bad request"),
            AgentError::session("expired"),
            AgentError::not_found("x"),
            AgentError::invalid_input("y"),
            AgentError::platform_not_available("z"),
            AgentError::not_implemented("w"),
        ] {
            let resp = ErrorResponse::from(&err);
            assert!(!resp.retryable, "expected non-retryable for {}", resp.code);
        }
    }

    #[test]
    fn compact_format_with_suggestion() {
        let err = AgentError::auth("token expired");
        let resp = ErrorResponse::from(&err);
        let compact = resp.to_compact();
        assert!(compact.contains("[AUTH_ERROR]"));
        assert!(compact.contains("token expired"));
        assert!(compact.contains("Suggestion:"));
        assert!(!compact.contains("(retryable)"));
    }

    #[test]
    fn compact_format_retryable() {
        let err = AgentError::network("reset");
        let resp = ErrorResponse::from(&err);
        let compact = resp.to_compact();
        assert!(compact.contains("(retryable)"));
    }

    #[test]
    fn constructor_helpers() {
        let e = AgentError::auth("x");
        assert!(matches!(e, AgentError::Auth(_)));
        let e = AgentError::api("x");
        assert!(matches!(e, AgentError::Api(_)));
        let e = AgentError::network("x");
        assert!(matches!(e, AgentError::Network(_)));
        let e = AgentError::session("x");
        assert!(matches!(e, AgentError::Session(_)));
        let e = AgentError::not_found("x");
        assert!(matches!(e, AgentError::NotFound(_)));
        let e = AgentError::invalid_input("x");
        assert!(matches!(e, AgentError::InvalidInput(_)));
        let e = AgentError::platform_not_available("x");
        assert!(matches!(e, AgentError::PlatformNotAvailable(_)));
        let e = AgentError::not_implemented("x");
        assert!(matches!(e, AgentError::NotImplemented(_)));
        let e = AgentError::internal("x");
        assert!(matches!(e, AgentError::Internal(_)));
    }
}
