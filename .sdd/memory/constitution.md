<!--
==============================================================================
SYNC IMPACT REPORT
==============================================================================
Version change: N/A → 1.0.0 (initial creation)
Modified principles: N/A (new constitution)
Added sections:
  - Core Principles (8 principles)
  - Development Workflow (Commits, Releases, Code Review)
  - Governance
Removed sections: N/A
Templates requiring updates:
  - plan-template.md: ✅ Compatible (Constitution Check section will use these principles)
  - spec-template.md: ✅ Compatible (no constitution-specific fields)
  - tasks-template.md: ✅ Compatible (task categories align with principles)
Follow-up TODOs: None
==============================================================================
-->

# SMILE Constitution

## Core Principles

### I. Ship Fast, Fix What Hurts

Build the smallest useful thing, use it yourself, iterate on real pain points. Ignore hypothetical requirements until they become actual problems.

**Rules:**
- New features MUST start as minimal implementations
- Dogfood all functionality before expanding
- Refactor when pain is real, not when it might be

**Rationale:** As a solo maintainer of an indefinite project, your time is precious. Building speculatively leads to maintenance burden on features nobody uses.

### II. Keep It Simple (KISS)

Do the simplest thing that works. If you cannot explain a component in one sentence, it is too complex.

**Rules:**
- Favor obvious solutions over clever ones
- Each component MUST have a single, clear purpose
- Complexity requires explicit justification

**Rationale:** Solo-maintained projects die from accumulated complexity. Future-you is your primary collaborator and deserves simple code.

### III. Modularity

Maintain well-defined boundaries between components. Explicit dependencies. No circular dependencies.

**Rules:**
- Rust and Python components MUST have clear interfaces
- Docker services SHOULD be independently runnable where possible
- Changes to one module MUST NOT cascade through the system

**Rationale:** An indefinite-lifespan project needs the ability to evolve parts independently. Clean boundaries let you replace or rewrite modules as needs change.

### IV. Test What Matters

Focus on catching bugs in critical paths, not coverage metrics. Test real workflows against real environments.

**Rules:**
- Critical paths MUST have automated tests
- Integration tests over unit tests for system behavior
- Skip testing trivial code and stable library wrappers
- Mocks are last resort; test against real implementations when feasible

**Rationale:** Limited time means testing strategically. Cover the paths users depend on; do not waste effort on low-risk code.

### V. Fail Fast & Loud

Crash early with clear context. No silent failures. Every error MUST tell the user what happened and suggest what to do.

**Rules:**
- Errors MUST include context (what was attempted, what failed)
- No swallowing exceptions without explicit reason
- Surface problems immediately rather than propagating bad state

**Rationale:** Developers running this locally need to diagnose issues quickly. Silent failures waste debugging time.

### VI. Human-Readable Errors

Error messages MUST be actionable, not cryptic. Include what failed, why it might have failed, and what to try.

**Rules:**
- Error format: "What failed: context. Try: suggestion."
- No raw stack traces as the only output
- Include relevant values that help diagnose (sanitized)

**Rationale:** As a dev tool, SMILE's error UX directly impacts developer experience. Clear errors reduce support burden.

### VII. README First

Installation, setup, and basic usage MUST be documented before a feature is considered complete.

**Rules:**
- README updated with each user-facing change
- One-liner install command required
- Common use cases documented with examples

**Rationale:** Open-source adoption depends on discoverability and ease of getting started. Undocumented features do not exist.

### VIII. Frictionless Setup

One command to install, one command to run. Minimize prerequisites and configuration.

**Rules:**
- Default configuration MUST work out of the box
- Document all prerequisites clearly
- Provide Docker-based setup as fallback

**Rationale:** Every friction point in setup loses potential users and contributors. Make the first experience smooth.

## Development Workflow

### Commits

- Use Conventional Commits format: `type(scope): subject`
- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`
- Keep commits atomic and focused

### Releases

- Continuous deployment when ready
- Tag releases with semantic versions
- Maintain CHANGELOG for notable changes

### Code Review

- Self-review before merge (solo project)
- CI MUST pass before merge
- Consider contributor PRs carefully for quality and scope

## Governance

This constitution guides development decisions for SMILE. It can be amended as the project evolves, but amendments MUST be documented with rationale.

### Amendment Process

1. Propose change with rationale
2. Update `.sdd/memory/constitution.md`
3. Document in changelog
4. Bump version according to semantic versioning:
   - MAJOR: Backward-incompatible principle removals or redefinitions
   - MINOR: New principle or materially expanded guidance
   - PATCH: Clarifications, wording, non-semantic refinements

### Compliance

- All PRs MUST verify compliance with these principles
- Constitution Check in implementation plans validates alignment
- Complexity violations require explicit justification

**Version**: 1.0.0 | **Ratified**: 2026-02-02 | **Last Amended**: 2026-02-02
