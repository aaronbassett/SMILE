//! Integration tests for WebSocket real-time event streaming.
//!
//! These tests validate the WebSocket server functionality including
//! connection handling, event broadcasting, and concurrent client support.

use std::net::TcpListener;
use std::time::Duration;

use futures::SinkExt;
use futures::StreamExt;
use smile_orchestrator::{
    create_router, AppState, Config, LoopEvent, LoopState, LoopStatus, StudentStatus,
};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tungstenite::Message;

/// Helper to find an available port for testing.
fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to port")
        .local_addr()
        .expect("Failed to get local addr")
        .port()
}

/// Helper type for WebSocket client
type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Spawns the test server and returns the WebSocket URL.
async fn spawn_test_server(state: AppState) -> (String, tokio::task::JoinHandle<()>) {
    let port = find_available_port();
    let addr = format!("127.0.0.1:{port}");
    let ws_url = format!("ws://{addr}/ws");

    let router = create_router(state);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("Server failed");
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    (ws_url, handle)
}

/// Connects a WebSocket client to the given URL.
async fn connect_client(url: &str) -> WsClient {
    let (ws_stream, _) = connect_async(url)
        .await
        .expect("Failed to connect to WebSocket");
    ws_stream
}

/// Receives the next text message from the WebSocket and parses it as LoopEvent.
/// Automatically handles ping frames by responding with pong.
async fn receive_event(client: &mut WsClient) -> LoopEvent {
    loop {
        let msg = timeout(Duration::from_secs(5), client.next())
            .await
            .expect("Timeout waiting for message")
            .expect("Stream ended")
            .expect("WebSocket error");

        match msg {
            Message::Text(text) => {
                return serde_json::from_str(&text).expect("Failed to parse event");
            }
            Message::Ping(data) => {
                // Respond to ping and continue waiting for text message
                client
                    .send(Message::Pong(data))
                    .await
                    .expect("Failed to send pong");
            }
            Message::Pong(_) => {
                // Ignore pong messages, continue waiting
            }
            other => panic!("Expected text message, got: {other:?}"),
        }
    }
}

// ============================================================================
// Connection Tests
// ============================================================================

/// Tests that a WebSocket client receives a connected event on connection.
#[tokio::test]
async fn test_client_receives_connected_event_on_connect() {
    let state = AppState::new(Config::default());
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    let event = receive_event(&mut client).await;

    assert!(
        matches!(event, LoopEvent::Connected(_)),
        "Expected Connected event, got: {event:?}"
    );

    if let LoopEvent::Connected(payload) = event {
        assert_eq!(payload.state.status, LoopStatus::Starting);
        assert_eq!(payload.state.iteration, 0);
    }
}

/// Tests that the connected event contains the current state.
#[tokio::test]
async fn test_connected_event_contains_current_state() {
    let config = Config::default();
    let mut loop_state = LoopState::new();

    // Simulate some state
    loop_state.status = LoopStatus::RunningStudent;
    loop_state.iteration = 3;

    let state = AppState::with_state(config, loop_state);
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    let event = receive_event(&mut client).await;

    if let LoopEvent::Connected(payload) = event {
        assert_eq!(payload.state.status, LoopStatus::RunningStudent);
        assert_eq!(payload.state.iteration, 3);
    } else {
        panic!("Expected Connected event");
    }
}

// ============================================================================
// Multiple Client Tests
// ============================================================================

/// Tests that multiple clients can connect concurrently.
#[tokio::test]
async fn test_multiple_clients_can_connect() {
    let state = AppState::new(Config::default());
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client1 = connect_client(&ws_url).await;
    let mut client2 = connect_client(&ws_url).await;
    let mut client3 = connect_client(&ws_url).await;

    // All clients should receive connected events
    let event1 = receive_event(&mut client1).await;
    let event2 = receive_event(&mut client2).await;
    let event3 = receive_event(&mut client3).await;

    assert!(matches!(event1, LoopEvent::Connected(_)));
    assert!(matches!(event2, LoopEvent::Connected(_)));
    assert!(matches!(event3, LoopEvent::Connected(_)));
}

// ============================================================================
// Event Broadcast Tests
// ============================================================================

/// Tests that events are broadcast to all connected clients.
#[tokio::test]
async fn test_events_broadcast_to_all_clients() {
    let state = AppState::new(Config::default());
    let broadcaster = state.broadcaster.clone();
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client1 = connect_client(&ws_url).await;
    let mut client2 = connect_client(&ws_url).await;

    // Consume connected events
    receive_event(&mut client1).await;
    receive_event(&mut client2).await;

    // Broadcast an event
    broadcaster.send(LoopEvent::iteration_start(1));

    // Both clients should receive it
    let event1 = receive_event(&mut client1).await;
    let event2 = receive_event(&mut client2).await;

    assert!(matches!(event1, LoopEvent::IterationStart(_)));
    assert!(matches!(event2, LoopEvent::IterationStart(_)));

    if let LoopEvent::IterationStart(payload) = event1 {
        assert_eq!(payload.iteration, 1);
    }
}

/// Tests that student output events are broadcast.
#[tokio::test]
async fn test_student_output_event_broadcast() {
    let state = AppState::new(Config::default());
    let broadcaster = state.broadcaster.clone();
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    receive_event(&mut client).await; // Consume connected event

    // Broadcast student output
    broadcaster.send(LoopEvent::student_output(
        StudentStatus::AskMentor,
        "Stuck on step 3".to_string(),
        "Step 3: Install deps".to_string(),
    ));

    let event = receive_event(&mut client).await;

    if let LoopEvent::StudentOutput(payload) = event {
        assert_eq!(payload.status, StudentStatus::AskMentor);
        assert_eq!(payload.summary, "Stuck on step 3");
        assert_eq!(payload.current_step, "Step 3: Install deps");
    } else {
        panic!("Expected StudentOutput event, got: {event:?}");
    }
}

/// Tests that mentor output events are broadcast.
#[tokio::test]
async fn test_mentor_output_event_broadcast() {
    let state = AppState::new(Config::default());
    let broadcaster = state.broadcaster.clone();
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    receive_event(&mut client).await; // Consume connected event

    // Broadcast mentor output
    broadcaster.send(LoopEvent::mentor_output("Try pip3 instead".to_string()));

    let event = receive_event(&mut client).await;

    if let LoopEvent::MentorOutput(payload) = event {
        assert_eq!(payload.notes, "Try pip3 instead");
    } else {
        panic!("Expected MentorOutput event, got: {event:?}");
    }
}

/// Tests that loop complete events are broadcast.
#[tokio::test]
async fn test_loop_complete_event_broadcast() {
    let state = AppState::new(Config::default());
    let broadcaster = state.broadcaster.clone();
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    receive_event(&mut client).await; // Consume connected event

    // Broadcast loop complete
    broadcaster.send(LoopEvent::loop_complete(
        LoopStatus::Completed,
        "Tutorial finished successfully".to_string(),
        5,
    ));

    let event = receive_event(&mut client).await;

    if let LoopEvent::LoopComplete(payload) = event {
        assert_eq!(payload.status, LoopStatus::Completed);
        assert_eq!(payload.summary, "Tutorial finished successfully");
        assert_eq!(payload.iterations, 5);
    } else {
        panic!("Expected LoopComplete event, got: {event:?}");
    }
}

/// Tests that error events are broadcast.
#[tokio::test]
async fn test_error_event_broadcast() {
    let state = AppState::new(Config::default());
    let broadcaster = state.broadcaster.clone();
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    receive_event(&mut client).await; // Consume connected event

    // Broadcast error
    broadcaster.send(LoopEvent::error("Docker connection lost"));

    let event = receive_event(&mut client).await;

    if let LoopEvent::Error(payload) = event {
        assert_eq!(payload.message, "Docker connection lost");
    } else {
        panic!("Expected Error event, got: {event:?}");
    }
}

// ============================================================================
// API Integration Tests
// ============================================================================

/// Tests that API endpoints trigger WebSocket events.
#[tokio::test]
async fn test_api_triggers_websocket_events() {
    let config = Config {
        max_iterations: 10,
        timeout: 3600,
        ..Config::default()
    };
    let state = AppState::new(config);

    // Setup state for receiving student result
    {
        let mut loop_state = state.loop_state.lock().await;
        loop_state.start().expect("Failed to start");
        loop_state
            .start_waiting_for_student()
            .expect("Failed to wait for student");
    }

    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    receive_event(&mut client).await; // Consume connected event

    // Make HTTP request to trigger student result
    let http_url = ws_url.replace("ws://", "http://").replace("/ws", "");
    let client_http = reqwest::Client::new();

    let response = client_http
        .post(format!("{http_url}/api/student/result"))
        .json(&serde_json::json!({
            "studentOutput": {
                "status": "completed",
                "current_step": "Final step",
                "attempted_actions": ["finished"],
                "summary": "All done!"
            },
            "timestamp": "2026-02-03T10:00:00Z"
        }))
        .send()
        .await
        .expect("Failed to send HTTP request");

    assert!(response.status().is_success());

    // Should receive student_output event
    let event = receive_event(&mut client).await;
    assert!(
        matches!(event, LoopEvent::StudentOutput(_)),
        "Expected StudentOutput event, got: {event:?}"
    );

    // Should also receive loop_complete event (since status was Completed)
    let event = receive_event(&mut client).await;
    assert!(
        matches!(event, LoopEvent::LoopComplete(_)),
        "Expected LoopComplete event, got: {event:?}"
    );
}

// ============================================================================
// Disconnection Tests
// ============================================================================

/// Tests that client can cleanly disconnect.
#[tokio::test]
async fn test_client_can_disconnect() {
    let state = AppState::new(Config::default());
    let (ws_url, _handle) = spawn_test_server(state).await;

    let mut client = connect_client(&ws_url).await;
    receive_event(&mut client).await; // Consume connected event

    // Send close frame
    client
        .close(None)
        .await
        .expect("Failed to close connection");
}

/// Tests that server continues after client disconnects.
#[tokio::test]
async fn test_server_continues_after_client_disconnect() {
    let state = AppState::new(Config::default());
    let broadcaster = state.broadcaster.clone();
    let (ws_url, _handle) = spawn_test_server(state).await;

    // Connect and disconnect first client
    let mut client1 = connect_client(&ws_url).await;
    receive_event(&mut client1).await;
    client1.close(None).await.ok();
    drop(client1);

    // Give server time to process disconnect
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect second client - should work fine
    let mut client2 = connect_client(&ws_url).await;
    let event = receive_event(&mut client2).await;
    assert!(matches!(event, LoopEvent::Connected(_)));

    // Broadcasting should still work
    broadcaster.send(LoopEvent::iteration_start(1));
    let event = receive_event(&mut client2).await;
    assert!(matches!(event, LoopEvent::IterationStart(_)));
}
