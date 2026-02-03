# Python LLM CLI Wrappers: Complete Reference Implementation

This directory contains a comprehensive, production-ready implementation of best practices for building Python wrappers around LLM command-line interfaces (Claude, Gemini, Codex, etc.).

## Quick Overview

**What You Get:**
- Complete wrapper implementation with subprocess management
- Structured JSON output handling with Pydantic validation
- Automatic retry logic with exponential backoff
- Timeout management for long-running calls
- Multi-turn conversation support
- Circuit breaker for fault tolerance
- Full async/await support
- Comprehensive test suite
- Real-world usage examples

**Key Files:**
- `LLM_CLI_WRAPPER_GUIDE.md` - Comprehensive best practices guide
- `llm_cli_wrapper.py` - Production-ready implementation (~700 lines)
- `test_llm_cli_wrapper.py` - Full test suite with 35+ test cases
- `examples_llm_cli_usage.py` - Real-world usage patterns

## Architecture

```
Application Layer (FastAPI, Script, etc.)
    ↓
LLMCliWrapper (Process lifecycle, stream handling)
    ↓
RetryHandler (Exponential backoff with jitter)
    ↓
SubprocessTimeout (Proper timeout handling)
    ↓
CircuitBreaker (Fault protection)
    ↓
External CLI Process (claude, gemini, codex binary)
```

## Core Components

### 1. Error Handling & Classification

**Problem:** Different errors need different handling strategies.

**Solution:** `ErrorCategory` enum with retryable vs. permanent classification.

```python
from llm_cli_wrapper import LLMError, ErrorCategory

try:
    result = await wrapper.call_api_async(prompt)
except LLMError as e:
    if e.is_retryable:
        # Safe to retry
        pass
    else:
        # Permanent failure, bail out
        raise
```

**Key Error Types:**
- `RETRYABLE` - Network errors, transient failures
- `TIMEOUT` - Process exceeded time limit
- `RATE_LIMIT` - API rate limit hit
- `MALFORMED_OUTPUT` - Invalid JSON/response format
- `AUTH_FAILURE` - Authentication issues
- `PERMANENT` - Won't succeed with retry

### 2. Structured JSON Output

**Problem:** LLMs don't always output valid JSON.

**Solution:** `JSONExtractor` with recovery and Pydantic validation.

```python
from pydantic import BaseModel
from llm_cli_wrapper import JSONExtractor

class AnalysisResult(BaseModel):
    summary: str
    issues: list[str]
    confidence: float

# Extract JSON with recovery
extractor = JSONExtractor()
data, error = extractor.extract_json(
    llm_response,
    AnalysisResult
)

if error:
    print(f"Failed: {error}")
else:
    result = AnalysisResult(**data)
```

**Features:**
- Removes markdown code blocks automatically
- Attempts partial JSON recovery
- Full Pydantic model validation
- Detailed error messages

### 3. Retry Logic

**Problem:** Transient failures need smart retry with backoff.

**Solution:** `RetryHandler` with exponential backoff and jitter.

```python
from llm_cli_wrapper import RetryHandler, RetryConfig

config = RetryConfig(
    max_attempts=3,
    initial_delay_ms=100,
    exponential_base=2.0,
    jitter=True
)

handler = RetryHandler(config)

# Async retry
result = await handler.async_call_with_retry(
    some_llm_function,
    prompt="Analyze this code"
)
```

**Backoff Strategy:**
- Attempt 1: 100ms ± jitter
- Attempt 2: 200ms ± jitter
- Attempt 3: 400ms ± jitter
- Capped at max_delay_ms (default: 30s)

### 4. Timeout Management

**Problem:** LLM calls can hang indefinitely.

**Solution:** Progressive timeout with activity tracking.

```python
from llm_cli_wrapper import TimeoutConfig, ProgressiveTimeoutManager

config = TimeoutConfig(
    total_execution=120.0,      # Total time limit
    chunk_receive=5.0,           # Time between chunks
    graceful_shutdown=5.0        # Cleanup time
)

manager = ProgressiveTimeoutManager(config)
manager.start()

async for chunk in stream_response():
    manager.activity()  # Mark activity
    remaining = manager.check_chunk_timeout()
    if remaining <= 0:
        raise LLMError("Exceeded total timeout")
```

### 5. Multi-Turn Conversations

**Problem:** Managing conversation state across turns is error-prone.

**Solution:** `ConversationManager` with automatic history management.

```python
from llm_cli_wrapper import ConversationManager

manager = ConversationManager(
    system_prompt="You are a code reviewer",
    max_turns=10
)

# Turn 1
response1 = await manager.send_message(
    "Review this code",
    call_func=my_llm_call
)

# Turn 2 - history is maintained automatically
response2 = await manager.send_message(
    "Explain the first issue more",
    call_func=my_llm_call
)

# Export history
manager.export_history("conversation.json")
```

**Features:**
- Automatic context trimming to prevent overflow
- Message history tracking with timestamps
- Easy export to JSON
- Turn counting

### 6. Circuit Breaker

**Problem:** Cascading failures when LLM service is down.

**Solution:** `CircuitBreaker` to reject requests when service fails.

```python
from llm_cli_wrapper import CircuitBreaker

breaker = CircuitBreaker(
    failure_threshold=5,
    success_threshold=2,
    timeout_seconds=60.0
)

try:
    result = breaker.call(llm_function, prompt)
except LLMError:
    if breaker.state.value == "open":
        # Service is degraded, return cached/fallback
        return get_cached_result()
    raise
```

**States:**
- `CLOSED` - Normal operation
- `OPEN` - Rejecting all requests (service down)
- `HALF_OPEN` - Testing if service recovered

## Usage Examples

### Example 1: Simple Code Analysis

```python
from llm_cli_wrapper import LLMCliWrapper
from pydantic import BaseModel

class CodeAnalysis(BaseModel):
    issues: list[str]
    score: float

async def analyze_code(code: str):
    wrapper = LLMCliWrapper("claude")

    prompt = f"Analyze this code:\n{code}"

    result = await wrapper.call_api_async(
        cmd_args=["api", "call"],
        input_data=json.dumps({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.0
        }),
        output_model=CodeAnalysis
    )

    return CodeAnalysis(**result)

# Usage
analysis = asyncio.run(analyze_code("def foo(): pass"))
print(f"Score: {analysis.score}")
```

### Example 2: Multi-Turn Conversation

```python
from llm_cli_wrapper import ConversationManager

manager = ConversationManager(
    system_prompt="You are a helpful assistant"
)

async def call_claude(prompt: str) -> str:
    wrapper = LLMCliWrapper("claude")
    return await wrapper.call_api_async(
        cmd_args=["api", "call"],
        input_data=json.dumps({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": prompt}]
        })
    )

# Multi-turn conversation
response1 = asyncio.run(
    manager.send_message("What is Python?", call_claude)
)
print(response1)

response2 = asyncio.run(
    manager.send_message("Tell me more", call_claude)
)
print(response2)
```

### Example 3: Batch Processing with Concurrency Control

```python
from llm_cli_wrapper import LLMCliWrapper

async def batch_analyze(files: dict[str, str]):
    wrapper = LLMCliWrapper("claude")
    semaphore = asyncio.Semaphore(3)  # Max 3 concurrent

    async def analyze_one(filename, code):
        async with semaphore:
            try:
                result = await wrapper.call_api_async(
                    cmd_args=["api", "call"],
                    input_data=json.dumps({
                        "model": "claude-3-5-sonnet",
                        "messages": [{
                            "role": "user",
                            "content": f"Analyze {filename}:\n{code}"
                        }]
                    })
                )
                return {"file": filename, "result": result}
            except LLMError as e:
                return {"file": filename, "error": str(e)}

    tasks = [analyze_one(f, c) for f, c in files.items()]
    return await asyncio.gather(*tasks)
```

## Testing

Run the comprehensive test suite:

```bash
# Install dependencies
pip install pytest pytest-asyncio pydantic

# Run all tests
pytest test_llm_cli_wrapper.py -v

# Run specific test class
pytest test_llm_cli_wrapper.py::TestRetryHandler -v

# Run with coverage
pytest test_llm_cli_wrapper.py --cov=llm_cli_wrapper
```

**Test Coverage:**
- Error categorization (5 tests)
- JSON extraction with recovery (6 tests)
- Retry logic with backoff (8 tests)
- Timeout management (5 tests)
- Circuit breaker (4 tests)
- Conversation management (4 tests)
- CLI wrapper integration (4 tests)
- Edge cases (5 tests)

## Best Practices

### 1. Always Specify Output Schema

```python
# Bad: No schema guidance
response = await wrapper.call_api_async(prompt)

# Good: Explicit schema in prompt
schema = MyModel.model_json_schema()
prompt = f"Respond with JSON matching schema:\n{json.dumps(schema)}\n\n..."
```

### 2. Use Deterministic Temperatures for Structured Output

```python
# Bad: High temperature for JSON
temp = 0.7  # Non-deterministic

# Good: Low temperature for structured
temp = 0.0  # Deterministic
```

### 3. Implement Proper Cleanup

```python
# Bad: Process might leak
try:
    result = await call_llm()
except:
    pass

# Good: Proper cleanup with context managers
try:
    result = await call_llm()
finally:
    if process and process.returncode is None:
        process.kill()
```

### 4. Distinguish Retryable vs. Permanent Errors

```python
# Bad: Retry everything
for i in range(max_retries):
    try:
        return call_llm()
    except:
        time.sleep(backoff)

# Good: Only retry appropriate errors
try:
    return await handler.async_call_with_retry(call_llm)
except LLMError as e:
    if e.is_retryable:
        raise  # Will be retried by handler
    else:
        raise  # Permanent, don't retry
```

### 5. Log Everything

```python
logger.info(f"Calling LLM with prompt: {prompt[:100]}...")
logger.error(f"LLM failed: {e.category.value}", extra={
    "error_message": e.message,
    "stderr": e.stderr,
    "original": e.original_error
})
```

## Performance Considerations

### Async for I/O

```python
# 10 sequential calls: 300s
for item in items:
    result = await call_llm(item)

# 10 concurrent calls: 30s
tasks = [call_llm(item) for item in items]
results = await asyncio.gather(*tasks)
```

### Streaming for Large Responses

```python
# Buffer entire response in memory
response = await call_llm(prompt)

# Stream chunks as they arrive
async for chunk in wrapper.stream_response(prompt):
    process_chunk(chunk)
```

### Conversation History Trimming

```python
# Wrong: Unbounded history
manager = ConversationManager()  # max_history defaults to 20

# Right: Bounded conversation
manager = ConversationManager(max_history=10)
```

## Integration with FastAPI

```python
from fastapi import FastAPI
from fastapi.responses import StreamingResponse

app = FastAPI()
wrapper = LLMCliWrapper("claude")

@app.post("/analyze")
async def analyze(code: str):
    try:
        result = await wrapper.call_api_async(
            cmd_args=["api", "call"],
            input_data=json.dumps({
                "model": "claude-3-5-sonnet",
                "messages": [{"role": "user", "content": f"Analyze:\n{code}"}]
            }),
            output_model=CodeAnalysis
        )
        return result
    except LLMError as e:
        raise HTTPException(status_code=503, detail=str(e))

@app.get("/stream")
async def stream_analysis(query: str):
    async def generate():
        async for chunk in wrapper.stream_response(query):
            yield chunk

    return StreamingResponse(generate(), media_type="text/plain")
```

## Troubleshooting

### Process Hangs

**Symptom:** Application freezes on LLM call

**Solution:** Reduce timeout, check if process is actually running

```python
config = TimeoutConfig(total_execution=30.0)  # More aggressive
wrapper = LLMCliWrapper("claude", timeout_config=config)
```

### Malformed JSON Responses

**Symptom:** "Invalid JSON" errors

**Solution:** Enable JSON recovery, add schema to prompt

```python
data, error = extractor.extract_json(
    response,
    model_class,
    strict=False  # Enable recovery
)
```

### Rate Limiting

**Symptom:** Increasing failures with "rate limit" errors

**Solution:** Add jitter to spread retries, increase retry delays

```python
config = RetryConfig(
    max_attempts=5,
    initial_delay_ms=1000,  # Start higher
    exponential_base=2.0,
    jitter=True  # Spread out retry times
)
```

### Memory Leaks with Streaming

**Symptom:** Memory grows over time

**Solution:** Ensure processes are cleaned up

```python
try:
    async for chunk in stream_response():
        process(chunk)
finally:
    if process:
        process.kill()
```

## Files Reference

| File | Purpose | Lines |
|------|---------|-------|
| `LLM_CLI_WRAPPER_GUIDE.md` | Comprehensive best practices guide | 800+ |
| `llm_cli_wrapper.py` | Production implementation | ~700 |
| `test_llm_cli_wrapper.py` | Test suite | ~600 |
| `examples_llm_cli_usage.py` | Real-world examples | ~400 |
| `README_LLM_CLI_WRAPPERS.md` | This file | - |

## Dependencies

```
pydantic>=2.0          # Data validation
typing-extensions      # Type hints (Python 3.8+)
pytest>=7.0           # Testing
pytest-asyncio        # Async testing
```

## Key Insights

1. **Subprocess Management**: Use `Popen` for fine-grained control over timeouts and cleanup
2. **Error Classification**: Categorize errors to make smart retry decisions
3. **JSON Validation**: Always validate LLM output with Pydantic
4. **Timeout Strategy**: Use progressive timeouts for different lifecycle phases
5. **Concurrency**: Use semaphores to control concurrent subprocess spawning
6. **State Management**: Maintain conversation context explicitly
7. **Observability**: Log all subprocess calls and errors
8. **Testing**: Mock subprocess to test without external dependencies

## Additional Resources

- [Python Subprocess Documentation](https://docs.python.org/3/library/subprocess.html)
- [Pydantic Documentation](https://docs.pydantic.dev/)
- [Anthropic Claude Documentation](https://docs.anthropic.com/)
- [AsyncIO Best Practices](https://docs.python.org/3/library/asyncio.html)

## License

This implementation is provided as reference material for building production-ready LLM CLI wrappers in Python.

## Contributing

To improve this reference implementation:

1. Add more comprehensive examples
2. Extend error categorization for edge cases
3. Add metrics/telemetry for monitoring
4. Create CLI-specific wrappers (Claude, Gemini, etc.)
5. Add support for streaming request bodies

---

**Last Updated:** 2026-02-02
**Status:** Production-ready reference implementation
