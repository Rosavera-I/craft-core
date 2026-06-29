use std::fmt;

/// Result type alias for bridge operations
pub type Result<T> = std::result::Result<T, BridgeError>;

/// Errors that can occur in the bridge
#[derive(Debug)]
pub enum BridgeError {
    /// HTTP request failed
    Http(String),
    /// JSON serialization/deserialization failed
    Json(String),
    /// Protocol-specific error
    Protocol(String),
    /// IO error
    Io(std::io::Error),
    /// Task not found
    TaskNotFound(String),
    /// Agent not found
    AgentNotFound(String),
    /// Authentication failed
    Auth(String),
    /// Timeout
    Timeout(String),
    /// Invalid URL or endpoint
    InvalidUrl(String),
    /// SSE stream error
    Stream(String),
    /// MCP error
    Mcp(String),
    /// Validation error
    Validation(String),
    /// Reconnection failed
    Reconnect(String),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::Http(msg) => write!(f, "HTTP error: {msg}"),
            BridgeError::Json(msg) => write!(f, "JSON error: {msg}"),
            BridgeError::Protocol(msg) => write!(f, "Protocol error: {msg}"),
            BridgeError::Io(err) => write!(f, "IO error: {err}"),
            BridgeError::TaskNotFound(id) => write!(f, "Task not found: {id}"),
            BridgeError::AgentNotFound(url) => write!(f, "Agent not found: {url}"),
            BridgeError::Auth(msg) => write!(f, "Authentication error: {msg}"),
            BridgeError::Timeout(msg) => write!(f, "Timeout: {msg}"),
            BridgeError::InvalidUrl(url) => write!(f, "Invalid URL: {url}"),
            BridgeError::Stream(msg) => write!(f, "Stream error: {msg}"),
            BridgeError::Mcp(msg) => write!(f, "MCP error: {msg}"),
            BridgeError::Validation(msg) => write!(f, "Validation error: {msg}"),
            BridgeError::Reconnect(msg) => write!(f, "Reconnection failed: {msg}"),
        }
    }
}

impl std::error::Error for BridgeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BridgeError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for BridgeError {
    fn from(err: reqwest::Error) -> Self {
        BridgeError::Http(err.to_string())
    }
}

impl From<serde_json::Error> for BridgeError {
    fn from(err: serde_json::Error) -> Self {
        BridgeError::Json(err.to_string())
    }
}

impl From<std::io::Error> for BridgeError {
    fn from(err: std::io::Error) -> Self {
        BridgeError::Io(err)
    }
}

impl From<url::ParseError> for BridgeError {
    fn from(err: url::ParseError) -> Self {
        BridgeError::InvalidUrl(err.to_string())
    }
}
