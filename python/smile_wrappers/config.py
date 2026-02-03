"""Configuration models for SMILE Loop.

This module defines the configuration schema for SMILE Loop execution,
including LLM provider selection, student behavior tuning, and runtime settings.
"""

from enum import Enum

from pydantic import BaseModel, Field


class LlmProvider(str, Enum):
    """Supported LLM providers for agent execution.

    Each provider corresponds to a CLI tool that must be available
    in the container environment.
    """

    CLAUDE = "claude"
    CODEX = "codex"
    GEMINI = "gemini"


class PatienceLevel(str, Enum):
    """Student patience level for handling difficulties.

    Determines how quickly the student escalates to the mentor
    when encountering problems.

    Attributes:
        LOW: Escalate quickly after minimal retry attempts.
        MEDIUM: Moderate retry attempts before escalating.
        HIGH: Maximum retry attempts before seeking help.
    """

    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"


class StudentBehavior(BaseModel):
    """Configuration for Student agent behavior and escalation rules.

    Controls when and how the Student agent asks for mentor assistance,
    including retry limits, timeout settings, and trigger conditions.

    Attributes:
        max_retries_before_help: Maximum attempts before asking mentor for help.
        ask_on_missing_dependency: Ask mentor when a dependency is missing.
        ask_on_ambiguous_instruction: Ask mentor when instructions are unclear.
        ask_on_command_failure: Ask mentor when a command fails unexpectedly.
        ask_on_timeout: Ask mentor when an operation times out.
        timeout_seconds: Timeout in seconds for individual operations.
        patience_level: Overall patience level affecting retry behavior.
    """

    max_retries_before_help: int = Field(default=3, ge=1)
    ask_on_missing_dependency: bool = True
    ask_on_ambiguous_instruction: bool = True
    ask_on_command_failure: bool = True
    ask_on_timeout: bool = True
    timeout_seconds: int = Field(default=60, ge=1)
    patience_level: PatienceLevel = PatienceLevel.LOW


class Config(BaseModel):
    """Main configuration for SMILE Loop execution.

    Defines all settings needed to run a SMILE validation session,
    including the tutorial path, LLM provider, resource limits,
    and student behavior configuration.

    Attributes:
        tutorial: Path to the tutorial markdown file to validate.
        llm_provider: The LLM provider to use for agent execution.
        max_iterations: Maximum number of student-mentor interaction cycles.
        timeout: Total timeout in seconds for the entire SMILE session.
        container_image: Docker image to use for isolated execution.
        student_behavior: Configuration for student agent behavior.
        state_file: Path to the state file for crash recovery.
        output_dir: Directory for output files and reports.
    """

    tutorial: str = "tutorial.md"
    llm_provider: LlmProvider = LlmProvider.CLAUDE
    max_iterations: int = Field(default=10, ge=1)
    timeout: int = Field(default=1800, ge=1)
    container_image: str = "smile-base:latest"
    student_behavior: StudentBehavior = Field(default_factory=StudentBehavior)
    state_file: str = ".smile/state.json"
    output_dir: str = "."
