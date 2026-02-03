# Feature Specification: SMILE Loop

**Feature Branch**: `001-smile-loop`
**Created**: 2026-02-02
**Status**: Draft
**Codebase Documentation**: See [.sdd/codebase/](.sdd/codebase/) for technical details
**Discovery**: See `discovery/` folder for full context

---

## Problem Statement

Tutorial authors (documentation teams, OSS maintainers, developer advocates) struggle to identify gaps, assumed knowledge, and unclear instructions in their technical tutorials. They typically discover problems only after users complain, submit support tickets, or abandon tutorials entirely.

SMILE Loop solves this by simulating a learner with intentionally constrained capabilities, automatically discovering what's missing before real users do. Authors run SMILE Loop locally before publishing and receive a comprehensive report with specific locations and suggested fixes.

## Personas

| Persona | Description | Primary Goals |
|---------|-------------|---------------|
| Tutorial Author | Creates technical tutorials (internal docs, OSS, devrel) | Produce clear, complete tutorials that learners can follow successfully |
| Tutorial Editor | Reviews and improves tutorial content | Identify gaps and improve tutorial quality systematically |
| Tutorial Learner | The simulated persona the Student agent embodies | Complete the tutorial using only provided instructions |

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Run Tutorial Validation (Priority: P1)

As a tutorial author, I want to run SMILE Loop against my markdown tutorial so that I can discover gaps and unclear instructions before publishing.

**Why this priority**: This is the core value proposition - without the ability to run validation, the tool provides no value.

**Independent Test**: Can be fully tested by running SMILE against a sample tutorial with intentional gaps and verifying it produces a report identifying those gaps.

**Acceptance Scenarios**:

1. **Given** a valid `smile.json` config and tutorial file, **When** I run SMILE Loop, **Then** the system starts a containerized environment and begins validation
2. **Given** the tutorial has missing prerequisites, **When** the Student agent encounters them, **Then** it escalates to the Mentor and the gap is logged
3. **Given** the loop completes, **When** I check the output, **Then** I receive both Markdown and JSON reports with identified gaps

---

### User Story 2 - Configure Validation Behavior (Priority: P1)

As a tutorial author, I want to customize how SMILE Loop behaves so that I can tune sensitivity and match my tutorial's complexity.

**Why this priority**: Different tutorials need different patience levels and stuck triggers - without configuration, the tool is too rigid.

**Independent Test**: Can be tested by creating configs with different settings and verifying behavior changes accordingly.

**Acceptance Scenarios**:

1. **Given** a `smile.json` exists in the current directory, **When** SMILE starts, **Then** it loads and validates all settings from that file
2. **Given** no `smile.json` exists, **When** SMILE starts, **Then** it uses all default values
3. **Given** a `smile.json` with invalid JSON or unknown fields, **When** SMILE attempts to load it, **Then** it exits with a descriptive error message

#### Configuration Schema

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tutorial` | string | `"tutorial.md"` | Path to markdown tutorial |
| `llmProvider` | enum | `"claude"` | One of: `claude`, `codex`, `gemini` |
| `maxIterations` | integer | `10` | Maximum loop iterations |
| `timeout` | integer | `1800` | Total timeout in seconds (30 min) |
| `containerImage` | string | `"smile-base:latest"` | Docker image for execution |
| `studentBehavior.maxRetriesBeforeHelp` | integer | `3` | Failures before asking Mentor |
| `studentBehavior.askOnMissingDependency` | boolean | `true` | Ask when dependency not found |
| `studentBehavior.askOnAmbiguousInstruction` | boolean | `true` | Ask when instruction unclear |
| `studentBehavior.askOnCommandFailure` | boolean | `true` | Ask when command fails |
| `studentBehavior.askOnTimeout` | boolean | `true` | Ask when step times out |
| `studentBehavior.timeoutSeconds` | integer | `60` | Per-step timeout |
| `studentBehavior.patienceLevel` | enum | `"low"` | One of: `low`, `medium`, `high` |
| `container.keepOnFailure` | boolean | `true` | Preserve container on failure for debugging |
| `container.keepOnSuccess` | boolean | `false` | Preserve container on success |
| `stateFile` | string | `".smile/state.json"` | Path for loop state persistence |
| `outputDir` | string | `"."` | Directory for report output |

#### Configuration Notes

- Enum values (`llmProvider`, `patienceLevel`) are case-insensitive
- Unknown fields at root level are silently ignored (forward compatibility)
- Unknown fields in `studentBehavior.*` are ignored
- Invalid enum values cause validation error

---

### User Story 3 - Load Tutorial Content (Priority: P1)

As a tutorial author, I want SMILE to properly load my markdown tutorial including images so that the Student agent can understand visual instructions.

**Why this priority**: Tutorial ingestion is foundational - the system cannot validate what it cannot read.

**Independent Test**: Can be tested by loading tutorials with various formats, sizes, and image references.

**Acceptance Scenarios**:

1. **Given** a markdown file exists at the configured path, **When** SMILE loads the tutorial, **Then** the raw markdown content is available for the Student agent
2. **Given** a markdown file contains image references, **When** SMILE loads the tutorial, **Then** images are resolved from paths relative to the tutorial file
3. **Given** a markdown file exceeds 100KB, **When** SMILE attempts to load it, **Then** it exits with error "Tutorial exceeds size limit (100KB)"

#### Specification

| Aspect | Value |
|--------|-------|
| Max file size | 100KB |
| Encoding | UTF-8 only |
| Image formats | PNG, JPG, GIF, SVG |
| Image paths | Relative to tutorial file location |
| Preprocessing | None — raw markdown passed to Student |

---

### User Story 4 - Isolated Execution Environment (Priority: P1)

As a SMILE orchestrator, I want to manage Docker containers for agent execution so that Student and Mentor run in isolated, reproducible environments.

**Why this priority**: Container isolation ensures reproducible results and prevents tutorial validation from affecting the host system.

**Independent Test**: Can be tested by verifying container starts, resets between iterations, and cleans up properly.

**Acceptance Scenarios**:

1. **Given** a valid configuration and tutorial, **When** SMILE begins a loop, **Then** it starts a container from the configured image with volumes mounted
2. **Given** an iteration completes, **When** the next iteration begins, **Then** the container is reset to a clean slate
3. **Given** Docker daemon is not running, **When** SMILE attempts to start, **Then** it exits with error "Docker is required but not available"

#### Container Reset Algorithm

1. Execute cleanup script in container (if exists)
2. If cleanup fails or times out (10s), stop container
3. Recreate container from image
4. If recreation fails, exit with error

#### Volume Mount Structure

| Host Path | Container Path | Purpose |
|-----------|----------------|---------|
| `{tutorial-dir}/` | `/workspace/tutorial/` | Tutorial file + images (read-only) |
| `{work-dir}/` | `/workspace/work/` | Student working directory |
| `{logs-dir}/` | `/workspace/logs/` | Output logs and artifacts |

---

### User Story 5 - Student Agent Follows Tutorial (Priority: P1)

As a SMILE orchestrator, I want to run the Student agent with the tutorial and mentor notes so that it attempts to follow the tutorial and reports stuck points.

**Why this priority**: The Student agent is the core simulation - it embodies the learner persona that discovers gaps.

**Independent Test**: Can be tested by running Student with tutorials of varying quality and verifying appropriate outputs.

**Acceptance Scenarios**:

1. **Given** an iteration begins, **When** the Student agent is invoked, **Then** it receives: raw tutorial markdown, accumulated mentor notes, and behavior settings
2. **Given** the Student successfully follows all tutorial steps, **When** it finishes, **Then** it outputs JSON with `status: "completed"`
3. **Given** the Student encounters a problem matching stuck triggers, **When** it asks for help, **Then** it outputs JSON with `status: "ask_mentor"` and a specific question
4. **Given** the Student encounters an unresolvable blocker, **When** it cannot continue, **Then** it outputs JSON with `status: "cannot_complete"` and the reason

#### Student Output Schema

```json
{
  "status": "completed" | "ask_mentor" | "cannot_complete",
  "currentStep": "string - what step you were on",
  "attemptedActions": ["array of actions taken"],
  "problem": "string - what went wrong (if not completed)",
  "questionForMentor": "string - specific question (if ask_mentor)",
  "reason": "string - why can't continue (if cannot_complete)",
  "summary": "string - what was accomplished",
  "filesCreated": ["array of file paths created"],
  "commandsRun": ["array of commands executed"]
}
```

#### Patience Level Effects

| Level | Behavior |
|-------|----------|
| `low` | Ask for help at first sign of difficulty; minimal retries |
| `medium` | Try 2-3 approaches before asking; moderate persistence |
| `high` | Exhaust multiple options before asking; high persistence |

---

### User Story 6 - Stuck Detection & Escalation (Priority: P1)

As a Student agent, I want to recognize when I'm stuck based on configured triggers so that I ask the Mentor for help at the right time.

**Why this priority**: Proper stuck detection determines whether gaps are identified - too sensitive wastes cycles, too lenient misses problems.

**Independent Test**: Can be tested by configuring different triggers and verifying escalation behavior.

**Acceptance Scenarios**:

1. **Given** `maxRetriesBeforeHelp` is 3, **When** Student fails the same step 3 times, **Then** Student outputs `ask_mentor` status
2. **Given** `askOnMissingDependency` is true, **When** Student encounters "command not found", **Then** Student outputs `ask_mentor` with the missing dependency noted
3. **Given** `askOnAmbiguousInstruction` is true, **When** Student cannot interpret an instruction, **Then** Student outputs `ask_mentor` with the confusing instruction quoted

#### Trigger Configuration Matrix

| Trigger | When Fires | Student Action |
|---------|------------|----------------|
| `maxRetriesBeforeHelp` | N consecutive failures on same step | `ask_mentor` |
| `askOnMissingDependency` | Package/command not found | `ask_mentor` |
| `askOnAmbiguousInstruction` | Cannot parse intent | `ask_mentor` |
| `askOnCommandFailure` | Command returns error | `ask_mentor` |
| `askOnTimeout` | Step exceeds time limit | `ask_mentor` |

---

### User Story 7 - Mentor Agent Consultation (Priority: P1)

As a SMILE orchestrator, I want to run the Mentor agent when Student is stuck so that the Mentor can research and provide helpful notes.

**Why this priority**: The Mentor bridges the gap between Student's constraints and the real world, enabling progressive improvement.

**Independent Test**: Can be tested by simulating stuck scenarios and verifying Mentor provides helpful, non-completing notes.

**Acceptance Scenarios**:

1. **Given** Student outputs `ask_mentor`, **When** Mentor is invoked, **Then** Mentor receives: the tutorial, Student's question, what Student tried, and error details
2. **Given** Mentor has broader research capabilities, **When** Mentor investigates, **Then** Mentor outputs text notes explaining the solution or missing context
3. **Given** Mentor cannot find an answer, **When** it exhausts options, **Then** Mentor outputs notes indicating the blocker is unresolvable

#### Mentor Capabilities

| Capability | Description |
|------------|-------------|
| Web search | Can search for documentation, Stack Overflow answers, etc. |
| Official docs | Can access language/framework documentation |
| Repo files | Can read other files in the tutorial's repository |
| No file modification | Cannot create, edit, or delete files |
| No task completion | Cannot execute commands on behalf of Student |

---

### User Story 8 - Loop Orchestration (Priority: P1)

As a SMILE orchestrator, I want to manage the Student-Mentor loop so that iterations continue until completion or termination.

**Why this priority**: The orchestrator coordinates everything - without proper loop management, the system cannot function.

**Independent Test**: Can be tested by verifying all termination conditions and state transitions.

**Acceptance Scenarios**:

1. **Given** Student completes the tutorial, **When** the loop checks conditions, **Then** the loop exits with success status
2. **Given** `maxIterations` is 10, **When** iteration 10 completes without success, **Then** the loop exits with "max iterations" status
3. **Given** Student or Mentor reports unresolvable blocker, **When** the loop checks conditions, **Then** the loop exits with "blocker" status

#### Loop State Machine

```
                    ┌─────────────────────────────────────────┐
                    │                                         │
                    ▼                                         │
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐     │
│  START  │───▶│ STUDENT │───▶│ CHECK   │───▶│ MENTOR  │─────┘
└─────────┘    └─────────┘    │ STATUS  │    └─────────┘
                              └────┬────┘
                                   │
                    ┌──────────────┼──────────────┐
                    ▼              ▼              ▼
              ┌──────────┐  ┌──────────┐  ┌──────────┐
              │ COMPLETE │  │ MAX ITER │  │ BLOCKER  │
              └──────────┘  └──────────┘  └──────────┘
```

#### Orchestrator HTTP API

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/student/result` | POST | Wrapper reports Student output |
| `/api/mentor/result` | POST | Wrapper reports Mentor output |
| `/api/status` | GET | Get current loop status |
| `/api/stop` | POST | Force stop the loop |

#### API Request/Response Schemas

**POST /api/student/result**
```json
Request:  {"studentOutput": StudentOutput, "timestamp": "ISO8601"}
Response: {"acknowledged": true, "nextAction": "continue|stop"}
```

**POST /api/mentor/result**
```json
Request:  {"mentorOutput": "string", "timestamp": "ISO8601"}
Response: {"acknowledged": true, "nextAction": "continue|stop"}
```

**GET /api/status**
```json
Response: LoopState (200 OK) or {"error": "message"} (503 if unhealthy)
```

**POST /api/stop**
```json
Request:  {"reason": "string"}
Response: {"stopped": true, "finalState": LoopState}
```

#### Error Severity Levels

| Severity | Description | Behavior |
|----------|-------------|----------|
| `fatal` | Unrecoverable (Docker unavailable, config invalid) | Exit loop immediately |
| `transient` | Temporary (API rate limit, network timeout) | Retry with exponential backoff |
| `instructional` | Tutorial-related (step failed, command error) | Log and escalate to Mentor |

#### Timeout Semantics

- **Global timeout**: Applies to entire loop; if exceeded, immediately exit with "timeout" status
- **Step timeout**: Applies per Student/Mentor invocation; if exceeded, trigger next state transition
- These are checked independently; whichever expires first takes precedence

#### State Persistence

- Loop state is persisted to `stateFile` after each iteration
- On crash, user can resume by running SMILE in same directory (auto-detects state file)
- Resume kills orphaned container and continues from last completed iteration
- State file is deleted on successful completion or explicit stop

---

### User Story 9 - Real-time Observation (Priority: P2)

As a tutorial author, I want to watch the SMILE loop in real-time so that I can observe progress and understand what's happening.

**Why this priority**: Real-time observation is valuable but not essential for core functionality - reports provide the same information post-hoc.

**Independent Test**: Can be tested by connecting to WebSocket during a loop and verifying all events are received.

**Acceptance Scenarios**:

1. **Given** SMILE loop is running, **When** user connects to WebSocket endpoint, **Then** they receive current loop state immediately
2. **Given** user is connected via WebSocket, **When** a new iteration begins, **Then** user receives update with iteration number and status
3. **Given** user is connected via WebSocket, **When** loop completes, **Then** user receives final status and summary

#### WebSocket Events

| Event | Payload | Description |
|-------|---------|-------------|
| `connected` | `{state}` | Initial state on connection |
| `iteration_start` | `{iteration, timestamp}` | New iteration beginning |
| `student_output` | `{status, summary, currentStep}` | Student result (summarized) |
| `mentor_output` | `{notes}` | Mentor notes |
| `loop_complete` | `{status, summary, iterations}` | Loop finished |
| `error` | `{message}` | Error occurred |

---

### User Story 10 - Report Generation (Priority: P1)

As a tutorial author, I want to receive a comprehensive report after SMILE completes so that I can understand what gaps exist and how to fix them.

**Why this priority**: The report is the primary deliverable - it's what authors use to improve their tutorials.

**Independent Test**: Can be tested by running loops with known gaps and verifying reports accurately document them.

**Acceptance Scenarios**:

1. **Given** the loop exits, **When** SMILE generates the report, **Then** report is written to configured output location
2. **Given** the loop ran for N iterations, **When** report is generated, **Then** it includes chronological timeline of all actions
3. **Given** Mentor provided notes during the loop, **When** report is generated, **Then** it includes each gap with tutorial location and suggested fix

#### Report Structure (Markdown)

```markdown
# SMILE Loop Report: {tutorial-name}

## Summary
- **Status**: {completed | max_iterations | blocker | timeout}
- **Iterations**: {N}
- **Duration**: {time}
- **Tutorial**: {path}

## Gaps Identified

### Gap 1: {title}
- **Location**: Line {N} - "{quote from tutorial}"
- **Problem**: {what went wrong}
- **Suggested Fix**: {what should be added/changed}

## Timeline
[Chronological iteration history]

## Audit Trail
[Commands, files, LLM calls]

## Recommendations
[Prioritized improvement suggestions]
```

#### Output Files

| File | Format | Purpose |
|------|--------|---------|
| `smile-report.md` | Markdown | Human-readable report |
| `smile-report.json` | JSON | Programmatic consumption |
| `smile-audit.log` | Text | Raw execution log |

---

### Edge Cases

| ID | Scenario | Handling |
|----|----------|----------|
| E01 | No smile.json | Use all defaults |
| E02 | Invalid JSON syntax | Exit with parse error |
| E03 | Unknown config fields | Ignore (forward compatibility) |
| E04 | Tutorial file missing | Exit with "not found" error |
| E05 | Tutorial > 100KB | Exit with size limit error |
| E06 | Non-UTF-8 encoding | Exit with encoding error |
| E07 | Missing referenced image | Student judgment (warn or report blocker) |
| E08 | Docker not available | Exit with descriptive error |
| E09 | Image not found | Exit with "pull image first" suggestion |
| E10 | LLM CLI not available | Exit with error listing missing CLI |
| E11 | LLM API error (rate limit, auth) | Retry with backoff; if persists, unresolvable |
| E12 | Student outputs invalid JSON | Wrapper attempts recovery; if fails, log raw output |
| E13 | Wrapper never calls back | Per-iteration timeout triggers error |
| E14 | Multiple concurrent loops | Not supported in v1; reject second attempt |
| E15 | No observers connected | Loop continues; events not buffered |
| E16 | Report write fails | Retry; if persists, output to stdout |
| E17 | WebSocket client falls behind | Drop oldest events; keep 100-event buffer max |
| E18 | State file locked (concurrent run) | Exit with "loop already running" error |
| E19 | Orchestrator crashes mid-loop | State persisted; user can resume |
| E20 | Student outputs malformed JSON | Attempt truncation recovery; if fails 3x, treat as cannot_complete |

---

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST load configuration from `smile.json` in current directory
- **FR-002**: System MUST use default values when config file is absent
- **FR-003**: System MUST validate config and exit with descriptive errors on invalid input
- **FR-004**: System MUST verify tutorial file exists before proceeding
- **FR-005**: System MUST support Claude, Codex, and Gemini LLM providers via CLI
- **FR-006**: System MUST load markdown tutorials up to 100KB in size
- **FR-007**: System MUST resolve image paths relative to tutorial file location
- **FR-008**: System MUST support multimodal processing of tutorial images (PNG, JPG, GIF, SVG)
- **FR-009**: System MUST pass raw markdown to Student without preprocessing
- **FR-010**: System MUST manage Docker containers for agent execution
- **FR-011**: System MUST reset container between iterations for clean slate
- **FR-012**: System MUST mount tutorial, working directory, and logs volumes
- **FR-013**: System MUST communicate with containers via `host.docker.internal`
- **FR-014**: System MUST support configurable container cleanup (keepOnFailure, keepOnSuccess)
- **FR-015**: Student agent MUST receive tutorial, mentor notes, and behavior settings
- **FR-016**: Student agent MUST output structured JSON with status and context
- **FR-017**: Student agent MUST respect configured constraints (no unauthorized web access)
- **FR-018**: Student agent MUST recognize stuck conditions based on configured triggers
- **FR-019**: Student agent MUST escalate to Mentor when stuck triggers fire
- **FR-020**: Mentor agent MUST receive stuck context and research the problem
- **FR-021**: Mentor agent MUST output text notes without completing tasks for Student
- **FR-022**: Orchestrator MUST manage loop state and iteration progression
- **FR-023**: Orchestrator MUST provide HTTP API for wrapper communication
- **FR-024**: Orchestrator MUST accumulate mentor notes across iterations
- **FR-025**: Orchestrator MUST terminate on completion, max iterations, blocker, or timeout
- **FR-026**: Orchestrator MUST provide WebSocket interface for real-time observation
- **FR-027**: System MUST generate comprehensive report in Markdown and JSON formats
- **FR-028**: Report MUST include timeline, gap analysis, and full audit trail
- **FR-029**: Report MUST provide actionable recommendations for tutorial improvement

### Non-Functional Requirements

- **NFR-001**: HTTP server MUST remain responsive to queries while loop is executing
- **NFR-002**: Long-running operations (Docker, LLM calls) MUST NOT block HTTP handlers
- **NFR-003**: Only one SMILE loop can run per instance; concurrent attempts MUST return error
- **NFR-004**: WebSocket observers MUST receive events in order; slow clients drop oldest events

### Key Entities

- **Configuration**: JSON object containing all settings for a SMILE Loop run. Contains tutorial path, LLM provider, iteration limits, timeouts, and student behavior settings. Loaded from `smile.json` in current directory with all fields optional.

- **Tutorial**: Markdown file containing instructions for the learner. UTF-8 encoded, max 100KB. May contain images referenced by relative paths. No required structure — Student infers steps from prose.

- **Container**: Docker container for isolated agent execution. Based on configurable image. Contains pre-installed LLM CLIs. Reset between iterations for clean slate.

- **Student Agent**: LLM-powered agent that attempts to follow tutorials. Has constrained capabilities. Outputs structured JSON with status, actions taken, and questions.

- **Mentor Agent**: LLM-powered agent that helps stuck Students. Has broader research capabilities. Outputs text notes. Cannot complete tasks for Student.

- **Loop State**: Orchestrator's internal state tracking. Includes iteration number, accumulated mentor notes, history of interactions, and timestamps.

- **Report**: Final output document with gap analysis. Available in Markdown and JSON formats. Contains timeline, gaps, audit trail, and recommendations.

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Configuration loads in under 100ms
- **SC-002**: Tutorial with images loads in under 500ms
- **SC-003**: Container starts and is ready in under 5 seconds
- **SC-004**: Container resets between iterations in under 3 seconds
- **SC-005**: 99%+ of Student outputs are successfully parsed as structured data
- **SC-006**: Patience level observably affects help-seeking behavior
- **SC-007**: Each stuck trigger type activates under expected conditions
- **SC-008**: Mentor notes enable Student to progress in 80%+ of resolvable cases
- **SC-009**: Loop exits correctly on all defined termination conditions
- **SC-010**: Real-time events delivered within 100ms of occurrence
- **SC-011**: Each identified gap includes specific location and suggested fix
- **SC-012**: Complete audit trail captures every command, file, and LLM call
- **SC-013**: Generated reports render correctly in standard Markdown viewers

---

## Assumptions

- Docker is available on the host system
- Users have valid API credentials for their chosen LLM provider
- Tutorials are written in Markdown format
- Host system has network access for LLM API calls
- Container images are pre-built and available locally or pullable

---

## Development Standards

### Code Quality

- **DS-001**: All code MUST pass linting before merge (Rust: clippy, Python: ruff)
- **DS-002**: All code MUST be formatted consistently (Rust: rustfmt, Python: black)
- **DS-003**: Pre-commit hooks MUST enforce linting and formatting

### Build & Automation

- **DS-004**: Common tasks MUST be available via Justfile (build, test, run, lint, format)
- **DS-005**: GitHub Actions MUST run tests on all push and pull request events
- **DS-006**: CI MUST pass before merging to main branch

### Version Control

- **DS-007**: All commits MUST follow Conventional Commits format
- **DS-008**: Version numbers MUST follow Semantic Versioning
- **DS-009**: Releases MUST include auto-generated changelog from commits

---

## Constraints

- **Environment**: First iteration runs locally in Docker containers (no cloud deployment)
- **Scope**: Report generation only; automated PR creation deferred to future version
- **Format**: Markdown tutorials only (no RST, AsciiDoc, Jupyter for v1)
- **Concurrency**: Single loop at a time (no parallel validation)
