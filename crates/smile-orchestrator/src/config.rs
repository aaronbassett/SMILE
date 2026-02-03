//! Configuration types for the SMILE Loop orchestrator.
//!
//! This module provides all configuration structures used to control
//! the behavior of the SMILE Loop, including LLM provider selection,
//! student behavior tuning, and container settings.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, SmileError};

/// The default config file name.
const CONFIG_FILE_NAME: &str = "smile.json";

/// Default tutorial file path.
fn default_tutorial() -> String {
    "tutorial.md".to_string()
}

/// Default maximum iterations before stopping the loop.
const fn default_max_iterations() -> u32 {
    10
}

/// Default timeout in seconds for the entire loop.
const fn default_timeout() -> u32 {
    1800
}

/// Default container image to use for agent execution.
fn default_container_image() -> String {
    "smile-base:latest".to_string()
}

/// Default state file path for crash recovery.
fn default_state_file() -> String {
    ".smile/state.json".to_string()
}

/// Default output directory for reports.
fn default_output_dir() -> String {
    ".".to_string()
}

/// Default maximum retries before asking for help.
const fn default_max_retries() -> u32 {
    3
}

/// Default step timeout in seconds.
const fn default_step_timeout() -> u32 {
    60
}

/// Default value for boolean options that default to true.
const fn default_true() -> bool {
    true
}

/// Main configuration for the SMILE Loop.
///
/// Controls all aspects of the validation loop including the tutorial
/// to validate, LLM provider, iteration limits, and container settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Path to the tutorial file to validate.
    #[serde(default = "default_tutorial")]
    pub tutorial: String,

    /// LLM provider to use for agent interactions.
    #[serde(default)]
    pub llm_provider: LlmProvider,

    /// Maximum number of Student-Mentor iterations before stopping.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Overall timeout for the entire loop in seconds.
    #[serde(default = "default_timeout")]
    pub timeout: u32,

    /// Docker image to use for running agents.
    #[serde(default = "default_container_image")]
    pub container_image: String,

    /// Configuration for student agent behavior.
    #[serde(default)]
    pub student_behavior: StudentBehavior,

    /// Container lifecycle configuration.
    #[serde(default)]
    pub container: ContainerConfig,

    /// Path to the state file for crash recovery.
    #[serde(default = "default_state_file")]
    pub state_file: String,

    /// Output directory for generated reports.
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tutorial: default_tutorial(),
            llm_provider: LlmProvider::default(),
            max_iterations: default_max_iterations(),
            timeout: default_timeout(),
            container_image: default_container_image(),
            student_behavior: StudentBehavior::default(),
            container: ContainerConfig::default(),
            state_file: default_state_file(),
            output_dir: default_output_dir(),
        }
    }
}

impl Config {
    /// Loads configuration from the current working directory.
    ///
    /// Looks for `smile.json` in the current directory. If found, loads and
    /// validates the configuration. If not found, returns default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but contains invalid JSON.
    pub fn load() -> Result<Self> {
        let current_dir = std::env::current_dir().map_err(|e| {
            SmileError::config_parse(
                "<current directory>",
                format!("cannot determine current directory: {e}"),
            )
        })?;
        Self::load_from_dir(&current_dir)
    }

    /// Loads configuration from a specific directory.
    ///
    /// Looks for `smile.json` in the given directory. If found, loads and
    /// validates the configuration. If not found, returns default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but contains invalid JSON.
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let config_path = dir.join(CONFIG_FILE_NAME);
        Self::load_from_file(&config_path)
    }

    /// Loads configuration from a specific file path.
    ///
    /// If the file does not exist, returns default configuration.
    /// If the file exists but contains invalid JSON, returns an error.
    ///
    /// # Errors
    ///
    /// Returns `SmileError::ConfigParseError` if the file exists but contains
    /// invalid JSON or invalid enum values.
    ///
    /// Returns `SmileError::ConfigValidationError` if the configuration values
    /// are invalid (e.g., zero iterations, empty paths).
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let config = Self::default();
                config.validate()?;
                return Ok(config);
            }
            Err(e) => {
                return Err(SmileError::config_parse(
                    path,
                    format!("failed to read file: {e}"),
                ));
            }
        };

        let config: Self = serde_json::from_str(&contents)
            .map_err(|e| SmileError::config_parse(path, e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration values.
    ///
    /// Checks that all required fields have valid values:
    /// - `max_iterations` must be greater than 0
    /// - `timeout` must be greater than 0
    /// - `student_behavior.timeout_seconds` must be greater than 0
    /// - `student_behavior.max_retries_before_help` must be greater than 0
    /// - `tutorial` path must not be empty
    /// - `output_dir` must not be empty
    ///
    /// # Errors
    ///
    /// Returns `SmileError::ConfigValidationError` if any validation check fails.
    pub fn validate(&self) -> Result<()> {
        if self.max_iterations == 0 {
            return Err(SmileError::config_validation(
                "maxIterations must be greater than 0",
                "Set maxIterations to at least 1 in your smile.json",
            ));
        }

        if self.timeout == 0 {
            return Err(SmileError::config_validation(
                "timeout must be greater than 0",
                "Set timeout to at least 1 second in your smile.json",
            ));
        }

        if self.student_behavior.timeout_seconds == 0 {
            return Err(SmileError::config_validation(
                "studentBehavior.timeoutSeconds must be greater than 0",
                "Set studentBehavior.timeoutSeconds to at least 1 second in your smile.json",
            ));
        }

        if self.student_behavior.max_retries_before_help == 0 {
            return Err(SmileError::config_validation(
                "studentBehavior.maxRetriesBeforeHelp must be greater than 0",
                "Set studentBehavior.maxRetriesBeforeHelp to at least 1 in your smile.json",
            ));
        }

        if self.tutorial.trim().is_empty() {
            return Err(SmileError::config_validation(
                "tutorial path must not be empty",
                "Provide a valid tutorial file path in your smile.json",
            ));
        }

        if self.output_dir.trim().is_empty() {
            return Err(SmileError::config_validation(
                "outputDir must not be empty",
                "Provide a valid output directory path in your smile.json (use '.' for current directory)",
            ));
        }

        Ok(())
    }
}

/// Supported LLM providers for agent interactions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LlmProvider {
    /// Anthropic Claude (default).
    #[default]
    Claude,
    /// `OpenAI` `Codex`.
    Codex,
    /// Google Gemini.
    Gemini,
}

impl LlmProvider {
    /// Parses a string into an `LlmProvider`, case-insensitively.
    fn from_str_case_insensitive(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for LlmProvider {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str_case_insensitive(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid LLM provider '{s}': expected one of 'claude', 'codex', 'gemini'"
            ))
        })
    }
}

impl Serialize for LlmProvider {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
        };
        serializer.serialize_str(s)
    }
}

/// Configuration for student agent behavior.
///
/// Controls how the student agent responds to various situations
/// and when it should escalate to the mentor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct StudentBehavior {
    /// Maximum retries before asking the mentor for help.
    #[serde(default = "default_max_retries")]
    pub max_retries_before_help: u32,

    /// Whether to ask for help when a dependency is missing.
    #[serde(default = "default_true")]
    pub ask_on_missing_dependency: bool,

    /// Whether to ask for help when an instruction is ambiguous.
    #[serde(default = "default_true")]
    pub ask_on_ambiguous_instruction: bool,

    /// Whether to ask for help when a command fails.
    #[serde(default = "default_true")]
    pub ask_on_command_failure: bool,

    /// Whether to ask for help when a step times out.
    #[serde(default = "default_true")]
    pub ask_on_timeout: bool,

    /// Timeout for individual steps in seconds.
    #[serde(default = "default_step_timeout")]
    pub timeout_seconds: u32,

    /// How patient the student should be before escalating.
    #[serde(default)]
    pub patience_level: PatienceLevel,
}

impl Default for StudentBehavior {
    fn default() -> Self {
        Self {
            max_retries_before_help: default_max_retries(),
            ask_on_missing_dependency: default_true(),
            ask_on_ambiguous_instruction: default_true(),
            ask_on_command_failure: default_true(),
            ask_on_timeout: default_true(),
            timeout_seconds: default_step_timeout(),
            patience_level: PatienceLevel::default(),
        }
    }
}

/// Patience level for the student agent.
///
/// Determines how quickly the student escalates issues to the mentor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PatienceLevel {
    /// Low patience - escalates quickly (default).
    #[default]
    Low,
    /// Medium patience - moderate tolerance for issues.
    Medium,
    /// High patience - tries harder before escalating.
    High,
}

impl PatienceLevel {
    /// Parses a string into a `PatienceLevel`, case-insensitively.
    fn from_str_case_insensitive(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for PatienceLevel {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str_case_insensitive(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid patience level '{s}': expected one of 'low', 'medium', 'high'"
            ))
        })
    }
}

impl Serialize for PatienceLevel {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        };
        serializer.serialize_str(s)
    }
}

/// Container lifecycle configuration.
///
/// Controls when containers are kept or removed after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConfig {
    /// Keep the container if the loop fails (for debugging).
    #[serde(default = "default_true")]
    pub keep_on_failure: bool,

    /// Keep the container if the loop succeeds.
    #[serde(default)]
    pub keep_on_success: bool,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            keep_on_failure: default_true(),
            keep_on_success: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();

        assert_eq!(config.tutorial, "tutorial.md");
        assert_eq!(config.llm_provider, LlmProvider::Claude);
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.timeout, 1800);
        assert_eq!(config.container_image, "smile-base:latest");
        assert_eq!(config.state_file, ".smile/state.json");
        assert_eq!(config.output_dir, ".");
    }

    #[test]
    fn test_student_behavior_default_values() {
        let behavior = StudentBehavior::default();

        assert_eq!(behavior.max_retries_before_help, 3);
        assert!(behavior.ask_on_missing_dependency);
        assert!(behavior.ask_on_ambiguous_instruction);
        assert!(behavior.ask_on_command_failure);
        assert!(behavior.ask_on_timeout);
        assert_eq!(behavior.timeout_seconds, 60);
        assert_eq!(behavior.patience_level, PatienceLevel::Low);
    }

    #[test]
    fn test_container_config_default_values() {
        let container = ContainerConfig::default();

        assert!(container.keep_on_failure);
        assert!(!container.keep_on_success);
    }

    #[test]
    fn test_llm_provider_serialization() {
        assert_eq!(
            serde_json::to_string(&LlmProvider::Claude).unwrap(),
            "\"claude\""
        );
        assert_eq!(
            serde_json::to_string(&LlmProvider::Codex).unwrap(),
            "\"codex\""
        );
        assert_eq!(
            serde_json::to_string(&LlmProvider::Gemini).unwrap(),
            "\"gemini\""
        );
    }

    #[test]
    fn test_patience_level_serialization() {
        assert_eq!(
            serde_json::to_string(&PatienceLevel::Low).unwrap(),
            "\"low\""
        );
        assert_eq!(
            serde_json::to_string(&PatienceLevel::Medium).unwrap(),
            "\"medium\""
        );
        assert_eq!(
            serde_json::to_string(&PatienceLevel::High).unwrap(),
            "\"high\""
        );
    }

    #[test]
    fn test_config_deserialization_with_defaults() {
        let json = r"{}";
        let config: Config = serde_json::from_str(json).unwrap();

        assert_eq!(config.tutorial, "tutorial.md");
        assert_eq!(config.max_iterations, 10);
    }

    #[test]
    fn test_config_deserialization_with_overrides() {
        let json = r#"{
            "tutorial": "custom.md",
            "llmProvider": "gemini",
            "maxIterations": 20,
            "studentBehavior": {
                "patienceLevel": "high",
                "maxRetriesBeforeHelp": 5
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();

        assert_eq!(config.tutorial, "custom.md");
        assert_eq!(config.llm_provider, LlmProvider::Gemini);
        assert_eq!(config.max_iterations, 20);
        assert_eq!(config.student_behavior.patience_level, PatienceLevel::High);
        assert_eq!(config.student_behavior.max_retries_before_help, 5);
        // Check that other fields got their defaults
        assert!(config.student_behavior.ask_on_missing_dependency);
    }

    #[test]
    fn test_llm_provider_case_insensitive() {
        // Test lowercase
        let config: Config = serde_json::from_str(r#"{"llmProvider": "claude"}"#).unwrap();
        assert_eq!(config.llm_provider, LlmProvider::Claude);

        // Test uppercase
        let config: Config = serde_json::from_str(r#"{"llmProvider": "CLAUDE"}"#).unwrap();
        assert_eq!(config.llm_provider, LlmProvider::Claude);

        // Test mixed case
        let config: Config = serde_json::from_str(r#"{"llmProvider": "Claude"}"#).unwrap();
        assert_eq!(config.llm_provider, LlmProvider::Claude);

        let config: Config = serde_json::from_str(r#"{"llmProvider": "GeMiNi"}"#).unwrap();
        assert_eq!(config.llm_provider, LlmProvider::Gemini);

        let config: Config = serde_json::from_str(r#"{"llmProvider": "CODEX"}"#).unwrap();
        assert_eq!(config.llm_provider, LlmProvider::Codex);
    }

    #[test]
    fn test_patience_level_case_insensitive() {
        // Test lowercase
        let json = r#"{"studentBehavior": {"patienceLevel": "low"}}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.student_behavior.patience_level, PatienceLevel::Low);

        // Test uppercase
        let json = r#"{"studentBehavior": {"patienceLevel": "HIGH"}}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.student_behavior.patience_level, PatienceLevel::High);

        // Test mixed case
        let json = r#"{"studentBehavior": {"patienceLevel": "Medium"}}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.student_behavior.patience_level,
            PatienceLevel::Medium
        );
    }

    #[test]
    fn test_invalid_llm_provider_error() {
        let json = r#"{"llmProvider": "gpt4"}"#;
        let result: std::result::Result<Config, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid LLM provider"));
        assert!(err.contains("gpt4"));
    }

    #[test]
    fn test_invalid_patience_level_error() {
        let json = r#"{"studentBehavior": {"patienceLevel": "extreme"}}"#;
        let result: std::result::Result<Config, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid patience level"));
        assert!(err.contains("extreme"));
    }

    #[test]
    fn test_load_from_file_valid_json() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_smile_valid.json");

        // Write a valid config file
        let json = r#"{
            "tutorial": "test.md",
            "llmProvider": "Gemini",
            "maxIterations": 5
        }"#;
        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        // Load and verify
        let config = Config::load_from_file(&config_path).unwrap();
        assert_eq!(config.tutorial, "test.md");
        assert_eq!(config.llm_provider, LlmProvider::Gemini);
        assert_eq!(config.max_iterations, 5);
        // Default values should be applied for missing fields
        assert_eq!(config.timeout, 1800);

        // Cleanup
        std::fs::remove_file(&config_path).ok();
    }

    #[test]
    fn test_load_from_file_invalid_json() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_smile_invalid.json");

        // Write invalid JSON
        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(b"{ not valid json }").unwrap();

        // Load should fail with ConfigParseError
        let result = Config::load_from_file(&config_path);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigParseError { path, message } if *path == config_path && !message.is_empty()),
            "Expected ConfigParseError with correct path, got: {err:?}"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
    }

    #[test]
    fn test_load_from_file_nonexistent_returns_default() {
        let nonexistent_path = PathBuf::from("/nonexistent/path/smile.json");
        let config = Config::load_from_file(&nonexistent_path).unwrap();

        // Should return default config
        assert_eq!(config.tutorial, "tutorial.md");
        assert_eq!(config.llm_provider, LlmProvider::Claude);
        assert_eq!(config.max_iterations, 10);
    }

    #[test]
    fn test_load_from_dir_finds_smile_json() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir().join("test_smile_dir");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let config_path = temp_dir.join("smile.json");
        let json = r#"{"tutorial": "dir_test.md"}"#;
        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        // Load from directory
        let config = Config::load_from_dir(&temp_dir).unwrap();
        assert_eq!(config.tutorial, "dir_test.md");

        // Cleanup
        std::fs::remove_file(&config_path).ok();
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_load_from_dir_no_config_returns_default() {
        let temp_dir = std::env::temp_dir().join("test_smile_empty_dir");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Directory exists but no smile.json
        let config = Config::load_from_dir(&temp_dir).unwrap();
        assert_eq!(config.tutorial, "tutorial.md");
        assert_eq!(config.llm_provider, LlmProvider::Claude);

        // Cleanup
        std::fs::remove_dir(&temp_dir).ok();
    }

    #[test]
    fn test_unknown_fields_ignored() {
        // Unknown fields at root level should be silently ignored (forward compatibility)
        let json = r#"{
            "tutorial": "test.md",
            "unknownField": "should be ignored",
            "anotherUnknown": 123
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.tutorial, "test.md");
    }

    #[test]
    fn test_config_validation_zero_max_iterations() {
        let config = Config {
            max_iterations: 0,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { message, suggestion }
                if message.contains("maxIterations") && suggestion.contains("maxIterations")),
            "Expected ConfigValidationError about maxIterations, got: {err:?}"
        );
    }

    #[test]
    fn test_config_validation_zero_timeout() {
        let config = Config {
            timeout: 0,
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { message, suggestion }
                if message.contains("timeout") && suggestion.contains("timeout")),
            "Expected ConfigValidationError about timeout, got: {err:?}"
        );
    }

    #[test]
    fn test_config_validation_zero_student_timeout_seconds() {
        let config = Config {
            student_behavior: StudentBehavior {
                timeout_seconds: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { message, suggestion }
                if message.contains("timeoutSeconds") && suggestion.contains("timeoutSeconds")),
            "Expected ConfigValidationError about timeoutSeconds, got: {err:?}"
        );
    }

    #[test]
    fn test_config_validation_zero_max_retries() {
        let config = Config {
            student_behavior: StudentBehavior {
                max_retries_before_help: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { message, suggestion }
                if message.contains("maxRetriesBeforeHelp") && suggestion.contains("maxRetriesBeforeHelp")),
            "Expected ConfigValidationError about maxRetriesBeforeHelp, got: {err:?}"
        );
    }

    #[test]
    fn test_config_validation_empty_tutorial() {
        let config = Config {
            tutorial: String::new(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { message, suggestion }
                if message.contains("tutorial") && suggestion.contains("tutorial")),
            "Expected ConfigValidationError about tutorial path, got: {err:?}"
        );

        // Also test whitespace-only tutorial path
        let config_whitespace = Config {
            tutorial: "   ".to_string(),
            ..Default::default()
        };
        let result = config_whitespace.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_empty_output_dir() {
        let config = Config {
            output_dir: String::new(),
            ..Default::default()
        };

        let result = config.validate();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { message, suggestion }
                if message.contains("outputDir") && suggestion.contains("output")),
            "Expected ConfigValidationError about outputDir, got: {err:?}"
        );

        // Also test whitespace-only output_dir
        let config_whitespace = Config {
            output_dir: "   ".to_string(),
            ..Default::default()
        };
        let result = config_whitespace.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_valid_config_passes() {
        let config = Config::default();
        let result = config.validate();
        assert!(result.is_ok(), "Default config should pass validation");

        // Test a fully customized valid config
        let custom_config = Config {
            tutorial: "my-tutorial.md".to_string(),
            llm_provider: LlmProvider::Gemini,
            max_iterations: 5,
            timeout: 600,
            container_image: "custom-image:v1".to_string(),
            student_behavior: StudentBehavior {
                max_retries_before_help: 2,
                ask_on_missing_dependency: false,
                ask_on_ambiguous_instruction: true,
                ask_on_command_failure: true,
                ask_on_timeout: false,
                timeout_seconds: 30,
                patience_level: PatienceLevel::High,
            },
            container: ContainerConfig::default(),
            state_file: ".custom/state.json".to_string(),
            output_dir: "/tmp/output".to_string(),
        };
        let result = custom_config.validate();
        assert!(result.is_ok(), "Custom valid config should pass validation");
    }

    #[test]
    fn test_load_from_file_validates_after_parsing() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join("test_smile_validation.json");

        // Write a syntactically valid config with invalid values
        let json = r#"{
            "tutorial": "test.md",
            "maxIterations": 0
        }"#;
        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        // Load should fail with validation error, not parse error
        let result = Config::load_from_file(&config_path);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            matches!(&err, SmileError::ConfigValidationError { .. }),
            "Expected ConfigValidationError, got: {err:?}"
        );

        // Cleanup
        std::fs::remove_file(&config_path).ok();
    }
}
