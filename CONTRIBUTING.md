# Contributing to SMILE Loop

Thank you for your interest in contributing to SMILE Loop! This document provides guidelines for reporting issues, submitting feedback, and contributing code.

## Table of Contents

- [Reporting Validation Issues](#reporting-validation-issues)
- [Reporting Bugs](#reporting-bugs)
- [Feature Requests](#feature-requests)
- [Alpha Feedback](#alpha-feedback)
- [Code Contributions](#code-contributions)
  - [Development Setup](#development-setup)
  - [Architecture Overview](#architecture-overview)
  - [Rust Development](#rust-development)
  - [Python Development](#python-development)
  - [Testing](#testing)
  - [Code Quality](#code-quality)
  - [Submitting Changes](#submitting-changes)

## Reporting Validation Issues

SMILE Loop validates technical tutorials by simulating learners. If you're using SMILE Loop to validate tutorials, here's how to report issues effectively:

### Before Reporting

1. **Run with debug output** to get detailed logs:
   ```bash
   RUST_LOG=debug cargo run -p smile-cli -- path/to/tutorial.md
   ```

2. **Check container logs** if the Student or Mentor agent fails:
   ```bash
   docker logs <container_id>
   ```

3. **Verify prerequisites** are met:
   - Rust 1.75+ installed
   - Python 3.11+ available
   - Docker running
   - LLM CLI installed (claude, codex, or gemini)

4. **Review existing issues** at https://github.com/aaronbassett/SMILE/issues

### Creating a Validation Issue

Use the [Bug Report](https://github.com/aaronbassett/SMILE/issues/new?template=bug_report.yml) template and include:

**Essential Information:**
- The tutorial content (or relevant section)
- Steps to reproduce
- Expected vs. actual behavior
- Error output or logs

**Context:**
- Which validation step failed
- LLM provider and version
- Configuration used (smile.json)
- Your system (OS, Rust version, Docker version)

**Example:**
```markdown
**Bug Category:** Student Agent

**What happened?**
Student got stuck trying to run `npm install` and asked for help.

**Tutorial:**
```
# Node.js Setup

Run `npm install` to install dependencies.
```

**Steps to Reproduce:**
1. Created smile.json with default config
2. Ran `RUST_LOG=debug cargo run -p smile-cli -- tutorial.md`
3. Student successfully downloaded Node.js but failed on npm install

**Expected Behavior:**
Student should attempt npm install, see a helpful error, and ask Mentor for help.

**Actual Behavior:**
Student got permission denied error and exited instead of asking for help.

**Logs:**
[Include error output]
```

## Reporting Bugs

If you find a bug in SMILE Loop itself (not in tutorial validation), use the [Bug Report](https://github.com/aaronbassett/SMILE/issues/new?template=bug_report.yml) template.

### Bug Report Guidelines

- **One issue per bug** - Don't combine multiple issues
- **Reproducible example** - We need to reproduce it to fix it
- **Specific details** - OS, versions, exact commands
- **Logs and output** - Error messages help tremendously
- **Minimal tutorial** - If possible, provide a small tutorial that triggers the bug

## Feature Requests

Have an idea to improve SMILE Loop? Use the [Feature Request](https://github.com/aaronbassett/SMILE/issues/new?template=feature_request.yml) template.

### Feature Request Guidelines

- **Problem-first** - Start with the problem you're solving
- **Use cases** - Show real scenarios where this helps
- **Implementation ideas** - Suggest how it might work
- **Examples** - Include usage examples if possible

## Alpha Feedback

SMILE Loop is an alpha release. We actively seek feedback on:

- **Accuracy** - Does SMILE Loop correctly identify tutorial gaps?
- **Usability** - Is the setup and workflow smooth?
- **Report quality** - Are the validation reports helpful?
- **Agent behavior** - Do Student/Mentor agents act appropriately?
- **Performance** - How fast is validation?

Use the [Alpha Feedback](https://github.com/aaronbassett/SMILE/issues/new?template=alpha_feedback.yml) template to share experiences.

## Code Contributions

### Development Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/aaronbassett/SMILE.git
   cd SMILE
   ```

2. **Install dependencies:**
   ```bash
   # Rust
   rustup update

   # Python
   python -m venv venv
   source venv/bin/activate
   pip install -e "python[dev]"

   # Task runner
   cargo install just
   ```

3. **Verify setup:**
   ```bash
   just build
   just test
   ```

### Architecture Overview

SMILE Loop consists of:

- **Rust Crates:**
  - `smile-cli` - Command-line interface entry point
  - `smile-orchestrator` - HTTP API, WebSocket server, loop state machine
  - `smile-container` - Docker lifecycle management (bollard)
  - `smile-report` - Markdown and JSON report generation

- **Python Packages:**
  - `smile_wrappers` - Student and Mentor agent wrappers
  - LLM CLI integration (claude, codex, gemini)

- **Docker:**
  - `Dockerfile.base` - Base image with LLM CLI tools
  - `Dockerfile.dev` - Development image with debugging tools

### Rust Development

#### Code Style

Follow Rust conventions with strict lint settings. See `Cargo.toml` and `clippy.toml` for configuration.

Key patterns:
- Use `String::new()` instead of `"".to_string()`
- Use `let-else` for Option/Result handling
- No `unwrap()` - use proper error handling or `#[allow(clippy::unwrap_used)]` with justification in tests
- Document public APIs with doc comments

#### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Specific crate
cargo build -p smile-orchestrator
```

#### Testing

```bash
# All Rust tests
cargo test --all

# Specific crate
cargo test -p smile-orchestrator

# With output
cargo test -- --nocapture

# Run ignored (integration) tests
cargo test -- --ignored --nocapture
```

Integration tests require Docker. They're skipped in CI but run locally.

#### Code Quality

```bash
# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Fix clippy warnings
cargo clippy --all-targets --all-features --fix --allow-dirty
```

### Python Development

#### Code Style

Use `ruff` for linting and `ruff format` for formatting. Configuration in `pyproject.toml`.

Key enforcements:
- Max 6 returns per function
- Max 12 branches per function
- No `print()` - use logging
- Type hints on public functions

#### Testing

```bash
# Run tests
cd python && pytest

# With coverage
pytest --cov=smile_wrappers --cov-report=html

# Specific test
pytest smile_wrappers/tests/test_student.py::test_initialization
```

#### Code Quality

```bash
# Lint
cd python && ruff check .

# Fix issues
ruff check --fix .

# Format
ruff format .

# Type check
mypy smile_wrappers
```

### Testing

#### Rust Tests

```bash
# All tests
just test

# Only Rust
just test-rust

# Only Python
just test-python

# Integration tests (requires Docker)
just test-integration

# With logging
RUST_LOG=debug cargo test
```

#### Python Tests

```bash
# Run all
cd python && pytest

# Coverage report
pytest --cov=smile_wrappers --cov-report=term-missing

# Specific file
pytest smile_wrappers/tests/test_student.py

# Matching pattern
pytest -k "test_stuck_detection"
```

#### What Should Be Tested

- **Integration tests** - Real workflows (tutorial validation, agent interaction)
- **Error handling** - Invalid configs, missing containers, LLM failures
- **State persistence** - Crash recovery, state.json correctness
- **Agent behavior** - Student getting stuck, Mentor escalation
- **Report generation** - Markdown and JSON structure

Test real scenarios, not just unit code paths.

### Submitting Changes

#### Branch Naming

- `feature/description` - New features
- `fix/description` - Bug fixes
- `docs/description` - Documentation
- `refactor/description` - Code improvements
- `test/description` - Test additions

#### Commit Messages

Use conventional commits:

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
```
feat(student): improve stuck detection for vague errors

- Analyze error message semantics
- Escalate after 2 vague errors instead of 3

Fixes #42
```

```
fix(container): handle docker socket errors gracefully

When Docker socket is unavailable, provide clear error message.

Relates to #35
```

#### Pull Request Process

1. **Create a feature branch** from `main`
2. **Make your changes** with descriptive commits
3. **Run tests and linters:**
   ```bash
   just ci  # Runs fmt-check, lint, and test
   ```
4. **Fix any issues** - All checks must pass
5. **Push to your fork**
6. **Create a pull request** with:
   - Clear description of changes
   - Reference to related issue (if any)
   - Testing notes
   - Screenshots (for UI changes)

#### PR Template

```markdown
## Description
Brief summary of changes.

## Related Issue
Fixes #123

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Refactoring

## Testing
Describe how you tested this:
- [ ] Unit tests added
- [ ] Integration tests added
- [ ] Manual testing: [describe]
- [ ] All tests passing

## Checklist
- [ ] Code follows style guidelines
- [ ] Documentation updated
- [ ] No new warnings from clippy/ruff
- [ ] Tests pass locally
```

#### Code Review

All PRs require at least one approval. Reviewers will check:
- Code quality and style
- Test coverage
- Performance impact
- Documentation
- Breaking changes

Be responsive to feedback and open to discussion.

### Documentation Contributions

Documentation improvements are always welcome!

- **README** - Installation, quick start, configuration
- **Code docs** - Rust doc comments, Python docstrings
- **Guides** - Tutorials, troubleshooting, examples
- **API docs** - Endpoint descriptions, examples

Build and view Rust docs:
```bash
cargo doc --no-deps --open
```

## Questions or Need Help?

- **GitHub Issues** - For bugs and features
- **GitHub Discussions** - For questions and ideas
- **Alpha Feedback** - For validation and usage feedback

See [CLAUDE.md](./CLAUDE.md) for project context and recent changes.

## Code of Conduct

This project is committed to providing a welcoming and inclusive environment. Please read our [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md).

## License

By contributing to SMILE Loop, you agree that your contributions will be licensed under the MIT License.

---

Thank you for making SMILE Loop better!
