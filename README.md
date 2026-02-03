# SMILE Loop

**Simulated Mentored Interactive Learning Experience**

SMILE Loop validates technical tutorials by simulating a constrained learner (Student agent) that attempts to follow instructions, escalating to a Mentor agent when stuck. It discovers gaps, unclear instructions, and missing prerequisites before real users do.

> **Status: Alpha (v0.1.0)** - SMILE Loop is an early-stage project actively seeking community feedback. [Report issues](https://github.com/aaronbassett/SMILE/issues/new?template=bug_report.yml), [share feedback](https://github.com/aaronbassett/SMILE/issues/new?template=alpha_feedback.yml), or [request features](https://github.com/aaronbassett/SMILE/issues/new?template=feature_request.yml).

## The Problem

Tutorial authors discover problems only after users complain, submit support tickets, or abandon tutorials entirely. SMILE Loop solves this by simulating a learner with intentionally constrained capabilities, automatically discovering what's missing before publication.

## How It Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                         SMILE Loop                                  │
│                                                                     │
│  ┌──────────┐    stuck?    ┌──────────┐    notes     ┌──────────┐ │
│  │ Tutorial │ ──────────►  │  Mentor  │ ──────────►  │ Student  │ │
│  │  (input) │              │  Agent   │              │  Agent   │ │
│  └──────────┘              └──────────┘              └──────────┘ │
│       │                                                    │       │
│       │                                                    │       │
│       │              ┌──────────────────┐                  │       │
│       └─────────────►│   Orchestrator   │◄─────────────────┘       │
│                      │  (Rust + axum)   │                          │
│                      └────────┬─────────┘                          │
│                               │                                     │
│                      ┌────────▼─────────┐                          │
│                      │  Gap Report      │                          │
│                      │  (MD + JSON)     │                          │
│                      └──────────────────┘                          │
└─────────────────────────────────────────────────────────────────────┘
```

1. **Load** tutorial content (markdown with images)
2. **Student** agent attempts to follow instructions in a Docker container
3. When **stuck**, Student escalates to Mentor with context
4. **Mentor** researches the problem and provides hints (not solutions)
5. **Loop** continues until completion, max iterations, or timeout
6. **Report** documents all gaps with locations and suggestions

## Quick Start

### Prerequisites

- Rust 1.75+
- Python 3.11+
- Docker (Desktop or Engine)
- An LLM CLI: `claude`, `codex`, or `gemini`

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/smile.git
cd smile

# Build Rust components
cargo build --release

# Install Python wrappers
cd python && pip install -e . && cd ..

# Build the base Docker image
docker build -t smile-base:latest -f docker/Dockerfile.base .
```

### Running SMILE

```bash
# Run against a tutorial
cargo run -p smile-cli -- path/to/tutorial.md

# With a custom config
cargo run -p smile-cli -- --config smile.json path/to/tutorial.md

# With debug output
RUST_LOG=debug cargo run -p smile-cli -- path/to/tutorial.md
```

### Output

After a run completes, SMILE generates:
- `smile-report.md` - Human-readable report with gaps and suggestions
- `smile-report.json` - Machine-readable report for CI integration

## Configuration

Create `smile.json` in your project root:

```json
{
  "tutorial": "tutorial.md",
  "llmProvider": "claude",
  "maxIterations": 10,
  "timeout": 1800,
  "studentBehavior": {
    "patienceLevel": "low",
    "maxRetriesBeforeHelp": 3
  }
}
```

| Option | Default | Description |
|--------|---------|-------------|
| `tutorial` | `tutorial.md` | Path to markdown tutorial |
| `llmProvider` | `claude` | LLM provider: `claude`, `codex`, `gemini` |
| `maxIterations` | `10` | Maximum Student-Mentor cycles |
| `timeout` | `1800` | Total timeout in seconds |
| `studentBehavior.patienceLevel` | `low` | How quickly Student asks for help |
| `studentBehavior.maxRetriesBeforeHelp` | `3` | Failures before escalating |

## Real-time Observation

Connect to the WebSocket endpoint to watch the loop in real-time:

```bash
# Using websocat
websocat ws://localhost:3000/ws
```

Events:
- `connected` - Connection established, includes current state
- `iteration_start` - New iteration begins
- `student_output` - Student completes an action
- `mentor_output` - Mentor provides guidance
- `loop_complete` - Loop terminates
- `error` - Error occurred

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Current loop state |
| `/api/student/result` | POST | Submit student result |
| `/api/mentor/result` | POST | Submit mentor result |
| `/api/stop` | POST | Force stop the loop |
| `/ws` | WS | Real-time event stream |

## Project Structure

```
smile/
├── crates/
│   ├── smile-cli/          # CLI entry point
│   ├── smile-orchestrator/ # HTTP/WS server, loop state machine
│   ├── smile-container/    # Docker management via bollard
│   └── smile-report/       # Report generation
├── python/
│   └── smile_wrappers/     # Student/Mentor agent wrappers
├── docker/
│   └── Dockerfile.base     # Base image with LLM CLIs
└── tests/integration/      # End-to-end tests
```

## Development

### Prerequisites

Additional development dependencies:
- `just` for task running: `cargo install just`
- `lefthook` for git hooks: `lefthook install`

### Commands

```bash
# Build and test
just build          # Build all components
just test           # Run all tests
just test-rust      # Rust tests only
just test-python    # Python tests only

# Code quality
just fmt            # Format code
just lint           # Run linters

# Development
just run tutorial.md    # Run against a tutorial
just docker-build       # Rebuild Docker image
```

### Running Tests

```bash
# All tests
cargo test --all

# Specific crate
cargo test -p smile-orchestrator

# Integration tests (requires Docker)
cd tests/integration && cargo test
```

## Troubleshooting

### Docker not available

```bash
# Check Docker is running
docker info

# On Linux, ensure user is in docker group
sudo usermod -aG docker $USER
# Then logout and login
```

### LLM CLI not found

The container needs an LLM CLI installed. Check that `docker/Dockerfile.base` includes:
- Claude: `npm install -g @anthropic-ai/cli`
- Or another provider's CLI

### Tutorial exceeds size limit

Tutorials must be under 100KB. Split large tutorials or reduce embedded content.

## Contributing

SMILE Loop is an alpha project and actively welcomes feedback and contributions!

### Reporting Issues

- **Validation Problems**: Use the [Bug Report](https://github.com/aaronbassett/SMILE/issues/new?template=bug_report.yml) template
- **Feature Ideas**: Use the [Feature Request](https://github.com/aaronbassett/SMILE/issues/new?template=feature_request.yml) template
- **General Feedback**: Use the [Alpha Feedback](https://github.com/aaronbassett/SMILE/issues/new?template=alpha_feedback.yml) template

### Contributing Code

See [CONTRIBUTING.md](./CONTRIBUTING.md) for detailed guidelines on:
- Setting up your development environment
- Architecture overview
- Rust and Python development practices
- Testing and code quality standards
- Submitting pull requests

### Community

- Follow our [Code of Conduct](./CODE_OF_CONDUCT.md)
- Review our [Security Policy](./SECURITY.md)
- Check [CLAUDE.md](./CLAUDE.md) for project context and development notes

## License

MIT
