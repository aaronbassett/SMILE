//! Markdown report generation for SMILE loop results.
//!
//! This module provides the [`MarkdownGenerator`] struct for converting a [`Report`]
//! into a human-readable Markdown document. The generated report includes:
//!
//! - A summary table with key metrics
//! - Documentation gaps organized by severity
//! - A timeline of events
//! - An audit trail of commands, files, and LLM calls
//! - Prioritized recommendations
//!
//! # Example
//!
//! ```rust
//! use smile_report::{Report, ReportSummary, ReportStatus, MarkdownGenerator};
//!
//! let report = Report {
//!     tutorial_name: "getting-started.md".to_string(),
//!     summary: ReportSummary {
//!         status: ReportStatus::Completed,
//!         iterations: 3,
//!         duration_seconds: 120,
//!         tutorial_path: "/tutorials/getting-started.md".to_string(),
//!     },
//!     gaps: vec![],
//!     timeline: vec![],
//!     audit_trail: Default::default(),
//!     recommendations: vec![],
//! };
//!
//! let generator = MarkdownGenerator::new(&report);
//! let markdown = generator.generate();
//! assert!(markdown.contains("# SMILE Validation Report"));
//! ```

use chrono::{DateTime, Utc};
use std::fmt::Write;

use crate::{AuditCommand, AuditFile, AuditLlmCall, Gap, GapSeverity, Report, TimelineEntry};

/// Maximum length for command output in the audit trail table.
const MAX_OUTPUT_DISPLAY_LENGTH: usize = 100;

/// Generates Markdown reports from SMILE loop results.
///
/// The generator takes a reference to a [`Report`] and produces a formatted
/// Markdown string suitable for human review. The output follows a consistent
/// structure with summary metrics, gaps organized by severity, and detailed
/// audit information.
pub struct MarkdownGenerator<'a> {
    report: &'a Report,
}

impl<'a> MarkdownGenerator<'a> {
    /// Creates a new Markdown generator for the given report.
    ///
    /// # Arguments
    ///
    /// * `report` - Reference to the report to generate Markdown for.
    #[must_use]
    pub const fn new(report: &'a Report) -> Self {
        Self { report }
    }

    /// Generates the complete Markdown report.
    ///
    /// This method assembles all sections of the report into a single
    /// Markdown string. The output includes:
    ///
    /// - Title and summary table
    /// - Documentation gaps by severity
    /// - Event timeline
    /// - Audit trail (commands, files, LLM calls)
    /// - Recommendations
    /// - Footer with generation timestamp
    #[must_use]
    pub fn generate(&self) -> String {
        let mut output = String::new();

        self.write_title(&mut output);
        self.write_summary(&mut output);
        self.write_gaps(&mut output);
        self.write_timeline(&mut output);
        self.write_audit_trail(&mut output);
        self.write_recommendations(&mut output);
        Self::write_footer(&mut output);

        output
    }

    /// Writes the report title.
    fn write_title(&self, output: &mut String) {
        let _ = writeln!(
            output,
            "# SMILE Validation Report: {}\n",
            escape_markdown(&self.report.tutorial_name)
        );
    }

    /// Writes the summary section with metrics table.
    fn write_summary(&self, output: &mut String) {
        let counts = self.report.gap_counts();

        let _ = writeln!(output, "## Summary\n");
        let _ = writeln!(output, "| Metric | Value |");
        let _ = writeln!(output, "|--------|-------|");
        let _ = writeln!(
            output,
            "| Status | {} |",
            self.report.summary.status.description()
        );
        let _ = writeln!(
            output,
            "| Iterations | {} |",
            self.report.summary.iterations
        );
        let _ = writeln!(
            output,
            "| Duration | {} |",
            format_duration(self.report.summary.duration_seconds)
        );
        let _ = writeln!(
            output,
            "| Tutorial | {} |",
            escape_markdown(&self.report.summary.tutorial_path)
        );
        let _ = writeln!(
            output,
            "| Gaps Found | {} ({} critical, {} major, {} minor) |",
            counts.total(),
            format_gap_count(counts.critical, "critical"),
            format_gap_count(counts.major, "major"),
            format_gap_count(counts.minor, "minor"),
        );
        let _ = writeln!(output);
    }

    /// Writes the documentation gaps section.
    fn write_gaps(&self, output: &mut String) {
        let _ = writeln!(output, "## Documentation Gaps\n");

        if self.report.gaps.is_empty() {
            let _ = writeln!(output, "*No documentation gaps identified.*\n");
            return;
        }

        // Collect gaps by severity
        let critical: Vec<_> = self
            .report
            .gaps
            .iter()
            .filter(|g| g.severity == GapSeverity::Critical)
            .collect();
        let major: Vec<_> = self
            .report
            .gaps
            .iter()
            .filter(|g| g.severity == GapSeverity::Major)
            .collect();
        let minor: Vec<_> = self
            .report
            .gaps
            .iter()
            .filter(|g| g.severity == GapSeverity::Minor)
            .collect();

        Self::write_gap_section(output, "Critical Gaps", &critical);
        Self::write_gap_section(output, "Major Gaps", &major);
        Self::write_gap_section(output, "Minor Gaps", &minor);
    }

    /// Writes a section for gaps of a specific severity.
    fn write_gap_section(output: &mut String, title: &str, gaps: &[&Gap]) {
        let icon = severity_icon_from_title(title);
        let _ = writeln!(output, "### {icon} {title}\n");

        if gaps.is_empty() {
            let _ = writeln!(output, "*None*\n");
            return;
        }

        for gap in gaps {
            Self::write_gap(output, gap);
        }
    }

    /// Writes a single gap entry.
    fn write_gap(output: &mut String, gap: &Gap) {
        let id = gap.id;
        let title = escape_markdown(&gap.title);
        let _ = writeln!(output, "#### Gap #{id}: {title}\n");

        // Location line
        let location_parts = build_location_parts(gap);
        if !location_parts.is_empty() {
            let _ = writeln!(output, "**Location**: {location_parts}");
        }

        let problem = escape_markdown(&gap.problem);
        let _ = writeln!(output, "**Problem**: {problem}");
        let suggested_fix = escape_markdown(&gap.suggested_fix);
        let _ = writeln!(output, "**Suggested Fix**: {suggested_fix}\n");
    }

    /// Writes the timeline section.
    fn write_timeline(&self, output: &mut String) {
        let _ = writeln!(output, "## Timeline\n");

        if self.report.timeline.is_empty() {
            let _ = writeln!(output, "*No timeline events recorded.*\n");
            return;
        }

        let _ = writeln!(output, "| Time | Iteration | Event | Details |");
        let _ = writeln!(output, "|------|-----------|-------|---------|");

        for entry in &self.report.timeline {
            Self::write_timeline_entry(output, entry);
        }

        let _ = writeln!(output);
    }

    /// Writes a single timeline entry row.
    fn write_timeline_entry(output: &mut String, entry: &TimelineEntry) {
        let details = entry
            .details
            .as_deref()
            .map(escape_markdown)
            .unwrap_or_default();

        let time = format_timestamp(&entry.timestamp);
        let iteration = entry.iteration;
        let event = escape_markdown(&entry.event);
        let _ = writeln!(output, "| {time} | #{iteration} | {event} | {details} |");
    }

    /// Writes the audit trail section.
    fn write_audit_trail(&self, output: &mut String) {
        let _ = writeln!(output, "## Audit Trail\n");

        self.write_commands(output);
        self.write_files(output);
        self.write_llm_calls(output);
    }

    /// Writes the commands executed subsection.
    fn write_commands(&self, output: &mut String) {
        let _ = writeln!(output, "### Commands Executed\n");

        if self.report.audit_trail.commands.is_empty() {
            let _ = writeln!(output, "*No commands executed.*\n");
            return;
        }

        let _ = writeln!(output, "| Time | Command | Exit | Output |");
        let _ = writeln!(output, "|------|---------|------|--------|");

        for cmd in &self.report.audit_trail.commands {
            Self::write_command_entry(output, cmd);
        }

        let _ = writeln!(output);
    }

    /// Writes a single command entry row.
    fn write_command_entry(output: &mut String, cmd: &AuditCommand) {
        let truncated_output = truncate_output(&cmd.output, MAX_OUTPUT_DISPLAY_LENGTH);

        let time = format_timestamp(&cmd.timestamp);
        let command = escape_markdown_inline_code(&cmd.command);
        let exit_code = cmd.exit_code;
        let output_escaped = escape_markdown(&truncated_output);
        let _ = writeln!(
            output,
            "| {time} | `{command}` | {exit_code} | {output_escaped} |"
        );
    }

    /// Writes the files modified subsection.
    fn write_files(&self, output: &mut String) {
        let _ = writeln!(output, "### Files Modified\n");

        if self.report.audit_trail.files.is_empty() {
            let _ = writeln!(output, "*No files modified.*\n");
            return;
        }

        let _ = writeln!(output, "| Time | Operation | Path |");
        let _ = writeln!(output, "|------|-----------|------|");

        for file in &self.report.audit_trail.files {
            Self::write_file_entry(output, file);
        }

        let _ = writeln!(output);
    }

    /// Writes a single file entry row.
    fn write_file_entry(output: &mut String, file: &AuditFile) {
        let time = format_timestamp(&file.timestamp);
        let operation = file.operation;
        let path = escape_markdown(&file.path);
        let _ = writeln!(output, "| {time} | {operation} | {path} |");
    }

    /// Writes the LLM calls subsection.
    fn write_llm_calls(&self, output: &mut String) {
        let _ = writeln!(output, "### LLM Calls\n");

        if self.report.audit_trail.llm_calls.is_empty() {
            let _ = writeln!(output, "*No LLM calls made.*\n");
            return;
        }

        let _ = writeln!(output, "| Time | Provider | Tokens | Duration |");
        let _ = writeln!(output, "|------|----------|--------|----------|");

        for call in &self.report.audit_trail.llm_calls {
            Self::write_llm_call_entry(output, call);
        }

        let _ = writeln!(output);
    }

    /// Writes a single LLM call entry row.
    fn write_llm_call_entry(output: &mut String, call: &AuditLlmCall) {
        let time = format_timestamp(&call.timestamp);
        let provider = escape_markdown(&call.provider);
        let prompt = call.prompt_tokens;
        let completion = call.completion_tokens;
        let duration = call.duration_ms;
        let _ = writeln!(
            output,
            "| {time} | {provider} | {prompt}+{completion} | {duration}ms |"
        );
    }

    /// Writes the recommendations section.
    fn write_recommendations(&self, output: &mut String) {
        let _ = writeln!(output, "## Recommendations\n");

        if self.report.recommendations.is_empty() {
            let _ = writeln!(output, "*No specific recommendations.*\n");
            return;
        }

        // Sort recommendations by priority for display
        let mut sorted_recs: Vec<_> = self.report.recommendations.iter().collect();
        sorted_recs.sort_by_key(|r| r.priority);

        for (index, rec) in sorted_recs.iter().enumerate() {
            let _ = writeln!(
                output,
                "{}. **[{}]** {}",
                index + 1,
                escape_markdown(&rec.category),
                escape_markdown(&rec.description),
            );
        }

        let _ = writeln!(output);
    }

    /// Writes the report footer.
    fn write_footer(output: &mut String) {
        let _ = writeln!(output, "---");
        let timestamp = format_timestamp(&Utc::now());
        let _ = writeln!(output, "*Generated by SMILE Loop at {timestamp}*");
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Formats a duration in seconds to a human-readable string.
///
/// Examples:
/// - 65 seconds -> "1m 5s"
/// - 3661 seconds -> "1h 1m 1s"
/// - 45 seconds -> "45s"
fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    let mut parts = Vec::new();

    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 || parts.is_empty() {
        parts.push(format!("{secs}s"));
    }

    parts.join(" ")
}

/// Formats a timestamp to a human-readable string.
///
/// Format: "YYYY-MM-DD HH:MM:SS UTC"
fn format_timestamp(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Formats a gap count with the appropriate indicator.
fn format_gap_count(count: usize, severity: &str) -> String {
    let icon = match severity {
        "critical" => "red_circle",
        "major" => "large_orange_diamond",
        "minor" => "yellow_circle",
        _ => "",
    };

    if icon.is_empty() {
        format!("{count} {severity}")
    } else {
        // Use HTML entity for emoji to ensure cross-platform compatibility
        let emoji = match severity {
            "critical" => "&#128308;",
            "major" => "&#128992;",
            "minor" => "&#128993;",
            _ => "",
        };
        format!("{emoji} {count}")
    }
}

/// Returns the appropriate icon for a gap severity section title.
fn severity_icon_from_title(title: &str) -> &'static str {
    if title.contains("Critical") {
        "&#128308;"
    } else if title.contains("Major") {
        "&#128992;"
    } else if title.contains("Minor") {
        "&#128993;"
    } else {
        ""
    }
}

/// Builds the location string for a gap.
fn build_location_parts(gap: &Gap) -> String {
    let mut parts = Vec::new();

    if let Some(line) = gap.location.line_number {
        parts.push(format!("Line {line}"));
    }

    if let Some(quote) = &gap.location.quote {
        parts.push(format!("`{}`", escape_markdown_inline_code(quote)));
    }

    parts.join(" | ")
}

/// Escapes special Markdown characters in text.
///
/// This prevents user content from being interpreted as Markdown formatting.
fn escape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            '*' | '_' | '`' | '#' | '[' | ']' | '(' | ')' | '!' | '\\' | '<' | '>' | '|' => {
                result.push('\\');
                result.push(ch);
            }
            '\n' => {
                // Replace newlines with <br> for table cells
                result.push_str("<br>");
            }
            _ => result.push(ch),
        }
    }

    result
}

/// Escapes backticks in text intended for inline code.
///
/// For inline code, we only need to handle backticks specially.
fn escape_markdown_inline_code(text: &str) -> String {
    text.replace('`', "'")
}

/// Truncates output to a maximum length, adding an ellipsis if needed.
/// Uses character boundaries to avoid panics on multibyte UTF-8 characters.
fn truncate_output(output: &str, max_length: usize) -> String {
    // Take only the first line to avoid table formatting issues
    let first_line = output.lines().next().unwrap_or("");

    if first_line.len() <= max_length {
        first_line.to_string()
    } else {
        // Find a valid char boundary at or before max_length
        let truncate_at = first_line
            .char_indices()
            .take_while(|(idx, _)| *idx < max_length)
            .last()
            .map_or(0, |(idx, c)| idx + c.len_utf8());
        format!("{}...", &first_line[..truncate_at])
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{
        AuditCommand, AuditFile, AuditLlmCall, AuditTrail, Gap, GapLocation, GapSeverity,
        Recommendation, Report, ReportStatus, ReportSummary, TimelineEntry,
    };

    fn sample_report() -> Report {
        Report {
            tutorial_name: "getting-started.md".to_string(),
            summary: ReportSummary {
                status: ReportStatus::Completed,
                iterations: 5,
                duration_seconds: 332,
                tutorial_path: "/tutorials/getting-started.md".to_string(),
            },
            gaps: vec![
                Gap {
                    id: 1,
                    title: "Missing package.json".to_string(),
                    location: GapLocation::at_line_with_quote(15, "Run npm install"),
                    problem:
                        "The tutorial instructs to run npm install but package.json is not provided"
                            .to_string(),
                    suggested_fix: "Add package.json contents before the install step".to_string(),
                    severity: GapSeverity::Critical,
                },
                Gap {
                    id: 2,
                    title: "Unclear environment variable".to_string(),
                    location: GapLocation::at_line(42),
                    problem: "DATABASE_URL is referenced but not explained".to_string(),
                    suggested_fix: "Add a section explaining required environment variables"
                        .to_string(),
                    severity: GapSeverity::Major,
                },
                Gap {
                    id: 3,
                    title: "Typo in command".to_string(),
                    location: GapLocation::with_quote("npm rn dev"),
                    problem: "Command has a typo".to_string(),
                    suggested_fix: "Change 'npm rn dev' to 'npm run dev'".to_string(),
                    severity: GapSeverity::Minor,
                },
            ],
            timeline: vec![
                TimelineEntry::with_details(1, "Student started", "Beginning tutorial"),
                TimelineEntry::new(2, "Mentor consulted"),
            ],
            audit_trail: AuditTrail {
                commands: vec![
                    AuditCommand::new("npm install", 1, "npm ERR! missing package.json"),
                    AuditCommand::new("npm run dev", 0, "Server started on port 3000"),
                ],
                files: vec![
                    AuditFile::created("package.json"),
                    AuditFile::modified("src/index.js"),
                ],
                llm_calls: vec![
                    AuditLlmCall::new("claude", 1500, 500, 2500),
                    AuditLlmCall::new("claude", 800, 200, 1200),
                ],
            },
            recommendations: vec![
                Recommendation::new(
                    1,
                    "completeness",
                    "Add all prerequisite files to the tutorial",
                ),
                Recommendation::new(
                    2,
                    "clarity",
                    "Explain environment variables in a dedicated section",
                ),
            ],
        }
    }

    #[test]
    fn test_generate_contains_title() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("# SMILE Validation Report: getting-started.md"));
    }

    #[test]
    fn test_generate_contains_summary_table() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("## Summary"));
        assert!(markdown.contains("| Status | Tutorial completed successfully |"));
        assert!(markdown.contains("| Iterations | 5 |"));
        assert!(markdown.contains("| Duration | 5m 32s |"));
    }

    #[test]
    fn test_generate_contains_gaps() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("## Documentation Gaps"));
        assert!(markdown.contains("### &#128308; Critical Gaps"));
        assert!(markdown.contains("#### Gap #1: Missing package.json"));
        assert!(markdown.contains("**Location**: Line 15 | `Run npm install`"));
    }

    #[test]
    fn test_generate_contains_timeline() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("## Timeline"));
        assert!(markdown.contains("| Time | Iteration | Event | Details |"));
        assert!(markdown.contains("Student started"));
    }

    #[test]
    fn test_generate_contains_audit_trail() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("## Audit Trail"));
        assert!(markdown.contains("### Commands Executed"));
        assert!(markdown.contains("### Files Modified"));
        assert!(markdown.contains("### LLM Calls"));
    }

    #[test]
    fn test_generate_contains_recommendations() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("## Recommendations"));
        assert!(markdown.contains("**[completeness]**"));
    }

    #[test]
    fn test_generate_contains_footer() {
        let report = sample_report();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("---"));
        assert!(markdown.contains("*Generated by SMILE Loop at"));
    }

    #[test]
    fn test_format_duration_seconds_only() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(0), "0s");
    }

    #[test]
    fn test_format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(65), "1m 5s");
        assert_eq!(format_duration(120), "2m");
        assert_eq!(format_duration(332), "5m 32s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3661), "1h 1m 1s");
        assert_eq!(format_duration(7200), "2h");
    }

    #[test]
    fn test_escape_markdown() {
        assert_eq!(escape_markdown("normal text"), "normal text");
        assert_eq!(escape_markdown("*bold*"), "\\*bold\\*");
        assert_eq!(escape_markdown("_italic_"), "\\_italic\\_");
        assert_eq!(escape_markdown("[link]"), "\\[link\\]");
        assert_eq!(escape_markdown("line1\nline2"), "line1<br>line2");
    }

    #[test]
    fn test_escape_markdown_inline_code() {
        assert_eq!(escape_markdown_inline_code("normal"), "normal");
        assert_eq!(escape_markdown_inline_code("back`tick"), "back'tick");
    }

    #[test]
    fn test_truncate_output() {
        assert_eq!(truncate_output("short", 100), "short");
        assert_eq!(
            truncate_output("this is a very long line that should be truncated", 20),
            "this is a very long ..."
        );
        assert_eq!(
            truncate_output("first line\nsecond line", 100),
            "first line"
        );
    }

    #[test]
    fn test_truncate_output_unicode() {
        // Ensure multibyte UTF-8 characters don't cause panics
        // Each Chinese character is 3 bytes
        let chinese = "你好世界這是測試";
        let result = truncate_output(chinese, 10);
        // Should truncate at char boundary without panicking
        assert!(result.ends_with("..."));
        // Verify valid UTF-8 by iterating chars
        assert!(result.chars().count() > 0);

        // Test with emojis (4 bytes each)
        let emojis = "🚀🎉🔥🌟✨";
        let result = truncate_output(emojis, 10);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() > 0);
    }

    #[test]
    fn test_empty_report() {
        let report = Report::default();
        let generator = MarkdownGenerator::new(&report);
        let markdown = generator.generate();

        assert!(markdown.contains("*No documentation gaps identified.*"));
        assert!(markdown.contains("*No timeline events recorded.*"));
        assert!(markdown.contains("*No commands executed.*"));
        assert!(markdown.contains("*No files modified.*"));
        assert!(markdown.contains("*No LLM calls made.*"));
        assert!(markdown.contains("*No specific recommendations.*"));
    }

    #[test]
    fn test_gap_location_line_only() {
        let gap = Gap {
            id: 1,
            title: "Test".to_string(),
            location: GapLocation::at_line(42),
            problem: "Problem".to_string(),
            suggested_fix: "Fix".to_string(),
            severity: GapSeverity::Minor,
        };

        let parts = build_location_parts(&gap);
        assert_eq!(parts, "Line 42");
    }

    #[test]
    fn test_gap_location_quote_only() {
        let gap = Gap {
            id: 1,
            title: "Test".to_string(),
            location: GapLocation::with_quote("some code"),
            problem: "Problem".to_string(),
            suggested_fix: "Fix".to_string(),
            severity: GapSeverity::Minor,
        };

        let parts = build_location_parts(&gap);
        assert_eq!(parts, "`some code`");
    }

    #[test]
    fn test_gap_location_both() {
        let gap = Gap {
            id: 1,
            title: "Test".to_string(),
            location: GapLocation::at_line_with_quote(10, "code"),
            problem: "Problem".to_string(),
            suggested_fix: "Fix".to_string(),
            severity: GapSeverity::Minor,
        };

        let parts = build_location_parts(&gap);
        assert_eq!(parts, "Line 10 | `code`");
    }

    #[test]
    fn test_severity_icons() {
        assert_eq!(severity_icon_from_title("Critical Gaps"), "&#128308;");
        assert_eq!(severity_icon_from_title("Major Gaps"), "&#128992;");
        assert_eq!(severity_icon_from_title("Minor Gaps"), "&#128993;");
    }

    #[test]
    fn test_format_gap_count() {
        let critical = format_gap_count(3, "critical");
        assert!(critical.contains('3'));
        assert!(critical.contains("&#128308;"));

        let major = format_gap_count(2, "major");
        assert!(major.contains('2'));
        assert!(major.contains("&#128992;"));

        let minor = format_gap_count(1, "minor");
        assert!(minor.contains('1'));
        assert!(minor.contains("&#128993;"));
    }

    // ========================================================================
    // Full Structure Validation Tests
    // ========================================================================
    //
    // These tests verify the complete structure and content of generated
    // markdown reports by comparing against expected output strings.
    // Uses fixed timestamps for deterministic test results.

    /// Creates a report with fixed timestamps for deterministic tests.
    #[allow(clippy::too_many_lines)]
    fn deterministic_sample_report() -> Report {
        use chrono::TimeZone;

        // Use fixed timestamps for deterministic output
        let fixed_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap();

        Report {
            tutorial_name: "getting-started.md".to_string(),
            summary: ReportSummary {
                status: ReportStatus::Completed,
                iterations: 5,
                duration_seconds: 332,
                tutorial_path: "/tutorials/getting-started.md".to_string(),
            },
            gaps: vec![
                Gap {
                    id: 1,
                    title: "Missing package.json".to_string(),
                    location: GapLocation::at_line_with_quote(15, "Run npm install"),
                    problem:
                        "The tutorial instructs to run npm install but package.json is not provided"
                            .to_string(),
                    suggested_fix: "Add package.json contents before the install step".to_string(),
                    severity: GapSeverity::Critical,
                },
                Gap {
                    id: 2,
                    title: "Unclear environment variable".to_string(),
                    location: GapLocation::at_line(42),
                    problem: "DATABASE_URL is referenced but not explained".to_string(),
                    suggested_fix: "Add a section explaining required environment variables"
                        .to_string(),
                    severity: GapSeverity::Major,
                },
                Gap {
                    id: 3,
                    title: "Typo in command".to_string(),
                    location: GapLocation::with_quote("npm rn dev"),
                    problem: "Command has a typo".to_string(),
                    suggested_fix: "Change 'npm rn dev' to 'npm run dev'".to_string(),
                    severity: GapSeverity::Minor,
                },
            ],
            timeline: vec![
                TimelineEntry {
                    timestamp: fixed_time,
                    iteration: 1,
                    event: "Student started".to_string(),
                    details: Some("Beginning tutorial".to_string()),
                },
                TimelineEntry {
                    timestamp: fixed_time + chrono::Duration::seconds(60),
                    iteration: 2,
                    event: "Mentor consulted".to_string(),
                    details: None,
                },
            ],
            audit_trail: AuditTrail {
                commands: vec![
                    AuditCommand {
                        command: "npm install".to_string(),
                        exit_code: 1,
                        output: "npm ERR! missing package.json".to_string(),
                        timestamp: fixed_time + chrono::Duration::seconds(10),
                    },
                    AuditCommand {
                        command: "npm run dev".to_string(),
                        exit_code: 0,
                        output: "Server started on port 3000".to_string(),
                        timestamp: fixed_time + chrono::Duration::seconds(120),
                    },
                ],
                files: vec![
                    AuditFile {
                        path: "package.json".to_string(),
                        operation: crate::FileOperation::Created,
                        timestamp: fixed_time + chrono::Duration::seconds(30),
                    },
                    AuditFile {
                        path: "src/index.js".to_string(),
                        operation: crate::FileOperation::Modified,
                        timestamp: fixed_time + chrono::Duration::seconds(90),
                    },
                ],
                llm_calls: vec![
                    AuditLlmCall {
                        provider: "claude".to_string(),
                        prompt_tokens: 1500,
                        completion_tokens: 500,
                        duration_ms: 2500,
                        timestamp: fixed_time + chrono::Duration::seconds(5),
                    },
                    AuditLlmCall {
                        provider: "claude".to_string(),
                        prompt_tokens: 800,
                        completion_tokens: 200,
                        duration_ms: 1200,
                        timestamp: fixed_time + chrono::Duration::seconds(65),
                    },
                ],
            },
            recommendations: vec![
                Recommendation::new(
                    1,
                    "completeness",
                    "Add all prerequisite files to the tutorial",
                ),
                Recommendation::new(
                    2,
                    "clarity",
                    "Explain environment variables in a dedicated section",
                ),
            ],
        }
    }

    /// Generates markdown without the footer (which has dynamic timestamp).
    fn generate_without_footer(report: &Report) -> String {
        let generator = MarkdownGenerator::new(report);
        let mut output = String::new();

        generator.write_title(&mut output);
        generator.write_summary(&mut output);
        generator.write_gaps(&mut output);
        generator.write_timeline(&mut output);
        generator.write_audit_trail(&mut output);
        generator.write_recommendations(&mut output);

        output
    }

    /// Validates the full report structure by checking all expected sections and content.
    #[test]
    fn test_full_report_structure() {
        let report = deterministic_sample_report();
        let markdown = generate_without_footer(&report);

        // Verify title
        assert!(markdown.contains("# SMILE Validation Report: getting-started.md\n"));

        // Verify summary table structure and values
        assert!(markdown.contains("## Summary\n"));
        assert!(markdown.contains("| Metric | Value |"));
        assert!(markdown.contains("| Status | Tutorial completed successfully |"));
        assert!(markdown.contains("| Iterations | 5 |"));
        assert!(markdown.contains("| Duration | 5m 32s |"));
        assert!(markdown.contains("| Tutorial | /tutorials/getting-started.md |"));
        assert!(markdown.contains("| Gaps Found | 3 ("));

        // Verify gap sections exist with correct headers
        assert!(markdown.contains("## Documentation Gaps\n"));
        assert!(markdown.contains("### &#128308; Critical Gaps\n"));
        assert!(markdown.contains("### &#128992; Major Gaps\n"));
        assert!(markdown.contains("### &#128993; Minor Gaps\n"));

        // Verify all three gaps are present with correct details
        assert!(markdown.contains("#### Gap #1: Missing package.json\n"));
        assert!(markdown.contains("**Location**: Line 15 | `Run npm install`"));
        assert!(markdown
            .contains("**Problem**: The tutorial instructs to run npm install but package.json"));
        assert!(
            markdown.contains("**Suggested Fix**: Add package.json contents before the install")
        );

        assert!(markdown.contains("#### Gap #2: Unclear environment variable\n"));
        assert!(markdown.contains("**Location**: Line 42\n"));
        assert!(markdown.contains("**Problem**: DATABASE\\_URL is referenced but not explained"));

        assert!(markdown.contains("#### Gap #3: Typo in command\n"));
        assert!(markdown.contains("**Location**: `npm rn dev`"));
        assert!(markdown.contains("**Problem**: Command has a typo"));

        // Verify timeline
        assert!(markdown.contains("## Timeline\n"));
        assert!(markdown.contains("| Time | Iteration | Event | Details |"));
        assert!(markdown.contains("2026-01-15 10:30:00 UTC"));
        assert!(markdown.contains("#1 | Student started | Beginning tutorial"));
        assert!(markdown.contains("2026-01-15 10:31:00 UTC"));
        assert!(markdown.contains("#2 | Mentor consulted"));

        // Verify audit trail sections
        assert!(markdown.contains("## Audit Trail\n"));
        assert!(markdown.contains("### Commands Executed\n"));
        assert!(markdown.contains("| Time | Command | Exit | Output |"));
        assert!(markdown.contains("`npm install`"));
        assert!(markdown.contains("| 1 |"));
        assert!(markdown.contains("`npm run dev`"));
        assert!(markdown.contains("| 0 |"));

        assert!(markdown.contains("### Files Modified\n"));
        assert!(markdown.contains("| Time | Operation | Path |"));
        assert!(markdown.contains("| created | package.json |"));
        assert!(markdown.contains("| modified | src/index.js |"));

        assert!(markdown.contains("### LLM Calls\n"));
        assert!(markdown.contains("| Time | Provider | Tokens | Duration |"));
        assert!(markdown.contains("| claude | 1500+500 | 2500ms |"));
        assert!(markdown.contains("| claude | 800+200 | 1200ms |"));

        // Verify recommendations
        assert!(markdown.contains("## Recommendations\n"));
        assert!(markdown.contains("1. **[completeness]** Add all prerequisite files"));
        assert!(markdown.contains("2. **[clarity]** Explain environment variables"));
    }

    /// Validates empty report shows appropriate placeholder messages.
    #[test]
    fn test_empty_report_structure() {
        let report = Report::default();
        let markdown = generate_without_footer(&report);

        // Empty report should have placeholders for empty sections
        assert!(markdown.contains("*No documentation gaps identified.*"));
        assert!(markdown.contains("*No timeline events recorded.*"));
        assert!(markdown.contains("*No commands executed.*"));
        assert!(markdown.contains("*No files modified.*"));
        assert!(markdown.contains("*No LLM calls made.*"));
        assert!(markdown.contains("*No specific recommendations.*"));

        // Summary should show zeros
        assert!(markdown.contains("| Iterations | 0 |"));
        assert!(markdown.contains("| Duration | 0s |"));
        assert!(markdown.contains("Gaps Found | 0"));
    }

    /// Validates critical-only gap report structure.
    #[test]
    fn test_critical_gaps_only_structure() {
        use chrono::TimeZone;
        let fixed_time = Utc.with_ymd_and_hms(2026, 1, 15, 10, 30, 0).unwrap();

        let report = Report {
            tutorial_name: "broken-tutorial.md".to_string(),
            summary: ReportSummary {
                status: ReportStatus::Blocker,
                iterations: 1,
                duration_seconds: 45,
                tutorial_path: "/tutorials/broken-tutorial.md".to_string(),
            },
            gaps: vec![
                Gap {
                    id: 1,
                    title: "Missing Docker prerequisite".to_string(),
                    location: GapLocation::at_line(5),
                    problem: "Docker is not installed but the tutorial requires Docker commands"
                        .to_string(),
                    suggested_fix: "Add Docker installation instructions as a prerequisite"
                        .to_string(),
                    severity: GapSeverity::Critical,
                },
                Gap {
                    id: 2,
                    title: "Invalid configuration file".to_string(),
                    location: GapLocation::with_quote("config.yaml"),
                    problem: "The config.yaml file referenced does not exist".to_string(),
                    suggested_fix: "Provide the complete config.yaml file contents".to_string(),
                    severity: GapSeverity::Critical,
                },
            ],
            timeline: vec![TimelineEntry {
                timestamp: fixed_time,
                iteration: 1,
                event: "Blocker encountered".to_string(),
                details: Some("Cannot proceed without Docker".to_string()),
            }],
            audit_trail: AuditTrail::default(),
            recommendations: vec![Recommendation::new(
                1,
                "prerequisites",
                "Verify all prerequisites are clearly documented at the start of the tutorial",
            )],
        };

        let markdown = generate_without_footer(&report);

        // Should show blocker status
        assert!(markdown.contains("| Status | Unresolvable blocker encountered |"));
        assert!(markdown.contains("| Iterations | 1 |"));

        // Should have 2 critical gaps
        assert!(markdown.contains("Gaps Found | 2 (&#128308; 2 critical"));

        // Critical section should have both gaps
        assert!(markdown.contains("### &#128308; Critical Gaps\n"));
        assert!(markdown.contains("#### Gap #1: Missing Docker prerequisite"));
        assert!(markdown.contains("#### Gap #2: Invalid configuration file"));

        // Major and Minor sections should show "None"
        assert!(markdown.contains("### &#128992; Major Gaps\n\n*None*"));
        assert!(markdown.contains("### &#128993; Minor Gaps\n\n*None*"));

        // Timeline should have the blocker event
        assert!(markdown.contains("Blocker encountered"));
        assert!(markdown.contains("Cannot proceed without Docker"));

        // Empty audit trail sections
        assert!(markdown.contains("*No commands executed.*"));
        assert!(markdown.contains("*No files modified.*"));
        assert!(markdown.contains("*No LLM calls made.*"));

        // Recommendations
        assert!(markdown.contains("**[prerequisites]**"));
    }
}
