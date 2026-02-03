"""Mentor agent wrapper for SMILE Loop.

This module implements the Mentor agent that helps stuck students by providing
hints, guidance, and explanations without completing tasks for them.

The Mentor receives context about what the Student tried and what went wrong,
then provides plain text guidance to help the Student progress.

The module also provides HTTP callback functionality to report results back
to the orchestrator via the `MentorOrchestratorClient` class.
"""

from __future__ import annotations

import json
import os
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

import httpx
from pydantic import BaseModel, Field, ValidationError

from smile_wrappers.config import Config
from smile_wrappers.prompts import build_mentor_prompt
from smile_wrappers.student import (
    LlmCli,
    LlmCliError,
    LlmTimeoutError,
    NextAction,
    OrchestratorCallbackError,
)

__all__ = [
    "MentorOrchestratorClient",
    "MentorResultRequest",
    "MentorResultResponse",
    "MentorWrapper",
    "StuckContext",
    "mentor_main",
]

# Default paths for container execution
DEFAULT_WORKSPACE_DIR = "/workspace"
DEFAULT_TUTORIAL_DIR = "/workspace/tutorial"
DEFAULT_CONFIG_FILE = "/workspace/config.json"
DEFAULT_STUCK_CONTEXT_FILE = "/workspace/stuck_context.json"
DEFAULT_MENTOR_NOTES_FILE = "/workspace/mentor_notes.json"

# Default orchestrator URL (from inside Docker container)
DEFAULT_ORCHESTRATOR_URL = "http://host.docker.internal:3000"

# HTTP client retry configuration
MAX_HTTP_RETRIES = 3
INITIAL_BACKOFF_SECONDS = 1.0
MAX_BACKOFF_SECONDS = 10.0

# Special exit code for stop action
EXIT_CODE_STOP = 42

# Default timeout for mentor LLM invocation (seconds)
DEFAULT_MENTOR_TIMEOUT_SECONDS = 120


class StuckContext(BaseModel):
    """Context about why the Student got stuck.

    This model captures the information passed from the Student to the Mentor,
    describing what step the Student was on and what problem they encountered.

    Attributes:
        current_step: Description of the step where the student is stuck.
        problem: Description of the problem the student encountered.
        question: The specific question the student is asking.
    """

    current_step: str = Field(alias="currentStep")
    problem: str
    question: str

    model_config = {"populate_by_name": True}


class MentorResultRequest(BaseModel):
    """Request body for reporting mentor result to the orchestrator.

    This model is serialized to JSON when calling the orchestrator's
    `/api/mentor/result` endpoint.

    Attributes:
        mentor_output: The plain text notes from the Mentor.
        timestamp: ISO8601 timestamp when the result was produced.
    """

    mentor_output: str = Field(alias="mentorOutput")
    timestamp: datetime

    model_config = {"populate_by_name": True}


class MentorResultResponse(BaseModel):
    """Response from orchestrator after reporting mentor result.

    Received from the orchestrator's `/api/mentor/result` endpoint
    after successfully reporting a result.

    Attributes:
        acknowledged: Whether the orchestrator acknowledged the result.
        next_action: The action the wrapper should take next.
    """

    acknowledged: bool
    next_action: NextAction = Field(alias="nextAction")

    model_config = {"populate_by_name": True}


class MentorOrchestratorClient:
    """HTTP client for reporting Mentor results to the SMILE orchestrator.

    Handles sending mentor output back to the orchestrator and receiving
    instructions for the next action. Implements retry logic with
    exponential backoff for transient failures.

    Attributes:
        base_url: The base URL of the orchestrator API.
        timeout_seconds: Timeout for HTTP requests in seconds.

    Example:
        >>> client = MentorOrchestratorClient(base_url="http://localhost:3000")
        >>> action = client.report_mentor_result("Try checking your PATH variable...")
        >>> action
        <NextAction.CONTINUE: 'continue'>
    """

    def __init__(
        self,
        base_url: str = DEFAULT_ORCHESTRATOR_URL,
        timeout_seconds: float = 30.0,
    ) -> None:
        """Initialize the MentorOrchestratorClient.

        Args:
            base_url: The base URL of the orchestrator API.
                Defaults to the Docker internal hostname.
            timeout_seconds: Timeout for HTTP requests in seconds.
                Defaults to 30 seconds.
        """
        # Ensure base_url doesn't have trailing slash
        self.base_url = base_url.rstrip("/")
        self.timeout_seconds = timeout_seconds

    def _log(self, message: str) -> None:
        """Log a message to stderr.

        Args:
            message: The message to log.
        """
        print(f"[MentorOrchestratorClient] {message}", file=sys.stderr)

    def _calculate_backoff(self, attempt: int) -> float:
        """Calculate backoff duration for a retry attempt.

        Uses exponential backoff with a maximum cap.

        Args:
            attempt: The current attempt number (0-indexed).

        Returns:
            The number of seconds to wait before retrying.
        """
        backoff = INITIAL_BACKOFF_SECONDS * (2**attempt)
        return float(min(backoff, MAX_BACKOFF_SECONDS))

    def report_mentor_result(self, output: str) -> NextAction:
        """Report a mentor result to the orchestrator.

        Sends the mentor's plain text output to the orchestrator's
        `/api/mentor/result` endpoint and returns the next action to take.

        Implements retry logic with exponential backoff for connection
        errors and 5xx server errors.

        Args:
            output: The plain text notes from the Mentor.

        Returns:
            The NextAction indicating what the wrapper should do next.

        Raises:
            OrchestratorCallbackError: If the callback fails after all retries.
        """
        endpoint = f"{self.base_url}/api/mentor/result"
        timestamp = datetime.now(UTC)

        request = MentorResultRequest(
            mentorOutput=output,
            timestamp=timestamp,
        )

        # Serialize with camelCase aliases for the API
        request_body = request.model_dump(mode="json", by_alias=True)

        last_error: Exception | None = None

        for attempt in range(MAX_HTTP_RETRIES):
            try:
                self._log(f"Reporting result to {endpoint} (attempt {attempt + 1})")

                with httpx.Client(timeout=self.timeout_seconds) as client:
                    response = client.post(
                        endpoint,
                        json=request_body,
                        headers={"Content-Type": "application/json"},
                    )

                # Handle specific HTTP status codes
                if response.status_code == 200:
                    try:
                        response_data = response.json()
                        result = MentorResultResponse.model_validate(response_data)
                        self._log(f"Result acknowledged, next action: {result.next_action.value}")
                        return result.next_action
                    except (json.JSONDecodeError, ValidationError) as e:
                        raise OrchestratorCallbackError(
                            f"Invalid response from orchestrator: {e}",
                            status_code=response.status_code,
                            response_body=response.text,
                        ) from e

                if response.status_code == 400:
                    # Client error - don't retry
                    raise OrchestratorCallbackError(
                        "Invalid request to orchestrator (400 Bad Request)",
                        status_code=response.status_code,
                        response_body=response.text,
                    )

                if response.status_code == 503:
                    # Service unavailable - may be transient, retry
                    self._log("Orchestrator unavailable (503), will retry")
                    last_error = OrchestratorCallbackError(
                        "Orchestrator unavailable (503 Service Unavailable)",
                        status_code=response.status_code,
                        response_body=response.text,
                    )
                elif response.status_code >= 500:
                    # Other server errors - retry
                    self._log(f"Server error ({response.status_code}), will retry")
                    last_error = OrchestratorCallbackError(
                        f"Orchestrator server error ({response.status_code})",
                        status_code=response.status_code,
                        response_body=response.text,
                    )
                else:
                    # Unexpected status code - don't retry
                    raise OrchestratorCallbackError(
                        f"Unexpected response from orchestrator ({response.status_code})",
                        status_code=response.status_code,
                        response_body=response.text,
                    )

            except httpx.ConnectError as e:
                self._log(f"Connection error: {e}")
                last_error = OrchestratorCallbackError(
                    f"Failed to connect to orchestrator at {self.base_url}: {e}",
                )

            except httpx.TimeoutException as e:
                self._log(f"Request timed out: {e}")
                last_error = OrchestratorCallbackError(
                    f"Request to orchestrator timed out after {self.timeout_seconds}s",
                )

            except OrchestratorCallbackError:
                # Re-raise non-retryable errors
                raise

            # Wait before retrying (except on last attempt)
            if attempt < MAX_HTTP_RETRIES - 1:
                backoff = self._calculate_backoff(attempt)
                self._log(f"Waiting {backoff:.1f}s before retry")
                time.sleep(backoff)

        # All retries exhausted
        raise last_error or OrchestratorCallbackError(
            "Failed to report result to orchestrator after all retries"
        )


class MentorWrapper:
    """Mentor agent wrapper for providing guidance to stuck students.

    The MentorWrapper helps students who are stuck on a tutorial step by
    providing hints, guidance, and explanations. It does NOT complete tasks
    for the student or provide complete solutions.

    Unlike the StudentWrapper, the Mentor outputs plain text notes (not JSON)
    and does not need structured output parsing.

    Attributes:
        config: The SMILE configuration.
        tutorial_content: The tutorial markdown content.
        current_step: Description of the step where the student is stuck.
        problem: Description of the problem encountered.
        question: The specific question the student is asking.
        previous_notes: Notes from previous mentor interactions.
        timeout_seconds: Timeout for LLM CLI invocation.

    Example:
        >>> wrapper = MentorWrapper(
        ...     config=Config(),
        ...     tutorial_content="# Tutorial...",
        ...     current_step="Install dependencies",
        ...     problem="pip install failed with permission error",
        ...     question="How do I fix this permission error?",
        ...     previous_notes=[],
        ... )
        >>> notes = wrapper.run()
        >>> print(notes)  # Plain text guidance
    """

    def __init__(
        self,
        *,
        config: Config,
        tutorial_content: str,
        current_step: str,
        problem: str,
        question: str,
        previous_notes: list[str],
        timeout_seconds: int = DEFAULT_MENTOR_TIMEOUT_SECONDS,
    ) -> None:
        """Initialize the MentorWrapper.

        Args:
            config: The SMILE configuration containing LLM provider settings.
            tutorial_content: The full markdown content of the tutorial.
            current_step: Description of the step where the student is stuck.
            problem: Description of the problem the student encountered.
            question: The specific question the student is asking.
            previous_notes: List of notes from previous mentor interactions.
            timeout_seconds: Timeout in seconds for LLM CLI invocation.
                Defaults to 120 seconds.
        """
        self.config = config
        self.tutorial_content = tutorial_content
        self.current_step = current_step
        self.problem = problem
        self.question = question
        self.previous_notes = previous_notes
        self.timeout_seconds = timeout_seconds

    def _build_prompt(self) -> str:
        """Build the prompt for the mentor agent.

        Returns:
            The complete prompt string.
        """
        return build_mentor_prompt(
            tutorial_content=self.tutorial_content,
            current_step=self.current_step,
            problem=self.problem,
            question=self.question,
            previous_notes=self.previous_notes,
        )

    def _create_fallback_note(self, error: Exception) -> str:
        """Create a fallback note when LLM invocation fails.

        The Mentor should always produce some output, even if it's just
        acknowledging that it couldn't find an answer.

        Args:
            error: The exception that caused the failure.

        Returns:
            A plain text fallback note for the student.
        """
        if isinstance(error, LlmTimeoutError):
            return (
                "I apologize, but I'm having trouble processing your question "
                f"(the request timed out after {error.timeout_seconds} seconds). "
                "This might indicate a complex problem that needs more investigation. "
                "In the meantime, I suggest:\n\n"
                "1. Double-check that all prerequisites mentioned in the tutorial are installed\n"
                "2. Review the exact error message for any hints about what's missing\n"
                "3. Try breaking down the problem into smaller steps\n\n"
                "If the problem persists, it may indicate a gap in the tutorial that "
                "needs to be addressed."
            )

        if isinstance(error, LlmCliError):
            return (
                "I encountered a technical issue while trying to help you "
                f"({type(error).__name__}). "
                "This shouldn't prevent you from continuing. Here are some general tips:\n\n"
                "1. Read the error message carefully - it often contains the solution\n"
                "2. Check if the command or tool mentioned is installed correctly\n"
                "3. Verify that any required files or directories exist\n"
                "4. Consider whether there might be environment-specific differences\n\n"
                "If you continue to encounter issues, this may indicate a gap in the "
                "tutorial instructions."
            )

        return (
            "I wasn't able to provide specific guidance for your question due to a "
            f"technical issue: {error}\n\n"
            "However, here are some general debugging strategies:\n\n"
            "1. Read error messages carefully - they usually point to the problem\n"
            "2. Check that all prerequisites are met\n"
            "3. Verify commands are typed exactly as shown\n"
            "4. Look for any assumptions the tutorial might be making\n\n"
            "If the issue persists, it may indicate something missing from the tutorial."
        )

    def run(self) -> str:
        """Execute the mentor agent and return guidance notes.

        Builds the prompt, invokes the LLM CLI, and returns the plain text
        response. If the LLM fails, returns a fallback note with general
        guidance.

        Returns:
            Plain text notes providing guidance to the student.
        """
        prompt = self._build_prompt()
        cli = LlmCli(self.config.llm_provider, timeout_seconds=self.timeout_seconds)

        try:
            output = cli.invoke(prompt)
            # Mentor output is plain text, just strip whitespace
            return output.strip()

        except LlmTimeoutError as e:
            print(
                f"[MentorWrapper] LLM timeout after {e.timeout_seconds}s",
                file=sys.stderr,
            )
            return self._create_fallback_note(e)

        except LlmCliError as e:
            print(
                f"[MentorWrapper] LLM CLI error: {e}",
                file=sys.stderr,
            )
            return self._create_fallback_note(e)


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


def _load_stuck_context_from_file(context_path: Path) -> StuckContext:
    """Load stuck context from a JSON file.

    Args:
        context_path: Path to the stuck context JSON file.

    Returns:
        A validated StuckContext object.

    Raises:
        FileNotFoundError: If the context file does not exist.
        json.JSONDecodeError: If the file is not valid JSON.
        ValidationError: If the JSON does not match the StuckContext schema.
    """
    with context_path.open() as f:
        data = json.load(f)
    return StuckContext.model_validate(data)


def _load_previous_notes_from_file(notes_path: Path) -> list[str]:
    """Load previous mentor notes from a JSON file.

    Args:
        notes_path: Path to the mentor notes JSON file.

    Returns:
        A list of previous mentor note strings.
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


def _report_result_to_orchestrator(
    notes: str,
    orchestrator_url: str,
) -> None:
    """Report mentor result to orchestrator and exit appropriately.

    Args:
        notes: The mentor notes to report.
        orchestrator_url: Base URL of the orchestrator.

    Raises:
        SystemExit: Always exits with appropriate code.
    """
    try:
        client = MentorOrchestratorClient(base_url=orchestrator_url)
        action = client.report_mentor_result(notes)

        if action == NextAction.STOP:
            print("[mentor_main] Orchestrator requested stop", file=sys.stderr)
            sys.exit(EXIT_CODE_STOP)
        else:
            print("[mentor_main] Orchestrator requested continue", file=sys.stderr)
            sys.exit(0)

    except OrchestratorCallbackError as e:
        print(f"Error: Failed to report result to orchestrator: {e}", file=sys.stderr)
        sys.exit(1)


def mentor_main() -> None:
    """Entry point for mentor wrapper.

    Loads configuration, stuck context, and tutorial content from the
    environment or mounted volumes, runs the mentor wrapper, reports
    the result to the orchestrator via HTTP callback, and exits based
    on the orchestrator's response.

    Environment variables:
        SMILE_CONFIG_FILE: Path to config JSON file (default: /workspace/config.json)
        SMILE_TUTORIAL_DIR: Path to tutorial directory (default: /workspace/tutorial)
        SMILE_STUCK_CONTEXT_FILE: Path to stuck context JSON
            (default: /workspace/stuck_context.json)
        SMILE_MENTOR_NOTES_FILE: Path to previous mentor notes JSON
            (default: /workspace/mentor_notes.json)
        SMILE_ORCHESTRATOR_URL: Orchestrator base URL
            (default: http://host.docker.internal:3000)
        SMILE_SKIP_CALLBACK: If set to "true", skip HTTP callback (for testing)

    Exit codes:
        0: Success (NextAction.CONTINUE)
        1: Error during execution
        42: Stop requested by orchestrator (NextAction.STOP)
    """
    # Get paths from environment or use defaults
    config_path = Path(os.environ.get("SMILE_CONFIG_FILE", DEFAULT_CONFIG_FILE))
    tutorial_dir = Path(os.environ.get("SMILE_TUTORIAL_DIR", DEFAULT_TUTORIAL_DIR))
    stuck_context_path = Path(
        os.environ.get("SMILE_STUCK_CONTEXT_FILE", DEFAULT_STUCK_CONTEXT_FILE)
    )
    mentor_notes_path = Path(os.environ.get("SMILE_MENTOR_NOTES_FILE", DEFAULT_MENTOR_NOTES_FILE))
    orchestrator_url = os.environ.get("SMILE_ORCHESTRATOR_URL", DEFAULT_ORCHESTRATOR_URL)
    skip_callback = os.environ.get("SMILE_SKIP_CALLBACK", "").lower() == "true"

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

    # Load stuck context
    try:
        stuck_context = _load_stuck_context_from_file(stuck_context_path)
    except FileNotFoundError:
        print(f"Error: Stuck context file not found at {stuck_context_path}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in stuck context file: {e}", file=sys.stderr)
        sys.exit(1)
    except ValidationError as e:
        print(f"Error: Invalid stuck context format: {e}", file=sys.stderr)
        sys.exit(1)

    # Load tutorial content
    try:
        tutorial_content = _load_tutorial_content(tutorial_dir)
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    # Load previous mentor notes
    try:
        previous_notes = _load_previous_notes_from_file(mentor_notes_path)
    except json.JSONDecodeError as e:
        print(f"Warning: Invalid JSON in mentor notes file: {e}", file=sys.stderr)
        previous_notes = []

    # Create and run the mentor wrapper
    wrapper = MentorWrapper(
        config=config,
        tutorial_content=tutorial_content,
        current_step=stuck_context.current_step,
        problem=stuck_context.problem,
        question=stuck_context.question,
        previous_notes=previous_notes,
    )

    notes = wrapper.run()

    # Output notes for debugging/logging
    print(notes)

    # Report result to orchestrator via HTTP callback
    if skip_callback:
        print("[mentor_main] Skipping HTTP callback (SMILE_SKIP_CALLBACK=true)", file=sys.stderr)
        sys.exit(0)

    _report_result_to_orchestrator(notes, orchestrator_url)


if __name__ == "__main__":
    mentor_main()
