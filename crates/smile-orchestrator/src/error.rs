//! Error types for the SMILE Loop orchestrator.
//!
//! This module defines the error hierarchy for all orchestrator operations,
//! including configuration loading, tutorial parsing, container management,
//! LLM interactions, and report generation.

use std::path::PathBuf;

/// A specialized `Result` type for SMILE orchestrator operations.
pub type Result<T> = std::result::Result<T, SmileError>;

/// Errors that can occur during SMILE Loop execution.
///
/// Error variants are organized by subsystem and include actionable suggestions
/// where possible to help users resolve issues.
#[derive(Debug, thiserror::Error)]
pub enum SmileError {
    // ========================================================================
    // Configuration Errors (E02)
    // ========================================================================
    /// Invalid JSON syntax in configuration file.
    ///
    /// Corresponds to edge case E02: Invalid JSON syntax.
    #[error("Invalid JSON in config file '{path}': {message}\n\nSuggestion: Validate your smile.json with a JSON linter")]
    ConfigParseError {
        /// Path to the configuration file.
        path: PathBuf,
        /// Description of the parse error.
        message: String,
    },

    /// Configuration validation failed.
    #[error("Invalid configuration: {message}\n\nSuggestion: {suggestion}")]
    ConfigValidationError {
        /// Description of the validation failure.
        message: String,
        /// Actionable suggestion for the user.
        suggestion: String,
    },

    // ========================================================================
    // Tutorial Loading Errors (E04, E05, E06)
    // ========================================================================
    /// Tutorial file was not found at the specified path.
    ///
    /// Corresponds to edge case E04: Tutorial file missing.
    #[error("Tutorial not found: '{path}'\n\nSuggestion: Check the 'tutorial' field in smile.json or create the file")]
    TutorialNotFound {
        /// Path where the tutorial was expected.
        path: PathBuf,
    },

    /// Tutorial file exceeds the 100KB size limit.
    ///
    /// Corresponds to edge case E05: Tutorial > 100KB.
    #[error("Tutorial exceeds size limit (100KB): '{path}' is {size_kb}KB\n\nSuggestion: Split into smaller tutorials or remove embedded content")]
    TutorialTooLarge {
        /// Path to the oversized tutorial.
        path: PathBuf,
        /// Actual size in kilobytes.
        size_kb: u64,
    },

    /// Tutorial file contains non-UTF-8 content.
    ///
    /// Corresponds to edge case E06: Non-UTF-8 encoding.
    #[error(
        "Tutorial has invalid encoding: '{path}'\n\nSuggestion: Convert the file to UTF-8 encoding"
    )]
    TutorialEncodingError {
        /// Path to the tutorial with encoding issues.
        path: PathBuf,
    },

    // ========================================================================
    // Docker/Container Errors (E08, E09)
    // ========================================================================
    /// Docker daemon is not available or not running.
    ///
    /// Corresponds to edge case E08: Docker not available.
    #[error("Docker is required but not available\n\nSuggestion: Ensure Docker is installed and the daemon is running (try 'docker info')")]
    DockerNotAvailable,

    /// The specified container image was not found.
    ///
    /// Corresponds to edge case E09: Image not found.
    #[error("Container image not found: '{image}'\n\nSuggestion: Pull the image first with 'docker pull {image}' or build it with 'just docker-build'")]
    ImageNotFound {
        /// Name of the missing image.
        image: String,
    },

    // ========================================================================
    // LLM Errors (E10, E11)
    // ========================================================================
    /// The required LLM CLI tool is not available in the container.
    ///
    /// Corresponds to edge case E10: LLM CLI not available.
    #[error("LLM CLI not available: '{cli}'\n\nSuggestion: Ensure the container image includes the {cli} CLI tool")]
    LlmCliNotAvailable {
        /// Name of the missing CLI tool (e.g., "claude", "codex", "gemini").
        cli: String,
    },

    /// LLM API returned an error (authentication, rate limiting, etc.).
    ///
    /// Corresponds to edge case E11: LLM API error.
    #[error("LLM API error ({kind}): {message}\n\nSuggestion: {suggestion}")]
    LlmApiError {
        /// The kind of API error (e.g., rate limit, authentication, server).
        kind: LlmErrorKind,
        /// Detailed error message from the API.
        message: String,
        /// Actionable suggestion for the user.
        suggestion: String,
    },

    // ========================================================================
    // Wrapper Communication Errors (E13)
    // ========================================================================
    /// Wrapper did not call back within the expected timeout.
    ///
    /// Corresponds to edge case E13: Wrapper never calls back.
    #[error("Wrapper timeout after {timeout_secs}s: no callback received from {agent} agent\n\nSuggestion: Check container logs for errors; the agent may have crashed")]
    WrapperTimeout {
        /// Which agent timed out ("Student" or "Mentor").
        agent: String,
        /// The timeout duration in seconds.
        timeout_secs: u64,
    },

    // ========================================================================
    // Concurrency Errors (E14, E18)
    // ========================================================================
    /// Another SMILE loop is already running.
    ///
    /// Corresponds to edge cases E14 and E18: Multiple concurrent loops / State file locked.
    #[error("A SMILE loop is already running (state file locked: '{state_file}')\n\nSuggestion: Wait for the other loop to complete or remove the state file if it's stale")]
    LoopAlreadyRunning {
        /// Path to the locked state file.
        state_file: PathBuf,
    },

    // ========================================================================
    // Report Errors (E16)
    // ========================================================================
    /// Failed to write the report to disk.
    ///
    /// Corresponds to edge case E16: Report write fails.
    #[error("Failed to write report to '{path}': {message}\n\nSuggestion: Check write permissions and available disk space")]
    ReportWriteError {
        /// Path where the report was to be written.
        path: PathBuf,
        /// Description of the write failure.
        message: String,
    },

    // ========================================================================
    // State Persistence Errors (E20)
    // ========================================================================
    /// State file contains malformed JSON that cannot be recovered.
    ///
    /// Corresponds to edge case E20: Malformed JSON recovery.
    #[error("Corrupted state file '{path}': {message}\n\nSuggestion: Remove the state file to start fresh, or restore from backup")]
    StateFileCorrupted {
        /// Path to the corrupted state file.
        path: PathBuf,
        /// Description of the corruption.
        message: String,
    },

    // ========================================================================
    // General I/O Errors
    // ========================================================================
    /// General I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ========================================================================
    // State Machine Errors
    // ========================================================================
    /// Invalid state transition attempted.
    #[error("Invalid state transition: cannot go from {from} to {to}")]
    InvalidStateTransition {
        /// The current state.
        from: String,
        /// The attempted target state.
        to: String,
    },
}

/// Categories of LLM API errors for structured error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmErrorKind {
    /// Authentication failure (invalid API key, expired credentials).
    Authentication,
    /// Rate limit exceeded.
    RateLimit,
    /// Server error (5xx responses).
    Server,
    /// Network connectivity issues.
    Network,
    /// Other unclassified errors.
    Other,
}

impl std::fmt::Display for LlmErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authentication => write!(f, "authentication"),
            Self::RateLimit => write!(f, "rate_limit"),
            Self::Server => write!(f, "server"),
            Self::Network => write!(f, "network"),
            Self::Other => write!(f, "other"),
        }
    }
}

impl LlmErrorKind {
    /// Returns a suggestion message for this error kind.
    #[must_use]
    pub const fn suggestion(&self) -> &'static str {
        match self {
            Self::Authentication => "Check your API key or credentials",
            Self::RateLimit => "Wait and retry, or reduce request frequency",
            Self::Server => "Retry later; the LLM service may be experiencing issues",
            Self::Network => "Check your network connection",
            Self::Other => "Check the LLM provider's status page",
        }
    }
}

impl SmileError {
    /// Creates a new `ConfigParseError` with the given path and message.
    #[must_use]
    pub fn config_parse(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::ConfigParseError {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Creates a new `ConfigValidationError` with the given message and suggestion.
    #[must_use]
    pub fn config_validation(message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self::ConfigValidationError {
            message: message.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Creates a new `TutorialNotFound` error.
    #[must_use]
    pub fn tutorial_not_found(path: impl Into<PathBuf>) -> Self {
        Self::TutorialNotFound { path: path.into() }
    }

    /// Creates a new `TutorialTooLarge` error.
    #[must_use]
    pub fn tutorial_too_large(path: impl Into<PathBuf>, size_kb: u64) -> Self {
        Self::TutorialTooLarge {
            path: path.into(),
            size_kb,
        }
    }

    /// Creates a new `TutorialEncodingError`.
    #[must_use]
    pub fn tutorial_encoding(path: impl Into<PathBuf>) -> Self {
        Self::TutorialEncodingError { path: path.into() }
    }

    /// Creates a new `ImageNotFound` error.
    #[must_use]
    pub fn image_not_found(image: impl Into<String>) -> Self {
        Self::ImageNotFound {
            image: image.into(),
        }
    }

    /// Creates a new `LlmCliNotAvailable` error.
    #[must_use]
    pub fn llm_cli_not_available(cli: impl Into<String>) -> Self {
        Self::LlmCliNotAvailable { cli: cli.into() }
    }

    /// Creates a new `LlmApiError` with automatic suggestion based on error kind.
    #[must_use]
    pub fn llm_api_error(kind: LlmErrorKind, message: impl Into<String>) -> Self {
        let suggestion = kind.suggestion().to_string();
        Self::LlmApiError {
            kind,
            message: message.into(),
            suggestion,
        }
    }

    /// Creates a new `WrapperTimeout` error.
    #[must_use]
    pub fn wrapper_timeout(agent: impl Into<String>, timeout_secs: u64) -> Self {
        Self::WrapperTimeout {
            agent: agent.into(),
            timeout_secs,
        }
    }

    /// Creates a new `LoopAlreadyRunning` error.
    #[must_use]
    pub fn loop_already_running(state_file: impl Into<PathBuf>) -> Self {
        Self::LoopAlreadyRunning {
            state_file: state_file.into(),
        }
    }

    /// Creates a new `ReportWriteError`.
    #[must_use]
    pub fn report_write(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::ReportWriteError {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Creates a new `StateFileCorrupted` error.
    #[must_use]
    pub fn state_corrupted(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::StateFileCorrupted {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Creates a new `InvalidStateTransition` error.
    #[must_use]
    pub fn invalid_transition(from: impl std::fmt::Display, to: impl std::fmt::Display) -> Self {
        Self::InvalidStateTransition {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    /// Returns `true` if this error is transient and may be retried.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::LlmApiError {
                kind: LlmErrorKind::RateLimit | LlmErrorKind::Server | LlmErrorKind::Network,
                ..
            } | Self::WrapperTimeout { .. }
        )
    }

    /// Returns `true` if this error is fatal and requires immediate termination.
    #[must_use]
    pub const fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::ConfigParseError { .. }
                | Self::ConfigValidationError { .. }
                | Self::TutorialNotFound { .. }
                | Self::TutorialTooLarge { .. }
                | Self::TutorialEncodingError { .. }
                | Self::DockerNotAvailable
                | Self::ImageNotFound { .. }
                | Self::LlmCliNotAvailable { .. }
                | Self::LlmApiError {
                    kind: LlmErrorKind::Authentication,
                    ..
                }
                | Self::LoopAlreadyRunning { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_messages() {
        let err = SmileError::tutorial_not_found("/path/to/tutorial.md");
        let msg = err.to_string();
        assert!(msg.contains("Tutorial not found"));
        assert!(msg.contains("/path/to/tutorial.md"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_llm_error_kind_display() {
        assert_eq!(LlmErrorKind::RateLimit.to_string(), "rate_limit");
        assert_eq!(LlmErrorKind::Authentication.to_string(), "authentication");
    }

    #[test]
    fn test_is_transient() {
        let rate_limit = SmileError::llm_api_error(LlmErrorKind::RateLimit, "Too many requests");
        assert!(rate_limit.is_transient());

        let auth_error = SmileError::llm_api_error(LlmErrorKind::Authentication, "Invalid key");
        assert!(!auth_error.is_transient());

        let docker_error = SmileError::DockerNotAvailable;
        assert!(!docker_error.is_transient());
    }

    #[test]
    fn test_is_fatal() {
        let docker_error = SmileError::DockerNotAvailable;
        assert!(docker_error.is_fatal());

        let auth_error = SmileError::llm_api_error(LlmErrorKind::Authentication, "Invalid key");
        assert!(auth_error.is_fatal());

        let rate_limit = SmileError::llm_api_error(LlmErrorKind::RateLimit, "Too many requests");
        assert!(!rate_limit.is_fatal());
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let smile_err: SmileError = io_err.into();
        assert!(matches!(smile_err, SmileError::Io(_)));
    }

    #[test]
    fn test_tutorial_too_large_display() {
        let err = SmileError::tutorial_too_large("/big/file.md", 150);
        let msg = err.to_string();
        assert!(msg.contains("150KB"));
        assert!(msg.contains("100KB"));
    }
}
