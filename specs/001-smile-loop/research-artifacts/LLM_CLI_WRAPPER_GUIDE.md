# Python LLM CLI Wrapper Best Practices

A comprehensive guide for building production-ready Python wrappers around LLM command-line interfaces (Claude, Gemini, Codex, etc.) using subprocess, httpx, and Pydantic.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Structured Output Handling](#structured-output-handling)
3. [Error Handling & Retry Patterns](#error-handling--retry-patterns)
4. [Timeout Management](#timeout-management)
5. [Multi-Turn Conversation Patterns](#multi-turn-conversation-patterns)
6. [Complete Production Examples](#complete-production-examples)
7. [Testing Strategies](#testing-strategies)

---

## Architecture Overview

### Design Principles

**1. Process Isolation**
- Run LLM CLI in separate processes to isolate failures
- Capture stdout/stderr to parse structured output
- Use timeout patterns to prevent hung processes

**2. Structured Communication**
- Request JSON schemas from LLMs using prompt engineering
- Validate responses with Pydantic models
- Parse partial/malformed JSON gracefully

**3. Retry & Resilience**
- Implement exponential backoff for transient failures
- Distinguish between retryable and permanent errors
- Use circuit breakers for degraded services

**4. Observability**
- Log all subprocess calls (cmd, input, output)
- Track timing metrics (latency, timeouts)
- Instrument error rates by type

### Key Components

```
┌─────────────────────────────────────────┐
│      Python Application                 │
│  (FastAPI, Script, etc.)               │
└──────────────┬──────────────────────────┘
               │
               │ (Pydantic validation)
               ▼
┌──────────────────────────────────────────┐
│    LLMWrapper (Base Class)              │
│  - Process lifecycle                    │
│  - Stream handling                      │
│  - Error parsing                        │
└──────────┬──────────────────────────────┘
           │
           │ (Retry/backoff logic)
           ▼
┌──────────────────────────────────────────┐
│  RetryHandler (Exponential Backoff)     │
│  - Transient error detection            │
│  - Jitter addition                      │
│  - Circuit breaker (optional)           │
└──────────┬──────────────────────────────┘
           │
           │ (subprocess.Popen)
           ▼
┌──────────────────────────────────────────┐
│  External CLI Process                   │
│  (claude, gemini, codex binary)         │
└──────────────────────────────────────────┘
```

---

## Structured Output Handling

### 1. Prompt Engineering for JSON Output

**Best Practice**: Always specify the expected JSON schema in your prompt.

```python
def construct_json_prompt(
    user_query: str,
    output_schema: dict,
    temperature: float = 0.0  # Use 0 for deterministic output
) -> str:
    """
    Construct a prompt that forces structured JSON output.

    Args:
        user_query: The user's request
        output_schema: Pydantic model schema (dict or JSON string)
        temperature: LLM temperature (0 for strict JSON)

    Returns:
        Formatted prompt with JSON constraints
    """
    schema_str = json.dumps(output_schema, indent=2)

    return f"""You are a helpful assistant that ALWAYS responds with valid JSON.

User Query:
{user_query}

Required JSON Output Schema:
{schema_str}

IMPORTANT RULES:
1. Your response MUST be valid JSON matching the schema above
2. Do not include any text before or after the JSON
3. Do not include markdown code blocks (```)
4. All required fields must be present
5. Use null for missing optional fields

Respond with only valid JSON:"""
```

### 2. Pydantic Models for Validation

**Best Practice**: Use Pydantic for runtime validation with helpful error messages.

```python
from pydantic import BaseModel, Field, field_validator
from typing import Optional, List
from datetime import datetime

class AnalysisResult(BaseModel):
    """Validated LLM analysis output."""

    summary: str = Field(
        ...,
        min_length=10,
        max_length=1000,
        description="Brief summary of analysis"
    )
    key_points: List[str] = Field(
        default_factory=list,
        max_length=10,
        description="Top findings (max 10)"
    )
    confidence: float = Field(
        ...,
        ge=0.0,
        le=1.0,
        description="Confidence score 0-1"
    )
    timestamp: datetime = Field(
        default_factory=datetime.utcnow
    )
    metadata: dict = Field(default_factory=dict)

    @field_validator('summary')
    @classmethod
    def validate_summary(cls, v: str) -> str:
        """Validate summary is not just placeholder text."""
        if v.lower() in ('n/a', 'unknown', 'error'):
            raise ValueError(f"Invalid summary: {v}")
        return v

    class Config:
        json_schema_extra = {
            "example": {
                "summary": "Analysis of code quality",
                "key_points": ["Good type hints", "Missing docstrings"],
                "confidence": 0.92,
                "timestamp": "2024-01-15T10:30:00",
                "metadata": {"model": "claude-3"}
            }
        }

class AnalysisResponse(BaseModel):
    """Validated response wrapper."""
    success: bool
    result: Optional[AnalysisResult] = None
    error: Optional[str] = None
```

### 3. Robust JSON Extraction

**Best Practice**: Handle malformed JSON with graceful fallbacks.

```python
import json
from typing import Any, Optional
import re

class JSONExtractor:
    """Extract and validate JSON from LLM responses."""

    @staticmethod
    def extract_json(
        text: str,
        model_class: type[BaseModel],
        strict: bool = False
    ) -> tuple[Optional[dict], Optional[str]]:
        """
        Extract JSON from text, handling common LLM output issues.

        Args:
            text: Raw LLM output
            model_class: Pydantic model for validation
            strict: If True, raise on parse errors; if False, attempt recovery

        Returns:
            (parsed_dict, error_message) tuple
        """
        # Remove markdown code blocks
        text = re.sub(r'^```(?:json)?\n?|\n?```$', '', text.strip(), flags=re.MULTILINE)

        # Try direct JSON parse first
        try:
            data = json.loads(text)
            # Validate with Pydantic
            model_class(**data)
            return data, None
        except json.JSONDecodeError as e:
            if strict:
                return None, f"Invalid JSON: {e}"
            # Try partial JSON extraction
            return JSONExtractor._recover_json(text, model_class)
        except ValueError as e:
            return None, f"Validation failed: {e}"

    @staticmethod
    def _recover_json(text: str, model_class: type[BaseModel]) -> tuple[Optional[dict], Optional[str]]:
        """Attempt to recover from malformed JSON."""
        # Find potential JSON structures
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
        """Get Pydantic schema as formatted JSON for prompts."""
        schema = model_class.model_json_schema()
        return json.dumps(schema, indent=2)


# Usage Example
extractor = JSONExtractor()
llm_output = """
Some preamble text...

{
  "summary": "Code review findings",
  "key_points": ["Missing error handling"],
  "confidence": 0.85
}

Additional notes...
"""

data, error = extractor.extract_json(llm_output, AnalysisResult)
if error:
    print(f"Extraction failed: {error}")
else:
    result = AnalysisResult(**data)
    print(f"Parsed: {result.summary}")
```

### 4. Streaming Response Handler

**Best Practice**: Handle streaming responses line-by-line for real-time output.

```python
from typing import AsyncIterator
import asyncio

class StreamingResponseHandler:
    """Handle streaming LLM responses with line buffering."""

    @staticmethod
    async def stream_json_chunks(
        process,
        buffer_size: int = 8192
    ) -> AsyncIterator[dict]:
        """
        Stream JSON objects from subprocess stdout.
        Assumes each line or logical unit is a JSON object.

        Args:
            process: subprocess.Popen object with stdout pipe
            buffer_size: Read buffer size

        Yields:
            Parsed JSON objects
        """
        buffer = ""

        try:
            while True:
                # Non-blocking read
                chunk = await asyncio.to_thread(
                    process.stdout.read,
                    buffer_size
                )
                if not chunk:
                    break

                buffer += chunk.decode('utf-8', errors='replace')

                # Process complete JSON objects (separated by newlines)
                while '\n' in buffer:
                    line, buffer = buffer.split('\n', 1)
                    line = line.strip()

                    if not line:
                        continue

                    try:
                        obj = json.loads(line)
                        yield obj
                    except json.JSONDecodeError:
                        # Log malformed line, continue
                        print(f"Warning: Malformed JSON line: {line[:100]}")

        finally:
            # Process any remaining buffer
            if buffer.strip():
                try:
                    obj = json.loads(buffer)
                    yield obj
                except json.JSONDecodeError:
                    pass

    @staticmethod
    async def collect_streaming_response(
        process,
        model_class: type[BaseModel],
        timeout_seconds: float = 60.0
    ) -> list[dict]:
        """
        Collect all streaming responses into a list.

        Args:
            process: subprocess.Popen object
            model_class: Pydantic model for validation
            timeout_seconds: Maximum collection time

        Returns:
            List of validated JSON objects
        """
        results = []

        try:
            async with asyncio.timeout(timeout_seconds):
                async for chunk in StreamingResponseHandler.stream_json_chunks(process):
                    try:
                        # Validate each chunk
                        validated = model_class(**chunk)
                        results.append(validated.model_dump())
                    except ValueError as e:
                        print(f"Validation error: {e}")

        except asyncio.TimeoutError:
            print(f"Stream collection timeout after {timeout_seconds}s")

        return results
```

---

## Error Handling & Retry Patterns

### 1. Error Classification

**Best Practice**: Distinguish between retryable and permanent errors.

```python
from enum import Enum
from typing import Callable

class ErrorCategory(Enum):
    """Classify subprocess errors for proper handling."""

    RETRYABLE = "retryable"           # Transient, safe to retry
    PERMANENT = "permanent"            # Won't succeed with retry
    RATE_LIMIT = "rate_limit"         # API rate limit
    TIMEOUT = "timeout"               # Process timeout
    MALFORMED_OUTPUT = "malformed"    # Can't parse output
    AUTH_FAILURE = "auth"             # Authentication/authorization
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
        """Check if this error should trigger a retry."""
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
) -> tuple[ErrorCategory, str]:
    """
    Categorize an error for proper retry handling.

    Args:
        exception: The exception that occurred
        stderr: Process stderr output
        stdout: Process stdout output

    Returns:
        (ErrorCategory, detailed_message) tuple
    """
    exc_str = str(exception).lower()
    stderr_lower = stderr.lower()

    # Check for specific patterns
    if isinstance(exception, subprocess.TimeoutExpired):
        return ErrorCategory.TIMEOUT, f"Process timed out: {exception}"

    if "rate limit" in stderr_lower or "too many requests" in stderr_lower:
        return ErrorCategory.RATE_LIMIT, "API rate limit exceeded"

    if "authentication" in stderr_lower or "unauthorized" in stderr_lower:
        return ErrorCategory.AUTH_FAILURE, "Authentication/authorization failed"

    if "connection" in exc_str or "network" in exc_str:
        return ErrorCategory.RETRYABLE, f"Network error: {exception}"

    if "timeout" in exc_str:
        return ErrorCategory.TIMEOUT, f"Timeout: {exception}"

    if isinstance(exception, json.JSONDecodeError):
        return ErrorCategory.MALFORMED_OUTPUT, f"Invalid JSON: {exception}"

    if "permission denied" in stderr_lower or "access denied" in stderr_lower:
        return ErrorCategory.AUTH_FAILURE, "Access denied"

    # Default to permanent for unknown errors
    return ErrorCategory.PERMANENT, f"Permanent error: {exception}"


# Usage with subprocess
try:
    result = subprocess.run(
        ["claude", "api", "call"],
        capture_output=True,
        timeout=30,
        text=True
    )
except subprocess.TimeoutExpired as e:
    category, msg = categorize_error(e, "", "")
    raise LLMError(msg, category, e)
except Exception as e:
    category, msg = categorize_error(e, "", "")
    raise LLMError(msg, category, e)
```

### 2. Retry Handler with Exponential Backoff

**Best Practice**: Use exponential backoff with jitter for distributed retries.

```python
import time
import random
from dataclasses import dataclass
from typing import TypeVar, Callable, Any

T = TypeVar('T')

@dataclass
class RetryConfig:
    """Configuration for retry behavior."""

    max_attempts: int = 3
    initial_delay_ms: float = 100
    max_delay_ms: float = 30000
    exponential_base: float = 2.0
    jitter: bool = True
    retryable_exceptions: tuple = (
        subprocess.TimeoutExpired,
        ConnectionError,
        TimeoutError
    )

class RetryHandler:
    """Handle retries with exponential backoff and jitter."""

    def __init__(self, config: RetryConfig = RetryConfig()):
        self.config = config

    def calculate_delay(self, attempt: int) -> float:
        """
        Calculate delay for attempt (exponential backoff with jitter).

        Args:
            attempt: 0-indexed attempt number

        Returns:
            Delay in seconds
        """
        # Exponential backoff: initial * base^attempt
        delay_ms = min(
            self.config.initial_delay_ms * (self.config.exponential_base ** attempt),
            self.config.max_delay_ms
        )

        # Add jitter (±25%)
        if self.config.jitter:
            jitter_range = delay_ms * 0.25
            delay_ms += random.uniform(-jitter_range, jitter_range)

        return max(0, delay_ms) / 1000.0  # Convert to seconds

    async def async_call_with_retry(
        self,
        func: Callable[..., Any],
        *args,
        **kwargs
    ) -> Any:
        """
        Execute async function with exponential backoff retry.

        Args:
            func: Async callable to execute
            *args, **kwargs: Arguments to pass to func

        Returns:
            Result from func

        Raises:
            LLMError: If all retries fail
        """
        last_error: Optional[Exception] = None

        for attempt in range(self.config.max_attempts):
            try:
                return await func(*args, **kwargs)

            except LLMError as e:
                last_error = e

                # Don't retry if not retryable
                if not e.is_retryable:
                    raise

                # Don't retry on last attempt
                if attempt >= self.config.max_attempts - 1:
                    raise

                delay = self.calculate_delay(attempt)
                print(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s: {e}")
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
                print(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s: {e}")
                await asyncio.sleep(delay)

        # Should not reach here
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

                if not e.is_retryable:
                    raise

                if attempt >= self.config.max_attempts - 1:
                    raise

                delay = self.calculate_delay(attempt)
                print(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s: {e}")
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
                print(f"Retry {attempt + 1}/{self.config.max_attempts} after {delay:.2f}s: {e}")
                time.sleep(delay)

        if last_error:
            raise last_error
        raise LLMError("Retry logic error", ErrorCategory.UNKNOWN)


# Usage
retry_config = RetryConfig(
    max_attempts=3,
    initial_delay_ms=100,
    max_delay_ms=5000
)

handler = RetryHandler(retry_config)

async def call_llm_api():
    return await handler.async_call_with_retry(
        some_llm_function,
        prompt="Analyze this code",
        timeout=30
    )
```

### 3. Circuit Breaker Pattern

**Best Practice**: Prevent cascading failures with circuit breaker.

```python
from enum import Enum
from datetime import datetime, timedelta

class CircuitState(Enum):
    """Circuit breaker states."""
    CLOSED = "closed"      # Normal operation
    OPEN = "open"         # Failing, reject requests
    HALF_OPEN = "half_open"  # Testing if service recovered

class CircuitBreaker:
    """
    Circuit breaker for protecting against cascading failures.

    Transitions:
    - CLOSED → OPEN: When error rate exceeds threshold
    - OPEN → HALF_OPEN: After timeout period
    - HALF_OPEN → CLOSED: If success
    - HALF_OPEN → OPEN: If failure
    """

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

    def call(
        self,
        func: Callable[..., T],
        *args,
        **kwargs
    ) -> T:
        """
        Execute func through circuit breaker.

        Raises:
            LLMError: If circuit is open
        """
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
        """Check if enough time has passed to attempt reset."""
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
            "last_failure_time": self.last_failure_time.isoformat() if self.last_failure_time else None
        }
```

---

## Timeout Management

### 1. Subprocess Timeout Patterns

**Best Practice**: Use Popen for fine-grained timeout control.

```python
from typing import Optional
import subprocess
import signal

class SubprocessTimeout:
    """Handle subprocess timeouts correctly."""

    @staticmethod
    def run_with_timeout(
        cmd: list[str],
        input_data: Optional[str] = None,
        timeout_seconds: float = 30.0,
        env: Optional[dict] = None
    ) -> tuple[str, str, int]:
        """
        Run subprocess with proper timeout handling.

        Args:
            cmd: Command as list (e.g., ["claude", "api", "call"])
            input_data: stdin data
            timeout_seconds: Timeout in seconds
            env: Environment variables

        Returns:
            (stdout, stderr, return_code) tuple

        Raises:
            LLMError: On timeout or other errors
        """
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
                # Kill process and get remaining output
                process.kill()

                try:
                    # Try to get any remaining output
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
            # Ensure process is cleaned up
            if process and process.poll() is None:
                try:
                    process.kill()
                    process.wait(timeout=5)
                except:
                    pass

    @staticmethod
    async def run_with_timeout_async(
        cmd: list[str],
        input_data: Optional[str] = None,
        timeout_seconds: float = 30.0,
        env: Optional[dict] = None
    ) -> tuple[str, str, int]:
        """Async version of timeout-aware subprocess runner."""

        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env
        )

        try:
            stdout_data, stderr_data = await asyncio.wait_for(
                process.communicate(input=input_data.encode() if input_data else None),
                timeout=timeout_seconds
            )

            return (
                stdout_data.decode('utf-8', errors='replace'),
                stderr_data.decode('utf-8', errors='replace'),
                process.returncode or 0
            )

        except asyncio.TimeoutError:
            # Kill process
            process.kill()

            try:
                await asyncio.wait_for(process.wait(), timeout=5)
            except asyncio.TimeoutError:
                # Force kill if graceful kill doesn't work
                if hasattr(process, 'kill'):
                    os.kill(process.pid, signal.SIGKILL)

            raise LLMError(
                f"Process timed out after {timeout_seconds}s",
                ErrorCategory.TIMEOUT,
                asyncio.TimeoutError(timeout_seconds)
            )
```

### 2. Progressive Timeout Strategy

**Best Practice**: Use different timeouts for different phases.

```python
from dataclasses import dataclass

@dataclass
class TimeoutConfig:
    """Timeouts for different subprocess phases."""

    initial_startup: float = 5.0      # Time to start process
    first_response: float = 10.0       # Time to first output
    chunk_receive: float = 5.0         # Time between chunks
    total_execution: float = 60.0      # Total time limit
    graceful_shutdown: float = 5.0     # Time to shutdown

class ProgressiveTimeoutManager:
    """Manage timeouts across subprocess lifecycle."""

    def __init__(self, config: TimeoutConfig = TimeoutConfig()):
        self.config = config
        self.start_time = None
        self.last_activity_time = None

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

        elapsed = datetime.utcnow() - self.start_time
        remaining = self.config.total_execution - elapsed.total_seconds()
        return max(0, remaining)

    def check_chunk_timeout(self) -> float:
        """
        Check if chunk timeout expired and return remaining total time.

        Raises:
            LLMError: If chunk timeout exceeded
        """
        if not self.last_activity_time:
            return self.config.chunk_receive

        elapsed = (datetime.utcnow() - self.last_activity_time).total_seconds()

        if elapsed > self.config.chunk_receive:
            raise LLMError(
                f"No data received for {elapsed:.1f}s (timeout: {self.config.chunk_receive}s)",
                ErrorCategory.TIMEOUT
            )

        return self.get_remaining_time()


# Usage in streaming context
timeout_manager = ProgressiveTimeoutManager(
    TimeoutConfig(
        initial_startup=5,
        total_execution=120,
        chunk_receive=10
    )
)

timeout_manager.start()

try:
    async for chunk in stream_response():
        timeout_manager.activity()
        remaining = timeout_manager.check_chunk_timeout()
        process_chunk(chunk)
except LLMError as e:
    if e.category == ErrorCategory.TIMEOUT:
        handle_timeout(e)
```

---

## Multi-Turn Conversation Patterns

### 1. Conversation State Management

**Best Practice**: Maintain conversation context with message history.

```python
from typing import Literal
from dataclasses import dataclass, field

MessageRole = Literal["user", "assistant", "system"]

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
    max_history: int = 20  # Prevent context explosion

    def add_user_message(self, content: str, metadata: dict = None) -> None:
        """Add user message to history."""
        self.messages.append(Message(
            role="user",
            content=content,
            metadata=metadata or {}
        ))
        self._trim_history()

    def add_assistant_message(self, content: str, metadata: dict = None) -> None:
        """Add assistant response to history."""
        self.messages.append(Message(
            role="assistant",
            content=content,
            metadata=metadata or {}
        ))
        self._trim_history()

    def _trim_history(self) -> None:
        """Keep only recent messages to prevent context overflow."""
        if len(self.messages) > self.max_history:
            # Keep system context (first message if system role)
            if self.messages and self.messages[0].role == "system":
                self.messages = [
                    self.messages[0],
                    *self.messages[-(self.max_history - 1):]
                ]
            else:
                self.messages = self.messages[-self.max_history:]

    def get_conversation_string(self, format: str = "natural") -> str:
        """
        Get formatted conversation history for prompt.

        Args:
            format: "natural" (readable) or "json" (structured)

        Returns:
            Formatted conversation string
        """
        if format == "natural":
            parts = []
            for msg in self.messages:
                if msg.role == "system":
                    continue  # System prompt handled separately
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
        """Number of user turns (exchanges)."""
        return sum(1 for msg in self.messages if msg.role == "user")


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
        model_class: Optional[type[BaseModel]] = None,
        **llm_kwargs
    ) -> str:
        """
        Send message and get response.

        Args:
            user_input: User's message
            model_class: Optional Pydantic model for response validation
            **llm_kwargs: Additional arguments for LLM call

        Returns:
            Assistant's response

        Raises:
            LLMError: If max turns exceeded or other errors
        """
        if self.context.turn_count >= self.max_turns:
            raise LLMError(
                f"Exceeded max conversation turns: {self.max_turns}",
                ErrorCategory.PERMANENT
            )

        self.context.add_user_message(user_input)

        # Build prompt with conversation history
        full_prompt = self._build_prompt(user_input)

        try:
            response = await self._call_llm(full_prompt, model_class, **llm_kwargs)
            self.context.add_assistant_message(response)
            return response

        except LLMError as e:
            # Remove last user message on error
            if self.context.messages and self.context.messages[-1].role == "user":
                self.context.messages.pop()
            raise

    def _build_prompt(self, user_input: str) -> str:
        """Build complete prompt with history."""
        parts = []

        if self.context.system_prompt:
            parts.append(f"System:\n{self.context.system_prompt}\n")

        # Add conversation history
        parts.append(self.context.get_conversation_string(format="natural"))

        # Add current user input
        parts.append(f"USER:\n{user_input}\n")
        parts.append("ASSISTANT:")

        return "\n".join(parts)

    async def _call_llm(
        self,
        prompt: str,
        model_class: Optional[type[BaseModel]] = None,
        **kwargs
    ) -> str:
        """Call LLM and get response (placeholder)."""
        # Implementation depends on specific CLI tool
        raise NotImplementedError("Implement LLM-specific call")

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
```

### 2. Request/Response Serialization

**Best Practice**: Use Pydantic for consistent serialization.

```python
from pydantic import BaseModel, Field

class LLMRequest(BaseModel):
    """Structured LLM request."""

    prompt: str
    system_prompt: Optional[str] = None
    model: str = "claude-3-sonnet"
    temperature: float = Field(0.0, ge=0.0, le=1.0)
    max_tokens: Optional[int] = None
    top_p: float = Field(1.0, ge=0.0, le=1.0)
    metadata: dict = Field(default_factory=dict)

class LLMResponse(BaseModel):
    """Structured LLM response."""

    content: str
    model: str
    tokens_used: int
    finish_reason: str  # "stop", "max_tokens", "error"
    latency_ms: float
    metadata: dict = Field(default_factory=dict)

class ConversationTurn(BaseModel):
    """Single turn in a conversation."""

    turn_number: int
    request: LLMRequest
    response: LLMResponse
    timestamp: datetime = Field(default_factory=datetime.utcnow)
```

---

## Complete Production Examples

### Example 1: Claude CLI Wrapper

```python
import subprocess
import json
import asyncio
from pathlib import Path

class ClaudeCliWrapper:
    """Production wrapper for Anthropic's Claude CLI."""

    def __init__(
        self,
        cli_path: str = "claude",
        default_model: str = "claude-3-5-sonnet",
        retry_config: Optional[RetryConfig] = None,
        timeout_config: Optional[TimeoutConfig] = None
    ):
        self.cli_path = cli_path
        self.default_model = default_model
        self.retry_handler = RetryHandler(retry_config or RetryConfig())
        self.timeout_config = timeout_config or TimeoutConfig()
        self.circuit_breaker = CircuitBreaker()

    async def call_api(
        self,
        prompt: str,
        system_prompt: Optional[str] = None,
        output_model: Optional[type[BaseModel]] = None,
        temperature: float = 0.0,
        max_tokens: Optional[int] = None,
        **kwargs
    ) -> Union[str, dict]:
        """
        Call Claude API via CLI with full error handling.

        Args:
            prompt: User prompt
            system_prompt: System context
            output_model: Pydantic model for response validation
            temperature: LLM temperature
            max_tokens: Maximum tokens in response
            **kwargs: Additional CLI arguments

        Returns:
            Response string or validated Pydantic model dict
        """

        async def _make_call():
            return await self._execute_cli(
                prompt=prompt,
                system_prompt=system_prompt,
                output_model=output_model,
                temperature=temperature,
                max_tokens=max_tokens,
                **kwargs
            )

        try:
            return await self.circuit_breaker.call(
                lambda: self.retry_handler.async_call_with_retry(_make_call)
            )
        except LLMError as e:
            # Log for observability
            self._log_error(e, prompt)
            raise

    async def _execute_cli(
        self,
        prompt: str,
        system_prompt: Optional[str] = None,
        output_model: Optional[type[BaseModel]] = None,
        temperature: float = 0.0,
        max_tokens: Optional[int] = None,
        **kwargs
    ) -> Union[str, dict]:
        """Execute CLI and parse response."""

        # Build CLI command
        cmd = [
            self.cli_path,
            "api",
            "call"
        ]

        # Prepare input
        input_data = json.dumps({
            "model": self.default_model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "system": system_prompt,
            "temperature": temperature,
            **({"max_tokens": max_tokens} if max_tokens else {}),
            **kwargs
        })

        # Execute with timeout
        try:
            stdout, stderr, returncode = await SubprocessTimeout.run_with_timeout_async(
                cmd,
                input_data=input_data,
                timeout_seconds=self.timeout_config.total_execution
            )

        except LLMError:
            raise
        except Exception as e:
            category, msg = categorize_error(e, "", "")
            raise LLMError(msg, category, e)

        # Check return code
        if returncode != 0:
            category, msg = categorize_error(
                RuntimeError(f"CLI returned {returncode}"),
                stderr=stderr
            )
            raise LLMError(msg, category, stderr=stderr)

        # Parse response
        try:
            response_data = json.loads(stdout)
            content = response_data.get("content", [{}])[0].get("text", "")

        except json.JSONDecodeError as e:
            raise LLMError(
                f"Invalid JSON response: {e}",
                ErrorCategory.MALFORMED_OUTPUT,
                e
            )

        # Validate with model if provided
        if output_model:
            extractor = JSONExtractor()
            data, error = extractor.extract_json(content, output_model)
            if error:
                raise LLMError(
                    f"Response validation failed: {error}",
                    ErrorCategory.MALFORMED_OUTPUT
                )
            return data

        return content

    async def stream_response(
        self,
        prompt: str,
        system_prompt: Optional[str] = None
    ) -> AsyncIterator[str]:
        """
        Stream response from Claude CLI.

        Yields:
            Response chunks
        """

        cmd = [self.cli_path, "api", "call", "--stream"]

        input_data = json.dumps({
            "model": self.default_model,
            "messages": [{"role": "user", "content": prompt}],
            "system": system_prompt
        })

        process = None
        timeout_manager = ProgressiveTimeoutManager(self.timeout_config)
        timeout_manager.start()

        try:
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )

            await process.stdin.write(input_data.encode())
            await process.stdin.drain()
            process.stdin.close()

            async for chunk in StreamingResponseHandler.stream_json_chunks(
                process,
                buffer_size=8192
            ):
                timeout_manager.activity()
                remaining = timeout_manager.check_chunk_timeout()

                if "text" in chunk:
                    yield chunk["text"]

        finally:
            if process and process.returncode is None:
                process.kill()
                try:
                    await asyncio.wait_for(process.wait(), timeout=5)
                except asyncio.TimeoutError:
                    pass

    def _log_error(self, error: LLMError, prompt: str) -> None:
        """Log error for observability."""
        print(f"LLM Error: {error.category.value}")
        print(f"Message: {error.message}")
        print(f"Prompt: {prompt[:100]}...")
        if error.stderr:
            print(f"Stderr: {error.stderr[:200]}")


# Usage
async def main():
    wrapper = ClaudeCliWrapper(
        timeout_config=TimeoutConfig(total_execution=120)
    )

    result = await wrapper.call_api(
        prompt="Explain quantum computing in 3 sentences",
        temperature=0.0,
        max_tokens=200
    )

    print(result)

    # Stream usage
    print("\nStreaming response:")
    async for chunk in wrapper.stream_response("Tell a short story"):
        print(chunk, end="", flush=True)

if __name__ == "__main__":
    asyncio.run(main())
```

### Example 2: FastAPI Integration

```python
from fastapi import FastAPI, BackgroundTasks
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

app = FastAPI(title="LLM API Wrapper")

class AnalysisRequest(BaseModel):
    code: str
    language: str = "python"

class AnalysisResponse(BaseModel):
    summary: str
    issues: list[str]
    suggestions: list[str]

claude = ClaudeCliWrapper()

@app.post("/analyze", response_model=AnalysisResponse)
async def analyze_code(request: AnalysisRequest):
    """Analyze code using Claude CLI."""

    prompt = f"""Analyze this {request.language} code:

```{request.language}
{request.code}
```

Provide:
1. Summary of what the code does
2. List of potential issues
3. List of suggestions for improvement

Respond with valid JSON only."""

    system_prompt = "You are a code review expert."

    try:
        response = await claude.call_api(
            prompt=prompt,
            system_prompt=system_prompt,
            output_model=AnalysisResponse,
            temperature=0.0
        )
        return response

    except LLMError as e:
        raise HTTPException(status_code=503, detail=str(e))

@app.get("/stream-explanation")
async def stream_explanation(query: str):
    """Stream explanation from Claude."""

    async def generate():
        try:
            async for chunk in claude.stream_response(
                f"Explain: {query}"
            ):
                yield chunk
        except LLMError as e:
            yield f"\nError: {e.message}"

    return StreamingResponse(generate(), media_type="text/plain")
```

---

## Testing Strategies

### 1. Unit Tests with Mocking

```python
import pytest
from unittest.mock import AsyncMock, Mock, patch

@pytest.mark.asyncio
async def test_claude_cli_success():
    """Test successful CLI call."""

    wrapper = ClaudeCliWrapper()

    mock_response = json.dumps({
        "content": [{"text": "Analysis complete"}]
    })

    with patch(
        'asyncio.create_subprocess_exec'
    ) as mock_exec:
        mock_process = AsyncMock()
        mock_process.communicate.return_value = (
            mock_response.encode(),
            b""
        )
        mock_process.returncode = 0
        mock_exec.return_value = mock_process

        result = await wrapper._execute_cli(
            prompt="Test prompt",
            temperature=0.0
        )

        assert result == "Analysis complete"

@pytest.mark.asyncio
async def test_timeout_handling():
    """Test timeout error handling."""

    wrapper = ClaudeCliWrapper(
        timeout_config=TimeoutConfig(total_execution=0.1)
    )

    with pytest.raises(LLMError) as exc_info:
        await wrapper._execute_cli(
            prompt="Long prompt",
            temperature=0.0
        )

    assert exc_info.value.category == ErrorCategory.TIMEOUT

def test_retry_logic():
    """Test exponential backoff retry."""

    config = RetryConfig(
        max_attempts=3,
        initial_delay_ms=10,
        exponential_base=2.0
    )

    handler = RetryHandler(config)

    # Test delay calculations
    assert handler.calculate_delay(0) >= 0.01  # 10ms
    assert handler.calculate_delay(1) >= 0.02  # 20ms
    assert handler.calculate_delay(2) >= 0.04  # 40ms
```

### 2. Integration Tests

```python
@pytest.mark.asyncio
async def test_full_conversation_flow():
    """Test multi-turn conversation."""

    manager = ConversationManager(
        system_prompt="You are a helpful assistant",
        max_turns=3
    )

    # Mock the LLM call
    with patch.object(manager, '_call_llm') as mock_call:
        mock_call.return_value = "Response 1"

        response = await manager.send_message("Hello")
        assert response == "Response 1"
        assert manager.context.turn_count == 1
        assert manager.context.message_count == 2  # User + Assistant
```

---

## Key Takeaways

### 1. Architecture
- Separate CLI invocation from response parsing
- Use Pydantic for validation at boundaries
- Implement retry logic with exponential backoff

### 2. Error Handling
- Categorize errors for proper retry decisions
- Use circuit breakers for cascading failure protection
- Log all errors with context for debugging

### 3. Timeout Management
- Use progressive timeouts for different lifecycle phases
- Clean up processes aggressively on timeout
- Distinguish between transient and permanent failures

### 4. Multi-Turn Interactions
- Maintain conversation state with ConversationContext
- Trim history to prevent context explosion
- Validate structured outputs with Pydantic

### 5. Testing
- Mock subprocess calls for deterministic tests
- Test timeout paths explicitly
- Validate retry logic with configurable delays

---

## References

- [Complete Ollama Tutorial (2026) – LLMs via CLI, Cloud & Python - DEV Community](https://dev.to/proflead/complete-ollama-tutorial-2026-llms-via-cli-cloud-python-3m97)
- [Using LLMs on the Command Line with llm: Practical Examples for Developers | Samuel Liedtke](https://www.samuelliedtke.com/blog/using-llms-on-the-command-line/)
- [LLM: A CLI utility and Python library for interacting with Large Language Models](https://llm.datasette.io/)
- [How to Use Pydantic for LLMs: Schema, Validation & Prompts description: | Pydantic](https://pydantic.dev/articles/llm-intro)
- [The Complete Guide to Using Pydantic for Validating LLM Outputs - MachineLearningMastery.com](https://machinelearningmastery.com/the-complete-guide-to-using-pydantic-for-validating-llm-outputs/)
- [Python API - LLM](https://llm.datasette.io/en/stable/python-api.html)
- [Anthropic Claude Tutorial: Structured Outputs with Instructor - Instructor](https://python.useinstructor.com/integrations/anthropic/)
- [subprocess — Subprocess management](https://docs.python.org/3/library/subprocess.html)
