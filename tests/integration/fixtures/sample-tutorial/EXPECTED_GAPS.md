# Expected Gaps in Sample Tutorial

This document describes the intentional documentation gaps seeded in `tutorial.md` for integration testing purposes.

## Gap 1: Missing Prerequisite (Critical)

**Location**: Prerequisites section
**Type**: Missing prerequisite
**Description**: The tutorial lists "Basic command-line familiarity" and "A text editor" as prerequisites, but does NOT mention that Node.js must be installed. Step 2 runs `npm init -y` which will fail without Node.js.

**Expected Student Behavior**: Student should get stuck at Step 2 when `npm` command is not found.
**Expected Detection**: The student should escalate to mentor with a "missing dependency" trigger.

## Gap 2: Ambiguous Instruction (Major)

**Location**: Step 4 - Configure the Executable
**Type**: Ambiguous instruction
**Description**: The instruction says "Update the configuration in your project to make it executable. Add the appropriate settings to your configuration file." This is vague:
- Which configuration file? (package.json? A new config file?)
- What specific settings? (bin field? chmod? Something else?)
- What makes something "executable"?

**Expected Student Behavior**: Student should be confused about what exactly to do.
**Expected Detection**: The student should escalate with an "ambiguous instruction" trigger.

## Gap 3: Missing Intermediate Step (Major)

**Location**: Between Step 4 and Step 5
**Type**: Missing step
**Description**: After the vague Step 4, Step 5 immediately tries to run `./counter.js show`. However:
- The file needs `chmod +x counter.js` to be executable
- The shebang line requires the file to be saved with Unix line endings
- These steps are never shown

**Expected Student Behavior**: Student gets "Permission denied" or similar error.
**Expected Detection**: The student should escalate with a "command failure" trigger.

## Gap 4: Environment-Specific Assumption (Minor)

**Location**: Step 3 - Create the Counter Script
**Type**: Environment assumption
**Description**: The script uses `process.env.HOME` to determine where to store the counter file. This:
- Assumes a Unix-like environment (HOME is not always set on Windows)
- Assumes the user wants the counter stored in their home directory
- May fail in container environments where HOME is not set

**Expected Student Behavior**: May fail in non-standard environments.
**Expected Detection**: Depending on the container setup, this may or may not trigger an error.

## Test Validation Criteria

When running SMILE Loop against this tutorial, the report should identify:

1. **At least 2 critical/major gaps** from the list above
2. **Gaps should reference specific steps** (Step 2, Step 4, Step 5)
3. **Gap descriptions should mention**:
   - Missing Node.js/npm prerequisite
   - Unclear "configuration" instructions
   - Missing chmod/permission step

## Gap Severity Mapping

| Gap | Expected Severity | Rationale |
|-----|------------------|-----------|
| Missing Node.js | Critical | Tutorial cannot proceed at all |
| Ambiguous config | Major | Blocks progress, requires guessing |
| Missing chmod | Major | Blocks execution of created script |
| HOME assumption | Minor | Only fails in specific environments |
