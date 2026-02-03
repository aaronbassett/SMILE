# Discovery State: smile-loop

**Updated**: 2026-02-02
**Iteration**: 1
**Phase**: Complete

---

## Problem Understanding

### Problem Statement
Tutorial authors (documentation teams, OSS maintainers, developer advocates) struggle to identify gaps, assumed knowledge, and unclear instructions in their technical tutorials. They typically discover problems only after users complain, submit support tickets, or abandon tutorials entirely. SMILE Loop solves this by simulating a learner with intentionally constrained capabilities, automatically discovering what's missing before real users do.

### Personas
| Persona | Description | Primary Goals |
|---------|-------------|---------------|
| Tutorial Author | Creates technical tutorials (internal docs, OSS, devrel) | Produce clear, complete tutorials that learners can follow successfully |
| Tutorial Editor | Reviews and improves tutorial content | Identify gaps and improve tutorial quality systematically |
| Tutorial Learner | The simulated persona the Student agent embodies | Complete the tutorial using only provided instructions |

### Current State vs. Desired State
**Today (without feature)**: Tutorial authors publish content and wait for feedback. Problems surface through user complaints, support tickets, GitHub issues, or silent abandonment. By the time gaps are discovered, many users have already struggled or given up.

**Tomorrow (with feature)**: Authors run SMILE Loop locally before publishing. The system automatically identifies gaps, missing prerequisites, ambiguous instructions, and assumed knowledge. Authors get a comprehensive report with specific locations and suggested fixes, enabling proactive improvement.

### Constraints
- **Environment**: First iteration runs locally in Docker containers (no cloud deployment yet)
- **Scope**: Report generation only; automated PR creation deferred to future version
- **Format**: Markdown tutorials only (no RST, AsciiDoc, Jupyter for v1)
- **LLM Support**: Must support Claude, OpenAI Codex, and Gemini CLI

---

## Story Landscape

### Story Status Overview
| # | Story | Priority | Status | Confidence | Blocked By |
|---|-------|----------|--------|------------|------------|
| 1 | Configuration Loading | P1 | ✅ In SPEC | 100% | - |
| 2 | Tutorial Ingestion | P1 | ✅ In SPEC | 100% | - |
| 3 | Container Lifecycle | P1 | ✅ In SPEC | 100% | - |
| 4 | Student Agent Execution | P1 | ✅ In SPEC | 100% | 2 |
| 5 | Stuck Detection & Escalation | P1 | ✅ In SPEC | 100% | 4 |
| 6 | Mentor Agent Consultation | P1 | ✅ In SPEC | 100% | 5 |
| 7 | Loop Orchestration | P1 | ✅ In SPEC | 100% | 3, 4, 5, 6 |
| 8 | Real-time Observation | P2 | ✅ In SPEC | 100% | 7 |
| 9 | Report Generation | P1 | ✅ In SPEC | 100% | 7 |

### Story Dependencies
```
[1] Configuration ──┐
                    ├──▶ [7] Loop Orchestration ──▶ [8] Real-time Observation
[2] Tutorial ───────┤              │
       │            │              ▼
       ▼            │         [9] Report Generation
[4] Student ────────┤
       │
       ▼
[5] Stuck Detection
       │
       ▼
[6] Mentor

[3] Container ──────┘
```

### Proto-Stories / Emerging Themes
*All stories graduated to SPEC.md*

---

## Completed Stories Summary

| # | Story | Priority | Completed | Key Decisions | Revision Risk |
|---|-------|----------|-----------|---------------|---------------|
| 1 | Configuration Loading | P1 | 2026-02-02 | JSON config, all defaults, error on invalid | Low |
| 2 | Tutorial Ingestion | P1 | 2026-02-02 | Raw markdown, 100KB limit, multimodal images | Low |
| 3 | Container Lifecycle | P1 | 2026-02-02 | Reset between iterations, host.docker.internal | Low |
| 4 | Student Agent Execution | P1 | 2026-02-02 | Structured JSON output, constrained capabilities | Low |
| 5 | Stuck Detection & Escalation | P1 | 2026-02-02 | Prompt-based, configurable triggers | Low |
| 6 | Mentor Agent Consultation | P1 | 2026-02-02 | Text notes, no task completion, research capabilities | Low |
| 7 | Loop Orchestration | P1 | 2026-02-02 | HTTP API, state machine, mentor notes accumulation | Low |
| 8 | Real-time Observation | P2 | 2026-02-02 | WebSocket interface, event streaming | Low |
| 9 | Report Generation | P1 | 2026-02-02 | Markdown + JSON, gap analysis, audit trail | Low |

*Full stories in SPEC.md*

---

## In-Progress Story Detail

*All stories complete*

---

## Watching List

*No items - specification complete*

---

## Glossary

- **SMILE Loop**: The overall system for validating tutorials through AI-driven testing
- **Student Agent**: LLM-powered agent with constrained capabilities that attempts to follow tutorials
- **Mentor Agent**: LLM-powered agent with broader research capabilities that helps when Student is stuck
- **Iteration**: One complete attempt by Student to follow the tutorial (from start to finish)
- **Mentor Notes**: Accumulated text from Mentor consultations, provided to Student in subsequent iterations
- **Stuck State**: Condition where Student cannot proceed, triggering Mentor consultation
- **Stuck Triggers**: Configurable conditions that cause Student to ask Mentor for help
- **SMILE Orchestrator**: Host application managing the loop, communication, and report generation
- **Wrapper**: Container scripts that run agents and communicate with orchestrator via HTTP
- **Gap**: A problem in the tutorial identified during the loop (missing info, ambiguous instruction, etc.)

---

## Next Actions

*Specification complete. Ready for implementation planning.*

