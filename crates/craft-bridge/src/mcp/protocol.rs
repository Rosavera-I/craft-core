use crate::error::{BridgeError, Result};
use crate::mcp::{JSONRPC_VERSION, MCP_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Request ID
    pub id: Option<Value>,
    /// Method name
    pub method: String,
    /// Method parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(uuid::Uuid::new_v4().to_string().into()),
            method: method.into(),
            params: None,
        }
    }

    /// Create a notification (no id)
    pub fn notification(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: None,
            method: method.into(),
            params: None,
        }
    }

    /// Set request ID
    pub fn with_id(mut self, id: impl Into<Value>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set parameters
    pub fn with_params(mut self, params: impl Into<Value>) -> Self {
        self.params = Some(params.into());
        self
    }
}

/// JSON-RPC 2.0 response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Request ID
    pub id: Option<Value>,
    /// Result (present on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error (present on failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a successful response
    pub fn success(id: impl Into<Value>, result: impl Into<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id.into()),
            result: Some(result.into()),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: Option<Value>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }

    /// Check if response is successful
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// JSON-RPC error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    /// Create a new error
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Set error data
    pub fn with_data(mut self, data: impl Into<Value>) -> Self {
        self.data = Some(data.into());
        self
    }

    // Standard error codes
    /// Parse error (-32700)
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid request (-32600)
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found (-32601)
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params (-32602)
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error (-32603)
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Server error (-32000 to -32099)
    pub const SERVER_ERROR: i32 = -32000;
}

/// JSON-RPC message (union of request/response/error)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// Request
    Request(JsonRpcRequest),
    /// Response
    Response(JsonRpcResponse),
}

/// Parse a JSON-RPC message from JSON string
pub fn parse_message(json: &str) -> Result<JsonRpcMessage> {
    let value: Value = serde_json::from_str(json)?;

    // Check if it's a response (has 'result' or 'error')
    if value.get("result").is_some() || value.get("error").is_some() {
        let response: JsonRpcResponse = serde_json::from_value(value)?;
        return Ok(JsonRpcMessage::Response(response));
    }

    // Otherwise it's a request
    let request: JsonRpcRequest = serde_json::from_value(value)?;
    Ok(JsonRpcMessage::Request(request))
}

/// Parse requests from newline-delimited JSON stream
pub fn parse_requests_stream(json: &str) -> Vec<Result<JsonRpcRequest>> {
    json.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let req: JsonRpcRequest = serde_json::from_str(line)?;
            Ok(req)
        })
        .collect()
}

/// MCP error codes (MCP-specific)
pub mod mcp_error {
    /// Connection closed
    pub const CONNECTION_CLOSED: i32 = -32000;
    /// Request timed out
    pub const REQUEST_TIMEOUT: i32 = -32001;
    /// Resource not found
    pub const RESOURCE_NOT_FOUND: i32 = -32002;
    /// Tool execution error
    pub const TOOL_EXECUTION_ERROR: i32 = -32003;
    /// Prompt not found
    pub const PROMPT_NOT_FOUND: i32 = -32004;
    /// Invalid resource URI
    pub const INVALID_RESOURCE_URI: i32 = -32005;
    /// Method not supported
    pub const METHOD_NOT_SUPPORTED: i32 = -32006;
}

/// Validate MCP protocol version
pub fn validate_protocol_version(version: &str) -> Result<()> {
    // Accept exact match for now; in production could support version ranges
    if version == MCP_VERSION {
        Ok(())
    } else {
        Err(BridgeError::Protocol(format!(
            "Protocol version mismatch: expected {}, got {}",
            MCP_VERSION, version
        )))
    }
}

/// Create initialize response
pub fn create_initialize_response(client_version: &str) -> Result<JsonRpcResponse> {
    use crate::mcp::types::*;

    validate_protocol_version(client_version)?;

    let result = InitializeResult {
        protocol_version: MCP_VERSION.to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolCapabilities { list_changed: true }),
            resources: Some(ResourceCapabilities {
                subscribe: true,
                list_changed: true,
            }),
            prompts: Some(PromptCapabilities { list_changed: true }),
            logging: Some(serde_json::json!({})),
        },
        server_info: Implementation {
            name: "craft-bridge-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    Ok(JsonRpcResponse::success(
        uuid::Uuid::new_v4().to_string(),
        serde_json::to_value(result)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request() -> crate::Result<()> {
        let req = JsonRpcRequest::new("initialize")
            .with_params(serde_json::json!({"protocol_version": "2024-11-05"}));

        let json = serde_json::to_string(&req)?;
        assert!(json.contains("initialize"));
        assert!(json.contains("2024-11-05"));

        let parsed: JsonRpcRequest = serde_json::from_str(&json)?;
        assert_eq!(parsed.method, "initialize");
        Ok(())
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let resp =
            JsonRpcResponse::success(1, serde_json::json!({"protocol_version": "2024-11-05"}));

        assert!(resp.is_success());
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let err = JsonRpcError::new(JsonRpcError::METHOD_NOT_FOUND, "Method not found");
        let resp = JsonRpcResponse::error(Some(1.into()), err);

        assert!(!resp.is_success());
        assert!(resp.error.is_some());
        assert!(resp.result.is_none());
    }

    #[test]
    fn test_parse_message() -> crate::Result<()> {
        let req_json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let msg = parse_message(req_json)?;

        match msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.method, "initialize");
            }
            _ => panic!("Expected request"),
        }
        Ok(())
    }

    #[test]
    fn test_parse_message_response() -> crate::Result<()> {
        let resp_json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
        let msg = parse_message(resp_json)?;

        match msg {
            JsonRpcMessage::Response(resp) => {
                assert!(resp.is_success());
            }
            _ => panic!("Expected response"),
        }
        Ok(())
    }

    #[test]
    fn test_notification_request() {
        let req = JsonRpcRequest::notification("$/progress");
        assert!(req.id.is_none());
    }
}
