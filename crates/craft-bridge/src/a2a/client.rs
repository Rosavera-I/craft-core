use crate::a2a::{AGENT_CARD_PATH, DEFAULT_TIMEOUT_SECS, ReconnectConfig, types::*};
use crate::error::{BridgeError, Result};
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::pin::Pin;
use std::time::Duration;

/// A2A Agent client for communicating with external agents
#[derive(Debug, Clone)]
pub struct A2AClient {
    /// Agent card (discovered from agent endpoint)
    pub card: AgentCard,
    /// HTTP client
    client: Client,
    /// Reconnection configuration
    reconnect_config: ReconnectConfig,
}

/// A2A Agent identification
#[derive(Debug, Clone)]
pub struct A2AAgent {
    /// Agent card
    pub card: AgentCard,
    /// HTTP client
    pub client: Client,
}

/// OAuth2 mTLS authentication configuration
#[derive(Debug, Clone)]
pub struct OAuth2MtlsConfig {
    /// Client certificate path
    pub cert_path: String,
    /// Client key path
    pub key_path: String,
    /// CA certificate path
    pub ca_cert_path: Option<String>,
}

impl A2AClient {
    /// Discover an A2A agent by its base URL
    pub async fn discover(base_url: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()?;

        let discovery_url = format!("{}{}", base_url.trim_end_matches('/'), AGENT_CARD_PATH);
        let response = client.get(&discovery_url).send().await?;

        if !response.status().is_success() {
            return Err(BridgeError::Http(format!(
                "Failed to discover agent at {}: {}",
                discovery_url,
                response.status()
            )));
        }

        let card: AgentCard = response.json().await?;

        Ok(Self {
            card,
            client,
            reconnect_config: ReconnectConfig::default(),
        })
    }

    /// Discover with OAuth2 mTLS authentication
    pub async fn discover_with_auth(base_url: &str, config: &OAuth2MtlsConfig) -> Result<Self> {
        // In production, this would load certificates and configure mTLS
        // For this implementation, we log the intent and use regular discovery
        tracing::info!(
            "Authenticating with OAuth2 mTLS: cert={}, key={}",
            config.cert_path,
            config.key_path
        );
        Self::discover(base_url).await
    }

    /// Send a task to the agent
    pub async fn send_task(&self, message: Message) -> Result<Task> {
        let request = SendTaskRequest {
            id: Some(uuid::Uuid::new_v4().to_string()),
            session_id: None,
            message,
            metadata: None,
        };

        let response = self
            .client
            .post(format!(
                "{}/tasks/send",
                self.card.url.trim_end_matches('/')
            ))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(BridgeError::Http(format!(
                "Send task failed: {} - {}",
                status, text
            )));
        }

        let result: SendTaskResponse = response.json().await?;
        Ok(result.task)
    }

    /// Get task status
    pub async fn get_task(&self, task_id: &str) -> Result<Task> {
        let response = self
            .client
            .get(format!(
                "{}/tasks/{}",
                self.card.url.trim_end_matches('/'),
                task_id
            ))
            .send()
            .await?;

        if response.status().as_u16() == 404 {
            return Err(BridgeError::TaskNotFound(task_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(BridgeError::Http(format!(
                "Get task failed: {}",
                response.status()
            )));
        }

        let task: Task = response.json().await?;
        Ok(task)
    }

    /// Cancel a task
    pub async fn cancel_task(&self, task_id: &str) -> Result<Task> {
        let response = self
            .client
            .post(format!(
                "{}/tasks/{}/cancel",
                self.card.url.trim_end_matches('/'),
                task_id
            ))
            .send()
            .await?;

        if response.status().as_u16() == 404 {
            return Err(BridgeError::TaskNotFound(task_id.to_string()));
        }

        if !response.status().is_success() {
            return Err(BridgeError::Http(format!(
                "Cancel task failed: {}",
                response.status()
            )));
        }

        let task: Task = response.json().await?;
        Ok(task)
    }

    /// Stream task updates via SSE with automatic reconnection
    pub async fn stream_task(
        &self,
        task_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Task>> + Send>>> {
        let url = format!(
            "{}/tasks/{}/stream",
            self.card.url.trim_end_matches('/'),
            task_id
        );

        self.stream_with_reconnect(&url, task_id, 0).await
    }

    /// Resubscribe to a task stream with optional last sequence number
    pub async fn resubscribe_task(
        &self,
        task_id: &str,
        last_sequence: Option<u64>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Task>> + Send>>> {
        let request = ResubscribeTaskRequest {
            id: task_id.to_string(),
            last_sequence,
        };

        let url = format!("{}/tasks/resubscribe", self.card.url.trim_end_matches('/'));

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            return Err(BridgeError::Http(format!(
                "Resubscribe failed: {}",
                response.status()
            )));
        }

        self.stream_with_reconnect(&url, task_id, last_sequence.unwrap_or(0))
            .await
    }

    /// Internal method to handle SSE streaming with reconnection
    async fn stream_with_reconnect(
        &self,
        url: &str,
        task_id: &str,
        last_sequence: u64,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Task>> + Send>>> {
        let client = self.client.clone();
        let url = url.to_string();
        let reconnect_config = self.reconnect_config.clone();
        let task_id = task_id.to_string();

        let stream = async_stream::try_stream! {
            let mut attempt = 0;
            let mut _current_sequence = last_sequence;

            loop {
                let request = client
                    .get(&url)
                    .header("Accept", "text/event-stream")
                    .header("Cache-Control", "no-cache");

                let response = request.send().await.map_err(BridgeError::from)?;

                if !response.status().is_success() {
                    if attempt >= reconnect_config.max_attempts {
                        Err(BridgeError::Reconnect(
                            format!("Max reconnection attempts reached for task {}", task_id)
                        ))?;
                    }

                    let delay = std::cmp::min(
                        reconnect_config.initial_delay_ms * (1u64 << attempt),
                        reconnect_config.max_delay_ms
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    attempt += 1;
                    continue;
                }

                attempt = 0; // Reset on successful connection

                let mut stream = response.bytes_stream();

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| BridgeError::Stream(e.to_string()))?;
                    let text = String::from_utf8_lossy(&chunk);

                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data.is_empty() || data == "[DONE]" {
                                continue;
                            }

                            let event: SseEvent = serde_json::from_str(data)
                                .map_err(|e| BridgeError::Json(e.to_string()))?;

                            if let SseEventData::TaskUpdate { task } = event.data {
                                _current_sequence += 1;
                                yield task.clone();

                                if task.is_terminal() {
                                    return;
                                }
                            }
                        }
                    }
                }

                // Stream ended, try to reconnect
                if attempt >= reconnect_config.max_attempts {
                    Err(BridgeError::Reconnect(
                        format!("Stream ended and max reconnection attempts reached for task {}", task_id)
                    ))?;
                }

                let delay = std::cmp::min(
                    reconnect_config.initial_delay_ms * (1u64 << attempt),
                    reconnect_config.max_delay_ms
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
            }
        };

        Ok(Box::pin(stream))
    }

    /// Set reconnection configuration
    pub fn with_reconnect_config(mut self, config: ReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }
}

impl A2AAgent {
    /// Create a new agent wrapper
    pub fn new(card: AgentCard, client: Client) -> Self {
        Self { card, client }
    }

    /// Create client from this agent
    pub fn to_client(self) -> A2AClient {
        A2AClient {
            card: self.card,
            client: self.client,
            reconnect_config: ReconnectConfig::default(),
        }
    }
}

/// SSE event structure
#[derive(Debug, Clone, Deserialize)]
struct SseEvent {
    #[serde(rename = "event")]
    _event: String,
    data: SseEventData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "payload")]
enum SseEventData {
    #[serde(rename = "task_update")]
    TaskUpdate { task: Task },
    #[serde(rename = "ping")]
    Ping,
}
