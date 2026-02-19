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
