"""Tests for mentor wrapper."""

from __future__ import annotations

import json
from datetime import UTC, datetime
from typing import TYPE_CHECKING
from unittest.mock import MagicMock, patch

import httpx
import pytest
from pydantic import ValidationError

from smile_wrappers.config import Config, LlmProvider
from smile_wrappers.mentor import (
    DEFAULT_MENTOR_TIMEOUT_SECONDS,
    MentorOrchestratorClient,
    MentorResultRequest,
    MentorResultResponse,
    MentorWrapper,
    StuckContext,
    _load_previous_notes_from_file,
    _load_stuck_context_from_file,
    _load_tutorial_content,
)
from smile_wrappers.student import (
    LlmCliError,
    LlmTimeoutError,
    NextAction,
    OrchestratorCallbackError,
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
def stuck_context() -> StuckContext:
    """Sample stuck context."""
    return StuckContext(
        currentStep="Step 2: Create a Virtual Environment",
        problem="Command 'python' not found",
        question="How do I install Python on my system?",
    )


@pytest.fixture
def previous_notes() -> list[str]:
    """Sample previous mentor notes."""
    return [
        "Make sure you have Python 3.8 or higher installed.",
        "Check your PATH environment variable.",
    ]


@pytest.fixture
def mentor_wrapper(
    config: Config,
    tutorial_content: str,
    stuck_context: StuckContext,
    previous_notes: list[str],
) -> MentorWrapper:
    """Create a MentorWrapper instance for testing."""
    return MentorWrapper(
        config=config,
        tutorial_content=tutorial_content,
        current_step=stuck_context.current_step,
        problem=stuck_context.problem,
        question=stuck_context.question,
        previous_notes=previous_notes,
    )


# =============================================================================
# StuckContext Model Tests
# =============================================================================


class TestStuckContext:
    """Tests for the StuckContext model."""

    def test_stuck_context_from_camel_case(self) -> None:
        """StuckContext should accept camelCase field names."""
        context = StuckContext(
            currentStep="Step 1",
            problem="Error occurred",
            question="What should I do?",
        )
        assert context.current_step == "Step 1"
        assert context.problem == "Error occurred"
        assert context.question == "What should I do?"

    def test_stuck_context_from_snake_case(self) -> None:
        """StuckContext should accept snake_case field names."""
        context = StuckContext(
            current_step="Step 1",
            problem="Error occurred",
            question="What should I do?",
        )
        assert context.current_step == "Step 1"

    def test_stuck_context_validation(self) -> None:
        """StuckContext should validate required fields."""
        with pytest.raises(ValidationError):
            StuckContext(currentStep="Step 1")  # Missing problem and question

    def test_stuck_context_serialization(self) -> None:
        """StuckContext should serialize with camelCase aliases."""
        context = StuckContext(
            currentStep="Step 1",
            problem="Error occurred",
            question="What should I do?",
        )
        data = context.model_dump(by_alias=True)
        assert "currentStep" in data
        assert "current_step" not in data


# =============================================================================
# MentorResultRequest Model Tests
# =============================================================================


class TestMentorResultRequest:
    """Tests for the MentorResultRequest model."""

    def test_request_from_camel_case(self) -> None:
        """MentorResultRequest should accept camelCase field names."""
        timestamp = datetime.now(UTC)
        request = MentorResultRequest(
            mentorOutput="Try checking your PATH",
            timestamp=timestamp,
        )
        assert request.mentor_output == "Try checking your PATH"
        assert request.timestamp == timestamp

    def test_request_from_snake_case(self) -> None:
        """MentorResultRequest should accept snake_case field names."""
        timestamp = datetime.now(UTC)
        request = MentorResultRequest(
            mentor_output="Try checking your PATH",
            timestamp=timestamp,
        )
        assert request.mentor_output == "Try checking your PATH"

    def test_request_serialization(self) -> None:
        """MentorResultRequest should serialize with camelCase aliases."""
        timestamp = datetime.now(UTC)
        request = MentorResultRequest(
            mentorOutput="Try checking your PATH",
            timestamp=timestamp,
        )
        data = request.model_dump(by_alias=True, mode="json")
        assert "mentorOutput" in data
        assert "mentor_output" not in data


# =============================================================================
# MentorResultResponse Model Tests
# =============================================================================


class TestMentorResultResponse:
    """Tests for the MentorResultResponse model."""

    def test_response_from_camel_case(self) -> None:
        """MentorResultResponse should accept camelCase field names."""
        response = MentorResultResponse(
            acknowledged=True,
            nextAction="continue",
        )
        assert response.acknowledged is True
        assert response.next_action == NextAction.CONTINUE

    def test_response_from_snake_case(self) -> None:
        """MentorResultResponse should accept snake_case field names."""
        response = MentorResultResponse(
            acknowledged=True,
            next_action="stop",
        )
        assert response.next_action == NextAction.STOP

    def test_response_validation_invalid_action(self) -> None:
        """MentorResultResponse should validate next_action values."""
        with pytest.raises(ValidationError):
            MentorResultResponse(
                acknowledged=True,
                nextAction="invalid",
            )


# =============================================================================
# MentorWrapper Tests
# =============================================================================


class TestMentorWrapper:
    """Tests for the MentorWrapper class."""

    def test_wrapper_initialization(self, mentor_wrapper: MentorWrapper) -> None:
        """MentorWrapper should initialize with all required attributes."""
        assert mentor_wrapper.config is not None
        assert mentor_wrapper.tutorial_content != ""
        assert mentor_wrapper.current_step != ""
        assert mentor_wrapper.problem != ""
        assert mentor_wrapper.question != ""
        assert isinstance(mentor_wrapper.previous_notes, list)
        assert mentor_wrapper.timeout_seconds == DEFAULT_MENTOR_TIMEOUT_SECONDS

    def test_wrapper_custom_timeout(self, config: Config, tutorial_content: str) -> None:
        """MentorWrapper should accept custom timeout."""
        wrapper = MentorWrapper(
            config=config,
            tutorial_content=tutorial_content,
            current_step="Step 1",
            problem="Error",
            question="How to fix?",
            previous_notes=[],
            timeout_seconds=30,
        )
        assert wrapper.timeout_seconds == 30

    def test_build_prompt(self, mentor_wrapper: MentorWrapper) -> None:
        """MentorWrapper should build a valid prompt."""
        prompt = mentor_wrapper._build_prompt()
        assert "Tutorial Mentor" in prompt
        assert mentor_wrapper.current_step in prompt
        assert mentor_wrapper.problem in prompt
        assert mentor_wrapper.question in prompt
        # Check that previous notes are included
        for note in mentor_wrapper.previous_notes:
            assert note in prompt

    @patch("smile_wrappers.mentor.LlmCli")
    def test_run_success(
        self, mock_llm_cli_class: MagicMock, mentor_wrapper: MentorWrapper
    ) -> None:
        """MentorWrapper.run should return LLM output on success."""
        mock_cli = MagicMock()
        mock_cli.invoke.return_value = "  Here's how to fix the issue...  \n"
        mock_llm_cli_class.return_value = mock_cli

        result = mentor_wrapper.run()

        assert result == "Here's how to fix the issue..."
        mock_cli.invoke.assert_called_once()

    @patch("smile_wrappers.mentor.LlmCli")
    def test_run_timeout_returns_fallback(
        self, mock_llm_cli_class: MagicMock, mentor_wrapper: MentorWrapper
    ) -> None:
        """MentorWrapper.run should return fallback note on timeout."""
        mock_cli = MagicMock()
        mock_cli.invoke.side_effect = LlmTimeoutError(
            "Timeout",
            timeout_seconds=120,
            provider=LlmProvider.CLAUDE,
        )
        mock_llm_cli_class.return_value = mock_cli

        result = mentor_wrapper.run()

        assert "timed out" in result.lower()
        assert "120" in result
        assert "prerequisites" in result.lower()

    @patch("smile_wrappers.mentor.LlmCli")
    def test_run_cli_error_returns_fallback(
        self, mock_llm_cli_class: MagicMock, mentor_wrapper: MentorWrapper
    ) -> None:
        """MentorWrapper.run should return fallback note on CLI error."""
        mock_cli = MagicMock()
        mock_cli.invoke.side_effect = LlmCliError(
            "CLI failed",
            provider=LlmProvider.CLAUDE,
            exit_code=1,
        )
        mock_llm_cli_class.return_value = mock_cli

        result = mentor_wrapper.run()

        assert "technical issue" in result.lower()
        assert "error message" in result.lower()

    def test_fallback_note_timeout(self, mentor_wrapper: MentorWrapper) -> None:
        """Fallback note for timeout should be helpful."""
        error = LlmTimeoutError("Timeout", timeout_seconds=60, provider=LlmProvider.CLAUDE)
        note = mentor_wrapper._create_fallback_note(error)

        assert "timed out" in note.lower()
        assert "60" in note
        assert "prerequisites" in note.lower()
        assert "error message" in note.lower()
        assert "smaller steps" in note.lower()

    def test_fallback_note_cli_error(self, mentor_wrapper: MentorWrapper) -> None:
        """Fallback note for CLI error should be helpful."""
        error = LlmCliError("CLI failed", provider=LlmProvider.CLAUDE, exit_code=1)
        note = mentor_wrapper._create_fallback_note(error)

        assert "technical issue" in note.lower()
        assert "LlmCliError" in note
        assert "error message" in note.lower()

    def test_fallback_note_generic_error(self, mentor_wrapper: MentorWrapper) -> None:
        """Fallback note for generic error should be helpful."""
        error = RuntimeError("Something went wrong")
        note = mentor_wrapper._create_fallback_note(error)

        assert "Something went wrong" in note
        assert "debugging strategies" in note.lower()


# =============================================================================
# MentorOrchestratorClient Tests
# =============================================================================


class TestMentorOrchestratorClient:
    """Tests for the MentorOrchestratorClient class."""

    def test_client_initialization(self) -> None:
        """Client should initialize with correct defaults."""
        client = MentorOrchestratorClient()
        assert client.base_url == "http://host.docker.internal:3000"
        assert client.timeout_seconds == 30.0

    def test_client_custom_url(self) -> None:
        """Client should accept custom URL."""
        client = MentorOrchestratorClient(base_url="http://localhost:8080/")
        assert client.base_url == "http://localhost:8080"  # Trailing slash stripped

    def test_calculate_backoff(self) -> None:
        """Client should calculate exponential backoff correctly."""
        client = MentorOrchestratorClient()
        assert client._calculate_backoff(0) == 1.0
        assert client._calculate_backoff(1) == 2.0
        assert client._calculate_backoff(2) == 4.0
        assert client._calculate_backoff(3) == 8.0
        assert client._calculate_backoff(10) == 10.0  # Capped at max

    @patch("smile_wrappers.mentor.httpx.Client")
    def test_report_mentor_result_success_continue(self, mock_client_class: MagicMock) -> None:
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

        client = MentorOrchestratorClient(base_url="http://localhost:3000")
        result = client.report_mentor_result("Here is my guidance")

        assert result == NextAction.CONTINUE
        mock_client.post.assert_called_once()
        call_args = mock_client.post.call_args
        assert call_args[0][0] == "http://localhost:3000/api/mentor/result"

    @patch("smile_wrappers.mentor.httpx.Client")
    def test_report_mentor_result_success_stop(self, mock_client_class: MagicMock) -> None:
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

        client = MentorOrchestratorClient(base_url="http://localhost:3000")
        result = client.report_mentor_result("Here is my guidance")

        assert result == NextAction.STOP

    @patch("smile_wrappers.mentor.httpx.Client")
    def test_report_mentor_result_bad_request(self, mock_client_class: MagicMock) -> None:
        """Client should raise error on 400 Bad Request without retrying."""
        mock_response = MagicMock()
        mock_response.status_code = 400
        mock_response.text = "Invalid request"
        mock_client = MagicMock()
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client.post.return_value = mock_response
        mock_client_class.return_value = mock_client

        client = MentorOrchestratorClient(base_url="http://localhost:3000")

        with pytest.raises(OrchestratorCallbackError) as exc_info:
            client.report_mentor_result("output")

        assert exc_info.value.status_code == 400
        # Should not retry on 400
        assert mock_client.post.call_count == 1

    @patch("smile_wrappers.mentor.httpx.Client")
    @patch("smile_wrappers.mentor.time.sleep")
    def test_report_mentor_result_retries_on_503(
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

        client = MentorOrchestratorClient(base_url="http://localhost:3000")
        result = client.report_mentor_result("output")

        assert result == NextAction.CONTINUE
        assert mock_client.post.call_count == 2
        mock_sleep.assert_called_once()

    @patch("smile_wrappers.mentor.httpx.Client")
    @patch("smile_wrappers.mentor.time.sleep")
    def test_report_mentor_result_connection_error(
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

        client = MentorOrchestratorClient(base_url="http://localhost:3000")

        with pytest.raises(OrchestratorCallbackError) as exc_info:
            client.report_mentor_result("output")

        assert "Connection refused" in str(exc_info.value)
        assert mock_client.post.call_count == 3  # Max retries


# =============================================================================
# File Loading Tests
# =============================================================================


class TestFileLoading:
    """Tests for file loading functions."""

    def test_load_stuck_context_from_file(self, tmp_path: Path) -> None:
        """Should load stuck context from JSON file."""
        context_file = tmp_path / "stuck_context.json"
        context_file.write_text(
            json.dumps(
                {
                    "currentStep": "Step 1",
                    "problem": "Error occurred",
                    "question": "How to fix?",
                }
            )
        )

        context = _load_stuck_context_from_file(context_file)

        assert context.current_step == "Step 1"
        assert context.problem == "Error occurred"
        assert context.question == "How to fix?"

    def test_load_stuck_context_file_not_found(self, tmp_path: Path) -> None:
        """Should raise FileNotFoundError for missing file."""
        with pytest.raises(FileNotFoundError):
            _load_stuck_context_from_file(tmp_path / "nonexistent.json")

    def test_load_stuck_context_invalid_json(self, tmp_path: Path) -> None:
        """Should raise JSONDecodeError for invalid JSON."""
        context_file = tmp_path / "stuck_context.json"
        context_file.write_text("not valid json")

        with pytest.raises(json.JSONDecodeError):
            _load_stuck_context_from_file(context_file)

    def test_load_previous_notes_from_file(self, tmp_path: Path) -> None:
        """Should load previous notes from JSON file."""
        notes_file = tmp_path / "notes.json"
        notes_file.write_text(json.dumps(["Note 1", "Note 2", "Note 3"]))

        notes = _load_previous_notes_from_file(notes_file)

        assert notes == ["Note 1", "Note 2", "Note 3"]

    def test_load_previous_notes_file_not_found(self, tmp_path: Path) -> None:
        """Should return empty list for missing file."""
        notes = _load_previous_notes_from_file(tmp_path / "nonexistent.json")
        assert notes == []

    def test_load_previous_notes_non_list(self, tmp_path: Path) -> None:
        """Should return empty list for non-list JSON."""
        notes_file = tmp_path / "notes.json"
        notes_file.write_text(json.dumps({"not": "a list"}))

        notes = _load_previous_notes_from_file(notes_file)
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
