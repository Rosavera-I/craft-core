//! CRAFT Bridge - A2A and MCP Protocol Implementation
//!
//! This crate provides interoperability with Google A2A (Agent2Agent)
//! and MCP (Model Context Protocol) for agent communication.

pub mod a2a;
pub mod cli;
pub mod error;
pub mod mcp;

pub use error::{BridgeError, Result};

// Re-export key types directly from the crate root
pub use a2a::{
    A2AAgent, A2AClient, AGENT_CARD_PATH, AgentCard, Capabilities, DEFAULT_TIMEOUT_SECS, Message,
    OAuth2MtlsConfig, Part, ReconnectConfig, Role, Skill, Task, TaskState, TaskStatus,
};
pub use mcp::{
    JSONRPC_VERSION, MCP_VERSION, McpServer, PromptCapabilities, PromptTemplate, Resource,
    ResourceCapabilities, ResourceContent, ServerCapabilities, Tool, ToolCapabilities, ToolContent,
    ToolResult, Transport,
    protocol::{JsonRpcError, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse},
};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::error::{BridgeError, Result};
    pub use crate::{A2AAgent, A2AClient, AgentCard, Task, TaskStatus};
    pub use crate::{McpServer, Transport};
}
