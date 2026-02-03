# Bollard Workflow Guide - Visual Reference

Quick visual guide to common Docker container workflows in Rust using bollard.

## Workflow 1: Simple Container Execution

```
┌─────────────────────────────────────────────────────────────┐
│ Connect to Docker                                           │
│ docker = Docker::connect_with_local_defaults()?             │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Create Container                                            │
│ config = Config {                                           │
│     image: Some("alpine:latest".to_string()),              │
│     ..Default::default()                                    │
│ }                                                           │
│ container = docker.create_container(..., config).await?     │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Start Container                                             │
│ docker.start_container(&container.id, None).await?          │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Execute Command                                             │
│ exec = docker.create_exec(...).await?                       │
│ stream = docker.start_exec(&exec.id, ...).await?            │
│ // consume stream...                                        │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Cleanup                                                     │
│ docker.stop_container(&container.id, ...).await?            │
│ docker.remove_container(&container.id, ...).await?          │
└─────────────────────────────────────────────────────────────┘
```

## Workflow 2: Container with Volume Mounts

```
┌─────────────────────────────────────────────────────────────┐
│ Validate Mount Path                                         │
│ path = Path::new(host_path);                                │
│ if !path.exists() { return Err(...); }                      │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Create Mount Specification                                  │
│ mount = Mount {                                             │
│     typ: Some(MountTypeEnum::BIND),                         │
│     source: Some(host_path.to_string()),                    │
│     target: Some(container_path.to_string()),               │
│     read_only: Some(false),                                 │
│     ..Default::default()                                    │
│ }                                                           │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Configure HostConfig                                        │
│ host_config = HostConfig {                                  │
│     mounts: Some(vec![mount]),                              │
│     extra_hosts: Some(vec![...]),                           │
│     ..Default::default()                                    │
│ }                                                           │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ Create Container with Config                                │
│ config = Config {                                           │
│     image: Some(image.to_string()),                         │
│     host_config: Some(host_config),                         │
│     ..Default::default()                                    │
│ }                                                           │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
           [Continue from Simple Workflow]
```

## Workflow 3: Container Reset Pattern

```
┌──────────────────────────────────────────────────────────────┐
│ Check if Container Exists                                    │
│ match docker.inspect_container(name).await {               │
│     Ok(container) => { /* Exists, need to remove */ }       │
│     Err(_) => { /* Doesn't exist, create new */ }           │
│ }                                                            │
└────┬────────────────────────────────────────────┬───────────┘
     │ EXISTS                          DOESN'T EXIST
     ▼                                      │
┌──────────────────────┐                     │
│ Stop Container       │                     │
│ docker.stop_container(id, Some(10))       │
└────┬─────────────────┘                     │
     │                                       │
     ▼                                       │
┌──────────────────────┐                     │
│ Check if Stopped     │                     │
│ let status =         │                     │
│   inspect_container()                      │
└────┬────────────────────────┬─────────────┘
     │ STOPPED       NOT STOPPED
     │ OR ERROR      ERROR
     ▼               ▼
   ┌─────────────────────┐
   │ Force Remove        │
   │ force: true         │
   └────┬────────────────┘
        │
        ▼
┌──────────────────────────────────────────────────────────────┐
│ Create Fresh Container                                       │
│ docker.create_container(...).await?                         │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Start Container                                              │
│ docker.start_container(&container.id, None).await?          │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Verify Container is Running                                  │
│ let status = inspect_container().state.status                │
│ if status != "running" { return Err(...); }                  │
└──────────────────────────────────────────────────────────────┘
```

## Workflow 4: Command Execution with Error Handling

```
┌──────────────────────────────────────────────────────────────┐
│ Create Exec Instance                                         │
│ exec_config = CreateExecOptions {                           │
│     cmd: Some(vec!["echo", "hello"]),                       │
│     attach_stdout: Some(true),                              │
│     attach_stderr: Some(true),                              │
│     ..Default::default()                                    │
│ }                                                            │
│ exec = docker.create_exec(container_id, config).await?      │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Start Exec with Timeout                                      │
│ result = timeout(Duration::from_secs(30),                   │
│     docker.start_exec(&exec.id, ...)                        │
│ ).await                                                      │
└────┬──────────────────────────────────────┬──────────────────┘
     │ SUCCESS                    TIMEOUT
     ▼                               ▼
┌──────────────────────┐    ┌──────────────────────┐
│ Consume Stream       │    │ Return Timeout Error │
│ while stream.next()  │    │                      │
└────┬────────────────┘    └──────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Get Exit Code                                                │
│ inspect = docker.inspect_exec(&exec.id).await?              │
│ exit_code = inspect.exit_code.unwrap_or(-1)                 │
└────┬────────────────────────────────────────────────────────┘
     │
     ├─ Exit Code 0 ────────> Success
     │
     └─ Exit Code != 0 ────> Command Failed Error
```

## Workflow 5: Container Manager (High-Level)

```
┌────────────────────────────────────────────────────────────────┐
│ Initialize ContainerManager                                    │
│ manager = ContainerManager::new().await?                       │
│ [Connects to Docker, verifies connection]                      │
└────┬───────────────────────────────────────────────────────────┘
     │
     ▼
┌────────────────────────────────────────────────────────────────┐
│ Start Tracked Container                                        │
│ manager.start_tracked(name, image, mounts).await?             │
│ [Stores ID in HashMap for later reference]                    │
└────┬───────────────────────────────────────────────────────────┘
     │
     ├─────────────────────────────┬──────────────────────────────┐
     │                             │                              │
     ▼                             ▼                              ▼
┌──────────────┐        ┌──────────────────┐        ┌──────────────┐
│ Execute      │        │ Get Container    │        │ Cleanup All  │
│ Command      │        │ Lifecycle Info   │        │              │
│ manager.     │        │ status, logs     │        │ manager.     │
│ execute(...) │        │ stats, etc.      │        │ cleanup_all()│
└──────────────┘        └──────────────────┘        └──────────────┘
```

## Workflow 6: Host Communication Setup

```
┌──────────────────────────────────────────────────────────────┐
│ Determine Platform                                           │
│ #[cfg(target_os = "linux")] → Need extra_hosts              │
│ #[cfg(target_os = "macos")] → Automatic                     │
│ #[cfg(target_os = "windows")] → Automatic (Docker Desktop)  │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Add extra_hosts Configuration                                │
│ extra_hosts: Some(vec![                                      │
│     "host.docker.internal:host-gateway".to_string(),         │
│ ])                                                           │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Set Environment Variables (Optional)                         │
│ env: Some(vec![                                              │
│     "HOST_SERVICE_URL=http://host.docker.internal:8000"      │
│ ])                                                           │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Create & Start Container                                     │
│ [Standard creation workflow]                                 │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Verify Connectivity (Optional)                               │
│ docker.execute("curl -f http://host.docker.internal:8000")   │
│ Check exit code == 0                                         │
└──────────────────────────────────────────────────────────────┘
```

## Workflow 7: Error Recovery with Retry

```
┌──────────────────────────────────────────────────────────────┐
│ Initialize Attempt Counter                                   │
│ attempt = 0, max_retries = 3                                 │
│ exponential_backoff_ms = 100                                 │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Attempt Operation                                            │
│ match docker.create_container(...).await {                   │
└────┬──────────────────────────────────────┬──────────────────┘
     │ SUCCESS                    FAILURE
     ▼                               ▼
┌──────────────────┐    ┌────────────────────────────┐
│ Return Result    │    │ Check Attempt Counter      │
└──────────────────┘    │ attempt < max_retries?     │
                        └────┬──────────┬────────────┘
                             │ YES      │ NO
                             ▼         ▼
                        ┌─────────┐  ┌─────────────┐
                        │ Calculate│  │ Return Err  │
                        │ Backoff  │  └─────────────┘
                        │ time:    │
                        │ base *   │
                        │ 2^n + jit│
                        └────┬────┘
                             │
                             ▼
                        ┌──────────────────┐
                        │ Sleep + Jitter   │
                        │ tokio::time::     │
                        │ sleep(duration)  │
                        └────┬─────────────┘
                             │
                             ▼
                        ┌──────────────────┐
                        │ attempt += 1     │
                        │ Try Again ───────┐
                        └──────────────────┘│
                                            │
                        [Loop back to Attempt Operation]
```

## Workflow 8: Circuit Breaker Pattern

```
┌─────────────────────────────────────────────────────────────────┐
│ Check Circuit State                                             │
│ failures >= threshold?                                          │
└────┬──────────────────┬──────────────────┬─────────────────────┘
     │ YES              │ YES & EXPIRED    │ NO
     │                  │ COOLDOWN         │
     ▼                  ▼                  ▼
┌──────────┐    ┌──────────────┐    ┌────────────┐
│ OPEN     │    │ HALF-OPEN    │    │ CLOSED     │
│          │    │              │    │            │
│ Reject   │    │ Reset count, │    │ Execute    │
│ request  │    │ test request │    │ request    │
│ instantly│    │              │    │            │
└────┬─────┘    └──────┬───────┘    └──────┬─────┘
     │                 │                   │
     └─────────────────┴───────────────────┘
                       │
                       ▼
                ┌──────────────┐
                │ Request      │
                │ Succeeds?    │
                └────┬─────┬──┘
                     │     │
                     │ NO  │
                     ▼     ▼
                   ┌─────────────┐
                   │ Increment   │
                   │ failure cnt │
                   └─────────────┘
```

## Workflow 9: Stream Consumption (Correct Pattern)

```
┌──────────────────────────────────────────────────────────────┐
│ Start Exec and Get Stream                                    │
│ stream = docker.start_exec(&exec_id, ...).await?             │
│ futures::pin_mut!(stream);  // Pin for iteration              │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Loop Through Stream Messages                                 │
│ while let Some(msg) = stream.next().await {                 │
└────┬──────────────────────────────────────────────┬──────────┘
     │                                              │ STREAM ENDS
     ▼                                              │
┌──────────────────────────────────────────────────┐
│ Match Message Type                               │
│ Ok(Attached { log }) ───> Accumulate output     │
│ Ok(Other) ───────────> Handle/ignore           │
│ Err(e) ───────────────> Return error            │
└────┬────────────────────────────────────────────┘
     │
     ├──────────────────────────────────┐
     ▼                                  ▼
 ┌──────────┐                    ┌──────────────┐
 │ Append   │                    │ Return Error │
 │ to output│                    │ immediately  │
 └────┬─────┘                    └──────────────┘
      │
      └─> [Back to loop]
             ▼
┌──────────────────────────────────────────────────────────────┐
│ Stream Ends (next() returns None)                            │
│ Exit while loop                                              │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Get Exit Code                                                │
│ inspect = docker.inspect_exec(&exec_id).await?              │
│ exit_code = inspect.exit_code.unwrap_or(-1)                 │
└────┬─────────────────────────────────────────────────────────┘
     │
     ▼
┌──────────────────────────────────────────────────────────────┐
│ Return Result                                                │
│ Ok(CommandOutput { stdout: output, exit_code })              │
└──────────────────────────────────────────────────────────────┘
```

## Workflow 10: Test Container Lifecycle

```
┌───────────────────────────────────────────────────────────────┐
│ #[tokio::test] async fn test_operation() -> Result<()> {    │
└─────┬─────────────────────────────────────────────────────────┘
      │
      ▼
┌───────────────────────────────────────────────────────────────┐
│ Create Test Container                                         │
│ let container = TestContainer::start(                        │
│     "test-container",                                        │
│     "alpine:latest",                                         │
│     mounts                                                   │
│ ).await?                                                     │
│ [Container stored in Guard, auto-cleanup on drop]            │
└─────┬─────────────────────────────────────────────────────────┘
      │
      ▼
┌───────────────────────────────────────────────────────────────┐
│ Execute Test Operations                                       │
│ container.execute(vec!["command"]).await?                     │
│ assert_eq!(output.exit_code, 0);                              │
└─────┬─────────────────────────────────────────────────────────┘
      │
      ▼
┌───────────────────────────────────────────────────────────────┐
│ Test Passes or Panics                                         │
└─────┬─────────────────────────────────────────────────────────┘
      │
      ▼
┌───────────────────────────────────────────────────────────────┐
│ TestContainer::drop() is called                               │
│ [Async cleanup spawned in background]                        │
│ - docker.stop_container()                                    │
│ - docker.remove_container(force: true)                       │
└───────────────────────────────────────────────────────────────┘
```

## Decision Tree: Which Pattern to Use?

```
Need to manage containers?
│
├─ Simple one-off operations
│  └─> Use connection directly
│      docker.create_container(...)
│      docker.start_container(...)
│
├─ Multiple related operations
│  └─> Use ContainerManager struct
│      manager.start_tracked(...)
│      manager.execute(...)
│
├─ Production system
│  ├─ Need resilience? ──> Circuit breaker + retry
│  ├─ Need reuse? ───────> Container pool
│  └─ Need monitoring? ──> Metrics + logging
│
└─ Testing
   └─> Use TestContainer guard
       Automatic cleanup on drop
```

## Quick Lookup

| Need | Pattern | File |
|------|---------|------|
| Basic setup | Connection | BOLLARD_BEST_PRACTICES.md |
| Error handling | Custom types | BOLLARD_BEST_PRACTICES.md |
| Volume mounts | Mount validation | BOLLARD_BEST_PRACTICES.md |
| Command execution | Stream consumption | BOLLARD_BEST_PRACTICES.md |
| Container reset | Reset patterns | BOLLARD_BEST_PRACTICES.md |
| Host communication | extra_hosts setup | BOLLARD_BEST_PRACTICES.md |
| Troubleshooting | Issue guide | BOLLARD_QUICK_REFERENCE.md |
| Performance | Optimization | BOLLARD_QUICK_REFERENCE.md |
| Production | Advanced patterns | BOLLARD_ADVANCED_PATTERNS.md |
| Resilience | Retry/circuit breaker | BOLLARD_ADVANCED_PATTERNS.md |
| Code examples | Working implementation | bollard_examples.rs |

