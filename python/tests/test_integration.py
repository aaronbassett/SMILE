"""Integration tests for the SMILE wrappers with mock LLM CLI.

These tests validate the full wrapper flow using a mock LLM CLI script
that returns predefined responses. This allows testing without real
API keys or network access.
"""

import json
import os
import shutil
import stat
import subprocess
import tempfile
from collections.abc import Generator
from pathlib import Path

import pytest

from smile_wrappers.config import Config, LlmProvider, PatienceLevel, StudentBehavior
from smile_wrappers.output import StudentOutput
from smile_wrappers.student import (
    LlmCli,
    StuckCondition,
    StuckDetector,
    _extract_json_from_output,
    _parse_student_output,
)

# Path to fixtures
FIXTURES_DIR = Path(__file__).parent.parent.parent / "tests" / "integration" / "fixtures"
MOCK_CLI_DIR = FIXTURES_DIR / "mock-cli"
SCENARIOS_DIR = MOCK_CLI_DIR / "scenarios"


@pytest.fixture
def temp_dir() -> Generator[Path, None, None]:
    """Create a temporary directory for test files."""
    tmp = Path(tempfile.mkdtemp())
    yield tmp
    shutil.rmtree(tmp)


@pytest.fixture
def mock_cli_path(temp_dir: Path) -> Path:
    """Create a mock CLI executable that returns predefined responses."""
    mock_script = temp_dir / "claude"
    mock_script.write_text('''#!/usr/bin/env python3
"""Mock claude CLI for testing."""
import json
import os
import sys

# Get scenario from environment
scenario_file = os.environ.get("MOCK_SCENARIO_FILE")
if not scenario_file:
    print(json.dumps({
        "status": "completed",
        "current_step": "Test",
        "attempted_actions": ["test"],
        "summary": "Mock response"
    }))
    sys.exit(0)

# Load scenario
with open(scenario_file) as f:
    scenario = json.load(f)

# Get response index from state file
state_file = scenario_file.replace(".json", ".state")
try:
    with open(state_file) as f:
        index = int(f.read().strip())
except (FileNotFoundError, ValueError):
    index = 0

# Get response
responses = scenario.get("responses", [])
if not responses:
    print(json.dumps({
        "status": "completed",
        "current_step": "Test",
        "attempted_actions": ["test"],
        "summary": "No responses in scenario"
    }))
    sys.exit(0)

response = responses[index % len(responses)]

# Update state
with open(state_file, "w") as f:
    f.write(str(index + 1))

# Output response
print(json.dumps(response, indent=2))
''')
    # Make executable
    mock_script.chmod(mock_script.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return mock_script


@pytest.fixture
def missing_npm_scenario() -> dict:
    """Scenario where student encounters missing npm."""
    return {
        "description": "Student hits npm not found",
        "responses": [
            {
                "status": "ask_mentor",
                "currentStep": "Step 2: Initialize the Project",
                "attemptedActions": ["mkdir my-counter", "cd my-counter", "npm init -y"],
                "problem": "Command not found: npm",
                "questionForMentor": "How do I install npm?",
                "summary": "Cannot proceed without npm",
                "filesCreated": ["my-counter/"],
                "commandsRun": ["mkdir my-counter", "npm init -y"],
            }
        ],
    }


@pytest.fixture
def completion_scenario() -> dict:
    """Scenario where student completes successfully."""
    return {
        "description": "Student completes tutorial",
        "responses": [
            {
                "status": "completed",
                "currentStep": "Step 6: Install Globally",
                "attemptedActions": ["All steps completed"],
                "summary": "Tutorial completed successfully",
                "filesCreated": ["my-counter/", "counter.js"],
                "commandsRun": ["npm init", "npm link"],
            }
        ],
    }


class TestMockCliScenarios:
    """Tests using mock CLI with different scenarios."""

    def test_mock_cli_returns_scenario_response(
        self,
        temp_dir: Path,
        mock_cli_path: Path,
        missing_npm_scenario: dict,
    ) -> None:
        """Mock CLI should return the scenario response."""
        # Write scenario file
        scenario_file = temp_dir / "test_scenario.json"
        scenario_file.write_text(json.dumps(missing_npm_scenario))

        # Set up environment
        env = os.environ.copy()
        env["MOCK_SCENARIO_FILE"] = str(scenario_file)
        env["PATH"] = f"{temp_dir}:{env.get('PATH', '')}"

        # Run the mock CLI
        result = subprocess.run(
            [str(mock_cli_path), "-p", "test prompt"],
            capture_output=True,
            text=True,
            env=env,
            check=True,
        )

        # Parse and verify
        output = json.loads(result.stdout)
        assert output["status"] == "ask_mentor"
        assert "npm" in output["problem"].lower()

    def test_mock_cli_increments_state(
        self,
        temp_dir: Path,
        mock_cli_path: Path,
    ) -> None:
        """Mock CLI should cycle through responses."""
        # Create scenario with multiple responses
        scenario = {
            "responses": [
                {
                    "status": "ask_mentor",
                    "currentStep": "1",
                    "attemptedActions": [],
                    "summary": "First",
                },
                {
                    "status": "ask_mentor",
                    "currentStep": "2",
                    "attemptedActions": [],
                    "summary": "Second",
                },
                {
                    "status": "completed",
                    "currentStep": "3",
                    "attemptedActions": [],
                    "summary": "Third",
                },
            ]
        }

        scenario_file = temp_dir / "multi_scenario.json"
        scenario_file.write_text(json.dumps(scenario))

        env = os.environ.copy()
        env["MOCK_SCENARIO_FILE"] = str(scenario_file)
        env["PATH"] = f"{temp_dir}:{env.get('PATH', '')}"

        # Run three times
        for expected_step in ["1", "2", "3"]:
            result = subprocess.run(
                [str(mock_cli_path), "-p", "test"],
                capture_output=True,
                text=True,
                env=env,
                check=True,
            )
            output = json.loads(result.stdout)
            assert output["currentStep"] == expected_step


class TestStudentWrapperWithMock:
    """Tests for StudentWrapper using mocked subprocess."""

    def test_wrapper_parses_ask_mentor_response(
        self,
        missing_npm_scenario: dict,
    ) -> None:
        """Wrapper should correctly parse ask_mentor response."""
        # Get the response directly
        response = missing_npm_scenario["responses"][0]

        # Create mock output JSON
        mock_output = json.dumps(response)

        # Parse it
        result = _parse_student_output(mock_output, LlmProvider.CLAUDE)

        assert result.status == "ask_mentor"
        assert result.current_step == "Step 2: Initialize the Project"
        assert result.problem == "Command not found: npm"
        assert result.question_for_mentor == "How do I install npm?"

    def test_wrapper_parses_completed_response(
        self,
        completion_scenario: dict,
    ) -> None:
        """Wrapper should correctly parse completed response."""
        response = completion_scenario["responses"][0]
        mock_output = json.dumps(response)

        result = _parse_student_output(mock_output, LlmProvider.CLAUDE)

        assert result.status == "completed"
        assert "completed" in result.summary.lower()

    def test_wrapper_handles_json_in_code_block(self) -> None:
        """Wrapper should extract JSON from markdown code blocks."""
        response = {
            "status": "completed",
            "currentStep": "Final Step",
            "attemptedActions": ["done"],
            "summary": "All done",
        }

        # Wrap in markdown code block
        wrapped = f"Here is my output:\n\n```json\n{json.dumps(response)}\n```\n\nDone!"

        result = _parse_student_output(wrapped, LlmProvider.CLAUDE)
        assert result.status == "completed"

    def test_stuck_detector_classifies_missing_dependency(self) -> None:
        """StuckDetector should identify missing dependency from output."""
        config = StudentBehavior()
        detector = StuckDetector(config)

        output = StudentOutput(
            status="ask_mentor",
            current_step="Install packages",
            attempted_actions=["npm install"],
            problem="Command not found: npm",
            question_for_mentor="How do I install npm?",
            summary="Cannot proceed",
        )

        condition = detector.classify_output(output)
        assert condition == StuckCondition.MISSING_DEPENDENCY

    def test_stuck_detector_classifies_ambiguous_instruction(self) -> None:
        """StuckDetector should identify ambiguous instruction."""
        config = StudentBehavior()
        detector = StuckDetector(config)

        output = StudentOutput(
            status="ask_mentor",
            current_step="Configure project",
            attempted_actions=["read instructions"],
            problem="Instructions are unclear about which file to edit",
            question_for_mentor="Which configuration file should I update?",
            summary="Stuck on configuration",
        )

        condition = detector.classify_output(output)
        assert condition == StuckCondition.AMBIGUOUS_INSTRUCTION

    def test_stuck_detector_tracks_retries(self) -> None:
        """StuckDetector should track failed attempts and trigger max_retries."""
        config = StudentBehavior(max_retries_before_help=3)
        detector = StuckDetector(config)

        step = "Install dependencies"

        # First two failures should not trigger
        assert not detector.record_attempt(step, success=False)
        assert not detector.record_attempt(step, success=False)

        # Third failure should trigger
        assert detector.record_attempt(step, success=False)
        assert detector.stuck_condition == StuckCondition.MAX_RETRIES

    def test_stuck_detector_resets_on_success(self) -> None:
        """StuckDetector should reset count when step succeeds."""
        config = StudentBehavior(max_retries_before_help=3)
        detector = StuckDetector(config)

        step = "Install dependencies"

        # Fail twice
        detector.record_attempt(step, success=False)
        detector.record_attempt(step, success=False)
        assert detector.get_retry_count(step) == 2

        # Success resets
        detector.record_attempt(step, success=True)
        assert detector.get_retry_count(step) == 0


class TestJsonExtraction:
    """Tests for JSON extraction from LLM output."""

    def test_extract_plain_json(self) -> None:
        """Should return plain JSON as-is."""
        json_str = '{"status": "completed"}'
        assert _extract_json_from_output(json_str) == '{"status": "completed"}'

    def test_extract_from_code_block(self) -> None:
        """Should extract JSON from code blocks."""
        wrapped = '```json\n{"status": "completed"}\n```'
        assert '"status": "completed"' in _extract_json_from_output(wrapped)

    def test_extract_from_generic_code_block(self) -> None:
        """Should extract from code blocks without language specifier."""
        wrapped = '```\n{"status": "completed"}\n```'
        assert '"status": "completed"' in _extract_json_from_output(wrapped)

    def test_extract_with_surrounding_text(self) -> None:
        """Should extract from code blocks with surrounding text."""
        wrapped = 'Here is the output:\n```json\n{"status": "completed"}\n```\nDone!'
        assert '"status": "completed"' in _extract_json_from_output(wrapped)

    def test_extract_last_code_block(self) -> None:
        """Should use last code block when multiple exist."""
        wrapped = '```json\n{"status": "old"}\n```\n\n```json\n{"status": "new"}\n```'
        result = _extract_json_from_output(wrapped)
        assert '"status": "new"' in result


class TestStudentOutputValidation:
    """Tests for StudentOutput model validation."""

    def test_minimal_completed_output(self) -> None:
        """Minimal required fields for completed status."""
        output = StudentOutput(
            status="completed",
            current_step="Final",
            attempted_actions=["done"],
            summary="Complete",
        )
        assert output.status == "completed"
        assert output.problem is None
        assert output.question_for_mentor is None

    def test_ask_mentor_requires_question(self) -> None:
        """ask_mentor status should have question_for_mentor."""
        output = StudentOutput(
            status="ask_mentor",
            current_step="Step 1",
            attempted_actions=["try"],
            problem="Something failed",
            question_for_mentor="How do I fix this?",
            summary="Stuck",
        )
        assert output.status == "ask_mentor"
        assert output.question_for_mentor is not None

    def test_cannot_complete_requires_reason(self) -> None:
        """cannot_complete status should have reason."""
        output = StudentOutput(
            status="cannot_complete",
            current_step="Step 1",
            attempted_actions=["try"],
            reason="Unrecoverable error",
            summary="Failed",
        )
        assert output.status == "cannot_complete"
        assert output.reason is not None

    def test_camelcase_aliases(self) -> None:
        """Should accept camelCase field names (from LLM output)."""
        data = {
            "status": "completed",
            "currentStep": "Final",
            "attemptedActions": ["done"],
            "summary": "Complete",
            "filesCreated": ["file.txt"],
            "commandsRun": ["echo hi"],
        }
        output = StudentOutput.model_validate(data)
        assert output.current_step == "Final"
        assert output.files_created == ["file.txt"]
        assert output.commands_run == ["echo hi"]


class TestLlmCliBuilder:
    """Tests for LLM CLI command building."""

    def test_claude_command(self) -> None:
        """Claude provider should use -p flag."""
        cli = LlmCli(LlmProvider.CLAUDE, timeout_seconds=60)
        cmd = cli._build_command("test prompt")
        assert cmd == ["claude", "-p", "test prompt"]

    def test_codex_command(self) -> None:
        """Codex provider should use --prompt flag."""
        cli = LlmCli(LlmProvider.CODEX, timeout_seconds=60)
        cmd = cli._build_command("test prompt")
        assert cmd == ["codex", "--prompt", "test prompt"]

    def test_gemini_command(self) -> None:
        """Gemini provider should use --prompt flag."""
        cli = LlmCli(LlmProvider.GEMINI, timeout_seconds=60)
        cmd = cli._build_command("test prompt")
        assert cmd == ["gemini", "--prompt", "test prompt"]


class TestConfigLoading:
    """Tests for configuration loading."""

    def test_default_config(self) -> None:
        """Config should have sensible defaults."""
        config = Config()
        assert config.llm_provider == LlmProvider.CLAUDE
        assert config.max_iterations == 10
        assert config.timeout == 1800
        assert config.student_behavior.max_retries_before_help == 3
        assert config.student_behavior.patience_level == PatienceLevel.LOW

    def test_config_from_dict(self) -> None:
        """Config should load from dictionary."""
        data = {
            "llmProvider": "gemini",
            "maxIterations": 5,
            "studentBehavior": {
                "maxRetriesBeforeHelp": 2,
                "patienceLevel": "high",
            },
        }
        config = Config.model_validate(data)
        assert config.llm_provider == LlmProvider.GEMINI
        assert config.max_iterations == 5
        assert config.student_behavior.max_retries_before_help == 2
        assert config.student_behavior.patience_level == PatienceLevel.HIGH

    def test_config_unknown_fields_ignored(self) -> None:
        """Unknown fields should be ignored (forward compatibility)."""
        data = {
            "llmProvider": "claude",
            "unknownField": "should be ignored",
            "studentBehavior": {
                "maxRetriesBeforeHelp": 2,
                "futureOption": True,
            },
        }
        config = Config.model_validate(data)
        assert config.llm_provider == LlmProvider.CLAUDE
        assert config.student_behavior.max_retries_before_help == 2
