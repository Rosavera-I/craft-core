//! Google A2A (Agent2Agent) Protocol Implementation
//!
//! Implements the Agent Card discovery, task lifecycle, and SSE streaming
//! as specified in the Google A2A protocol.

pub mod client;
pub mod server;
pub mod types;

pub use client::{A2AAgent, A2AClient, OAuth2MtlsConfig};
pub use server::A2AServer;
pub use types::*;

/// Default well-known path for Agent Card discovery
pub const AGENT_CARD_PATH: &str = "/.well-known/agent.json";

/// Default timeout for HTTP operations
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Maximum number of reconnection attempts
    pub max_attempts: u32,
    /// Initial delay between attempts (exponential backoff)
    pub initial_delay_ms: u64,
    /// Maximum delay between attempts
    pub max_delay_ms: u64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
        }
    }
}
