//! Unit tests for craft-bridge protocol parsing
#![allow(clippy::expect_used, clippy::unwrap_used)]

use craft_bridge::a2a::types::*;
use craft_bridge::mcp::protocol::*;
use craft_bridge::mcp::types::*;
use serde_json::json;

// A2A Protocol Tests

#[test]
fn test_task_state_serialization() {
    let states = vec![
        TaskState::Submitted,
        TaskState::Working,
        TaskState::InputRequired,
        TaskState::Completed,
        TaskState::Failed,
        TaskState::Cancelled,
    ];

    for state in states {
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }
}

#[test]
fn test_task_serialization() {
    let task = Task::new("task-123".to_string());
    let json = serde_json::to_string(&task).unwrap();

    assert!(json.contains("task-123"));
    assert!(json.contains("SUBMITTED"));

    let deserialized: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(task.id, deserialized.id);
    assert_eq!(task.status.state, deserialized.status.state);
}

#[test]
fn test_message_serialization() {
    let message = Message {
        role: Role::User,
        parts: vec![
            Part::Text {
                text: "Hello".to_string(),
            },
            Part::File {
                name: "test.txt".to_string(),
                mime_type: "text/plain".to_string(),
                bytes: "SGVsbG8gV29ybGQ=".to_string(),
            },
        ],
    };

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("Hello"));
    assert!(json.contains("text"));

    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(message.role, deserialized.role);
    assert_eq!(message.parts.len(), deserialized.parts.len());
}

#[test]
fn test_agent_card_capabilities() {
    let card = AgentCard {
        name: "Test Agent".to_string(),
        description: "Test".to_string(),
        version: "1.0.0".to_string(),
        url: "https://example.com".to_string(),
        capabilities: Capabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: true,
            history_count: Some(50),
        },
        authentication: None,
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
        skills: vec![],
        extras: std::collections::HashMap::new(),
    };

    let json = serde_json::to_string(&card).unwrap();
    assert!(json.contains("streaming"));
    assert!(json.contains("true"));

    let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
    assert!(deserialized.capabilities.streaming);
    assert!(!deserialized.capabilities.push_notifications);
}

#[test]
fn test_skill_serialization() {
    let skill = Skill {
        id: "skill-1".to_string(),
        name: "Echo".to_string(),
        description: "Echoes input".to_string(),
        input_modes: vec!["text".to_string(), "audio".to_string()],
        output_modes: vec!["text".to_string()],
    };

    let json = serde_json::to_string(&skill).unwrap();

    let deserialized: Skill = serde_json::from_str(&json).unwrap();
    assert_eq!(skill.description, deserialized.description);
    assert_eq!(skill.input_modes.len(), deserialized.input_modes.len());
}

#[test]
fn test_send_task_request() {
    let request = SendTaskRequest {
        id: Some("task-123".to_string()),
        session_id: Some("session-456".to_string()),
        message: Message {
            role: Role::User,
            parts: vec![Part::Text {
                text: "Hello".to_string(),
            }],
        },
        metadata: Some({
            let mut map = std::collections::HashMap::new();
            map.insert("key".to_string(), json!("value"));
            map
        }),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("task-123"));
    assert!(json.contains("session-456"));

    let deserialized: SendTaskRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request.id, deserialized.id);
}

// MCP Protocol Tests

#[test]
fn test_jsonrpc_request_creation() {
    let req = JsonRpcRequest::new("initialize")
        .with_id("req-1".to_string())
        .with_params(json!({"key": "value"}));

    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "initialize");
    assert!(req.params.is_some());
}

#[test]
fn test_jsonrpc_notification() {
    let req = JsonRpcRequest::notification("$/progress");

    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "$/progress");
    assert!(req.id.is_none());
}

#[test]
fn test_jsonrpc_response_success() {
    let resp = JsonRpcResponse::success(1, json!({"result": "ok"}));

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
    assert_eq!(resp.error.as_ref().unwrap().code, -32601);
}

#[test]
fn test_jsonrpc_error_codes() {
    assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
    assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
    assert_eq!(JsonRpcError::METHOD_NOT_FOUND, -32601);
    assert_eq!(JsonRpcError::INVALID_PARAMS, -32602);
    assert_eq!(JsonRpcError::INTERNAL_ERROR, -32603);
}

#[test]
fn test_jsonrpc_parse_message() {
    let req_json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let msg = parse_message(req_json).expect("Should parse");

    match msg {
        JsonRpcMessage::Request(req) => {
            assert_eq!(req.method, "initialize");
        }
        _ => panic!("Expected request"),
    }
}

#[test]
fn test_jsonrpc_parse_response() {
    let resp_json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
    let msg = parse_message(resp_json).expect("Should parse");

    match msg {
        JsonRpcMessage::Response(resp) => {
            assert!(resp.is_success());
        }
        _ => panic!("Expected response"),
    }
}

#[test]
fn test_parse_invalid_json() {
    let result = parse_message("not json");
    assert!(result.is_err());
}

// MCP Types Tests

#[test]
fn test_tool_serialization() {
    let tool = Tool {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "param1": { "type": "string" }
            }
        }),
        annotations: Some(ToolAnnotations {
            title: Some("Test Tool".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }),
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("test_tool"));

    let deserialized: Tool = serde_json::from_str(&json).unwrap();
    assert_eq!(tool.name, deserialized.name);
    assert!(deserialized.annotations.is_some());
}

#[test]
fn test_resource_serialization() {
    let resource = Resource {
        uri: "file:///test.txt".to_string(),
        name: "test.txt".to_string(),
        description: Some("Test file".to_string()),
        mime_type: Some("text/plain".to_string()),
        size: Some(100),
        annotations: Some(ResourceAnnotations {
            title: Some("Test".to_string()),
            description: Some("Desc".to_string()),
        }),
    };

    let json = serde_json::to_string(&resource).unwrap();
    assert!(json.contains("file:///test.txt"));

    let deserialized: Resource = serde_json::from_str(&json).unwrap();
    assert_eq!(resource.uri, deserialized.uri);
}

#[test]
fn test_prompt_template_serialization() {
    let prompt = PromptTemplate {
        name: "greeting".to_string(),
        description: Some("A greeting prompt".to_string()),
        arguments: Some(vec![PromptArgument {
            name: "name".to_string(),
            description: Some("User name".to_string()),
            required: Some(true),
        }]),
    };

    let json = serde_json::to_string(&prompt).unwrap();
    assert!(json.contains("greeting"));

    let deserialized: PromptTemplate = serde_json::from_str(&json).unwrap();
    assert_eq!(prompt.name, deserialized.name);
}

#[test]
fn test_tool_result_serialization() {
    let result = ToolResult {
        content: vec![ToolContent::Text {
            text: "Hello".to_string(),
        }],
        is_error: Some(false),
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("Hello"));

    let deserialized: ToolResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result.content.len(), deserialized.content.len());
}

#[test]
fn test_initialize_request_serialization() {
    let req = InitializeRequest {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ClientCapabilities {
            experimental: None,
            roots: Some(RootsCapability {
                list_changed: Some(true),
            }),
            sampling: None,
        },
        client_info: Implementation {
            name: "Test Client".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("2024-11-05"));

    let deserialized: InitializeRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(req.protocol_version, deserialized.protocol_version);
}

#[test]
fn test_mcp_error_codes() {
    assert_eq!(mcp_error::CONNECTION_CLOSED, -32000);
    assert_eq!(mcp_error::REQUEST_TIMEOUT, -32001);
    assert_eq!(mcp_error::RESOURCE_NOT_FOUND, -32002);
    assert_eq!(mcp_error::TOOL_EXECUTION_ERROR, -32003);
}

// Protocol Validation Tests

#[test]
fn test_parse_requests_stream() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}
{"jsonrpc":"2.0","id":2,"method":"resources/list"}
"#;

    let results = parse_requests_stream(input);
    assert_eq!(results.len(), 2);

    for result in results {
        assert!(result.is_ok());
    }
}

#[test]
fn test_validate_protocol_version() {
    // Valid version
    assert!(validate_protocol_version("2024-11-05").is_ok());

    // Invalid version
    assert!(validate_protocol_version("invalid").is_err());
    assert!(validate_protocol_version("2023-01-01").is_err());
}

// Transport Tests

#[test]
fn test_transport_default() {
    let transport = Transport::default();
    match transport {
        Transport::Stdio => (), // Expected
        _ => panic!("Default should be Stdio"),
    }
}

#[test]
fn test_transport_http() {
    let transport = Transport::Http { port: 8080 };
    match transport {
        Transport::Http { port } => assert_eq!(port, 8080),
        _ => panic!("Expected HTTP transport"),
    }
}

// Server Building Tests

#[test]
fn test_mcp_server_building() {
    let server = McpServer::new("test", "1.0.0")
        .with_transport(Transport::Http { port: 9000 })
        .with_tool(Tool {
            name: "echo".to_string(),
            description: "Echo".to_string(),
            input_schema: json!({}),
            annotations: None,
        })
        .with_resource(Resource {
            uri: "test://resource".to_string(),
            name: "resource".to_string(),
            description: None,
            mime_type: None,
            size: None,
            annotations: None,
        })
        .with_prompt(PromptTemplate {
            name: "test_prompt".to_string(),
            description: None,
            arguments: None,
        });

    assert_eq!(server.name, "test");
    assert_eq!(server.tools.len(), 1);
    assert_eq!(server.resources.len(), 1);
    assert_eq!(server.prompts.len(), 1);

    match server.transport {
        Transport::Http { port } => assert_eq!(port, 9000),
        _ => panic!("Expected HTTP transport"),
    }
}

// Task State Transition Tests

#[test]
fn test_task_state_transitions() {
    let mut task = Task::new("task-123".to_string());

    assert_eq!(task.status.state, TaskState::Submitted);
    assert!(!task.is_terminal());

    task.transition(TaskState::Working, Some("Processing"));
    assert_eq!(task.status.state, TaskState::Working);
    assert_eq!(task.history.len(), 1);

    task.transition(TaskState::Completed, None);
    assert_eq!(task.status.state, TaskState::Completed);
    assert!(task.is_terminal());
}

#[test]
fn test_task_defaults() {
    let task = Task::new("task-1".to_string());

    assert_eq!(task.history.len(), 0);
    assert!(task.artifacts.is_none());
}
