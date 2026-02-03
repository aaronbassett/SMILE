//! Configuration types for the SMILE Loop orchestrator.
//!
//! This module provides all configuration structures used to control
//! the behavior of the SMILE Loop, including LLM provider selection,
//! student behavior tuning, and container settings.

use serde::{Deserialize, Serialize};

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

/// Supported LLM providers for agent interactions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// Anthropic Claude (default).
    #[default]
    Claude,
    /// `OpenAI` `Codex`.
    Codex,
    /// Google Gemini.
    Gemini,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatienceLevel {
    /// Low patience - escalates quickly (default).
    #[default]
    Low,
    /// Medium patience - moderate tolerance for issues.
    Medium,
    /// High patience - tries harder before escalating.
    High,
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
}
