use crate::a2a::types::*;
use crate::error::{BridgeError, Result as BridgeResult};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

/// A2A Server for hosting an agent
#[derive(Debug, Clone)]
pub struct A2AServer {
    /// Agent card
    pub card: AgentCard,
    /// Task storage
    tasks: Arc<RwLock<HashMap<A2AId, Task>>>,
    /// Event sender for SSE
    event_tx: broadcast::Sender<Task>,
}

impl A2AServer {
    /// Create a new A2A server
    pub fn new(card: AgentCard) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            card,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Create router for the A2A server
    pub fn router(self) -> Router {
        let state = Arc::new(self);

        Router::new()
            .route("/.well-known/agent.json", get(get_agent_card))
            .route("/tasks/send", post(send_task))
            .route("/tasks/{id}", get(get_task))
            .route("/tasks/{id}/cancel", post(cancel_task))
            .route("/tasks/{id}/stream", get(stream_task))
            .route("/tasks/resubscribe", post(resubscribe_task))
            .with_state(state)
    }

    /// Store a task
    pub fn store_task(&self, task: Task) -> BridgeResult<()> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| BridgeError::Protocol("Failed to lock task storage".to_string()))?;
        tasks.insert(task.id.clone(), task.clone());
        drop(tasks);

        // Broadcast update
        let _ = self.event_tx.send(task);
        Ok(())
    }

    /// Get a task by ID
    pub fn get_task(&self, task_id: &str) -> BridgeResult<Option<Task>> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| BridgeError::Protocol("Failed to lock task storage".to_string()))?;
        Ok(tasks.get(task_id).cloned())
    }

    /// Update a task
    pub fn update_task(&self, task: Task) -> BridgeResult<()> {
        self.store_task(task)
    }

    /// Get all tasks
    pub fn list_tasks(&self) -> BridgeResult<Vec<Task>> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| BridgeError::Protocol("Failed to lock task storage".to_string()))?;
        Ok(tasks.values().cloned().collect())
    }
}

/// Get agent card
async fn get_agent_card(
    State(state): State<Arc<A2AServer>>,
) -> std::result::Result<Json<AgentCard>, StatusCode> {
    Ok(Json(state.card.clone()))
}

/// Send task handler
async fn send_task(
    State(state): State<Arc<A2AServer>>,
    Json(request): Json<SendTaskRequest>,
) -> std::result::Result<Json<SendTaskResponse>, StatusCode> {
    let task_id = request
        .id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut task = Task::new(task_id.clone());
    task.session_id = request.session_id;
    task.metadata = request.metadata;

    // Store initial task
    state
        .store_task(task.clone())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Transition to working state
    task.transition(TaskState::Working, Some("Task accepted"));
    state
        .store_task(task.clone())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Task execution workers are outside this server surface; callers can poll the stored task.
    Ok(Json(SendTaskResponse { task }))
}

/// Get task handler
async fn get_task(
    State(state): State<Arc<A2AServer>>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Task>, StatusCode> {
    let task = state
        .get_task(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match task {
        Some(task) => Ok(Json(task)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Cancel task handler
async fn cancel_task(
    State(state): State<Arc<A2AServer>>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Task>, StatusCode> {
    let task = state
        .get_task(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match task {
        Some(mut task) => {
            if task.is_terminal() {
                return Err(StatusCode::BAD_REQUEST);
            }
            task.transition(TaskState::Cancelled, Some("Cancelled by user"));
            state
                .store_task(task.clone())
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Json(task))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Stream task updates via SSE
async fn stream_task(
    State(state): State<Arc<A2AServer>>,
    Path(id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let task_id = id.clone();
    let rx = state.event_tx.subscribe();

    let stream =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| match result {
            Ok(task) if task.id == task_id => {
                let event = Event::default()
                    .event("task_update")
                    .data(serde_json::to_string(&task).unwrap_or_default());
                Some(Ok(event))
            }
            Ok(_) => None,
            Err(_) => None,
        });

    Sse::new(stream)
}

/// Resubscribe to task updates
async fn resubscribe_task(
    State(state): State<Arc<A2AServer>>,
    Json(request): Json<ResubscribeTaskRequest>,
) -> std::result::Result<
    Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>>,
    StatusCode,
> {
    // Get current task state
    let task = state
        .get_task(&request.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match task {
        Some(_task) => {
            let rx = state.event_tx.subscribe();

            let stream =
                tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| {
                    match result {
                        Ok(t) if t.id == request.id => {
                            let event = Event::default()
                                .event("task_update")
                                .data(serde_json::to_string(&t).unwrap_or_default());
                            Some(Ok(event))
                        }
                        Ok(_) => None,
                        Err(_) => None,
                    }
                });

            Ok(Sse::new(stream))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    fn create_test_agent_card() -> AgentCard {
        AgentCard {
            name: "Test Server".to_string(),
            description: "Test A2A server".to_string(),
            version: "1.0.0".to_string(),
            url: "http://localhost:8080".to_string(),
            capabilities: Capabilities {
                streaming: true,
                push_notifications: false,
                state_transition_history: true,
                history_count: Some(100),
            },
            authentication: None,
            default_input_modes: vec!["text".to_string()],
            default_output_modes: vec!["text".to_string()],
            skills: vec![],
            extras: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_get_agent_card() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let card = create_test_agent_card();
        let server = A2AServer::new(card.clone());
        let app = server.router();

        let response = app
            .oneshot(axum::http::Request::get("/.well-known/agent.json").body(Body::empty())?)
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn test_send_task() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let card = create_test_agent_card();
        let server = A2AServer::new(card);
        let app = server.router();

        let request = SendTaskRequest {
            id: Some("test-task-123".to_string()),
            session_id: None,
            message: Message {
                role: Role::User,
                parts: vec![Part::Text {
                    text: "Hello".to_string(),
                }],
            },
            metadata: None,
        };

        let response = app
            .oneshot(
                axum::http::Request::post("/tasks/send")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
