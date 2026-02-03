//! SMILE Loop Orchestrator
//!
//! Manages the Student-Mentor loop, HTTP API, and WebSocket events.

pub mod api;
pub mod config;
pub mod error;
pub mod loop_state;
pub mod tutorial;

pub use api::{
    create_router, AppState, ErrorResponse, MentorResultRequest, MentorResultResponse, NextAction,
    StopRequest, StopResponse, StudentResultRequest, StudentResultResponse,
};
pub use config::{Config, ContainerConfig, LlmProvider, PatienceLevel, StudentBehavior};
pub use error::{LlmErrorKind, Result, SmileError};
pub use loop_state::{
    IterationRecord, LoopState, LoopStatus, MentorNote, StateLock, StudentOutput, StudentStatus,
    STATE_VERSION,
};
pub use tutorial::{ImageFormat, Tutorial, TutorialImage, MAX_TUTORIAL_SIZE};
