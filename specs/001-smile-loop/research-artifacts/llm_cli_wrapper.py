"""
Production-ready Python wrapper for LLM CLI tools.

Provides:
- Structured JSON output handling
- Automatic retry with exponential backoff
- Timeout management for long-running calls
- Multi-turn conversation support
- Circuit breaker for fault protection
- Full async/await support

Usage:
    from llm_cli_wrapper import ClaudeCliWrapper

    wrapper = ClaudeCliWrapper()
    response = await wrapper.call_api(
        prompt="Analyze this code",
        output_model=AnalysisResponse
    )
"""

import json
import subprocess
import asyncio
import signal
import os
import re
import time
import random
import logging
from typing import (
    Optional, Union, TypeVar, Callable, Any, Iterator,
    AsyncIterator, Literal, Tuple
)
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import Enum
from pathlib import Path
from contextlib import asynccontextmanager

from pydantic import BaseModel, Field, field_validator

# Type variables
T = TypeVar('T')
MessageRole = Literal["user", "assistant", "system"]

# Configure logging
logger = logging.getLogger(__name__)


# ============================================================================
# Error Handling & Classification
# ============================================================================

class ErrorCategory(Enum):
    """Classify subprocess errors for proper handling."""
    RETRYABLE = "retryable"
    PERMANENT = "permanent"
    RATE_LIMIT = "rate_limit"
    TIMEOUT = "timeout"
    MALFORMED_OUTPUT = "malformed"
    AUTH_FAILURE = "auth"
    UNKNOWN = "unknown"


class LLMError(Exception):
    """Base exception for LLM operations."""

    def __init__(
        self,
        message: str,
        category: ErrorCategory = ErrorCategory.UNKNOWN,
        original_error: Optional[Exception] = None,
        stderr: Optional[str] = None
    ):
        self.message = message
        self.category = category
        self.original_error = original_error
        self.stderr = stderr
        super().__init__(message)

    @property
    def is_retryable(self) -> bool:
        """Check if error should trigger retry."""
        return self.category in (
            ErrorCategory.RETRYABLE,
            ErrorCategory.RATE_LIMIT,
            ErrorCategory.TIMEOUT
        )

    def __repr__(self) -> str:
        return f"LLMError({self.category.value}: {self.message})"


def categorize_error(
    exception: Exception,
    stderr: str = "",
    stdout: str = ""
) -> Tuple[ErrorCategory, str]:
    """Categorize error for proper retry handling."""
    exc_str = str(exception).lower()
    stderr_lower = stderr.lower()

    if isinstance(exception, subprocess.TimeoutExpired):
        return ErrorCategory.TIMEOUT, f"Process timed out: {exception}"

    if "rate limit" in stderr_lower or "too many requests" in stderr_lower:
        return ErrorCategory.RATE_LIMIT, "API rate limit exceeded"

    if "authentication" in stderr_lower or "unauthorized" in stderr_lower:
        return ErrorCategory.AUTH_FAILURE, "Authentication failed"

    if "connection" in exc_str or "network" in exc_str:
        return ErrorCategory.RETRYABLE, f"Network error: {exception}"

    if "timeout" in exc_str:
        return ErrorCategory.TIMEOUT, f"Timeout: {exception}"

    if isinstance(exception, json.JSONDecodeError):
        return ErrorCategory.MALFORMED_OUTPUT, f"Invalid JSON: {exception}"

    if "permission denied" in stderr_lower or "access denied" in stderr_lower:
        return ErrorCategory.AUTH_FAILURE, "Access denied"

    return ErrorCategory.PERMANENT, f"Permanent error: {exception}"


# ============================================================================
# Configuration & State Management
# ============================================================================

@dataclass
class RetryConfig:
    """Configuration for retry behavior."""
    max_attempts: int = 3
    initial_delay_ms: float = 100
    max_delay_ms: float = 30000
    exponential_base: float = 2.0
    jitter: bool = True
    retryable_exceptions: Tuple = (
        subprocess.TimeoutExpired,
        ConnectionError,
        TimeoutError
    )


@dataclass
class TimeoutConfig:
    """Timeouts for different subprocess phases."""
    initial_startup: float = 5.0
    first_response: float = 10.0
    chunk_receive: float = 5.0
    total_execution: float = 60.0
    graceful_shutdown: float = 5.0


@dataclass
class Message:
    """Single message in conversation."""
    role: MessageRole
    content: str
    timestamp: datetime = field(default_factory=datetime.utcnow)
    metadata: dict = field(default_factory=dict)


@dataclass
class ConversationContext:
    """Maintains state for multi-turn interactions."""
    system_prompt: str = ""
    messages: list[Message] = field(default_factory=list)
    model_config: dict = field(default_factory=dict)
    max_history: int = 20

    def add_user_message(self, content: str, metadata: Optional[dict] = None) -> None:
        """Add user message to history."""
        self.messages.append(Message(
            role="user",
            content=content,
            metadata=metadata or {}
        ))
        self._trim_history()

    def add_assistant_message(self, content: str, metadata: Optional[dict] = None) -> None:
        """Add assistant response to history."""
        self.messages.append(Message(
            role="assistant",
            content=content,
            metadata=metadata or {}
        ))
        self._trim_history()

    def _trim_history(self) -> None:
        """Keep only recent messages."""
        if len(self.messages) > self.max_history:
            if self.messages and self.messages[0].role == "system":
                self.messages = [
                    self.messages[0],
                    *self.messages[-(self.max_history - 1):]
                ]
            else:
                self.messages = self.messages[-self.max_history:]

    def get_conversation_string(self, format: str = "natural") -> str:
        """Get formatted conversation history."""
        if format == "natural":
            parts = []
            for msg in self.messages:
                if msg.role == "system":
                    continue
                parts.append(f"{msg.role.upper()}:\n{msg.content}\n")
            return "\n".join(parts)

        elif format == "json":
            return json.dumps([
                {
                    "role": msg.role,
                    "content": msg.content,
                    "timestamp": msg.timestamp.isoformat()
                }
                for msg in self.messages
            ], indent=2)

        else:
            raise ValueError(f"Unknown format: {format}")

    def clear(self) -> None:
        """Clear conversation history."""
        self.messages.clear()

    @property
    def message_count(self) -> int:
        """Total messages in history."""
        return len(self.messages)

    @property
    def turn_count(self) -> int:
        """Number of user turns."""
        return sum(1 for msg in self.messages if msg.role == "user")


# ============================================================================
# JSON Extraction & Validation
# ============================================================================

class JSONExtractor:
    """Extract and validate JSON from LLM responses."""

    @staticmethod
    def extract_json(
        text: str,
        model_class: type[BaseModel],
        strict: bool = False
    ) -> Tuple[Optional[dict], Optional[str]]:
        """
        Extract JSON from text with recovery options.

        Returns:
            (parsed_dict, error_message) tuple
        """
        # Remove markdown code blocks
        text = re.sub(
            r'^```(?:json)?\n?|\n?```$',
            '',
            text.strip(),
            flags=re.MULTILINE
        )

        # Try direct parse
        try:
            data = json.loads(text)
            model_class(**data)
            return data, None
        except json.JSONDecodeError as e:
            if strict:
                return None, f"Invalid JSON: {e}"
            return JSONExtractor._recover_json(text, model_class)
        except ValueError as e:
            return None, f"Validation failed: {e}"

    @staticmethod
    def _recover_json(
        text: str,
        model_class: type[BaseModel]
    ) -> Tuple[Optional[dict], Optional[str]]:
        """Attempt recovery from malformed JSON."""
        patterns = [
            r'\{[^{}]*\}(?=\s*$)',  # Simple object at end
            r'\{.*\}',  # Any object (greedy)
        ]

        for pattern in patterns:
            match = re.search(pattern, text, re.DOTALL)
            if match:
                try:
                    data = json.loads(match.group(0))
                    model_class(**data)
                    return data, None
                except (json.JSONDecodeError, ValueError):
                    continue

        return None, "Could not extract valid JSON from response"

    @staticmethod
    def get_schema_json(model_class: type[BaseModel]) -> str:
        """Get Pydantic schema as formatted JSON."""
        schema = model_class.model_json_schema()
        return json.dumps(schema, indent=2)


# ============================================================================
# Retry Logic
# ============================================================================

class RetryHandler:
    """Handle retries with exponential backoff and jitter."""

    def __init__(self, config: Optional[RetryConfig] = None):
        self.config = config or RetryConfig()

    def calculate_delay(self, attempt: int) -> float:
        """Calculate delay for attempt."""
        delay_ms = min(
            self.config.initial_delay_ms * (self.config.exponential_base ** attempt),
            self.config.max_delay_ms
        )

        if self.config.jitter:
            jitter_range = delay_ms * 0.25
            delay_ms += random.uniform(-jitter_range, jitter_range)

        return max(0, delay_ms) / 1000.0

    async def async_call_with_retry(
        self,
        func: Callable[..., Any],
        *args,
        **kwargs
    ) -> Any:
        """Execute async function with retry."""
        last_error: Optional[Exception] = None

        for attempt in range(self.config.max_attempts):
            try:
                return await func(*args, **kwargs)

            except LLMError as e:
                last_error = e
                if not e.is_retryable or attempt >= self.config.max_attempts - 1:
                    raise

                delay = self.calculate_delay(attempt)
                logger.info(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s")
                await asyncio.sleep(delay)

            except self.config.retryable_exceptions as e:
                last_error = e
                if attempt >= self.config.max_attempts - 1:
                    raise LLMError(
                        f"Failed after {self.config.max_attempts} attempts",
                        ErrorCategory.RETRYABLE,
                        e
                    )

                delay = self.calculate_delay(attempt)
                logger.info(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s")
                await asyncio.sleep(delay)

        if last_error:
            raise last_error
        raise LLMError("Retry logic error", ErrorCategory.UNKNOWN)

    def sync_call_with_retry(
        self,
        func: Callable[..., T],
        *args,
        **kwargs
    ) -> T:
        """Synchronous version of retry handler."""
        last_error: Optional[Exception] = None

        for attempt in range(self.config.max_attempts):
            try:
                return func(*args, **kwargs)

            except LLMError as e:
                last_error = e
                if not e.is_retryable or attempt >= self.config.max_attempts - 1:
                    raise

                delay = self.calculate_delay(attempt)
                logger.info(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s")
                time.sleep(delay)

            except self.config.retryable_exceptions as e:
                last_error = e
                if attempt >= self.config.max_attempts - 1:
                    raise LLMError(
                        f"Failed after {self.config.max_attempts} attempts",
                        ErrorCategory.RETRYABLE,
                        e
                    )

                delay = self.calculate_delay(attempt)
                logger.info(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s")
                time.sleep(delay)

        if last_error:
            raise last_error
        raise LLMError("Retry logic error", ErrorCategory.UNKNOWN)


# ============================================================================
# Timeout Management
# ============================================================================

class ProgressiveTimeoutManager:
    """Manage timeouts across subprocess lifecycle."""

    def __init__(self, config: Optional[TimeoutConfig] = None):
        self.config = config or TimeoutConfig()
        self.start_time: Optional[datetime] = None
        self.last_activity_time: Optional[datetime] = None

    def start(self) -> None:
        """Mark start of execution."""
        self.start_time = datetime.utcnow()
        self.last_activity_time = self.start_time

    def activity(self) -> None:
        """Mark activity timestamp."""
        self.last_activity_time = datetime.utcnow()

    def get_remaining_time(self) -> float:
        """Get remaining time before total timeout."""
        if not self.start_time:
            return self.config.total_execution

        elapsed = (datetime.utcnow() - self.start_time).total_seconds()
        remaining = self.config.total_execution - elapsed
        return max(0, remaining)

    def check_chunk_timeout(self) -> float:
        """Check if chunk timeout expired."""
        if not self.last_activity_time:
            return self.config.chunk_receive

        elapsed = (datetime.utcnow() - self.last_activity_time).total_seconds()
        if elapsed > self.config.chunk_receive:
            raise LLMError(
                f"No data for {elapsed:.1f}s (timeout: {self.config.chunk_receive}s)",
                ErrorCategory.TIMEOUT
            )

        return self.get_remaining_time()


# ============================================================================
# Subprocess Execution
# ============================================================================

class SubprocessTimeout:
    """Handle subprocess timeouts correctly."""

    @staticmethod
    async def run_with_timeout_async(
        cmd: list[str],
        input_data: Optional[str] = None,
        timeout_seconds: float = 30.0,
        env: Optional[dict] = None
    ) -> Tuple[str, str, int]:
        """Run subprocess with proper timeout handling."""

        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env
        )

        try:
            input_bytes = input_data.encode() if input_data else None
            stdout_data, stderr_data = await asyncio.wait_for(
                process.communicate(input=input_bytes),
                timeout=timeout_seconds
            )

            return (
                stdout_data.decode('utf-8', errors='replace'),
                stderr_data.decode('utf-8', errors='replace'),
                process.returncode or 0
            )

        except asyncio.TimeoutError:
            process.kill()
            try:
                await asyncio.wait_for(process.wait(), timeout=5)
            except asyncio.TimeoutError:
                if hasattr(process, 'pid'):
                    try:
                        os.kill(process.pid, signal.SIGKILL)
                    except:
                        pass

            raise LLMError(
                f"Process timed out after {timeout_seconds}s",
                ErrorCategory.TIMEOUT,
                asyncio.TimeoutError(timeout_seconds)
            )

    @staticmethod
    def run_with_timeout(
        cmd: list[str],
        input_data: Optional[str] = None,
        timeout_seconds: float = 30.0,
        env: Optional[dict] = None
    ) -> Tuple[str, str, int]:
        """Synchronous version of timeout-aware subprocess runner."""

        process = None

        try:
            process = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=env
            )

            try:
                stdout, stderr = process.communicate(
                    input=input_data,
                    timeout=timeout_seconds
                )
                return stdout, stderr, process.returncode

            except subprocess.TimeoutExpired:
                process.kill()
                try:
                    stdout, stderr = process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    stdout, stderr = "", ""

                raise LLMError(
                    f"Process timed out after {timeout_seconds}s",
                    ErrorCategory.TIMEOUT,
                    subprocess.TimeoutExpired(cmd, timeout_seconds),
                    stderr
                )

        except OSError as e:
            raise LLMError(
                f"Failed to start process: {e}",
                ErrorCategory.PERMANENT,
                e
            )

        finally:
            if process and process.poll() is None:
                try:
                    process.kill()
                    process.wait(timeout=5)
                except:
                    pass


# ============================================================================
# Circuit Breaker
# ============================================================================

class CircuitState(Enum):
    """Circuit breaker states."""
    CLOSED = "closed"
    OPEN = "open"
    HALF_OPEN = "half_open"


class CircuitBreaker:
    """Circuit breaker for protecting against cascading failures."""

    def __init__(
        self,
        failure_threshold: int = 5,
        success_threshold: int = 2,
        timeout_seconds: float = 60.0
    ):
        self.failure_threshold = failure_threshold
        self.success_threshold = success_threshold
        self.timeout_seconds = timeout_seconds

        self.state = CircuitState.CLOSED
        self.failure_count = 0
        self.success_count = 0
        self.last_failure_time: Optional[datetime] = None

    def call(self, func: Callable[..., T], *args, **kwargs) -> T:
        """Execute func through circuit breaker."""

        if self.state == CircuitState.OPEN:
            if self._should_attempt_reset():
                self.state = CircuitState.HALF_OPEN
                self.success_count = 0
            else:
                raise LLMError(
                    "Circuit breaker is open",
                    ErrorCategory.RETRYABLE
                )

        try:
            result = func(*args, **kwargs)
            self._on_success()
            return result
        except Exception as e:
            self._on_failure()
            raise

    def _should_attempt_reset(self) -> bool:
        """Check if enough time passed to attempt reset."""
        if self.last_failure_time is None:
            return False

        elapsed = datetime.utcnow() - self.last_failure_time
        return elapsed >= timedelta(seconds=self.timeout_seconds)

    def _on_success(self) -> None:
        """Handle successful call."""
        self.failure_count = 0

        if self.state == CircuitState.HALF_OPEN:
            self.success_count += 1
            if self.success_count >= self.success_threshold:
                self.state = CircuitState.CLOSED
                self.success_count = 0

    def _on_failure(self) -> None:
        """Handle failed call."""
        self.failure_count += 1
        self.last_failure_time = datetime.utcnow()

        if self.state == CircuitState.HALF_OPEN:
            self.state = CircuitState.OPEN
        elif self.failure_count >= self.failure_threshold:
            self.state = CircuitState.OPEN

    @property
    def status(self) -> dict:
        """Get circuit breaker status."""
        return {
            "state": self.state.value,
            "failure_count": self.failure_count,
            "success_count": self.success_count,
            "last_failure_time": (
                self.last_failure_time.isoformat()
                if self.last_failure_time
                else None
            )
        }


# ============================================================================
# Main Wrapper Classes
# ============================================================================

class ConversationManager:
    """Manage multi-turn LLM conversations."""

    def __init__(
        self,
        system_prompt: str = "",
        max_turns: int = 10,
        timeout_seconds: float = 30.0
    ):
        self.context = ConversationContext(system_prompt=system_prompt)
        self.max_turns = max_turns
        self.timeout_seconds = timeout_seconds

    async def send_message(
        self,
        user_input: str,
        call_func: Callable,
        **kwargs
    ) -> str:
        """
        Send message and get response.

        Args:
            user_input: User's message
            call_func: Async function to call LLM
            **kwargs: Additional arguments for call_func

        Returns:
            Assistant's response
        """
        if self.context.turn_count >= self.max_turns:
            raise LLMError(
                f"Exceeded max turns: {self.max_turns}",
                ErrorCategory.PERMANENT
            )

        self.context.add_user_message(user_input)
        full_prompt = self._build_prompt(user_input)

        try:
            response = await call_func(full_prompt, **kwargs)
            self.context.add_assistant_message(response)
            return response
        except LLMError:
            if self.context.messages and self.context.messages[-1].role == "user":
                self.context.messages.pop()
            raise

    def _build_prompt(self, user_input: str) -> str:
        """Build complete prompt with history."""
        parts = []

        if self.context.system_prompt:
            parts.append(f"System:\n{self.context.system_prompt}\n")

        parts.append(self.context.get_conversation_string(format="natural"))
        parts.append(f"USER:\n{user_input}\n")
        parts.append("ASSISTANT:")

        return "\n".join(parts)

    def reset(self) -> None:
        """Reset conversation."""
        self.context.clear()

    def export_history(self, filepath: str) -> None:
        """Export conversation history to JSON."""
        history = {
            "system_prompt": self.context.system_prompt,
            "messages": [
                {
                    "role": msg.role,
                    "content": msg.content,
                    "timestamp": msg.timestamp.isoformat()
                }
                for msg in self.context.messages
            ]
        }

        with open(filepath, 'w') as f:
            json.dump(history, f, indent=2)


class LLMCliWrapper:
    """Base class for LLM CLI wrappers."""

    def __init__(
        self,
        cli_path: str,
        retry_config: Optional[RetryConfig] = None,
        timeout_config: Optional[TimeoutConfig] = None
    ):
        self.cli_path = cli_path
        self.retry_handler = RetryHandler(retry_config or RetryConfig())
        self.timeout_config = timeout_config or TimeoutConfig()
        self.circuit_breaker = CircuitBreaker()

    async def call_api_async(
        self,
        cmd_args: list[str],
        input_data: Optional[str] = None,
        output_model: Optional[type[BaseModel]] = None
    ) -> Union[str, dict]:
        """
        Call LLM CLI asynchronously with full error handling.

        Args:
            cmd_args: CLI arguments (without cli_path)
            input_data: stdin data
            output_model: Optional Pydantic model for validation

        Returns:
            Response string or validated model dict
        """

        async def _make_call():
            cmd = [self.cli_path] + cmd_args
            stdout, stderr, returncode = await SubprocessTimeout.run_with_timeout_async(
                cmd,
                input_data=input_data,
                timeout_seconds=self.timeout_config.total_execution
            )

            if returncode != 0:
                category, msg = categorize_error(
                    RuntimeError(f"CLI returned {returncode}"),
                    stderr=stderr
                )
                raise LLMError(msg, category, stderr=stderr)

            try:
                response_data = json.loads(stdout)
                content = response_data.get("content", [{}])[0].get("text", "")
            except (json.JSONDecodeError, KeyError, IndexError) as e:
                raise LLMError(
                    f"Invalid response format: {e}",
                    ErrorCategory.MALFORMED_OUTPUT,
                    e
                )

            if output_model:
                extractor = JSONExtractor()
                data, error = extractor.extract_json(content, output_model)
                if error:
                    raise LLMError(
                        f"Validation failed: {error}",
                        ErrorCategory.MALFORMED_OUTPUT
                    )
                return data

            return content

        return await self.retry_handler.async_call_with_retry(_make_call)

    def call_api_sync(
        self,
        cmd_args: list[str],
        input_data: Optional[str] = None,
        output_model: Optional[type[BaseModel]] = None
    ) -> Union[str, dict]:
        """Synchronous version of call_api_async."""

        def _make_call():
            cmd = [self.cli_path] + cmd_args
            stdout, stderr, returncode = SubprocessTimeout.run_with_timeout(
                cmd,
                input_data=input_data,
                timeout_seconds=self.timeout_config.total_execution
            )

            if returncode != 0:
                category, msg = categorize_error(
                    RuntimeError(f"CLI returned {returncode}"),
                    stderr=stderr
                )
                raise LLMError(msg, category, stderr=stderr)

            try:
                response_data = json.loads(stdout)
                content = response_data.get("content", [{}])[0].get("text", "")
            except (json.JSONDecodeError, KeyError, IndexError) as e:
                raise LLMError(
                    f"Invalid response format: {e}",
                    ErrorCategory.MALFORMED_OUTPUT,
                    e
                )

            if output_model:
                extractor = JSONExtractor()
                data, error = extractor.extract_json(content, output_model)
                if error:
                    raise LLMError(
                        f"Validation failed: {error}",
                        ErrorCategory.MALFORMED_OUTPUT
                    )
                return data

            return content

        return self.retry_handler.sync_call_with_retry(_make_call)


__all__ = [
    'LLMError',
    'ErrorCategory',
    'RetryConfig',
    'TimeoutConfig',
    'Message',
    'ConversationContext',
    'JSONExtractor',
    'RetryHandler',
    'ProgressiveTimeoutManager',
    'SubprocessTimeout',
    'CircuitBreaker',
    'CircuitState',
    'ConversationManager',
    'LLMCliWrapper',
    'categorize_error',
]
