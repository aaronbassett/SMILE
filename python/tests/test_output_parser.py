"""Tests for the OutputParser class and related utilities."""

import pytest

from smile_wrappers.output import (
    OutputParseError,
    OutputParser,
    StudentOutput,
    ValidationWarning,
    validate_student_output,
)


# Test fixtures
@pytest.fixture
def parser() -> OutputParser:
    """Create a parser with verbose mode disabled for cleaner test output."""
    return OutputParser(verbose=False)


@pytest.fixture
def valid_json() -> str:
    """A valid JSON string representing a StudentOutput."""
    return """{
        "status": "completed",
        "current_step": "Step 1: Install dependencies",
        "attempted_actions": ["npm install", "npm run build"],
        "summary": "Successfully installed all dependencies"
    }"""


@pytest.fixture
def ask_mentor_json() -> str:
    """A valid JSON string for ask_mentor status."""
    return """{
        "status": "ask_mentor",
        "current_step": "Step 2: Configure database",
        "attempted_actions": ["Create config file", "Set env vars"],
        "problem": "Connection refused to database",
        "question_for_mentor": "How do I configure the database connection string?",
        "summary": "Unable to connect to database"
    }"""


@pytest.fixture
def cannot_complete_json() -> str:
    """A valid JSON string for cannot_complete status."""
    return """{
        "status": "cannot_complete",
        "current_step": "Step 3: Deploy application",
        "attempted_actions": ["docker build"],
        "reason": "Missing AWS credentials",
        "summary": "Deployment failed due to missing credentials"
    }"""


class TestOutputParserDirectParse:
    """Tests for direct JSON parsing."""

    def test_parse_valid_json(self, parser: OutputParser, valid_json: str) -> None:
        """Parser should handle valid JSON directly."""
        result = parser.parse(valid_json)
        assert result.status == "completed"
        assert result.current_step == "Step 1: Install dependencies"
        assert len(result.attempted_actions) == 2
        assert result.summary == "Successfully installed all dependencies"

    def test_parse_with_whitespace(self, parser: OutputParser, valid_json: str) -> None:
        """Parser should handle JSON with leading/trailing whitespace."""
        result = parser.parse(f"   \n\n{valid_json}\n   ")
        assert result.status == "completed"

    def test_parse_compact_json(self, parser: OutputParser) -> None:
        """Parser should handle compact JSON without pretty-printing."""
        json_str = (
            '{"status":"completed","current_step":"Step 1",'
            '"attempted_actions":["action"],"summary":"Done"}'
        )
        result = parser.parse(json_str)
        assert result.status == "completed"


class TestOutputParserCodeBlockExtraction:
    """Tests for markdown code block extraction."""

    def test_parse_json_code_block(self, parser: OutputParser, valid_json: str) -> None:
        """Parser should extract JSON from ```json code blocks."""
        wrapped = f"```json\n{valid_json}\n```"
        result = parser.parse(wrapped)
        assert result.status == "completed"

    def test_parse_generic_code_block(self, parser: OutputParser, valid_json: str) -> None:
        """Parser should extract JSON from ``` code blocks without language specifier."""
        wrapped = f"```\n{valid_json}\n```"
        result = parser.parse(wrapped)
        assert result.status == "completed"

    def test_parse_code_block_with_surrounding_text(
        self, parser: OutputParser, valid_json: str
    ) -> None:
        """Parser should extract JSON from code blocks with surrounding text."""
        wrapped = (
            f"Here is the output:\n\n```json\n{valid_json}\n```\n\n"
            f"Let me know if you need anything else."
        )
        result = parser.parse(wrapped)
        assert result.status == "completed"

    def test_parse_multiple_code_blocks_uses_last(self, parser: OutputParser) -> None:
        """Parser should use the last code block when multiple exist."""
        first_json = (
            '{"status": "ask_mentor", "current_step": "Old", '
            '"attempted_actions": [], "summary": "Old"}'
        )
        second_json = (
            '{"status": "completed", "current_step": "New", '
            '"attempted_actions": ["action"], "summary": "New"}'
        )
        wrapped = (
            f"First attempt:\n```json\n{first_json}\n```\n\n"
            f"Final answer:\n```json\n{second_json}\n```"
        )
        result = parser.parse(wrapped)
        assert result.status == "completed"
        assert result.current_step == "New"


class TestOutputParserJsonObjectSearch:
    """Tests for JSON object search in text."""

    def test_parse_json_in_text(self, parser: OutputParser, valid_json: str) -> None:
        """Parser should find JSON objects embedded in text."""
        text = f"The student produced the following output: {valid_json} That's all."
        result = parser.parse(text)
        assert result.status == "completed"

    def test_parse_json_with_nested_objects(self, parser: OutputParser) -> None:
        """Parser should handle JSON with nested objects."""
        json_str = """{
            "status": "completed",
            "current_step": "Step 1",
            "attempted_actions": ["action"],
            "summary": "Done with {nested} braces"
        }"""
        result = parser.parse(json_str)
        assert result.status == "completed"
        assert "{nested}" in result.summary

    def test_parse_json_with_escaped_quotes(self, parser: OutputParser) -> None:
        """Parser should handle JSON with escaped quotes in strings."""
        json_str = (
            r'{"status": "completed", "current_step": "Test \"quoted\" step", '
            r'"attempted_actions": ["action"], "summary": "Done"}'
        )
        result = parser.parse(json_str)
        assert result.status == "completed"
        assert '"quoted"' in result.current_step


class TestOutputParserFieldExtraction:
    """Tests for field extraction recovery strategy."""

    def test_parse_malformed_json_with_extractable_fields(self, parser: OutputParser) -> None:
        """Parser should extract fields from partially valid JSON."""
        # JSON with a trailing comma issue
        malformed = """
        Here's what I found:
        "status": "completed",
        "current_step": "Step 1",
        "summary": "All done"
        Some other text
        """
        result = parser.parse(malformed)
        assert result.status == "completed"
        assert result.current_step == "Step 1"
        assert result.summary == "All done"


class TestOutputParserErrors:
    """Tests for error handling."""

    def test_parse_empty_string_raises_error(self, parser: OutputParser) -> None:
        """Parser should raise OutputParseError for empty input."""
        with pytest.raises(OutputParseError) as exc_info:
            parser.parse("")
        assert "Empty output" in str(exc_info.value)
        assert exc_info.value.raw_output == ""

    def test_parse_whitespace_only_raises_error(self, parser: OutputParser) -> None:
        """Parser should raise OutputParseError for whitespace-only input."""
        with pytest.raises(OutputParseError) as exc_info:
            parser.parse("   \n\t  ")
        assert "Empty output" in str(exc_info.value)

    def test_parse_invalid_json_raises_error(self, parser: OutputParser) -> None:
        """Parser should raise OutputParseError for unparseable content."""
        with pytest.raises(OutputParseError) as exc_info:
            parser.parse("This is not JSON at all")
        assert "all recovery strategies" in str(exc_info.value)
        assert exc_info.value.raw_output == "This is not JSON at all"

    def test_parse_missing_required_fields_raises_error(self, parser: OutputParser) -> None:
        """Parser should raise error when required fields are missing."""
        incomplete_json = '{"status": "completed"}'
        with pytest.raises(OutputParseError):
            parser.parse(incomplete_json)

    def test_parse_invalid_status_raises_error(self, parser: OutputParser) -> None:
        """Parser should raise error for invalid status values."""
        invalid_json = (
            '{"status": "invalid_status", "current_step": "Step 1", '
            '"attempted_actions": [], "summary": "Done"}'
        )
        with pytest.raises(OutputParseError):
            parser.parse(invalid_json)


class TestOutputParserWithValidation:
    """Tests for parse_with_validation method."""

    def test_parse_with_validation_returns_warnings(self, parser: OutputParser) -> None:
        """parse_with_validation should return warnings for semantic issues."""
        # ask_mentor without question_for_mentor
        json_str = """{
            "status": "ask_mentor",
            "current_step": "Step 1",
            "attempted_actions": [],
            "summary": "Stuck"
        }"""
        result = parser.parse_with_validation(json_str)
        assert result.output.status == "ask_mentor"
        assert len(result.warnings) > 0
        warning_fields = [w.field for w in result.warnings]
        assert "question_for_mentor" in warning_fields
        assert "attempted_actions" in warning_fields

    def test_parse_with_validation_no_warnings_for_valid(
        self, parser: OutputParser, valid_json: str
    ) -> None:
        """parse_with_validation should return no warnings for valid output."""
        result = parser.parse_with_validation(valid_json)
        assert result.output.status == "completed"
        assert len(result.warnings) == 0


class TestValidateStudentOutput:
    """Tests for the validate_student_output function."""

    def test_validate_completed_status_valid(self) -> None:
        """Completed status with all fields should have no warnings."""
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action1", "action2"],
            summary="Successfully completed",
        )
        warnings = validate_student_output(output)
        assert len(warnings) == 0

    def test_validate_ask_mentor_missing_question(self) -> None:
        """ask_mentor status without question_for_mentor should warn."""
        output = StudentOutput(
            status="ask_mentor",
            current_step="Step 1",
            attempted_actions=["tried something"],
            summary="Stuck on step",
        )
        warnings = validate_student_output(output)
        assert any(w.field == "question_for_mentor" for w in warnings)

    def test_validate_ask_mentor_missing_problem(self) -> None:
        """ask_mentor status without problem should warn."""
        output = StudentOutput(
            status="ask_mentor",
            current_step="Step 1",
            attempted_actions=["tried something"],
            question_for_mentor="How do I fix this?",
            summary="Stuck on step",
        )
        warnings = validate_student_output(output)
        assert any(w.field == "problem" for w in warnings)

    def test_validate_cannot_complete_missing_reason(self) -> None:
        """cannot_complete status without reason should warn."""
        output = StudentOutput(
            status="cannot_complete",
            current_step="Step 1",
            attempted_actions=["tried something"],
            summary="Cannot complete",
        )
        warnings = validate_student_output(output)
        assert any(w.field == "reason" for w in warnings)

    def test_validate_empty_summary_warns(self) -> None:
        """Empty or whitespace-only summary should warn."""
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=["action"],
            summary="   ",
        )
        warnings = validate_student_output(output)
        assert any(w.field == "summary" for w in warnings)

    def test_validate_empty_attempted_actions_warns(self) -> None:
        """Empty attempted_actions should warn."""
        output = StudentOutput(
            status="completed",
            current_step="Step 1",
            attempted_actions=[],
            summary="Done",
        )
        warnings = validate_student_output(output)
        assert any(w.field == "attempted_actions" for w in warnings)


class TestExtractJson:
    """Tests for the static extract_json method."""

    def test_extract_json_from_code_block(self) -> None:
        """extract_json should extract content from code blocks."""
        text = '```json\n{"key": "value"}\n```'
        result = OutputParser.extract_json(text)
        assert result == '{"key": "value"}'

    def test_extract_json_from_plain_text(self) -> None:
        """extract_json should find JSON in plain text."""
        text = 'The result is {"key": "value"} here'
        result = OutputParser.extract_json(text)
        assert result == '{"key": "value"}'

    def test_extract_json_returns_original_if_no_json(self) -> None:
        """extract_json should return original text if no JSON found."""
        text = "Just plain text"
        result = OutputParser.extract_json(text)
        assert result == "Just plain text"


class TestFindJsonObject:
    """Tests for the static find_json_object method."""

    def test_find_json_object_simple(self) -> None:
        """find_json_object should find simple JSON objects."""
        text = 'prefix {"key": "value"} suffix'
        result = OutputParser.find_json_object(text)
        assert result == '{"key": "value"}'

    def test_find_json_object_nested(self) -> None:
        """find_json_object should handle nested objects."""
        text = '{"outer": {"inner": "value"}}'
        result = OutputParser.find_json_object(text)
        assert result == '{"outer": {"inner": "value"}}'

    def test_find_json_object_with_array(self) -> None:
        """find_json_object should handle objects containing arrays."""
        text = '{"items": [1, 2, 3]}'
        result = OutputParser.find_json_object(text)
        assert result == '{"items": [1, 2, 3]}'

    def test_find_json_object_braces_in_string(self) -> None:
        """find_json_object should handle braces inside strings."""
        text = '{"message": "contains {braces}"}'
        result = OutputParser.find_json_object(text)
        assert result == '{"message": "contains {braces}"}'

    def test_find_json_object_returns_none_for_invalid(self) -> None:
        """find_json_object should return None if no valid JSON found."""
        text = "no json here"
        result = OutputParser.find_json_object(text)
        assert result is None

    def test_find_json_object_first_match(self) -> None:
        """find_json_object should return the first valid object."""
        text = '{"first": 1} {"second": 2}'
        result = OutputParser.find_json_object(text)
        assert result == '{"first": 1}'


class TestOutputParseErrorException:
    """Tests for the OutputParseError exception class."""

    def test_exception_stores_attributes(self) -> None:
        """OutputParseError should store raw_output and parse_error."""
        error = OutputParseError(
            "Test error",
            raw_output="raw content",
            parse_error="specific error",
        )
        assert error.raw_output == "raw content"
        assert error.parse_error == "specific error"
        assert str(error) == "Test error | Parse error: specific error | Output: raw content"

    def test_exception_truncates_long_output(self) -> None:
        """OutputParseError should truncate long raw_output in string representation."""
        long_output = "x" * 300
        error = OutputParseError(
            "Test error",
            raw_output=long_output,
            parse_error="error",
        )
        str_repr = str(error)
        assert len(str_repr) < len(long_output) + 100
        assert "..." in str_repr


class TestValidationWarning:
    """Tests for the ValidationWarning dataclass."""

    def test_warning_stores_attributes(self) -> None:
        """ValidationWarning should store field and message."""
        warning = ValidationWarning(field="test_field", message="Test message")
        assert warning.field == "test_field"
        assert warning.message == "Test message"
