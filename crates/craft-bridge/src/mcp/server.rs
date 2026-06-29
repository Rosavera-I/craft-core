use crate::error::{BridgeError, Result};
use crate::mcp::{MCP_VERSION, protocol::*, types::*};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// MCP Server handle
#[derive(Debug, Clone)]
pub struct McpServerHandle {
    /// Server information
    inner: Arc<McpServer>,
    /// Resource subscriptions
    subscriptions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Update broadcaster
    update_tx: broadcast::Sender<ResourceContent>,
}

impl McpServerHandle {
    /// Send resource update notification
    pub fn notify_resource_update(&self, _uri: &str, content: ResourceContent) -> Result<()> {
        let _ = self.update_tx.send(content);
        Ok(())
    }

    /// Get resource by URI
    pub fn get_resource(&self, uri: &str) -> Option<Resource> {
        self.inner.resources.iter().find(|r| r.uri == uri).cloned()
    }

    /// Get tool by name
    pub fn get_tool(&self, name: &str) -> Option<Tool> {
        self.inner.tools.iter().find(|t| t.name == name).cloned()
    }

    /// Get prompt by name
    pub fn get_prompt(&self, name: &str) -> Option<PromptTemplate> {
        self.inner.prompts.iter().find(|p| p.name == name).cloned()
    }
}

/// MCP Server implementation
impl McpServer {
    /// Run the MCP server with the configured transport
    pub async fn run(self) -> Result<()> {
        match self.transport.clone() {
            Transport::Stdio => self.run_stdio().await,
            Transport::Http { port } => self.run_http(port).await,
        }
    }

    /// Run as stdio server (JSON-RPC over stdin/stdout)
    async fn run_stdio(self) -> Result<()> {
        let handle = Arc::new(McpServerHandle {
            inner: Arc::new(self),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            update_tx: broadcast::channel(100).0,
        });

        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stdout_lock = stdout.lock();
        let reader = stdin.lock();

        for line in reader.lines() {
            let line = line.map_err(BridgeError::Io)?;
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            let response = process_request_line(&handle, line).await;

            let json = serde_json::to_string(&response)?;
            writeln!(stdout_lock, "{}", json).map_err(BridgeError::Io)?;
            stdout_lock.flush().map_err(BridgeError::Io)?;
        }

        Ok(())
    }

    /// Run as HTTP server (JSON-RPC over HTTP)
    async fn run_http(self, port: u16) -> Result<()> {
        let handle = Arc::new(McpServerHandle {
            inner: Arc::new(self),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            update_tx: broadcast::channel(100).0,
        });

        let app = Router::new()
            .route("/mcp", post(mcp_http_handler))
            .route("/mcp/tools", get(list_tools_handler))
            .route("/mcp/resources", get(list_resources_handler))
            .route("/mcp/resources/{id}", get(read_resource_handler))
            .route("/mcp/prompts", get(list_prompts_handler))
            .route("/mcp/prompts/{id}", get(get_prompt_handler))
            .with_state(handle);

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .map_err(BridgeError::Io)?;

        tracing::info!("MCP server listening on port {}", port);
        axum::serve(listener, app)
            .await
            .map_err(|e| BridgeError::Http(e.to_string()))?;

        Ok(())
    }
}

/// Process a single JSON-RPC request line
async fn process_request_line(handle: &Arc<McpServerHandle>, line: &str) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(e) => {
            return JsonRpcResponse::error(
                None,
                JsonRpcError::new(
                    JsonRpcError::PARSE_ERROR,
                    format!("Failed to parse JSON: {}", e),
                ),
            );
        }
    };

    let result = dispatch_method(handle, &request.method, request.params.clone()).await;

    match result {
        Ok(value) => JsonRpcResponse::success(request.id.unwrap_or_else(|| "null".into()), value),
        Err(e) => JsonRpcResponse::error(
            request.id,
            JsonRpcError::new(JsonRpcError::INTERNAL_ERROR, e.to_string()),
        ),
    }
}

/// Dispatch JSON-RPC method call
async fn dispatch_method(
    handle: &Arc<McpServerHandle>,
    method: &str,
    params: Option<Value>,
) -> Result<Value> {
    match method {
        "initialize" => handle_initialize(params),
        "tools/list" => handle_tools_list(handle),
        "tools/call" => handle_tool_call(handle, params).await,
        "resources/list" => handle_resources_list(handle),
        "resources/read" => handle_resource_read(handle, params),
        "resources/subscribe" => handle_resource_subscribe(handle, params),
        "resources/unsubscribe" => handle_resource_unsubscribe(handle, params),
        "prompts/list" => handle_prompts_list(handle),
        "prompts/get" => handle_prompt_get(handle, params),
        _ => Err(BridgeError::Protocol(format!(
            "Method not found: {}",
            method
        ))),
    }
}

fn handle_initialize(params: Option<Value>) -> Result<Value> {
    let init_req: InitializeRequest = serde_json::from_value(
        params.ok_or_else(|| BridgeError::Validation("Missing params".to_string()))?,
    )?;

    validate_protocol_version(&init_req.protocol_version)?;

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

    Ok(serde_json::to_value(result)?)
}

fn handle_tools_list(handle: &Arc<McpServerHandle>) -> Result<Value> {
    let result = ListToolsResult {
        tools: handle.inner.tools.clone(),
        next_cursor: None,
    };
    Ok(serde_json::to_value(result)?)
}

async fn handle_tool_call(handle: &Arc<McpServerHandle>, params: Option<Value>) -> Result<Value> {
    let req: CallToolRequest = serde_json::from_value(
        params.ok_or_else(|| BridgeError::Validation("Missing params".to_string()))?,
    )?;

    let tool = handle
        .get_tool(&req.name)
        .ok_or_else(|| BridgeError::Protocol(format!("Tool not found: {}", req.name)))?;

    // Simple echo implementation as default
    let result = ToolResult {
        content: vec![ToolContent::Text {
            text: format!("Called tool '{}' with args: {:?}", tool.name, req.arguments),
        }],
        is_error: Some(false),
    };

    Ok(serde_json::to_value(result)?)
}

fn handle_resources_list(handle: &Arc<McpServerHandle>) -> Result<Value> {
    let result = ListResourcesResult {
        resources: handle.inner.resources.clone(),
        next_cursor: None,
    };
    Ok(serde_json::to_value(result)?)
}

fn handle_resource_read(handle: &Arc<McpServerHandle>, params: Option<Value>) -> Result<Value> {
    let req: ReadResourceRequest = serde_json::from_value(
        params.ok_or_else(|| BridgeError::Validation("Missing params".to_string()))?,
    )?;

    let resource = handle
        .get_resource(&req.uri)
        .ok_or_else(|| BridgeError::Protocol(format!("Resource not found: {}", req.uri)))?;

    let result = ReadResourceResult {
        contents: vec![ResourceContent::Text {
            uri: resource.uri,
            text: format!("Resource: {}", resource.name),
            mime_type: resource
                .mime_type
                .unwrap_or_else(|| "text/plain".to_string()),
        }],
    };

    Ok(serde_json::to_value(result)?)
}

fn handle_resource_subscribe(
    handle: &Arc<McpServerHandle>,
    params: Option<Value>,
) -> Result<Value> {
    let req: SubscribeResourceRequest = serde_json::from_value(
        params.ok_or_else(|| BridgeError::Validation("Missing params".to_string()))?,
    )?;

    let mut subs = handle
        .subscriptions
        .write()
        .map_err(|_| BridgeError::Protocol("Failed to lock subscriptions".to_string()))?;

    subs.entry(req.uri.clone()).or_insert_with(Vec::new);

    Ok(serde_json::json!({}))
}

fn handle_resource_unsubscribe(
    handle: &Arc<McpServerHandle>,
    params: Option<Value>,
) -> Result<Value> {
    let req: UnsubscribeResourceRequest = serde_json::from_value(
        params.ok_or_else(|| BridgeError::Validation("Missing params".to_string()))?,
    )?;

    let mut subs = handle
        .subscriptions
        .write()
        .map_err(|_| BridgeError::Protocol("Failed to lock subscriptions".to_string()))?;

    subs.remove(&req.uri);

    Ok(serde_json::json!({}))
}

fn handle_prompts_list(handle: &Arc<McpServerHandle>) -> Result<Value> {
    let result = ListPromptsResult {
        prompts: handle.inner.prompts.clone(),
        next_cursor: None,
    };
    Ok(serde_json::to_value(result)?)
}

fn handle_prompt_get(handle: &Arc<McpServerHandle>, params: Option<Value>) -> Result<Value> {
    let req: GetPromptRequest = serde_json::from_value(
        params.ok_or_else(|| BridgeError::Validation("Missing params".to_string()))?,
    )?;

    let prompt = handle
        .get_prompt(&req.name)
        .ok_or_else(|| BridgeError::Protocol(format!("Prompt not found: {}", req.name)))?;

    let result = GetPromptResult {
        description: Some(format!("Prompt: {}", prompt.name)),
        messages: vec![PromptMessage {
            role: MessageRole::User,
            content: MessageContent::Text {
                text: format!("Prompt: {} with args {:?}", prompt.name, req.arguments),
            },
        }],
    };

    Ok(serde_json::to_value(result)?)
}

// HTTP handlers

async fn mcp_http_handler(
    State(_handle): State<Arc<McpServerHandle>>,
    Json(_request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        "HTTP MCP not fully implemented",
    )
}

async fn list_tools_handler(
    State(handle): State<Arc<McpServerHandle>>,
) -> std::result::Result<Json<Value>, StatusCode> {
    handle_tools_list(&handle)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn list_resources_handler(
    State(handle): State<Arc<McpServerHandle>>,
) -> std::result::Result<Json<Value>, StatusCode> {
    handle_resources_list(&handle)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn read_resource_handler(
    State(handle): State<Arc<McpServerHandle>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> std::result::Result<Json<Value>, StatusCode> {
    let req = format!("resource://{}", id);
    handle_resource_read(&handle, Some(serde_json::json!({"uri": req})))
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

async fn list_prompts_handler(
    State(handle): State<Arc<McpServerHandle>>,
) -> std::result::Result<Json<Value>, StatusCode> {
    handle_prompts_list(&handle)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_prompt_handler(
    State(handle): State<Arc<McpServerHandle>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> std::result::Result<Json<Value>, StatusCode> {
    handle_prompt_get(&handle, Some(serde_json::json!({"name": id})))
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_handle() -> Arc<McpServerHandle> {
        let server = McpServer::new("test", "1.0.0").with_tool(Tool {
            name: "echo".to_string(),
            description: "Echo input".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            }),
            annotations: None,
        });

        Arc::new(McpServerHandle {
            inner: Arc::new(server),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            update_tx: broadcast::channel(100).0,
        })
    }

    #[tokio::test]
    async fn test_initialize() {
        let handle = create_test_handle();
        let req = serde_json::json!({
            "protocol_version": MCP_VERSION,
            "capabilities": {},
            "client_info": { "name": "test", "version": "1.0.0" }
        });

        let result = dispatch_method(&handle, "initialize", Some(req)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tools_list() {
        let handle = create_test_handle();
        let result = dispatch_method(&handle, "tools/list", None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_method() {
        let handle = create_test_handle();
        let result = dispatch_method(&handle, "invalid/method", None).await;
        assert!(result.is_err());
    }
}
