//! WebSocket handlers for real-time features

use axum::{
    extract::Extension,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::server::AppState;
use crate::{ValidationRequest, ValidationResult, ValidationStatus};

/// WebSocket message protocol for validation
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ValidationMessage {
    #[serde(rename = "validate")]
    Validate { request: ValidationRequest },
    #[serde(rename = "result")]
    Result { result: ValidationResult },
    #[serde(rename = "progress")]
    Progress {
        harness_name: String,
        status: String,
    },
    #[serde(rename = "complete")]
    Complete,
    #[serde(rename = "error")]
    Error { message: String },
}

/// Handler for /ws/validate WebSocket endpoint
pub async fn validation_handler(
    ws: WebSocketUpgrade,
    Extension(_state): Extension<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(handle_validation_socket)
}

async fn handle_validation_socket(mut socket: WebSocket) {
    info!("WebSocket connection established for validation");

    while let Some(result) = socket.recv().await {
        match result {
            Ok(Message::Text(text)) => {
                if let Err(e) = handle_validation_message(&mut socket, &text).await {
                    warn!("Validation message error: {}", e);
                    let _ = send_error(&mut socket, &e).await;
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket connection closed");
                break;
            }
            Err(e) => {
                warn!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    info!("WebSocket handler ended");
}

async fn handle_validation_message(socket: &mut WebSocket, text: &str) -> Result<(), String> {
    let message: ValidationMessage =
        serde_json::from_str(text).map_err(|e| format!("Failed to parse message: {}", e))?;

    match message {
        ValidationMessage::Validate { request } => {
            info!(
                "Starting validation for {} harnesses",
                request.harness_names.len()
            );

            // Send progress updates for each harness
            for harness_name in &request.harness_names {
                let progress = ValidationMessage::Progress {
                    harness_name: harness_name.clone(),
                    status: "validating".to_string(),
                };
                send_message(socket, &progress).await?;

                // Simulate validation delay
                tokio::time::sleep(Duration::from_millis(100)).await;

                // Validate the harness exists
                let result = validate_harness(harness_name).await;
                send_message(socket, &ValidationMessage::Result { result }).await?;
            }

            send_message(socket, &ValidationMessage::Complete).await?;
        }
        _ => {
            warn!("Unexpected message type in validation socket");
        }
    }

    Ok(())
}

async fn validate_harness(harness_name: &str) -> ValidationResult {
    // Attempt to validate harness by checking registry
    let validation_result = tokio::time::timeout(Duration::from_secs(5), async {
        // Try to open registry and check if harness exists
        match validate_harness_exists(harness_name).await {
            Ok(true) => ValidationResult {
                status: ValidationStatus::Valid,
                message: format!("Harness `{}` is valid", harness_name),
                details: Some({
                    let mut map = HashMap::new();
                    map.insert("name".to_string(), harness_name.to_string());
                    map
                }),
            },
            Ok(false) => ValidationResult {
                status: ValidationStatus::Error,
                message: format!("Harness `{}` not installed", harness_name),
                details: Some({
                    let mut map = HashMap::new();
                    map.insert("name".to_string(), harness_name.to_string());
                    map
                }),
            },
            Err(e) => ValidationResult {
                status: ValidationStatus::Error,
                message: format!("Error validating `{}`: {}", harness_name, e),
                details: None,
            },
        }
    })
    .await;

    match validation_result {
        Ok(result) => result,
        Err(_) => ValidationResult {
            status: ValidationStatus::Error,
            message: format!("Validation timeout for `{}`", harness_name),
            details: None,
        },
    }
}

async fn send_message(socket: &mut WebSocket, message: &ValidationMessage) -> Result<(), String> {
    let json = serde_json::to_string(message)
        .map_err(|e| format!("Failed to serialize message: {}", e))?;
    socket
        .send(Message::Text(json))
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;
    Ok(())
}

async fn send_error(socket: &mut WebSocket, error: &str) -> Result<(), String> {
    let message = ValidationMessage::Error {
        message: error.to_string(),
    };
    send_message(socket, &message).await
}

/// Helper function to check if a harness exists
async fn validate_harness_exists(name: &str) -> Result<bool, crate::error::WebError> {
    use craft_core::{CraftHome, HarnessRegistry};

    let home = CraftHome::from_env()
        .map_err(|e| crate::error::WebError::Internal(format!("CRAFT_HOME error: {}", e)))?;

    let registry = HarnessRegistry::open(home.registry_path())?;

    // Try to get the harness info - if it fails, the harness doesn't exist
    match registry.info(name) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
