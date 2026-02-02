# SMILE Wrappers

Python agent wrappers for the SMILE (Student-Mentor Iterative Loop Evaluator) system.

## Overview

This package provides Student and Mentor agent wrappers that:
- Invoke LLM CLIs (Claude, Codex, Gemini) to execute tutorial steps
- Communicate with the SMILE orchestrator via HTTP callbacks
- Parse structured outputs for loop state management

## Installation

```bash
pip install -e ".[dev]"
```

## Usage

The wrappers are designed to run inside Docker containers managed by the SMILE orchestrator.

### Student Agent

```bash
smile-student --config config.json --tutorial tutorial.md
```

### Mentor Agent

```bash
smile-mentor --config config.json --context stuck-context.json
```

## Development

```bash
# Run tests
pytest

# Run linter
ruff check .

# Run type checker
mypy smile_wrappers
```
