//! Integration tests for craft-bridge
//!
//! These tests verify the end-to-end functionality of A2A and MCP protocols.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::body::{Body, to_bytes};
use craft_bridge::a2a::{
    AgentCard, Capabilities, Message, Part, Role, SendTaskRequest, SendTaskResponse, Skill,
    TaskState,
};
use craft_bridge::mcp::{
    McpServer,
    protocol::{JsonRpcRequest, JsonRpcResponse, parse_message},
    types::*,
};
use serde_json::json;
use std::collections::HashMap;
use tower::ServiceExt;

// A2A Integration Tests

/// Test Agent Card discovery
#[tokio::test]
async fn test_a2a_agent_discovery() {
    // Create a mock server
    let card = AgentCard {
        name: "Test Agent".to_string(),
        description: "A test agent for integration tests".to_string(),
        version: "1.0.0".to_string(),
        url: "http://127.0.0.1:8081".to_string(),
        capabilities: Capabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
            history_count: Some(100),
        },
        authentication: None,
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
        skills: vec![Skill {
            id: "echo".to_string(),
            name: "Echo".to_string(),
            description: "Echoes input back".to_string(),
            input_modes: vec!["text".to_string()],
            output_modes: vec!["text".to_string()],
        }],
        extras: HashMap::new(),
    };

    let app = craft_bridge::a2a::A2AServer::new(card.clone()).router();

    let response = app
        .oneshot(
            axum::http::Request::get("/.well-known/agent.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let discovered: AgentCard = serde_json::from_slice(&body).unwrap();
    assert_eq!(discovered, card);
}

/// Test A2A task lifecycle
#[tokio::test]
async fn test_a2a_task_lifecycle() {
    let card = AgentCard {
        name: "Lifecycle Test Agent".to_string(),
        description: "Tests task lifecycle".to_string(),
        version: "1.0.0".to_string(),
        url: "http://127.0.0.1:8082".to_string(),
        capabilities: Capabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
            history_count: Some(10),
        },
        authentication: None,
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
        skills: vec![],
        extras: HashMap::new(),
    };

    let app = craft_bridge::a2a::A2AServer::new(card).router();

    let request = SendTaskRequest {
        id: Some("task-1".to_string()),
        session_id: Some("session-1".to_string()),
        message: Message {
            role: Role::User,
            parts: vec![Part::Text {
                text: "hello".to_string(),
            }],
        },
        metadata: None,
    };

    let response = app
        .oneshot(
            axum::http::Request::post("/tasks/send")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let sent: SendTaskResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(sent.task.id, "task-1");
    assert_eq!(sent.task.session_id.as_deref(), Some("session-1"));
    assert_eq!(sent.task.status.state, TaskState::Working);
}

/// Test A2A message creation
#[test]
fn test_a2a_message_creation() {
    let message = Message {
        role: Role::User,
        parts: vec![
            Part::Text {
                text: "Hello agent".to_string(),
            },
            Part::File {
                name: "data.json".to_string(),
                mime_type: "application/json".to_string(),
                bytes: "eyJrZXkiOiAidmFsdWUifQ==".to_string(),
            },
        ],
    };

    assert_eq!(message.role, Role::User);
    assert_eq!(message.parts.len(), 2);

    match &message.parts[0] {
        Part::Text { text } => assert_eq!(text, "Hello agent"),
        _ => panic!("Expected text part"),
    }
}

// MCP Integration Tests

/// Test MCP server initialization
#[tokio::test]
async fn test_mcp_server_initialization() {
    let server = McpServer::new("Test Server", "1.0.0")
        .with_transport(Transport::Stdio)
        .with_tool(Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                },
                "required": ["input"]
            }),
            annotations: None,
        });

    assert_eq!(server.name, "Test Server");
    assert_eq!(server.tools.len(), 1);
}

/// Test MCP tool discovery
#[test]
fn test_mcp_tool_discovery() {
    let tools_list = ListToolsResult {
        tools: vec![
            Tool {
                name: "tool1".to_string(),
                description: "First tool".to_string(),
                input_schema: json!({}),
                annotations: None,
            },
            Tool {
                name: "tool2".to_string(),
                description: "Second tool".to_string(),
                input_schema: json!({}),
                annotations: None,
            },
        ],
        next_cursor: None,
    };

    assert_eq!(tools_list.tools.len(), 2);
    assert_eq!(tools_list.tools[0].name, "tool1");
}

/// Test MCP resource subscription
#[test]
fn test_mcp_resource_subscription() {
    let subscribe_req = SubscribeResourceRequest {
        uri: "file:///test.txt".to_string(),
    };

    let json = serde_json::to_string(&subscribe_req).unwrap();
    assert!(json.contains("file:///test.txt"));

    let deserialized: SubscribeResourceRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(subscribe_req.uri, deserialized.uri);
}

/// Test MCP JSON-RPC message flow
#[test]
fn test_mcp_jsonrpc_flow() {
    // Client sends initialize request
    let init_request = JsonRpcRequest::new("initialize").with_params(json!({
        "protocol_version": "2024-11-05",
        "capabilities": {},
        "client_info": {
            "name": "Test Client",
            "version": "1.0.0"
        }
    }));

    let request_json = serde_json::to_string(&init_request).unwrap();

    // Server parses request
    let msg = parse_message(&request_json).expect("Should parse");

    match msg {
        craft_bridge::mcp::protocol::JsonRpcMessage::Request(req) => {
            assert_eq!(req.method, "initialize");
        }
        _ => panic!("Expected request"),
    }

    // Server responds
    let init_result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolCapabilities { list_changed: true }),
            resources: Some(ResourceCapabilities {
                subscribe: true,
                list_changed: true,
            }),
            prompts: Some(PromptCapabilities { list_changed: true }),
            logging: Some(json!({})),
        },
        server_info: Implementation {
            name: "Test Server".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    let response = JsonRpcResponse::success(1, serde_json::to_value(init_result).unwrap());
    let response_json = serde_json::to_string(&response).unwrap();

    // Client parses response
    let parsed_msg = parse_message(&response_json).expect("Should parse");

    match parsed_msg {
        craft_bridge::mcp::protocol::JsonRpcMessage::Response(resp) => {
            assert!(resp.is_success());
        }
        _ => panic!("Expected response"),
    }
}

/// Test MCP tool call
#[test]
fn test_mcp_tool_call() {
    let call_req = CallToolRequest {
        name: "echo".to_string(),
        arguments: Some(json!({
            "message": "Hello"
        })),
    };

    let json = serde_json::to_string(&call_req).unwrap();
    assert!(json.contains("echo"));
    assert!(json.contains("Hello"));

    let deserialized: CallToolRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(call_req.name, deserialized.name);
}

/// Test MCP resource reading
#[test]
fn test_mcp_resource_reading() {
    let read_req = ReadResourceRequest {
        uri: "file:///data.txt".to_string(),
    };
    assert_eq!(read_req.uri, "file:///data.txt");

    let result = ReadResourceResult {
        contents: vec![ResourceContent::Text {
            uri: "file:///data.txt".to_string(),
            text: "Hello, World!".to_string(),
            mime_type: "text/plain".to_string(),
        }],
    };

    assert_eq!(result.contents.len(), 1);
    match &result.contents[0] {
        ResourceContent::Text { text, .. } => assert_eq!(text, "Hello, World!"),
        _ => panic!("Expected text content"),
    }
}

/// Test MCP prompt retrieval
#[test]
fn test_mcp_prompt_retrieval() {
    let get_req = GetPromptRequest {
        name: "greeting".to_string(),
        arguments: Some({
            let mut map = HashMap::new();
            map.insert("name".to_string(), "Alice".to_string());
            map
        }),
    };

    assert_eq!(get_req.name, "greeting");
    assert!(get_req.arguments.is_some());
    assert_eq!(
        get_req.arguments.as_ref().unwrap().get("name"),
        Some(&"Alice".to_string())
    );
}

// Protocol Compliance Tests

/// Test A2A protocol compliance - task state transitions
#[test]
fn test_a2a_state_transition_compliance() {
    use craft_bridge::a2a::types::{Task, TaskState};

    let mut task = Task::new("task-1".to_string());

    // Initial state
    assert_eq!(task.status.state, TaskState::Submitted);

    // Working transition
    task.transition(TaskState::Working, Some("Started processing"));
    assert_eq!(task.status.state, TaskState::Working);
    assert_eq!(task.history.len(), 1);

    // Terminal state
    task.transition(TaskState::Completed, Some("Done"));
    assert!(task.is_terminal());
    assert_eq!(task.history.len(), 2);
}

/// Test MCP protocol compliance - JSON-RPC 2.0 format
#[test]
fn test_mcp_jsonrpc_compliance() {
    // Request must have jsonrpc = "2.0"
    let req = JsonRpcRequest::new("test");
    assert_eq!(req.jsonrpc, "2.0");

    // Response must have jsonrpc = "2.0"
    let resp = JsonRpcResponse::success(1, json!(null));
    assert_eq!(resp.jsonrpc, "2.0");

    // Error must have code and message
    let err = craft_bridge::mcp::protocol::JsonRpcError::new(
        craft_bridge::mcp::protocol::JsonRpcError::METHOD_NOT_FOUND,
        "Method not found",
    );
    assert_eq!(err.code, -32601);
    assert_eq!(err.message, "Method not found");
}

/// Test A2A SSE event format compliance
#[test]
fn test_a2a_sse_event_format() {
    // SSE events should be formatted as:
    // event: <event_type>
    // data: <json_data>
    //

    let task = craft_bridge::a2a::types::Task::new("task-1".to_string());
    let task_json = serde_json::to_string(&task).unwrap();

    // Format as SSE
    let sse_event = format!("event: task_update\ndata: {}\n", task_json);

    assert!(sse_event.starts_with("event: "));
    assert!(sse_event.contains("data: "));
    assert!(sse_event.contains(&task.id));
}

/// Test MCP capabilities discovery
#[test]
fn test_mcp_capabilities_discovery() {
    let capabilities = ServerCapabilities {
        tools: Some(ToolCapabilities { list_changed: true }),
        resources: Some(ResourceCapabilities {
            subscribe: true,
            list_changed: true,
        }),
        prompts: Some(PromptCapabilities { list_changed: true }),
        logging: Some(json!({})),
    };

    assert!(capabilities.tools.is_some());
    assert!(capabilities.resources.is_some());
    assert!(capabilities.prompts.is_some());

    let caps = capabilities.tools.as_ref().unwrap();
    assert!(caps.list_changed);
}

/// Test A2A authentication parsing
#[test]
fn test_a2a_authentication_parsing() {
    let auth = craft_bridge::a2a::types::Authentication {
        scheme: "oauth2".to_string(),
        config: {
            let mut map = HashMap::new();
            map.insert("token_url".to_string(), json!("https://example.com/token"));
            map.insert("scopes".to_string(), json!(["read", "write"]));
            map
        },
    };

    let json = serde_json::to_string(&auth).unwrap();
    assert!(json.contains("oauth2"));

    let deserialized: craft_bridge::a2a::types::Authentication =
        serde_json::from_str(&json).unwrap();
    assert_eq!(auth.scheme, deserialized.scheme);
}

// Edge Case Tests

/// Test edge case: empty parts
#[test]
fn test_edge_empty_message_parts() {
    let message = Message {
        role: Role::User,
        parts: vec![],
    };

    assert!(message.parts.is_empty());
}

/// Test edge case: task with no history
#[test]
fn test_edge_task_no_history() {
    let task = craft_bridge::a2a::types::Task::new("task-1".to_string());
    assert!(task.history.is_empty());
}

/// Test edge case: MCP tool with no arguments
#[test]
fn test_edge_tool_no_arguments() {
    let call = CallToolRequest {
        name: "simpl_tool".to_string(),
        arguments: None,
    };

    assert!(call.arguments.is_none());
}

/// Test edge case: resource with unknown size
#[test]
fn test_edge_resource_unknown_size() {
    let resource = Resource {
        uri: "stream://data".to_string(),
        name: "streaming-resource".to_string(),
        description: None,
        mime_type: Some("application/octet-stream".to_string()),
        size: None,
        annotations: None,
    };

    assert!(resource.size.is_none());
}

/// Test edge case: JSON-RPC null id
#[test]
fn test_edge_jsonrpc_null_id() {
    let resp = JsonRpcResponse::error(
        None,
        craft_bridge::mcp::protocol::JsonRpcError::new(
            craft_bridge::mcp::protocol::JsonRpcError::PARSE_ERROR,
            "Parse error",
        ),
    );

    assert!(resp.id.is_none());
}
