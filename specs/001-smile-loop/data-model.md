# Data Model: SMILE Loop

**Feature**: 001-smile-loop | **Date**: 2026-02-02

---

## Entity Relationship Overview

```
┌─────────────┐     loads      ┌─────────────┐
│ Configuration│◄──────────────│   smile.json │
└──────┬──────┘                └─────────────┘
       │
       │ configures
       ▼
┌─────────────┐     manages    ┌─────────────┐
│ Orchestrator │◄─────────────►│  Container   │
└──────┬──────┘                └──────┬──────┘
       │                              │
       │ tracks                       │ runs
       ▼                              ▼
┌─────────────┐               ┌─────────────┐
│  LoopState  │               │   Agents    │
└──────┬──────┘               └──────┬──────┘
       │                              │
       │ generates                    │ produces
       ▼                              ▼
┌─────────────┐               ┌─────────────┐
│   Report    │◄──────────────│StudentOutput│
└─────────────┘               └─────────────┘
```

---

## Core Entities

### 1. Configuration

Loaded from `smile.json`. All fields optional with defaults.

```rust
// Rust (smile-orchestrator/src/config.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_tutorial")]
    pub tutorial: String,                    // "tutorial.md"

    #[serde(default)]
    pub llm_provider: LlmProvider,           // Claude

    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,                 // 10

    #[serde(default = "default_timeout")]
    pub timeout: u32,                        // 1800 (seconds)

    #[serde(default = "default_container_image")]
    pub container_image: String,             // "smile-base:latest"

    #[serde(default)]
    pub student_behavior: StudentBehavior,

    #[serde(default)]
    pub container: ContainerConfig,

    #[serde(default = "default_state_file")]
    pub state_file: String,                  // ".smile/state.json"

    #[serde(default = "default_output_dir")]
    pub output_dir: String,                  // "."
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    Claude,
    Codex,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentBehavior {
    #[serde(default = "default_max_retries")]
    pub max_retries_before_help: u32,        // 3

    #[serde(default = "default_true")]
    pub ask_on_missing_dependency: bool,     // true

    #[serde(default = "default_true")]
    pub ask_on_ambiguous_instruction: bool,  // true

    #[serde(default = "default_true")]
    pub ask_on_command_failure: bool,        // true

    #[serde(default = "default_true")]
    pub ask_on_timeout: bool,                // true

    #[serde(default = "default_step_timeout")]
    pub timeout_seconds: u32,                // 60

    #[serde(default)]
    pub patience_level: PatienceLevel,       // Low
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatienceLevel {
    #[default]
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerConfig {
    #[serde(default = "default_true")]
    pub keep_on_failure: bool,               // true

    #[serde(default)]
    pub keep_on_success: bool,               // false
}
```

```python
# Python (smile_wrappers/config.py)
from pydantic import BaseModel, Field
from typing import Literal
from enum import Enum

class LlmProvider(str, Enum):
    CLAUDE = "claude"
    CODEX = "codex"
    GEMINI = "gemini"

class PatienceLevel(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"

class StudentBehavior(BaseModel):
    max_retries_before_help: int = Field(default=3, ge=1)
    ask_on_missing_dependency: bool = True
    ask_on_ambiguous_instruction: bool = True
    ask_on_command_failure: bool = True
    ask_on_timeout: bool = True
    timeout_seconds: int = Field(default=60, ge=1)
    patience_level: PatienceLevel = PatienceLevel.LOW

class Config(BaseModel):
    tutorial: str = "tutorial.md"
    llm_provider: LlmProvider = LlmProvider.CLAUDE
    max_iterations: int = Field(default=10, ge=1)
    timeout: int = Field(default=1800, ge=1)
    container_image: str = "smile-base:latest"
    student_behavior: StudentBehavior = Field(default_factory=StudentBehavior)
    state_file: str = ".smile/state.json"
    output_dir: str = "."
```

**Validation Rules**:
- `tutorial`: Must be valid file path, file must exist
- `max_iterations`: Positive integer
- `timeout`: Positive integer (seconds)
- `container_image`: Valid Docker image reference
- Enum fields case-insensitive
- Unknown fields at root level silently ignored (forward compatibility)

---

### 2. Tutorial

In-memory representation of loaded tutorial.

```rust
// Rust (smile-orchestrator/src/tutorial.rs)
pub struct Tutorial {
    pub path: PathBuf,
    pub content: String,
    pub images: Vec<TutorialImage>,
    pub size_bytes: usize,
}

pub struct TutorialImage {
    pub reference: String,     // path as it appears in markdown
    pub resolved_path: PathBuf, // absolute path
    pub format: ImageFormat,
    pub data: Vec<u8>,         // loaded image bytes
}

#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    Png,
    Jpg,
    Gif,
    Svg,
}
```

**Validation Rules**:
- `size_bytes`: Must be <= 100KB (102,400 bytes)
- `content`: Must be valid UTF-8
- Image paths resolved relative to tutorial file location
- Supported image formats: PNG, JPG, GIF, SVG

---

### 3. Container

Docker container state managed by orchestrator.

```rust
// Rust (smile-container/src/lib.rs)
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: ContainerStatus,
    pub mounts: Vec<Mount>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContainerStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Removing,
    Gone,
}

pub struct Mount {
    pub host_path: PathBuf,
    pub container_path: String,
    pub read_only: bool,
}
```

**Volume Mounts** (from spec):

| Host Path | Container Path | Read-Only |
|-----------|----------------|-----------|
| `{tutorial-dir}/` | `/workspace/tutorial/` | Yes |
| `{work-dir}/` | `/workspace/work/` | No |
| `{logs-dir}/` | `/workspace/logs/` | No |

---

### 4. LoopState

Orchestrator's internal state, persisted to state file.

```rust
// Rust (smile-orchestrator/src/loop_state.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    pub status: LoopStatus,
    pub iteration: u32,
    pub mentor_notes: Vec<MentorNote>,
    pub history: Vec<IterationRecord>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopStatus {
    Starting,
    RunningStudent,
    WaitingForStudent,
    RunningMentor,
    WaitingForMentor,
    Completed,
    MaxIterations,
    Blocker,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentorNote {
    pub iteration: u32,
    pub question: String,
    pub answer: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationRecord {
    pub iteration: u32,
    pub student_output: StudentOutput,
    pub mentor_output: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}
```

**State Transitions** (from spec state machine):

```
Starting → RunningStudent → WaitingForStudent → (check result)
  ├─ completed → Completed
  ├─ ask_mentor → RunningMentor → WaitingForMentor → RunningStudent
  ├─ cannot_complete → Blocker
  ├─ max iterations reached → MaxIterations
  └─ global timeout → Timeout
```

---

### 5. StudentOutput

Structured output from Student agent.

```rust
// Rust (smile-orchestrator/src/student.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StudentOutput {
    pub status: StudentStatus,
    pub current_step: String,
    pub attempted_actions: Vec<String>,
    pub problem: Option<String>,
    pub question_for_mentor: Option<String>,
    pub reason: Option<String>,
    pub summary: String,
    pub files_created: Vec<String>,
    pub commands_run: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudentStatus {
    Completed,
    AskMentor,
    CannotComplete,
}
```

```python
# Python (smile_wrappers/output.py)
from pydantic import BaseModel
from typing import Literal, Optional

class StudentOutput(BaseModel):
    status: Literal["completed", "ask_mentor", "cannot_complete"]
    current_step: str
    attempted_actions: list[str]
    problem: Optional[str] = None
    question_for_mentor: Optional[str] = None
    reason: Optional[str] = None
    summary: str
    files_created: list[str] = []
    commands_run: list[str] = []
```

**Validation Rules**:
- `status`: Required, one of three values
- `current_step`: Required, non-empty string
- `question_for_mentor`: Required when `status == "ask_mentor"`
- `reason`: Required when `status == "cannot_complete"`

---

### 6. Report

Final output document generated after loop completion.

```rust
// Rust (smile-report/src/lib.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub tutorial_name: String,
    pub summary: ReportSummary,
    pub gaps: Vec<Gap>,
    pub timeline: Vec<TimelineEntry>,
    pub audit_trail: AuditTrail,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub status: LoopStatus,
    pub iterations: u32,
    pub duration_seconds: u64,
    pub tutorial_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub id: u32,
    pub title: String,
    pub location: GapLocation,
    pub problem: String,
    pub suggested_fix: String,
    pub severity: GapSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapLocation {
    pub line_number: Option<u32>,
    pub quote: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GapSeverity {
    Critical,  // Blocks progress completely
    Major,     // Significant confusion or delay
    Minor,     // Minor clarification needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub timestamp: DateTime<Utc>,
    pub iteration: u32,
    pub event: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrail {
    pub commands: Vec<AuditCommand>,
    pub files: Vec<AuditFile>,
    pub llm_calls: Vec<AuditLlmCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub priority: u32,
    pub category: String,
    pub description: String,
}
```

**Output Files**:
- `smile-report.md`: Human-readable Markdown
- `smile-report.json`: Programmatic JSON (serialized Report struct)
- `smile-audit.log`: Raw execution log (text)

---

## API Request/Response Models

See `contracts/orchestrator-api.yaml` for full OpenAPI specification.

### POST /api/student/result

```rust
#[derive(Debug, Deserialize)]
pub struct StudentResultRequest {
    pub student_output: StudentOutput,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct StudentResultResponse {
    pub acknowledged: bool,
    pub next_action: NextAction,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NextAction {
    Continue,
    Stop,
}
```

### POST /api/mentor/result

```rust
#[derive(Debug, Deserialize)]
pub struct MentorResultRequest {
    pub mentor_output: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MentorResultResponse {
    pub acknowledged: bool,
    pub next_action: NextAction,
}
```

### GET /api/status

```rust
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub state: LoopState,
}
```

### POST /api/stop

```rust
#[derive(Debug, Deserialize)]
pub struct StopRequest {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct StopResponse {
    pub stopped: bool,
    pub final_state: LoopState,
}
```

---

## WebSocket Events

See `contracts/websocket-events.yaml` for full schema.

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum WsEvent {
    Connected { state: LoopState },
    IterationStart { iteration: u32, timestamp: DateTime<Utc> },
    StudentOutput { status: StudentStatus, summary: String, current_step: String },
    MentorOutput { notes: String },
    LoopComplete { status: LoopStatus, summary: String, iterations: u32 },
    Error { message: String },
}
```
