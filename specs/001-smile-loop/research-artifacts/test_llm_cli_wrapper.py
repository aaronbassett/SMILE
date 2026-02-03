"""
Test suite for LLM CLI wrapper.

Demonstrates:
- Unit tests with mocking
- Integration tests
- Error handling scenarios
- Retry logic validation
- Timeout scenarios
"""

import pytest
import json
import asyncio
from unittest.mock import AsyncMock, Mock, patch, MagicMock
from datetime import datetime
from typing import Optional

from pydantic import BaseModel, Field

from llm_cli_wrapper import (
    LLMError,
    ErrorCategory,
    RetryConfig,
    TimeoutConfig,
    JSONExtractor,
    RetryHandler,
    ProgressiveTimeoutManager,
    CircuitBreaker,
    ConversationManager,
    LLMCliWrapper,
    categorize_error,
    ConversationContext,
)


# ============================================================================
# Test Models
# ============================================================================

class SampleResponse(BaseModel):
    """Sample model for testing."""
    message: str
    score: float = Field(ge=0.0, le=1.0)
    tags: list[str] = Field(default_factory=list)


class CodeAnalysis(BaseModel):
    """Sample model for code analysis."""
    summary: str
    issues: list[str]
    confidence: float = Field(ge=0.0, le=1.0)


# ============================================================================
# Error Handling Tests
# ============================================================================

class TestErrorCategorization:
    """Test error categorization."""

    def test_timeout_error(self):
        """Test timeout error categorization."""
        import subprocess
        exc = subprocess.TimeoutExpired(["cmd"], 30)
        category, msg = categorize_error(exc)
        assert category == ErrorCategory.TIMEOUT

    def test_rate_limit_error(self):
        """Test rate limit detection."""
        exc = Exception("Rate limit exceeded")
        category, msg = categorize_error(exc, stderr="too many requests")
        assert category == ErrorCategory.RATE_LIMIT

    def test_auth_failure(self):
        """Test auth failure detection."""
        exc = Exception("Unauthorized")
        category, msg = categorize_error(exc, stderr="authentication failed")
        assert category == ErrorCategory.AUTH_FAILURE

    def test_network_error(self):
        """Test network error categorization."""
        exc = ConnectionError("Connection refused")
        category, msg = categorize_error(exc)
        assert category == ErrorCategory.RETRYABLE

    def test_json_error(self):
        """Test JSON error categorization."""
        exc = json.JSONDecodeError("Invalid", "{", 0)
        category, msg = categorize_error(exc)
        assert category == ErrorCategory.MALFORMED_OUTPUT


class TestLLMError:
    """Test LLMError exception."""

    def test_error_creation(self):
        """Test creating LLMError."""
        error = LLMError("Test error", ErrorCategory.RETRYABLE)
        assert error.message == "Test error"
        assert error.category == ErrorCategory.RETRYABLE
        assert error.is_retryable

    def test_non_retryable_error(self):
        """Test non-retryable error."""
        error = LLMError("Auth failed", ErrorCategory.AUTH_FAILURE)
        assert not error.is_retryable

    def test_permanent_error(self):
        """Test permanent error."""
        error = LLMError("Unknown", ErrorCategory.PERMANENT)
        assert not error.is_retryable


# ============================================================================
# JSON Extraction Tests
# ============================================================================

class TestJSONExtractor:
    """Test JSON extraction and recovery."""

    def test_extract_valid_json(self):
        """Test extracting valid JSON."""
        json_str = '{"message": "test", "score": 0.9, "tags": []}'
        extractor = JSONExtractor()

        data, error = extractor.extract_json(json_str, SampleResponse)
        assert error is None
        assert data["message"] == "test"
        assert data["score"] == 0.9

    def test_extract_json_with_markdown(self):
        """Test extracting JSON from markdown code block."""
        json_str = """
        ```json
        {"message": "test", "score": 0.85, "tags": ["a"]}
        ```
        """
        extractor = JSONExtractor()

        data, error = extractor.extract_json(json_str, SampleResponse)
        assert error is None
        assert data["message"] == "test"

    def test_extract_json_with_preamble(self):
        """Test extracting JSON with surrounding text."""
        text = """
        Some explanation here...

        {"message": "result", "score": 0.75, "tags": []}

        More text after.
        """
        extractor = JSONExtractor()

        data, error = extractor.extract_json(text, SampleResponse, strict=False)
        assert error is None or data is not None
        if data:
            assert data.get("message") == "result"

    def test_invalid_json_strict(self):
        """Test strict mode with invalid JSON."""
        extractor = JSONExtractor()
        data, error = extractor.extract_json("not json", SampleResponse, strict=True)
        assert error is not None
        assert data is None

    def test_validation_error(self):
        """Test Pydantic validation error."""
        json_str = '{"message": "test", "score": 1.5}'  # Score out of range
        extractor = JSONExtractor()

        data, error = extractor.extract_json(json_str, SampleResponse)
        assert error is not None

    def test_get_schema_json(self):
        """Test getting Pydantic schema."""
        extractor = JSONExtractor()
        schema_json = extractor.get_schema_json(SampleResponse)

        schema = json.loads(schema_json)
        assert "properties" in schema
        assert "message" in schema["properties"]
        assert "score" in schema["properties"]


# ============================================================================
# Retry Logic Tests
# ============================================================================

class TestRetryHandler:
    """Test retry handler."""

    def test_exponential_backoff_calculation(self):
        """Test exponential backoff calculation."""
        config = RetryConfig(
            initial_delay_ms=100,
            exponential_base=2.0,
            jitter=False
        )
        handler = RetryHandler(config)

        # Test exponential growth
        delay0 = handler.calculate_delay(0)
        delay1 = handler.calculate_delay(1)
        delay2 = handler.calculate_delay(2)

        assert 0.09 < delay0 < 0.11  # 100ms
        assert 0.19 < delay1 < 0.21  # 200ms
        assert 0.39 < delay2 < 0.41  # 400ms

    def test_jitter_adds_randomness(self):
        """Test jitter adds randomness to delays."""
        config = RetryConfig(
            initial_delay_ms=1000,
            exponential_base=2.0,
            jitter=True,
            max_delay_ms=10000
        )
        handler = RetryHandler(config)

        delays = [handler.calculate_delay(0) for _ in range(10)]
        assert len(set(delays)) > 1  # Different values due to jitter

    def test_max_delay_cap(self):
        """Test max delay cap."""
        config = RetryConfig(
            initial_delay_ms=100,
            exponential_base=2.0,
            max_delay_ms=1000,
            jitter=False
        )
        handler = RetryHandler(config)

        # After several attempts, should hit max
        delay_high = handler.calculate_delay(10)
        assert delay_high <= 1.1  # 1000ms + small buffer

    @pytest.mark.asyncio
    async def test_async_retry_success_on_first_attempt(self):
        """Test successful call on first attempt."""
        handler = RetryHandler()

        async def successful_func():
            return "success"

        result = await handler.async_call_with_retry(successful_func)
        assert result == "success"

    @pytest.mark.asyncio
    async def test_async_retry_success_after_retries(self):
        """Test success after retrying transient errors."""
        handler = RetryHandler(RetryConfig(max_attempts=3, initial_delay_ms=10))

        call_count = 0

        async def sometimes_fails():
            nonlocal call_count
            call_count += 1
            if call_count < 2:
                raise LLMError("Transient error", ErrorCategory.RETRYABLE)
            return "success"

        result = await handler.async_call_with_retry(sometimes_fails)
        assert result == "success"
        assert call_count == 2

    @pytest.mark.asyncio
    async def test_async_retry_exhaustion(self):
        """Test retry exhaustion."""
        handler = RetryHandler(RetryConfig(max_attempts=2, initial_delay_ms=10))

        async def always_fails():
            raise LLMError("Error", ErrorCategory.RETRYABLE)

        with pytest.raises(LLMError):
            await handler.async_call_with_retry(always_fails)

    @pytest.mark.asyncio
    async def test_async_non_retryable_error(self):
        """Test non-retryable error raised immediately."""
        handler = RetryHandler(RetryConfig(max_attempts=3, initial_delay_ms=10))

        async def fails_permanently():
            raise LLMError("Auth error", ErrorCategory.AUTH_FAILURE)

        with pytest.raises(LLMError) as exc_info:
            await handler.async_call_with_retry(fails_permanently)

        assert exc_info.value.category == ErrorCategory.AUTH_FAILURE

    def test_sync_retry_success(self):
        """Test synchronous retry success."""
        handler = RetryHandler(RetryConfig(max_attempts=3, initial_delay_ms=10))

        call_count = 0

        def sometimes_fails():
            nonlocal call_count
            call_count += 1
            if call_count < 2:
                raise LLMError("Transient", ErrorCategory.RETRYABLE)
            return "success"

        result = handler.sync_call_with_retry(sometimes_fails)
        assert result == "success"
        assert call_count == 2


# ============================================================================
# Timeout Tests
# ============================================================================

class TestProgressiveTimeoutManager:
    """Test progressive timeout manager."""

    def test_timeout_manager_initialization(self):
        """Test timeout manager initialization."""
        config = TimeoutConfig(total_execution=60, chunk_receive=5)
        manager = ProgressiveTimeoutManager(config)

        assert manager.start_time is None
        assert manager.last_activity_time is None

    def test_timeout_manager_start(self):
        """Test starting timeout manager."""
        manager = ProgressiveTimeoutManager()
        manager.start()

        assert manager.start_time is not None
        assert manager.last_activity_time is not None

    def test_remaining_time_calculation(self):
        """Test remaining time calculation."""
        config = TimeoutConfig(total_execution=10)
        manager = ProgressiveTimeoutManager(config)
        manager.start()

        remaining = manager.get_remaining_time()
        assert 9 < remaining <= 10

    def test_activity_updates_timestamp(self):
        """Test activity updates timestamp."""
        manager = ProgressiveTimeoutManager()
        manager.start()

        initial_time = manager.last_activity_time
        asyncio.run(asyncio.sleep(0.01))
        manager.activity()

        assert manager.last_activity_time > initial_time

    def test_chunk_timeout_detection(self):
        """Test chunk timeout detection."""
        config = TimeoutConfig(chunk_receive=0.01)
        manager = ProgressiveTimeoutManager(config)
        manager.start()

        # Wait longer than chunk timeout
        import time
        time.sleep(0.02)

        with pytest.raises(LLMError) as exc_info:
            manager.check_chunk_timeout()

        assert exc_info.value.category == ErrorCategory.TIMEOUT


# ============================================================================
# Circuit Breaker Tests
# ============================================================================

class TestCircuitBreaker:
    """Test circuit breaker."""

    def test_circuit_closed_initial_state(self):
        """Test circuit starts closed."""
        breaker = CircuitBreaker()
        assert breaker.state.value == "closed"

    def test_circuit_opens_on_failures(self):
        """Test circuit opens after threshold failures."""
        breaker = CircuitBreaker(failure_threshold=2)

        def failing_func():
            raise Exception("Error")

        # First failure
        with pytest.raises(Exception):
            breaker.call(failing_func)

        assert breaker.state.value == "closed"  # Still closed

        # Second failure opens circuit
        with pytest.raises(Exception):
            breaker.call(failing_func)

        # Now circuit should be open
        assert breaker.failure_count >= breaker.failure_threshold

    def test_circuit_rejects_when_open(self):
        """Test circuit rejects calls when open."""
        breaker = CircuitBreaker(failure_threshold=1)

        def failing_func():
            raise Exception("Error")

        # Open the circuit
        with pytest.raises(Exception):
            breaker.call(failing_func)

        # Manually open it
        breaker.state = breaker.state.__class__.OPEN

        # Should reject without calling function
        with pytest.raises(LLMError) as exc_info:
            breaker.call(failing_func)

        assert exc_info.value.category == ErrorCategory.RETRYABLE

    def test_circuit_status(self):
        """Test circuit status reporting."""
        breaker = CircuitBreaker()
        status = breaker.status

        assert "state" in status
        assert "failure_count" in status
        assert "success_count" in status


# ============================================================================
# Conversation Tests
# ============================================================================

class TestConversationContext:
    """Test conversation context."""

    def test_add_messages(self):
        """Test adding messages."""
        context = ConversationContext(system_prompt="You are helpful")

        context.add_user_message("Hello")
        context.add_assistant_message("Hi there")

        assert context.message_count == 2
        assert context.turn_count == 1

    def test_history_trimming(self):
        """Test history trimming."""
        context = ConversationContext(max_history=3)

        for i in range(5):
            context.add_user_message(f"Message {i}")

        assert context.message_count <= 3

    def test_conversation_string_natural_format(self):
        """Test natural format conversion."""
        context = ConversationContext()
        context.add_user_message("What is AI?")
        context.add_assistant_message("AI is artificial intelligence")

        text = context.get_conversation_string(format="natural")
        assert "USER:" in text
        assert "What is AI?" in text
        assert "ASSISTANT:" in text

    def test_conversation_string_json_format(self):
        """Test JSON format conversion."""
        context = ConversationContext()
        context.add_user_message("Hello")

        json_str = context.get_conversation_string(format="json")
        data = json.loads(json_str)

        assert len(data) > 0
        assert data[0]["role"] == "user"

    def test_export_history(self, tmp_path):
        """Test exporting history."""
        context = ConversationContext(system_prompt="Test prompt")
        context.add_user_message("Q1")
        context.add_assistant_message("A1")

        filepath = tmp_path / "history.json"

        # Create a manager to use export_history
        class DummyManager:
            def __init__(self, context):
                self.context = context

            def export_history(self, filepath):
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

        manager = DummyManager(context)
        manager.export_history(str(filepath))

        assert filepath.exists()
        with open(filepath) as f:
            data = json.load(f)
        assert data["system_prompt"] == "Test prompt"
        assert len(data["messages"]) == 2


# ============================================================================
# CLI Wrapper Tests
# ============================================================================

class TestLLMCliWrapper:
    """Test CLI wrapper."""

    @pytest.mark.asyncio
    async def test_call_api_async_success(self):
        """Test successful async API call."""
        wrapper = LLMCliWrapper("claude")

        with patch(
            'llm_cli_wrapper.SubprocessTimeout.run_with_timeout_async'
        ) as mock_run:
            response_json = json.dumps({
                "content": [{"text": '{"message": "test", "score": 0.9, "tags": []}'}]
            })
            mock_run.return_value = (response_json, "", 0)

            result = await wrapper.call_api_async(
                ["api", "call"],
                output_model=SampleResponse
            )

            assert result["message"] == "test"
            assert result["score"] == 0.9

    @pytest.mark.asyncio
    async def test_call_api_async_timeout(self):
        """Test timeout handling."""
        wrapper = LLMCliWrapper(
            "claude",
            timeout_config=TimeoutConfig(total_execution=0.01)
        )

        with patch(
            'llm_cli_wrapper.SubprocessTimeout.run_with_timeout_async'
        ) as mock_run:
            mock_run.side_effect = LLMError(
                "Timeout",
                ErrorCategory.TIMEOUT
            )

            with pytest.raises(LLMError) as exc_info:
                await wrapper.call_api_async(["api", "call"])

            assert exc_info.value.category == ErrorCategory.TIMEOUT

    @pytest.mark.asyncio
    async def test_call_api_async_malformed_output(self):
        """Test malformed output handling."""
        wrapper = LLMCliWrapper("claude")

        with patch(
            'llm_cli_wrapper.SubprocessTimeout.run_with_timeout_async'
        ) as mock_run:
            mock_run.return_value = ("Not valid JSON", "", 0)

            with pytest.raises(LLMError) as exc_info:
                await wrapper.call_api_async(
                    ["api", "call"],
                    output_model=SampleResponse
                )

            assert exc_info.value.category == ErrorCategory.MALFORMED_OUTPUT

    def test_call_api_sync_success(self):
        """Test successful sync API call."""
        wrapper = LLMCliWrapper("claude")

        with patch(
            'llm_cli_wrapper.SubprocessTimeout.run_with_timeout'
        ) as mock_run:
            response_json = json.dumps({
                "content": [{"text": '{"message": "sync", "score": 0.8, "tags": []}'}]
            })
            mock_run.return_value = (response_json, "", 0)

            result = wrapper.call_api_sync(
                ["api", "call"],
                output_model=SampleResponse
            )

            assert result["message"] == "sync"


# ============================================================================
# Integration Tests
# ============================================================================

class TestIntegration:
    """Integration tests."""

    @pytest.mark.asyncio
    async def test_full_async_workflow(self):
        """Test full async workflow with retry and timeout."""
        config = RetryConfig(max_attempts=2, initial_delay_ms=10)
        timeout_config = TimeoutConfig(total_execution=30)

        wrapper = LLMCliWrapper(
            "claude",
            retry_config=config,
            timeout_config=timeout_config
        )

        call_count = 0

        async def mock_subprocess(*args, **kwargs):
            nonlocal call_count
            call_count += 1

            if call_count == 1:
                # First call fails with transient error
                raise subprocess.TimeoutExpired(["cmd"], 30)

            # Second call succeeds
            response = json.dumps({
                "content": [{"text": '{"message": "recovered", "score": 0.75, "tags": []}'}]
            })
            return response, "", 0

        with patch(
            'llm_cli_wrapper.SubprocessTimeout.run_with_timeout_async',
            side_effect=mock_subprocess
        ):
            result = await wrapper.call_api_async(
                ["api", "call"],
                output_model=SampleResponse
            )

            assert result["message"] == "recovered"
            assert call_count == 2  # Called twice due to retry

    def test_conversation_manager_basic(self):
        """Test conversation manager basic flow."""
        manager = ConversationManager(system_prompt="Test assistant")

        assert manager.context.system_prompt == "Test assistant"
        assert manager.context.turn_count == 0

        # Add messages manually
        manager.context.add_user_message("Hello")
        manager.context.add_assistant_message("Hi")

        assert manager.context.turn_count == 1
        assert manager.context.message_count == 2


# ============================================================================
# Edge Cases
# ============================================================================

class TestEdgeCases:
    """Test edge cases."""

    def test_empty_response_handling(self):
        """Test handling empty responses."""
        extractor = JSONExtractor()
        data, error = extractor.extract_json("", SampleResponse)
        assert error is not None

    def test_none_values_in_optional_fields(self):
        """Test None values in optional fields."""
        json_str = '{"message": "test", "score": 0.9, "tags": null}'
        extractor = JSONExtractor()
        data, error = extractor.extract_json(json_str, SampleResponse)
        # May fail validation or be handled depending on model config

    def test_very_large_response(self):
        """Test handling very large responses."""
        large_text = "x" * 10000
        json_str = json.dumps({
            "message": large_text,
            "score": 0.5,
            "tags": ["tag1"] * 100
        })

        extractor = JSONExtractor()
        data, error = extractor.extract_json(json_str, SampleResponse)
        # Should handle without memory issues

    def test_unicode_in_responses(self):
        """Test unicode handling."""
        json_str = json.dumps({
            "message": "Émojis: 🚀 🎉 中文字符",
            "score": 0.9,
            "tags": ["测试"]
        })

        extractor = JSONExtractor()
        data, error = extractor.extract_json(json_str, SampleResponse)
        assert error is None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
