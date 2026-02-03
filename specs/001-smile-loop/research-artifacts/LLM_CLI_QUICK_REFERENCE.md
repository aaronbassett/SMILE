# LLM CLI Wrapper - Quick Reference Card

## Import Everything You Need

```python
from llm_cli_wrapper import (
    LLMCliWrapper,           # Main wrapper
    LLMError,               # Exception
    ErrorCategory,          # Error types
    RetryConfig,            # Retry settings
    TimeoutConfig,          # Timeout settings
    JSONExtractor,          # JSON parsing
    RetryHandler,           # Retry logic
    ConversationManager,    # Multi-turn conversations
    CircuitBreaker,         # Fault protection
)
```

## 5-Minute Setup

```python
from llm_cli_wrapper import LLMCliWrapper
from pydantic import BaseModel
import json
import asyncio

# Define your output model
class MyOutput(BaseModel):
    result: str
    confidence: float

# Create wrapper
wrapper = LLMCliWrapper("claude")

# Call LLM
async def main():
    result = await wrapper.call_api_async(
        cmd_args=["api", "call"],
        input_data=json.dumps({
            "model": "claude-3-5-sonnet",
            "messages": [
                {"role": "user", "content": "Say hello"}
            ]
        })
    )
    return result

response = asyncio.run(main())
```

## Common Patterns

### Pattern 1: Simple Sync Call with Retry

```python
from llm_cli_wrapper import LLMCliWrapper, RetryConfig

config = RetryConfig(max_attempts=3)
wrapper = LLMCliWrapper("claude", retry_config=config)

result = wrapper.call_api_sync(
    cmd_args=["api", "call"],
    input_data=my_json_input
)
```

### Pattern 2: Async with Timeout

```python
from llm_cli_wrapper import TimeoutConfig

config = TimeoutConfig(total_execution=60.0)
wrapper = LLMCliWrapper("claude", timeout_config=config)

result = await wrapper.call_api_async(
    cmd_args=["api", "call"],
    input_data=my_json_input
)
```

### Pattern 3: Structured JSON Output

```python
from pydantic import BaseModel
from llm_cli_wrapper import JSONExtractor

class Result(BaseModel):
    answer: str
    confidence: float

extractor = JSONExtractor()
data, error = extractor.extract_json(
    llm_response_text,
    Result,
    strict=False  # Enable recovery
)

if error:
    print(f"Failed: {error}")
else:
    result = Result(**data)
```

### Pattern 4: Multi-Turn Conversation

```python
from llm_cli_wrapper import ConversationManager

manager = ConversationManager(
    system_prompt="You are helpful"
)

async def call_llm(prompt):
    # Your LLM call here
    pass

response = await manager.send_message("Hi", call_llm)
response2 = await manager.send_message("Tell me more", call_llm)
```

### Pattern 5: Batch Processing

```python
async def batch_process(items):
    semaphore = asyncio.Semaphore(3)  # Max 3 concurrent

    async def process_one(item):
        async with semaphore:
            return await wrapper.call_api_async(cmd, item)

    results = await asyncio.gather(
        *[process_one(item) for item in items]
    )
    return results
```

## Error Handling

### Check if Error is Retryable

```python
from llm_cli_wrapper import LLMError, ErrorCategory

try:
    result = await wrapper.call_api_async(cmd)
except LLMError as e:
    print(f"Category: {e.category.value}")

    if e.is_retryable:
        # Safe to retry
        pass
    else:
        # Permanent failure
        raise
```

### Error Categories

| Category | Retryable | Cause |
|----------|-----------|-------|
| `TIMEOUT` | Yes | Process exceeded time limit |
| `RATE_LIMIT` | Yes | API rate limit hit |
| `RETRYABLE` | Yes | Network/transient error |
| `AUTH_FAILURE` | No | Authentication failed |
| `MALFORMED_OUTPUT` | No | Invalid JSON response |
| `PERMANENT` | No | Unknown permanent error |

## Configuration

### Retry Config

```python
from llm_cli_wrapper import RetryConfig

config = RetryConfig(
    max_attempts=3,           # Total attempts
    initial_delay_ms=100,     # First delay in ms
    max_delay_ms=30000,       # Max delay cap
    exponential_base=2.0,     # 2x backoff per retry
    jitter=True               # Add randomness
)

wrapper = LLMCliWrapper("claude", retry_config=config)
```

**Backoff calculation:**
- Attempt 1: 100ms ± jitter
- Attempt 2: 200ms ± jitter
- Attempt 3: 400ms ± jitter (capped at 30s)

### Timeout Config

```python
from llm_cli_wrapper import TimeoutConfig

config = TimeoutConfig(
    initial_startup=5.0,      # Time to start
    first_response=10.0,      # Time to first output
    chunk_receive=5.0,        # Time between chunks
    total_execution=60.0,     # Total time limit
    graceful_shutdown=5.0     # Cleanup time
)

wrapper = LLMCliWrapper("claude", timeout_config=config)
```

## Testing

### Mock Subprocess in Tests

```python
from unittest.mock import patch
import json

@pytest.mark.asyncio
async def test_my_function():
    mock_response = json.dumps({
        "content": [{"text": '{"result": "success"}'}]
    })

    with patch('llm_cli_wrapper.SubprocessTimeout.run_with_timeout_async') as mock:
        mock.return_value = (mock_response, "", 0)
        result = await wrapper.call_api_async(cmd)
        assert result["result"] == "success"
```

## Debugging Tips

### Enable Logging

```python
import logging

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

# Now all retry/timeout info is logged
```

### Check Circuit Breaker Status

```python
status = wrapper.circuit_breaker.status
print(f"State: {status['state']}")
print(f"Failures: {status['failure_count']}")
```

### Validate JSON Extraction

```python
extractor = JSONExtractor()

# Get schema for prompt engineering
schema = extractor.get_schema_json(MyModel)
print(schema)  # Use in your prompt
```

## Common Mistakes

### ❌ Not specifying schema
```python
# Bad: LLM doesn't know what structure you want
response = await wrapper.call_api_async(prompt)
```

### ✅ Include schema in prompt
```python
# Good: Explicit schema guidance
schema = MyModel.model_json_schema()
prompt = f"Respond with JSON:\n{json.dumps(schema)}\n..."
```

### ❌ Using high temperature for structured output
```python
# Bad: Non-deterministic JSON
temp = 0.7
```

### ✅ Use low/zero temperature
```python
# Good: Deterministic structured output
temp = 0.0
```

### ❌ Ignoring timeouts
```python
# Bad: Can hang forever
subprocess.run(cmd)
```

### ✅ Set explicit timeouts
```python
# Good: Always has a timeout
config = TimeoutConfig(total_execution=30)
```

## Performance Tips

### 1. Use Async for I/O

```python
# Slow: Sequential
for item in items:
    await call_llm(item)  # N * latency

# Fast: Concurrent
await asyncio.gather(*[call_llm(item) for item in items])  # Max of latencies
```

### 2. Limit Concurrency

```python
# Avoid: Too many processes
await asyncio.gather(*[call_llm(i) for i in range(1000)])

# Better: Controlled concurrency
semaphore = asyncio.Semaphore(10)
```

### 3. Stream Large Responses

```python
# Don't buffer everything:
# response = await call_llm()  # 10GB in memory

# Stream instead:
async for chunk in wrapper.stream_response(prompt):
    process(chunk)
```

### 4. Bound Conversation History

```python
# Bad: Unbounded growth
manager = ConversationManager()

# Good: Limited history
manager = ConversationManager(max_history=10)
```

## Real-World Example

```python
import asyncio
import json
from pydantic import BaseModel
from llm_cli_wrapper import LLMCliWrapper, RetryConfig

class CodeIssue(BaseModel):
    line: int
    severity: str
    message: str

async def analyze_code(code: str) -> list[CodeIssue]:
    config = RetryConfig(max_attempts=3, initial_delay_ms=100)
    wrapper = LLMCliWrapper("claude", retry_config=config)

    prompt = f"""Find issues in this code:

```python
{code}
```

Respond with JSON array of issues."""

    try:
        response = await wrapper.call_api_async(
            cmd_args=["api", "call"],
            input_data=json.dumps({
                "model": "claude-3-5-sonnet",
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.0
            })
        )

        issues = json.loads(response)
        return [CodeIssue(**issue) for issue in issues]

    except Exception as e:
        print(f"Analysis failed: {e}")
        return []

# Usage
code = "def foo():\n    x = 1 / 0"
issues = asyncio.run(analyze_code(code))
for issue in issues:
    print(f"Line {issue.line}: {issue.message}")
```

## Key Takeaways

1. **Always use retry for transient failures**
2. **Always set timeouts** (subprocess can hang)
3. **Validate all LLM output** with Pydantic
4. **Use structured JSON** in prompts and responses
5. **Stream large responses** to save memory
6. **Use async for concurrency** and better performance
7. **Log everything** for debugging
8. **Test with mocks** to avoid external dependencies

## Resources

- Full Guide: `LLM_CLI_WRAPPER_GUIDE.md`
- Implementation: `llm_cli_wrapper.py`
- Tests: `test_llm_cli_wrapper.py`
- Examples: `examples_llm_cli_usage.py`
- Readme: `README_LLM_CLI_WRAPPERS.md`

---

**Bookmark this card for quick reference!**
