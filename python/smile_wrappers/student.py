"""Student agent wrapper for SMILE Loop.

This module implements the Student agent that follows tutorial instructions
step-by-step, invoking an LLM CLI to simulate a constrained learner.
When the student encounters difficulties, it escalates to the Mentor agent.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from enum import Enum
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import ValidationError

from smile_wrappers.config import Config, LlmProvider, StudentBehavior
from smile_wrappers.output import StudentOutput
from smile_wrappers.prompts import build_student_prompt

if TYPE_CHECKING:
    from collections.abc import Sequence

__all__ = [
    "LlmCli",
    "LlmCliError",
    "LlmParseError",
    "LlmTimeoutError",
    "StuckCondition",
    "StuckDetector",
    "StudentWrapper",
    "main",
]

# Default paths for container execution
DEFAULT_WORKSPACE_DIR = "/workspace"
DEFAULT_TUTORIAL_DIR = "/workspace/tutorial"
DEFAULT_CONFIG_FILE = "/workspace/config.json"
DEFAULT_MENTOR_NOTES_FILE = "/workspace/mentor_notes.json"

# Maximum retry attempts for JSON parsing
MAX_PARSE_RETRIES = 3


class StuckCondition(Enum):
    """Classification of why the student got stuck.

    Each condition represents a specific type of difficulty that the student
    encountered, helping the orchestrator and mentor understand the nature
    of the problem.

    Attributes:
        TIMEOUT: The operation exceeded the configured timeout duration.
        MAX_RETRIES: Maximum retry attempts reached for the same step.
        MISSING_DEPENDENCY: A required dependency (package, tool, etc.) is missing.
        AMBIGUOUS_INSTRUCTION: The tutorial instruction is unclear or ambiguous.
        COMMAND_FAILURE: A shell command failed with a non-zero exit code.
        PARSE_FAILURE: Unable to parse LLM output as valid JSON.
        CANNOT_COMPLETE: The step cannot be completed for other reasons.
    """

    TIMEOUT = "timeout"
    MAX_RETRIES = "max_retries"
    MISSING_DEPENDENCY = "missing_dependency"
    AMBIGUOUS_INSTRUCTION = "ambiguous_instruction"
    COMMAND_FAILURE = "command_failure"
    PARSE_FAILURE = "parse_failure"
    CANNOT_COMPLETE = "cannot_complete"


# Patterns for detecting missing dependencies
_MISSING_DEPENDENCY_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"command not found", re.IGNORECASE),
    re.compile(r"ModuleNotFoundError", re.IGNORECASE),
    re.compile(r"No module named", re.IGNORECASE),
    re.compile(r"not installed", re.IGNORECASE),
    re.compile(r"package .* not found", re.IGNORECASE),
    re.compile(r"Cannot find module", re.IGNORECASE),
    re.compile(r"ImportError", re.IGNORECASE),
    re.compile(r"error: externally-managed-environment", re.IGNORECASE),
    re.compile(r"pip install", re.IGNORECASE),
    re.compile(r"npm install", re.IGNORECASE),
    re.compile(r"cargo install", re.IGNORECASE),
    re.compile(r"apt install", re.IGNORECASE),
    re.compile(r"brew install", re.IGNORECASE),
]

# Patterns for detecting ambiguous instructions
_AMBIGUOUS_INSTRUCTION_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"\bunclear\b", re.IGNORECASE),
    re.compile(r"\bambiguous\b", re.IGNORECASE),
    re.compile(r"don'?t understand", re.IGNORECASE),
    re.compile(r"not sure (what|which|how)", re.IGNORECASE),
    re.compile(r"which (one|version|option)", re.IGNORECASE),
    re.compile(r"what (do you mean|does .* mean)", re.IGNORECASE),
    re.compile(r"could you clarify", re.IGNORECASE),
    re.compile(r"please clarify", re.IGNORECASE),
    re.compile(r"need more (information|details|context)", re.IGNORECASE),
    re.compile(r"instructions? (are|is) (unclear|confusing)", re.IGNORECASE),
    re.compile(r"multiple (ways|options|interpretations)", re.IGNORECASE),
]

# Patterns for detecting command failures
_COMMAND_FAILURE_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"^error:", re.IGNORECASE | re.MULTILINE),
    re.compile(r"^failed:", re.IGNORECASE | re.MULTILINE),
    re.compile(r"command failed", re.IGNORECASE),
    re.compile(r"exit (code|status) [1-9]", re.IGNORECASE),
    re.compile(r"non-zero exit", re.IGNORECASE),
    re.compile(r"permission denied", re.IGNORECASE),
    re.compile(r"access denied", re.IGNORECASE),
    re.compile(r"operation not permitted", re.IGNORECASE),
    re.compile(r"fatal error", re.IGNORECASE),
    re.compile(r"compilation failed", re.IGNORECASE),
    re.compile(r"build failed", re.IGNORECASE),
]


class StuckDetector:
    """Detector for classifying why the student got stuck.

    Tracks retry counts per step and analyzes error messages and output
    to classify stuck conditions. Used by `StudentWrapper` to determine
    when and why to escalate to the mentor.

    Attributes:
        config: Student behavior configuration controlling escalation rules.
        stuck_condition: The most recently detected stuck condition, if any.

    Example:
        >>> detector = StuckDetector(config=StudentBehavior())
        >>> detector.record_attempt("Install dependencies", success=False)
        False
        >>> detector.record_attempt("Install dependencies", success=False)
        False
        >>> detector.record_attempt("Install dependencies", success=False)
        True  # max_retries reached
        >>> detector.stuck_condition
        <StuckCondition.MAX_RETRIES: 'max_retries'>
    """

    def __init__(self, config: StudentBehavior) -> None:
        """Initialize the StuckDetector.

        Args:
            config: Student behavior configuration controlling escalation rules.
        """
        self.config = config
        self.stuck_condition: StuckCondition | None = None
        self._retry_counts: dict[str, int] = defaultdict(int)

    def record_attempt(self, step: str, *, success: bool) -> bool:
        """Record an attempt at a step and check if max retries reached.

        Tracks the number of failed attempts for each step. When the
        maximum retry count is reached, sets `stuck_condition` to
        `MAX_RETRIES` and returns True.

        Args:
            step: Identifier or description of the current step.
            success: Whether the attempt succeeded.

        Returns:
            True if max retries have been reached, False otherwise.
        """
        if success:
            # Reset counter on success
            self._retry_counts[step] = 0
            return False

        self._retry_counts[step] += 1

        if self._retry_counts[step] >= self.config.max_retries_before_help:
            self.stuck_condition = StuckCondition.MAX_RETRIES
            return True

        return False

    def classify_output(self, output: StudentOutput) -> StuckCondition | None:
        """Classify stuck condition from a StudentOutput.

        Analyzes the output status, problem description, and other fields
        to determine if and why the student is stuck.

        Args:
            output: The StudentOutput to analyze.

        Returns:
            The detected StuckCondition, or None if not stuck.
        """
        # Check status first
        if output.status == "completed":
            return None

        if output.status == "cannot_complete":
            self.stuck_condition = StuckCondition.CANNOT_COMPLETE
            return self.stuck_condition

        # For ask_mentor status, analyze the problem and question
        text_to_analyze = " ".join(
            filter(
                None,
                [
                    output.problem,
                    output.question_for_mentor,
                    output.reason,
                    output.summary,
                ],
            )
        )

        if text_to_analyze:
            condition = self._detect_condition_from_text(text_to_analyze)
            if condition:
                self.stuck_condition = condition
                return condition

        # Default to None if we can't classify
        return None

    def detect_from_error(self, error: str) -> StuckCondition | None:
        """Detect stuck condition from an error message.

        Analyzes error text to identify specific stuck conditions like
        missing dependencies or command failures.

        Args:
            error: The error message to analyze.

        Returns:
            The detected StuckCondition, or None if not detected.
        """
        condition = self._detect_condition_from_text(error)
        if condition:
            self.stuck_condition = condition
        return condition

    def _detect_condition_from_text(self, text: str) -> StuckCondition | None:
        """Detect stuck condition from arbitrary text.

        Args:
            text: The text to analyze for patterns.

        Returns:
            The detected StuckCondition, or None if no pattern matches.
        """
        # Check for missing dependency patterns first (most specific)
        for pattern in _MISSING_DEPENDENCY_PATTERNS:
            if pattern.search(text):
                return StuckCondition.MISSING_DEPENDENCY

        # Check for ambiguous instruction patterns
        for pattern in _AMBIGUOUS_INSTRUCTION_PATTERNS:
            if pattern.search(text):
                return StuckCondition.AMBIGUOUS_INSTRUCTION

        # Check for command failure patterns
        for pattern in _COMMAND_FAILURE_PATTERNS:
            if pattern.search(text):
                return StuckCondition.COMMAND_FAILURE

        return None

    def should_ask_mentor(self, condition: StuckCondition) -> bool:
        """Check if the given condition should trigger mentor assistance.

        Uses the configuration to determine whether the specific condition
        warrants escalating to the mentor.

        Args:
            condition: The stuck condition to check.

        Returns:
            True if the condition should trigger mentor assistance.
        """
        # Map conditions to config attributes (or None for always-true)
        condition_config_map: dict[StuckCondition, bool | None] = {
            StuckCondition.TIMEOUT: self.config.ask_on_timeout,
            StuckCondition.MISSING_DEPENDENCY: self.config.ask_on_missing_dependency,
            StuckCondition.AMBIGUOUS_INSTRUCTION: self.config.ask_on_ambiguous_instruction,
            StuckCondition.COMMAND_FAILURE: self.config.ask_on_command_failure,
            StuckCondition.MAX_RETRIES: None,  # Always ask
            StuckCondition.PARSE_FAILURE: None,  # Always ask
            StuckCondition.CANNOT_COMPLETE: None,  # Always ask
        }
        config_value = condition_config_map.get(condition)
        # None means always ask, otherwise use the config value
        return config_value is None or config_value

    def reset(self) -> None:
        """Reset the detector state.

        Clears all retry counts and the stuck condition. Call this
        when starting a new iteration or tutorial.
        """
        self._retry_counts.clear()
        self.stuck_condition = None

    def get_retry_count(self, step: str) -> int:
        """Get the current retry count for a step.

        Args:
            step: Identifier or description of the step.

        Returns:
            The number of failed attempts for this step.
        """
        return self._retry_counts.get(step, 0)


class LlmCliError(Exception):
    """Base exception for LLM CLI errors.

    Raised when the LLM CLI command fails to execute properly,
    returns a non-zero exit code, or produces unexpected output.

    Attributes:
        provider: The LLM provider that was invoked.
        command: The command that was executed.
        exit_code: The exit code from the CLI, if available.
        stderr: The stderr output from the CLI, if available.
    """

    def __init__(
        self,
        message: str,
        *,
        provider: LlmProvider | None = None,
        command: Sequence[str] | None = None,
        exit_code: int | None = None,
        stderr: str | None = None,
    ) -> None:
        super().__init__(message)
        self.provider = provider
        self.command = list(command) if command else None
        self.exit_code = exit_code
        self.stderr = stderr

    def __str__(self) -> str:
        parts = [super().__str__()]
        if self.provider:
            parts.append(f"Provider: {self.provider.value}")
        if self.exit_code is not None:
            parts.append(f"Exit code: {self.exit_code}")
        if self.stderr:
            parts.append(f"Stderr: {self.stderr[:500]}")  # Truncate long stderr
        return " | ".join(parts)


class LlmTimeoutError(LlmCliError):
    """Exception raised when the LLM CLI times out.

    Raised when the CLI process exceeds the configured timeout duration
    and is forcibly terminated.

    Attributes:
        timeout_seconds: The timeout duration that was exceeded.
    """

    def __init__(
        self,
        message: str,
        *,
        timeout_seconds: int,
        provider: LlmProvider | None = None,
        command: Sequence[str] | None = None,
    ) -> None:
        super().__init__(message, provider=provider, command=command)
        self.timeout_seconds = timeout_seconds

    def __str__(self) -> str:
        base = super().__str__()
        return f"{base} | Timeout: {self.timeout_seconds}s"


class LlmParseError(LlmCliError):
    """Exception raised when LLM output cannot be parsed as valid JSON.

    Raised when the CLI produces output that cannot be parsed into
    the expected StudentOutput JSON format.

    Attributes:
        raw_output: The raw output that failed to parse.
        parse_error: The underlying parsing error message.
    """

    def __init__(
        self,
        message: str,
        *,
        raw_output: str,
        parse_error: str,
        provider: LlmProvider | None = None,
    ) -> None:
        super().__init__(message, provider=provider)
        self.raw_output = raw_output
        self.parse_error = parse_error

    def __str__(self) -> str:
        base = super().__str__()
        # Truncate raw output for display
        truncated_output = (
            self.raw_output[:200] + "..." if len(self.raw_output) > 200 else self.raw_output
        )
        return f"{base} | Parse error: {self.parse_error} | Output: {truncated_output}"


class LlmCli:
    """LLM CLI invocation handler.

    Provides a unified interface for invoking different LLM CLI tools
    (claude, codex, gemini) via subprocess with proper timeout handling
    and error management.

    Attributes:
        provider: The LLM provider to use.
        timeout_seconds: Timeout in seconds for CLI execution.

    Example:
        >>> cli = LlmCli(LlmProvider.CLAUDE, timeout_seconds=60)
        >>> output = cli.invoke("What is 2+2?")
        >>> print(output)
    """

    def __init__(self, provider: LlmProvider, *, timeout_seconds: int = 60) -> None:
        """Initialize the LLM CLI handler.

        Args:
            provider: The LLM provider to use (claude, codex, or gemini).
            timeout_seconds: Timeout in seconds for CLI execution.
        """
        self.provider = provider
        self.timeout_seconds = timeout_seconds

    def _build_command(self, prompt: str) -> list[str]:
        """Build the CLI command for the configured provider.

        Args:
            prompt: The prompt to send to the LLM.

        Returns:
            The command as a list of strings ready for subprocess execution.
        """
        match self.provider:
            case LlmProvider.CLAUDE:
                # Claude uses -p flag for prompt
                return ["claude", "-p", prompt]
            case LlmProvider.CODEX:
                # Codex uses --prompt flag
                return ["codex", "--prompt", prompt]
            case LlmProvider.GEMINI:
                # Gemini uses --prompt flag
                return ["gemini", "--prompt", prompt]

    def invoke(self, prompt: str) -> str:
        """Invoke the LLM CLI with the given prompt.

        Executes the CLI command as a subprocess, captures the output,
        and handles timeout and error conditions.

        Args:
            prompt: The prompt to send to the LLM.

        Returns:
            The stdout output from the CLI.

        Raises:
            LlmTimeoutError: If the CLI execution exceeds the timeout.
            LlmCliError: If the CLI returns a non-zero exit code.
        """
        command = self._build_command(prompt)

        try:
            result = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
        except subprocess.TimeoutExpired as e:
            raise LlmTimeoutError(
                f"LLM CLI timed out after {self.timeout_seconds} seconds",
                timeout_seconds=self.timeout_seconds,
                provider=self.provider,
                command=command,
            ) from e

        if result.returncode != 0:
            raise LlmCliError(
                f"LLM CLI failed with exit code {result.returncode}",
                provider=self.provider,
                command=command,
                exit_code=result.returncode,
                stderr=result.stderr,
            )

        return result.stdout


def _extract_json_from_output(output: str) -> str:
    """Extract JSON from LLM output, handling markdown code blocks.

    LLMs sometimes wrap JSON output in markdown code blocks. This function
    attempts to extract the JSON content from such wrappers.

    Args:
        output: The raw LLM output.

    Returns:
        The extracted JSON string.
    """
    # First, try to use the output as-is (after stripping whitespace)
    stripped = output.strip()

    # Check for markdown code blocks with json specifier
    json_block_pattern = r"```(?:json)?\s*\n([\s\S]*?)\n```"
    matches: list[str] = re.findall(json_block_pattern, stripped)
    if matches:
        # Return the last JSON block (most likely to be the final output)
        return matches[-1].strip()

    # Check if the output starts with a code fence
    if stripped.startswith("```"):
        lines = stripped.split("\n")
        # Remove first and last lines (the fences)
        if len(lines) >= 3:
            # Find the closing fence
            for i in range(len(lines) - 1, 0, -1):
                if lines[i].strip() == "```":
                    return "\n".join(lines[1:i]).strip()

    # Return the stripped output if no code blocks found
    return stripped


def _parse_student_output(
    output: str,
    provider: LlmProvider,
) -> StudentOutput:
    """Parse LLM output into a StudentOutput object.

    Attempts to parse the raw LLM output as JSON and validate it
    against the StudentOutput schema. Handles markdown code block
    extraction and provides detailed error messages on failure.

    Args:
        output: The raw output from the LLM CLI.
        provider: The LLM provider (for error reporting).

    Returns:
        A validated StudentOutput object.

    Raises:
        LlmParseError: If the output cannot be parsed or validated.
    """
    json_str = _extract_json_from_output(output)

    if not json_str:
        raise LlmParseError(
            "LLM output is empty after extraction",
            raw_output=output,
            parse_error="Empty output",
            provider=provider,
        )

    try:
        data = json.loads(json_str)
    except json.JSONDecodeError as e:
        raise LlmParseError(
            "Failed to parse LLM output as JSON",
            raw_output=output,
            parse_error=str(e),
            provider=provider,
        ) from e

    try:
        return StudentOutput.model_validate(data)
    except ValidationError as e:
        raise LlmParseError(
            "LLM output does not conform to StudentOutput schema",
            raw_output=output,
            parse_error=str(e),
            provider=provider,
        ) from e


class StudentWrapper:
    """Student agent wrapper for executing tutorial steps.

    The StudentWrapper simulates a constrained learner attempting to follow
    a technical tutorial. It invokes an LLM CLI to process the tutorial
    content and produces structured output indicating progress, problems,
    and requests for mentor assistance.

    Attributes:
        config: Student behavior configuration.
        provider: The LLM provider to use.
        tutorial_content: The tutorial markdown content.
        mentor_notes: Notes from previous mentor interactions.
        iteration: The current iteration number (1-indexed).
        stuck_detector: Optional detector for classifying stuck conditions.
        stuck_condition: The detected stuck condition after running, if any.

    Example:
        >>> detector = StuckDetector(config=StudentBehavior())
        >>> wrapper = StudentWrapper(
        ...     config=StudentBehavior(),
        ...     provider=LlmProvider.CLAUDE,
        ...     tutorial_content="# Tutorial...",
        ...     mentor_notes=[],
        ...     iteration=1,
        ...     stuck_detector=detector,
        ... )
        >>> result = wrapper.run()
        >>> if wrapper.stuck_condition:
        ...     print(f"Student got stuck: {wrapper.stuck_condition}")
    """

    def __init__(
        self,
        *,
        config: StudentBehavior,
        provider: LlmProvider,
        tutorial_content: str,
        mentor_notes: list[str],
        iteration: int,
        stuck_detector: StuckDetector | None = None,
    ) -> None:
        """Initialize the StudentWrapper.

        Args:
            config: Student behavior configuration controlling escalation rules.
            provider: The LLM provider to use for execution.
            tutorial_content: The full markdown content of the tutorial.
            mentor_notes: List of notes from previous mentor interactions.
            iteration: The current iteration number (1-indexed).
            stuck_detector: Optional detector for tracking retries and classifying
                stuck conditions. If not provided, stuck detection is disabled.
        """
        self.config = config
        self.provider = provider
        self.tutorial_content = tutorial_content
        self.mentor_notes = mentor_notes
        self.iteration = iteration
        self.stuck_detector = stuck_detector
        self.stuck_condition: StuckCondition | None = None

    def _build_prompt(self) -> str:
        """Build the prompt for the student agent.

        Returns:
            The complete prompt string.
        """
        return build_student_prompt(
            tutorial_content=self.tutorial_content,
            student_behavior=self.config,
            mentor_notes=self.mentor_notes,
            iteration=self.iteration,
        )

    def _create_fallback_output(
        self,
        error: Exception,
        *,
        condition: StuckCondition | None = None,
    ) -> StudentOutput:
        """Create a fallback StudentOutput when all retries fail.

        Args:
            error: The exception that caused the final failure.
            condition: Optional stuck condition to set. If not provided,
                the condition is inferred from the error type.

        Returns:
            A StudentOutput indicating the need for mentor assistance.
        """
        error_message = str(error)

        # Determine stuck condition from error type if not provided
        if condition is None:
            if isinstance(error, LlmTimeoutError):
                condition = StuckCondition.TIMEOUT
            elif isinstance(error, LlmParseError):
                condition = StuckCondition.PARSE_FAILURE
            elif self.stuck_detector:
                # Try to detect from error message
                condition = self.stuck_detector.detect_from_error(error_message)
                if condition is None:
                    condition = StuckCondition.COMMAND_FAILURE

        # Set stuck_condition on wrapper
        self.stuck_condition = condition

        # Also update detector if available
        if self.stuck_detector and condition:
            self.stuck_detector.stuck_condition = condition

        if isinstance(error, LlmTimeoutError):
            problem = f"LLM CLI timed out after {error.timeout_seconds} seconds"
            question = (
                "The LLM is taking too long to respond. Is there an issue with "
                "the prompt or should I try with simpler instructions?"
            )
        elif isinstance(error, LlmParseError):
            problem = f"Unable to parse LLM response: {error.parse_error}"
            question = (
                "The LLM is not producing valid JSON output. Can you help me "
                "understand what format the response should be in?"
            )
        else:
            problem = f"LLM CLI error: {error_message}"
            question = (
                "I encountered an error while trying to follow the tutorial. What should I do?"
            )

        return StudentOutput(
            status="ask_mentor",
            current_step="Initial tutorial processing",
            attempted_actions=["Invoke LLM CLI"],
            problem=problem,
            question_for_mentor=question,
            summary="Failed to process tutorial due to LLM CLI issues",
        )

    def _record_failed_attempt(self, step: str, error: Exception) -> StudentOutput | None:
        """Record a failed attempt and return fallback if max retries reached.

        Args:
            step: The step being attempted.
            error: The exception that caused the failure.

        Returns:
            A fallback StudentOutput if max retries reached, None otherwise.
        """
        if not self.stuck_detector:
            return None
        max_reached = self.stuck_detector.record_attempt(step, success=False)
        if max_reached:
            return self._create_fallback_output(error, condition=StuckCondition.MAX_RETRIES)
        return None

    def _handle_successful_output(self, result: StudentOutput, current_step: str) -> None:
        """Handle a successful LLM output by updating detector state.

        Args:
            result: The parsed StudentOutput.
            current_step: The step being attempted.
        """
        if not self.stuck_detector:
            return
        condition = self.stuck_detector.classify_output(result)
        if condition:
            self.stuck_condition = condition
        if result.status == "completed":
            self.stuck_detector.record_attempt(current_step, success=True)

    def _handle_timeout_error(
        self, error: LlmTimeoutError, current_step: str
    ) -> StudentOutput | None:
        """Handle timeout error and return fallback if needed.

        Args:
            error: The timeout error.
            current_step: The step being attempted.

        Returns:
            Fallback output if should ask mentor, None to re-raise.
        """
        if fallback := self._record_failed_attempt(current_step, error):
            return fallback
        if self.config.ask_on_timeout:
            return self._create_fallback_output(error, condition=StuckCondition.TIMEOUT)
        return None

    def _handle_cli_error(self, error: LlmCliError, current_step: str) -> StudentOutput | None:
        """Handle CLI error and return fallback if needed.

        Args:
            error: The CLI error.
            current_step: The step being attempted.

        Returns:
            Fallback output if should ask mentor, None to re-raise.
        """
        if fallback := self._record_failed_attempt(current_step, error):
            return fallback
        detected_condition = (
            self.stuck_detector.detect_from_error(error.stderr)
            if self.stuck_detector and error.stderr
            else None
        )
        if self.config.ask_on_command_failure:
            return self._create_fallback_output(error, condition=detected_condition)
        return None

    def run(self, *, current_step: str = "Initial tutorial processing") -> StudentOutput:
        """Execute the student agent and return the result.

        Builds the prompt, invokes the LLM CLI, and parses the response.
        Retries up to MAX_PARSE_RETRIES times on parse errors before
        returning a fallback response requesting mentor assistance.

        If a `stuck_detector` is configured, it will be used to:
        - Track retry attempts per step
        - Classify stuck conditions from errors and output
        - Set the `stuck_condition` attribute on this wrapper

        Args:
            current_step: Description of the current step being attempted.
                Used for retry tracking with StuckDetector.

        Returns:
            A StudentOutput object containing the result of the step attempt.

        Raises:
            LlmTimeoutError: If the CLI times out and config.ask_on_timeout is False.
            LlmCliError: If CLI fails and error is not recoverable.
        """
        prompt = self._build_prompt()
        cli = LlmCli(self.provider, timeout_seconds=self.config.timeout_seconds)
        last_error: Exception | None = None

        for attempt in range(MAX_PARSE_RETRIES):
            try:
                output = cli.invoke(prompt)
                result = _parse_student_output(output, self.provider)
                self._handle_successful_output(result, current_step)
                return result

            except LlmTimeoutError as e:
                if fallback := self._handle_timeout_error(e, current_step):
                    return fallback
                raise

            except LlmParseError as e:
                last_error = e
                print(
                    f"Parse attempt {attempt + 1}/{MAX_PARSE_RETRIES} failed: {e.parse_error}",
                    file=sys.stderr,
                )
                if fallback := self._record_failed_attempt(current_step, e):
                    return fallback
                continue

            except LlmCliError as e:
                if fallback := self._handle_cli_error(e, current_step):
                    return fallback
                raise

        # All retries exhausted
        return self._create_fallback_output(
            last_error or LlmCliError("Unknown error"),
            condition=StuckCondition.PARSE_FAILURE,
        )


def _load_config_from_file(config_path: Path) -> Config:
    """Load configuration from a JSON file.

    Args:
        config_path: Path to the configuration JSON file.

    Returns:
        A validated Config object.

    Raises:
        FileNotFoundError: If the config file does not exist.
        json.JSONDecodeError: If the file is not valid JSON.
        ValidationError: If the JSON does not match the Config schema.
    """
    with config_path.open() as f:
        data = json.load(f)
    return Config.model_validate(data)


def _load_mentor_notes_from_file(notes_path: Path) -> list[str]:
    """Load mentor notes from a JSON file.

    Args:
        notes_path: Path to the mentor notes JSON file.

    Returns:
        A list of mentor note strings.
    """
    if not notes_path.exists():
        return []

    with notes_path.open() as f:
        data = json.load(f)

    if isinstance(data, list):
        return [str(note) for note in data]
    return []


def _load_tutorial_content(tutorial_dir: Path) -> str:
    """Load tutorial content from the tutorial directory.

    Looks for common tutorial filenames in the specified directory.

    Args:
        tutorial_dir: Path to the directory containing the tutorial.

    Returns:
        The tutorial content as a string.

    Raises:
        FileNotFoundError: If no tutorial file is found.
    """
    # Common tutorial filenames to look for
    tutorial_names = [
        "tutorial.md",
        "README.md",
        "index.md",
        "TUTORIAL.md",
    ]

    for name in tutorial_names:
        tutorial_path = tutorial_dir / name
        if tutorial_path.exists():
            return tutorial_path.read_text()

    # If no common name found, look for any markdown file
    md_files = list(tutorial_dir.glob("*.md"))
    if md_files:
        return md_files[0].read_text()

    raise FileNotFoundError(
        f"No tutorial file found in {tutorial_dir}. Expected one of: {', '.join(tutorial_names)}"
    )


def main() -> None:
    """Entry point for student wrapper.

    Loads configuration and tutorial content from the environment or
    mounted volumes, runs the student wrapper, and outputs the result.

    Environment variables:
        SMILE_CONFIG_FILE: Path to config JSON file (default: /workspace/config.json)
        SMILE_TUTORIAL_DIR: Path to tutorial directory (default: /workspace/tutorial)
        SMILE_MENTOR_NOTES_FILE: Path to mentor notes JSON (default: /workspace/mentor_notes.json)
        SMILE_ITERATION: Current iteration number (default: 1)
    """
    # Get paths from environment or use defaults
    config_path = Path(os.environ.get("SMILE_CONFIG_FILE", DEFAULT_CONFIG_FILE))
    tutorial_dir = Path(os.environ.get("SMILE_TUTORIAL_DIR", DEFAULT_TUTORIAL_DIR))
    mentor_notes_path = Path(os.environ.get("SMILE_MENTOR_NOTES_FILE", DEFAULT_MENTOR_NOTES_FILE))
    iteration = int(os.environ.get("SMILE_ITERATION", "1"))

    # Load configuration
    try:
        config = _load_config_from_file(config_path)
    except FileNotFoundError:
        print(f"Error: Config file not found at {config_path}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in config file: {e}", file=sys.stderr)
        sys.exit(1)
    except ValidationError as e:
        print(f"Error: Invalid config format: {e}", file=sys.stderr)
        sys.exit(1)

    # Load tutorial content
    try:
        tutorial_content = _load_tutorial_content(tutorial_dir)
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    # Load mentor notes
    try:
        mentor_notes = _load_mentor_notes_from_file(mentor_notes_path)
    except json.JSONDecodeError as e:
        print(f"Warning: Invalid JSON in mentor notes file: {e}", file=sys.stderr)
        mentor_notes = []

    # Create and run the student wrapper
    wrapper = StudentWrapper(
        config=config.student_behavior,
        provider=config.llm_provider,
        tutorial_content=tutorial_content,
        mentor_notes=mentor_notes,
        iteration=iteration,
    )

    try:
        result = wrapper.run()
        # Output result as JSON for the orchestrator to consume
        print(result.model_dump_json(indent=2))
    except LlmCliError as e:
        print(f"Error: LLM CLI failed: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
