//! WebSocket event types and broadcasting for real-time loop observation.
//!
//! This module provides WebSocket-based event streaming for observing SMILE loop
//! execution in real-time. Events are broadcast to all connected clients as the
//! loop progresses through its states.
//!
//! # Event Types
//!
//! - `connected` - Sent when a client connects, includes current state
//! - `iteration_start` - New iteration begins
//! - `student_output` - Student agent completes (summarized)
//! - `mentor_output` - Mentor agent provides notes
//! - `loop_complete` - Loop terminates (success or failure)
//! - `error` - Error occurs during execution
//!
//! # Example
//!
//! ```no_run
//! use smile_orchestrator::websocket::{EventBroadcaster, LoopEvent};
//! use smile_orchestrator::LoopState;
//!
//! # async fn example() {
//! // Create a broadcaster
//! let broadcaster = EventBroadcaster::new(100);
//!
//! // Subscribe to events
//! let mut receiver = broadcaster.subscribe();
//!
//! // Broadcast an event
//! let state = LoopState::new();
//! broadcaster.send(LoopEvent::connected(state));
//!
//! // Receive the event
//! if let Ok(event) = receiver.recv().await {
//!     println!("Received: {:?}", event);
//! }
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{LoopState, LoopStatus, StudentStatus};

// ============================================================================
// Event Payloads
// ============================================================================

/// Payload for the `connected` event.
///
/// Sent immediately when a WebSocket client connects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedPayload {
    /// The current loop state.
    pub state: LoopState,
}

/// Payload for the `iteration_start` event.
///
/// Sent when a new iteration begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStartPayload {
    /// The iteration number (1-indexed).
    pub iteration: u32,
    /// When this iteration started.
    pub timestamp: DateTime<Utc>,
}

/// Payload for the `student_output` event.
///
/// Sent when the student agent completes. Contains a summary rather than
/// full output to keep event size manageable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentOutputPayload {
    /// The student's completion status.
    pub status: StudentStatus,
    /// Brief summary of what the student accomplished.
    pub summary: String,
    /// The step the student was working on.
    pub current_step: String,
}

/// Payload for the `mentor_output` event.
///
/// Sent when the mentor agent provides notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentorOutputPayload {
    /// The mentor's guidance notes.
    pub notes: String,
}

/// Payload for the `loop_complete` event.
///
/// Sent when the loop terminates (success or failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopCompletePayload {
    /// The terminal status.
    pub status: LoopStatus,
    /// Summary of the loop outcome.
    pub summary: String,
    /// Total iterations executed.
    pub iterations: u32,
}

/// Payload for the `error` event.
///
/// Sent when an error occurs during loop execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    /// Human-readable error message.
    pub message: String,
}

// ============================================================================
// Event Enum
// ============================================================================

/// WebSocket event types for loop observation.
///
/// All events are serialized as JSON objects with "event" and "payload" fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
pub enum LoopEvent {
    /// Sent when a client connects.
    Connected(ConnectedPayload),
    /// Sent when a new iteration begins.
    IterationStart(IterationStartPayload),
    /// Sent when the student agent completes.
    StudentOutput(StudentOutputPayload),
    /// Sent when the mentor agent provides notes.
    MentorOutput(MentorOutputPayload),
    /// Sent when the loop terminates.
    LoopComplete(LoopCompletePayload),
    /// Sent when an error occurs.
    Error(ErrorPayload),
}

impl LoopEvent {
    /// Creates a `Connected` event with the current loop state.
    #[must_use]
    pub const fn connected(state: LoopState) -> Self {
        Self::Connected(ConnectedPayload { state })
    }

    /// Creates an `IterationStart` event.
    #[must_use]
    pub fn iteration_start(iteration: u32) -> Self {
        Self::IterationStart(IterationStartPayload {
            iteration,
            timestamp: Utc::now(),
        })
    }

    /// Creates a `StudentOutput` event from student data.
    #[must_use]
    pub const fn student_output(
        status: StudentStatus,
        summary: String,
        current_step: String,
    ) -> Self {
        Self::StudentOutput(StudentOutputPayload {
            status,
            summary,
            current_step,
        })
    }

    /// Creates a `MentorOutput` event.
    #[must_use]
    pub const fn mentor_output(notes: String) -> Self {
        Self::MentorOutput(MentorOutputPayload { notes })
    }

    /// Creates a `LoopComplete` event.
    #[must_use]
    pub const fn loop_complete(status: LoopStatus, summary: String, iterations: u32) -> Self {
        Self::LoopComplete(LoopCompletePayload {
            status,
            summary,
            iterations,
        })
    }

    /// Creates an `Error` event.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error(ErrorPayload {
            message: message.into(),
        })
    }

    /// Returns the event name as a string.
    #[must_use]
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::Connected(_) => "connected",
            Self::IterationStart(_) => "iteration_start",
            Self::StudentOutput(_) => "student_output",
            Self::MentorOutput(_) => "mentor_output",
            Self::LoopComplete(_) => "loop_complete",
            Self::Error(_) => "error",
        }
    }
}

// ============================================================================
// Event Broadcaster
// ============================================================================

/// Broadcasts loop events to all connected WebSocket clients.
///
/// Uses a tokio broadcast channel for pub-sub event distribution.
/// Events are not persisted for disconnected clients.
#[derive(Debug, Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<LoopEvent>,
}

impl EventBroadcaster {
    /// Creates a new `EventBroadcaster` with the specified buffer capacity.
    ///
    /// The buffer determines how many events can be queued per subscriber
    /// before old events are dropped.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The buffer size for each subscriber (typically 100)
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Creates a new subscriber for receiving events.
    ///
    /// Each subscriber maintains its own buffer. If a subscriber falls behind,
    /// it will receive a `Lagged` error and miss some events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LoopEvent> {
        self.sender.subscribe()
    }

    /// Broadcasts an event to all connected subscribers.
    ///
    /// Returns the number of active receivers that will receive the event.
    /// A return value of 0 means no clients are currently connected.
    pub fn send(&self, event: LoopEvent) -> usize {
        // send() returns Err only if there are no receivers, which is fine
        self.sender.send(event).unwrap_or(0)
    }

    /// Returns the number of active subscribers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new(100)
    }
}

// ============================================================================
// WebSocket Handler
// ============================================================================

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, info, warn};

/// Shared state for WebSocket handlers.
#[derive(Debug, Clone)]
pub struct WsState {
    /// The event broadcaster for sending events to clients.
    pub broadcaster: EventBroadcaster,
    /// The current loop state (shared with HTTP handlers).
    pub loop_state: Arc<Mutex<LoopState>>,
}

impl WsState {
    /// Creates a new `WsState` with the given loop state.
    #[must_use]
    pub fn new(loop_state: Arc<Mutex<LoopState>>) -> Self {
        Self {
            broadcaster: EventBroadcaster::default(),
            loop_state,
        }
    }

    /// Creates a new `WsState` with custom broadcaster capacity.
    #[must_use]
    pub fn with_capacity(loop_state: Arc<Mutex<LoopState>>, capacity: usize) -> Self {
        Self {
            broadcaster: EventBroadcaster::new(capacity),
            loop_state,
        }
    }
}

/// WebSocket upgrade handler.
///
/// Called when a client connects to `/ws`. Upgrades the HTTP connection
/// to a WebSocket and spawns a handler task.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<WsState>>) -> Response {
    info!("New WebSocket connection request");
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Maximum number of missed pong responses before disconnecting.
const MAX_MISSED_PONGS: u8 = 3;

/// Handles a single WebSocket connection.
///
/// - Sends `connected` event with current state immediately
/// - Subscribes to the event broadcaster
/// - Forwards all events to the client
/// - Sends heartbeat pings every 30 seconds
/// - Closes connection after 3 missed pongs
async fn handle_socket(socket: WebSocket, state: Arc<WsState>) {
    let (mut sender, mut receiver) = socket.split();

    // Get current state and send connected event
    let current_state = {
        let loop_state = state.loop_state.lock().await;
        loop_state.clone()
    };

    let connected_event = LoopEvent::connected(current_state);
    let connected_json = match serde_json::to_string(&connected_event) {
        Ok(json) => json,
        Err(e) => {
            warn!("Failed to serialize connected event: {}", e);
            return;
        }
    };

    if sender.send(Message::Text(connected_json)).await.is_err() {
        debug!("Client disconnected before receiving connected event");
        return;
    }

    info!("WebSocket client connected, sent initial state");

    // Subscribe to broadcast events
    let mut event_receiver = state.broadcaster.subscribe();

    // Heartbeat interval (30 seconds as per spec)
    let mut heartbeat_interval = interval(Duration::from_secs(30));
    let mut missed_pongs = 0u8;

    loop {
        tokio::select! {
            // Handle incoming messages (primarily pong responses)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Pong(_))) => {
                        missed_pongs = 0;
                        debug!("Received pong from client");
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("Client requested close");
                        break;
                    }
                    Some(Ok(Message::Text(_))) => {
                        // Clients don't send text messages; ignore
                        debug!("Ignoring text message from client");
                    }
                    Some(Ok(Message::Binary(_))) => {
                        // Clients don't send binary messages; ignore
                        debug!("Ignoring binary message from client");
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // Respond to ping with pong
                        if sender.send(Message::Pong(data)).await.is_err() {
                            debug!("Failed to send pong, client disconnected");
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        debug!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        break;
                    }
                }
            }

            // Forward broadcast events to client
            event = event_receiver.recv() => {
                match event {
                    Ok(loop_event) => {
                        let json = match serde_json::to_string(&loop_event) {
                            Ok(j) => j,
                            Err(e) => {
                                warn!("Failed to serialize event: {}", e);
                                continue;
                            }
                        };

                        if sender.send(Message::Text(json)).await.is_err() {
                            debug!("Failed to send event, client disconnected");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Client fell behind; warn but continue
                        warn!("Client lagged, missed {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Broadcaster was dropped; should not happen in normal operation
                        info!("Broadcaster closed");
                        break;
                    }
                }
            }

            // Send heartbeat ping
            _ = heartbeat_interval.tick() => {
                if sender.send(Message::Ping(vec![])).await.is_err() {
                    debug!("Failed to send ping, client disconnected");
                    break;
                }
                missed_pongs += 1;
                if missed_pongs >= MAX_MISSED_PONGS {
                    info!("Client missed {} pongs, closing connection", MAX_MISSED_PONGS);
                    break;
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Event Serialization Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_connected_event_serialization() {
        let state = LoopState::new();
        let event = LoopEvent::connected(state);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"connected""#));
        assert!(json.contains(r#""payload""#));
        assert!(json.contains(r#""state""#));
    }

    #[test]
    fn test_iteration_start_event_serialization() {
        let event = LoopEvent::iteration_start(3);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"iteration_start""#));
        assert!(json.contains(r#""iteration":3"#));
        assert!(json.contains(r#""timestamp""#));
    }

    #[test]
    fn test_student_output_event_serialization() {
        let event = LoopEvent::student_output(
            StudentStatus::AskMentor,
            "Stuck on step 3".to_string(),
            "Step 3: Install deps".to_string(),
        );

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"student_output""#));
        assert!(json.contains(r#""status":"ask_mentor""#));
        assert!(json.contains(r#""summary":"Stuck on step 3""#));
        assert!(json.contains(r#""currentStep":"Step 3: Install deps""#));
    }

    #[test]
    fn test_mentor_output_event_serialization() {
        let event = LoopEvent::mentor_output("Try pip3 instead of pip".to_string());

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"mentor_output""#));
        assert!(json.contains(r#""notes":"Try pip3 instead of pip""#));
    }

    #[test]
    fn test_loop_complete_event_serialization() {
        let event =
            LoopEvent::loop_complete(LoopStatus::Completed, "Tutorial finished".to_string(), 5);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"loop_complete""#));
        assert!(json.contains(r#""status":"completed""#));
        assert!(json.contains(r#""summary":"Tutorial finished""#));
        assert!(json.contains(r#""iterations":5"#));
    }

    #[test]
    fn test_error_event_serialization() {
        let event = LoopEvent::error("Docker connection lost");

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"error""#));
        assert!(json.contains(r#""message":"Docker connection lost""#));
    }

    // ------------------------------------------------------------------------
    // Event Deserialization Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_connected_event_deserialization() {
        let json = r#"{"event":"connected","payload":{"state":{"version":1,"status":"starting","iteration":0,"mentor_notes":[],"history":[],"started_at":"2026-02-03T10:00:00Z","updated_at":"2026-02-03T10:00:00Z"}}}"#;

        let event: LoopEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, LoopEvent::Connected(_)));

        if let LoopEvent::Connected(payload) = event {
            assert_eq!(payload.state.status, LoopStatus::Starting);
            assert_eq!(payload.state.iteration, 0);
        }
    }

    #[test]
    fn test_iteration_start_event_deserialization() {
        let json = r#"{"event":"iteration_start","payload":{"iteration":2,"timestamp":"2026-02-03T10:00:00Z"}}"#;

        let event: LoopEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, LoopEvent::IterationStart(_)));

        if let LoopEvent::IterationStart(payload) = event {
            assert_eq!(payload.iteration, 2);
        }
    }

    #[test]
    fn test_student_output_event_deserialization() {
        let json = r#"{"event":"student_output","payload":{"status":"completed","summary":"All done","currentStep":"Final step"}}"#;

        let event: LoopEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, LoopEvent::StudentOutput(_)));

        if let LoopEvent::StudentOutput(payload) = event {
            assert_eq!(payload.status, StudentStatus::Completed);
            assert_eq!(payload.summary, "All done");
            assert_eq!(payload.current_step, "Final step");
        }
    }

    #[test]
    fn test_error_event_deserialization() {
        let json = r#"{"event":"error","payload":{"message":"Something went wrong"}}"#;

        let event: LoopEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, LoopEvent::Error(_)));

        if let LoopEvent::Error(payload) = event {
            assert_eq!(payload.message, "Something went wrong");
        }
    }

    // ------------------------------------------------------------------------
    // Event Name Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_event_names() {
        let state = LoopState::new();
        assert_eq!(LoopEvent::connected(state).event_name(), "connected");
        assert_eq!(
            LoopEvent::iteration_start(1).event_name(),
            "iteration_start"
        );
        assert_eq!(
            LoopEvent::student_output(StudentStatus::Completed, String::new(), String::new())
                .event_name(),
            "student_output"
        );
        assert_eq!(
            LoopEvent::mentor_output(String::new()).event_name(),
            "mentor_output"
        );
        assert_eq!(
            LoopEvent::loop_complete(LoopStatus::Completed, String::new(), 0).event_name(),
            "loop_complete"
        );
        assert_eq!(LoopEvent::error("").event_name(), "error");
    }

    // ------------------------------------------------------------------------
    // Broadcaster Tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_broadcaster_send_receive() {
        let broadcaster = EventBroadcaster::new(10);
        let mut receiver = broadcaster.subscribe();

        let event = LoopEvent::iteration_start(1);
        let count = broadcaster.send(event);

        assert_eq!(count, 1);

        let event_recv = receiver.recv().await.unwrap();
        assert!(matches!(event_recv, LoopEvent::IterationStart(_)));
    }

    #[tokio::test]
    async fn test_broadcaster_multiple_subscribers() {
        let broadcaster = EventBroadcaster::new(10);
        let mut receiver1 = broadcaster.subscribe();
        let mut receiver2 = broadcaster.subscribe();

        let event = LoopEvent::error("test");
        let count = broadcaster.send(event);

        assert_eq!(count, 2);

        let event_one = receiver1.recv().await.unwrap();
        let event_two = receiver2.recv().await.unwrap();

        assert!(matches!(event_one, LoopEvent::Error(_)));
        assert!(matches!(event_two, LoopEvent::Error(_)));
    }

    #[test]
    fn test_broadcaster_no_subscribers() {
        let broadcaster = EventBroadcaster::new(10);
        let event = LoopEvent::iteration_start(1);

        // Should not panic with no subscribers
        let count = broadcaster.send(event);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_broadcaster_receiver_count() {
        let broadcaster = EventBroadcaster::new(10);
        assert_eq!(broadcaster.receiver_count(), 0);

        let _receiver1 = broadcaster.subscribe();
        assert_eq!(broadcaster.receiver_count(), 1);

        let _receiver2 = broadcaster.subscribe();
        assert_eq!(broadcaster.receiver_count(), 2);
    }

    #[test]
    fn test_broadcaster_default() {
        let broadcaster = EventBroadcaster::default();
        assert_eq!(broadcaster.receiver_count(), 0);
    }

    // ------------------------------------------------------------------------
    // Payload Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_connected_payload() {
        let state = LoopState::new();
        let payload = ConnectedPayload { state };

        assert_eq!(payload.state.status, LoopStatus::Starting);
    }

    #[test]
    fn test_iteration_start_payload() {
        let payload = IterationStartPayload {
            iteration: 5,
            timestamp: Utc::now(),
        };

        assert_eq!(payload.iteration, 5);
    }

    #[test]
    fn test_student_output_payload() {
        let payload = StudentOutputPayload {
            status: StudentStatus::AskMentor,
            summary: "Need help".to_string(),
            current_step: "Step 2".to_string(),
        };

        assert_eq!(payload.status, StudentStatus::AskMentor);
        assert_eq!(payload.summary, "Need help");
        assert_eq!(payload.current_step, "Step 2");
    }

    #[test]
    fn test_mentor_output_payload() {
        let payload = MentorOutputPayload {
            notes: "Try this approach".to_string(),
        };

        assert_eq!(payload.notes, "Try this approach");
    }

    #[test]
    fn test_loop_complete_payload() {
        let payload = LoopCompletePayload {
            status: LoopStatus::MaxIterations,
            summary: "Reached limit".to_string(),
            iterations: 10,
        };

        assert_eq!(payload.status, LoopStatus::MaxIterations);
        assert_eq!(payload.summary, "Reached limit");
        assert_eq!(payload.iterations, 10);
    }

    #[test]
    fn test_error_payload() {
        let payload = ErrorPayload {
            message: "Connection lost".to_string(),
        };

        assert_eq!(payload.message, "Connection lost");
    }
}
