"""SMILE Wrappers - Student and Mentor agent wrappers for SMILE Loop."""

from smile_wrappers.config import Config, LlmProvider, PatienceLevel, StudentBehavior
from smile_wrappers.mentor import (
    MentorOrchestratorClient,
    MentorResultRequest,
    MentorResultResponse,
    MentorWrapper,
    StuckContext,
    mentor_main,
)
from smile_wrappers.output import (
    OutputParseError,
    OutputParser,
    StudentOutput,
    validate_student_output,
)
from smile_wrappers.prompts import (
    build_mentor_prompt,
    build_student_prompt,
    get_student_output_schema,
)
from smile_wrappers.student import (
    LlmCli,
    LlmCliError,
    LlmParseError,
    LlmTimeoutError,
    NextAction,
    OrchestratorCallbackError,
    OrchestratorClient,
    StuckCondition,
    StuckDetector,
    StudentWrapper,
)

__version__ = "0.1.0"

__all__ = [
    "Config",
    "LlmCli",
    "LlmCliError",
    "LlmParseError",
    "LlmProvider",
    "LlmTimeoutError",
    "MentorOrchestratorClient",
    "MentorResultRequest",
    "MentorResultResponse",
    "MentorWrapper",
    "NextAction",
    "OrchestratorCallbackError",
    "OrchestratorClient",
    "OutputParseError",
    "OutputParser",
    "PatienceLevel",
    "StuckCondition",
    "StuckContext",
    "StuckDetector",
    "StudentBehavior",
    "StudentOutput",
    "StudentWrapper",
    "build_mentor_prompt",
    "build_student_prompt",
    "get_student_output_schema",
    "mentor_main",
    "validate_student_output",
]
