"""Output models for SMILE agent wrappers.

This module defines the structured output format for the Student agent,
which communicates progress, problems, and completion status back to
the orchestrator.

It also provides parsing utilities for extracting structured output from
LLM responses, including recovery strategies for malformed output.
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, ClassVar, Literal

from pydantic import BaseModel, Field, ValidationError

if TYPE_CHECKING:
    from re import Pattern

__all__ = [
    "OutputParseError",
    "OutputParser",
    "StudentOutput",
    "ValidationWarning",
    "validate_student_output",
]


class StudentOutput(BaseModel):
    """Structured output from the Student agent after processing a tutorial step.

    The Student agent produces this output to communicate its progress,
    any problems encountered, and whether it needs mentor assistance.

    Attributes:
        status: The outcome of the step attempt.
            - "completed": Step was successfully completed.
            - "ask_mentor": Student is stuck and needs mentor help.
            - "cannot_complete": Step cannot be completed (e.g., missing resources).
        current_step: Description or identifier of the step being attempted.
        attempted_actions: List of actions the student tried during this step.
        problem: Description of the problem encountered, if any.
        question_for_mentor: Specific question to ask the mentor when status is "ask_mentor".
        reason: Explanation for the current status, especially for "cannot_complete".
        summary: Brief summary of what was accomplished or attempted.
        files_created: List of file paths created during this step.
        commands_run: List of shell commands executed during this step.

    Note:
        Field aliases allow LLMs to use either camelCase (currentStep) or
        snake_case (current_step) in their output. Both formats are accepted.
    """

    model_config = {"populate_by_name": True}

    status: Literal["completed", "ask_mentor", "cannot_complete"]
    current_step: str = Field(validation_alias="currentStep")
    attempted_actions: list[str] = Field(validation_alias="attemptedActions")
    problem: str | None = None
    question_for_mentor: str | None = Field(default=None, validation_alias="questionForMentor")
    reason: str | None = None
    summary: str
    files_created: list[str] = Field(default=[], validation_alias="filesCreated")
    commands_run: list[str] = Field(default=[], validation_alias="commandsRun")


class OutputParseError(Exception):
    """Exception raised when output parsing fails.

    This exception is raised when the `OutputParser` cannot extract valid
    `StudentOutput` from an LLM response after all recovery strategies
    have been exhausted.

    Attributes:
        raw_output: The original LLM output that failed to parse.
        parse_error: Description of why parsing failed.
    """

    def __init__(self, message: str, *, raw_output: str, parse_error: str) -> None:
        """Initialize OutputParseError.

        Args:
            message: Human-readable error message.
            raw_output: The original LLM output that failed to parse.
            parse_error: Specific description of the parsing failure.
        """
        super().__init__(message)
        self.raw_output = raw_output
        self.parse_error = parse_error

    def __str__(self) -> str:
        """Return a string representation including context."""
        truncated = self.raw_output[:200] + "..." if len(self.raw_output) > 200 else self.raw_output
        return f"{super().__str__()} | Parse error: {self.parse_error} | Output: {truncated}"


@dataclass
class ValidationWarning:
    """Warning about potential issues with parsed output.

    These warnings indicate non-fatal issues that don't prevent parsing
    but may indicate problems with the LLM output quality.

    Attributes:
        field: The field that has a potential issue.
        message: Description of the warning.
    """

    field: str
    message: str


@dataclass
class ParseResult:
    """Result of parsing LLM output.

    Contains the parsed `StudentOutput` along with any warnings generated
    during validation.

    Attributes:
        output: The successfully parsed StudentOutput.
        warnings: List of non-fatal validation warnings.
    """

    output: StudentOutput
    warnings: list[ValidationWarning] = field(default_factory=list)


def validate_student_output(output: StudentOutput) -> list[ValidationWarning]:
    """Validate semantic constraints on StudentOutput.

    Checks for logical consistency in the output beyond basic schema
    validation. For example, ensures that `question_for_mentor` is
    present when status is "ask_mentor".

    Args:
        output: The StudentOutput to validate.

    Returns:
        A list of ValidationWarning objects for any issues found.
        An empty list indicates no warnings.

    Example:
        >>> output = StudentOutput(
        ...     status="ask_mentor",
        ...     current_step="Step 1",
        ...     attempted_actions=["Try something"],
        ...     summary="Tried and failed",
        ... )
        >>> warnings = validate_student_output(output)
        >>> len(warnings) > 0
        True
    """
    warnings: list[ValidationWarning] = []

    # Check ask_mentor status requirements
    if output.status == "ask_mentor":
        if not output.question_for_mentor:
            warnings.append(
                ValidationWarning(
                    field="question_for_mentor",
                    message="Status is 'ask_mentor' but question_for_mentor is empty or missing",
                )
            )
        if not output.problem:
            warnings.append(
                ValidationWarning(
                    field="problem",
                    message="Status is 'ask_mentor' but problem description is missing",
                )
            )

    # Check cannot_complete status requirements
    if output.status == "cannot_complete" and not output.reason:
        warnings.append(
            ValidationWarning(
                field="reason",
                message="Status is 'cannot_complete' but reason is empty or missing",
            )
        )

    # Check for empty summary
    if not output.summary or not output.summary.strip():
        warnings.append(
            ValidationWarning(
                field="summary",
                message="Summary is empty or contains only whitespace",
            )
        )

    # Check for empty attempted_actions
    if not output.attempted_actions:
        warnings.append(
            ValidationWarning(
                field="attempted_actions",
                message="No attempted actions recorded",
            )
        )

    return warnings


class OutputParser:
    """Parser for extracting and validating StudentOutput from LLM responses.

    Handles various formats that LLMs might produce, including:
    - Raw JSON
    - JSON wrapped in markdown code blocks (```json ... ```)
    - JSON with surrounding explanatory text

    The parser applies multiple recovery strategies in order:
    1. Direct parse: Try parsing the trimmed output directly
    2. Code block extraction: Find ```json ... ``` or ``` ... ``` blocks
    3. JSON object search: Find first { ... } substring and try to parse
    4. Field extraction: Try to extract required fields individually

    Attributes:
        verbose: If True, log recovery attempts to stderr.

    Example:
        >>> parser = OutputParser()
        >>> result = parser.parse('{"status": "completed", ...}')
        >>> result.status
        'completed'
    """

    # Pattern for markdown code blocks with optional json specifier
    _CODE_BLOCK_PATTERN: ClassVar[Pattern[str]] = re.compile(
        r"```(?:json)?\s*\n?([\s\S]*?)\n?```", re.MULTILINE
    )

    # Pattern for finding JSON objects (non-greedy, handles nested braces)
    _JSON_START_PATTERN: ClassVar[Pattern[str]] = re.compile(r"\{")

    # Patterns for extracting individual fields
    _FIELD_PATTERNS: ClassVar[dict[str, Pattern[str]]] = {
        "status": re.compile(
            r'"status"\s*:\s*"(completed|ask_mentor|cannot_complete)"', re.IGNORECASE
        ),
        "current_step": re.compile(r'"current_step"\s*:\s*"([^"]*)"'),
        "summary": re.compile(r'"summary"\s*:\s*"([^"]*)"'),
        "problem": re.compile(r'"problem"\s*:\s*"([^"]*)"'),
        "question_for_mentor": re.compile(r'"question_for_mentor"\s*:\s*"([^"]*)"'),
        "reason": re.compile(r'"reason"\s*:\s*"([^"]*)"'),
    }

    def __init__(self, *, verbose: bool = True) -> None:
        """Initialize the OutputParser.

        Args:
            verbose: If True, log recovery attempts to stderr. Defaults to True.
        """
        self.verbose = verbose

    def _log(self, message: str) -> None:
        """Log a message to stderr if verbose mode is enabled.

        Args:
            message: The message to log.
        """
        if self.verbose:
            print(f"[OutputParser] {message}", file=sys.stderr)

    def parse(self, output: str) -> StudentOutput:
        """Parse LLM output into StudentOutput with recovery strategies.

        Attempts multiple strategies to extract valid JSON from the output,
        in order of preference:
        1. Direct parse of trimmed output
        2. Extract from markdown code blocks
        3. Find JSON object in text
        4. Extract fields individually and construct output

        Args:
            output: The raw LLM output string.

        Returns:
            A validated StudentOutput object.

        Raises:
            OutputParseError: If all recovery strategies fail.

        Example:
            >>> parser = OutputParser(verbose=False)
            >>> result = parser.parse('''```json
            ... {"status": "completed", "current_step": "Step 1",
            ...  "attempted_actions": ["action"], "summary": "Done"}
            ... ```''')
            >>> result.status
            'completed'
        """
        if not output or not output.strip():
            raise OutputParseError(
                "Empty output received from LLM",
                raw_output=output or "",
                parse_error="Output is empty or contains only whitespace",
            )

        stripped = output.strip()

        # Strategy 1: Direct parse
        result = self._try_direct_parse(stripped)
        if result:
            return result

        # Strategy 2: Code block extraction
        result = self._try_code_block_extraction(stripped)
        if result:
            return result

        # Strategy 3: JSON object search
        result = self._try_json_object_search(stripped)
        if result:
            return result

        # Strategy 4: Field extraction
        result = self._try_field_extraction(stripped)
        if result:
            return result

        # All strategies failed
        raise OutputParseError(
            "Failed to parse LLM output after all recovery strategies",
            raw_output=output,
            parse_error="No valid JSON structure found in output",
        )

    def parse_with_validation(self, output: str) -> ParseResult:
        """Parse LLM output and include validation warnings.

        This method is similar to `parse()` but also runs semantic
        validation on the result and returns warnings alongside the
        parsed output.

        Args:
            output: The raw LLM output string.

        Returns:
            A ParseResult containing the StudentOutput and any warnings.

        Raises:
            OutputParseError: If parsing fails.

        Example:
            >>> parser = OutputParser(verbose=False)
            >>> result = parser.parse_with_validation(
            ...     '{"status": "ask_mentor", "current_step": "Step 1", '
            ...     '"attempted_actions": [], "summary": "Stuck"}'
            ... )
            >>> len(result.warnings) > 0
            True
        """
        parsed = self.parse(output)
        warnings = validate_student_output(parsed)
        return ParseResult(output=parsed, warnings=warnings)

    def _try_direct_parse(self, text: str) -> StudentOutput | None:
        """Try to parse text directly as JSON.

        Args:
            text: The text to parse.

        Returns:
            StudentOutput if successful, None otherwise.
        """
        try:
            data = json.loads(text)
            result = StudentOutput.model_validate(data)
            self._log("Successfully parsed output directly")
            return result
        except (json.JSONDecodeError, ValidationError):
            return None

    def _try_code_block_extraction(self, text: str) -> StudentOutput | None:
        """Try to extract JSON from markdown code blocks.

        Args:
            text: The text containing potential code blocks.

        Returns:
            StudentOutput if successful, None otherwise.
        """
        matches = self._CODE_BLOCK_PATTERN.findall(text)

        for i, match in enumerate(reversed(matches)):
            # Try matches in reverse order (last block is most likely the output)
            json_str = match.strip()
            if not json_str:
                continue

            try:
                data = json.loads(json_str)
                result = StudentOutput.model_validate(data)
                self._log(f"Successfully extracted from code block {len(matches) - i}")
                return result
            except (json.JSONDecodeError, ValidationError):
                continue

        return None

    def _try_json_object_search(self, text: str) -> StudentOutput | None:
        """Try to find and parse a JSON object in the text.

        Searches for the first valid JSON object by finding opening
        braces and attempting to find matching closing braces.

        Args:
            text: The text to search.

        Returns:
            StudentOutput if successful, None otherwise.
        """
        json_str = self.find_json_object(text)
        if not json_str:
            return None

        try:
            data = json.loads(json_str)
            result = StudentOutput.model_validate(data)
            self._log("Successfully extracted JSON object from text")
            return result
        except (json.JSONDecodeError, ValidationError):
            return None

    def _try_field_extraction(self, text: str) -> StudentOutput | None:
        """Try to extract individual fields and construct output.

        This is a last-resort strategy that attempts to extract required
        fields using regex patterns when the JSON structure is broken.

        Args:
            text: The text to extract fields from.

        Returns:
            StudentOutput if required fields found, None otherwise.
        """
        extracted: dict[str, str | list[str]] = {}

        for field_name, pattern in self._FIELD_PATTERNS.items():
            match = pattern.search(text)
            if match:
                extracted[field_name] = match.group(1)

        # Check for required fields
        required_fields = {"status", "current_step", "summary"}
        if not required_fields.issubset(extracted.keys()):
            return None

        # Add default values for list fields
        extracted.setdefault("attempted_actions", ["Extracted from malformed output"])
        extracted.setdefault("files_created", [])
        extracted.setdefault("commands_run", [])

        try:
            result = StudentOutput.model_validate(extracted)
            self._log("Successfully recovered output via field extraction")
            return result
        except ValidationError:
            return None

    @staticmethod
    def extract_json(text: str) -> str:
        """Extract JSON string from various wrapper formats.

        This is a convenience method that applies extraction logic
        without full validation. Useful when you need just the JSON
        string for further processing.

        Args:
            text: The text containing JSON.

        Returns:
            The extracted JSON string, or the original text if no
            wrappers are detected.

        Example:
            >>> OutputParser.extract_json('```json\\n{"key": "value"}\\n```')
            '{"key": "value"}'
        """
        stripped = text.strip()

        # Try code block extraction
        matches: list[str] = OutputParser._CODE_BLOCK_PATTERN.findall(stripped)
        if matches:
            return matches[-1].strip()

        # Try to find JSON object
        json_str = OutputParser.find_json_object(stripped)
        if json_str:
            return json_str

        return stripped

    @staticmethod
    def find_json_object(text: str) -> str | None:
        """Find the first valid JSON object in text.

        Searches for opening braces and attempts to find the matching
        closing brace by tracking brace depth. Handles nested objects
        and strings containing braces.

        Args:
            text: The text to search for JSON objects.

        Returns:
            The JSON object string if found, None otherwise.

        Example:
            >>> OutputParser.find_json_object('The result is {"key": "value"} here')
            '{"key": "value"}'
        """
        # Find all potential starting positions
        for match in OutputParser._JSON_START_PATTERN.finditer(text):
            start_pos = match.start()
            json_str = OutputParser._extract_balanced_braces(text, start_pos)
            if json_str:
                # Verify it's actually valid JSON
                try:
                    json.loads(json_str)
                    return json_str
                except json.JSONDecodeError:
                    continue
        return None

    @staticmethod
    def _extract_balanced_braces(text: str, start: int) -> str | None:
        """Extract a substring with balanced braces starting at position.

        Handles nested braces and strings (including escaped quotes).

        Args:
            text: The full text.
            start: Starting position (must be an opening brace).

        Returns:
            The balanced substring if found, None otherwise.
        """
        if start >= len(text) or text[start] != "{":
            return None

        depth = 0
        in_string = False
        escape_next = False
        pos = start

        while pos < len(text):
            char = text[pos]

            if escape_next:
                escape_next = False
                pos += 1
                continue

            if char == "\\":
                escape_next = True
                pos += 1
                continue

            if char == '"' and not escape_next:
                in_string = not in_string
                pos += 1
                continue

            if not in_string:
                if char == "{":
                    depth += 1
                elif char == "}":
                    depth -= 1
                    if depth == 0:
                        return text[start : pos + 1]

            pos += 1

        return None
