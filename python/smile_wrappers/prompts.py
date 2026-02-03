"""Prompt construction for SMILE Loop agents.

This module provides functions to build prompts for the Student and Mentor
agents, including JSON schema generation for structured output requirements.
"""

from __future__ import annotations

import json
from textwrap import dedent
from typing import Any

from smile_wrappers.config import PatienceLevel, StudentBehavior
from smile_wrappers.output import StudentOutput

__all__ = [
    "build_mentor_prompt",
    "build_student_prompt",
    "get_student_output_schema",
]


def get_student_output_schema() -> dict[str, Any]:
    """Return the JSON schema for StudentOutput.

    Generates a JSON schema from the StudentOutput pydantic model that can
    be included in prompts to guide LLM structured output generation.

    Returns:
        A dictionary containing the JSON schema for StudentOutput.

    Example:
        >>> schema = get_student_output_schema()
        >>> print(schema["title"])
        'StudentOutput'
    """
    return StudentOutput.model_json_schema()


def _format_behavior_rules(behavior: StudentBehavior) -> str:
    """Format StudentBehavior config into human-readable rules.

    Args:
        behavior: The StudentBehavior configuration.

    Returns:
        A formatted string describing the behavior rules.
    """
    patience_descriptions = {
        PatienceLevel.LOW: "Ask for help relatively quickly when stuck.",
        PatienceLevel.MEDIUM: "Make a moderate effort before asking for help.",
        PatienceLevel.HIGH: "Try hard to solve problems yourself before asking.",
    }

    patience_desc = patience_descriptions[behavior.patience_level]
    rules = [
        f"- Maximum retry attempts before asking for help: {behavior.max_retries_before_help}",
        f"- Patience level: {behavior.patience_level.value} - {patience_desc}",
    ]

    if behavior.ask_on_missing_dependency:
        rules.append("- Ask for help when a required dependency or tool is missing.")
    if behavior.ask_on_ambiguous_instruction:
        rules.append("- Ask for help when instructions are unclear or ambiguous.")
    if behavior.ask_on_command_failure:
        rules.append("- Ask for help when a command fails unexpectedly.")
    if behavior.ask_on_timeout:
        timeout = behavior.timeout_seconds
        rules.append(f"- Ask for help when an operation takes longer than {timeout} seconds.")

    return "\n".join(rules)


def _format_mentor_notes(mentor_notes: list[str]) -> str:
    """Format previous mentor notes for inclusion in prompts.

    Args:
        mentor_notes: List of notes from previous mentor interactions.

    Returns:
        A formatted string containing the mentor notes section,
        or an empty string if no notes exist.
    """
    if not mentor_notes:
        return ""

    formatted_notes = "\n\n".join(
        f"### Note {i + 1}\n{note}" for i, note in enumerate(mentor_notes)
    )

    return dedent(f"""
        ## Previous Mentor Guidance

        The mentor has provided the following guidance in previous iterations.
        Use this information to help you progress:

        {formatted_notes}
    """).strip()


def _format_json_schema_section(schema: dict[str, Any]) -> str:
    """Format the JSON schema section for the prompt.

    Args:
        schema: The JSON schema dictionary.

    Returns:
        A formatted string containing the schema and output instructions.
    """
    schema_json = json.dumps(schema, indent=2)

    return dedent(f"""
        ## Required Output Format

        You MUST respond with a JSON object that conforms to the following schema:

        ```json
        {schema_json}
        ```

        ### Status Values

        - "completed": Use this when you successfully complete the current step.
        - "ask_mentor": Use this when you are stuck and need help from the mentor.
        - "cannot_complete": Use this when the step is impossible to complete
          (e.g., required resources are unavailable, prerequisites are missing).

        ### Output Guidelines

        - Always fill in `current_step` with a description of what step you are working on.
        - Always fill in `summary` with a brief description of what you accomplished or attempted.
        - When status is "ask_mentor", you MUST provide both `problem` and `question_for_mentor`.
        - When status is "cannot_complete", you MUST provide both `problem` and `reason`.
        - List all files you create in `files_created` (full paths).
        - List all commands you run in `commands_run`.

        Respond ONLY with the JSON object, no additional text before or after.
    """).strip()


def _format_iteration_context(iteration: int) -> str:
    """Format the iteration context section for the student prompt.

    Args:
        iteration: The current iteration number (1-indexed).

    Returns:
        A formatted string describing the current iteration context.
    """
    if iteration == 1:
        attempt_text = "This is your first attempt."
    else:
        attempt_text = f"You have had {iteration - 1} previous attempt(s)."

    return dedent(f"""
        ## Current Context

        This is iteration {iteration} of your attempt to follow the tutorial.
        {attempt_text}

        ## Tutorial Content

        Follow this tutorial step-by-step:

        ---
    """).strip()


def build_student_prompt(
    tutorial_content: str,
    student_behavior: StudentBehavior,
    mentor_notes: list[str],
    iteration: int,
) -> str:
    """Build the prompt for the Student agent.

    Constructs a complete prompt that instructs an LLM to act as a beginner
    following a tutorial step-by-step, with specific behavior rules and
    output format requirements.

    Args:
        tutorial_content: The full markdown content of the tutorial to follow.
        student_behavior: Configuration controlling when to ask for help.
        mentor_notes: List of guidance notes from previous mentor interactions.
        iteration: The current iteration number (1-indexed).

    Returns:
        The complete prompt string for the Student agent.

    Example:
        >>> behavior = StudentBehavior(max_retries_before_help=2)
        >>> prompt = build_student_prompt(
        ...     tutorial_content="# Tutorial...",
        ...     student_behavior=behavior,
        ...     mentor_notes=[],
        ...     iteration=1
        ... )
        >>> "beginner" in prompt.lower()
        True
    """
    behavior_rules = _format_behavior_rules(student_behavior)
    mentor_notes_section = _format_mentor_notes(mentor_notes)
    schema_section = _format_json_schema_section(get_student_output_schema())

    prompt_parts = [
        dedent("""
            # Role: Tutorial Student

            You are a beginner learning from a technical tutorial. Your goal is to
            follow the tutorial instructions step-by-step, executing commands and
            creating files as directed.

            ## Your Constraints

            As a beginner, you have the following characteristics:
            - You follow instructions literally without making assumptions.
            - You do not have deep knowledge of the technologies being taught.
            - You cannot fill in gaps or fix problems on your own beyond basic troubleshooting.
            - When you encounter problems, you must ask for help rather than guessing.

            ## Behavior Rules

            Follow these rules when deciding whether to ask for help:

        """).strip(),
        behavior_rules,
    ]

    if mentor_notes_section:
        prompt_parts.append("")
        prompt_parts.append(mentor_notes_section)

    prompt_parts.extend(
        [
            "",
            _format_iteration_context(iteration),
            "",
            tutorial_content,
            "",
            "---",
            "",
            schema_section,
        ]
    )

    return "\n".join(prompt_parts)


def build_mentor_prompt(
    tutorial_content: str,
    current_step: str,
    problem: str,
    question: str,
    previous_notes: list[str],
) -> str:
    """Build the prompt for the Mentor agent.

    Constructs a prompt that instructs an LLM to act as a helpful mentor,
    providing guidance without directly completing the task for the student.

    Args:
        tutorial_content: The full markdown content of the tutorial.
        current_step: Description of the step where the student is stuck.
        problem: Description of the problem the student encountered.
        question: The specific question the student is asking.
        previous_notes: List of notes from previous mentor interactions
            to avoid repeating advice.

    Returns:
        The complete prompt string for the Mentor agent.
        The mentor should return plain text guidance, not JSON.

    Example:
        >>> prompt = build_mentor_prompt(
        ...     tutorial_content="# Tutorial...",
        ...     current_step="Install dependencies",
        ...     problem="pip install failed",
        ...     question="How do I fix this error?",
        ...     previous_notes=[]
        ... )
        >>> "mentor" in prompt.lower()
        True
    """
    previous_notes_section = ""
    if previous_notes:
        formatted_previous = "\n\n".join(
            f"- Note {i + 1}: {note}" for i, note in enumerate(previous_notes)
        )
        previous_notes_section = dedent(f"""
            ## Your Previous Guidance

            You have previously provided the following guidance to this student.
            Do NOT repeat this advice - provide new, different guidance:

            {formatted_previous}
        """).strip()

    prompt_parts = [
        dedent("""
            # Role: Tutorial Mentor

            You are a helpful mentor assisting a beginner who is learning from a
            technical tutorial. The student is stuck and needs your guidance.

            ## Your Guidelines

            As a mentor, you must follow these principles:
            - Provide hints and guidance, NOT complete solutions.
            - Help the student understand the problem and how to approach it.
            - Do NOT write code or run commands for the student.
            - Encourage the student to think through the problem.
            - Point to relevant documentation or concepts they should learn.
            - Be patient and supportive.

            ## Important Constraints

            - Your response should be plain text guidance (NOT JSON).
            - Keep your response focused and concise.
            - Do not repeat advice you have already given.
            - Guide the student toward understanding, not just copying answers.
        """).strip(),
    ]

    if previous_notes_section:
        prompt_parts.append("")
        prompt_parts.append(previous_notes_section)

    prompt_parts.extend(
        [
            "",
            dedent("""
            ## Tutorial Context

            The student is following this tutorial:

            ---
        """).strip(),
            "",
            tutorial_content,
            "",
            "---",
            "",
            dedent(f"""
            ## Student's Current Situation

            **Current Step:** {current_step}

            **Problem Encountered:** {problem}

            **Student's Question:** {question}

            ## Your Response

            Provide helpful guidance to help the student overcome this obstacle.
            Remember: hints and direction, not direct solutions.
        """).strip(),
        ]
    )

    return "\n".join(prompt_parts)
