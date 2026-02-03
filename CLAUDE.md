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

## Clippy Patterns

Strict clippy configuration requires these patterns:

1. **Test struct initialization**: Use `Config { field: value, ..Default::default() }` not `let mut c = Config::default(); c.field = value;`
2. **Empty strings**: Use `String::new()` not `"".to_string()`
3. **No panic in tests**: Use `assert!(matches!(...), "message")` instead of `panic!()`
4. **let-else over match**: Use `let Some(x) = opt else { continue };` instead of `match opt { Some(x) => x, None => continue }`
5. **MSRV compliance**: Project MSRV is 1.75. Avoid `std::sync::LazyLock` (requires 1.80). Use `once_cell` crate or recreate resources.
6. **Raw strings**: Prefer `r"..."` over `r#"..."#` when content has no quotes
7. **Test module allows**: Tests need `#[allow(clippy::unwrap_used)]` on `mod tests`. Also allow `expect_used` and `panic` if needed: `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`
8. **Doc comment backticks**: Use backticks around method/type names in doc comments: `/// Test that \`start_container\` returns`

## Ruff Patterns

Strict ruff configuration enforces these limits:

1. **Max return statements (PLR0911)**: Max 6 returns per function. Extract helper methods for complex logic.
2. **Max branches (PLR0912)**: Max 12 branches per function. Use dictionary lookups instead of long match/if-elif chains.
3. **Sorted `__all__`**: Run `ruff check --fix` to auto-sort `__all__` exports (RUF022).
4. **Dictionary-based dispatch**: Replace match statements with dict lookups to reduce return count:
   ```python
   condition_map = {Condition.A: config.field_a, Condition.B: None}
   return condition_map.get(condition) is None or condition_map.get(condition)
   ```

## Recent Changes

- 2026-02-03: Phase 6 complete - Student agent wrapper with LLM CLI invocation, stuck detection, output parsing
- 2026-02-03: Phase 5 complete - container management via bollard (create, start, stop, remove, reset)
- 2026-02-03: Phase 4 complete - tutorial loading, image extraction, CLI integration
- 2026-02-03: Phase 3 complete - config loading, validation, CLI integration
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
