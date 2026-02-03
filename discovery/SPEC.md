# Feature Specification: smile-loop

**Feature Branch**: `feature/smile-loop`
**Created**: 2026-02-02
**Last Updated**: 2026-02-02
**Status**: In Progress
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

## User Scenarios & Testing

<!--
  Stories are ordered by priority (P1 first).
  Each story is independently testable and delivers standalone value.
  Stories may be revised if later discovery reveals gaps - see REVISIONS.md
-->

### Story 1: Configuration Loading [P1] [GRADUATED]

**As a** tutorial author
**I want to** configure SMILE Loop via a JSON file
**So that** I can customize behavior for my specific tutorial and environment

#### Acceptance Scenarios

**Scenario 1.1: Load explicit config**
- Given a `smile.json` exists in the current directory
- When SMILE starts
- Then it loads and validates all settings from that file

**Scenario 1.2: Run with defaults**
- Given no `smile.json` exists
- When SMILE starts
- Then it uses all default values (tutorial.md, claude, maxIterations=10, etc.)

**Scenario 1.3: Invalid config**
- Given a `smile.json` with invalid JSON or unknown fields
- When SMILE attempts to load it
- Then it exits with a descriptive error message

**Scenario 1.4: Missing tutorial file**
- Given config specifies a tutorial path that doesn't exist
- When SMILE validates config
- Then it exits with error "Tutorial not found: {path}"

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

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 1.E1 | No smile.json | Use all defaults |
| 1.E2 | Invalid JSON syntax | Exit with parse error |
| 1.E3 | Unknown fields | Ignore (forward compatibility) |
| 1.E4 | Tutorial file missing | Exit with "not found" error |
| 1.E5 | Invalid enum value | Exit with validation error |
| 1.E6 | Negative integer | Exit with validation error |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 1.S1 | Config loads quickly | < 100ms |
| 1.S2 | Actionable error messages | All validation errors explain what's wrong and how to fix |
| 1.S3 | Forward compatible | Unknown fields silently ignored |

---

### Story 2: Tutorial Ingestion [P1] [GRADUATED]

**As a** tutorial author
**I want to** SMILE to load my markdown tutorial
**So that** the Student agent can understand and follow the instructions

#### Acceptance Scenarios

**Scenario 2.1: Load valid tutorial**
- Given a markdown file exists at the configured path
- When SMILE loads the tutorial
- Then the raw markdown content is available for the Student agent

**Scenario 2.2: Load tutorial with images**
- Given a markdown file contains image references (`![alt](path/to/image.png)`)
- When SMILE loads the tutorial
- Then images are resolved from paths relative to the tutorial file and available for multimodal processing

**Scenario 2.3: Tutorial too large**
- Given a markdown file exceeds 100KB
- When SMILE attempts to load it
- Then it exits with error "Tutorial exceeds size limit (100KB)"

**Scenario 2.4: Invalid encoding**
- Given a file is not valid UTF-8
- When SMILE attempts to load it
- Then it exits with error describing the encoding issue

**Scenario 2.5: Missing image**
- Given a tutorial references an image that doesn't exist
- When Student processes the tutorial
- Then Student uses judgment (continue with warning or report blocker)

#### Specification

| Aspect | Value |
|--------|-------|
| Max file size | 100KB |
| Encoding | UTF-8 only |
| Image formats | PNG, JPG, GIF, SVG |
| Image paths | Relative to tutorial file location |
| Preprocessing | None — raw markdown passed to Student |
| Frontmatter | Ignored by Student |
| Structure parsing | None — Student infers steps from prose |

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 2.E1 | File > 100KB | Exit with size limit error |
| 2.E2 | Non-UTF-8 encoding | Exit with encoding error |
| 2.E3 | Missing referenced image | Student judgment (warn or report blocker) |
| 2.E4 | Empty file | Student judgment (report can't complete) |
| 2.E5 | Binary file (not text) | Exit with "not a valid markdown file" error |
| 2.E6 | Frontmatter present | Student ignores it |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 2.S1 | Fast loading | Tutorial + images loaded in < 500ms |
| 2.S2 | Accurate image resolution | All relative paths correctly resolved |
| 2.S3 | Clear error messages | Size/encoding errors explain the issue |

---

### Story 3: Container Lifecycle [P1] [GRADUATED]

**As a** SMILE orchestrator
**I want to** manage Docker containers for agent execution
**So that** Student and Mentor run in isolated, reproducible environments

#### Acceptance Scenarios

**Scenario 3.1: Start container for loop**
- Given a valid configuration and tutorial
- When SMILE begins a loop
- Then it starts a container from the configured image with volumes mounted

**Scenario 3.2: Reset between iterations**
- Given an iteration completes (Student asks Mentor or completes tutorial)
- When the next iteration begins
- Then the container is reset to a clean slate (fastest mechanism available)

**Scenario 3.3: Container communicates with host**
- Given the container is running
- When wrappers need to call the orchestrator
- Then they reach it via `host.docker.internal`

**Scenario 3.4: Keep container on failure (configured)**
- Given `container.keepOnFailure` is true
- When the loop exits due to failure/max iterations
- Then the container is preserved for debugging

**Scenario 3.5: Cleanup container on success**
- Given `container.keepOnSuccess` is false
- When the loop completes successfully
- Then the container is destroyed but mounted volumes persist

**Scenario 3.6: Docker not available**
- Given Docker daemon is not running or not installed
- When SMILE attempts to start
- Then it exits with error "Docker is required but not available"

#### Configuration Schema (additions)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `container.keepOnFailure` | boolean | `true` | Preserve container on failure for debugging |
| `container.keepOnSuccess` | boolean | `false` | Preserve container on success |

#### Base Image Contents

The `smile-base:latest` image includes:
- Common language runtimes (Python, Node.js, Go, Rust, etc.)
- LLM CLI tools (claude, codex CLI, gemini CLI)
- Student and Mentor wrapper scripts
- Standard development tools (git, curl, etc.)

#### Volume Mount Structure

| Host Path | Container Path | Purpose |
|-----------|----------------|---------|
| `{tutorial-dir}/` | `/workspace/tutorial/` | Tutorial file + images (read-only) |
| `{work-dir}/` | `/workspace/work/` | Student working directory |
| `{logs-dir}/` | `/workspace/logs/` | Output logs and artifacts |

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 3.E1 | Docker not available | Exit with descriptive error |
| 3.E2 | Image not found | Exit with "pull image first" suggestion |
| 3.E3 | Container fails to start | Exit with Docker error details |
| 3.E4 | Reset fails mid-loop | Attempt recovery; if fails, treat as unresolvable blocker |
| 3.E5 | Host.docker.internal unavailable | Exit with networking error |
| 3.E6 | Disk full (volume mount fails) | Exit with disk space error |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 3.S1 | Fast startup | Container ready in < 5 seconds |
| 3.S2 | Fast reset | Container reset between iterations in < 3 seconds |
| 3.S3 | Reliable communication | Host reachable from container 100% of the time |
| 3.S4 | Clean isolation | No state leaks between iterations |

---

### Story 4: Student Agent Execution [P1] [GRADUATED]

**As a** SMILE orchestrator
**I want to** run the Student agent with the tutorial and mentor notes
**So that** it attempts to follow the tutorial and reports stuck points

#### Acceptance Scenarios

**Scenario 4.1: Student receives correct inputs**
- Given an iteration begins
- When the Student agent is invoked
- Then it receives: raw tutorial markdown, accumulated mentor notes (if any), and behavior settings

**Scenario 4.2: Student completes tutorial**
- Given the Student successfully follows all tutorial steps
- When it finishes
- Then it outputs JSON with `status: "completed"` and a summary of actions taken

**Scenario 4.3: Student asks for help**
- Given the Student encounters a problem matching configured stuck triggers
- When it decides to ask for help
- Then it outputs JSON with `status: "ask_mentor"`, what it tried, and a specific question

**Scenario 4.4: Student reports unresolvable blocker**
- Given the Student encounters something it cannot resolve (e.g., paid service required)
- When it determines it cannot continue
- Then it outputs JSON with `status: "cannot_complete"` and the reason

**Scenario 4.5: Student respects constraints**
- Given the Student is configured with limited capabilities
- When it follows the tutorial
- Then it does not search the web or access resources beyond what the tutorial provides (unless tutorial instructs it to)

#### Student Prompt Structure

```
[System Prompt]
You are a student learning from a tutorial. Follow the instructions exactly.
You have limited patience and should ask for help when stuck.

Constraints:
- Do not search the web unless the tutorial explicitly tells you to
- Do not assume knowledge the tutorial doesn't provide
- Treat the tutorial as your only source of truth
- Execute all commands in /workspace/work/

Behavior settings:
- Ask for help after {maxRetriesBeforeHelp} failed attempts
- Patience level: {patienceLevel}
[Additional behavior toggles translated to instructions]

[Tutorial Content]
{raw markdown}

[Mentor Notes from Previous Iterations]
{accumulated notes, if any}

[Instruction]
Begin following the tutorial. Output your result as JSON.
```

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

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 4.E1 | LLM CLI not available | Exit with error listing missing CLI |
| 4.E2 | LLM API error (rate limit, auth) | Retry with backoff; if persists, treat as unresolvable |
| 4.E3 | Student outputs invalid JSON | Wrapper attempts recovery; if fails, log raw output and treat as error |
| 4.E4 | Student runs indefinitely | Per-step timeout (configured) terminates and triggers ask_mentor |
| 4.E5 | Tutorial has no executable steps | Student reports completed with note "no actions required" |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 4.S1 | Correct constraint enforcement | Student never accesses resources beyond tutorial without instruction |
| 4.S2 | Reliable output parsing | 99%+ of Student outputs successfully parsed as JSON |
| 4.S3 | Behavior alignment | Patience level observably affects help-seeking behavior |

---

### Story 5: Stuck Detection & Escalation [P1] [GRADUATED]

**As a** Student agent
**I want to** recognize when I'm stuck based on configured triggers
**So that** I ask the Mentor for help at the right time

#### Acceptance Scenarios

**Scenario 5.1: Stuck after max retries**
- Given `maxRetriesBeforeHelp` is 3
- When Student fails the same step 3 times
- Then Student outputs `ask_mentor` status

**Scenario 5.2: Stuck on missing dependency**
- Given `askOnMissingDependency` is true
- When Student encounters "command not found" or similar
- Then Student outputs `ask_mentor` with the missing dependency noted

**Scenario 5.3: Stuck on ambiguous instruction**
- Given `askOnAmbiguousInstruction` is true
- When Student cannot interpret what the tutorial is asking
- Then Student outputs `ask_mentor` with the confusing instruction quoted

**Scenario 5.4: Stuck on command failure**
- Given `askOnCommandFailure` is true
- When a command returns an error after retries
- Then Student outputs `ask_mentor` with the error details

**Scenario 5.5: Stuck on timeout**
- Given `askOnTimeout` is true and `timeoutSeconds` is 60
- When a step takes longer than 60 seconds
- Then Student outputs `ask_mentor` noting the timeout

**Scenario 5.6: Trigger disabled**
- Given `askOnMissingDependency` is false
- When Student encounters a missing dependency
- Then Student attempts to resolve it independently or reports `cannot_complete`

#### Trigger Configuration Matrix

| Trigger | When Fires | Student Action |
|---------|------------|----------------|
| `maxRetriesBeforeHelp` | N consecutive failures on same step | `ask_mentor` |
| `askOnMissingDependency` | Package/command not found | `ask_mentor` |
| `askOnAmbiguousInstruction` | Cannot parse intent | `ask_mentor` |
| `askOnCommandFailure` | Command returns error | `ask_mentor` |
| `askOnTimeout` | Step exceeds time limit | `ask_mentor` |

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 5.E1 | Multiple triggers fire simultaneously | Report the most specific trigger |
| 5.E2 | Trigger fires but Student has context to continue | Student uses judgment; may continue if confident |
| 5.E3 | All triggers disabled | Student either completes or reports `cannot_complete` |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 5.S1 | Triggers fire correctly | Each trigger type activates under expected conditions |
| 5.S2 | Configurable behavior | Disabling a trigger prevents that escalation type |
| 5.S3 | Clear escalation reasons | Every `ask_mentor` includes specific trigger and context |

---

### Story 6: Mentor Agent Consultation [P1] [GRADUATED]

**As a** SMILE orchestrator
**I want to** run the Mentor agent when Student is stuck
**So that** the Mentor can research and provide helpful notes

#### Acceptance Scenarios

**Scenario 6.1: Mentor receives stuck context**
- Given Student outputs `ask_mentor`
- When Mentor is invoked
- Then Mentor receives: the tutorial, Student's question, what Student tried, and the error details

**Scenario 6.2: Mentor researches and responds**
- Given Mentor has broader research capabilities
- When Mentor investigates the problem
- Then Mentor outputs text notes explaining the solution or missing context

**Scenario 6.3: Mentor cannot find answer**
- Given Mentor exhausts research options
- When it cannot find a solution
- Then Mentor outputs notes indicating the blocker is unresolvable

**Scenario 6.4: Mentor notes accumulated**
- Given this is iteration N > 1
- When Mentor provides new notes
- Then the notes are appended to previous notes for the next Student iteration

#### Mentor Prompt Structure

```
[System Prompt]
You are a mentor helping a student who is stuck on a tutorial.
You have access to web search, documentation, and related files.
Your job is to provide the missing context so the student can continue.

Do NOT:
- Complete the task for the student
- Provide full code solutions
- Modify any files

DO:
- Explain what concept or step the student is missing
- Point to relevant documentation or resources
- Clarify ambiguous instructions
- Suggest what the student should try next

[Tutorial Content]
{raw markdown}

[Student's Question]
{questionForMentor}

[What Student Tried]
{attemptedActions}

[Error Details]
{problem}

[Instruction]
Provide helpful notes for the student. Be concise but complete.
```

#### Mentor Output Format

Plain text notes (not JSON) that will be appended to mentor notes for next iteration.

Example:
```
The tutorial assumes you have PostgreSQL installed. The error "psql: command not found"
means PostgreSQL client tools are not in your PATH.

To fix this:
1. The tutorial should have mentioned installing PostgreSQL first
2. On Ubuntu, you would run: sudo apt install postgresql-client

This is a gap in the tutorial - it doesn't mention this prerequisite.
```

#### Mentor Capabilities

| Capability | Description |
|------------|-------------|
| Web search | Can search for documentation, Stack Overflow answers, etc. |
| Official docs | Can access language/framework documentation |
| Repo files | Can read other files in the tutorial's repository |
| No file modification | Cannot create, edit, or delete files |
| No task completion | Cannot execute commands on behalf of Student |

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 6.E1 | Mentor API error | Retry with backoff; if persists, treat as unresolvable |
| 6.E2 | Mentor outputs very long response | Truncate to reasonable limit (e.g., 2000 tokens) |
| 6.E3 | Mentor cannot find any information | Output notes stating the blocker appears unresolvable |
| 6.E4 | Mentor suggests installing paid service | Note this as potential blocker for Student to report |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 6.S1 | Helpful notes | Mentor notes enable Student to progress in 80%+ of cases |
| 6.S2 | No task completion | Mentor never directly solves the problem for Student |
| 6.S3 | Research utilized | Mentor uses available research capabilities when needed |

---

### Story 7: Loop Orchestration [P1] [GRADUATED]

**As a** SMILE orchestrator
**I want to** manage the Student-Mentor loop
**So that** iterations continue until completion or termination

#### Acceptance Scenarios

**Scenario 7.1: Successful completion**
- Given Student completes the tutorial
- When the loop checks termination conditions
- Then the loop exits with success status

**Scenario 7.2: Max iterations reached**
- Given `maxIterations` is 10
- When iteration 10 completes without success
- Then the loop exits with "max iterations" status and partial progress

**Scenario 7.3: Unresolvable blocker**
- Given Student or Mentor reports unresolvable blocker
- When the loop checks termination conditions
- Then the loop exits with "blocker" status and the reason

**Scenario 7.4: Timeout reached**
- Given `timeout` is 1800 seconds
- When total elapsed time exceeds 1800 seconds
- Then the loop exits with "timeout" status and partial progress

**Scenario 7.5: Mentor notes accumulated**
- Given iteration N Student asks Mentor
- When iteration N+1 begins
- Then Student receives all mentor notes from iterations 1 through N

**Scenario 7.6: HTTP API for wrapper communication**
- Given wrappers call orchestrator via HTTP
- When a wrapper reports Student/Mentor output
- Then orchestrator processes and continues the loop

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

#### Loop State Schema

```json
{
  "iteration": 3,
  "status": "running" | "completed" | "max_iterations" | "blocker" | "timeout",
  "startTime": "2026-02-02T10:00:00Z",
  "mentorNotes": ["note from iteration 1", "note from iteration 2"],
  "history": [
    {
      "iteration": 1,
      "studentResult": {...},
      "mentorResult": "..."
    }
  ],
  "finalResult": null | {...}
}
```

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 7.E1 | Wrapper never calls back | Per-iteration timeout triggers; treat as error |
| 7.E2 | Orchestrator crashes mid-loop | State persisted to disk; can resume |
| 7.E3 | Multiple concurrent loops | Not supported in v1; reject second loop attempt |
| 7.E4 | Student completes on iteration 1 | Success; loop exits immediately |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 7.S1 | Correct termination | Loop exits on all defined conditions |
| 7.S2 | State consistency | Loop state accurately reflects history |
| 7.S3 | Reliable communication | Wrapper-orchestrator communication 99.9% reliable |

---

### Story 8: Real-time Observation [P2] [GRADUATED]

**As a** tutorial author
**I want to** watch the SMILE loop in real-time
**So that** I can observe progress and understand what's happening

#### Acceptance Scenarios

**Scenario 8.1: Connect to WebSocket**
- Given SMILE loop is running
- When user connects to WebSocket endpoint
- Then they receive current loop state immediately

**Scenario 8.2: Receive iteration updates**
- Given user is connected via WebSocket
- When a new iteration begins
- Then user receives update with iteration number and status

**Scenario 8.3: Receive Student output**
- Given user is connected via WebSocket
- When Student produces output
- Then user receives the output (possibly summarized)

**Scenario 8.4: Receive Mentor notes**
- Given user is connected via WebSocket
- When Mentor provides notes
- Then user receives the notes

**Scenario 8.5: Receive completion**
- Given user is connected via WebSocket
- When loop completes (any exit condition)
- Then user receives final status and summary

#### WebSocket Events

| Event | Payload | Description |
|-------|---------|-------------|
| `connected` | `{state}` | Initial state on connection |
| `iteration_start` | `{iteration, timestamp}` | New iteration beginning |
| `student_output` | `{status, summary, currentStep}` | Student result (summarized) |
| `mentor_output` | `{notes}` | Mentor notes |
| `loop_complete` | `{status, summary, iterations}` | Loop finished |
| `error` | `{message}` | Error occurred |

#### WebSocket Endpoint

`ws://localhost:{port}/ws/observe`

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 8.E1 | No observers connected | Loop continues; events not buffered |
| 8.E2 | Observer disconnects | No impact on loop; reconnection gets current state |
| 8.E3 | Multiple observers | All receive same events |
| 8.E4 | High-frequency updates | Throttle to max 10 events/second |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 8.S1 | Low latency | Events delivered within 100ms of occurrence |
| 8.S2 | Complete visibility | All significant loop events are observable |
| 8.S3 | No impact on loop | Observation does not slow down execution |

---

### Story 9: Report Generation [P1] [GRADUATED]

**As a** tutorial author
**I want to** receive a comprehensive report after SMILE completes
**So that** I can understand what gaps exist and how to fix them

#### Acceptance Scenarios

**Scenario 9.1: Report generated on completion**
- Given the loop exits (any condition)
- When SMILE generates the report
- Then report is written to configured output location

**Scenario 9.2: Report includes timeline**
- Given the loop ran for N iterations
- When report is generated
- Then it includes chronological timeline of all actions

**Scenario 9.3: Report includes gap analysis**
- Given Mentor provided notes during the loop
- When report is generated
- Then it includes each gap identified with tutorial location and suggested fix

**Scenario 9.4: Report in multiple formats**
- Given report generation completes
- When output files are written
- Then both JSON (for programmatic use) and Markdown (for human reading) are produced

**Scenario 9.5: Report includes full audit trail**
- Given everything is logged during execution
- When report is generated
- Then it includes all commands run, files created, LLM calls, and outputs

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

### Gap 2: ...

## Timeline

### Iteration 1
- **Started**: {timestamp}
- **Student Actions**: {summary}
- **Result**: {completed | ask_mentor | cannot_complete}
- **Mentor Notes**: {if applicable}

### Iteration 2
...

## Audit Trail

### Commands Executed
| Iteration | Command | Exit Code | Output Summary |
|-----------|---------|-----------|----------------|

### Files Created
| Iteration | Path | Size |
|-----------|------|------|

### LLM Calls
| Agent | Provider | Tokens In | Tokens Out | Duration |
|-------|----------|-----------|------------|----------|

## Recommendations

1. {Prioritized recommendation based on gaps}
2. ...
```

#### Report Structure (JSON)

```json
{
  "meta": {
    "tutorial": "path/to/tutorial.md",
    "status": "completed",
    "iterations": 3,
    "duration": 245,
    "timestamp": "2026-02-02T10:04:05Z"
  },
  "gaps": [
    {
      "id": 1,
      "location": {"line": 42, "text": "Run the setup script"},
      "problem": "Setup script path not specified",
      "suggestedFix": "Add: 'Run ./scripts/setup.sh from the project root'",
      "severity": "high"
    }
  ],
  "timeline": [...],
  "auditTrail": {
    "commands": [...],
    "files": [...],
    "llmCalls": [...]
  },
  "recommendations": [...]
}
```

#### Output Files

| File | Format | Purpose |
|------|--------|---------|
| `smile-report.md` | Markdown | Human-readable report |
| `smile-report.json` | JSON | Programmatic consumption |
| `smile-audit.log` | Text | Raw execution log |

#### Edge Cases

| ID | Case | Handling |
|----|------|----------|
| 9.E1 | Loop exits with no gaps found | Report indicates success; no gaps section |
| 9.E2 | Very long audit trail | Truncate command outputs; full log in separate file |
| 9.E3 | Report write fails | Retry; if persists, output to stdout |
| 9.E4 | Output directory doesn't exist | Create it |

#### Success Criteria

| ID | Criterion | Measurement |
|----|-----------|-------------|
| 9.S1 | Actionable gaps | Each gap includes specific location and suggested fix |
| 9.S2 | Complete audit trail | Every command, file, and LLM call logged |
| 9.S3 | Parseable JSON | JSON report validates against schema |
| 9.S4 | Readable Markdown | Markdown renders correctly in GitHub/editors |

---

## Edge Cases

| ID | Scenario | Handling | Stories Affected |
|----|----------|----------|------------------|
| 1.E1 | No smile.json | Use all defaults | 1 |
| 1.E2 | Invalid JSON syntax | Exit with parse error | 1 |
| 1.E3 | Unknown fields | Ignore (forward compatibility) | 1 |
| 1.E4 | Tutorial file missing | Exit with "not found" error | 1 |
| 1.E5 | Invalid enum value | Exit with validation error | 1 |
| 1.E6 | Negative integer | Exit with validation error | 1 |
| 2.E1 | File > 100KB | Exit with size limit error | 2 |
| 2.E2 | Non-UTF-8 encoding | Exit with encoding error | 2 |
| 2.E3 | Missing referenced image | Student judgment (warn or report blocker) | 2 |
| 2.E4 | Empty file | Student judgment (report can't complete) | 2 |
| 2.E5 | Binary file (not text) | Exit with "not a valid markdown file" error | 2 |
| 2.E6 | Frontmatter present | Student ignores it | 2 |
| 3.E1 | Docker not available | Exit with descriptive error | 3 |
| 3.E2 | Image not found | Exit with "pull image first" suggestion | 3 |
| 3.E3 | Container fails to start | Exit with Docker error details | 3 |
| 3.E4 | Reset fails mid-loop | Attempt recovery; if fails, treat as unresolvable blocker | 3 |
| 3.E5 | Host.docker.internal unavailable | Exit with networking error | 3 |
| 3.E6 | Disk full (volume mount fails) | Exit with disk space error | 3 |
| 4.E1 | LLM CLI not available | Exit with error listing missing CLI | 4 |
| 4.E2 | LLM API error (rate limit, auth) | Retry with backoff; if persists, treat as unresolvable | 4 |
| 4.E3 | Student outputs invalid JSON | Wrapper attempts recovery; if fails, log raw output | 4 |
| 4.E4 | Student runs indefinitely | Per-step timeout terminates and triggers ask_mentor | 4 |
| 4.E5 | Tutorial has no executable steps | Student reports completed with note | 4 |
| 5.E1 | Multiple triggers fire simultaneously | Report the most specific trigger | 5 |
| 5.E2 | Trigger fires but Student can continue | Student uses judgment | 5 |
| 5.E3 | All triggers disabled | Student completes or reports cannot_complete | 5 |
| 6.E1 | Mentor API error | Retry with backoff; if persists, unresolvable | 6 |
| 6.E2 | Mentor outputs very long response | Truncate to 2000 tokens | 6 |
| 6.E3 | Mentor cannot find information | Note blocker appears unresolvable | 6 |
| 6.E4 | Mentor suggests paid service | Note as potential blocker | 6 |
| 7.E1 | Wrapper never calls back | Per-iteration timeout triggers error | 7 |
| 7.E2 | Orchestrator crashes mid-loop | State persisted; can resume | 7 |
| 7.E3 | Multiple concurrent loops | Not supported; reject second attempt | 7 |
| 7.E4 | Student completes iteration 1 | Success; exit immediately | 7 |
| 8.E1 | No observers connected | Loop continues; events not buffered | 8 |
| 8.E2 | Observer disconnects | No impact; reconnection gets current state | 8 |
| 8.E3 | Multiple observers | All receive same events | 8 |
| 8.E4 | High-frequency updates | Throttle to max 10 events/second | 8 |
| 9.E1 | No gaps found | Report indicates success; no gaps section | 9 |
| 9.E2 | Very long audit trail | Truncate outputs; full log separate | 9 |
| 9.E3 | Report write fails | Retry; if persists, output to stdout | 9 |
| 9.E4 | Output directory missing | Create it | 9 |

---

## Requirements

### Functional Requirements

| ID | Requirement | Stories | Confidence |
|----|-------------|---------|------------|
| FR1 | System shall load configuration from smile.json in current directory | 1 | 100% |
| FR2 | System shall use default values when config file is absent | 1 | 100% |
| FR3 | System shall validate config and exit with descriptive errors on invalid input | 1 | 100% |
| FR4 | System shall verify tutorial file exists before proceeding | 1 | 100% |
| FR5 | System shall support Claude, Codex, and Gemini LLM providers via CLI | 1 | 100% |
| FR6 | System shall load markdown tutorial up to 100KB in size | 2 | 100% |
| FR7 | System shall resolve image paths relative to tutorial file location | 2 | 100% |
| FR8 | System shall support multimodal processing of tutorial images (PNG, JPG, GIF, SVG) | 2 | 100% |
| FR9 | System shall pass raw markdown to Student without preprocessing | 2 | 100% |
| FR10 | System shall manage Docker containers for agent execution | 3 | 100% |
| FR11 | System shall reset container between iterations for clean slate | 3 | 100% |
| FR12 | System shall mount tutorial, working directory, and logs volumes | 3 | 100% |
| FR13 | System shall communicate with containers via host.docker.internal | 3 | 100% |
| FR14 | System shall support configurable container cleanup (keepOnFailure, keepOnSuccess) | 3 | 100% |
| FR15 | Student agent shall receive tutorial, mentor notes, and behavior settings | 4 | 100% |
| FR16 | Student agent shall output structured JSON with status and context | 4 | 100% |
| FR17 | Student agent shall respect configured constraints (no unauthorized web access) | 4 | 100% |
| FR18 | Student agent shall recognize stuck conditions based on configured triggers | 5 | 100% |
| FR19 | Student agent shall escalate to Mentor when stuck triggers fire | 5 | 100% |
| FR20 | Mentor agent shall receive stuck context and research the problem | 6 | 100% |
| FR21 | Mentor agent shall output text notes without completing tasks for Student | 6 | 100% |
| FR22 | Orchestrator shall manage loop state and iteration progression | 7 | 100% |
| FR23 | Orchestrator shall provide HTTP API for wrapper communication | 7 | 100% |
| FR24 | Orchestrator shall accumulate mentor notes across iterations | 7 | 100% |
| FR25 | Orchestrator shall terminate on completion, max iterations, blocker, or timeout | 7 | 100% |
| FR26 | Orchestrator shall provide WebSocket interface for real-time observation | 8 | 100% |
| FR27 | System shall generate comprehensive report in Markdown and JSON formats | 9 | 100% |
| FR28 | Report shall include timeline, gap analysis, and full audit trail | 9 | 100% |
| FR29 | Report shall provide actionable recommendations for tutorial improvement | 9 | 100% |

### Key Entities

**Configuration**: JSON object containing all settings for a SMILE Loop run
- Contains tutorial path, LLM provider, iteration limits, timeouts, and student behavior settings
- Loaded from `smile.json` in current directory
- All fields optional with sensible defaults

**Tutorial**: Markdown file containing instructions for the learner
- UTF-8 encoded, max 100KB
- May contain images referenced by relative paths
- No required structure — Student infers steps from prose
- Frontmatter (if present) is ignored

**Container**: Docker container for isolated agent execution
- Based on `smile-base:latest` image (configurable)
- Contains pre-installed LLM CLIs (claude, codex, gemini)
- Contains Student and Mentor wrapper scripts
- Reset between iterations for clean slate
- Communicates with host orchestrator via `host.docker.internal`

**Student Agent**: LLM-powered agent that attempts to follow tutorials
- Receives tutorial, mentor notes, and behavior settings
- Has constrained capabilities (limited web access, no assumed knowledge)
- Outputs structured JSON with status, actions taken, and questions
- Runs continuously until complete, stuck, or cannot continue

**Mentor Agent**: LLM-powered agent that helps stuck Students
- Receives tutorial, student's question, and error context
- Has broader research capabilities (web, docs, repo files)
- Outputs text notes explaining missing context
- Cannot complete tasks for Student or modify files

**Loop State**: Orchestrator's internal state tracking
- Current iteration number
- Accumulated mentor notes
- History of all student/mentor interactions
- Timestamps and elapsed time

**Report**: Final output document with gap analysis
- Available in Markdown (human) and JSON (programmatic) formats
- Contains timeline, gaps, audit trail, and recommendations
- Each gap includes location, problem, and suggested fix

---

## Success Criteria

| ID | Criterion | Measurement | Stories |
|----|-----------|-------------|---------|
| 1.S1 | Config loads quickly | < 100ms | 1 |
| 1.S2 | Actionable error messages | All validation errors explain what's wrong and how to fix | 1 |
| 1.S3 | Forward compatible | Unknown fields silently ignored | 1 |
| 2.S1 | Fast loading | Tutorial + images loaded in < 500ms | 2 |
| 2.S2 | Accurate image resolution | All relative paths correctly resolved | 2 |
| 2.S3 | Clear error messages | Size/encoding errors explain the issue | 2 |
| 3.S1 | Fast startup | Container ready in < 5 seconds | 3 |
| 3.S2 | Fast reset | Container reset between iterations in < 3 seconds | 3 |
| 3.S3 | Reliable communication | Host reachable from container 100% of the time | 3 |
| 3.S4 | Clean isolation | No state leaks between iterations | 3 |
| 4.S1 | Correct constraint enforcement | Student never accesses unauthorized resources | 4 |
| 4.S2 | Reliable output parsing | 99%+ of Student outputs parsed as JSON | 4 |
| 4.S3 | Behavior alignment | Patience level affects help-seeking behavior | 4 |
| 5.S1 | Triggers fire correctly | Each trigger activates under expected conditions | 5 |
| 5.S2 | Configurable behavior | Disabling trigger prevents that escalation | 5 |
| 5.S3 | Clear escalation reasons | Every ask_mentor includes trigger and context | 5 |
| 6.S1 | Helpful notes | Mentor notes enable progress 80%+ of cases | 6 |
| 6.S2 | No task completion | Mentor never solves problems for Student | 6 |
| 6.S3 | Research utilized | Mentor uses research capabilities when needed | 6 |
| 7.S1 | Correct termination | Loop exits on all defined conditions | 7 |
| 7.S2 | State consistency | Loop state accurately reflects history | 7 |
| 7.S3 | Reliable communication | Wrapper-orchestrator 99.9% reliable | 7 |
| 8.S1 | Low latency | Events delivered within 100ms | 8 |
| 8.S2 | Complete visibility | All significant loop events observable | 8 |
| 8.S3 | No impact on loop | Observation doesn't slow execution | 8 |
| 9.S1 | Actionable gaps | Each gap has location and suggested fix | 9 |
| 9.S2 | Complete audit trail | Every command, file, LLM call logged | 9 |
| 9.S3 | Parseable JSON | JSON validates against schema | 9 |
| 9.S4 | Readable Markdown | Markdown renders correctly | 9 |

---

## Appendix: Story Revision History

*Major revisions to graduated stories. Full details in `archive/REVISIONS.md`*

| Date | Story | Change | Reason |
|------|-------|--------|--------|
| *No revisions yet* | - | - | - |
