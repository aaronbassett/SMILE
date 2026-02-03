# Justfile for SMILE Loop
# Install just: cargo install just

# Default recipe - show available commands
default:
    @just --list

# Build all Rust crates
build:
    cargo build --all

# Build release version
build-release:
    cargo build --all --release

# Run all tests (Rust + Python)
test: test-rust test-python

# Run Rust tests
test-rust:
    cargo test --all

# Run Python tests
test-python:
    cd python && pytest

# Run Python tests with coverage
test-python-cov:
    cd python && pytest --cov=smile_wrappers --cov-report=html

# Format all code
fmt: fmt-rust fmt-python

# Format Rust code
fmt-rust:
    cargo fmt --all

# Format Python code
fmt-python:
    cd python && ruff format .

# Check formatting without modifying files
fmt-check: fmt-check-rust fmt-check-python

fmt-check-rust:
    cargo fmt --all -- --check

fmt-check-python:
    cd python && ruff format --check .

# Lint all code
lint: lint-rust lint-python

# Lint Rust code
lint-rust:
    cargo clippy --all-targets --all-features -- -D warnings

# Lint Python code
lint-python:
    cd python && ruff check .

# Fix linting issues where possible
lint-fix: lint-fix-rust lint-fix-python

lint-fix-rust:
    cargo clippy --all-targets --all-features --fix --allow-dirty

lint-fix-python:
    cd python && ruff check --fix .

# Type check Python
typecheck:
    cd python && mypy smile_wrappers

# Run all checks (what CI runs)
ci: fmt-check lint test typecheck
    @echo "All CI checks passed!"

# Quick check during development
check:
    cargo check --all
    cd python && ruff check .

# Run SMILE against a tutorial
run TUTORIAL:
    cargo run -p smile-cli -- {{TUTORIAL}}

# Run with debug logging
run-debug TUTORIAL:
    RUST_LOG=debug cargo run -p smile-cli -- {{TUTORIAL}}

# Start orchestrator only (for debugging)
orchestrator:
    cargo run -p smile-orchestrator

# Build Docker images
docker-build:
    docker build -t smile-base:latest -f docker/Dockerfile.base .

# Build dev Docker image
docker-build-dev:
    docker build -t smile-dev:latest -f docker/Dockerfile.dev .

# Clean build artifacts
clean:
    cargo clean
    rm -rf python/.pytest_cache python/.mypy_cache python/.ruff_cache
    rm -rf python/htmlcov python/.coverage
    find python -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true

# Install Python dev dependencies
setup-python:
    cd python && pip install -e ".[dev]"

# Install git hooks via lefthook
setup-hooks:
    lefthook install

# Full project setup
setup: setup-python setup-hooks
    @echo "Project setup complete!"

# Watch mode for Rust (requires cargo-watch)
watch:
    cargo watch -x check -x 'test --all'

# Generate documentation
docs:
    cargo doc --no-deps --open

# Run integration tests (requires Docker)
test-integration:
    cargo test --test '*' -- --ignored

# Audit dependencies for security issues
audit:
    cargo audit
    cd python && pip-audit

# Update dependencies
update:
    cargo update
    cd python && pip install --upgrade -e ".[dev]"
