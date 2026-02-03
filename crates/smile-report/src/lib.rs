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
}
