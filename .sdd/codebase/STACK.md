# Technology Stack

**Project**: SMILE Loop
**Last Updated**: 2026-02-02
**Status**: Planned (Greenfield)

---

## Languages

| Language | Version | Purpose |
|----------|---------|---------|
| Rust | 1.75+ | Orchestrator, CLI, container management, HTTP/WebSocket server |
| Python | 3.11+ | Agent wrappers, LLM CLI integration, prompt construction |

## Core Components

### Rust Components
- **smile-cli**: Main CLI binary for running SMILE Loop
- **smile-orchestrator**: Loop state machine, HTTP API, WebSocket server
- **smile-container**: Docker container lifecycle management
- **smile-report**: Report generation (Markdown + JSON)

### Python Components
- **student-wrapper**: Script that runs Student agent inside container
- **mentor-wrapper**: Script that runs Mentor agent inside container
- **prompt-builder**: Constructs prompts from tutorial + mentor notes

## Frameworks & Libraries

### Rust
| Library | Purpose |
|---------|---------|
| tokio | Async runtime for HTTP/WebSocket |
| axum | HTTP framework for orchestrator API |
| bollard | Docker API client |
| serde | JSON serialization/deserialization |
| clap | CLI argument parsing |
| tracing | Logging and instrumentation |

### Python
| Library | Purpose |
|---------|---------|
| subprocess | LLM CLI invocation |
| httpx | HTTP client for orchestrator communication |
| pydantic | Configuration and output validation |

## External Dependencies

| Dependency | Required | Purpose |
|------------|----------|---------|
| Docker | Yes | Container isolation for agents |
| claude CLI | Conditional | Claude LLM provider |
| codex CLI | Conditional | OpenAI Codex provider |
| gemini CLI | Conditional | Google Gemini provider |

## Build & Package

| Tool | Purpose |
|------|---------|
| cargo | Rust build system |
| pip/uv | Python package management |
| Docker | Container image building |

## Development Environment

- **Editor**: Any (VSCode recommended for Rust + Python)
- **OS**: Linux, macOS, Windows (WSL2)
- **Container Runtime**: Docker Desktop or Docker Engine

---

## Architecture Decision

**Why Rust + Python?**

1. **Rust for Orchestrator**: Performance-critical loop management, efficient container API calls, low resource overhead for long-running processes
2. **Python for Wrappers**: LLM CLI tools have excellent Python ecosystem support, simpler prompt manipulation, easier to modify agent behavior
3. **Clean Boundary**: Orchestrator (Rust) communicates with wrappers (Python) via HTTP - each can be developed and tested independently

---

## File Structure (Planned)

```
smile/
├── Cargo.toml              # Rust workspace
├── crates/
│   ├── smile-cli/          # CLI entry point
│   ├── smile-orchestrator/ # HTTP API, WebSocket, loop logic
│   ├── smile-container/    # Docker management
│   └── smile-report/       # Report generation
├── python/
│   ├── pyproject.toml      # Python package
│   ├── smile_wrappers/
│   │   ├── student.py      # Student agent wrapper
│   │   ├── mentor.py       # Mentor agent wrapper
│   │   └── prompts.py      # Prompt construction
│   └── tests/
├── docker/
│   ├── Dockerfile.base     # smile-base image
│   └── Dockerfile.dev      # Development image
└── tests/
    └── integration/        # End-to-end tests
```
