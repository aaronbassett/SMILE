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

    Example:
        >>> wrapper = StudentWrapper(
        ...     config=StudentBehavior(),
        ...     provider=LlmProvider.CLAUDE,
        ...     tutorial_content="# Tutorial...",
        ...     mentor_notes=[],
        ...     iteration=1
        ... )
        >>> result = wrapper.run()  # Returns StudentOutput
    """

    def __init__(
        self,
        *,
        config: StudentBehavior,
        provider: LlmProvider,
        tutorial_content: str,
        mentor_notes: list[str],
        iteration: int,
    ) -> None:
        """Initialize the StudentWrapper.

        Args:
            config: Student behavior configuration controlling escalation rules.
            provider: The LLM provider to use for execution.
            tutorial_content: The full markdown content of the tutorial.
            mentor_notes: List of notes from previous mentor interactions.
            iteration: The current iteration number (1-indexed).
        """
        self.config = config
        self.provider = provider
        self.tutorial_content = tutorial_content
        self.mentor_notes = mentor_notes
        self.iteration = iteration

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

    def _create_fallback_output(self, error: Exception) -> StudentOutput:
        """Create a fallback StudentOutput when all retries fail.

        Args:
            error: The exception that caused the final failure.

        Returns:
            A StudentOutput indicating the need for mentor assistance.
        """
        error_message = str(error)
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

    def run(self) -> StudentOutput:
        """Execute the student agent and return the result.

        Builds the prompt, invokes the LLM CLI, and parses the response.
        Retries up to MAX_PARSE_RETRIES times on parse errors before
        returning a fallback response requesting mentor assistance.

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
                return _parse_student_output(output, self.provider)
            except LlmTimeoutError as e:
                # On timeout, immediately return fallback if configured to ask mentor
                if self.config.ask_on_timeout:
                    return self._create_fallback_output(e)
                raise
            except LlmParseError as e:
                last_error = e
                # Log the attempt (in production, use proper logging)
                print(
                    f"Parse attempt {attempt + 1}/{MAX_PARSE_RETRIES} failed: {e.parse_error}",
                    file=sys.stderr,
                )
                # Continue to next retry
                continue
            except LlmCliError as e:
                # For other CLI errors, return fallback if configured
                if self.config.ask_on_command_failure:
                    return self._create_fallback_output(e)
                raise

        # All retries exhausted, return fallback
        if last_error is not None:
            return self._create_fallback_output(last_error)

        # This should never happen, but satisfy type checker
        return self._create_fallback_output(LlmCliError("Unknown error during student execution"))


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
