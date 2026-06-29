use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for A2A entities
pub type A2AId = String;

/// Agent Card describing an A2A agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCard {
    /// Agent name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Agent version
    pub version: String,
    /// Base URL for agent endpoints
    pub url: String,
    /// Capabilities supported by this agent
    pub capabilities: Capabilities,
    /// Authentication configuration
    pub authentication: Option<Authentication>,
    /// Default input modes
    pub default_input_modes: Vec<String>,
    /// Default output modes
    pub default_output_modes: Vec<String>,
    /// Skills offered by this agent
    pub skills: Vec<Skill>,
    /// Additional metadata
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    /// Supports streaming updates
    pub streaming: bool,
    /// Supports push notifications
    pub push_notifications: bool,
    /// State transition history
    pub state_transition_history: bool,
    /// Maximum task history length
    #[serde(rename = "task_history_count")]
    pub history_count: Option<usize>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Authentication {
    /// Authentication scheme (e.g., "oauth2", "mtls", "apikey")
    pub scheme: String,
    /// Additional configuration
    #[serde(flatten)]
    pub config: HashMap<String, serde_json::Value>,
}

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    /// Skill ID
    pub id: A2AId,
    /// Skill name
    pub name: String,
    /// Skill description
    pub description: String,
    /// Input parameters
    pub input_modes: Vec<String>,
    /// Output modalities
    pub output_modes: Vec<String>,
}

/// A2A Task representing a unit of work
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// Task ID
    pub id: A2AId,
    /// Task session ID
    pub session_id: Option<A2AId>,
    /// Task status
    #[serde(flatten)]
    pub status: TaskStatus,
    /// Task history
    pub history: Vec<TaskStatus>,
    /// Task artifacts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    /// Task metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Task status information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatus {
    /// Current state
    pub state: TaskState,
    /// Status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Timestamp
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
}

/// Task states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    /// Task was submitted
    Submitted,
    /// Task is being worked on
    Working,
    /// Task completed with input required
    InputRequired,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Task artifact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    /// Artifact name
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// Artifact data (base64 encoded)
    pub data: String,
}

/// Send task request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTaskRequest {
    /// Task ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<A2AId>,
    /// Session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<A2AId>,
    /// Task message
    pub message: Message,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Send task response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTaskResponse {
    /// Task result
    pub task: Task,
}

/// Get task request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskRequest {
    /// Task ID
    pub id: A2AId,
}

/// Cancel task request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    /// Task ID
    pub id: A2AId,
}

/// Resubscribe task request for SSE streams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResubscribeTaskRequest {
    /// Task ID
    pub id: A2AId,
    /// Last known sequence number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u64>,
}

/// A2A Message for task communication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// Message role
    pub role: Role,
    /// Message parts
    pub parts: Vec<Part>,
}

/// Message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Agent,
}

/// Message part (text or file)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    File {
        name: String,
        mime_type: String,
        bytes: String,
    },
}

/// SSE task update event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpdateEvent {
    /// Event type
    pub event: String,
    /// Task data
    pub data: Task,
}

/// A2A error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl Task {
    /// Create a new task with the given ID
    pub fn new(id: A2AId) -> Self {
        Self {
            id,
            session_id: None,
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: Utc::now(),
            },
            history: Vec::new(),
            artifacts: None,
            metadata: None,
        }
    }

    /// Transition the task to a new state
    pub fn transition(&mut self, state: TaskState, message: Option<&str>) {
        let old_status = self.status.clone();
        self.history.push(old_status);
        self.status = TaskStatus {
            state,
            message: message.map(|s| s.to_string()),
            timestamp: Utc::now(),
        };
    }

    /// Check if the task is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.state,
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_transition() {
        let mut task = Task::new("task-123".to_string());
        assert_eq!(task.status.state, TaskState::Submitted);

        task.transition(TaskState::Working, Some("Started processing"));
        assert_eq!(task.status.state, TaskState::Working);
        assert_eq!(task.history.len(), 1);
        assert_eq!(task.history[0].state, TaskState::Submitted);

        task.transition(TaskState::Completed, None);
        assert!(task.is_terminal());
    }

    #[test]
    fn test_agent_card_serialization() -> serde_json::Result<()> {
        let card = AgentCard {
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            version: "1.0.0".to_string(),
            url: "https://example.com/agent".to_string(),
            capabilities: Capabilities {
                streaming: true,
                push_notifications: false,
                state_transition_history: true,
                history_count: Some(10),
            },
            authentication: Some(Authentication {
                scheme: "oauth2".to_string(),
                config: HashMap::new(),
            }),
            default_input_modes: vec!["text".to_string()],
            default_output_modes: vec!["text".to_string()],
            skills: vec![Skill {
                id: "skill-1".to_string(),
                name: "Echo".to_string(),
                description: "Echoes input".to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            }],
            extras: HashMap::new(),
        };

        let json = serde_json::to_string(&card)?;
        assert!(json.contains("Test Agent"));
        assert!(json.contains("oauth2"));

        let deserialized: AgentCard = serde_json::from_str(&json)?;
        assert_eq!(card, deserialized);
        Ok(())
    }

    #[test]
    fn test_message_part_serialization() -> serde_json::Result<()> {
        let part = Part::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&part)?;
        assert!(json.contains("Hello"));
        assert!(json.contains("text"));

        let deserialized: Part = serde_json::from_str(&json)?;
        assert_eq!(part, deserialized);
        Ok(())
    }
}
