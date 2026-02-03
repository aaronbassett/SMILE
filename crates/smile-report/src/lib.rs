//! SMILE Report Generation
//!
//! This crate provides types and utilities for generating reports from SMILE loop results.
//! Reports can be serialized to JSON for programmatic access or rendered to Markdown
//! for human consumption.
//!
//! # Types
//!
//! - [`Report`] - The complete report structure containing all loop results
//! - [`ReportSummary`] - High-level summary of the loop execution
//! - [`Gap`] - A documentation gap identified during the loop
//! - [`TimelineEntry`] - A timestamped event from the loop execution
//! - [`AuditTrail`] - Complete audit trail of commands, files, and LLM calls
//! - [`Recommendation`] - A prioritized improvement suggestion
//!
//! # Generators
//!
//! - [`json::JsonGenerator`] - Generate JSON reports with compact or pretty formatting
//! - [`MarkdownGenerator`] - Generate human-readable Markdown reports
//!
//! # Example
//!
//! ```rust
//! use smile_report::{Report, ReportSummary, ReportStatus, Gap, GapSeverity, GapLocation};
//! use smile_report::json::JsonGenerator;
//!
//! let report = Report {
//!     tutorial_name: "getting-started.md".to_string(),
//!     summary: ReportSummary {
//!         status: ReportStatus::Completed,
//!         iterations: 3,
//!         duration_seconds: 120,
//!         tutorial_path: "/tutorials/getting-started.md".to_string(),
//!     },
//!     gaps: vec![Gap {
//!         id: 1,
//!         title: "Missing dependency".to_string(),
//!         location: GapLocation {
//!             line_number: Some(15),
//!             quote: Some("Run npm install".to_string()),
//!         },
//!         problem: "Package.json not provided".to_string(),
//!         suggested_fix: "Add package.json contents before install step".to_string(),
//!         severity: GapSeverity::Major,
//!     }],
//!     timeline: vec![],
//!     audit_trail: Default::default(),
//!     recommendations: vec![],
//! };
//!
//! // Generate JSON report
//! let generator = JsonGenerator::new(&report);
//! let json = generator.generate_pretty().unwrap();
//! ```

pub mod json;
mod markdown;

pub use markdown::MarkdownGenerator;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during report generation.
#[derive(Debug, Error)]
pub enum ReportError {
    /// Failed to serialize the report to JSON.
    #[error("failed to serialize report: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Failed to read or write report files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid report data.
    #[error("invalid report data: {0}")]
    InvalidData(String),
}

/// Result type for report operations.
pub type Result<T> = std::result::Result<T, ReportError>;

// ============================================================================
// Report Status (local copy to avoid cross-crate dependency)
// ============================================================================

/// Status of the SMILE loop execution.
///
/// This is a local copy of `LoopStatus` from the orchestrator crate to avoid
/// circular dependencies. It represents the final state of the loop when
/// the report was generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
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

impl ReportStatus {
    /// Returns `true` if the status indicates successful completion.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Completed)
    }

    /// Returns `true` if the status indicates a failure state.
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::MaxIterations | Self::Blocker | Self::Timeout | Self::Error
        )
    }

    /// Returns a human-readable description of the status.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Starting => "Loop is initializing",
            Self::RunningStudent => "Student agent is processing",
            Self::WaitingForStudent => "Waiting for student response",
            Self::RunningMentor => "Mentor agent is processing",
            Self::WaitingForMentor => "Waiting for mentor response",
            Self::Completed => "Tutorial completed successfully",
            Self::MaxIterations => "Maximum iterations reached",
            Self::Blocker => "Unresolvable blocker encountered",
            Self::Timeout => "Global timeout exceeded",
            Self::Error => "Unrecoverable error occurred",
        }
    }
}

impl std::fmt::Display for ReportStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

// ============================================================================
// Report
// ============================================================================

/// Complete SMILE loop report.
///
/// This is the top-level structure containing all information about a SMILE
/// loop execution. It includes the summary, identified gaps, timeline of events,
/// audit trail, and recommendations for improvement.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    /// Name of the tutorial that was validated.
    pub tutorial_name: String,

    /// High-level summary of the loop execution.
    pub summary: ReportSummary,

    /// Documentation gaps identified during the loop.
    pub gaps: Vec<Gap>,

    /// Chronological timeline of events.
    pub timeline: Vec<TimelineEntry>,

    /// Complete audit trail of all operations.
    pub audit_trail: AuditTrail,

    /// Prioritized recommendations for improvement.
    pub recommendations: Vec<Recommendation>,
}

impl Report {
    /// Creates a new report builder.
    #[must_use]
    pub fn builder() -> ReportBuilder {
        ReportBuilder::default()
    }

    /// Serializes the report to JSON.
    ///
    /// # Errors
    ///
    /// Returns `ReportError::Serialization` if JSON serialization fails.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(ReportError::from)
    }

    /// Returns the total number of gaps by severity.
    #[must_use]
    pub fn gap_counts(&self) -> GapCounts {
        let mut counts = GapCounts::default();
        for gap in &self.gaps {
            match gap.severity {
                GapSeverity::Critical => counts.critical += 1,
                GapSeverity::Major => counts.major += 1,
                GapSeverity::Minor => counts.minor += 1,
            }
        }
        counts
    }

    /// Returns `true` if the report contains any critical gaps.
    #[must_use]
    pub fn has_critical_gaps(&self) -> bool {
        self.gaps
            .iter()
            .any(|g| g.severity == GapSeverity::Critical)
    }
}

/// Gap counts by severity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GapCounts {
    /// Number of critical gaps.
    pub critical: usize,
    /// Number of major gaps.
    pub major: usize,
    /// Number of minor gaps.
    pub minor: usize,
}

impl GapCounts {
    /// Returns the total number of gaps.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.critical + self.major + self.minor
    }
}

// ============================================================================
// ReportBuilder
// ============================================================================

/// Builder for constructing [`Report`] instances.
#[derive(Debug, Clone, Default)]
pub struct ReportBuilder {
    tutorial_name: Option<String>,
    summary: Option<ReportSummary>,
    gaps: Vec<Gap>,
    timeline: Vec<TimelineEntry>,
    audit_trail: Option<AuditTrail>,
    recommendations: Vec<Recommendation>,
}

impl ReportBuilder {
    /// Sets the tutorial name.
    #[must_use]
    pub fn tutorial_name(mut self, name: impl Into<String>) -> Self {
        self.tutorial_name = Some(name.into());
        self
    }

    /// Sets the report summary.
    #[must_use]
    pub fn summary(mut self, summary: ReportSummary) -> Self {
        self.summary = Some(summary);
        self
    }

    /// Adds a gap to the report.
    #[must_use]
    pub fn gap(mut self, gap: Gap) -> Self {
        self.gaps.push(gap);
        self
    }

    /// Sets all gaps at once.
    #[must_use]
    pub fn gaps(mut self, gaps: Vec<Gap>) -> Self {
        self.gaps = gaps;
        self
    }

    /// Adds a timeline entry.
    #[must_use]
    pub fn timeline_entry(mut self, entry: TimelineEntry) -> Self {
        self.timeline.push(entry);
        self
    }

    /// Sets the complete timeline.
    #[must_use]
    pub fn timeline(mut self, timeline: Vec<TimelineEntry>) -> Self {
        self.timeline = timeline;
        self
    }

    /// Sets the audit trail.
    #[must_use]
    pub fn audit_trail(mut self, audit_trail: AuditTrail) -> Self {
        self.audit_trail = Some(audit_trail);
        self
    }

    /// Adds a recommendation.
    #[must_use]
    pub fn recommendation(mut self, rec: Recommendation) -> Self {
        self.recommendations.push(rec);
        self
    }

    /// Sets all recommendations at once.
    #[must_use]
    pub fn recommendations(mut self, recs: Vec<Recommendation>) -> Self {
        self.recommendations = recs;
        self
    }

    /// Builds the report.
    ///
    /// # Errors
    ///
    /// Returns `ReportError::InvalidData` if required fields are missing.
    pub fn build(self) -> Result<Report> {
        let tutorial_name = self
            .tutorial_name
            .ok_or_else(|| ReportError::InvalidData("tutorial_name is required".to_string()))?;

        let summary = self
            .summary
            .ok_or_else(|| ReportError::InvalidData("summary is required".to_string()))?;

        Ok(Report {
            tutorial_name,
            summary,
            gaps: self.gaps,
            timeline: self.timeline,
            audit_trail: self.audit_trail.unwrap_or_default(),
            recommendations: self.recommendations,
        })
    }
}

// ============================================================================
// ReportSummary
// ============================================================================

/// High-level summary of the SMILE loop execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportSummary {
    /// Final status of the loop.
    pub status: ReportStatus,

    /// Total number of iterations completed.
    pub iterations: u32,

    /// Total duration of the loop in seconds.
    pub duration_seconds: u64,

    /// Path to the tutorial file.
    pub tutorial_path: String,
}

// ============================================================================
// Gap
// ============================================================================

/// A documentation gap identified during the SMILE loop.
///
/// Gaps represent issues in the tutorial that caused problems for the
/// simulated learner (Student agent). Each gap includes the problem
/// description, its location in the tutorial, and a suggested fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    /// Unique identifier for this gap within the report.
    pub id: u32,

    /// Short, descriptive title for the gap.
    pub title: String,

    /// Location of the gap in the tutorial.
    pub location: GapLocation,

    /// Detailed description of the problem encountered.
    pub problem: String,

    /// Suggested fix or improvement for the documentation.
    pub suggested_fix: String,

    /// Severity level of the gap.
    pub severity: GapSeverity,
}

impl Gap {
    /// Creates a new gap builder.
    #[must_use]
    pub fn builder() -> GapBuilder {
        GapBuilder::default()
    }
}

/// Builder for constructing [`Gap`] instances.
#[derive(Debug, Clone, Default)]
pub struct GapBuilder {
    id: Option<u32>,
    title: Option<String>,
    location: Option<GapLocation>,
    problem: Option<String>,
    suggested_fix: Option<String>,
    severity: Option<GapSeverity>,
}

impl GapBuilder {
    /// Sets the gap ID.
    #[must_use]
    pub const fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the gap title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the gap location.
    #[must_use]
    pub fn location(mut self, location: GapLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Sets the problem description.
    #[must_use]
    pub fn problem(mut self, problem: impl Into<String>) -> Self {
        self.problem = Some(problem.into());
        self
    }

    /// Sets the suggested fix.
    #[must_use]
    pub fn suggested_fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }

    /// Sets the severity level.
    #[must_use]
    pub const fn severity(mut self, severity: GapSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    /// Builds the gap.
    ///
    /// # Errors
    ///
    /// Returns `ReportError::InvalidData` if required fields are missing.
    pub fn build(self) -> Result<Gap> {
        let id = self
            .id
            .ok_or_else(|| ReportError::InvalidData("gap id is required".to_string()))?;

        let title = self
            .title
            .ok_or_else(|| ReportError::InvalidData("gap title is required".to_string()))?;

        let location = self.location.unwrap_or_default();

        let problem = self
            .problem
            .ok_or_else(|| ReportError::InvalidData("gap problem is required".to_string()))?;

        let suggested_fix = self.suggested_fix.unwrap_or_default();

        let severity = self.severity.unwrap_or(GapSeverity::Minor);

        Ok(Gap {
            id,
            title,
            location,
            problem,
            suggested_fix,
            severity,
        })
    }
}

// ============================================================================
// GapLocation
// ============================================================================

/// Location of a gap within the tutorial document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GapLocation {
    /// Line number where the gap occurs (1-indexed).
    pub line_number: Option<u32>,

    /// Quoted text from the tutorial related to the gap.
    pub quote: Option<String>,
}

impl GapLocation {
    /// Creates a new location with a line number.
    #[must_use]
    pub const fn at_line(line: u32) -> Self {
        Self {
            line_number: Some(line),
            quote: None,
        }
    }

    /// Creates a new location with a quote.
    #[must_use]
    pub fn with_quote(quote: impl Into<String>) -> Self {
        Self {
            line_number: None,
            quote: Some(quote.into()),
        }
    }

    /// Creates a new location with both line number and quote.
    #[must_use]
    pub fn at_line_with_quote(line: u32, quote: impl Into<String>) -> Self {
        Self {
            line_number: Some(line),
            quote: Some(quote.into()),
        }
    }

    /// Returns `true` if the location has no information.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.line_number.is_none() && self.quote.is_none()
    }
}

// ============================================================================
// GapSeverity
// ============================================================================

/// Severity level of a documentation gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapSeverity {
    /// Blocks progress completely - learner cannot continue without resolution.
    Critical,

    /// Significant confusion or delay - learner struggles but may eventually proceed.
    Major,

    /// Minor clarification needed - small improvement opportunity.
    #[default]
    Minor,
}

impl GapSeverity {
    /// Returns a numeric priority value (lower = more severe).
    #[must_use]
    pub const fn priority(&self) -> u32 {
        match self {
            Self::Critical => 1,
            Self::Major => 2,
            Self::Minor => 3,
        }
    }

    /// Returns a human-readable label for the severity.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Major => "Major",
            Self::Minor => "Minor",
        }
    }
}

impl std::fmt::Display for GapSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ============================================================================
// TimelineEntry
// ============================================================================

/// A timestamped event in the SMILE loop execution timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,

    /// Iteration number when the event occurred.
    pub iteration: u32,

    /// Short description of the event.
    pub event: String,

    /// Optional additional details about the event.
    pub details: Option<String>,
}

impl TimelineEntry {
    /// Creates a new timeline entry.
    #[must_use]
    pub fn new(iteration: u32, event: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            iteration,
            event: event.into(),
            details: None,
        }
    }

    /// Creates a new timeline entry with details.
    #[must_use]
    pub fn with_details(
        iteration: u32,
        event: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            iteration,
            event: event.into(),
            details: Some(details.into()),
        }
    }

    /// Creates a new timeline entry with a specific timestamp.
    #[must_use]
    pub fn at_time(timestamp: DateTime<Utc>, iteration: u32, event: impl Into<String>) -> Self {
        Self {
            timestamp,
            iteration,
            event: event.into(),
            details: None,
        }
    }
}

// ============================================================================
// AuditTrail
// ============================================================================

/// Complete audit trail of operations performed during the SMILE loop.
///
/// This includes all commands executed, files modified, and LLM API calls made.
/// The audit trail provides transparency and reproducibility for the validation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditTrail {
    /// Commands executed during the loop.
    pub commands: Vec<AuditCommand>,

    /// Files created, modified, or deleted.
    pub files: Vec<AuditFile>,

    /// LLM API calls made.
    pub llm_calls: Vec<AuditLlmCall>,
}

impl AuditTrail {
    /// Creates a new empty audit trail.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            files: Vec::new(),
            llm_calls: Vec::new(),
        }
    }

    /// Returns the total number of commands executed.
    #[must_use]
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Returns the total number of file operations.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Returns the total number of LLM calls.
    #[must_use]
    pub fn llm_call_count(&self) -> usize {
        self.llm_calls.len()
    }

    /// Returns the total tokens used across all LLM calls.
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.llm_calls
            .iter()
            .map(|c| u64::from(c.prompt_tokens) + u64::from(c.completion_tokens))
            .sum()
    }

    /// Returns the total LLM duration in milliseconds.
    #[must_use]
    pub fn total_llm_duration_ms(&self) -> u64 {
        self.llm_calls.iter().map(|c| c.duration_ms).sum()
    }
}

// ============================================================================
// AuditCommand
// ============================================================================

/// Record of a command executed during the SMILE loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditCommand {
    /// The command that was executed.
    pub command: String,

    /// Exit code of the command.
    pub exit_code: i32,

    /// Output of the command (may be truncated).
    pub output: String,

    /// When the command was executed.
    pub timestamp: DateTime<Utc>,
}

impl AuditCommand {
    /// Maximum length for command output before truncation.
    pub const MAX_OUTPUT_LENGTH: usize = 4096;

    /// Creates a new audit command record.
    #[must_use]
    pub fn new(command: impl Into<String>, exit_code: i32, output: impl Into<String>) -> Self {
        let output_str = output.into();
        let truncated_output = if output_str.len() > Self::MAX_OUTPUT_LENGTH {
            format!(
                "{}... [truncated, {} bytes total]",
                &output_str[..Self::MAX_OUTPUT_LENGTH],
                output_str.len()
            )
        } else {
            output_str
        };

        Self {
            command: command.into(),
            exit_code,
            output: truncated_output,
            timestamp: Utc::now(),
        }
    }

    /// Returns `true` if the command succeeded (exit code 0).
    #[must_use]
    pub const fn succeeded(&self) -> bool {
        self.exit_code == 0
    }
}

// ============================================================================
// AuditFile
// ============================================================================

/// Record of a file operation during the SMILE loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFile {
    /// Path to the file.
    pub path: String,

    /// Type of operation performed.
    pub operation: FileOperation,

    /// When the operation occurred.
    pub timestamp: DateTime<Utc>,
}

impl AuditFile {
    /// Creates a new file audit record.
    #[must_use]
    pub fn new(path: impl Into<String>, operation: FileOperation) -> Self {
        Self {
            path: path.into(),
            operation,
            timestamp: Utc::now(),
        }
    }

    /// Creates a record for a file creation.
    #[must_use]
    pub fn created(path: impl Into<String>) -> Self {
        Self::new(path, FileOperation::Created)
    }

    /// Creates a record for a file modification.
    #[must_use]
    pub fn modified(path: impl Into<String>) -> Self {
        Self::new(path, FileOperation::Modified)
    }

    /// Creates a record for a file deletion.
    #[must_use]
    pub fn deleted(path: impl Into<String>) -> Self {
        Self::new(path, FileOperation::Deleted)
    }
}

/// Type of file operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

impl std::fmt::Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Modified => write!(f, "modified"),
            Self::Deleted => write!(f, "deleted"),
        }
    }
}

// ============================================================================
// AuditLlmCall
// ============================================================================

/// Record of an LLM API call during the SMILE loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLlmCall {
    /// LLM provider (e.g., "claude", "codex", "gemini").
    pub provider: String,

    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,

    /// Number of tokens in the completion.
    pub completion_tokens: u32,

    /// Duration of the API call in milliseconds.
    pub duration_ms: u64,

    /// When the call was made.
    pub timestamp: DateTime<Utc>,
}

impl AuditLlmCall {
    /// Creates a new LLM call audit record.
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        prompt_tokens: u32,
        completion_tokens: u32,
        duration_ms: u64,
    ) -> Self {
        Self {
            provider: provider.into(),
            prompt_tokens,
            completion_tokens,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    /// Returns the total number of tokens used.
    #[must_use]
    pub const fn total_tokens(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

// ============================================================================
// Recommendation
// ============================================================================

/// A prioritized recommendation for improving the tutorial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Priority of this recommendation (1 = highest priority).
    pub priority: u32,

    /// Category of the recommendation (e.g., "clarity", "completeness", "accuracy").
    pub category: String,

    /// Detailed description of the recommended improvement.
    pub description: String,
}

impl Recommendation {
    /// Creates a new recommendation.
    #[must_use]
    pub fn new(priority: u32, category: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            priority,
            category: category.into(),
            description: description.into(),
        }
    }
}

// ============================================================================
// Input Types (mirrors orchestrator types to avoid circular deps)
// ============================================================================

/// Input data for report generation.
///
/// This mirrors `LoopState` from the orchestrator crate but is owned by
/// `smile-report` to avoid circular dependencies. When generating a report,
/// the orchestrator converts its `LoopState` into this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportInput {
    /// Name of the tutorial being validated.
    pub tutorial_name: String,
    /// Path to the tutorial file.
    pub tutorial_path: String,
    /// Final status of the loop.
    pub status: ReportStatus,
    /// Total number of iterations completed.
    pub iterations: u32,
    /// When the loop started.
    pub started_at: DateTime<Utc>,
    /// When the loop ended.
    pub ended_at: DateTime<Utc>,
    /// History of all iterations.
    pub history: Vec<IterationInput>,
    /// Notes from mentor consultations.
    pub mentor_notes: Vec<MentorNoteInput>,
}

/// Input data for a single iteration.
///
/// This mirrors `IterationRecord` from the orchestrator crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationInput {
    /// Iteration number (1-indexed).
    pub iteration: u32,
    /// Final status of the student agent for this iteration.
    pub student_status: StudentStatusInput,
    /// The step being worked on.
    pub current_step: String,
    /// Problem description if stuck.
    pub problem: Option<String>,
    /// Question for the mentor if status is `AskMentor`.
    pub question_for_mentor: Option<String>,
    /// Reason if status is `CannotComplete`.
    pub reason: Option<String>,
    /// Summary of work done in this iteration.
    pub summary: String,
    /// Files created during this iteration.
    pub files_created: Vec<String>,
    /// Commands executed during this iteration.
    pub commands_run: Vec<String>,
    /// When the iteration started.
    pub started_at: DateTime<Utc>,
    /// When the iteration ended.
    pub ended_at: DateTime<Utc>,
}

/// Student status from a single iteration.
///
/// This mirrors `StudentStatus` from the orchestrator crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudentStatusInput {
    /// Student completed successfully.
    Completed,
    /// Student needs to ask the mentor.
    AskMentor,
    /// Student cannot complete the task.
    CannotComplete,
}

/// Input data for a mentor note.
///
/// This mirrors `MentorNote` from the orchestrator crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentorNoteInput {
    /// Iteration when the question was asked.
    pub iteration: u32,
    /// The question asked by the student.
    pub question: String,
    /// The mentor's answer.
    pub answer: String,
    /// When the mentor responded.
    pub timestamp: DateTime<Utc>,
}

// ============================================================================
// Static Regex Patterns
// ============================================================================

/// Pre-compiled regex patterns for extracting line numbers from step descriptions.
///
/// These patterns are lazily compiled on first use and reused for all subsequent calls,
/// avoiding the overhead of recompiling regexes on every function call.
static LINE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    // These patterns are known to be valid, so unwrap is safe here
    [
        r"[Ll]ine\s*(\d+)", // "line 15", "Line 15"
        r"[Ll](\d+)",       // "L42"
        r"#L(\d+)",         // "#L42" (GitHub-style)
        r"[Ss]tep\s*(\d+)", // "step 3"
        r":(\d+)",          // ":42" (filename:line)
    ]
    .iter()
    .filter_map(|pattern| Regex::new(pattern).ok())
    .collect()
});

// ============================================================================
// ReportGenerator
// ============================================================================

/// Generates reports from loop state history.
///
/// The `ReportGenerator` transforms raw loop execution data into structured
/// reports with gaps, timelines, audit trails, and recommendations.
///
/// # Example
///
/// ```rust
/// use smile_report::{ReportGenerator, ReportInput, ReportStatus};
/// use chrono::Utc;
///
/// let input = ReportInput {
///     tutorial_name: "getting-started.md".to_string(),
///     tutorial_path: "/tutorials/getting-started.md".to_string(),
///     status: ReportStatus::Completed,
///     iterations: 3,
///     started_at: Utc::now(),
///     ended_at: Utc::now(),
///     history: vec![],
///     mentor_notes: vec![],
/// };
///
/// let generator = ReportGenerator::new(input);
/// let report = generator.generate();
/// ```
pub struct ReportGenerator {
    input: ReportInput,
}

impl ReportGenerator {
    /// Creates a new report generator from input data.
    #[must_use]
    pub const fn new(input: ReportInput) -> Self {
        Self { input }
    }

    /// Generates a complete report from the input data.
    ///
    /// This method extracts gaps from the iteration history, builds a timeline
    /// of events, creates an audit trail, and generates recommendations.
    #[must_use]
    pub fn generate(&self) -> Report {
        let duration_seconds = u64::try_from(
            self.input
                .ended_at
                .signed_duration_since(self.input.started_at)
                .num_seconds()
                .max(0),
        )
        .unwrap_or(0);

        let summary = ReportSummary {
            status: self.input.status,
            iterations: self.input.iterations,
            duration_seconds,
            tutorial_path: self.input.tutorial_path.clone(),
        };

        Report {
            tutorial_name: self.input.tutorial_name.clone(),
            summary,
            gaps: self.extract_gaps(),
            timeline: self.build_timeline(),
            audit_trail: self.build_audit_trail(),
            recommendations: self.generate_recommendations(),
        }
    }

    /// Extracts documentation gaps from the iteration history.
    ///
    /// Gaps are identified from:
    /// - `AskMentor` events (severity: Major) - student needed help
    /// - `CannotComplete` events (severity: Critical) - student was blocked
    fn extract_gaps(&self) -> Vec<Gap> {
        let mut gaps = Vec::new();
        let mut gap_id = 1u32;

        for iteration in &self.input.history {
            match iteration.student_status {
                StudentStatusInput::AskMentor => {
                    let mentor_note = self.find_mentor_note(iteration.iteration);
                    let suggested_fix = mentor_note.map_or_else(
                        || "Review and clarify this step".to_string(),
                        |n| n.answer.clone(),
                    );

                    let title = iteration.problem.as_ref().map_or_else(
                        || "Needed mentor guidance".to_string(),
                        |p| truncate_string(p, 50),
                    );

                    let problem = iteration
                        .question_for_mentor
                        .clone()
                        .or_else(|| iteration.problem.clone())
                        .unwrap_or_else(|| "Student required mentor assistance".to_string());

                    gaps.push(Gap {
                        id: gap_id,
                        title,
                        location: Self::extract_location(&iteration.current_step),
                        problem,
                        suggested_fix,
                        severity: GapSeverity::Major,
                    });
                    gap_id += 1;
                }
                StudentStatusInput::CannotComplete => {
                    let title = iteration.reason.as_ref().map_or_else(
                        || "Unable to complete step".to_string(),
                        |r| truncate_string(r, 50),
                    );

                    let problem = iteration
                        .reason
                        .clone()
                        .or_else(|| iteration.problem.clone())
                        .unwrap_or_else(|| "Student could not proceed".to_string());

                    gaps.push(Gap {
                        id: gap_id,
                        title,
                        location: Self::extract_location(&iteration.current_step),
                        problem,
                        suggested_fix: "Verify prerequisites are documented and step is complete"
                            .to_string(),
                        severity: GapSeverity::Critical,
                    });
                    gap_id += 1;
                }
                StudentStatusInput::Completed => {
                    // No gap for completed iterations
                }
            }
        }

        gaps
    }

    /// Finds the mentor note for a specific iteration.
    fn find_mentor_note(&self, iteration: u32) -> Option<&MentorNoteInput> {
        self.input
            .mentor_notes
            .iter()
            .find(|n| n.iteration == iteration)
    }

    /// Extracts location information from a step description.
    ///
    /// Looks for line number patterns like "line 15", "L42", or "step 3"
    /// and extracts them into a `GapLocation`.
    ///
    /// Uses pre-compiled regex patterns from `LINE_PATTERNS` for better performance.
    fn extract_location(step: &str) -> GapLocation {
        // Try to extract line numbers from patterns like "line 15", "L42", etc.
        // Uses lazily-compiled static patterns for performance
        for re in LINE_PATTERNS.iter() {
            if let Some(caps) = re.captures(step) {
                if let Some(num_match) = caps.get(1) {
                    if let Ok(line_num) = num_match.as_str().parse::<u32>() {
                        return GapLocation {
                            line_number: Some(line_num),
                            quote: Some(step.to_string()),
                        };
                    }
                }
            }
        }

        // No line number found, just include the step as a quote
        GapLocation {
            line_number: None,
            quote: if step.is_empty() {
                None
            } else {
                Some(step.to_string())
            },
        }
    }

    /// Builds a timeline of events from the iteration history.
    fn build_timeline(&self) -> Vec<TimelineEntry> {
        let mut timeline = Vec::new();

        // Loop started event
        timeline.push(TimelineEntry {
            timestamp: self.input.started_at,
            iteration: 0,
            event: "Loop started".to_string(),
            details: Some(format!("Tutorial: {}", self.input.tutorial_name)),
        });

        // Events for each iteration
        for iteration in &self.input.history {
            // Iteration started
            timeline.push(TimelineEntry {
                timestamp: iteration.started_at,
                iteration: iteration.iteration,
                event: format!("Iteration {} started", iteration.iteration),
                details: Some(format!("Working on: {}", iteration.current_step)),
            });

            // Status-specific events
            match iteration.student_status {
                StudentStatusInput::AskMentor => {
                    if let Some(question) = &iteration.question_for_mentor {
                        timeline.push(TimelineEntry {
                            timestamp: iteration.ended_at,
                            iteration: iteration.iteration,
                            event: format!(
                                "Student asked mentor: {}",
                                truncate_string(question, 60)
                            ),
                            details: iteration.problem.clone(),
                        });
                    }

                    // Check for mentor response
                    if let Some(note) = self.find_mentor_note(iteration.iteration) {
                        timeline.push(TimelineEntry {
                            timestamp: note.timestamp,
                            iteration: iteration.iteration,
                            event: "Mentor provided guidance".to_string(),
                            details: Some(truncate_string(&note.answer, 100)),
                        });
                    }
                }
                StudentStatusInput::CannotComplete => {
                    let reason = iteration
                        .reason
                        .as_ref()
                        .map_or_else(|| "Unknown blocker".to_string(), |r| truncate_string(r, 60));
                    timeline.push(TimelineEntry {
                        timestamp: iteration.ended_at,
                        iteration: iteration.iteration,
                        event: format!("Blocker encountered: {reason}"),
                        details: iteration.problem.clone(),
                    });
                }
                StudentStatusInput::Completed => {
                    timeline.push(TimelineEntry {
                        timestamp: iteration.ended_at,
                        iteration: iteration.iteration,
                        event: "Iteration completed successfully".to_string(),
                        details: if iteration.summary.is_empty() {
                            None
                        } else {
                            Some(truncate_string(&iteration.summary, 100))
                        },
                    });
                }
            }
        }

        // Loop ended event
        let end_event = match self.input.status {
            ReportStatus::Completed => "Tutorial completed successfully",
            ReportStatus::MaxIterations => "Reached maximum iterations",
            ReportStatus::Blocker => "Blocked by unresolvable issue",
            ReportStatus::Timeout => "Global timeout exceeded",
            ReportStatus::Error => "Unrecoverable error occurred",
            _ => "Loop ended",
        };

        timeline.push(TimelineEntry {
            timestamp: self.input.ended_at,
            iteration: self.input.iterations,
            event: end_event.to_string(),
            details: None,
        });

        timeline
    }

    /// Builds an audit trail from the iteration history.
    fn build_audit_trail(&self) -> AuditTrail {
        let mut audit = AuditTrail::new();

        for iteration in &self.input.history {
            // Add commands from this iteration
            for cmd in &iteration.commands_run {
                audit.commands.push(AuditCommand {
                    command: cmd.clone(),
                    exit_code: 0, // We don't have exit codes in the input
                    output: String::new(),
                    timestamp: iteration.ended_at,
                });
            }

            // Add files from this iteration
            for file in &iteration.files_created {
                audit.files.push(AuditFile {
                    path: file.clone(),
                    operation: FileOperation::Created,
                    timestamp: iteration.ended_at,
                });
            }
        }

        // LLM calls data is not available from loop history
        // It would need to be tracked separately by the orchestrator

        audit
    }

    /// Generates recommendations based on patterns in the history.
    fn generate_recommendations(&self) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();
        let mut priority = 1u32;

        // Recommendation if multiple iterations were needed
        if self.input.iterations > 1 {
            recommendations.push(Recommendation {
                priority,
                category: "complexity".to_string(),
                description: format!(
                    "Tutorial required {} iterations. Consider breaking down complex steps into smaller, more manageable pieces.",
                    self.input.iterations
                ),
            });
            priority += 1;
        }

        // Recommendations for mentor consultations
        let mentor_count = self
            .input
            .history
            .iter()
            .filter(|i| i.student_status == StudentStatusInput::AskMentor)
            .count();

        if mentor_count > 0 {
            // Find the steps that needed mentor help
            let steps_needing_help: Vec<&str> = self
                .input
                .history
                .iter()
                .filter(|i| i.student_status == StudentStatusInput::AskMentor)
                .map(|i| i.current_step.as_str())
                .collect();

            let step_list = if steps_needing_help.len() <= 3 {
                steps_needing_help.join(", ")
            } else {
                format!(
                    "{}, and {} others",
                    steps_needing_help[..2].join(", "),
                    steps_needing_help.len() - 2
                )
            };

            recommendations.push(Recommendation {
                priority,
                category: "clarity".to_string(),
                description: format!(
                    "Mentor was consulted {mentor_count} time(s) for: {step_list}. Add more context or examples to these steps."
                ),
            });
            priority += 1;
        }

        // Recommendations for blockers
        let blocker_count = self
            .input
            .history
            .iter()
            .filter(|i| i.student_status == StudentStatusInput::CannotComplete)
            .count();

        if blocker_count > 0 {
            recommendations.push(Recommendation {
                priority,
                category: "completeness".to_string(),
                description: format!(
                    "Student was blocked {blocker_count} time(s). Verify all prerequisites are documented and all necessary files/code are provided."
                ),
            });
            priority += 1;
        }

        // Recommendation if loop did not complete successfully
        if self.input.status.is_failure() {
            let status_advice = match self.input.status {
                ReportStatus::MaxIterations => {
                    "Tutorial could not be completed within the iteration limit. Review overall tutorial length and complexity."
                }
                ReportStatus::Blocker => {
                    "An unresolvable blocker was encountered. Check for missing prerequisites or unclear instructions."
                }
                ReportStatus::Timeout => {
                    "Tutorial validation exceeded the time limit. Consider simplifying or splitting the tutorial."
                }
                ReportStatus::Error => {
                    "An error occurred during validation. Review the error logs for details."
                }
                _ => "Tutorial validation did not complete successfully.",
            };

            recommendations.push(Recommendation {
                priority,
                category: "outcome".to_string(),
                description: status_advice.to_string(),
            });
        }

        recommendations
    }
}

/// Truncates a string to the specified maximum length, adding "..." if truncated.
/// Uses character boundaries to avoid panics on multibyte UTF-8 characters.
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Find a valid char boundary at or before max_len - 3 (for "...")
        let target = max_len.saturating_sub(3);
        let truncate_at = s
            .char_indices()
            .take_while(|(idx, _)| *idx < target)
            .last()
            .map_or(0, |(idx, c)| idx + c.len_utf8());
        format!("{}...", &s[..truncate_at])
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_report_status_display() {
        assert_eq!(
            ReportStatus::Completed.to_string(),
            "Tutorial completed successfully"
        );
        assert_eq!(
            ReportStatus::Blocker.to_string(),
            "Unresolvable blocker encountered"
        );
    }

    #[test]
    fn test_report_status_is_success() {
        assert!(ReportStatus::Completed.is_success());
        assert!(!ReportStatus::MaxIterations.is_success());
        assert!(!ReportStatus::Starting.is_success());
    }

    #[test]
    fn test_report_status_is_failure() {
        assert!(ReportStatus::MaxIterations.is_failure());
        assert!(ReportStatus::Blocker.is_failure());
        assert!(ReportStatus::Timeout.is_failure());
        assert!(ReportStatus::Error.is_failure());
        assert!(!ReportStatus::Completed.is_failure());
        assert!(!ReportStatus::Starting.is_failure());
    }

    #[test]
    fn test_gap_severity_priority() {
        assert!(GapSeverity::Critical.priority() < GapSeverity::Major.priority());
        assert!(GapSeverity::Major.priority() < GapSeverity::Minor.priority());
    }

    #[test]
    fn test_gap_builder() {
        let gap = Gap::builder()
            .id(1)
            .title("Missing dependency")
            .problem("npm install fails")
            .suggested_fix("Add package.json first")
            .severity(GapSeverity::Major)
            .build()
            .unwrap();

        assert_eq!(gap.id, 1);
        assert_eq!(gap.title, "Missing dependency");
        assert_eq!(gap.severity, GapSeverity::Major);
    }

    #[test]
    fn test_gap_builder_missing_required_fields() {
        let result = Gap::builder().build();
        assert!(result.is_err());

        let result = Gap::builder().id(1).build();
        assert!(result.is_err());

        let result = Gap::builder().id(1).title("test").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_gap_location_constructors() {
        let loc = GapLocation::at_line(42);
        assert_eq!(loc.line_number, Some(42));
        assert!(loc.quote.is_none());

        let loc = GapLocation::with_quote("some text");
        assert!(loc.line_number.is_none());
        assert_eq!(loc.quote.as_deref(), Some("some text"));

        let loc = GapLocation::at_line_with_quote(42, "some text");
        assert_eq!(loc.line_number, Some(42));
        assert_eq!(loc.quote.as_deref(), Some("some text"));
    }

    #[test]
    fn test_gap_location_is_empty() {
        assert!(GapLocation::default().is_empty());
        assert!(!GapLocation::at_line(1).is_empty());
        assert!(!GapLocation::with_quote("test").is_empty());
    }

    #[test]
    fn test_report_builder() {
        let report = Report::builder()
            .tutorial_name("test.md")
            .summary(ReportSummary {
                status: ReportStatus::Completed,
                iterations: 5,
                duration_seconds: 300,
                tutorial_path: "/path/to/test.md".to_string(),
            })
            .gap(Gap {
                id: 1,
                title: "Test gap".to_string(),
                location: GapLocation::default(),
                problem: "Test problem".to_string(),
                suggested_fix: "Test fix".to_string(),
                severity: GapSeverity::Minor,
            })
            .build()
            .unwrap();

        assert_eq!(report.tutorial_name, "test.md");
        assert_eq!(report.gaps.len(), 1);
    }

    #[test]
    fn test_report_gap_counts() {
        let report = Report {
            tutorial_name: "test.md".to_string(),
            summary: ReportSummary::default(),
            gaps: vec![
                Gap {
                    id: 1,
                    title: "Critical".to_string(),
                    location: GapLocation::default(),
                    problem: "p".to_string(),
                    suggested_fix: "f".to_string(),
                    severity: GapSeverity::Critical,
                },
                Gap {
                    id: 2,
                    title: "Major".to_string(),
                    location: GapLocation::default(),
                    problem: "p".to_string(),
                    suggested_fix: "f".to_string(),
                    severity: GapSeverity::Major,
                },
                Gap {
                    id: 3,
                    title: "Minor".to_string(),
                    location: GapLocation::default(),
                    problem: "p".to_string(),
                    suggested_fix: "f".to_string(),
                    severity: GapSeverity::Minor,
                },
            ],
            timeline: vec![],
            audit_trail: AuditTrail::default(),
            recommendations: vec![],
        };

        let counts = report.gap_counts();
        assert_eq!(counts.critical, 1);
        assert_eq!(counts.major, 1);
        assert_eq!(counts.minor, 1);
        assert_eq!(counts.total(), 3);
        assert!(report.has_critical_gaps());
    }

    #[test]
    fn test_report_serialization() {
        let report = Report::default();
        let json = report.to_json().unwrap();
        assert!(json.contains("tutorial_name"));
        assert!(json.contains("summary"));

        let parsed: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tutorial_name, report.tutorial_name);
    }

    #[test]
    fn test_audit_command_truncation() {
        let long_output = "x".repeat(10000);
        let cmd = AuditCommand::new("echo test", 0, long_output);
        assert!(cmd.output.len() < 10000);
        assert!(cmd.output.contains("truncated"));
    }

    #[test]
    fn test_audit_command_succeeded() {
        let cmd = AuditCommand::new("echo", 0, "hello");
        assert!(cmd.succeeded());

        let cmd = AuditCommand::new("false", 1, String::new());
        assert!(!cmd.succeeded());
    }

    #[test]
    fn test_audit_file_constructors() {
        let file = AuditFile::created("/path/to/file");
        assert_eq!(file.operation, FileOperation::Created);

        let file = AuditFile::modified("/path/to/file");
        assert_eq!(file.operation, FileOperation::Modified);

        let file = AuditFile::deleted("/path/to/file");
        assert_eq!(file.operation, FileOperation::Deleted);
    }

    #[test]
    fn test_audit_trail_totals() {
        let mut trail = AuditTrail::new();
        trail
            .llm_calls
            .push(AuditLlmCall::new("claude", 100, 50, 1000));
        trail
            .llm_calls
            .push(AuditLlmCall::new("claude", 200, 100, 2000));

        assert_eq!(trail.total_tokens(), 450);
        assert_eq!(trail.total_llm_duration_ms(), 3000);
        assert_eq!(trail.llm_call_count(), 2);
    }

    #[test]
    fn test_timeline_entry_constructors() {
        let entry = TimelineEntry::new(1, "Test event");
        assert_eq!(entry.iteration, 1);
        assert_eq!(entry.event, "Test event");
        assert!(entry.details.is_none());

        let entry = TimelineEntry::with_details(2, "Another event", "Some details");
        assert_eq!(entry.iteration, 2);
        assert_eq!(entry.details.as_deref(), Some("Some details"));
    }

    #[test]
    fn test_recommendation_new() {
        let rec = Recommendation::new(1, "clarity", "Improve step 3 instructions");
        assert_eq!(rec.priority, 1);
        assert_eq!(rec.category, "clarity");
        assert_eq!(rec.description, "Improve step 3 instructions");
    }

    #[test]
    fn test_file_operation_display() {
        assert_eq!(FileOperation::Created.to_string(), "created");
        assert_eq!(FileOperation::Modified.to_string(), "modified");
        assert_eq!(FileOperation::Deleted.to_string(), "deleted");
    }

    #[test]
    fn test_gap_severity_display() {
        assert_eq!(GapSeverity::Critical.to_string(), "Critical");
        assert_eq!(GapSeverity::Major.to_string(), "Major");
        assert_eq!(GapSeverity::Minor.to_string(), "Minor");
    }

    // ========================================================================
    // ReportGenerator Tests
    // ========================================================================

    fn make_test_input() -> ReportInput {
        ReportInput {
            tutorial_name: "getting-started.md".to_string(),
            tutorial_path: "/tutorials/getting-started.md".to_string(),
            status: ReportStatus::Completed,
            iterations: 2,
            started_at: Utc::now() - chrono::Duration::seconds(120),
            ended_at: Utc::now(),
            history: vec![],
            mentor_notes: vec![],
        }
    }

    #[test]
    fn test_report_generator_empty_history() {
        let input = make_test_input();
        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        assert_eq!(report.tutorial_name, "getting-started.md");
        assert_eq!(report.summary.status, ReportStatus::Completed);
        assert_eq!(report.summary.iterations, 2);
        assert!(report.gaps.is_empty());
        // Timeline has start and end events
        assert!(report.timeline.len() >= 2);
    }

    #[test]
    fn test_report_generator_extracts_ask_mentor_gaps() {
        let mut input = make_test_input();
        input.history.push(IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 3: Configure database".to_string(),
            problem: Some("Database connection fails".to_string()),
            question_for_mentor: Some("How do I set up PostgreSQL?".to_string()),
            reason: None,
            summary: "Tried to connect but failed".to_string(),
            files_created: vec![],
            commands_run: vec!["psql -U postgres".to_string()],
            started_at: Utc::now() - chrono::Duration::seconds(60),
            ended_at: Utc::now() - chrono::Duration::seconds(30),
        });
        input.mentor_notes.push(MentorNoteInput {
            iteration: 1,
            question: "How do I set up PostgreSQL?".to_string(),
            answer: "Install PostgreSQL first and create a database".to_string(),
            timestamp: Utc::now() - chrono::Duration::seconds(25),
        });

        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        assert_eq!(report.gaps.len(), 1);
        let gap = &report.gaps[0];
        assert_eq!(gap.id, 1);
        assert_eq!(gap.severity, GapSeverity::Major);
        assert_eq!(gap.problem, "How do I set up PostgreSQL?");
        assert_eq!(
            gap.suggested_fix,
            "Install PostgreSQL first and create a database"
        );
    }

    #[test]
    fn test_report_generator_extracts_cannot_complete_gaps() {
        let mut input = make_test_input();
        input.status = ReportStatus::Blocker;
        input.history.push(IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::CannotComplete,
            current_step: "Step 5: Deploy to production".to_string(),
            problem: Some("Missing credentials".to_string()),
            question_for_mentor: None,
            reason: Some("AWS credentials not provided in tutorial".to_string()),
            summary: "Cannot deploy".to_string(),
            files_created: vec![],
            commands_run: vec![],
            started_at: Utc::now() - chrono::Duration::seconds(60),
            ended_at: Utc::now() - chrono::Duration::seconds(30),
        });

        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        assert_eq!(report.gaps.len(), 1);
        let gap = &report.gaps[0];
        assert_eq!(gap.id, 1);
        assert_eq!(gap.severity, GapSeverity::Critical);
        assert!(gap.problem.contains("AWS credentials"));
    }

    #[test]
    fn test_report_generator_extract_location_line_number() {
        // Test "line 15" pattern
        let loc = ReportGenerator::extract_location("See line 15 for details");
        assert_eq!(loc.line_number, Some(15));

        // Test "L42" pattern
        let loc = ReportGenerator::extract_location("Error at L42");
        assert_eq!(loc.line_number, Some(42));

        // Test "step 3" pattern
        let loc = ReportGenerator::extract_location("Step 3: Configure database");
        assert_eq!(loc.line_number, Some(3));

        // Test no pattern
        let loc = ReportGenerator::extract_location("Some random text");
        assert!(loc.line_number.is_none());
        assert_eq!(loc.quote.as_deref(), Some("Some random text"));
    }

    #[test]
    fn test_report_generator_builds_timeline() {
        let mut input = make_test_input();
        input.history.push(IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::Completed,
            current_step: "Step 1".to_string(),
            problem: None,
            question_for_mentor: None,
            reason: None,
            summary: "Completed step 1".to_string(),
            files_created: vec![],
            commands_run: vec![],
            started_at: Utc::now() - chrono::Duration::seconds(60),
            ended_at: Utc::now() - chrono::Duration::seconds(30),
        });

        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        // Should have: start, iteration start, iteration complete, end
        assert!(report.timeline.len() >= 4);

        // First event should be "Loop started"
        assert_eq!(report.timeline[0].event, "Loop started");

        // Last event should indicate completion
        assert!(report.timeline.last().unwrap().event.contains("completed"));
    }

    #[test]
    fn test_report_generator_builds_audit_trail() {
        let mut input = make_test_input();
        input.history.push(IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::Completed,
            current_step: "Step 1".to_string(),
            problem: None,
            question_for_mentor: None,
            reason: None,
            summary: "Created files".to_string(),
            files_created: vec!["config.json".to_string(), "main.rs".to_string()],
            commands_run: vec!["cargo build".to_string(), "cargo test".to_string()],
            started_at: Utc::now() - chrono::Duration::seconds(60),
            ended_at: Utc::now() - chrono::Duration::seconds(30),
        });

        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        assert_eq!(report.audit_trail.command_count(), 2);
        assert_eq!(report.audit_trail.file_count(), 2);
        assert_eq!(report.audit_trail.commands[0].command, "cargo build");
        assert_eq!(report.audit_trail.files[0].path, "config.json");
    }

    #[test]
    fn test_report_generator_generates_recommendations() {
        let mut input = make_test_input();
        input.iterations = 3;
        input.history.push(IterationInput {
            iteration: 1,
            student_status: StudentStatusInput::AskMentor,
            current_step: "Step 2".to_string(),
            problem: Some("Confused".to_string()),
            question_for_mentor: Some("What do I do?".to_string()),
            reason: None,
            summary: String::new(),
            files_created: vec![],
            commands_run: vec![],
            started_at: Utc::now() - chrono::Duration::seconds(60),
            ended_at: Utc::now() - chrono::Duration::seconds(30),
        });

        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        // Should have recommendations for multiple iterations and mentor consultation
        assert!(!report.recommendations.is_empty());

        // Check for complexity recommendation
        let has_complexity_rec = report
            .recommendations
            .iter()
            .any(|r| r.category == "complexity");
        assert!(has_complexity_rec);

        // Check for clarity recommendation (mentor was consulted)
        let has_clarity_rec = report
            .recommendations
            .iter()
            .any(|r| r.category == "clarity");
        assert!(has_clarity_rec);
    }

    #[test]
    fn test_report_generator_failure_recommendations() {
        let mut input = make_test_input();
        input.status = ReportStatus::MaxIterations;

        let generator = ReportGenerator::new(input);
        let report = generator.generate();

        // Should have an outcome recommendation
        let has_outcome_rec = report
            .recommendations
            .iter()
            .any(|r| r.category == "outcome");
        assert!(has_outcome_rec);
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("this is a long string", 10), "this is...");
        assert_eq!(truncate_string("abc", 3), "abc");
        assert_eq!(truncate_string("abcd", 3), "...");
    }

    #[test]
    fn test_truncate_string_unicode() {
        // Ensure multibyte UTF-8 characters don't cause panics
        // Each emoji is 4 bytes, so "🚀🎉🔥" is 12 bytes
        let emojis = "🚀🎉🔥";
        // Truncating to 10 bytes (less than 12) should not panic
        let result = truncate_string(emojis, 10);
        // Should truncate at char boundary, keeping at most 2 emojis + "..."
        assert!(result.ends_with("..."));
        // Verify we can iterate over the chars (proves valid UTF-8)
        assert!(result.chars().count() > 0);

        // Test with mixed ASCII and multibyte
        let mixed = "Hello 世界!";
        let result = truncate_string(mixed, 10);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() > 0);
    }

    #[test]
    fn test_student_status_input_serialization() {
        let completed = StudentStatusInput::Completed;
        let json = serde_json::to_string(&completed).unwrap();
        assert_eq!(json, r#""completed""#);

        let ask_mentor = StudentStatusInput::AskMentor;
        let json = serde_json::to_string(&ask_mentor).unwrap();
        assert_eq!(json, r#""ask_mentor""#);

        let cannot_complete = StudentStatusInput::CannotComplete;
        let json = serde_json::to_string(&cannot_complete).unwrap();
        assert_eq!(json, r#""cannot_complete""#);
    }

    #[test]
    fn test_report_input_serialization() {
        let input = make_test_input();
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("tutorial_name"));
        assert!(json.contains("getting-started.md"));

        let parsed: ReportInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tutorial_name, input.tutorial_name);
    }
}
