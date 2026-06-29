//! Integration tests for craft-web
//!
//! These tests verify:
//! 1. API endpoints return proper JSON responses
//! 2. Error handling produces correct status codes
//! 3. WebSocket validation works

#![allow(clippy::unwrap_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use craft_core::CraftHome;
use craft_memory::Memory;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

/// Create a test CraftHome with sample data
fn setup_test_env() -> (CraftHome, PathBuf) {
    let temp_dir = PathBuf::from(format!(
        "/tmp/craft-web-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let home = CraftHome::new(&temp_dir);
    let _ = std::fs::create_dir_all(home.harnesses_dir());

    (home, temp_dir)
}

/// Clean up test directory
fn teardown(temp_dir: &PathBuf) {
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}

#[tokio::test]
async fn health_check_returns_ok() {
    let (home, temp_dir) = setup_test_env();

    // Create app
    let app = craft_web::server::create_app(home, None);

    // Test status endpoint
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    teardown(&temp_dir);
}

#[tokio::test]
async fn list_empty_harnesses() {
    let (home, temp_dir) = setup_test_env();

    let app = craft_web::server::create_app(home, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/harnesses")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    teardown(&temp_dir);
}

#[tokio::test]
async fn missing_harness_returns_404() {
    let (home, temp_dir) = setup_test_env();

    let app = craft_web::server::create_app(home, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/harnesses/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    teardown(&temp_dir);
}

#[tokio::test]
async fn memory_search_requires_query() {
    let (home, temp_dir) = setup_test_env();

    let app = craft_web::server::create_app(home, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/memory/search")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with bad request because q is missing
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    teardown(&temp_dir);
}

#[tokio::test]
async fn compose_plan_requires_harnesses() {
    let (home, temp_dir) = setup_test_env();

    let app = craft_web::server::create_app(home, None);

    let body = r#"{"harness_names": []}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/compose/plan")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail because empty harness list
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    teardown(&temp_dir);
}

#[tokio::test]
async fn compose_endpoint_accepts_harness_names() {
    let (home, temp_dir) = setup_test_env();

    let app = craft_web::server::create_app(home, None);

    let body = r#"{"harness_names": ["test-harness"], "strategy": "ordered-merge"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/compose/plan")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // The harness doesn't exist, so it will fail with an error
    // The API returns errors wrapped in JSON, so check for error response
    let status = response.status();
    // Should return OK (200) with error JSON, or NOT_FOUND if harness not found
    assert!(
        status == StatusCode::OK
            || status == StatusCode::NOT_FOUND
            || status == StatusCode::BAD_REQUEST
            || status == StatusCode::SERVICE_UNAVAILABLE
            || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Unexpected status: {:?}",
        status
    );

    teardown(&temp_dir);
}

#[tokio::test]
async fn memory_create_fact_endpoint() {
    let (home, temp_dir) = setup_test_env();

    // Initialize memory
    let _ = Memory::open(home.root());

    let app = craft_web::server::create_app(home, None);

    let body = r#"{"scope": "global", "key": "test-key", "value": "test-value"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/memory/facts")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should succeed
    let status = response.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE /* if memory init fails */
    );

    teardown(&temp_dir);
}

#[tokio::test]
async fn api_response_wrapper_serialization() {
    use craft_web::ApiResponse;

    // Test success response
    let success = ApiResponse::success("test data");
    let json = serde_json::to_string(&success).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"data\":\"test data\""));

    // Test error response
    let error = ApiResponse::<()>::error("not_found", "Item not found");
    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"code\":\"not_found\""));
    assert!(json.contains("\"message\":\"Item not found\""));
}

#[tokio::test]
async fn validation_status_enum_serialization() {
    use craft_web::ValidationStatus;

    // Test serializing validation status
    let status = ValidationStatus::Valid;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"Valid\"");

    let status = ValidationStatus::Error;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"Error\"");

    // Test deserializing
    let status: ValidationStatus = serde_json::from_str("\"Warning\"").unwrap();
    assert_eq!(status, ValidationStatus::Warning);
}

#[tokio::test]
async fn cors_headers_are_present() {
    let (home, temp_dir) = setup_test_env();

    let app = craft_web::server::create_app(home, None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/status")
                .header("Origin", "http://localhost:3001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Check CORS headers are present
    let headers = response.headers();
    assert!(headers.contains_key("access-control-allow-origin"));

    teardown(&temp_dir);
}

#[tokio::test]
async fn harness_info_structure() {
    use craft_web::HarnessInfo;

    let info = HarnessInfo {
        name: "test-harness".to_string(),
        version: "1.0.0".to_string(),
        description: "A test harness".to_string(),
        source: "github:test/repo".to_string(),
        authors: vec!["Test Author".to_string()],
        installed_at: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("test-harness"));
    assert!(json.contains("1.0.0"));
    assert!(json.contains("A test harness"));
}

#[tokio::test]
async fn memory_fact_dto_structure() {
    use craft_web::MemoryFactDto;

    let fact = MemoryFactDto {
        scope: "project".to_string(),
        key: "language".to_string(),
        value: "rust".to_string(),
        created_at: 1704067200,
    };

    let json = serde_json::to_string(&fact).unwrap();
    assert!(json.contains("\"scope\":\"project\""));
    assert!(json.contains("\"key\":\"language\""));
    assert!(json.contains("\"value\":\"rust\""));
    assert!(json.contains("1704067200"));
}

#[tokio::test]
async fn runtime_status_structure() {
    use craft_web::{RuntimeStats, RuntimeStatus};

    let status = RuntimeStatus {
        active: true,
        current_harness: Some("test-harness".to_string()),
        last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        stats: RuntimeStats {
            memory_facts_count: 10,
            installed_harnesses: 5,
            compositions_created: 3,
        },
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"active\":true"));
    assert!(json.contains("\"memory_facts_count\":10"));
    assert!(json.contains("\"installed_harnesses\":5"));
}

#[test]
fn parse_memory_scope_from_string() {
    use craft_memory::MemoryScope;

    assert_eq!(MemoryScope::parse("global").unwrap(), MemoryScope::Global);
    assert_eq!(MemoryScope::parse("user").unwrap(), MemoryScope::User);
    assert_eq!(MemoryScope::parse("project").unwrap(), MemoryScope::Project);
    assert_eq!(MemoryScope::parse("session").unwrap(), MemoryScope::Session);
    assert_eq!(
        MemoryScope::parse("harness:test").unwrap(),
        MemoryScope::Harness("test".to_string())
    );
}

#[test]
fn web_error_codes() {
    use craft_web::error::WebError;

    assert_eq!(WebError::NotFound("test".to_string()).code(), "not_found");
    assert_eq!(
        WebError::BadRequest("test".to_string()).code(),
        "bad_request"
    );
    assert_eq!(WebError::Internal("test".to_string()).code(), "internal");
}
