# Expected Gaps in Docker Setup Tutorial

This document describes the intentional documentation gaps seeded in `tutorial.md` for integration testing purposes.

## Gap 1: Docker Not Installed (Critical)

**Location**: Prerequisites & Step 1
**Type**: Missing prerequisite
**Description**: The prerequisites mention "64-bit operating system" and "administrative access" but never mention that Docker must be installed first. Step 1 asks to verify Docker installation, which implies it should already exist, but there's no link or instructions for installing Docker.

For reference, installation varies by platform:
- macOS: Download Docker Desktop from docker.com
- Windows: Docker Desktop or WSL2 + Docker
- Ubuntu: `apt-get install docker.io` or Docker's official repo
- Other Linux: varies by distribution

**Expected Student Behavior**: Student runs `docker --version` and gets "command not found".
**Expected Detection**: Student should escalate with "missing_dependency" trigger.

## Gap 2: Docker Daemon Not Running (Critical)

**Location**: Step 1 through Step 6
**Type**: Missing troubleshooting
**Description**: Even when Docker is installed, the daemon might not be running. Common scenarios:
- Docker Desktop not started (macOS/Windows)
- dockerd service not running (Linux)
- Permission issues (Linux user not in docker group)

Errors look like:
- "Cannot connect to the Docker daemon"
- "permission denied while trying to connect to the Docker daemon socket"

The tutorial provides no troubleshooting for these common issues.

**Expected Student Behavior**: Student gets "Cannot connect to Docker daemon" error.
**Expected Detection**: Student should escalate with "command_failure" trigger.

## Gap 3: Missing Docker Hub Authentication (Major)

**Location**: Step 2 - Pull Your First Image
**Type**: Missing information
**Description**: While `hello-world` is public and doesn't require authentication, many images do. The tutorial doesn't mention:
- Docker Hub rate limits for anonymous pulls
- How to authenticate with `docker login`
- What to do if you hit rate limits

**Expected Student Behavior**: May hit rate limits or confusion with private images later.
**Expected Detection**: May not trigger with hello-world specifically, but would with other images.

## Gap 4: Linux Permission Requirements (Major)

**Location**: Step 3 onwards
**Type**: Platform-specific gap
**Description**: On Linux, running docker commands requires either:
- Running as root (using `sudo`)
- Adding user to the `docker` group: `sudo usermod -aG docker $USER`

Without this, users get "permission denied" on every docker command. The tutorial mentions "administrative access" but doesn't explain this Linux-specific requirement.

**Expected Student Behavior**: Linux users get "permission denied" errors.
**Expected Detection**: Student should escalate with "permission_error" trigger.

## Gap 5: Network Requirements Not Mentioned (Minor)

**Location**: Step 2 - Pull Your First Image
**Type**: Environment assumption
**Description**: `docker pull` requires internet connectivity. In restricted environments (corporate firewalls, air-gapped systems), this will fail. The tutorial doesn't mention:
- Network requirements
- Proxy configuration (`HTTP_PROXY`, `HTTPS_PROXY`)
- Offline alternatives

**Expected Student Behavior**: May fail with timeout or connection refused in restricted networks.
**Expected Detection**: Depends on environment, may trigger "network_error".

## Test Validation Criteria

When running SMILE Loop against this tutorial, the report should identify:

1. **At least 2 critical gaps** related to Docker installation/daemon
2. **Gaps should reference specific steps** (Prerequisites, Step 1, Step 2)
3. **Gap descriptions should mention**:
   - Docker installation missing
   - Docker daemon not running
   - Permission issues on Linux

## Gap Severity Mapping

| Gap | Expected Severity | Rationale |
|-----|------------------|-----------|
| Docker not installed | Critical | Tutorial cannot start without Docker |
| Daemon not running | Critical | Confusing error, common problem |
| Docker Hub auth | Major | Rate limits increasingly common |
| Linux permissions | Major | Blocks all Linux users without sudo |
| Network requirements | Minor | Only affects restricted environments |
