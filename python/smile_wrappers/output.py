"""Output models for SMILE agent wrappers.

This module defines the structured output format for the Student agent,
which communicates progress, problems, and completion status back to
the orchestrator.
"""

from typing import Literal

from pydantic import BaseModel


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
    """

    status: Literal["completed", "ask_mentor", "cannot_complete"]
    current_step: str
    attempted_actions: list[str]
    problem: str | None = None
    question_for_mentor: str | None = None
    reason: str | None = None
    summary: str
    files_created: list[str] = []
    commands_run: list[str] = []
