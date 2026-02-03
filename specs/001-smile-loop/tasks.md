# Implementation Tasks: SMILE Loop

**Feature**: 001-smile-loop | **Generated**: 2026-02-02
**Plan**: [plan.md](./plan.md) | **Spec**: [spec.md](./spec.md)

---

## Summary

| Metric | Value |
|--------|-------|
| Total Tasks | 98 |
| Setup Tasks | 8 |
| Foundational Tasks | 14 |
| User Story Tasks | 68 |
| Polish Tasks | 8 |
| Parallel Opportunities | 32 |

### User Story Task Breakdown

| Story | Priority | Tasks | Description |
|-------|----------|-------|-------------|
| US2 | P1 | 8 | Configure Validation Behavior |
| US3 | P1 | 6 | Load Tutorial Content |
| US4 | P1 | 8 | Isolated Execution Environment |
| US5+6 | P1 | 10 | Student Agent + Stuck Detection |
| US7 | P1 | 8 | Mentor Agent Consultation |
| US8 | P1 | 10 | Loop Orchestration |
| US10 | P1 | 8 | Report Generation |
| US1 | P1 | 6 | Run Tutorial Validation (Integration) |
| US9 | P2 | 4 | Real-time Observation |

### MVP Scope

**Recommended MVP**: Complete through US8 (Loop Orchestration)
- Core loop functional: config → container → student → mentor → repeat
- Can validate tutorials end-to-end
- Report generation (US10) can follow as fast-follow

---

## Phase 1: Setup

Project initialization and scaffolding.

### Phase Start
- [x] T001 [GIT] Verify on main branch and working tree is clean
- [x] T002 [GIT] Pull latest changes from origin/main
- [x] T003 [GIT] Create feature branch: 001-smile-loop

### Implementation
- [x] T004 Verify Rust workspace compiles with `cargo check` (use devs:rust-dev agent)
- [x] T005 [GIT] Commit: verify workspace compiles
- [x] T006 Install Python dev dependencies with `pip install -e ".[dev]"` in python/
- [x] T007 [GIT] Commit: verify python package setup
- [x] T008 Install lefthook and verify hooks with `lefthook install && lefthook run pre-commit`
- [x] T009 [GIT] Commit: verify git hooks work
- [x] T010 Create docker/Dockerfile.base with Ubuntu base, Python, and placeholder for LLM CLIs
- [x] T011 [GIT] Commit: add base Dockerfile

### Phase Completion
- [x] T012 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [x] T013 [GIT] Create/update PR to main with phase summary
- [x] T014 [GIT] Verify all CI checks pass
- [x] T015 [GIT] Report PR ready status

---

## Phase 2: Foundational

Shared infrastructure required by all user stories.

### Phase Start
- [x] T016 [GIT] Verify working tree is clean before starting Phase 2
- [x] T017 [GIT] Pull and rebase on origin/main if needed
- [x] T018 Create retro/P2.md for this phase
- [x] T019 [GIT] Commit: initialize phase 2 retro

### Implementation

#### Error Types (shared across crates)
- [x] T020 [P] Create SmileError enum with variants in crates/smile-orchestrator/src/error.rs (use devs:rust-dev agent)
- [x] T021 [GIT] Commit: add error types

#### Configuration (US2 foundation)
- [x] T022 [P] Implement Config struct with serde defaults in crates/smile-orchestrator/src/config.rs (use devs:rust-dev agent)
- [x] T023 [P] Implement StudentBehavior struct in crates/smile-orchestrator/src/config.rs (use devs:rust-dev agent)
- [x] T024 [P] Implement LlmProvider and PatienceLevel enums in crates/smile-orchestrator/src/config.rs (use devs:rust-dev agent)
- [x] T025 [GIT] Commit: add configuration types

#### Python Output Models (shared by wrappers)
- [x] T026 [P] Implement StudentOutput pydantic model in python/smile_wrappers/output.py (use devs:python-expert agent)
- [x] T027 [P] Implement Config pydantic model in python/smile_wrappers/config.py (use devs:python-expert agent)
- [x] T028 [GIT] Commit: add Python output and config models

#### CLI Skeleton
- [x] T029 Implement CLI arg parsing with clap in crates/smile-cli/src/main.rs (use devs:rust-dev agent)
- [x] T030 [GIT] Commit: add CLI skeleton

### Phase Completion
- [x] T031 Run /sdd:map incremental for Phase 2 changes (skipped - not configured)
- [x] T032 [GIT] Commit: update codebase documents for phase 2
- [x] T033 Review retro/P2.md and extract critical learnings to CLAUDE.md (conservative)
- [x] T034 [GIT] Commit: finalize phase 2 retro
- [x] T035 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [x] T036 [GIT] Create/update PR to main with phase summary
- [x] T037 [GIT] Verify all CI checks pass
- [x] T038 [GIT] Report PR ready status

---

## Phase 3: User Story 2 - Configure Validation Behavior [US2]

**Goal**: Load and validate smile.json configuration with defaults
**Independent Test**: Create configs with different settings, verify behavior changes

### Phase Start
- [x] T039 [GIT] Verify working tree is clean before starting Phase 3
- [x] T040 [GIT] Pull and rebase on origin/main if needed
- [x] T041 [US2] Create retro/P3.md for this phase
- [x] T042 [GIT] Commit: initialize phase 3 retro

### Implementation
- [x] T043 [US2] Implement config loading from smile.json in crates/smile-orchestrator/src/config.rs (use devs:rust-dev agent)
- [x] T044 [GIT] Commit: implement config file loading
- [x] T045 [US2] Implement config validation with descriptive errors in crates/smile-orchestrator/src/config.rs (use devs:rust-dev agent)
- [x] T046 [GIT] Commit: add config validation
- [x] T047 [US2] Implement default values for all config fields in crates/smile-orchestrator/src/config.rs (use devs:rust-dev agent) [done in Phase 2]
- [x] T048 [GIT] Commit: add config defaults [done in Phase 2]
- [x] T049 [US2] Add config loading integration in CLI in crates/smile-cli/src/main.rs (use devs:rust-dev agent)
- [x] T050 [GIT] Commit: integrate config loading in CLI

### Phase Completion
- [x] T051 [US2] Run /sdd:map incremental for Phase 3 changes (skipped - not configured)
- [x] T052 [GIT] Commit: update codebase documents for phase 3 (skipped - not configured)
- [x] T053 [US2] Review retro/P3.md and extract critical learnings to CLAUDE.md (conservative)
- [x] T054 [GIT] Commit: finalize phase 3 retro
- [x] T055 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [x] T056 [GIT] Create/update PR to main with phase summary (PR #1 exists, manual edit needed due to token scope)
- [x] T057 [GIT] Verify all CI checks pass (31 tests pass locally, pre-push hooks pass)
- [x] T058 [GIT] Report PR ready status

---

## Phase 4: User Story 3 - Load Tutorial Content [US3]

**Goal**: Load markdown tutorials with images, enforce size limits
**Independent Test**: Load tutorials with various formats, sizes, and image references

### Phase Start
- [x] T059 [GIT] Verify working tree is clean before starting Phase 4
- [x] T060 [GIT] Pull and rebase on origin/main if needed
- [x] T061 [US3] Create retro/P4.md for this phase
- [x] T062 [GIT] Commit: initialize phase 4 retro

### Implementation
- [x] T063 [US3] Create Tutorial struct in crates/smile-orchestrator/src/tutorial.rs (use devs:rust-dev agent)
- [x] T064 [GIT] Commit: add Tutorial types
- [x] T065 [US3] Implement tutorial loading with size validation (100KB limit) in crates/smile-orchestrator/src/tutorial.rs (use devs:rust-dev agent)
- [x] T066 [GIT] Commit: implement tutorial loading with size limit
- [x] T067 [US3] Implement image reference extraction and resolution in crates/smile-orchestrator/src/tutorial.rs (use devs:rust-dev agent)
- [x] T068 [GIT] Commit: add image extraction and resolution
- [x] T069 [US3] Add tutorial loading to CLI flow in crates/smile-cli/src/main.rs (use devs:rust-dev agent)
- [x] T070 [GIT] Commit: integrate tutorial loading in CLI

### Phase Completion
- [x] T071 [US3] Run /sdd:map incremental for Phase 4 changes (skipped - not configured)
- [x] T072 [GIT] Commit: update codebase documents for phase 4 (skipped - not configured)
- [x] T073 [US3] Review retro/P4.md and extract critical learnings to CLAUDE.md (conservative)
- [x] T074 [GIT] Commit: finalize phase 4 retro
- [x] T075 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [x] T076 [GIT] Create/update PR to main with phase summary (PR #1 exists, manual edit needed due to token scope)
- [x] T077 [GIT] Verify all CI checks pass (52 tests pass locally, pre-push hooks pass)
- [x] T078 [GIT] Report PR ready status

---

## Phase 5: User Story 4 - Isolated Execution Environment [US4]

**Goal**: Manage Docker containers with volume mounts and reset capability
**Independent Test**: Verify container starts, resets between iterations, cleans up properly

### Phase Start
- [x] T079 [GIT] Verify working tree is clean before starting Phase 5
- [x] T080 [GIT] Pull and rebase on origin/main if needed
- [x] T081 [US4] Create retro/P5.md for this phase
- [x] T082 [GIT] Commit: initialize phase 5 retro

### Implementation
- [x] T083 [US4] Create Container and ContainerStatus types in crates/smile-container/src/lib.rs (use devs:rust-dev agent)
- [x] T084 [GIT] Commit: add container types
- [x] T085 [US4] Implement Docker connection and health check via bollard in crates/smile-container/src/manager.rs (use devs:rust-dev agent)
- [x] T086 [GIT] Commit: implement Docker connection
- [x] T087 [US4] Implement container creation with volume mounts in crates/smile-container/src/manager.rs (use devs:rust-dev agent)
- [x] T088 [GIT] Commit: add container creation with mounts
- [x] T089 [US4] Implement container start/stop/remove lifecycle in crates/smile-container/src/manager.rs (use devs:rust-dev agent)
- [x] T090 [GIT] Commit: implement container lifecycle
- [x] T091 [US4] Implement container reset (stop, remove, recreate) in crates/smile-container/src/manager.rs (use devs:rust-dev agent)
- [x] T092 [GIT] Commit: add container reset

### Phase Completion
- [x] T093 [US4] Run /sdd:map incremental for Phase 5 changes (skipped - not configured)
- [x] T094 [GIT] Commit: update codebase documents for phase 5 (skipped - not configured)
- [x] T095 [US4] Review retro/P5.md and extract critical learnings to CLAUDE.md (conservative)
- [x] T096 [GIT] Commit: finalize phase 5 retro
- [x] T097 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [x] T098 [GIT] Create/update PR to main with phase summary (PR #1 exists, manual edit needed due to token scope)
- [x] T099 [GIT] Verify all CI checks pass (74 tests pass locally, pre-push hooks pass)
- [x] T100 [GIT] Report PR ready status

---

## Phase 6: User Stories 5+6 - Student Agent + Stuck Detection [US5][US6]

**Goal**: Student agent follows tutorial, detects stuck conditions, escalates to Mentor
**Independent Test**: Run Student with tutorials of varying quality, verify appropriate outputs

### Phase Start
- [x] T101 [GIT] Verify working tree is clean before starting Phase 6
- [x] T102 [GIT] Pull and rebase on origin/main if needed
- [x] T103 [US5] Create retro/P6.md for this phase
- [x] T104 [GIT] Commit: initialize phase 6 retro

### Implementation
- [x] T105 [P] [US5] Create prompt construction module in python/smile_wrappers/prompts.py (use devs:python-expert agent)
- [x] T106 [GIT] Commit: add prompt construction
- [x] T107 [US5] Implement Student wrapper with LLM CLI invocation in python/smile_wrappers/student.py (use devs:python-expert agent)
- [x] T108 [GIT] Commit: implement student wrapper
- [x] T109 [US6] Implement stuck detection logic based on triggers in python/smile_wrappers/student.py (use devs:python-expert agent)
- [ ] T110 [GIT] Commit: add stuck detection
- [ ] T111 [US5] Implement structured output parsing with recovery in python/smile_wrappers/output.py (use devs:python-expert agent)
- [ ] T112 [GIT] Commit: add output parsing with recovery
- [ ] T113 [US5] Implement HTTP callback to orchestrator in python/smile_wrappers/student.py (use devs:python-expert agent)
- [ ] T114 [GIT] Commit: add orchestrator callback

### Phase Completion
- [ ] T115 [US5] Run /sdd:map incremental for Phase 6 changes
- [ ] T116 [GIT] Commit: update codebase documents for phase 6
- [ ] T117 [US5] Review retro/P6.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T118 [GIT] Commit: finalize phase 6 retro
- [ ] T119 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T120 [GIT] Create/update PR to main with phase summary
- [ ] T121 [GIT] Verify all CI checks pass
- [ ] T122 [GIT] Report PR ready status

---

## Phase 7: User Story 7 - Mentor Agent Consultation [US7]

**Goal**: Mentor agent receives stuck context, researches problem, provides notes
**Independent Test**: Simulate stuck scenarios, verify Mentor provides helpful non-completing notes

### Phase Start
- [ ] T123 [GIT] Verify working tree is clean before starting Phase 7
- [ ] T124 [GIT] Pull and rebase on origin/main if needed
- [ ] T125 [US7] Create retro/P7.md for this phase
- [ ] T126 [GIT] Commit: initialize phase 7 retro

### Implementation
- [ ] T127 [US7] Implement Mentor prompt construction in python/smile_wrappers/prompts.py (use devs:python-expert agent)
- [ ] T128 [GIT] Commit: add mentor prompt construction
- [ ] T129 [US7] Implement Mentor wrapper with LLM CLI invocation in python/smile_wrappers/mentor.py (use devs:python-expert agent)
- [ ] T130 [GIT] Commit: implement mentor wrapper
- [ ] T131 [US7] Implement Mentor output handling (text notes) in python/smile_wrappers/mentor.py (use devs:python-expert agent)
- [ ] T132 [GIT] Commit: add mentor output handling
- [ ] T133 [US7] Implement HTTP callback to orchestrator in python/smile_wrappers/mentor.py (use devs:python-expert agent)
- [ ] T134 [GIT] Commit: add mentor orchestrator callback

### Phase Completion
- [ ] T135 [US7] Run /sdd:map incremental for Phase 7 changes
- [ ] T136 [GIT] Commit: update codebase documents for phase 7
- [ ] T137 [US7] Review retro/P7.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T138 [GIT] Commit: finalize phase 7 retro
- [ ] T139 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T140 [GIT] Create/update PR to main with phase summary
- [ ] T141 [GIT] Verify all CI checks pass
- [ ] T142 [GIT] Report PR ready status

---

## Phase 8: User Story 8 - Loop Orchestration [US8]

**Goal**: Manage Student-Mentor loop with state machine, HTTP API, termination conditions
**Independent Test**: Verify all termination conditions and state transitions

### Phase Start
- [ ] T143 [GIT] Verify working tree is clean before starting Phase 8
- [ ] T144 [GIT] Pull and rebase on origin/main if needed
- [ ] T145 [US8] Create retro/P8.md for this phase
- [ ] T146 [GIT] Commit: initialize phase 8 retro

### Implementation
- [ ] T147 [US8] Create LoopState and LoopStatus types in crates/smile-orchestrator/src/loop_state.rs (use devs:rust-dev agent)
- [ ] T148 [GIT] Commit: add loop state types
- [ ] T149 [US8] Implement loop state machine with transitions in crates/smile-orchestrator/src/loop_state.rs (use devs:rust-dev agent)
- [ ] T150 [GIT] Commit: implement state machine
- [ ] T151 [P] [US8] Implement HTTP API endpoints per contracts/orchestrator-api.yaml in crates/smile-orchestrator/src/api.rs (use devs:rust-dev agent)
- [ ] T152 [GIT] Commit: add HTTP API endpoints
- [ ] T153 [US8] Implement state persistence to JSON file in crates/smile-orchestrator/src/loop_state.rs (use devs:rust-dev agent)
- [ ] T154 [GIT] Commit: add state persistence
- [ ] T155 [US8] Implement termination conditions (max iterations, timeout, blocker) in crates/smile-orchestrator/src/loop_state.rs (use devs:rust-dev agent)
- [ ] T156 [GIT] Commit: add termination conditions
- [ ] T157 [US8] Integrate loop into CLI with container management in crates/smile-cli/src/main.rs (use devs:rust-dev agent)
- [ ] T158 [GIT] Commit: integrate loop in CLI

### Phase Completion
- [ ] T159 [US8] Run /sdd:map incremental for Phase 8 changes
- [ ] T160 [GIT] Commit: update codebase documents for phase 8
- [ ] T161 [US8] Review retro/P8.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T162 [GIT] Commit: finalize phase 8 retro
- [ ] T163 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T164 [GIT] Create/update PR to main with phase summary
- [ ] T165 [GIT] Verify all CI checks pass
- [ ] T166 [GIT] Report PR ready status

---

## Phase 9: User Story 10 - Report Generation [US10]

**Goal**: Generate comprehensive Markdown and JSON reports after loop completion
**Independent Test**: Run loops with known gaps, verify reports accurately document them

### Phase Start
- [ ] T167 [GIT] Verify working tree is clean before starting Phase 9
- [ ] T168 [GIT] Pull and rebase on origin/main if needed
- [ ] T169 [US10] Create retro/P9.md for this phase
- [ ] T170 [GIT] Commit: initialize phase 9 retro

### Implementation
- [ ] T171 [US10] Create Report, Gap, and ReportSummary types in crates/smile-report/src/lib.rs (use devs:rust-dev agent)
- [ ] T172 [GIT] Commit: add report types
- [ ] T173 [P] [US10] Implement Markdown report generation in crates/smile-report/src/markdown.rs (use devs:rust-dev agent)
- [ ] T174 [P] [US10] Implement JSON report generation in crates/smile-report/src/json.rs (use devs:rust-dev agent)
- [ ] T175 [GIT] Commit: add report generation
- [ ] T176 [US10] Implement gap extraction from loop history in crates/smile-report/src/lib.rs (use devs:rust-dev agent)
- [ ] T177 [GIT] Commit: add gap extraction
- [ ] T178 [US10] Integrate report generation into CLI on loop completion in crates/smile-cli/src/main.rs (use devs:rust-dev agent)
- [ ] T179 [GIT] Commit: integrate report generation

### Phase Completion
- [ ] T180 [US10] Run /sdd:map incremental for Phase 9 changes
- [ ] T181 [GIT] Commit: update codebase documents for phase 9
- [ ] T182 [US10] Review retro/P9.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T183 [GIT] Commit: finalize phase 9 retro
- [ ] T184 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T185 [GIT] Create/update PR to main with phase summary
- [ ] T186 [GIT] Verify all CI checks pass
- [ ] T187 [GIT] Report PR ready status

---

## Phase 10: User Story 1 - Run Tutorial Validation (Integration) [US1]

**Goal**: Full end-to-end integration: run SMILE against tutorial, get report
**Independent Test**: Run against sample tutorial with intentional gaps, verify report identifies gaps

### Phase Start
- [ ] T188 [GIT] Verify working tree is clean before starting Phase 10
- [ ] T189 [GIT] Pull and rebase on origin/main if needed
- [ ] T190 [US1] Create retro/P10.md for this phase
- [ ] T191 [GIT] Commit: initialize phase 10 retro

### Implementation
- [ ] T192 [US1] Create tests/integration/fixtures/sample-tutorial/ with intentional gaps
- [ ] T193 [GIT] Commit: add sample tutorial fixture
- [ ] T194 [US1] Finalize Dockerfile.base with LLM CLI installation in docker/Dockerfile.base
- [ ] T195 [GIT] Commit: finalize base Dockerfile
- [ ] T196 [US1] Implement end-to-end integration test in tests/integration/test_loop.rs (use devs:rust-dev agent)
- [ ] T197 [GIT] Commit: add end-to-end integration test

### Phase Completion
- [ ] T198 [US1] Run /sdd:map incremental for Phase 10 changes
- [ ] T199 [GIT] Commit: update codebase documents for phase 10
- [ ] T200 [US1] Review retro/P10.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T201 [GIT] Commit: finalize phase 10 retro
- [ ] T202 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T203 [GIT] Create/update PR to main with phase summary
- [ ] T204 [GIT] Verify all CI checks pass
- [ ] T205 [GIT] Report PR ready status

---

## Phase 11: User Story 9 - Real-time Observation (P2) [US9]

**Goal**: WebSocket interface for real-time loop observation
**Independent Test**: Connect to WebSocket during loop, verify all events received

### Phase Start
- [ ] T206 [GIT] Verify working tree is clean before starting Phase 11
- [ ] T207 [GIT] Pull and rebase on origin/main if needed
- [ ] T208 [US9] Create retro/P11.md for this phase
- [ ] T209 [GIT] Commit: initialize phase 11 retro

### Implementation
- [ ] T210 [US9] Implement WebSocket event types per contracts/websocket-events.yaml in crates/smile-orchestrator/src/websocket.rs (use devs:rust-dev agent)
- [ ] T211 [GIT] Commit: add WebSocket event types
- [ ] T212 [US9] Implement WebSocket server with broadcast in crates/smile-orchestrator/src/websocket.rs (use devs:rust-dev agent)
- [ ] T213 [GIT] Commit: implement WebSocket server
- [ ] T214 [US9] Integrate WebSocket events into loop state machine in crates/smile-orchestrator/src/loop_state.rs (use devs:rust-dev agent)
- [ ] T215 [GIT] Commit: integrate WebSocket into loop

### Phase Completion
- [ ] T216 [US9] Run /sdd:map incremental for Phase 11 changes
- [ ] T217 [GIT] Commit: update codebase documents for phase 11
- [ ] T218 [US9] Review retro/P11.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T219 [GIT] Commit: finalize phase 11 retro
- [ ] T220 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T221 [GIT] Create/update PR to main with phase summary
- [ ] T222 [GIT] Verify all CI checks pass
- [ ] T223 [GIT] Report PR ready status

---

## Phase 12: Polish & Cross-Cutting Concerns

Final cleanup, documentation, and quality improvements.

### Phase Start
- [ ] T224 [GIT] Verify working tree is clean before starting Phase 12
- [ ] T225 [GIT] Pull and rebase on origin/main if needed
- [ ] T226 Create retro/P12.md for this phase
- [ ] T227 [GIT] Commit: initialize phase 12 retro

### Implementation
- [ ] T228 Update README.md with installation and usage instructions
- [ ] T229 [GIT] Commit: update README
- [ ] T230 Review and fix all clippy warnings across crates (use devs:rust-dev agent)
- [ ] T231 [GIT] Commit: fix clippy warnings
- [ ] T232 Review and fix all ruff/mypy issues in python/ (use devs:python-expert agent)
- [ ] T233 [GIT] Commit: fix python linting issues
- [ ] T234 Ensure all edge cases from spec are handled (E01-E20)
- [ ] T235 [GIT] Commit: handle remaining edge cases

### Phase Completion
- [ ] T236 Run /sdd:map incremental for Phase 12 changes
- [ ] T237 [GIT] Commit: update codebase documents for phase 12
- [ ] T238 Review retro/P12.md and extract critical learnings to CLAUDE.md (conservative)
- [ ] T239 [GIT] Commit: finalize phase 12 retro
- [ ] T240 [GIT] Push branch to origin (ensure pre-push hooks pass)
- [ ] T241 [GIT] Create/update PR to main with phase summary
- [ ] T242 [GIT] Verify all CI checks pass
- [ ] T243 [GIT] Report PR ready status

---

## Dependencies

### User Story Completion Order

```
US2 (Config) ─────┐
                  ├──► US4 (Container) ─────┐
US3 (Tutorial) ───┘                         │
                                            ├──► US8 (Loop) ──► US10 (Report) ──► US1 (Integration)
US5+6 (Student) ──► US7 (Mentor) ───────────┘
                                                                                        │
                                                                                        ▼
                                                                                   US9 (WebSocket) [P2]
```

### Parallel Execution Opportunities

**Within Phase 2 (Foundational)**:
- T020, T022, T023, T024 can run in parallel (different files)
- T026, T027 can run in parallel (different Python files)

**Within Phase 5 (Container)**:
- After container types created, manager methods can be parallelized

**Within Phase 6 (Student)**:
- T105 (prompts) can run parallel to other setup

**Within Phase 8 (Loop)**:
- T151 (API endpoints) can run parallel to state machine implementation

**Within Phase 9 (Report)**:
- T173, T174 (Markdown and JSON generation) can run in parallel

---

## Implementation Strategy

### Incremental Delivery

1. **Milestone 1** (Phases 1-4): Config + Tutorial loading
   - Validates basic setup and data ingestion

2. **Milestone 2** (Phases 5-7): Container + Agents
   - Core agent functionality working

3. **Milestone 3** (Phases 8-9): Loop + Report
   - Full validation loop operational

4. **Milestone 4** (Phase 10): Integration
   - End-to-end validation working

5. **Milestone 5** (Phase 11-12): Polish
   - WebSocket, edge cases, documentation

### Risk Mitigation

- **Docker dependency**: Test container management early (Phase 5)
- **LLM CLI variability**: Abstract provider interface in prompts.py
- **State persistence**: Test crash recovery in Phase 8
- **WebSocket complexity**: Defer to Phase 11 (P2), not blocking MVP
