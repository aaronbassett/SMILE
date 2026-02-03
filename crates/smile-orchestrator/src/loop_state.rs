//! Loop state types for the SMILE Loop orchestrator.
//!
//! This module defines the state machine types for tracking loop execution,
//! including status, iteration history, and mentor consultations.
//!
//! ## Persistence
//!
//! The [`LoopState`] supports file persistence for crash recovery:
//! - [`LoopState::save`] writes state atomically to disk
//! - [`LoopState::load`] reads state from disk
//! - [`LoopState::acquire_lock`] prevents concurrent loop execution
//!
//! State files are versioned for forward compatibility.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::{Result, SmileError};

/// Current schema version for state file serialization.
///
/// Increment this when making breaking changes to the state format.
pub const STATE_VERSION: u32 = 1;

/// Default version for deserialization when version field is missing.
const fn default_version() -> u32 {
    STATE_VERSION
}

// ============================================================================
// TerminationReason
// ============================================================================

/// Reason why the SMILE loop terminated.
///
/// Provides detailed information about why the loop stopped, useful for
/// reporting and debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// Tutorial completed successfully.
    Completed,
    /// Maximum iteration limit was reached.
    MaxIterations {
        /// The iteration count when termination occurred.
        reached: u32,
        /// The configured maximum iterations limit.
        limit: u32,
    },
    /// Global timeout was exceeded.
    Timeout {
        /// Elapsed time in seconds when termination occurred.
        elapsed_secs: u64,
        /// The configured timeout limit in seconds.
        limit_secs: u32,
    },
    /// Student encountered an unresolvable blocker.
    Blocker {
        /// Description of the blocker.
        reason: String,
    },
    /// Unrecoverable error occurred.
    Error {
        /// Description of the error.
        message: String,
    },
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completed => write!(f, "Tutorial completed successfully"),
            Self::MaxIterations { reached, limit } => {
                write!(f, "Maximum iterations reached ({reached}/{limit})")
            }
            Self::Timeout {
                elapsed_secs,
                limit_secs,
            } => {
                write!(f, "Timeout exceeded ({elapsed_secs}s/{limit_secs}s)")
            }
            Self::Blocker { reason } => {
                write!(f, "Unresolvable blocker: {reason}")
            }
            Self::Error { message } => {
                write!(f, "Error: {message}")
            }
        }
    }
}

// ============================================================================
// LoopStatus
// ============================================================================

/// Current status of the SMILE loop execution.
///
/// The status transitions through these states:
/// - `Starting` -> `RunningStudent` -> `WaitingForStudent`
/// - From `WaitingForStudent`:
///   - `Completed` (student finished tutorial)
///   - `RunningMentor` -> `WaitingForMentor` -> `RunningStudent` (escalation cycle)
///   - `Blocker` (student cannot complete)
///   - `MaxIterations` (iteration limit reached)
///   - `Timeout` (global timeout exceeded)
///   - `Error` (unrecoverable error)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopStatus {
    /// Loop is initializing.
    #[default]
    Starting,
    /// Student agent is actively processing the tutorial.
    RunningStudent,
    /// Waiting for student agent callback.
    WaitingForStudent,
    /// Mentor agent is processing a question.
    RunningMentor,
    /// Waiting for mentor agent callback.
    WaitingForMentor,
    /// Tutorial completed successfully.
    Completed,
    /// Maximum iteration count reached without completion.
    MaxIterations,
    /// Student encountered an unresolvable blocker.
    Blocker,
    /// Global timeout exceeded.
    Timeout,
    /// Unrecoverable error occurred.
    Error,
}

impl std::fmt::Display for LoopStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Starting => "Starting",
            Self::RunningStudent => "RunningStudent",
            Self::WaitingForStudent => "WaitingForStudent",
            Self::RunningMentor => "RunningMentor",
            Self::WaitingForMentor => "WaitingForMentor",
            Self::Completed => "Completed",
            Self::MaxIterations => "MaxIterations",
            Self::Blocker => "Blocker",
            Self::Timeout => "Timeout",
            Self::Error => "Error",
        };
        write!(f, "{s}")
    }
}

impl LoopStatus {
    /// Returns `true` if this status represents a terminal state.
    ///
    /// Terminal states are: `Completed`, `MaxIterations`, `Blocker`, `Timeout`, `Error`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::LoopStatus;
    ///
    /// assert!(LoopStatus::Completed.is_terminal());
    /// assert!(LoopStatus::Error.is_terminal());
    /// assert!(!LoopStatus::RunningStudent.is_terminal());
    /// ```
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::MaxIterations | Self::Blocker | Self::Timeout | Self::Error
        )
    }

    /// Returns `true` if this status represents a waiting state.
    ///
    /// Waiting states are: `WaitingForStudent`, `WaitingForMentor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::LoopStatus;
    ///
    /// assert!(LoopStatus::WaitingForStudent.is_waiting());
    /// assert!(LoopStatus::WaitingForMentor.is_waiting());
    /// assert!(!LoopStatus::RunningStudent.is_waiting());
    /// ```
    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        matches!(self, Self::WaitingForStudent | Self::WaitingForMentor)
    }
}

// ============================================================================
// StudentStatus and StudentOutput
// ============================================================================

/// Status reported by the student agent after processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudentStatus {
    /// Student successfully completed the tutorial.
    Completed,
    /// Student needs to ask the mentor a question.
    AskMentor,
    /// Student cannot complete the tutorial (blocker).
    CannotComplete,
}

/// Structured output from the student agent.
///
/// Contains the results of a student iteration including status,
/// actions taken, and any questions for the mentor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentOutput {
    /// The student's status after this iteration.
    pub status: StudentStatus,

    /// The current step the student is working on.
    pub current_step: String,

    /// Actions the student attempted during this iteration.
    pub attempted_actions: Vec<String>,

    /// Description of the problem encountered (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,

    /// Question for the mentor (required when `status == AskMentor`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_for_mentor: Option<String>,

    /// Reason for inability to complete (required when `status == CannotComplete`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Summary of what happened in this iteration.
    pub summary: String,

    /// Files created during this iteration.
    #[serde(default)]
    pub files_created: Vec<String>,

    /// Commands run during this iteration.
    #[serde(default)]
    pub commands_run: Vec<String>,
}

impl Default for StudentOutput {
    fn default() -> Self {
        Self {
            status: StudentStatus::Completed,
            current_step: String::new(),
            attempted_actions: Vec::new(),
            problem: None,
            question_for_mentor: None,
            reason: None,
            summary: String::new(),
            files_created: Vec::new(),
            commands_run: Vec::new(),
        }
    }
}

// ============================================================================
// MentorNote
// ============================================================================

/// Record of a mentor consultation.
///
/// Captures the question asked by the student and the mentor's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentorNote {
    /// The iteration number when this consultation occurred.
    pub iteration: u32,

    /// The question asked by the student.
    pub question: String,

    /// The mentor's answer.
    pub answer: String,

    /// When this consultation occurred.
    pub timestamp: DateTime<Utc>,
}

impl MentorNote {
    /// Creates a new `MentorNote` with the current timestamp.
    #[must_use]
    pub fn new(iteration: u32, question: impl Into<String>, answer: impl Into<String>) -> Self {
        Self {
            iteration,
            question: question.into(),
            answer: answer.into(),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// IterationRecord
// ============================================================================

/// Record of a single loop iteration.
///
/// Captures the student output and optional mentor response for history tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationRecord {
    /// The iteration number (1-indexed).
    pub iteration: u32,

    /// The student's output for this iteration.
    pub student_output: StudentOutput,

    /// The mentor's response (if mentor was consulted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentor_output: Option<String>,

    /// When this iteration started.
    pub started_at: DateTime<Utc>,

    /// When this iteration ended.
    pub ended_at: DateTime<Utc>,
}

impl IterationRecord {
    /// Creates a new `IterationRecord` with the given student output.
    ///
    /// Sets `started_at` and `ended_at` to the current time.
    #[must_use]
    pub fn new(iteration: u32, student_output: StudentOutput) -> Self {
        let now = Utc::now();
        Self {
            iteration,
            student_output,
            mentor_output: None,
            started_at: now,
            ended_at: now,
        }
    }

    /// Creates a new `IterationRecord` with explicit timestamps.
    #[must_use]
    pub const fn with_timestamps(
        iteration: u32,
        student_output: StudentOutput,
        started_at: DateTime<Utc>,
        ended_at: DateTime<Utc>,
    ) -> Self {
        Self {
            iteration,
            student_output,
            mentor_output: None,
            started_at,
            ended_at,
        }
    }
}

// ============================================================================
// LoopState
// ============================================================================

/// Complete state of the SMILE loop execution.
///
/// This state is persisted to disk for crash recovery and can be serialized
/// to JSON for the status API endpoint.
///
/// ## Version Compatibility
///
/// The `version` field enables forward compatibility. When loading state files:
/// - Version 1 (current): All fields as documented
/// - Missing version: Treated as version 1 (backward compatible)
///
/// ## Example
///
/// ```
/// use smile_orchestrator::LoopState;
///
/// let state = LoopState::new();
/// assert_eq!(state.version, 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    /// Schema version for this state file.
    ///
    /// Used for forward compatibility when the state format changes.
    #[serde(default = "default_version")]
    pub version: u32,

    /// Current status of the loop.
    pub status: LoopStatus,

    /// Current iteration number (0 before first iteration starts).
    pub iteration: u32,

    /// All mentor consultations that have occurred.
    pub mentor_notes: Vec<MentorNote>,

    /// History of all completed iterations.
    pub history: Vec<IterationRecord>,

    /// When the loop started.
    pub started_at: DateTime<Utc>,

    /// When the state was last updated.
    pub updated_at: DateTime<Utc>,

    /// Error message when status is `Error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Current question from student to pass to mentor.
    ///
    /// Set when transitioning to `RunningMentor`, cleared when mentor responds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_question: Option<String>,
}

impl Default for LoopState {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopState {
    /// Creates a new `LoopState` in the `Starting` status.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let state = LoopState::new();
    /// assert_eq!(state.status, LoopStatus::Starting);
    /// assert_eq!(state.iteration, 0);
    /// assert!(state.mentor_notes.is_empty());
    /// assert!(state.history.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            version: STATE_VERSION,
            status: LoopStatus::Starting,
            iteration: 0,
            mentor_notes: Vec::new(),
            history: Vec::new(),
            started_at: now,
            updated_at: now,
            error_message: None,
            current_question: None,
        }
    }

    /// Returns `true` if the loop is in a terminal state.
    ///
    /// Terminal states are: `Completed`, `MaxIterations`, `Blocker`, `Timeout`, `Error`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// assert!(!state.is_terminal());
    ///
    /// state.status = LoopStatus::Completed;
    /// assert!(state.is_terminal());
    /// ```
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Returns `true` if the loop is actively processing.
    ///
    /// Running states are: `RunningStudent`, `RunningMentor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// assert!(!state.is_running());
    ///
    /// state.status = LoopStatus::RunningStudent;
    /// assert!(state.is_running());
    ///
    /// state.status = LoopStatus::WaitingForStudent;
    /// assert!(!state.is_running());
    /// ```
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(
            self.status,
            LoopStatus::RunningStudent | LoopStatus::RunningMentor
        )
    }

    /// Updates the `updated_at` timestamp to the current time.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Adds a mentor note and updates the timestamp.
    pub fn add_mentor_note(&mut self, note: MentorNote) {
        self.mentor_notes.push(note);
        self.touch();
    }

    /// Adds an iteration record and updates the timestamp.
    pub fn add_iteration(&mut self, record: IterationRecord) {
        self.history.push(record);
        self.touch();
    }

    /// Returns the duration since the loop started.
    #[must_use]
    pub fn elapsed(&self) -> chrono::Duration {
        Utc::now() - self.started_at
    }

    // ========================================================================
    // Termination Condition Methods
    // ========================================================================

    /// Check if the loop has exceeded the global timeout.
    ///
    /// Returns `true` if the elapsed time since `started_at` is greater than
    /// or equal to `timeout_seconds`.
    ///
    /// # Arguments
    ///
    /// * `timeout_seconds` - The maximum allowed duration in seconds
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::LoopState;
    ///
    /// let state = LoopState::new();
    /// // Just created, so timeout should not be triggered
    /// assert!(!state.check_timeout(60));
    /// ```
    #[must_use]
    pub fn check_timeout(&self, timeout_seconds: u32) -> bool {
        let elapsed_secs = self.elapsed().num_seconds();
        // Handle negative durations (clock skew) by treating them as 0
        let elapsed_secs = u64::try_from(elapsed_secs).unwrap_or(0);
        elapsed_secs >= u64::from(timeout_seconds)
    }

    /// Check if the loop has reached the maximum iterations.
    ///
    /// Returns `true` if the current `iteration` count is greater than or
    /// equal to `max_iterations`.
    ///
    /// # Arguments
    ///
    /// * `max_iterations` - The maximum number of iterations allowed
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::LoopState;
    ///
    /// let mut state = LoopState::new();
    /// assert!(!state.check_max_iterations(10)); // iteration = 0
    ///
    /// state.iteration = 10;
    /// assert!(!state.check_max_iterations(10)); // at limit, still running
    ///
    /// state.iteration = 11;
    /// assert!(state.check_max_iterations(10)); // over limit
    /// ```
    #[must_use]
    pub const fn check_max_iterations(&self, max_iterations: u32) -> bool {
        self.iteration > max_iterations
    }

    /// Check all termination conditions and transition state if needed.
    ///
    /// This is the main check to call before each iteration. It checks:
    /// 1. If the loop is already in a terminal state
    /// 2. If the global timeout has been exceeded
    /// 3. If the maximum iterations have been reached
    ///
    /// If a termination condition is met, the state is transitioned to the
    /// appropriate terminal status and that status is returned.
    ///
    /// # Arguments
    ///
    /// * `max_iterations` - The maximum number of iterations allowed
    /// * `timeout_seconds` - The maximum allowed duration in seconds
    ///
    /// # Returns
    ///
    /// * `Some(LoopStatus)` - If a terminal condition was reached
    /// * `None` - If the loop should continue
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// // Fresh state, no termination
    /// assert!(state.check_termination(10, 3600).is_none());
    ///
    /// // At max iterations - still running, not over yet
    /// state.iteration = 10;
    /// assert!(state.check_termination(10, 3600).is_none());
    ///
    /// // Over max iterations - terminates
    /// state.iteration = 11;
    /// let result = state.check_termination(10, 3600);
    /// assert_eq!(result, Some(LoopStatus::MaxIterations));
    /// assert_eq!(state.status, LoopStatus::MaxIterations);
    /// ```
    pub fn check_termination(
        &mut self,
        max_iterations: u32,
        timeout_seconds: u32,
    ) -> Option<LoopStatus> {
        // Already terminal - return current status
        if self.is_terminal() {
            return Some(self.status);
        }

        // Check timeout first (takes priority over max iterations)
        if self.check_timeout(timeout_seconds) {
            // Use the timeout() method to properly transition
            // This will fail if already terminal, but we checked above
            let _ = self.timeout();
            return Some(LoopStatus::Timeout);
        }

        // Check max iterations
        if self.check_max_iterations(max_iterations) {
            self.status = LoopStatus::MaxIterations;
            self.touch();
            return Some(LoopStatus::MaxIterations);
        }

        None
    }

    /// Returns the reason why the loop terminated.
    ///
    /// This method extracts detailed information about the termination,
    /// including specific limits and values where applicable.
    ///
    /// Returns `None` if the loop is not in a terminal state.
    ///
    /// # Arguments
    ///
    /// * `max_iterations` - The configured maximum iterations (for context)
    /// * `timeout_seconds` - The configured timeout in seconds (for context)
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus, TerminationReason};
    ///
    /// let mut state = LoopState::new();
    /// assert!(state.termination_reason(10, 3600).is_none());
    ///
    /// state.status = LoopStatus::Completed;
    /// let reason = state.termination_reason(10, 3600);
    /// assert_eq!(reason, Some(TerminationReason::Completed));
    /// ```
    #[must_use]
    pub fn termination_reason(
        &self,
        max_iterations: u32,
        timeout_seconds: u32,
    ) -> Option<TerminationReason> {
        match self.status {
            LoopStatus::Completed => Some(TerminationReason::Completed),
            LoopStatus::MaxIterations => Some(TerminationReason::MaxIterations {
                reached: self.iteration,
                limit: max_iterations,
            }),
            LoopStatus::Timeout => {
                let elapsed_secs = u64::try_from(self.elapsed().num_seconds()).unwrap_or(0);
                Some(TerminationReason::Timeout {
                    elapsed_secs,
                    limit_secs: timeout_seconds,
                })
            }
            LoopStatus::Blocker => {
                // Try to extract the reason from the last history entry
                let reason = self
                    .history
                    .last()
                    .and_then(|record| record.student_output.reason.clone())
                    .unwrap_or_else(|| "Unknown blocker".to_string());
                Some(TerminationReason::Blocker { reason })
            }
            LoopStatus::Error => {
                let message = self
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string());
                Some(TerminationReason::Error { message })
            }
            // Non-terminal states
            LoopStatus::Starting
            | LoopStatus::RunningStudent
            | LoopStatus::WaitingForStudent
            | LoopStatus::RunningMentor
            | LoopStatus::WaitingForMentor => None,
        }
    }

    /// Returns a human-readable summary of how the loop terminated.
    ///
    /// Only valid when `is_terminal()` is true. Returns `None` for
    /// non-terminal states.
    ///
    /// # Arguments
    ///
    /// * `max_iterations` - The configured maximum iterations (for context)
    /// * `timeout_seconds` - The configured timeout in seconds (for context)
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// assert!(state.termination_summary(10, 3600).is_none());
    ///
    /// state.status = LoopStatus::Completed;
    /// let summary = state.termination_summary(10, 3600);
    /// assert!(summary.is_some());
    /// assert!(summary.unwrap().contains("completed successfully"));
    /// ```
    #[must_use]
    pub fn termination_summary(&self, max_iterations: u32, timeout_seconds: u32) -> Option<String> {
        self.termination_reason(max_iterations, timeout_seconds)
            .map(|reason| reason.to_string())
    }

    // ========================================================================
    // State Transition Methods
    // ========================================================================

    /// Starts the loop, transitioning from `Starting` to `RunningStudent`.
    ///
    /// Sets the iteration to 1.
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is not `Starting`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// assert!(state.start().is_ok());
    /// assert_eq!(state.status, LoopStatus::RunningStudent);
    /// assert_eq!(state.iteration, 1);
    /// ```
    pub fn start(&mut self) -> Result<()> {
        if self.status != LoopStatus::Starting {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::RunningStudent,
            ));
        }
        self.status = LoopStatus::RunningStudent;
        self.iteration = 1;
        self.touch();
        Ok(())
    }

    /// Transitions from `RunningStudent` to `WaitingForStudent`.
    ///
    /// Called when the student agent has been invoked and we are waiting
    /// for its callback.
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is not `RunningStudent`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// state.start().unwrap();
    /// assert!(state.start_waiting_for_student().is_ok());
    /// assert_eq!(state.status, LoopStatus::WaitingForStudent);
    /// ```
    pub fn start_waiting_for_student(&mut self) -> Result<()> {
        if self.status != LoopStatus::RunningStudent {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::WaitingForStudent,
            ));
        }
        self.status = LoopStatus::WaitingForStudent;
        self.touch();
        Ok(())
    }

    /// Processes the result from the student agent.
    ///
    /// Based on the student's status:
    /// - `Completed`: Transitions to `Completed` terminal state
    /// - `AskMentor`: Transitions to `RunningMentor` (or `MaxIterations` if limit reached)
    /// - `CannotComplete`: Transitions to `Blocker` terminal state
    ///
    /// Always records the iteration in history.
    ///
    /// # Arguments
    ///
    /// * `output` - The student's output from this iteration
    /// * `max_iterations` - The maximum number of iterations allowed
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is not `WaitingForStudent`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus, StudentOutput, StudentStatus};
    ///
    /// let mut state = LoopState::new();
    /// state.start().unwrap();
    /// state.start_waiting_for_student().unwrap();
    ///
    /// let output = StudentOutput {
    ///     status: StudentStatus::Completed,
    ///     current_step: "Final step".to_string(),
    ///     summary: "All done!".to_string(),
    ///     ..Default::default()
    /// };
    ///
    /// assert!(state.receive_student_result(output, 10).is_ok());
    /// assert_eq!(state.status, LoopStatus::Completed);
    /// assert_eq!(state.history.len(), 1);
    /// ```
    pub fn receive_student_result(
        &mut self,
        output: StudentOutput,
        max_iterations: u32,
    ) -> Result<()> {
        if self.status != LoopStatus::WaitingForStudent {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::Completed, // Placeholder - actual target depends on output
            ));
        }

        // Record the iteration
        let record = IterationRecord::new(self.iteration, output.clone());
        self.add_iteration(record);

        // Transition based on student status
        match output.status {
            StudentStatus::Completed => {
                self.status = LoopStatus::Completed;
            }
            StudentStatus::AskMentor => {
                // Check if we've hit max iterations before allowing mentor consultation
                if self.iteration >= max_iterations {
                    self.status = LoopStatus::MaxIterations;
                } else {
                    self.status = LoopStatus::RunningMentor;
                    // Store the question for the mentor
                    self.current_question = output.question_for_mentor;
                }
            }
            StudentStatus::CannotComplete => {
                self.status = LoopStatus::Blocker;
            }
        }

        self.touch();
        Ok(())
    }

    /// Transitions from `RunningMentor` to `WaitingForMentor`.
    ///
    /// Called when the mentor agent has been invoked and we are waiting
    /// for its callback.
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is not `RunningMentor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus, StudentOutput, StudentStatus};
    ///
    /// let mut state = LoopState::new();
    /// state.start().unwrap();
    /// state.start_waiting_for_student().unwrap();
    ///
    /// let output = StudentOutput {
    ///     status: StudentStatus::AskMentor,
    ///     current_step: "Step 2".to_string(),
    ///     question_for_mentor: Some("How do I do this?".to_string()),
    ///     summary: "Need help".to_string(),
    ///     ..Default::default()
    /// };
    /// state.receive_student_result(output, 10).unwrap();
    ///
    /// assert!(state.start_waiting_for_mentor().is_ok());
    /// assert_eq!(state.status, LoopStatus::WaitingForMentor);
    /// ```
    pub fn start_waiting_for_mentor(&mut self) -> Result<()> {
        if self.status != LoopStatus::RunningMentor {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::WaitingForMentor,
            ));
        }
        self.status = LoopStatus::WaitingForMentor;
        self.touch();
        Ok(())
    }

    /// Processes the result from the mentor agent.
    ///
    /// Records the mentor note and transitions back to `RunningStudent`
    /// to continue the loop. Increments the iteration count.
    ///
    /// # Arguments
    ///
    /// * `notes` - The mentor's response/advice
    /// * `question` - The question that was asked (for record keeping)
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is not `WaitingForMentor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus, StudentOutput, StudentStatus};
    ///
    /// let mut state = LoopState::new();
    /// state.start().unwrap();
    /// state.start_waiting_for_student().unwrap();
    ///
    /// let output = StudentOutput {
    ///     status: StudentStatus::AskMentor,
    ///     current_step: "Step 2".to_string(),
    ///     question_for_mentor: Some("How do I do this?".to_string()),
    ///     summary: "Need help".to_string(),
    ///     ..Default::default()
    /// };
    /// state.receive_student_result(output, 10).unwrap();
    /// state.start_waiting_for_mentor().unwrap();
    ///
    /// assert!(state.receive_mentor_result("Try this approach".to_string(), "How do I do this?".to_string()).is_ok());
    /// assert_eq!(state.status, LoopStatus::RunningStudent);
    /// assert_eq!(state.iteration, 2);
    /// assert_eq!(state.mentor_notes.len(), 1);
    /// ```
    pub fn receive_mentor_result(&mut self, notes: String, question: String) -> Result<()> {
        if self.status != LoopStatus::WaitingForMentor {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::RunningStudent,
            ));
        }

        // Record the mentor consultation
        let note = MentorNote::new(self.iteration, question, notes);
        self.add_mentor_note(note);

        // Clear the current question
        self.current_question = None;

        // Increment iteration and continue the loop
        self.iteration += 1;
        self.status = LoopStatus::RunningStudent;
        self.touch();
        Ok(())
    }

    /// Forces the loop into the `Timeout` terminal state.
    ///
    /// Can be called from any non-terminal state when the global timeout
    /// has been exceeded.
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is already terminal.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// state.start().unwrap();
    ///
    /// assert!(state.timeout().is_ok());
    /// assert_eq!(state.status, LoopStatus::Timeout);
    /// ```
    pub fn timeout(&mut self) -> Result<()> {
        if self.status.is_terminal() {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::Timeout,
            ));
        }
        self.status = LoopStatus::Timeout;
        self.touch();
        Ok(())
    }

    /// Forces the loop into the `Error` terminal state.
    ///
    /// Can be called from any non-terminal state when an unrecoverable
    /// error has occurred. Stores the error message for debugging.
    ///
    /// # Arguments
    ///
    /// * `message` - Description of the error that occurred
    ///
    /// # Errors
    ///
    /// Returns `InvalidStateTransition` if the current status is already terminal.
    ///
    /// # Examples
    ///
    /// ```
    /// use smile_orchestrator::{LoopState, LoopStatus};
    ///
    /// let mut state = LoopState::new();
    /// state.start().unwrap();
    ///
    /// assert!(state.error("Docker crashed".to_string()).is_ok());
    /// assert_eq!(state.status, LoopStatus::Error);
    /// assert_eq!(state.error_message, Some("Docker crashed".to_string()));
    /// ```
    pub fn error(&mut self, message: String) -> Result<()> {
        if self.status.is_terminal() {
            return Err(SmileError::invalid_transition(
                self.status,
                LoopStatus::Error,
            ));
        }
        self.status = LoopStatus::Error;
        self.error_message = Some(message);
        self.touch();
        Ok(())
    }

    // ========================================================================
    // Persistence Methods
    // ========================================================================

    /// Saves the state to the specified file path.
    ///
    /// This method performs an atomic write by:
    /// 1. Writing to a temporary file (`{path}.tmp`)
    /// 2. Renaming the temporary file to the target path
    ///
    /// This ensures the state file is never in a partial/corrupted state
    /// if the process crashes during write.
    ///
    /// Parent directories are created if they don't exist.
    ///
    /// # Errors
    ///
    /// Returns `SmileError::Io` if:
    /// - Parent directory cannot be created
    /// - Temporary file cannot be written
    /// - File cannot be renamed
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use smile_orchestrator::LoopState;
    ///
    /// # async fn example() -> smile_orchestrator::Result<()> {
    /// let state = LoopState::new();
    /// state.save(Path::new(".smile/state.json")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn save(&self, path: &Path) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        // Serialize to JSON with pretty printing for debuggability
        let json = serde_json::to_string_pretty(self)?;

        // Write to temporary file first for atomicity
        let tmp_path = path.with_extension("json.tmp");
        let mut file = fs::File::create(&tmp_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?;

        // Atomic rename
        fs::rename(&tmp_path, path).await?;

        Ok(())
    }

    /// Loads state from the specified file path.
    ///
    /// Returns `None` if the file does not exist (indicating a fresh start).
    /// Returns the deserialized state if the file exists and is valid.
    ///
    /// # Errors
    ///
    /// Returns `SmileError::StateFileCorrupted` if the file exists but
    /// contains invalid JSON.
    ///
    /// Returns `SmileError::Io` for other file system errors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use smile_orchestrator::LoopState;
    ///
    /// # async fn example() -> smile_orchestrator::Result<()> {
    /// let state = LoopState::load(Path::new(".smile/state.json")).await?;
    /// match state {
    ///     Some(s) => println!("Resuming from iteration {}", s.iteration),
    ///     None => println!("Fresh start"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn load(path: &Path) -> Result<Option<Self>> {
        let contents = match fs::read_to_string(path).await {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };

        let state: Self = serde_json::from_str(&contents)
            .map_err(|e| SmileError::state_corrupted(path, format!("invalid JSON: {e}")))?;

        Ok(Some(state))
    }

    /// Attempts to acquire an exclusive lock on the state file.
    ///
    /// This prevents multiple SMILE loops from running concurrently on the
    /// same tutorial. The lock is held until the returned [`StateLock`] is
    /// dropped.
    ///
    /// The lock file is created at `{path}.lock` (e.g., `.smile/state.lock`
    /// for `.smile/state.json`).
    ///
    /// # Errors
    ///
    /// Returns `SmileError::LoopAlreadyRunning` if another process holds
    /// the lock.
    ///
    /// Returns `SmileError::Io` for file system errors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use smile_orchestrator::LoopState;
    ///
    /// # async fn example() -> smile_orchestrator::Result<()> {
    /// let lock = LoopState::acquire_lock(Path::new(".smile/state.json")).await?;
    /// // Lock is held while `lock` is in scope
    /// println!("Lock acquired, running loop...");
    /// // Lock is automatically released when `lock` is dropped
    /// # Ok(())
    /// # }
    /// ```
    pub async fn acquire_lock(path: &Path) -> Result<StateLock> {
        StateLock::acquire(path).await
    }
}

// ============================================================================
// StateLock
// ============================================================================

/// A guard that holds an exclusive lock on a state file.
///
/// The lock is held as long as this struct exists and is automatically
/// released when dropped. This prevents multiple SMILE loops from running
/// concurrently.
///
/// ## Implementation
///
/// The lock uses a simple lock file approach with `O_CREAT | O_EXCL` semantics:
/// - Creating the lock file atomically fails if it already exists
/// - The lock file is deleted when the guard is dropped
/// - The lock file contains the process ID for debugging
///
/// ## Example
///
/// ```no_run
/// use std::path::Path;
/// use smile_orchestrator::LoopState;
///
/// # async fn example() -> smile_orchestrator::Result<()> {
/// // Acquire lock
/// let lock = LoopState::acquire_lock(Path::new(".smile/state.json")).await?;
///
/// // Do work while lock is held...
///
/// // Lock is released when `lock` goes out of scope
/// drop(lock);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct StateLock {
    /// Path to the lock file.
    lock_path: PathBuf,
}

impl StateLock {
    /// Lock file extension added to state file path.
    const LOCK_EXTENSION: &'static str = "lock";

    /// Attempts to acquire an exclusive lock.
    ///
    /// Creates a lock file adjacent to the state file. The lock file
    /// contains the current process ID for debugging purposes.
    async fn acquire(state_path: &Path) -> Result<Self> {
        // Compute lock file path: .smile/state.json -> .smile/state.lock
        let lock_path = state_path.with_extension(Self::LOCK_EXTENSION);

        // Create parent directories if needed
        if let Some(parent) = lock_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        // Try to create the lock file exclusively
        // This is atomic on POSIX systems
        let lock_result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path);

        match lock_result {
            Ok(mut file) => {
                // Write PID to lock file for debugging
                use std::io::Write;
                let pid = std::process::id();
                let _ = writeln!(file, "{pid}");
                Ok(Self { lock_path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(SmileError::loop_already_running(state_path))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Returns the path to the lock file.
    #[must_use]
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        // Best-effort cleanup - ignore errors during drop
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // LoopStatus tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_loop_status_is_terminal() {
        // Terminal states
        assert!(LoopStatus::Completed.is_terminal());
        assert!(LoopStatus::MaxIterations.is_terminal());
        assert!(LoopStatus::Blocker.is_terminal());
        assert!(LoopStatus::Timeout.is_terminal());
        assert!(LoopStatus::Error.is_terminal());

        // Non-terminal states
        assert!(!LoopStatus::Starting.is_terminal());
        assert!(!LoopStatus::RunningStudent.is_terminal());
        assert!(!LoopStatus::WaitingForStudent.is_terminal());
        assert!(!LoopStatus::RunningMentor.is_terminal());
        assert!(!LoopStatus::WaitingForMentor.is_terminal());
    }

    #[test]
    fn test_loop_status_is_waiting() {
        // Waiting states
        assert!(LoopStatus::WaitingForStudent.is_waiting());
        assert!(LoopStatus::WaitingForMentor.is_waiting());

        // Non-waiting states
        assert!(!LoopStatus::Starting.is_waiting());
        assert!(!LoopStatus::RunningStudent.is_waiting());
        assert!(!LoopStatus::RunningMentor.is_waiting());
        assert!(!LoopStatus::Completed.is_waiting());
        assert!(!LoopStatus::MaxIterations.is_waiting());
        assert!(!LoopStatus::Blocker.is_waiting());
        assert!(!LoopStatus::Timeout.is_waiting());
        assert!(!LoopStatus::Error.is_waiting());
    }

    #[test]
    fn test_loop_status_default() {
        assert_eq!(LoopStatus::default(), LoopStatus::Starting);
    }

    #[test]
    fn test_loop_status_serialization() {
        assert_eq!(
            serde_json::to_string(&LoopStatus::Starting).unwrap(),
            r#""starting""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::RunningStudent).unwrap(),
            r#""running_student""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::WaitingForStudent).unwrap(),
            r#""waiting_for_student""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::RunningMentor).unwrap(),
            r#""running_mentor""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::WaitingForMentor).unwrap(),
            r#""waiting_for_mentor""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::Completed).unwrap(),
            r#""completed""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::MaxIterations).unwrap(),
            r#""max_iterations""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::Blocker).unwrap(),
            r#""blocker""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::Timeout).unwrap(),
            r#""timeout""#
        );
        assert_eq!(
            serde_json::to_string(&LoopStatus::Error).unwrap(),
            r#""error""#
        );
    }

    #[test]
    fn test_loop_status_deserialization() {
        let status: LoopStatus = serde_json::from_str(r#""starting""#).unwrap();
        assert_eq!(status, LoopStatus::Starting);

        let status: LoopStatus = serde_json::from_str(r#""running_student""#).unwrap();
        assert_eq!(status, LoopStatus::RunningStudent);

        let status: LoopStatus = serde_json::from_str(r#""waiting_for_mentor""#).unwrap();
        assert_eq!(status, LoopStatus::WaitingForMentor);

        let status: LoopStatus = serde_json::from_str(r#""max_iterations""#).unwrap();
        assert_eq!(status, LoopStatus::MaxIterations);
    }

    // ------------------------------------------------------------------------
    // StudentStatus and StudentOutput tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_student_status_serialization() {
        assert_eq!(
            serde_json::to_string(&StudentStatus::Completed).unwrap(),
            r#""completed""#
        );
        assert_eq!(
            serde_json::to_string(&StudentStatus::AskMentor).unwrap(),
            r#""ask_mentor""#
        );
        assert_eq!(
            serde_json::to_string(&StudentStatus::CannotComplete).unwrap(),
            r#""cannot_complete""#
        );
    }

    #[test]
    fn test_student_output_default() {
        let output = StudentOutput::default();
        assert_eq!(output.status, StudentStatus::Completed);
        assert!(output.current_step.is_empty());
        assert!(output.attempted_actions.is_empty());
        assert!(output.problem.is_none());
        assert!(output.question_for_mentor.is_none());
        assert!(output.reason.is_none());
        assert!(output.summary.is_empty());
        assert!(output.files_created.is_empty());
        assert!(output.commands_run.is_empty());
    }

    #[test]
    fn test_student_output_serialization() {
        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 3: Install dependencies".to_string(),
            attempted_actions: vec!["npm install".to_string(), "yarn install".to_string()],
            problem: Some("Package not found".to_string()),
            question_for_mentor: Some("Which package manager should I use?".to_string()),
            reason: None,
            summary: "Tried installing dependencies but failed".to_string(),
            files_created: vec!["package.json".to_string()],
            commands_run: vec!["npm init -y".to_string()],
        };

        let json = serde_json::to_string_pretty(&output).unwrap();
        assert!(json.contains(r#""status": "ask_mentor""#));
        assert!(json.contains(r#""current_step": "Step 3: Install dependencies""#));
        assert!(json.contains(r#""question_for_mentor": "Which package manager should I use?""#));
        // reason should not be present (skip_serializing_if)
        assert!(!json.contains("reason"));
    }

    #[test]
    fn test_student_output_deserialization() {
        let json = r#"{
            "status": "cannot_complete",
            "current_step": "Step 5",
            "attempted_actions": ["tried", "again"],
            "reason": "Missing credentials",
            "summary": "Failed to authenticate"
        }"#;

        let output: StudentOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.status, StudentStatus::CannotComplete);
        assert_eq!(output.current_step, "Step 5");
        assert_eq!(output.attempted_actions, vec!["tried", "again"]);
        assert_eq!(output.reason, Some("Missing credentials".to_string()));
        assert!(output.problem.is_none());
        assert!(output.question_for_mentor.is_none());
        // Default values for missing fields
        assert!(output.files_created.is_empty());
        assert!(output.commands_run.is_empty());
    }

    // ------------------------------------------------------------------------
    // MentorNote tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_mentor_note_new() {
        let note = MentorNote::new(1, "How do I install npm?", "Run: apt install nodejs npm");

        assert_eq!(note.iteration, 1);
        assert_eq!(note.question, "How do I install npm?");
        assert_eq!(note.answer, "Run: apt install nodejs npm");
        // Timestamp should be recent (within last second)
        let elapsed = Utc::now() - note.timestamp;
        assert!(elapsed.num_seconds() < 1);
    }

    #[test]
    fn test_mentor_note_serialization() {
        let note = MentorNote {
            iteration: 2,
            question: "What is the config format?".to_string(),
            answer: "Use JSON format".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-02-03T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string(&note).unwrap();
        assert!(json.contains(r#""iteration":2"#));
        assert!(json.contains(r#""question":"What is the config format?""#));
        assert!(json.contains(r#""answer":"Use JSON format""#));
        assert!(json.contains(r#""timestamp":"2026-02-03T10:00:00Z""#));
    }

    // ------------------------------------------------------------------------
    // IterationRecord tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_iteration_record_new() {
        let output = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Step 1".to_string(),
            summary: "Completed step 1".to_string(),
            ..Default::default()
        };

        let record = IterationRecord::new(1, output);

        assert_eq!(record.iteration, 1);
        assert_eq!(record.student_output.current_step, "Step 1");
        assert!(record.mentor_output.is_none());
        // started_at and ended_at should be equal and recent
        assert_eq!(record.started_at, record.ended_at);
        let elapsed = Utc::now() - record.started_at;
        assert!(elapsed.num_seconds() < 1);
    }

    #[test]
    fn test_iteration_record_with_timestamps() {
        let output = StudentOutput::default();
        let start = DateTime::parse_from_rfc3339("2026-02-03T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-02-03T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let record = IterationRecord::with_timestamps(3, output, start, end);

        assert_eq!(record.iteration, 3);
        assert_eq!(record.started_at, start);
        assert_eq!(record.ended_at, end);
    }

    // ------------------------------------------------------------------------
    // LoopState tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_loop_state_new() {
        let state = LoopState::new();

        assert_eq!(state.status, LoopStatus::Starting);
        assert_eq!(state.iteration, 0);
        assert!(state.mentor_notes.is_empty());
        assert!(state.history.is_empty());
        // started_at and updated_at should be equal and recent
        assert_eq!(state.started_at, state.updated_at);
        let elapsed = Utc::now() - state.started_at;
        assert!(elapsed.num_seconds() < 1);
    }

    #[test]
    fn test_loop_state_default() {
        let state = LoopState::default();
        assert_eq!(state.status, LoopStatus::Starting);
        assert_eq!(state.iteration, 0);
    }

    #[test]
    fn test_loop_state_is_terminal() {
        let mut state = LoopState::new();

        // Starting is not terminal
        assert!(!state.is_terminal());

        // Test all terminal states
        state.status = LoopStatus::Completed;
        assert!(state.is_terminal());

        state.status = LoopStatus::MaxIterations;
        assert!(state.is_terminal());

        state.status = LoopStatus::Blocker;
        assert!(state.is_terminal());

        state.status = LoopStatus::Timeout;
        assert!(state.is_terminal());

        state.status = LoopStatus::Error;
        assert!(state.is_terminal());

        // Running is not terminal
        state.status = LoopStatus::RunningStudent;
        assert!(!state.is_terminal());
    }

    #[test]
    fn test_loop_state_is_running() {
        let mut state = LoopState::new();

        // Starting is not running
        assert!(!state.is_running());

        // RunningStudent is running
        state.status = LoopStatus::RunningStudent;
        assert!(state.is_running());

        // RunningMentor is running
        state.status = LoopStatus::RunningMentor;
        assert!(state.is_running());

        // WaitingForStudent is not running
        state.status = LoopStatus::WaitingForStudent;
        assert!(!state.is_running());

        // WaitingForMentor is not running
        state.status = LoopStatus::WaitingForMentor;
        assert!(!state.is_running());

        // Completed is not running
        state.status = LoopStatus::Completed;
        assert!(!state.is_running());
    }

    #[test]
    fn test_loop_state_touch() {
        let mut state = LoopState::new();
        let original_updated_at = state.updated_at;

        // Wait a tiny bit then touch
        std::thread::sleep(std::time::Duration::from_millis(10));
        state.touch();

        assert!(state.updated_at > original_updated_at);
        // started_at should remain unchanged
        assert_eq!(state.started_at, original_updated_at);
    }

    #[test]
    fn test_loop_state_add_mentor_note() {
        let mut state = LoopState::new();
        let original_updated_at = state.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        let note = MentorNote::new(1, "Question", "Answer");
        state.add_mentor_note(note);

        assert_eq!(state.mentor_notes.len(), 1);
        assert_eq!(state.mentor_notes[0].question, "Question");
        assert!(state.updated_at > original_updated_at);
    }

    #[test]
    fn test_loop_state_add_iteration() {
        let mut state = LoopState::new();
        let original_updated_at = state.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        let output = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Step 1".to_string(),
            summary: "Done".to_string(),
            ..Default::default()
        };
        let record = IterationRecord::new(1, output);
        state.add_iteration(record);

        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].iteration, 1);
        assert!(state.updated_at > original_updated_at);
    }

    #[test]
    fn test_loop_state_elapsed() {
        let state = LoopState::new();

        // Elapsed should be very small (just created)
        let elapsed = state.elapsed();
        assert!(elapsed.num_milliseconds() < 100);

        // Wait a bit and check again
        std::thread::sleep(std::time::Duration::from_millis(50));
        let elapsed = state.elapsed();
        assert!(elapsed.num_milliseconds() >= 50);
    }

    #[test]
    fn test_loop_state_serialization() {
        let mut state = LoopState::new();
        state.status = LoopStatus::RunningStudent;
        state.iteration = 3;

        let json = serde_json::to_string_pretty(&state).unwrap();

        assert!(json.contains(r#""status": "running_student""#));
        assert!(json.contains(r#""iteration": 3"#));
        assert!(json.contains("started_at"));
        assert!(json.contains("updated_at"));
        assert!(json.contains("mentor_notes"));
        assert!(json.contains("history"));
    }

    #[test]
    fn test_loop_state_deserialization() {
        let json = r#"{
            "status": "waiting_for_mentor",
            "iteration": 5,
            "mentor_notes": [{
                "iteration": 3,
                "question": "What is X?",
                "answer": "X is Y",
                "timestamp": "2026-02-03T10:00:00Z"
            }],
            "history": [],
            "started_at": "2026-02-03T09:00:00Z",
            "updated_at": "2026-02-03T10:00:00Z"
        }"#;

        let state: LoopState = serde_json::from_str(json).unwrap();

        assert_eq!(state.status, LoopStatus::WaitingForMentor);
        assert_eq!(state.iteration, 5);
        assert_eq!(state.mentor_notes.len(), 1);
        assert_eq!(state.mentor_notes[0].question, "What is X?");
        assert!(state.history.is_empty());
    }

    #[test]
    fn test_full_loop_state_roundtrip() {
        // Create a comprehensive state
        let mut state = LoopState::new();
        state.status = LoopStatus::RunningStudent;
        state.iteration = 2;

        // Add a mentor note
        state.add_mentor_note(MentorNote::new(1, "How do I start?", "Run npm init"));

        // Add an iteration record
        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2: Configure project".to_string(),
            attempted_actions: vec!["read docs".to_string()],
            problem: Some("Config format unclear".to_string()),
            question_for_mentor: Some("What format should config be?".to_string()),
            reason: None,
            summary: "Need help with configuration".to_string(),
            files_created: vec!["package.json".to_string()],
            commands_run: vec!["npm init -y".to_string()],
        };
        state.add_iteration(IterationRecord::new(1, output));

        // Serialize
        let json = serde_json::to_string(&state).unwrap();

        // Deserialize
        let restored: LoopState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.status, LoopStatus::RunningStudent);
        assert_eq!(restored.iteration, 2);
        assert_eq!(restored.mentor_notes.len(), 1);
        assert_eq!(restored.mentor_notes[0].answer, "Run npm init");
        assert_eq!(restored.history.len(), 1);
        assert_eq!(
            restored.history[0].student_output.status,
            StudentStatus::AskMentor
        );
        assert_eq!(
            restored.history[0].student_output.files_created,
            vec!["package.json"]
        );
    }

    // ------------------------------------------------------------------------
    // LoopStatus Display tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_loop_status_display() {
        assert_eq!(LoopStatus::Starting.to_string(), "Starting");
        assert_eq!(LoopStatus::RunningStudent.to_string(), "RunningStudent");
        assert_eq!(
            LoopStatus::WaitingForStudent.to_string(),
            "WaitingForStudent"
        );
        assert_eq!(LoopStatus::RunningMentor.to_string(), "RunningMentor");
        assert_eq!(LoopStatus::WaitingForMentor.to_string(), "WaitingForMentor");
        assert_eq!(LoopStatus::Completed.to_string(), "Completed");
        assert_eq!(LoopStatus::MaxIterations.to_string(), "MaxIterations");
        assert_eq!(LoopStatus::Blocker.to_string(), "Blocker");
        assert_eq!(LoopStatus::Timeout.to_string(), "Timeout");
        assert_eq!(LoopStatus::Error.to_string(), "Error");
    }

    // ------------------------------------------------------------------------
    // State Transition tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_start_transition_success() {
        let mut state = LoopState::new();
        assert_eq!(state.status, LoopStatus::Starting);
        assert_eq!(state.iteration, 0);

        let result = state.start();
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::RunningStudent);
        assert_eq!(state.iteration, 1);
    }

    #[test]
    fn test_start_transition_invalid_from_running() {
        let mut state = LoopState::new();
        state.status = LoopStatus::RunningStudent;

        let result = state.start();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
        assert!(err.to_string().contains("RunningStudent"));
    }

    #[test]
    fn test_start_waiting_for_student_success() {
        let mut state = LoopState::new();
        state.start().unwrap();

        let result = state.start_waiting_for_student();
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::WaitingForStudent);
    }

    #[test]
    fn test_start_waiting_for_student_invalid_from_starting() {
        let mut state = LoopState::new();

        let result = state.start_waiting_for_student();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
        assert!(err.to_string().contains("Starting"));
    }

    #[test]
    fn test_receive_student_result_completed() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Final step".to_string(),
            summary: "Tutorial completed!".to_string(),
            ..Default::default()
        };

        let result = state.receive_student_result(output, 10);
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::Completed);
        assert_eq!(state.history.len(), 1);
        assert_eq!(
            state.history[0].student_output.status,
            StudentStatus::Completed
        );
    }

    #[test]
    fn test_receive_student_result_ask_mentor() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("How do I do this?".to_string()),
            summary: "Need help".to_string(),
            ..Default::default()
        };

        let result = state.receive_student_result(output, 10);
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::RunningMentor);
        assert_eq!(
            state.current_question,
            Some("How do I do this?".to_string())
        );
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn test_receive_student_result_cannot_complete() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::CannotComplete,
            current_step: "Step 3".to_string(),
            reason: Some("Missing credentials".to_string()),
            summary: "Cannot proceed".to_string(),
            ..Default::default()
        };

        let result = state.receive_student_result(output, 10);
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::Blocker);
        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn test_receive_student_result_max_iterations() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        // Simulate reaching max iterations (iteration = 3, max = 3)
        state.iteration = 3;

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("One more question?".to_string()),
            summary: "Still stuck".to_string(),
            ..Default::default()
        };

        let result = state.receive_student_result(output, 3);
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::MaxIterations);
        // Question should not be stored since we didn't go to RunningMentor
        assert!(state.current_question.is_none());
    }

    #[test]
    fn test_receive_student_result_invalid_from_starting() {
        let mut state = LoopState::new();

        let output = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Step 1".to_string(),
            summary: "Done".to_string(),
            ..Default::default()
        };

        let result = state.receive_student_result(output, 10);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
    }

    #[test]
    fn test_start_waiting_for_mentor_success() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("Help?".to_string()),
            summary: "Stuck".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();

        let result = state.start_waiting_for_mentor();
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::WaitingForMentor);
    }

    #[test]
    fn test_start_waiting_for_mentor_invalid_from_waiting_student() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let result = state.start_waiting_for_mentor();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
        assert!(err.to_string().contains("WaitingForStudent"));
    }

    #[test]
    fn test_receive_mentor_result_success() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("How do I configure this?".to_string()),
            summary: "Need config help".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();
        state.start_waiting_for_mentor().unwrap();

        assert_eq!(state.iteration, 1);

        let result = state.receive_mentor_result(
            "Use JSON format for the config".to_string(),
            "How do I configure this?".to_string(),
        );
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::RunningStudent);
        assert_eq!(state.iteration, 2);
        assert_eq!(state.mentor_notes.len(), 1);
        assert_eq!(state.mentor_notes[0].question, "How do I configure this?");
        assert_eq!(
            state.mentor_notes[0].answer,
            "Use JSON format for the config"
        );
        assert!(state.current_question.is_none()); // Should be cleared
    }

    #[test]
    fn test_receive_mentor_result_invalid_from_running_mentor() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("Help?".to_string()),
            summary: "Stuck".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();
        // Don't call start_waiting_for_mentor

        let result = state.receive_mentor_result("Answer".to_string(), "Question".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
        assert!(err.to_string().contains("RunningMentor"));
    }

    #[test]
    fn test_timeout_from_running_student() {
        let mut state = LoopState::new();
        state.start().unwrap();

        let result = state.timeout();
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::Timeout);
    }

    #[test]
    fn test_timeout_from_waiting_for_mentor() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("Help?".to_string()),
            summary: "Stuck".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();
        state.start_waiting_for_mentor().unwrap();

        let result = state.timeout();
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::Timeout);
    }

    #[test]
    fn test_timeout_invalid_from_terminal() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Done".to_string(),
            summary: "All done".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();
        assert_eq!(state.status, LoopStatus::Completed);

        let result = state.timeout();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
        assert!(err.to_string().contains("Completed"));
    }

    #[test]
    fn test_error_from_running() {
        let mut state = LoopState::new();
        state.start().unwrap();

        let result = state.error("Docker container crashed".to_string());
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::Error);
        assert_eq!(
            state.error_message,
            Some("Docker container crashed".to_string())
        );
    }

    #[test]
    fn test_error_from_starting() {
        let mut state = LoopState::new();

        let result = state.error("Initialization failed".to_string());
        assert!(result.is_ok());
        assert_eq!(state.status, LoopStatus::Error);
        assert_eq!(
            state.error_message,
            Some("Initialization failed".to_string())
        );
    }

    #[test]
    fn test_error_invalid_from_terminal() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.timeout().unwrap();
        assert_eq!(state.status, LoopStatus::Timeout);

        let result = state.error("Another error".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid state transition"));
        assert!(err.to_string().contains("Timeout"));
    }

    #[test]
    fn test_full_happy_path_transition_sequence() {
        let mut state = LoopState::new();

        // Start the loop
        state.start().unwrap();
        assert_eq!(state.status, LoopStatus::RunningStudent);
        assert_eq!(state.iteration, 1);

        // Student starts processing
        state.start_waiting_for_student().unwrap();
        assert_eq!(state.status, LoopStatus::WaitingForStudent);

        // Student asks for help
        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 1".to_string(),
            question_for_mentor: Some("How do I start?".to_string()),
            summary: "Need guidance".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();
        assert_eq!(state.status, LoopStatus::RunningMentor);
        assert_eq!(state.history.len(), 1);

        // Mentor starts processing
        state.start_waiting_for_mentor().unwrap();
        assert_eq!(state.status, LoopStatus::WaitingForMentor);

        // Mentor provides answer
        state
            .receive_mentor_result("Run npm init".to_string(), "How do I start?".to_string())
            .unwrap();
        assert_eq!(state.status, LoopStatus::RunningStudent);
        assert_eq!(state.iteration, 2);
        assert_eq!(state.mentor_notes.len(), 1);

        // Student continues
        state.start_waiting_for_student().unwrap();
        assert_eq!(state.status, LoopStatus::WaitingForStudent);

        // Student completes
        let output = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Final step".to_string(),
            summary: "Tutorial completed successfully".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();
        assert_eq!(state.status, LoopStatus::Completed);
        assert_eq!(state.history.len(), 2);
    }

    #[test]
    fn test_multiple_mentor_consultations() {
        let mut state = LoopState::new();
        state.start().unwrap();

        // First iteration - ask mentor
        state.start_waiting_for_student().unwrap();
        let output1 = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 1".to_string(),
            question_for_mentor: Some("Question 1?".to_string()),
            summary: "First question".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output1, 10).unwrap();
        state.start_waiting_for_mentor().unwrap();
        state
            .receive_mentor_result("Answer 1".to_string(), "Question 1?".to_string())
            .unwrap();
        assert_eq!(state.iteration, 2);
        assert_eq!(state.mentor_notes.len(), 1);

        // Second iteration - ask mentor again
        state.start_waiting_for_student().unwrap();
        let output2 = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 2".to_string(),
            question_for_mentor: Some("Question 2?".to_string()),
            summary: "Second question".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output2, 10).unwrap();
        state.start_waiting_for_mentor().unwrap();
        state
            .receive_mentor_result("Answer 2".to_string(), "Question 2?".to_string())
            .unwrap();
        assert_eq!(state.iteration, 3);
        assert_eq!(state.mentor_notes.len(), 2);

        // Third iteration - complete
        state.start_waiting_for_student().unwrap();
        let output3 = StudentOutput {
            status: StudentStatus::Completed,
            current_step: "Done".to_string(),
            summary: "Completed".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output3, 10).unwrap();
        assert_eq!(state.status, LoopStatus::Completed);
        assert_eq!(state.history.len(), 3);
        assert_eq!(state.mentor_notes.len(), 2);
    }

    #[test]
    fn test_new_fields_serialization() {
        let mut state = LoopState::new();
        state.error_message = Some("Test error".to_string());
        state.current_question = Some("Test question".to_string());

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains(r#""error_message": "Test error""#));
        assert!(json.contains(r#""current_question": "Test question""#));

        // Test deserialization
        let restored: LoopState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.error_message, Some("Test error".to_string()));
        assert_eq!(restored.current_question, Some("Test question".to_string()));
    }

    #[test]
    fn test_new_fields_skip_serializing_if_none() {
        let state = LoopState::new();

        let json = serde_json::to_string(&state).unwrap();
        // Fields should not be present when None
        assert!(!json.contains("error_message"));
        assert!(!json.contains("current_question"));
    }

    // ------------------------------------------------------------------------
    // Version field tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_loop_state_has_version() {
        let state = LoopState::new();
        assert_eq!(state.version, STATE_VERSION);
        assert_eq!(state.version, 1);
    }

    #[test]
    fn test_version_serialization() {
        let state = LoopState::new();
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains(r#""version":1"#) || json.contains(r#""version": 1"#));
    }

    #[test]
    fn test_version_defaults_on_missing() {
        // Simulate loading old state without version field
        let json = r#"{
            "status": "starting",
            "iteration": 0,
            "mentor_notes": [],
            "history": [],
            "started_at": "2026-02-03T10:00:00Z",
            "updated_at": "2026-02-03T10:00:00Z"
        }"#;

        let state: LoopState = serde_json::from_str(json).unwrap();
        // Should default to current version
        assert_eq!(state.version, STATE_VERSION);
    }

    // ------------------------------------------------------------------------
    // Persistence tests (async)
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join("smile_test_roundtrip");
        let state_path = temp_dir.join("state.json");

        // Create a state with some data
        let mut state = LoopState::new();
        state.start().unwrap();
        state.iteration = 3;
        state.add_mentor_note(MentorNote::new(1, "Q1", "A1"));

        // Save
        state.save(&state_path).await.unwrap();

        // Verify file exists
        assert!(state_path.exists());

        // Load
        let loaded = LoopState::load(&state_path).await.unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.version, state.version);
        assert_eq!(loaded.status, LoopStatus::RunningStudent);
        assert_eq!(loaded.iteration, 3);
        assert_eq!(loaded.mentor_notes.len(), 1);
        assert_eq!(loaded.mentor_notes[0].question, "Q1");

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_load_missing_file_returns_none() {
        let nonexistent = std::path::PathBuf::from("/nonexistent/path/state.json");
        let result = LoopState::load(&nonexistent).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_save_creates_parent_directories() {
        let temp_dir = std::env::temp_dir().join("smile_test_mkdir");
        let nested_path = temp_dir.join("a").join("b").join("c").join("state.json");

        // Ensure parent doesn't exist
        let _ = std::fs::remove_dir_all(&temp_dir);
        assert!(!temp_dir.exists());

        // Save should create directories
        let state = LoopState::new();
        state.save(&nested_path).await.unwrap();

        // Verify file was created
        assert!(nested_path.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_atomic_write_creates_temp_file() {
        let temp_dir = std::env::temp_dir().join("smile_test_atomic");
        let state_path = temp_dir.join("state.json");
        let tmp_path = state_path.with_extension("json.tmp");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);

        let state = LoopState::new();
        state.save(&state_path).await.unwrap();

        // The temp file should NOT exist after save completes
        // (it should have been renamed to the final path)
        assert!(!tmp_path.exists());
        assert!(state_path.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_load_corrupted_state_returns_error() {
        let temp_dir = std::env::temp_dir().join("smile_test_corrupt");
        let state_path = temp_dir.join("state.json");

        // Create directory and write invalid JSON
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(&state_path, "{ not valid json }").unwrap();

        let result = LoopState::load(&state_path).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_string = err.to_string();
        assert!(
            err_string.contains("Corrupted state file"),
            "Expected StateFileCorrupted error, got: {err_string}"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ------------------------------------------------------------------------
    // StateLock tests
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn test_acquire_lock_creates_lock_file() {
        let temp_dir = std::env::temp_dir().join("smile_test_lock_create");
        let state_path = temp_dir.join("state.json");
        let lock_path = state_path.with_extension("lock");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Acquire lock
        let lock = LoopState::acquire_lock(&state_path).await.unwrap();

        // Lock file should exist
        assert!(lock_path.exists());
        assert_eq!(lock.lock_path(), lock_path);

        // Lock file should contain PID
        let contents = std::fs::read_to_string(&lock_path).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());

        // Cleanup (drop lock first)
        drop(lock);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lock_released_on_drop() {
        let temp_dir = std::env::temp_dir().join("smile_test_lock_drop");
        let state_path = temp_dir.join("state.json");
        let lock_path = state_path.with_extension("lock");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Acquire and drop lock
        {
            let _lock = LoopState::acquire_lock(&state_path).await.unwrap();
            assert!(lock_path.exists());
        }

        // Lock file should be deleted after drop
        assert!(!lock_path.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_acquire_lock_fails_when_already_held() {
        let temp_dir = std::env::temp_dir().join("smile_test_lock_conflict");
        let state_path = temp_dir.join("state.json");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Acquire first lock
        let lock1 = LoopState::acquire_lock(&state_path).await.unwrap();

        // Second attempt should fail
        let result = LoopState::acquire_lock(&state_path).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_string = err.to_string();
        assert!(
            err_string.contains("SMILE loop is already running"),
            "Expected LoopAlreadyRunning error, got: {err_string}"
        );

        // Cleanup
        drop(lock1);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lock_can_be_reacquired_after_release() {
        let temp_dir = std::env::temp_dir().join("smile_test_lock_reacquire");
        let state_path = temp_dir.join("state.json");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);

        // First lock
        {
            let _lock1 = LoopState::acquire_lock(&state_path).await.unwrap();
        }

        // Second lock should succeed after first is released
        let lock2 = LoopState::acquire_lock(&state_path).await;
        assert!(lock2.is_ok());

        // Cleanup
        drop(lock2);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lock_creates_parent_directories() {
        let temp_dir = std::env::temp_dir().join("smile_test_lock_mkdir");
        let nested_path = temp_dir.join("deep").join("nested").join("state.json");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);
        assert!(!temp_dir.exists());

        // Acquire lock - should create directories
        let lock = LoopState::acquire_lock(&nested_path).await.unwrap();

        // Verify lock file was created
        assert!(lock.lock_path().exists());

        // Cleanup
        drop(lock);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_save_load_with_full_state() {
        let temp_dir = std::env::temp_dir().join("smile_test_full_state");
        let state_path = temp_dir.join("state.json");

        // Ensure clean slate
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Create a comprehensive state
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::AskMentor,
            current_step: "Step 1: Install deps".to_string(),
            attempted_actions: vec!["npm install".to_string()],
            problem: Some("Package not found".to_string()),
            question_for_mentor: Some("Which package manager?".to_string()),
            reason: None,
            summary: "Need help with deps".to_string(),
            files_created: vec!["package.json".to_string()],
            commands_run: vec!["npm init -y".to_string()],
        };
        state.receive_student_result(output, 10).unwrap();
        state.start_waiting_for_mentor().unwrap();
        state
            .receive_mentor_result(
                "Use yarn instead".to_string(),
                "Which package manager?".to_string(),
            )
            .unwrap();

        // Save
        state.save(&state_path).await.unwrap();

        // Load and verify
        let loaded = LoopState::load(&state_path).await.unwrap().unwrap();

        assert_eq!(loaded.version, STATE_VERSION);
        assert_eq!(loaded.status, LoopStatus::RunningStudent);
        assert_eq!(loaded.iteration, 2);
        assert_eq!(loaded.history.len(), 1);
        assert_eq!(loaded.mentor_notes.len(), 1);
        assert_eq!(
            loaded.history[0].student_output.current_step,
            "Step 1: Install deps"
        );
        assert_eq!(loaded.mentor_notes[0].answer, "Use yarn instead");

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ------------------------------------------------------------------------
    // Termination condition tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_check_timeout_not_exceeded() {
        let state = LoopState::new();
        // Just created, timeout should not be triggered for any reasonable timeout
        assert!(!state.check_timeout(1));
        assert!(!state.check_timeout(60));
        assert!(!state.check_timeout(3600));
    }

    #[test]
    fn test_check_timeout_with_zero() {
        let state = LoopState::new();
        // Zero timeout should immediately trigger (elapsed >= 0)
        assert!(state.check_timeout(0));
    }

    #[test]
    fn test_check_timeout_with_past_start_time() {
        use chrono::Duration;

        let mut state = LoopState::new();
        // Set started_at to 10 seconds ago
        state.started_at = Utc::now() - Duration::seconds(10);

        assert!(!state.check_timeout(60)); // 10s < 60s
        assert!(!state.check_timeout(11)); // 10s < 11s
        assert!(state.check_timeout(10)); // 10s >= 10s
        assert!(state.check_timeout(5)); // 10s >= 5s
    }

    #[test]
    fn test_check_max_iterations_not_reached() {
        let mut state = LoopState::new();
        assert!(!state.check_max_iterations(1)); // 0 not > 1
        assert!(!state.check_max_iterations(10)); // 0 not > 10

        state.iteration = 5;
        assert!(!state.check_max_iterations(10)); // 5 not > 10
        assert!(!state.check_max_iterations(5)); // 5 not > 5 (at boundary, still running)
    }

    #[test]
    fn test_check_max_iterations_at_boundary() {
        let mut state = LoopState::new();

        state.iteration = 10;
        assert!(!state.check_max_iterations(10)); // 10 not > 10 (at boundary, still running)
        assert!(!state.check_max_iterations(11)); // 10 not > 11
    }

    #[test]
    fn test_check_max_iterations_exceeded() {
        let mut state = LoopState::new();
        state.iteration = 15;

        assert!(state.check_max_iterations(10)); // 15 > 10
        assert!(state.check_max_iterations(14)); // 15 > 14
        assert!(!state.check_max_iterations(15)); // 15 not > 15 (at boundary)
    }

    #[test]
    fn test_check_termination_no_termination() {
        let mut state = LoopState::new();
        // Fresh state should not trigger termination
        let result = state.check_termination(10, 3600);
        assert!(result.is_none());
        assert_eq!(state.status, LoopStatus::Starting);
    }

    #[test]
    fn test_check_termination_already_terminal() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Completed;

        let result = state.check_termination(10, 3600);
        assert_eq!(result, Some(LoopStatus::Completed));
        assert_eq!(state.status, LoopStatus::Completed);
    }

    #[test]
    fn test_check_termination_triggers_timeout() {
        use chrono::Duration;

        let mut state = LoopState::new();
        state.start().unwrap();
        // Set started_at to 100 seconds ago
        state.started_at = Utc::now() - Duration::seconds(100);

        let result = state.check_termination(10, 60); // 100s >= 60s
        assert_eq!(result, Some(LoopStatus::Timeout));
        assert_eq!(state.status, LoopStatus::Timeout);
    }

    #[test]
    fn test_check_termination_triggers_max_iterations() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.iteration = 11; // Over max of 10

        let result = state.check_termination(10, 3600);
        assert_eq!(result, Some(LoopStatus::MaxIterations));
        assert_eq!(state.status, LoopStatus::MaxIterations);
    }

    #[test]
    fn test_check_termination_timeout_takes_priority_over_max_iterations() {
        use chrono::Duration;

        let mut state = LoopState::new();
        state.start().unwrap();
        state.iteration = 11; // Over max of 10
                              // Set started_at to 100 seconds ago
        state.started_at = Utc::now() - Duration::seconds(100);

        // Both conditions met, but timeout should be checked first
        let result = state.check_termination(10, 60);
        assert_eq!(result, Some(LoopStatus::Timeout));
        assert_eq!(state.status, LoopStatus::Timeout);
    }

    #[test]
    fn test_check_termination_updates_timestamp() {
        let mut state = LoopState::new();
        state.start().unwrap();
        let original_updated_at = state.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        state.iteration = 11; // Over max of 10
        state.check_termination(10, 3600);

        assert!(state.updated_at > original_updated_at);
    }

    // ------------------------------------------------------------------------
    // TerminationReason tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_termination_reason_display() {
        assert_eq!(
            TerminationReason::Completed.to_string(),
            "Tutorial completed successfully"
        );

        assert_eq!(
            TerminationReason::MaxIterations {
                reached: 10,
                limit: 10
            }
            .to_string(),
            "Maximum iterations reached (10/10)"
        );

        assert_eq!(
            TerminationReason::Timeout {
                elapsed_secs: 120,
                limit_secs: 60
            }
            .to_string(),
            "Timeout exceeded (120s/60s)"
        );

        assert_eq!(
            TerminationReason::Blocker {
                reason: "Missing API key".to_string()
            }
            .to_string(),
            "Unresolvable blocker: Missing API key"
        );

        assert_eq!(
            TerminationReason::Error {
                message: "Docker crashed".to_string()
            }
            .to_string(),
            "Error: Docker crashed"
        );
    }

    #[test]
    fn test_termination_reason_equality() {
        assert_eq!(TerminationReason::Completed, TerminationReason::Completed);
        assert_ne!(
            TerminationReason::Completed,
            TerminationReason::MaxIterations {
                reached: 10,
                limit: 10
            }
        );
        assert_eq!(
            TerminationReason::MaxIterations {
                reached: 5,
                limit: 10
            },
            TerminationReason::MaxIterations {
                reached: 5,
                limit: 10
            }
        );
        assert_ne!(
            TerminationReason::MaxIterations {
                reached: 5,
                limit: 10
            },
            TerminationReason::MaxIterations {
                reached: 6,
                limit: 10
            }
        );
    }

    // ------------------------------------------------------------------------
    // termination_reason() method tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_termination_reason_for_non_terminal_state() {
        let state = LoopState::new();
        assert!(state.termination_reason(10, 3600).is_none());

        let mut running_state = LoopState::new();
        running_state.start().unwrap();
        assert!(running_state.termination_reason(10, 3600).is_none());
    }

    #[test]
    fn test_termination_reason_for_completed() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Completed;

        let reason = state.termination_reason(10, 3600);
        assert_eq!(reason, Some(TerminationReason::Completed));
    }

    #[test]
    fn test_termination_reason_for_max_iterations() {
        let mut state = LoopState::new();
        state.status = LoopStatus::MaxIterations;
        state.iteration = 10;

        let reason = state.termination_reason(10, 3600);
        assert_eq!(
            reason,
            Some(TerminationReason::MaxIterations {
                reached: 10,
                limit: 10
            })
        );
    }

    #[test]
    fn test_termination_reason_for_timeout() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Timeout;

        let reason = state.termination_reason(10, 60);
        assert!(matches!(
            reason,
            Some(TerminationReason::Timeout { limit_secs: 60, .. })
        ));
    }

    #[test]
    fn test_termination_reason_for_blocker_with_reason() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::CannotComplete,
            current_step: "Step 3".to_string(),
            reason: Some("Missing credentials".to_string()),
            summary: "Cannot proceed".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();

        let reason = state.termination_reason(10, 3600);
        assert_eq!(
            reason,
            Some(TerminationReason::Blocker {
                reason: "Missing credentials".to_string()
            })
        );
    }

    #[test]
    fn test_termination_reason_for_blocker_without_reason() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Blocker;
        // No history entry, should use default

        let reason = state.termination_reason(10, 3600);
        assert_eq!(
            reason,
            Some(TerminationReason::Blocker {
                reason: "Unknown blocker".to_string()
            })
        );
    }

    #[test]
    fn test_termination_reason_for_error_with_message() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.error("Docker container crashed".to_string()).unwrap();

        let reason = state.termination_reason(10, 3600);
        assert_eq!(
            reason,
            Some(TerminationReason::Error {
                message: "Docker container crashed".to_string()
            })
        );
    }

    #[test]
    fn test_termination_reason_for_error_without_message() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Error;
        // No error_message set, should use default

        let reason = state.termination_reason(10, 3600);
        assert_eq!(
            reason,
            Some(TerminationReason::Error {
                message: "Unknown error".to_string()
            })
        );
    }

    // ------------------------------------------------------------------------
    // termination_summary() method tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_termination_summary_for_non_terminal_state() {
        let state = LoopState::new();
        assert!(state.termination_summary(10, 3600).is_none());
    }

    #[test]
    fn test_termination_summary_for_completed() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Completed;

        let summary = state.termination_summary(10, 3600);
        assert!(summary.is_some());
        assert!(summary.unwrap().contains("completed successfully"));
    }

    #[test]
    fn test_termination_summary_for_max_iterations() {
        let mut state = LoopState::new();
        state.status = LoopStatus::MaxIterations;
        state.iteration = 10;

        let summary = state.termination_summary(10, 3600);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Maximum iterations"));
        assert!(summary.contains("10/10"));
    }

    #[test]
    fn test_termination_summary_for_timeout() {
        let mut state = LoopState::new();
        state.status = LoopStatus::Timeout;

        let summary = state.termination_summary(10, 60);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Timeout"));
        assert!(summary.contains("60s"));
    }

    #[test]
    fn test_termination_summary_for_blocker() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.start_waiting_for_student().unwrap();

        let output = StudentOutput {
            status: StudentStatus::CannotComplete,
            current_step: "Step 3".to_string(),
            reason: Some("API key expired".to_string()),
            summary: "Cannot proceed".to_string(),
            ..Default::default()
        };
        state.receive_student_result(output, 10).unwrap();

        let summary = state.termination_summary(10, 3600);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Unresolvable blocker"));
        assert!(summary.contains("API key expired"));
    }

    #[test]
    fn test_termination_summary_for_error() {
        let mut state = LoopState::new();
        state.start().unwrap();
        state.error("Network failure".to_string()).unwrap();

        let summary = state.termination_summary(10, 3600);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Error"));
        assert!(summary.contains("Network failure"));
    }

    #[test]
    fn test_termination_summary_all_terminal_states() {
        // Verify each terminal state returns a non-empty summary
        let terminal_states = [
            LoopStatus::Completed,
            LoopStatus::MaxIterations,
            LoopStatus::Blocker,
            LoopStatus::Timeout,
            LoopStatus::Error,
        ];

        for status in terminal_states {
            let mut state = LoopState::new();
            state.status = status;
            state.iteration = 5; // Set iteration for MaxIterations

            let summary = state.termination_summary(10, 3600);
            assert!(
                summary.is_some(),
                "Expected summary for status {status}, got None"
            );
            assert!(
                !summary.as_ref().unwrap().is_empty(),
                "Expected non-empty summary for status {status}"
            );
        }
    }
}
