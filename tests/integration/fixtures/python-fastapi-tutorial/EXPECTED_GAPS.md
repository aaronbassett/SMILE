# Expected Gaps in Python FastAPI Tutorial

This document describes the intentional documentation gaps seeded in `tutorial.md` for integration testing purposes.

## Gap 1: Missing Python Version Prerequisite (Critical)

**Location**: Prerequisites section
**Type**: Missing prerequisite
**Description**: The tutorial lists "Basic Python knowledge" but does NOT specify which Python version is required. FastAPI requires Python 3.7+. Users with Python 2.x or older Python 3.x versions will encounter cryptic errors.

**Expected Student Behavior**: Student may fail at Step 2 with `python -m venv venv` if Python is not installed or is too old, or get syntax errors in Step 4 due to f-string or type hint incompatibility.
**Expected Detection**: Student should escalate with "missing prerequisite" or "version mismatch" trigger.

## Gap 2: Platform-Specific Activation Command (Major)

**Location**: Step 2 - Create Virtual Environment
**Type**: Incomplete instruction
**Description**: The tutorial shows `source venv/bin/activate` which only works on Unix-like systems. On Windows, the command should be:
- Windows CMD: `venv\Scripts\activate.bat`
- Windows PowerShell: `venv\Scripts\Activate.ps1`

**Expected Student Behavior**: Windows users cannot proceed past Step 2.
**Expected Detection**: Student should escalate with "command failure" or "platform incompatibility" trigger.

## Gap 3: No Port Conflict Guidance (Major)

**Location**: Step 5 - Run the Server
**Type**: Missing error handling
**Description**: If port 8000 is already in use, uvicorn will fail. The tutorial provides no guidance on:
- How to identify port conflicts
- How to use a different port (`--port 8001`)
- How to find and kill processes using port 8000

**Expected Student Behavior**: Student gets "Address already in use" error.
**Expected Detection**: Student should escalate with "command failure" trigger.

## Gap 4: Missing Curl Installation (Minor)

**Location**: Step 6 - Test the API
**Type**: Missing prerequisite
**Description**: The tutorial uses `curl` without mentioning it as a prerequisite. Not all systems have curl installed by default (especially Windows).

**Expected Student Behavior**: May fail with "command not found: curl".
**Expected Detection**: Student should escalate with "missing dependency" trigger.

## Gap 5: Browser Instructions in CLI Context (Minor)

**Location**: Step 7 - Add Documentation
**Type**: Environment assumption
**Description**: The tutorial instructs users to "visit these URLs in your browser" but if running in a container or headless environment, there is no browser access. Should suggest using curl alternatives:
```bash
curl http://localhost:8000/openapi.json
```

**Expected Student Behavior**: Confusion about how to view docs in non-GUI environment.
**Expected Detection**: May or may not trigger depending on container setup.

## Test Validation Criteria

When running SMILE Loop against this tutorial, the report should identify:

1. **At least 2 critical/major gaps** from the list above
2. **Gaps should reference specific steps** (Step 2, Step 5, Step 6)
3. **Gap descriptions should mention**:
   - Python version requirements
   - Platform-specific commands (Windows vs Unix)
   - Port conflict handling

## Gap Severity Mapping

| Gap | Expected Severity | Rationale |
|-----|------------------|-----------|
| Missing Python version | Critical | May cause cryptic failures |
| Platform activation | Major | Blocks Windows users completely |
| Port conflict | Major | Common error, no guidance |
| Missing curl | Minor | Workarounds easy to find |
| Browser in CLI | Minor | Only affects headless environments |
