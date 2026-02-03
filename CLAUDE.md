# SMILE Loop - Claude Code Context

This document provides context for Claude Code when working in the SMILE repository.

## Project Overview

SMILE Loop validates technical tutorials by simulating a constrained learner (Student agent) that attempts to follow instructions, escalating to a Mentor agent when stuck. Built with Rust (orchestrator) and Python (agent wrappers).

## Active Technologies

- **Rust 1.75+**: Orchestrator, CLI, container management, HTTP/WebSocket server
- **Python 3.11+**: Agent wrappers, LLM CLI integration
- **Docker**: Container isolation for agent execution
- **tokio/axum**: Async HTTP and WebSocket server
- **bollard**: Docker API client
- **pydantic**: Config and output validation
- **just**: Task runner

## Project Structure

```
smile/
├── Cargo.toml              # Rust workspace root
├── Justfile                # Task runner commands
├── crates/
│   ├── smile-cli/          # CLI entry point
│   ├── smile-orchestrator/ # HTTP API, WebSocket, loop state machine
│   ├── smile-container/    # Docker lifecycle via bollard
│   └── smile-report/       # Markdown/JSON report generation
├── python/
│   ├── pyproject.toml      # Python package config
│   └── smile_wrappers/     # Student/Mentor agent wrappers
├── docker/
│   ├── Dockerfile.base     # Base image with LLM CLIs
│   └── Dockerfile.dev      # Development image
├── tests/integration/      # End-to-end tests
├── specs/                  # Feature specifications
│   └── 001-smile-loop/     # Current feature
└── .sdd/                   # SDD workflow artifacts
```

## Key Commands

### Build & Test

```bash
just build              # Build all Rust crates
just test               # Run all tests (Rust + Python)
just test-rust          # Run Rust tests only
just test-python        # Run Python tests only
cargo test -p smile-orchestrator  # Test specific crate
```

### Linting & Formatting

```bash
just fmt                # Format all code
just lint               # Run all linters
cargo clippy --all      # Rust lints
cd python && ruff check # Python lints
```

### Running

```bash
just run tutorial.md    # Run SMILE against a tutorial
cargo run -p smile-cli -- tutorial.md  # Direct cargo run
RUST_LOG=debug just run tutorial.md    # With debug logging
```

### Docker

```bash
just docker-build       # Build smile-base image
docker exec -it <id> bash  # Shell into running container
```

## Architecture Notes

1. **Orchestrator-Wrapper Communication**: Wrappers (Python) inside containers call back to orchestrator (Rust) via HTTP using `host.docker.internal:3000`

2. **State Machine**: Loop state persisted to `.smile/state.json` for crash recovery

3. **WebSocket Events**: Real-time observation via `ws://localhost:3000/ws`

4. **LLM CLI Invocation**: Wrappers call claude/codex/gemini CLIs via subprocess

## Constitution Principles

Follow these principles when making changes:

1. **Ship Fast**: Minimal implementations first, iterate on real pain
2. **KISS**: Simplest solution that works, single-purpose components
3. **Modularity**: Clear interfaces between Rust crates and Python wrappers
4. **Test What Matters**: Integration tests over unit tests, test real workflows
5. **Fail Fast**: Clear error messages with context and suggestions
6. **README First**: Document before feature is complete

## Recent Changes

- 2026-02-02: Initial planning phase for 001-smile-loop feature

## Useful Files

- Spec: `specs/001-smile-loop/spec.md`
- Plan: `specs/001-smile-loop/plan.md`
- Data Model: `specs/001-smile-loop/data-model.md`
- API Contracts: `specs/001-smile-loop/contracts/`
- Constitution: `.sdd/memory/constitution.md`

<!-- MANUAL ADDITIONS START -->
<!-- Add project-specific notes below this line -->
<!-- MANUAL ADDITIONS END -->
