//! JSON report generation for SMILE Loop.
//!
//! This module provides [`JsonGenerator`] for serializing SMILE reports to JSON format.
//! Reports can be generated as compact single-line JSON or pretty-printed for human readability.
//!
//! # Example
//!
//! ```rust
//! use smile_report::{Report, ReportSummary, ReportStatus};
//! use smile_report::json::JsonGenerator;
//! use std::path::Path;
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
//! let generator = JsonGenerator::new(&report);
//!
//! // Generate compact JSON
//! let compact = generator.generate().unwrap();
//!
//! // Generate pretty-printed JSON
//! let pretty = generator.generate_pretty().unwrap();
//!
//! // Write to file
//! // generator.write_to_file(Path::new("smile-report.json"), true).unwrap();
//! ```

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::{Report, ReportError, Result};

/// JSON report generator.
///
/// Wraps a [`Report`] reference and provides methods for serializing it to JSON
/// in various formats.
///
/// # Example
///
/// ```rust
/// use smile_report::{Report, json::JsonGenerator};
///
/// let report = Report::default();
/// let generator = JsonGenerator::new(&report);
///
/// let json = generator.generate_pretty().unwrap();
/// assert!(json.contains("tutorial_name"));
/// ```
pub struct JsonGenerator<'a> {
    report: &'a Report,
}

impl<'a> JsonGenerator<'a> {
    /// Creates a new JSON generator for the given report.
    ///
    /// # Arguments
    ///
    /// * `report` - Reference to the report to serialize.
    ///
    /// # Example
    ///
    /// ```rust
    /// use smile_report::{Report, json::JsonGenerator};
    ///
    /// let report = Report::default();
    /// let generator = JsonGenerator::new(&report);
    /// ```
    #[must_use]
    pub const fn new(report: &'a Report) -> Self {
        Self { report }
    }

    /// Generates compact JSON output (single line, no extra whitespace).
    ///
    /// This format is optimal for programmatic consumption and data transfer
    /// where human readability is not required.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError::Serialization`] if JSON serialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use smile_report::{Report, json::JsonGenerator};
    ///
    /// let report = Report::default();
    /// let generator = JsonGenerator::new(&report);
    /// let json = generator.generate().unwrap();
    ///
    /// // Compact JSON has no newlines
    /// assert!(!json.contains('\n'));
    /// ```
    pub fn generate(&self) -> Result<String> {
        serde_json::to_string(self.report).map_err(ReportError::from)
    }

    /// Generates pretty-printed JSON output with indentation.
    ///
    /// This format is optimal for human readability and debugging.
    /// Uses 2-space indentation as per `serde_json` defaults.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError::Serialization`] if JSON serialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use smile_report::{Report, json::JsonGenerator};
    ///
    /// let report = Report::default();
    /// let generator = JsonGenerator::new(&report);
    /// let json = generator.generate_pretty().unwrap();
    ///
    /// // Pretty JSON has newlines and indentation
    /// assert!(json.contains('\n'));
    /// assert!(json.contains("  "));
    /// ```
    pub fn generate_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self.report).map_err(ReportError::from)
    }

    /// Writes the JSON report directly to a file.
    ///
    /// This method creates or overwrites the file at the specified path.
    /// Parent directories must exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the output file (e.g., `smile-report.json`).
    /// * `pretty` - If `true`, write pretty-printed JSON; otherwise, write compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError::Serialization`] if JSON serialization fails.
    /// Returns [`ReportError::Io`] if file creation or writing fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use smile_report::{Report, json::JsonGenerator};
    /// use std::path::Path;
    ///
    /// let report = Report::default();
    /// let generator = JsonGenerator::new(&report);
    ///
    /// // Write pretty-printed JSON
    /// generator.write_to_file(Path::new("smile-report.json"), true).unwrap();
    ///
    /// // Write compact JSON
    /// generator.write_to_file(Path::new("smile-report.min.json"), false).unwrap();
    /// ```
    pub fn write_to_file(&self, path: &Path, pretty: bool) -> Result<()> {
        let json = if pretty {
            self.generate_pretty()?
        } else {
            self.generate()?
        };

        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{
        AuditTrail, Gap, GapLocation, GapSeverity, Recommendation, ReportStatus, ReportSummary,
        TimelineEntry,
    };
    use std::io::Read;

    /// Creates a sample report for testing.
    fn sample_report() -> Report {
        Report {
            tutorial_name: "Build a REST API".to_string(),
            summary: ReportSummary {
                status: ReportStatus::Completed,
                iterations: 3,
                duration_seconds: 120,
                tutorial_path: "tutorial.md".to_string(),
            },
            gaps: vec![Gap {
                id: 1,
                title: "Missing dependency installation".to_string(),
                location: GapLocation::at_line_with_quote(15, "Run npm install"),
                problem: "Package name not specified".to_string(),
                suggested_fix: "Add: npm install express".to_string(),
                severity: GapSeverity::Major,
            }],
            timeline: vec![TimelineEntry::new(1, "Started tutorial validation")],
            audit_trail: AuditTrail::default(),
            recommendations: vec![Recommendation::new(
                1,
                "clarity",
                "Specify exact package versions",
            )],
        }
    }

    #[test]
    fn test_generate_compact_json() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate().unwrap();

        // Compact JSON should not have newlines
        assert!(!json.contains('\n'));

        // Should contain expected fields
        assert!(json.contains(r#""tutorial_name":"Build a REST API""#));
        assert!(json.contains(r#""status":"completed""#));
        assert!(json.contains(r#""iterations":3"#));
    }

    #[test]
    fn test_generate_pretty_json() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate_pretty().unwrap();

        // Pretty JSON should have newlines and indentation
        assert!(json.contains('\n'));
        assert!(json.contains("  "));

        // Should contain expected fields
        assert!(json.contains("\"tutorial_name\""));
        assert!(json.contains("\"Build a REST API\""));
        assert!(json.contains("\"completed\""));
    }

    #[test]
    fn test_json_contains_all_top_level_fields() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate_pretty().unwrap();

        // Verify all top-level fields are present
        assert!(json.contains("\"tutorial_name\""));
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"gaps\""));
        assert!(json.contains("\"timeline\""));
        assert!(json.contains("\"audit_trail\""));
        assert!(json.contains("\"recommendations\""));
    }

    #[test]
    fn test_json_contains_summary_fields() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate_pretty().unwrap();

        // Verify summary fields
        assert!(json.contains("\"status\""));
        assert!(json.contains("\"iterations\""));
        assert!(json.contains("\"duration_seconds\""));
        assert!(json.contains("\"tutorial_path\""));
    }

    #[test]
    fn test_json_contains_gap_fields() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate_pretty().unwrap();

        // Verify gap fields
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"title\""));
        assert!(json.contains("\"location\""));
        assert!(json.contains("\"problem\""));
        assert!(json.contains("\"suggested_fix\""));
        assert!(json.contains("\"severity\""));

        // Verify location subfields
        assert!(json.contains("\"line_number\""));
        assert!(json.contains("\"quote\""));
    }

    #[test]
    fn test_json_roundtrip() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate().unwrap();
        let parsed: Report = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tutorial_name, report.tutorial_name);
        assert_eq!(parsed.summary.status, report.summary.status);
        assert_eq!(parsed.summary.iterations, report.summary.iterations);
        assert_eq!(parsed.gaps.len(), report.gaps.len());
        assert_eq!(parsed.gaps[0].title, report.gaps[0].title);
        assert_eq!(parsed.recommendations.len(), report.recommendations.len());
    }

    #[test]
    fn test_json_pretty_roundtrip() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate_pretty().unwrap();
        let parsed: Report = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tutorial_name, report.tutorial_name);
        assert_eq!(parsed.summary.status, report.summary.status);
    }

    #[test]
    fn test_empty_report_serialization() {
        let report = Report::default();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate().unwrap();

        // Should still be valid JSON
        let parsed: Report = serde_json::from_str(&json).unwrap();
        assert!(parsed.tutorial_name.is_empty());
        assert!(parsed.gaps.is_empty());
    }

    #[test]
    fn test_write_to_file() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("smile-test-report.json");

        // Write pretty JSON
        generator.write_to_file(&file_path, true).unwrap();

        // Read and verify
        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        assert!(contents.contains('\n'));
        assert!(contents.contains("\"tutorial_name\""));
        assert!(contents.contains("\"Build a REST API\""));

        // Clean up
        std::fs::remove_file(&file_path).unwrap();
    }

    #[test]
    fn test_write_to_file_compact() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("smile-test-report-compact.json");

        // Write compact JSON
        generator.write_to_file(&file_path, false).unwrap();

        // Read and verify
        let mut file = File::open(&file_path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        assert!(!contents.contains('\n'));
        assert!(contents.contains("\"tutorial_name\""));

        // Clean up
        std::fs::remove_file(&file_path).unwrap();
    }

    #[test]
    fn test_write_to_file_invalid_path() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        // Try to write to a non-existent directory
        let result = generator.write_to_file(Path::new("/nonexistent/dir/report.json"), true);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ReportError::Io(_)));
    }

    #[test]
    fn test_severity_serializes_as_snake_case() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate().unwrap();

        // GapSeverity should serialize as snake_case
        assert!(json.contains(r#""severity":"major""#));
    }

    #[test]
    fn test_status_serializes_as_snake_case() {
        let report = sample_report();
        let generator = JsonGenerator::new(&report);

        let json = generator.generate().unwrap();

        // ReportStatus should serialize as snake_case
        assert!(json.contains(r#""status":"completed""#));
    }

    #[test]
    fn test_multiple_gaps_serialization() {
        let mut report = sample_report();
        report.gaps.push(Gap {
            id: 2,
            title: "Unclear instructions".to_string(),
            location: GapLocation::at_line(42),
            problem: "Step 5 is ambiguous".to_string(),
            suggested_fix: "Add code example".to_string(),
            severity: GapSeverity::Minor,
        });
        report.gaps.push(Gap {
            id: 3,
            title: "Missing prerequisite".to_string(),
            location: GapLocation::default(),
            problem: "Requires Node.js but not mentioned".to_string(),
            suggested_fix: "Add prerequisites section".to_string(),
            severity: GapSeverity::Critical,
        });

        let generator = JsonGenerator::new(&report);
        let json = generator.generate().unwrap();

        let parsed: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.gaps.len(), 3);
        assert_eq!(parsed.gaps[1].severity, GapSeverity::Minor);
        assert_eq!(parsed.gaps[2].severity, GapSeverity::Critical);
    }
}
