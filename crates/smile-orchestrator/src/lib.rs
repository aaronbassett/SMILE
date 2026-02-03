//! SMILE Loop Orchestrator
//!
//! Manages the Student-Mentor loop, HTTP API, and WebSocket events.

pub mod config;
pub mod error;

pub use config::{Config, ContainerConfig, LlmProvider, PatienceLevel, StudentBehavior};
pub use error::{LlmErrorKind, Result, SmileError};
