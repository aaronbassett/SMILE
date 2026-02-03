"""Tests for student wrapper."""

from __future__ import annotations

import json
from datetime import UTC, datetime
from subprocess import TimeoutExpired
from typing import TYPE_CHECKING
from unittest.mock import MagicMock, patch

import httpx
import pytest
from pydantic import ValidationError

from smile_wrappers.config import Config, LlmProvider, StudentBehavior
from smile_wrappers.output import StudentOutput
from smile_wrappers.student import (
    DEFAULT_ORCHESTRATOR_URL,
    LlmCli,
    LlmCliError,
    LlmParseError,
    LlmTimeoutError,
    NextAction,
    OrchestratorCallbackError,
    OrchestratorClient,
    StuckCondition,
    StuckDetector,
    StudentResultRequest,
    StudentResultResponse,
    StudentWrapper,
    _extract_json_from_output,
    _load_config_from_file,
    _load_mentor_notes_from_file,
    _load_tutorial_content,
    _parse_student_output,
)

if TYPE_CHECKING:
    from pathlib import Path


# =============================================================================
# Fixtures
# =============================================================================


@pytest.fixture
def config() -> Config:
    """Create a basic test configuration."""
    return Config(llm_provider=LlmProvider.CLAUDE)


@pytest.fixture
def student_behavior() -> StudentBehavior:
    """Create a basic student behavior configuration."""
    return StudentBehavior()


@pytest.fixture
def tutorial_content() -> str:
    """Sample tutorial content."""
    return """# Getting Started with Python

## Step 1: Install Python

First, download Python from python.org.

## Step 2: Create a Virtual Environment

Run the following command:

```bash
python -m venv .venv
```
"""


@pytest.fixture
def mentor_notes() -> list[str]:
    """Sample mentor notes."""
    return [
        "Make sure you have Python 3.8 or higher installed.",
        "Check your PATH environment variable.",
    ]


@pytest.fixture
def student_wrapper(
    student_behavior: StudentBehavior,
    tutorial_content: str,
    mentor_notes: list[str],
) -> StudentWrapper:
    """Create a StudentWrapper instance for testing."""
    return StudentWrapper(
        config=student_behavior,
        provider=LlmProvider.CLAUDE,
        tutorial_content=tutorial_content,
        mentor_notes=mentor_notes,
        iteration=1,
    )


@pytest.fixture
def stuck_detector(student_behavior: StudentBehavior) -> StuckDetector:
    """Create a StuckDetector instance for testing."""
    return StuckDetector(config=student_behavior)


@pytest.fixture
def valid_student_output_json() -> str:
    """Valid student output as JSON string."""
    return json.dumps(
        {
            "status": "completed",
            "currentStep": "Step 1: Install Python",
            "attemptedActions": ["Downloaded Python", "Ran installer"],
            "summary": "Successfully installed Python 3.11",
            "filesCreated": [],
            "commandsRun": ["python --version"],
        }
    )


# =============================================================================
# StuckCondition Tests
# =============================================================================


class TestStuckCondition:
    """Tests for the StuckCondition enum."""

    def test_stuck_condition_values(self) -> None:
        """StuckCondition should have expected values."""
        assert StuckCondition.TIMEOUT.value == "timeout"
        assert StuckCondition.MAX_RETRIES.value == "max_retries"
        assert StuckCondition.MISSING_DEPENDENCY.value == "missing_dependency"
        assert StuckCondition.AMBIGUOUS_INSTRUCTION.value == "ambiguous_instruction"
        assert StuckCondition.COMMAND_FAILURE.value == "command_failure"
        assert StuckCondition.PARSE_FAILURE.value == "parse_failure"
        assert StuckCondition.CANNOT_COMPLETE.value == "cannot_complete"


# =============================================================================
# NextAction Tests
# =============================================================================


class TestNextAction:
    """Tests for the NextAction enum."""

    def test_next_action_values(self) -> None:
        """NextAction should have expected values."""
        assert NextAction.CONTINUE.value == "continue"
        assert NextAction.STOP.value == "stop"


# =============================================================================
# OrchestratorCallbackError Tests
# =============================================================================


class TestOrchestratorCallbackError:
    """Tests for the OrchestratorCallbackError exception."""

    def test_error_with_status_code(self) -> None:
        """Error should include status code in string representation."""
        error = OrchestratorCallbackError(
            "Test error",
            status_code=500,
            response_body="Internal server error",
        )
        error_str = str(error)
        assert "Test error" in error_str
        assert "500" in error_str
        assert "Internal server error" in error_str

    def test_error_without_status_code(self) -> None:
        """Error should handle missing status code."""
        error = OrchestratorCallbackError("Connection failed")
        error_str = str(error)
        assert "Connection failed" in error_str

    def test_error_truncates_long_body(self) -> None:
        """Error should truncate long response bodies."""
        long_body = "x" * 500
        error = OrchestratorCallbackError(
            "Test error",
            status_code=400,
            response_body=long_body,
        )
        error_str = str(error)
        assert "..." in error_str
        assert len(error_str) < len(long_body) + 100


# =============================================================================
# StudentResultRequest Model Tests
# =============================================================================


class TestStudentResultRequest:
    """Tests for the StudentResultRequest model."""

    def test_request_from_camel_case(self) -> None:
        """StudentResultRequest should accept camelCase field names."""
        timestamp = datetime.now(UTC)
        output = StudentOutput(
            status="completed",
            currentStep="Step 1",
            attemptedActions=["action"],
            summary="Done",
        )
        request = StudentResultRequest(
            studentOutput=output,
            timestamp=timestamp,
        )
        assert request.student_output == output
        assert request.timestamp == timestamp

    def test_request_from_snake_case(self) -> None:
        """StudentResultRequest should accept snake_case field names."""
        timestamp = datetime.now(UTC)
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )
        request = StudentResultRequest(
            student_output=output,
            timestamp=timestamp,
        )
        assert request.student_output == output

    def test_request_serialization(self) -> None:
        """StudentResultRequest should serialize with camelCase aliases."""
        timestamp = datetime.now(UTC)
        output = StudentOutput(
            status="completed",
            currentStep="Step 1",
            attemptedActions=["action"],
            summary="Done",
        )
        request = StudentResultRequest(
            studentOutput=output,
            timestamp=timestamp,
        )
        data = request.model_dump(by_alias=True, mode="json")
        assert "studentOutput" in data
        assert "student_output" not in data


# =============================================================================
# StudentResultResponse Model Tests
# =============================================================================


class TestStudentResultResponse:
    """Tests for the StudentResultResponse model."""

    def test_response_from_camel_case(self) -> None:
        """StudentResultResponse should accept camelCase field names."""
        response = StudentResultResponse(
            acknowledged=True,
            nextAction="continue",
        )
        assert response.acknowledged is True
        assert response.next_action == NextAction.CONTINUE

    def test_response_from_snake_case(self) -> None:
        """StudentResultResponse should accept snake_case field names."""
        response = StudentResultResponse(
            acknowledged=True,
            next_action="stop",
        )
        assert response.next_action == NextAction.STOP

    def test_response_validation_invalid_action(self) -> None:
        """StudentResultResponse should validate next_action values."""
        with pytest.raises(ValidationError):
            StudentResultResponse(
                acknowledged=True,
                nextAction="invalid",
            )


# =============================================================================
# StuckDetector Tests
# =============================================================================


class TestStuckDetector:
    """Tests for the StuckDetector class."""

    def test_record_attempt_success_resets_counter(self, stuck_detector: StuckDetector) -> None:
        """Successful attempt should reset retry counter."""
        stuck_detector.record_attempt("Step 1", success=False)
        stuck_detector.record_attempt("Step 1", success=False)
        assert stuck_detector.get_retry_count("Step 1") == 2

        stuck_detector.record_attempt("Step 1", success=True)
        assert stuck_detector.get_retry_count("Step 1") == 0

    def test_record_attempt_max_retries(self, stuck_detector: StuckDetector) -> None:
        """Should return True when max retries reached."""
        # Default max_retries_before_help is 3
        assert not stuck_detector.record_attempt("Step 1", success=False)
        assert not stuck_detector.record_attempt("Step 1", success=False)
        assert stuck_detector.record_attempt("Step 1", success=False)  # 3rd attempt
        assert stuck_detector.stuck_condition == StuckCondition.MAX_RETRIES

    def test_classify_output_completed(self, stuck_detector: StuckDetector) -> None:
        """Completed status should return None (not stuck)."""
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )
        condition = stuck_detector.classify_output(output)
        assert condition is None

    def test_classify_output_cannot_complete(self, stuck_detector: StuckDetector) -> None:
        """Cannot complete status should return CANNOT_COMPLETE."""
        output = StudentOutput(
            status="cannot_complete",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Cannot do this",
            reason="Missing resources",
        )
        condition = stuck_detector.classify_output(output)
        assert condition == StuckCondition.CANNOT_COMPLETE

    def test_classify_output_missing_dependency(self, stuck_detector: StuckDetector) -> None:
        """Should detect missing dependency from problem text."""
        output = StudentOutput(
            status="ask_mentor",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Stuck",
            problem="command not found: pip",
            question_for_mentor="How do I install pip?",
        )
        condition = stuck_detector.classify_output(output)
        assert condition == StuckCondition.MISSING_DEPENDENCY

    def test_classify_output_ambiguous_instruction(self, stuck_detector: StuckDetector) -> None:
        """Should detect ambiguous instruction from problem text."""
        output = StudentOutput(
            status="ask_mentor",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Stuck",
            problem="I'm not sure which version to use - the instructions are unclear",
            question_for_mentor="Which version should I use?",
        )
        condition = stuck_detector.classify_output(output)
        assert condition == StuckCondition.AMBIGUOUS_INSTRUCTION

    def test_classify_output_command_failure(self, stuck_detector: StuckDetector) -> None:
        """Should detect command failure from problem text."""
        output = StudentOutput(
            status="ask_mentor",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Stuck",
            problem="error: permission denied when running the command",
            question_for_mentor="How do I fix permission issues?",
        )
        condition = stuck_detector.classify_output(output)
        assert condition == StuckCondition.COMMAND_FAILURE

    def test_detect_from_error_missing_dependency(self, stuck_detector: StuckDetector) -> None:
        """Should detect missing dependency from error message."""
        error_msg = "ModuleNotFoundError: No module named 'requests'"
        condition = stuck_detector.detect_from_error(error_msg)
        assert condition == StuckCondition.MISSING_DEPENDENCY

    def test_detect_from_error_command_failure(self, stuck_detector: StuckDetector) -> None:
        """Should detect command failure from error message."""
        condition = stuck_detector.detect_from_error("Error: build failed with exit code 1")
        assert condition == StuckCondition.COMMAND_FAILURE

    def test_should_ask_mentor_with_config(self, student_behavior: StudentBehavior) -> None:
        """Should respect config settings for asking mentor."""
        detector = StuckDetector(config=student_behavior)
        assert detector.should_ask_mentor(StuckCondition.TIMEOUT)
        assert detector.should_ask_mentor(StuckCondition.MAX_RETRIES)  # Always ask

        # Test with config that disables asking on timeout
        behavior_no_timeout = StudentBehavior(ask_on_timeout=False)
        detector2 = StuckDetector(config=behavior_no_timeout)
        assert not detector2.should_ask_mentor(StuckCondition.TIMEOUT)
        assert detector2.should_ask_mentor(StuckCondition.MAX_RETRIES)  # Still always ask

    def test_reset_clears_state(self, stuck_detector: StuckDetector) -> None:
        """Reset should clear all state."""
        stuck_detector.record_attempt("Step 1", success=False)
        stuck_detector.stuck_condition = StuckCondition.TIMEOUT

        stuck_detector.reset()

        assert stuck_detector.get_retry_count("Step 1") == 0
        assert stuck_detector.stuck_condition is None


# =============================================================================
# LlmCliError Tests
# =============================================================================


class TestLlmCliError:
    """Tests for LLM CLI error exceptions."""

    def test_llm_cli_error_basic(self) -> None:
        """LlmCliError should store all attributes."""
        error = LlmCliError(
            "CLI failed",
            provider=LlmProvider.CLAUDE,
            command=["claude", "-p", "test"],
            exit_code=1,
            stderr="Some error",
        )
        assert error.provider == LlmProvider.CLAUDE
        assert error.command == ["claude", "-p", "test"]
        assert error.exit_code == 1
        assert error.stderr == "Some error"

    def test_llm_cli_error_str_representation(self) -> None:
        """LlmCliError string should include context."""
        error = LlmCliError(
            "CLI failed",
            provider=LlmProvider.CLAUDE,
            exit_code=1,
            stderr="Error details",
        )
        error_str = str(error)
        assert "CLI failed" in error_str
        assert "claude" in error_str
        assert "1" in error_str
        assert "Error details" in error_str

    def test_llm_timeout_error(self) -> None:
        """LlmTimeoutError should include timeout duration."""
        error = LlmTimeoutError(
            "Timeout occurred",
            timeout_seconds=60,
            provider=LlmProvider.CLAUDE,
        )
        assert error.timeout_seconds == 60
        error_str = str(error)
        assert "60" in error_str

    def test_llm_parse_error(self) -> None:
        """LlmParseError should include parse details."""
        error = LlmParseError(
            "Parse failed",
            raw_output="not json",
            parse_error="Invalid JSON",
            provider=LlmProvider.CLAUDE,
        )
        assert error.raw_output == "not json"
        assert error.parse_error == "Invalid JSON"
        error_str = str(error)
        assert "Invalid JSON" in error_str


# =============================================================================
# LlmCli Tests
# =============================================================================


class TestLlmCli:
    """Tests for the LlmCli class."""

    def test_cli_build_command_claude(self) -> None:
        """Should build correct command for Claude provider."""
        cli = LlmCli(LlmProvider.CLAUDE, timeout_seconds=60)
        command = cli._build_command("test prompt")
        assert command == ["claude", "-p", "test prompt"]

    def test_cli_build_command_codex(self) -> None:
        """Should build correct command for Codex provider."""
        cli = LlmCli(LlmProvider.CODEX, timeout_seconds=60)
        command = cli._build_command("test prompt")
        assert command == ["codex", "--prompt", "test prompt"]

    def test_cli_build_command_gemini(self) -> None:
        """Should build correct command for Gemini provider."""
        cli = LlmCli(LlmProvider.GEMINI, timeout_seconds=60)
        command = cli._build_command("test prompt")
        assert command == ["gemini", "--prompt", "test prompt"]

    @patch("smile_wrappers.student.subprocess.run")
    def test_cli_invoke_success(self, mock_run: MagicMock) -> None:
        """Should return stdout on successful invocation."""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="LLM response",
            stderr="",
        )
        cli = LlmCli(LlmProvider.CLAUDE, timeout_seconds=60)
        result = cli.invoke("test prompt")
        assert result == "LLM response"

    @patch("smile_wrappers.student.subprocess.run")
    def test_cli_invoke_timeout(self, mock_run: MagicMock) -> None:
        """Should raise LlmTimeoutError on timeout."""
        mock_run.side_effect = TimeoutExpired(cmd="claude", timeout=60)

        cli = LlmCli(LlmProvider.CLAUDE, timeout_seconds=60)
        with pytest.raises(LlmTimeoutError) as exc_info:
            cli.invoke("test prompt")
        assert exc_info.value.timeout_seconds == 60

    @patch("smile_wrappers.student.subprocess.run")
    def test_cli_invoke_nonzero_exit(self, mock_run: MagicMock) -> None:
        """Should raise LlmCliError on non-zero exit code."""
        mock_run.return_value = MagicMock(
            returncode=1,
            stdout="",
            stderr="Error occurred",
        )
        cli = LlmCli(LlmProvider.CLAUDE, timeout_seconds=60)
        with pytest.raises(LlmCliError) as exc_info:
            cli.invoke("test prompt")
        assert exc_info.value.exit_code == 1
        assert "Error occurred" in exc_info.value.stderr


# =============================================================================
# JSON Extraction Tests
# =============================================================================


class TestJsonExtraction:
    """Tests for JSON extraction from LLM output."""

    def test_extract_raw_json(self) -> None:
        """Should extract raw JSON without wrappers."""
        json_str = '{"status": "completed"}'
        result = _extract_json_from_output(json_str)
        assert result == '{"status": "completed"}'

    def test_extract_json_from_code_block(self) -> None:
        """Should extract JSON from markdown code block."""
        output = """Here is the result:

```json
{"status": "completed", "summary": "Done"}
```

That's the output."""
        result = _extract_json_from_output(output)
        assert '"status": "completed"' in result

    def test_extract_json_from_plain_code_block(self) -> None:
        """Should extract JSON from plain code block without json specifier."""
        output = """```
{"status": "completed"}
```"""
        result = _extract_json_from_output(output)
        assert '"status": "completed"' in result

    def test_extract_json_whitespace_handling(self) -> None:
        """Should handle whitespace properly."""
        output = """

{"status": "completed"}

  """
        result = _extract_json_from_output(output)
        assert result == '{"status": "completed"}'


# =============================================================================
# Output Parsing Tests
# =============================================================================


class TestOutputParsing:
    """Tests for parsing LLM output to StudentOutput."""

    def test_parse_valid_output(self, valid_student_output_json: str) -> None:
        """Should parse valid JSON output."""
        result = _parse_student_output(valid_student_output_json, LlmProvider.CLAUDE)
        assert result.status == "completed"
        assert result.current_step == "Step 1: Install Python"

    def test_parse_output_from_code_block(self) -> None:
        """Should parse output wrapped in code block."""
        output = """```json
{
    "status": "completed",
    "currentStep": "Step 1",
    "attemptedActions": ["action"],
    "summary": "Done"
}
```"""
        result = _parse_student_output(output, LlmProvider.CLAUDE)
        assert result.status == "completed"

    def test_parse_invalid_json_raises_error(self) -> None:
        """Should raise LlmParseError for invalid JSON."""
        with pytest.raises(LlmParseError) as exc_info:
            _parse_student_output("not json at all", LlmProvider.CLAUDE)
        # The parse_error contains the underlying JSON decode error message
        assert exc_info.value.parse_error is not None

    def test_parse_empty_output_raises_error(self) -> None:
        """Should raise LlmParseError for empty output."""
        with pytest.raises(LlmParseError):
            _parse_student_output("", LlmProvider.CLAUDE)

    def test_parse_invalid_schema_raises_error(self) -> None:
        """Should raise LlmParseError when JSON doesn't match schema."""
        # Missing required fields
        invalid_json = '{"status": "completed"}'
        with pytest.raises(LlmParseError) as exc_info:
            _parse_student_output(invalid_json, LlmProvider.CLAUDE)
        # The exception message (first arg) should mention schema
        assert "schema" in str(exc_info.value.args[0]).lower()


# =============================================================================
# StudentWrapper Tests
# =============================================================================


class TestStudentWrapper:
    """Tests for the StudentWrapper class."""

    def test_wrapper_initialization(self, student_wrapper: StudentWrapper) -> None:
        """StudentWrapper should initialize with all required attributes."""
        assert student_wrapper.config is not None
        assert student_wrapper.provider == LlmProvider.CLAUDE
        assert student_wrapper.tutorial_content != ""
        assert isinstance(student_wrapper.mentor_notes, list)
        assert student_wrapper.iteration == 1
        assert student_wrapper.stuck_condition is None

    def test_wrapper_with_stuck_detector(
        self,
        student_behavior: StudentBehavior,
        tutorial_content: str,
        stuck_detector: StuckDetector,
    ) -> None:
        """StudentWrapper should accept stuck detector."""
        wrapper = StudentWrapper(
            config=student_behavior,
            provider=LlmProvider.CLAUDE,
            tutorial_content=tutorial_content,
            mentor_notes=[],
            iteration=1,
            stuck_detector=stuck_detector,
        )
        assert wrapper.stuck_detector is stuck_detector

    def test_build_prompt(self, student_wrapper: StudentWrapper) -> None:
        """StudentWrapper should build a valid prompt."""
        prompt = student_wrapper._build_prompt()
        assert "Student" in prompt or "tutorial" in prompt.lower()
        assert student_wrapper.tutorial_content in prompt or "Getting Started" in prompt

    @patch("smile_wrappers.student.LlmCli")
    def test_run_success(
        self, mock_llm_cli_class: MagicMock, student_wrapper: StudentWrapper
    ) -> None:
        """StudentWrapper.run should return StudentOutput on success."""
        mock_cli = MagicMock()
        mock_cli.invoke.return_value = json.dumps(
            {
                "status": "completed",
                "currentStep": "Step 1",
                "attemptedActions": ["action"],
                "summary": "Done",
            }
        )
        mock_llm_cli_class.return_value = mock_cli

        result = student_wrapper.run()

        assert result.status == "completed"
        mock_cli.invoke.assert_called_once()

    @patch("smile_wrappers.student.LlmCli")
    def test_run_timeout_with_ask_on_timeout(
        self,
        mock_llm_cli_class: MagicMock,
        tutorial_content: str,
    ) -> None:
        """StudentWrapper.run should return fallback on timeout when configured."""
        mock_cli = MagicMock()
        mock_cli.invoke.side_effect = LlmTimeoutError(
            "Timeout",
            timeout_seconds=60,
            provider=LlmProvider.CLAUDE,
        )
        mock_llm_cli_class.return_value = mock_cli

        behavior = StudentBehavior(ask_on_timeout=True)
        wrapper = StudentWrapper(
            config=behavior,
            provider=LlmProvider.CLAUDE,
            tutorial_content=tutorial_content,
            mentor_notes=[],
            iteration=1,
        )

        result = wrapper.run()

        assert result.status == "ask_mentor"
        assert "timed out" in result.problem.lower()
        assert wrapper.stuck_condition == StuckCondition.TIMEOUT

    @patch("smile_wrappers.student.LlmCli")
    def test_run_timeout_without_ask_on_timeout_raises(
        self,
        mock_llm_cli_class: MagicMock,
        tutorial_content: str,
    ) -> None:
        """StudentWrapper.run should raise on timeout when not configured to ask."""
        mock_cli = MagicMock()
        mock_cli.invoke.side_effect = LlmTimeoutError(
            "Timeout",
            timeout_seconds=60,
            provider=LlmProvider.CLAUDE,
        )
        mock_llm_cli_class.return_value = mock_cli

        behavior = StudentBehavior(ask_on_timeout=False)
        wrapper = StudentWrapper(
            config=behavior,
            provider=LlmProvider.CLAUDE,
            tutorial_content=tutorial_content,
            mentor_notes=[],
            iteration=1,
        )

        with pytest.raises(LlmTimeoutError):
            wrapper.run()

    @patch("smile_wrappers.student.LlmCli")
    def test_run_cli_error_returns_fallback(
        self,
        mock_llm_cli_class: MagicMock,
        tutorial_content: str,
    ) -> None:
        """StudentWrapper.run should return fallback on CLI error."""
        mock_cli = MagicMock()
        mock_cli.invoke.side_effect = LlmCliError(
            "CLI failed",
            provider=LlmProvider.CLAUDE,
            exit_code=1,
            stderr="command not found: pip",
        )
        mock_llm_cli_class.return_value = mock_cli

        behavior = StudentBehavior(ask_on_command_failure=True)
        wrapper = StudentWrapper(
            config=behavior,
            provider=LlmProvider.CLAUDE,
            tutorial_content=tutorial_content,
            mentor_notes=[],
            iteration=1,
        )

        result = wrapper.run()

        assert result.status == "ask_mentor"

    @patch("smile_wrappers.student.LlmCli")
    def test_run_parse_error_retries(
        self,
        mock_llm_cli_class: MagicMock,
        tutorial_content: str,
    ) -> None:
        """StudentWrapper.run should retry on parse errors."""
        mock_cli = MagicMock()
        # First two calls fail, third succeeds
        mock_cli.invoke.side_effect = [
            "not json",
            "still not json",
            json.dumps(
                {
                    "status": "completed",
                    "currentStep": "Step 1",
                    "attemptedActions": ["action"],
                    "summary": "Done",
                }
            ),
        ]
        mock_llm_cli_class.return_value = mock_cli

        behavior = StudentBehavior()
        wrapper = StudentWrapper(
            config=behavior,
            provider=LlmProvider.CLAUDE,
            tutorial_content=tutorial_content,
            mentor_notes=[],
            iteration=1,
        )

        result = wrapper.run()

        assert result.status == "completed"
        assert mock_cli.invoke.call_count == 3

    @patch("smile_wrappers.student.LlmCli")
    def test_run_parse_error_exhausted_returns_fallback(
        self,
        mock_llm_cli_class: MagicMock,
        tutorial_content: str,
    ) -> None:
        """StudentWrapper.run should return fallback when all retries exhausted."""
        mock_cli = MagicMock()
        mock_cli.invoke.return_value = "always invalid json"
        mock_llm_cli_class.return_value = mock_cli

        behavior = StudentBehavior()
        wrapper = StudentWrapper(
            config=behavior,
            provider=LlmProvider.CLAUDE,
            tutorial_content=tutorial_content,
            mentor_notes=[],
            iteration=1,
        )

        result = wrapper.run()

        assert result.status == "ask_mentor"
        assert wrapper.stuck_condition == StuckCondition.PARSE_FAILURE

    def test_create_fallback_output_timeout(self, student_wrapper: StudentWrapper) -> None:
        """Fallback output for timeout should be helpful."""
        error = LlmTimeoutError("Timeout", timeout_seconds=60, provider=LlmProvider.CLAUDE)
        output = student_wrapper._create_fallback_output(error)

        assert output.status == "ask_mentor"
        assert "timed out" in output.problem.lower()
        assert "60" in output.problem

    def test_create_fallback_output_parse_error(self, student_wrapper: StudentWrapper) -> None:
        """Fallback output for parse error should be helpful."""
        error = LlmParseError(
            "Parse failed",
            raw_output="bad json",
            parse_error="Invalid",
            provider=LlmProvider.CLAUDE,
        )
        output = student_wrapper._create_fallback_output(error)

        assert output.status == "ask_mentor"
        assert "parse" in output.problem.lower()

    def test_create_fallback_output_generic_error(self, student_wrapper: StudentWrapper) -> None:
        """Fallback output for generic error should be helpful."""
        error = RuntimeError("Something went wrong")
        output = student_wrapper._create_fallback_output(error)

        assert output.status == "ask_mentor"
        assert "Something went wrong" in output.problem


# =============================================================================
# OrchestratorClient Tests
# =============================================================================


class TestOrchestratorClient:
    """Tests for the OrchestratorClient class."""

    def test_client_initialization(self) -> None:
        """Client should initialize with correct defaults."""
        client = OrchestratorClient()
        assert client.base_url == DEFAULT_ORCHESTRATOR_URL
        assert client.timeout_seconds == 30.0

    def test_client_custom_url(self) -> None:
        """Client should accept custom URL."""
        client = OrchestratorClient(base_url="http://localhost:8080/")
        assert client.base_url == "http://localhost:8080"  # Trailing slash stripped

    def test_calculate_backoff(self) -> None:
        """Client should calculate exponential backoff correctly."""
        client = OrchestratorClient()
        assert client._calculate_backoff(0) == 1.0
        assert client._calculate_backoff(1) == 2.0
        assert client._calculate_backoff(2) == 4.0
        assert client._calculate_backoff(3) == 8.0
        assert client._calculate_backoff(10) == 10.0  # Capped at max

    @patch("smile_wrappers.student.httpx.Client")
    def test_report_student_result_success_continue(self, mock_client_class: MagicMock) -> None:
        """Client should return CONTINUE action on successful report."""
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "acknowledged": True,
            "nextAction": "continue",
        }
        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.return_value = mock_response
        mock_client_class.return_value = mock_client

        client = OrchestratorClient(base_url="http://localhost:3000")
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )
        result = client.report_student_result(output)

        assert result == NextAction.CONTINUE
        mock_client.post.assert_called_once()
        call_args = mock_client.post.call_args
        assert call_args[0][0] == "http://localhost:3000/api/student/result"

    @patch("smile_wrappers.student.httpx.Client")
    def test_report_student_result_success_stop(self, mock_client_class: MagicMock) -> None:
        """Client should return STOP action when orchestrator requests stop."""
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "acknowledged": True,
            "nextAction": "stop",
        }
        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.return_value = mock_response
        mock_client_class.return_value = mock_client

        client = OrchestratorClient(base_url="http://localhost:3000")
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )
        result = client.report_student_result(output)

        assert result == NextAction.STOP

    @patch("smile_wrappers.student.httpx.Client")
    def test_report_student_result_bad_request(self, mock_client_class: MagicMock) -> None:
        """Client should raise error on 400 Bad Request without retrying."""
        mock_response = MagicMock()
        mock_response.status_code = 400
        mock_response.text = "Invalid request"
        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.return_value = mock_response
        mock_client_class.return_value = mock_client

        client = OrchestratorClient(base_url="http://localhost:3000")
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )

        with pytest.raises(OrchestratorCallbackError) as exc_info:
            client.report_student_result(output)

        assert exc_info.value.status_code == 400
        # Should not retry on 400
        assert mock_client.post.call_count == 1

    @patch("smile_wrappers.student.httpx.Client")
    @patch("smile_wrappers.student.time.sleep")
    def test_report_student_result_retries_on_503(
        self, mock_sleep: MagicMock, mock_client_class: MagicMock
    ) -> None:
        """Client should retry on 503 Service Unavailable."""
        mock_response_503 = MagicMock()
        mock_response_503.status_code = 503
        mock_response_503.text = "Service unavailable"

        mock_response_200 = MagicMock()
        mock_response_200.status_code = 200
        mock_response_200.json.return_value = {
            "acknowledged": True,
            "nextAction": "continue",
        }

        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.side_effect = [mock_response_503, mock_response_200]
        mock_client_class.return_value = mock_client

        client = OrchestratorClient(base_url="http://localhost:3000")
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )
        result = client.report_student_result(output)

        assert result == NextAction.CONTINUE
        assert mock_client.post.call_count == 2
        mock_sleep.assert_called_once()

    @patch("smile_wrappers.student.httpx.Client")
    @patch("smile_wrappers.student.time.sleep")
    def test_report_student_result_connection_error(
        self, mock_sleep: MagicMock, mock_client_class: MagicMock
    ) -> None:
        """Client should retry on connection error."""
        # Verify sleep is called for backoff between retries
        _ = mock_sleep  # Mark as used, we need it to prevent actual sleep

        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.side_effect = httpx.ConnectError("Connection refused")
        mock_client_class.return_value = mock_client

        client = OrchestratorClient(base_url="http://localhost:3000")
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )

        with pytest.raises(OrchestratorCallbackError) as exc_info:
            client.report_student_result(output)

        assert "Connection refused" in str(exc_info.value)
        assert mock_client.post.call_count == 3  # Max retries

    @patch("smile_wrappers.student.httpx.Client")
    @patch("smile_wrappers.student.time.sleep")
    def test_report_student_result_timeout_error(
        self, mock_sleep: MagicMock, mock_client_class: MagicMock
    ) -> None:
        """Client should retry on timeout error."""
        _ = mock_sleep

        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.side_effect = httpx.TimeoutException("Request timed out")
        mock_client_class.return_value = mock_client

        client = OrchestratorClient(base_url="http://localhost:3000")
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="Done",
        )

        with pytest.raises(OrchestratorCallbackError) as exc_info:
            client.report_student_result(output)

        assert "timed out" in str(exc_info.value)


# =============================================================================
# File Loading Tests
# =============================================================================


class TestFileLoading:
    """Tests for file loading functions."""

    def test_load_config_from_file(self, tmp_path: Path) -> None:
        """Should load config from JSON file."""
        config_file = tmp_path / "config.json"
        config_file.write_text(
            json.dumps(
                {
                    "llmProvider": "claude",
                    "maxIterations": 5,
                }
            )
        )

        config = _load_config_from_file(config_file)

        assert config.llm_provider == LlmProvider.CLAUDE
        assert config.max_iterations == 5

    def test_load_config_file_not_found(self, tmp_path: Path) -> None:
        """Should raise FileNotFoundError for missing file."""
        with pytest.raises(FileNotFoundError):
            _load_config_from_file(tmp_path / "nonexistent.json")

    def test_load_config_invalid_json(self, tmp_path: Path) -> None:
        """Should raise JSONDecodeError for invalid JSON."""
        config_file = tmp_path / "config.json"
        config_file.write_text("not valid json")

        with pytest.raises(json.JSONDecodeError):
            _load_config_from_file(config_file)

    def test_load_mentor_notes_from_file(self, tmp_path: Path) -> None:
        """Should load mentor notes from JSON file."""
        notes_file = tmp_path / "notes.json"
        notes_file.write_text(json.dumps(["Note 1", "Note 2", "Note 3"]))

        notes = _load_mentor_notes_from_file(notes_file)

        assert notes == ["Note 1", "Note 2", "Note 3"]

    def test_load_mentor_notes_file_not_found(self, tmp_path: Path) -> None:
        """Should return empty list for missing file."""
        notes = _load_mentor_notes_from_file(tmp_path / "nonexistent.json")
        assert notes == []

    def test_load_mentor_notes_non_list(self, tmp_path: Path) -> None:
        """Should return empty list for non-list JSON."""
        notes_file = tmp_path / "notes.json"
        notes_file.write_text(json.dumps({"not": "a list"}))

        notes = _load_mentor_notes_from_file(notes_file)
        assert notes == []

    def test_load_tutorial_content_tutorial_md(self, tmp_path: Path) -> None:
        """Should load tutorial.md by default."""
        tutorial_file = tmp_path / "tutorial.md"
        tutorial_file.write_text("# Tutorial Content")

        content = _load_tutorial_content(tmp_path)

        assert content == "# Tutorial Content"

    def test_load_tutorial_content_readme_md(self, tmp_path: Path) -> None:
        """Should load README.md if tutorial.md not found."""
        readme_file = tmp_path / "README.md"
        readme_file.write_text("# README Content")

        content = _load_tutorial_content(tmp_path)

        assert content == "# README Content"

    def test_load_tutorial_content_any_md(self, tmp_path: Path) -> None:
        """Should load any .md file if standard names not found."""
        md_file = tmp_path / "guide.md"
        md_file.write_text("# Guide Content")

        content = _load_tutorial_content(tmp_path)

        assert content == "# Guide Content"

    def test_load_tutorial_content_not_found(self, tmp_path: Path) -> None:
        """Should raise FileNotFoundError if no markdown found."""
        with pytest.raises(FileNotFoundError):
            _load_tutorial_content(tmp_path)

    def test_load_tutorial_content_priority(self, tmp_path: Path) -> None:
        """Should prioritize tutorial.md over README.md."""
        (tmp_path / "tutorial.md").write_text("Tutorial")
        (tmp_path / "README.md").write_text("README")

        content = _load_tutorial_content(tmp_path)

        assert content == "Tutorial"
