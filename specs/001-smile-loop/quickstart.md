# SMILE Loop Developer Quickstart

**Feature**: 001-smile-loop | **Date**: 2026-02-02

Get up and running with SMILE Loop development.

---

## Prerequisites

- **Rust**: 1.75+ (`rustup update stable`)
- **Python**: 3.11+ (`python3 --version`)
- **Docker**: Desktop or Engine (`docker --version`)
- **just**: Task runner (`cargo install just`)
- **LLM CLI**: At least one of:
  - `claude` (Anthropic CLI)
  - `codex` (OpenAI CLI)
  - `gemini` (Google CLI)

## Quick Setup

```bash
# Clone and enter project
git clone <repo-url> smile && cd smile

# Install Rust dependencies
cargo build

# Install Python dependencies
cd python && pip install -e ".[dev]" && cd ..

# Build the base Docker image
docker build -t smile-base:latest -f docker/Dockerfile.base .

# Verify everything works
just check
```

## Project Layout

```
smile/
├── crates/                  # Rust workspace
│   ├── smile-cli/          # Entry point: `cargo run -p smile-cli`
│   ├── smile-orchestrator/ # Loop logic, HTTP/WS server
│   ├── smile-container/    # Docker management via bollard
│   └── smile-report/       # Report generation
├── python/
│   └── smile_wrappers/     # Student/Mentor agents
├── docker/
│   └── Dockerfile.base     # Container with LLM CLIs
└── tests/integration/      # End-to-end tests
```

## Common Tasks (Justfile)

```bash
# Build everything
just build

# Run all tests
just test

# Run only Rust tests
just test-rust

# Run only Python tests
just test-python

# Format code
just fmt

# Lint code
just lint

# Run SMILE against a tutorial (development)
just run path/to/tutorial.md

# Start orchestrator only (for debugging)
just orchestrator

# Build Docker image
just docker-build

# Clean build artifacts
just clean
```

## Development Workflow

### 1. Working on Rust Components

```bash
# Watch mode (recompile on changes)
cargo watch -x check -x 'test -p smile-orchestrator'

# Run with debug logging
RUST_LOG=debug cargo run -p smile-cli -- path/to/tutorial.md

# Check specific crate
cargo clippy -p smile-container
```

### 2. Working on Python Wrappers

```bash
cd python

# Run tests with coverage
pytest --cov=smile_wrappers

# Type checking
mypy smile_wrappers

# Format
black smile_wrappers tests

# Lint
ruff check smile_wrappers
```

### 3. Testing the Full Loop

```bash
# Use the sample tutorial with known gaps
just test-integration

# Or manually:
cargo run -p smile-cli -- tests/integration/fixtures/sample-tutorial/tutorial.md

# Watch the WebSocket events (in another terminal)
websocat ws://localhost:3000/ws
```

### 4. Debugging Container Issues

```bash
# Keep container running after failure
SMILE_KEEP_CONTAINER=1 cargo run -p smile-cli -- tutorial.md

# Shell into the container
docker exec -it smile-student-<id> /bin/bash

# Check container logs
docker logs smile-student-<id>
```

## Configuration

Create `smile.json` in your working directory (all fields optional):

```json
{
  "tutorial": "tutorial.md",
  "llmProvider": "claude",
  "maxIterations": 10,
  "timeout": 1800,
  "studentBehavior": {
    "maxRetriesBeforeHelp": 3,
    "patienceLevel": "low"
  }
}
```

See `data-model.md` for full schema.

## API Endpoints (for debugging)

When the orchestrator is running:

```bash
# Get current status
curl http://localhost:3000/api/status | jq

# Force stop the loop
curl -X POST http://localhost:3000/api/stop \
  -H "Content-Type: application/json" \
  -d '{"reason": "manual stop"}'

# Watch WebSocket events
websocat ws://localhost:3000/ws
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |
| `SMILE_PORT` | `3000` | Orchestrator HTTP port |
| `SMILE_KEEP_CONTAINER` | `0` | Keep container after loop (1=yes) |
| `SMILE_STATE_DIR` | `.smile` | State file directory |

## Troubleshooting

### "Docker is not available"

```bash
# Check Docker daemon is running
docker info

# On Linux, ensure user is in docker group
sudo usermod -aG docker $USER
# Then logout/login
```

### "LLM CLI not found"

The container needs the LLM CLI installed. Check `docker/Dockerfile.base`:
- Claude: `npm install -g @anthropic-ai/cli`
- Codex: `pip install openai-cli`
- Gemini: (installation varies)

### "Connection refused to host.docker.internal"

On Linux, ensure the container has `--add-host=host.docker.internal:host-gateway`.
This is handled automatically by smile-container.

### "Tutorial exceeds size limit"

Tutorials must be under 100KB. Split large tutorials or reduce image count.

### Tests Failing on CI

```bash
# Run the same checks CI runs
just ci

# Check formatting
just fmt-check

# Check lints
just lint
```

## Next Steps

1. Read the spec: `specs/001-smile-loop/spec.md`
2. Review the data model: `specs/001-smile-loop/data-model.md`
3. Check API contracts: `specs/001-smile-loop/contracts/`
4. Run the sample tutorial: `just run tests/fixtures/sample.md`
