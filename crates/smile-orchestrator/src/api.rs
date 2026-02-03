//! HTTP API endpoints for the SMILE Loop orchestrator.
//!
//! This module provides the REST API used by agent wrappers running inside
//! Docker containers to report results back to the orchestrator.
//!
//! # Endpoints
//!
//! - `POST /api/student/result` - Report student agent result
//! - `POST /api/mentor/result` - Report mentor agent result
//! - `GET /api/status` - Get current loop status
//! - `POST /api/stop` - Force stop the loop
//!
//! # Example
//!
//! ```no_run
//! use smile_orchestrator::{AppState, Config, LoopState, create_router};
//! use std::sync::Arc;
//! use tokio::sync::Mutex;
//!
//! # async fn example() {
//! let state = AppState {
//!     config: Config::default(),
//!     loop_state: Arc::new(Mutex::new(LoopState::new())),
//! };
//!
//! let router = create_router(state);
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//! axum::serve(listener, router).await.unwrap();
//! # }
//! ```

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{info, warn};

use crate::{Config, LoopState, LoopStatus, StudentOutput};

// ============================================================================
// Request/Response Types
// ============================================================================

/// The next action the wrapper should take after reporting a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NextAction {
    /// Continue the loop (student continues or mentor answers).
    Continue,
    /// Stop the loop (terminal state reached).
    Stop,
}

/// Request body for the student result endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentResultRequest {
    /// The student agent's output.
    pub student_output: StudentOutput,
    /// Timestamp when the result was produced.
    pub timestamp: DateTime<Utc>,
}

/// Response body for the student result endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentResultResponse {
    /// Whether the result was acknowledged.
    pub acknowledged: bool,
    /// The next action for the wrapper.
    pub next_action: NextAction,
}

/// Request body for the mentor result endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentorResultRequest {
    /// The mentor agent's notes/advice.
    pub mentor_output: String,
    /// Timestamp when the result was produced.
    pub timestamp: DateTime<Utc>,
}

/// Response body for the mentor result endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentorResultResponse {
    /// Whether the result was acknowledged.
    pub acknowledged: bool,
    /// The next action for the wrapper.
    pub next_action: NextAction,
}

/// Request body for the stop endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct StopRequest {
    /// Reason for stopping the loop.
    pub reason: String,
}

/// Response body for the stop endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopResponse {
    /// Whether the loop was stopped.
    pub stopped: bool,
    /// The final state of the loop.
    pub final_state: LoopState,
}

/// Error response body returned on failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Description of the error.
    pub error: String,
}

// ============================================================================
// Application State
// ============================================================================

/// Shared application state for the HTTP server.
///
/// Contains the configuration and the mutable loop state, both wrapped
/// for thread-safe sharing across handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Configuration for the orchestrator.
    pub config: Config,
    /// Current state of the SMILE loop.
    pub loop_state: Arc<Mutex<LoopState>>,
}

impl AppState {
    /// Creates a new `AppState` with the given configuration.
    ///
    /// Initializes the loop state to `Starting`.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            loop_state: Arc::new(Mutex::new(LoopState::new())),
        }
    }

    /// Creates a new `AppState` with an existing loop state.
    ///
    /// Useful for crash recovery when restoring state from disk.
    #[must_use]
    pub fn with_state(config: Config, loop_state: LoopState) -> Self {
        Self {
            config,
            loop_state: Arc::new(Mutex::new(loop_state)),
        }
    }
}

// ============================================================================
// API Error Type
// ============================================================================

/// Internal error type for API handlers.
#[derive(Debug)]
enum ApiError {
    /// Loop is not in a state that accepts this request.
    LoopNotRunning(String),
    /// State transition failed.
    StateTransition(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::LoopNotRunning(msg) | Self::StateTransition(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, msg)
            }
        };

        let body = Json(ErrorResponse { error: message });
        (status, body).into_response()
    }
}

// ============================================================================
// Router Setup
// ============================================================================

/// Creates the HTTP router with all API endpoints.
///
/// # Arguments
///
/// * `state` - The shared application state
///
/// # Returns
///
/// An axum `Router` configured with:
/// - All API routes under `/api`
/// - CORS middleware for development
/// - Tracing middleware for request logging
pub fn create_router(state: AppState) -> Router {
    // Configure CORS for development (allow all origins)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build the API routes
    let api_routes = Router::new()
        .route("/student/result", post(handle_student_result))
        .route("/mentor/result", post(handle_mentor_result))
        .route("/status", get(handle_status))
        .route("/stop", post(handle_stop));

    // Combine with state and middleware
    Router::new()
        .nest("/api", api_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(Arc::new(state))
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `POST /api/student/result`.
///
/// Processes the student agent's result and transitions the loop state.
async fn handle_student_result(
    State(state): State<Arc<AppState>>,
    Json(request): Json<StudentResultRequest>,
) -> Result<Json<StudentResultResponse>, ApiError> {
    info!(
        status = ?request.student_output.status,
        step = %request.student_output.current_step,
        "Received student result"
    );

    let mut loop_state = state.loop_state.lock().await;

    // Check if loop is in a valid state to receive student results
    if !matches!(loop_state.status, LoopStatus::WaitingForStudent) {
        warn!(
            current_status = %loop_state.status,
            "Cannot accept student result: loop not waiting for student"
        );
        return Err(ApiError::LoopNotRunning(format!(
            "Loop is not waiting for student result (current status: {})",
            loop_state.status
        )));
    }

    // Process the result
    loop_state
        .receive_student_result(request.student_output, state.config.max_iterations)
        .map_err(|e| ApiError::StateTransition(e.to_string()))?;

    // Determine next action based on new state
    let next_action = if loop_state.is_terminal() {
        NextAction::Stop
    } else {
        NextAction::Continue
    };

    info!(
        new_status = %loop_state.status,
        next_action = ?next_action,
        "Student result processed"
    );

    Ok(Json(StudentResultResponse {
        acknowledged: true,
        next_action,
    }))
}

/// Handler for `POST /api/mentor/result`.
///
/// Processes the mentor agent's result and transitions the loop state.
async fn handle_mentor_result(
    State(state): State<Arc<AppState>>,
    Json(request): Json<MentorResultRequest>,
) -> Result<Json<MentorResultResponse>, ApiError> {
    info!(
        output_len = request.mentor_output.len(),
        "Received mentor result"
    );

    let mut loop_state = state.loop_state.lock().await;

    // Check if loop is in a valid state to receive mentor results
    if !matches!(loop_state.status, LoopStatus::WaitingForMentor) {
        warn!(
            current_status = %loop_state.status,
            "Cannot accept mentor result: loop not waiting for mentor"
        );
        return Err(ApiError::LoopNotRunning(format!(
            "Loop is not waiting for mentor result (current status: {})",
            loop_state.status
        )));
    }

    // Get the current question before we clear it
    let question = loop_state
        .current_question
        .clone()
        .unwrap_or_else(|| "Unknown question".to_string());

    // Process the result
    loop_state
        .receive_mentor_result(request.mentor_output, question)
        .map_err(|e| ApiError::StateTransition(e.to_string()))?;

    // Determine next action based on new state
    let next_action = if loop_state.is_terminal() {
        NextAction::Stop
    } else {
        NextAction::Continue
    };

    info!(
        new_status = %loop_state.status,
        iteration = loop_state.iteration,
        next_action = ?next_action,
        "Mentor result processed"
    );

    Ok(Json(MentorResultResponse {
        acknowledged: true,
        next_action,
    }))
}

/// Handler for `GET /api/status`.
///
/// Returns the current state of the SMILE loop.
async fn handle_status(State(state): State<Arc<AppState>>) -> Json<LoopState> {
    let loop_state = state.loop_state.lock().await;
    Json(loop_state.clone())
}

/// Handler for `POST /api/stop`.
///
/// Forces the loop into an error state with the given reason.
async fn handle_stop(
    State(state): State<Arc<AppState>>,
    Json(request): Json<StopRequest>,
) -> Result<Json<StopResponse>, ApiError> {
    info!(reason = %request.reason, "Stop request received");

    let mut loop_state = state.loop_state.lock().await;

    // Check if loop is already terminal
    if loop_state.is_terminal() {
        warn!(
            current_status = %loop_state.status,
            "Cannot stop: loop already in terminal state"
        );
        return Err(ApiError::LoopNotRunning(format!(
            "Loop is already stopped (current status: {})",
            loop_state.status
        )));
    }

    // Transition to error state
    loop_state
        .error(request.reason)
        .map_err(|e| ApiError::StateTransition(e.to_string()))?;

    info!(final_status = %loop_state.status, "Loop stopped");

    Ok(Json(StopResponse {
        stopped: true,
        final_state: loop_state.clone(),
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use tower::util::ServiceExt;

    use super::*;
    use crate::{LoopStatus, StudentStatus};

    /// Creates a test app state with a fresh loop state.
    fn test_state() -> AppState {
        AppState::new(Config::default())
    }

    /// Creates a test app state with loop ready to receive student result.
    async fn state_waiting_for_student() -> AppState {
        let state = test_state();
        let mut loop_state = state.loop_state.lock().await;
        loop_state.start().unwrap();
        loop_state.start_waiting_for_student().unwrap();
        drop(loop_state);
        state
    }

    /// Creates a test app state with loop ready to receive mentor result.
    async fn state_waiting_for_mentor() -> AppState {
        let state = test_state();
        let mut loop_state = state.loop_state.lock().await;
        loop_state.start().unwrap();
        loop_state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 1".to_string(),
            question_for_mentor: Some("Help?".to_string()),
            summary: "Need help".to_string(),
            ..Default::default()
        };
        loop_state.receive_student_result(output, 10).unwrap();
        loop_state.start_waiting_for_mentor().unwrap();
        drop(loop_state);
        state
    }

    // ------------------------------------------------------------------------
    // Status endpoint tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_status_returns_loop_state() {
        let state = test_state();
        let router = create_router(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let state: LoopState = serde_json::from_slice(&body).unwrap();

        assert_eq!(state.status, LoopStatus::Starting);
        assert_eq!(state.iteration, 0);
    }

    // ------------------------------------------------------------------------
    // Student result endpoint tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_student_result_success_completed() {
        let state = state_waiting_for_student().await;
        let router = create_router(state);

        let request_body = serde_json::json!({
            "studentOutput": {
                "status": "completed",
                "current_step": "Final step",
                "attempted_actions": ["finished"],
                "summary": "All done!"
            },
            "timestamp": "2026-02-03T10:00:00Z"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/student/result")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: StudentResultResponse = serde_json::from_slice(&body).unwrap();

        assert!(response.acknowledged);
        assert_eq!(response.next_action, NextAction::Stop);
    }

    #[tokio::test]
    async fn test_student_result_success_ask_mentor() {
        let state = state_waiting_for_student().await;
        let router = create_router(state);

        let request_body = serde_json::json!({
            "studentOutput": {
                "status": "ask_mentor",
                "current_step": "Step 2",
                "attempted_actions": ["tried something"],
                "question_for_mentor": "How do I do this?",
                "summary": "Stuck on step 2"
            },
            "timestamp": "2026-02-03T10:00:00Z"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/student/result")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: StudentResultResponse = serde_json::from_slice(&body).unwrap();

        assert!(response.acknowledged);
        assert_eq!(response.next_action, NextAction::Continue);
    }

    #[tokio::test]
    async fn test_student_result_wrong_state_returns_503() {
        // Loop is in Starting state, not WaitingForStudent
        let state = test_state();
        let router = create_router(state);

        let request_body = serde_json::json!({
            "studentOutput": {
                "status": "completed",
                "current_step": "Step 1",
                "attempted_actions": [],
                "summary": "Done"
            },
            "timestamp": "2026-02-03T10:00:00Z"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/student/result")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("not waiting for student"));
    }

    #[tokio::test]
    async fn test_student_result_invalid_json_returns_400() {
        let state = state_waiting_for_student().await;
        let router = create_router(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/student/result")
                    .header("content-type", "application/json")
                    .body(Body::from("{ invalid json }"))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Axum returns 400 for JSON parsing errors
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ------------------------------------------------------------------------
    // Mentor result endpoint tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_mentor_result_success() {
        let state = state_waiting_for_mentor().await;
        let router = create_router(state);

        let request_body = serde_json::json!({
            "mentorOutput": "Try running npm install first",
            "timestamp": "2026-02-03T10:00:00Z"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/mentor/result")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: MentorResultResponse = serde_json::from_slice(&body).unwrap();

        assert!(response.acknowledged);
        assert_eq!(response.next_action, NextAction::Continue);
    }

    #[tokio::test]
    async fn test_mentor_result_wrong_state_returns_503() {
        // Loop is in WaitingForStudent, not WaitingForMentor
        let state = state_waiting_for_student().await;
        let router = create_router(state);

        let request_body = serde_json::json!({
            "mentorOutput": "Some advice",
            "timestamp": "2026-02-03T10:00:00Z"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/mentor/result")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("not waiting for mentor"));
    }

    // ------------------------------------------------------------------------
    // Stop endpoint tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_stop_success() {
        let state = state_waiting_for_student().await;
        let router = create_router(state);

        let request_body = serde_json::json!({
            "reason": "User cancelled"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/stop")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: StopResponse = serde_json::from_slice(&body).unwrap();

        assert!(response.stopped);
        assert_eq!(response.final_state.status, LoopStatus::Error);
        assert_eq!(
            response.final_state.error_message,
            Some("User cancelled".to_string())
        );
    }

    #[tokio::test]
    async fn test_stop_already_terminal_returns_503() {
        let state = test_state();
        // Put loop into terminal state
        {
            let mut loop_state = state.loop_state.lock().await;
            loop_state.start().unwrap();
            loop_state.start_waiting_for_student().unwrap();
            let output = StudentOutput {
                status: StudentStatus::Completed,
                current_step: "Done".to_string(),
                summary: "Finished".to_string(),
                ..Default::default()
            };
            loop_state.receive_student_result(output, 10).unwrap();
        }

        let router = create_router(state);

        let request_body = serde_json::json!({
            "reason": "Want to stop"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/stop")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("already stopped"));
    }

    // ------------------------------------------------------------------------
    // Router configuration tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_cors_headers_present() {
        let state = test_state();
        let router = create_router(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/status")
                    .header("origin", "http://localhost:5173")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // OPTIONS preflight should succeed
        assert!(response.status().is_success() || response.status() == StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        let state = test_state();
        let router = create_router(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // ------------------------------------------------------------------------
    // AppState tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_app_state_new() {
        let config = Config::default();
        let state = AppState::new(config);

        // Use tokio test runtime for async operations
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let status = state.loop_state.lock().await.status;
            assert_eq!(status, LoopStatus::Starting);
        });
    }

    #[test]
    fn test_app_state_with_state() {
        let config = Config::default();
        let mut existing_state = LoopState::new();
        existing_state.status = LoopStatus::RunningStudent;
        existing_state.iteration = 5;

        let state = AppState::with_state(config, existing_state);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (status, iteration) = {
                let loop_state = state.loop_state.lock().await;
                (loop_state.status, loop_state.iteration)
            };
            assert_eq!(status, LoopStatus::RunningStudent);
            assert_eq!(iteration, 5);
        });
    }

    // ------------------------------------------------------------------------
    // NextAction tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_next_action_serialization() {
        assert_eq!(
            serde_json::to_string(&NextAction::Continue).unwrap(),
            r#""continue""#
        );
        assert_eq!(
            serde_json::to_string(&NextAction::Stop).unwrap(),
            r#""stop""#
        );
    }

    #[test]
    fn test_next_action_deserialization() {
        let action: NextAction = serde_json::from_str(r#""continue""#).unwrap();
        assert_eq!(action, NextAction::Continue);

        let action: NextAction = serde_json::from_str(r#""stop""#).unwrap();
        assert_eq!(action, NextAction::Stop);
    }

    // ------------------------------------------------------------------------
    // Request/Response serialization tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_student_result_request_deserialization() {
        let json = r#"{
            "studentOutput": {
                "status": "completed",
                "current_step": "Step 1",
                "attempted_actions": ["action1"],
                "summary": "Done"
            },
            "timestamp": "2026-02-03T10:00:00Z"
        }"#;

        let request: StudentResultRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.student_output.status, StudentStatus::Completed);
        assert_eq!(request.student_output.current_step, "Step 1");
    }

    #[test]
    fn test_mentor_result_request_deserialization() {
        let json = r#"{
            "mentorOutput": "Try this approach",
            "timestamp": "2026-02-03T10:00:00Z"
        }"#;

        let request: MentorResultRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.mentor_output, "Try this approach");
    }

    #[test]
    fn test_stop_request_deserialization() {
        let json = r#"{"reason": "Timeout exceeded"}"#;

        let request: StopRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.reason, "Timeout exceeded");
    }

    #[test]
    fn test_student_result_response_serialization() {
        let response = StudentResultResponse {
            acknowledged: true,
            next_action: NextAction::Continue,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""acknowledged":true"#));
        assert!(json.contains(r#""nextAction":"continue""#));
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse {
            error: "Something went wrong".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""error":"Something went wrong""#));
    }
}
