# Integration Test Fixtures

This directory contains fixtures for SMILE Loop integration tests.

## Directory Structure

```
fixtures/
├── docker-setup-tutorial/      # Tutorial with Docker setup steps
├── python-fastapi-tutorial/    # Tutorial for FastAPI application
└── README.md                   # This file
```

## Fixture Components

Each tutorial fixture directory should contain:

| File | Description | Required |
|------|-------------|----------|
| `tutorial.md` | The tutorial markdown to validate | Yes |
| `smile.json` | Configuration for the validation run | Yes |
| `EXPECTED_GAPS.md` | Documentation of expected gaps (for test validation) | Optional |
| `scenarios/` | Mock LLM response scenarios for testing | Optional |

### tutorial.md

The tutorial content that SMILE will validate. Should be a realistic markdown tutorial with intentional gaps for testing gap detection.

### smile.json

Configuration file for the validation run:

```json
{
  "tutorial": "tutorial.md",
  "maxIterations": 5,
  "llmProvider": "claude",
  "studentBehavior": {
    "maxRetriesBeforeHelp": 3,
    "patienceLevel": "low"
  }
}
```

### EXPECTED_GAPS.md

Documents the expected gaps for regression testing:

```markdown
## Expected Gaps

### Gap 1: Missing prerequisite
- **Severity**: Critical (CannotComplete)
- **Location**: Step 2, line 15
- **Problem**: Docker not mentioned as prerequisite
```

### scenarios/

Mock LLM CLI response scenarios for testing without real API calls:

```json
{
  "description": "Student hits missing npm",
  "responses": [
    {
      "status": "ask_mentor",
      "currentStep": "Step 2",
      "attemptedActions": ["npm init"],
      "problem": "npm: command not found",
      "questionForMentor": "How do I install npm?"
    }
  ]
}
```

## Adding New Fixtures

1. Create a new directory under `fixtures/`
2. Add `tutorial.md` with your test tutorial
3. Add `smile.json` with appropriate configuration
4. Optionally add `EXPECTED_GAPS.md` to document expected behavior
5. Add mock scenarios if needed for deterministic testing

## Gap Severity Mapping

When analyzing test results, understand how SMILE maps student outcomes to gap severities:

| StudentStatus | GapSeverity | Meaning |
|---------------|-------------|---------|
| `ask_mentor` | Major | Student needed help but could continue |
| `cannot_complete` | Critical | Student was blocked, tutorial has fundamental issue |
| `completed` | N/A | No gap - iteration successful |

## Mock LLM CLI

The Python tests use a mock CLI script that reads scenarios from JSON files. The mock CLI:

1. Reads `MOCK_SCENARIO_FILE` environment variable
2. Returns responses from the scenario in sequence
3. Tracks state in a `.state` file alongside the scenario (e.g., `scenario.json` → `scenario.state`)

**State File Cleanup**: Each test creates its own temp directory via pytest fixtures. The `.state` files are created inside this temp directory and are automatically cleaned up when the test completes via `shutil.rmtree()`. This prevents state leakage between tests and ensures parallel test execution works correctly.

See `python/tests/test_integration.py` for examples of mock CLI usage.

## Running Integration Tests

```bash
# Run Rust integration tests (requires Docker)
cargo test --test integration -- --ignored

# Run Python integration tests (uses mock CLI)
cd python && python -m pytest tests/test_integration.py -v
```

## Notes

- Integration tests with real containers are marked `#[ignore]` and require Docker
- Mock CLI tests provide fast, deterministic validation without external dependencies
- Fixture directories can be empty during development (placeholders)
