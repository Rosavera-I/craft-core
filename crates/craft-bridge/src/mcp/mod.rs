//! MCP (Model Context Protocol) Implementation
//!
//! Provides JSON-RPC 2.0 compatible protocol server for tool/resource
//! discovery and invocation.

pub mod protocol;
pub mod server;
pub mod types;

pub use protocol::{JsonRpcError, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse};
pub use types::{
    CallToolRequest, ClientCapabilities, EmbeddedResource, GetPromptRequest, GetPromptResult,
    Implementation, InitializeRequest, InitializeResult, ListPromptsResult, ListResourcesResult,
    ListToolsResult, McpServer, MessageContent, MessageRole, PromptArgument, PromptCapabilities,
    PromptMessage, PromptTemplate, ReadResourceRequest, ReadResourceResult, Resource,
    ResourceAnnotations, ResourceCapabilities, ResourceContent, RootsCapability,
    ServerCapabilities, SubscribeResourceRequest, Tool, ToolAnnotations, ToolCapabilities,
    ToolContent, ToolResult, Transport, UnsubscribeResourceRequest,
};

/// Default MCP protocol version
pub const MCP_VERSION: &str = "2024-11-05";

/// Default JSON-RPC version
pub const JSONRPC_VERSION: &str = "2.0";
