# Research: SMILE Loop

**Feature**: 001-smile-loop | **Date**: 2026-02-02

This document captures research findings for the SMILE Loop implementation. Detailed reference materials are in `research-artifacts/`.

---

## 1. Rust Async HTTP/WebSocket Server (tokio + axum)

### Decision: Use tokio + axum with broadcast channels

**Rationale**: axum provides type-safe extractors, integrates with tower middleware, and has first-class WebSocket support. tokio's broadcast channel efficiently handles multi-client event distribution.

**Alternatives Considered**:
- actix-web: Higher performance but more complex lifetime management
- warp: Simpler but less ecosystem support
- hyper directly: Too low-level for our needs

### Key Patterns

1. **Shared State**: Use `Arc<RwLock<T>>` for state accessed by both HTTP handlers and background loop
   - Always release locks before await points (scope in blocks)
   - Use `RwLock` for read-heavy workloads

2. **WebSocket Broadcast**: Use `tokio::sync::broadcast` channel
   - Fixed buffer (100 events) - old events auto-discarded
   - Each client subscribes independently
   - Dropped subscribers auto-cleanup

3. **Graceful Shutdown**: Broadcast a shutdown signal via dedicated channel
   - All tasks select on shutdown receiver
   - Drop sender to trigger all receivers to error

4. **Error Handling**: Use `thiserror` for typed errors that implement `IntoResponse`
   ```rust
   #[derive(Error, Debug)]
   pub enum SmileError {
       #[error("Config error: {0}")]
       Config(String),
       #[error("Docker error: {0}")]
       Docker(#[from] bollard::errors::Error),
   }
   ```

---

## 2. Docker Container Management (bollard)

### Decision: Use bollard with explicit lifecycle management

**Rationale**: bollard is the most maintained Docker API client for Rust, supports async, and provides full API coverage.

**Alternatives Considered**:
- shiplift: Older, less maintained
- Direct Docker CLI via subprocess: Loses type safety and error handling

### Key Patterns

1. **Connection**: Connect via default socket, verify with ping
   ```rust
   let docker = Docker::connect_with_socket_defaults()?;
   docker.ping().await?;
   ```

2. **Volume Mounts**: Validate host paths before container creation
   ```rust
   let mount = Mount {
       target: Some("/workspace/tutorial".to_string()),
       source: Some(tutorial_dir.to_string()),
       typ: Some(MountTypeEnum::BIND),
       read_only: Some(true),
       ..Default::default()
   };
   ```

3. **Container Reset**: Stop, remove, recreate
   - Use timeout on stop (10s default)
   - Force remove if stop fails
   - Always recreate from image for clean slate

4. **Host Communication**: Configure `host.docker.internal`
   - Linux requires explicit extra_hosts
   - macOS/Windows have it by default
   ```rust
   extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
   ```

5. **Command Execution**: Use exec with timeout
   - Always consume output streams completely
   - Set timeout via tokio::time::timeout
   - Handle exit codes explicitly

### Common Pitfalls Avoided

- Always start container after create (create returns stopped)
- Consume exec output streams fully (prevent leaks)
- Set timeouts on all long-running operations
- Handle container name conflicts (remove stale containers)

---

## 3. Python LLM CLI Wrappers

### Decision: Use subprocess with pydantic validation

**Rationale**: Direct CLI invocation provides maximum compatibility with different LLM providers. Pydantic ensures type-safe structured output.

**Alternatives Considered**:
- Python SDK for each provider: More coupling, version conflicts
- HTTP API directly: Loses CLI-specific features

### Key Patterns

1. **Structured Output**: Use pydantic models with JSON extraction
   ```python
   class StudentOutput(BaseModel):
       status: Literal["completed", "ask_mentor", "cannot_complete"]
       current_step: str
       question_for_mentor: Optional[str] = None
   ```

2. **Error Categories**: Distinguish retryable vs permanent
   - Timeout: Retryable with longer timeout
   - Rate limit: Retryable with backoff
   - Auth failure: Permanent (fail fast)
   - Parse error: Retry with recovery prompt

3. **Retry with Backoff**: Exponential backoff with jitter
   ```python
   delay = min(base_delay * (2 ** attempt), max_delay)
   delay += random.uniform(0, delay * 0.1)  # jitter
   ```

4. **Timeout Handling**: Progressive timeouts
   - Startup: 10s (CLI initialization)
   - Per-chunk: 30s (streaming response)
   - Total: 300s (entire call)

5. **Prompt Construction**: Include schema in system prompt
   - Generate JSON schema from pydantic model
   - Include in instructions: "Output JSON matching this schema: ..."
   - Request structured output mode if supported

### Output Recovery

When JSON parsing fails:
1. Try extracting JSON from markdown code blocks
2. Try finding first `{...}` or `[...]` block
3. If fails 3x consecutively, treat as cannot_complete

---

## 4. Inter-Process Communication

### Decision: HTTP callbacks from container to orchestrator

**Rationale**: HTTP is language-agnostic, debuggable, and works reliably across the Docker boundary.

**Alternatives Considered**:
- Unix sockets: Complex volume mounting
- gRPC: Overkill for simple request/response
- File-based: Polling is inefficient

### Pattern

1. Orchestrator listens on host port (e.g., 3000)
2. Container uses `host.docker.internal:3000` to reach orchestrator
3. Python wrappers use httpx for async HTTP calls
4. Pydantic validates request/response bodies

---

## 5. State Persistence

### Decision: JSON file with atomic writes

**Rationale**: Simple, debuggable, survives crashes. No external dependencies.

**Alternatives Considered**:
- SQLite: Overkill for single-user local tool
- In-memory only: Loses crash recovery

### Pattern

1. Write to temp file first
2. fsync the temp file
3. Atomic rename to target path
4. On startup, check for state file and resume

---

## Reference Materials

Detailed code examples and patterns are in `research-artifacts/`:
- `BOLLARD_BEST_PRACTICES.md` - Comprehensive bollard patterns
- `BOLLARD_QUICK_REFERENCE.md` - Quick lookup for Docker operations
- `bollard_examples.rs` - Production-ready Rust code
- `LLM_CLI_WRAPPER_GUIDE.md` - Full Python wrapper guide
- `llm_cli_wrapper.py` - Production-ready Python implementation
