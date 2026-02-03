# Bollard Crate - Quick Reference & Troubleshooting

Quick lookup guide for common bollard usage patterns and solutions to frequent issues.

## Quick Setup

```rust
// Cargo.toml
[dependencies]
bollard = "0.15"
tokio = { version = "1", features = ["full"] }
futures = "0.3"
thiserror = "1.0"
tracing = "0.1"
```

```rust
// Connect to Docker
let docker = Docker::connect_with_local_defaults()?;
```

---

## Common Patterns at a Glance

### Create and Start Container

```rust
let config = Config {
    image: Some("alpine:latest".to_string()),
    ..Default::default()
};

let container = docker.create_container(
    Some(CreateContainerOptions { name: "my-app" }),
    config
).await?;

docker.start_container::<String>(&container.id, None).await?;
```

### Create with Mounts

```rust
let config = Config {
    image: Some("alpine:latest".to_string()),
    host_config: Some(HostConfig {
        mounts: Some(vec![Mount {
            typ: Some(bollard::models::MountTypeEnum::BIND),
            source: Some("/host/path".to_string()),
            target: Some("/container/path".to_string()),
            read_only: Some(false),
            ..Default::default()
        }]),
        ..Default::default()
    }),
    ..Default::default()
};
```

### Execute Command

```rust
let exec = docker.create_exec(
    "container_id",
    CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(vec!["sh", "-c", "echo hello"]),
        ..Default::default()
    }
).await?;

let stream = docker.start_exec(&exec.id, Some(StartExecOptions::default())).await?;
```

### Stop Container

```rust
docker.stop_container("container_id", Some(10)).await?;
```

### Remove Container

```rust
docker.remove_container(
    "container_id",
    Some(RemoveContainerOptions { force: true, ..Default::default() })
).await?;
```

---

## Troubleshooting Guide

### Issue 1: "No such container" Error

**Symptoms:**
```
Error: Docker error: {"message":"No such container: my-container"}
```

**Causes:**
- Container doesn't exist
- Wrong container ID/name
- Container was already removed
- Container failed to create

**Solutions:**

```rust
// DO: Check before operations
if docker.inspect_container("my-container").await.is_ok() {
    // Container exists
}

// DO: Handle missing containers gracefully
match docker.inspect_container(id).await {
    Ok(c) => { /* use it */ },
    Err(e) if e.to_string().contains("No such container") => {
        // Container doesn't exist, create new one
    },
    Err(e) => return Err(e.into()),
}

// DO: Verify container creation succeeded
let container = docker.create_container(Some(opts), config).await?;
println!("Container ID: {}", container.id);
```

---

### Issue 2: Mount Path Doesn't Exist

**Symptoms:**
```
Error: Docker error: {"message":"error while creating mount source path ..."}
```

**Causes:**
- Host path doesn't exist
- Permission issues
- Path is relative instead of absolute

**Solutions:**

```rust
// DO: Validate paths before creating container
fn validate_mount(host_path: &str, container_path: &str) -> Result<()> {
    let path = std::path::Path::new(host_path);
    if !path.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {}", host_path));
    }
    if !container_path.starts_with('/') {
        return Err(anyhow::anyhow!("Container path must be absolute"));
    }
    Ok(())
}

// DO: Create paths if needed
if !Path::new("/host/path").exists() {
    std::fs::create_dir_all("/host/path")?;
}

// DO: Use absolute paths
let mounts = vec![Mount {
    source: Some("/absolute/path".to_string()), // Correct
    target: Some("/container/path".to_string()),
    ..Default::default()
}];
```

---

### Issue 3: Container Starts but Immediately Exits

**Symptoms:**
```
Container status is "exited" seconds after starting
```

**Causes:**
- Application inside container crashed
- Image doesn't have a default command
- Wrong entrypoint/command
- Missing dependencies

**Solutions:**

```rust
// DO: Check exit code
let container = docker.inspect_container("id").await?;
if let Some(state) = container.state {
    println!("Exit code: {:?}", state.exit_code);
}

// DO: Check logs
let logs = docker.logs("container_id", Some(LogsOptions::default())).await?;

// DO: Use command that keeps container alive
let config = Config {
    image: Some("alpine:latest".to_string()),
    entrypoint: Some(vec!["tail".to_string(), "-f".to_string(), "/dev/null".to_string()]),
    ..Default::default()
};

// DO: Test image locally first
docker.create_container(opts, config).await?;
let mut stream = docker.wait_container(id, Some(WaitContainerOptions { condition: "next-exit" })).await?;
while let Some(status) = stream.next().await {
    println!("Wait status: {:?}", status);
}
```

---

### Issue 4: Stream Hangs or Never Completes

**Symptoms:**
```
Program appears to hang when executing commands
Streams from logs/exec never close
```

**Causes:**
- Stream not fully consumed
- No timeout set
- Process inside container is blocked
- Stream error not handled

**Solutions:**

```rust
// DON'T: Just await without loop
let stream = docker.start_exec(&id, Some(opts)).await?;
// stream just sits here

// DO: Consume entire stream with timeout
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(30),
    consume_stream(docker, exec_id)
).await;

async fn consume_stream(docker: &Docker, exec_id: &str) -> Result<String> {
    let stream = docker.start_exec(exec_id, Some(StartExecOptions::default())).await?;
    let mut output = String::new();

    futures::pin_mut!(stream);
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(bollard::exec::StartExecResults::Attached { log }) => {
                output.push_str(&String::from_utf8_lossy(&log));
            }
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    Ok(output)
}
```

---

### Issue 5: Container Already Exists

**Symptoms:**
```
Error: Docker error: {"message":"Conflict. The container name ... is already in use"}
```

**Causes:**
- Container with same name already running
- Container wasn't cleaned up from previous run
- Name conflict with another container

**Solutions:**

```rust
// DO: Remove existing container first
pub async fn start_fresh_container(
    docker: &Docker,
    name: &str,
    config: Config,
) -> Result<String> {
    // Try to remove old one
    if let Ok(c) = docker.inspect_container(name).await {
        if let Some(id) = c.id {
            let _ = docker.stop_container(&id, Some(10)).await;
            let _ = docker.remove_container(&id, Some(
                RemoveContainerOptions { force: true, ..Default::default() }
            )).await;
        }
    }

    // Now safe to create
    let container = docker.create_container(
        Some(CreateContainerOptions { name }),
        config
    ).await?;

    docker.start_container::<String>(&container.id, None).await?;
    Ok(container.id)
}

// DO: Use force flag for removal
docker.remove_container(
    "container_id",
    Some(RemoveContainerOptions { force: true, ..Default::default() })
).await?;
```

---

### Issue 6: host.docker.internal Not Working in Container

**Symptoms:**
```
Inside container: curl: (7) Failed to connect to host.docker.internal
Connection refused or Name does not resolve
```

**Causes:**
- Running on Linux (not automatic like Docker Desktop)
- Network configuration not set up for host access
- Firewall blocking connection

**Solutions:**

```rust
// DO: Add extra_hosts for host.docker.internal
let config = Config {
    image: Some("alpine:latest".to_string()),
    host_config: Some(HostConfig {
        extra_hosts: Some(vec![
            "host.docker.internal:host-gateway".to_string(),
        ]),
        ..Default::default()
    }),
    ..Default::default()
};

// DO: Use environment variable with host IP
let config = Config {
    image: Some("alpine:latest".to_string()),
    env: Some(vec![
        format!("HOST_IP={}", get_host_ip()?),
    ]),
    ..Default::default()
};

// DO: Test connectivity from container
docker.create_exec(
    container_id,
    CreateExecOptions {
        attach_stdout: Some(true),
        cmd: Some(vec!["curl", "-v", "http://host.docker.internal:8000"]),
        ..Default::default()
    }
).await?;

// DO: Check if on Linux and handle differently
#[cfg(target_os = "linux")]
let extra_hosts = Some(vec!["host.docker.internal:host-gateway".to_string()]);

#[cfg(not(target_os = "linux"))]
let extra_hosts = None; // Docker Desktop handles it
```

---

### Issue 7: Permission Denied When Mounting Volumes

**Symptoms:**
```
Permission denied while trying to connect to Docker daemon socket at unix:///var/run/docker.sock
```

**Causes:**
- Running as non-root user without Docker group
- Socket file permissions issue
- SELinux/AppArmor restrictions

**Solutions:**

```bash
# Add user to docker group (requires logout/login)
sudo usermod -aG docker $USER

# Or use sudo
sudo docker ps

# Check socket permissions
ls -la /var/run/docker.sock
```

```rust
// DO: Detect and provide helpful error message
pub async fn connect_docker_safe() -> Result<Docker> {
    match Docker::connect_with_local_defaults() {
        Ok(d) => {
            d.version().await?;
            Ok(d)
        }
        Err(e) => {
            if e.to_string().contains("Permission denied") {
                Err(anyhow::anyhow!(
                    "Permission denied connecting to Docker. \
                    Run: sudo usermod -aG docker $USER"
                ))
            } else {
                Err(anyhow::anyhow!("Failed to connect to Docker: {}", e))
            }
        }
    }
}
```

---

### Issue 8: Command Exit Code Not Captured

**Symptoms:**
```
exec.exit_code is None
Can't determine if command succeeded
```

**Causes:**
- Not inspecting the exec after completion
- Stream closed before checking exit code
- Command output never fully consumed

**Solutions:**

```rust
// DON'T: Check exit code before consuming stream
let stream = docker.start_exec(&exec_id, Some(opts)).await?;
// Don't: immediately call inspect_exec
// let inspect = docker.inspect_exec(&exec_id).await?;

// DO: Fully consume stream, then check exit code
let stream = docker.start_exec(&exec_id, Some(opts)).await?;
let mut output = String::new();

futures::pin_mut!(stream);
while let Some(msg) = stream.next().await {
    if let Ok(bollard::exec::StartExecResults::Attached { log }) = msg {
        output.push_str(&String::from_utf8_lossy(&log));
    }
}

// NOW get exit code
let inspect = docker.inspect_exec(&exec_id).await?;
let exit_code = inspect.exit_code.unwrap_or(-1);
```

---

### Issue 9: Docker Daemon Connection Lost

**Symptoms:**
```
"connection reset by peer"
"broken pipe"
Intermittent connection failures
```

**Causes:**
- Docker daemon restarted
- Network connectivity issue
- Long-running connection timeout

**Solutions:**

```rust
// DO: Implement retry logic with exponential backoff
pub async fn docker_operation_with_retry<F, T>(
    mut operation: F,
    max_retries: u32,
) -> Result<T>
where
    F: FnMut() -> futures::future::BoxFuture<'static, Result<T>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < max_retries => {
                let wait_ms = 100 * 2_u64.pow(attempt);
                warn!("Attempt {} failed: {}. Retrying in {}ms", attempt, e, wait_ms);
                tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

// DO: Verify connection is healthy
let docker = Docker::connect_with_local_defaults()?;
docker.version().await?; // Verify connection works
```

---

### Issue 10: High Memory Usage / Containers Not Cleaned Up

**Symptoms:**
```
Program memory grows over time
Old containers accumulate in docker ps -a
```

**Causes:**
- Streams not fully consumed
- Containers not explicitly removed
- Logs accumulating
- No cleanup on error

**Solutions:**

```rust
// DO: Always clean up containers
pub async fn safe_container_operation<F, T>(
    docker: &Docker,
    name: &str,
    f: F,
) -> Result<T>
where
    F: for<'a> FnOnce(&'a str) -> futures::future::BoxFuture<'a, Result<T>>,
{
    // Create and setup
    let container = create_container(docker, name).await?;

    // Execute operation
    let result = match f(&container.id).await {
        Ok(r) => Ok(r),
        Err(e) => {
            warn!("Operation failed, cleaning up: {}", e);
            Err(e)
        }
    };

    // Always cleanup
    let _ = docker.stop_container(&container.id, Some(10)).await;
    let _ = docker.remove_container(
        &container.id,
        Some(RemoveContainerOptions { force: true, ..Default::default() })
    ).await;

    result
}

// DO: Use guard pattern for cleanup
struct ContainerGuard {
    docker: Arc<Docker>,
    id: String,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let docker = Arc::clone(&self.docker);
        let id = self.id.clone();
        tokio::spawn(async move {
            let _ = docker.remove_container(
                &id,
                Some(RemoveContainerOptions { force: true, ..Default::default() })
            ).await;
        });
    }
}
```

---

## Performance Tips

### 1. Reuse Docker Connection

```rust
// DON'T: Create new connection for each operation
async fn bad_operation() {
    let docker = Docker::connect_with_local_defaults().unwrap();
    // ... use once
}

async fn bad_loop() {
    for i in 0..100 {
        let docker = Docker::connect_with_local_defaults().unwrap(); // Inefficient!
        // ...
    }
}

// DO: Share connection
async fn good_operation(docker: &Docker) {
    // ... use connection
}

let docker = Docker::connect_with_local_defaults()?;
for i in 0..100 {
    good_operation(&docker).await?;
}
```

### 2. Use Named Volumes for Persistence

```rust
// DON'T: Use bind mounts for temporary data
let mount = Mount {
    typ: Some(bollard::models::MountTypeEnum::BIND),
    source: Some("/tmp/data".to_string()),
    target: Some("/app/data".to_string()),
    ..Default::default()
};

// DO: Use named volumes for better performance
let mount = Mount {
    typ: Some(bollard::models::MountTypeEnum::VOLUME),
    source: Some("my-volume".to_string()),
    target: Some("/app/data".to_string()),
    ..Default::default()
};
```

### 3. Limit Output Captured

```rust
// DON'T: Capture unlimited output
let mut output = String::new();
while let Some(msg) = stream.next().await {
    // All output accumulated in memory
}

// DO: Process in chunks or limit size
let mut output = String::with_capacity(1024 * 1024); // 1MB limit
let mut total_bytes = 0;
const MAX_OUTPUT: usize = 10 * 1024 * 1024;

while let Some(msg) = stream.next().await {
    if let Ok(bollard::exec::StartExecResults::Attached { log }) = msg {
        if total_bytes + log.len() < MAX_OUTPUT {
            output.push_str(&String::from_utf8_lossy(&log));
            total_bytes += log.len();
        }
    }
}
```

---

## Testing Checklist

Before deploying code using bollard:

- [ ] Test on actual Docker daemon (not just Docker Desktop)
- [ ] Test with non-existent containers/images
- [ ] Test with permission denied scenarios
- [ ] Test with long-running commands (add timeout)
- [ ] Test stream consumption (don't leave hanging)
- [ ] Test container cleanup in error cases
- [ ] Test on Linux, macOS, and Windows if applicable
- [ ] Test host connectivity with `host.docker.internal`
- [ ] Test with limited resources (memory constraints)
- [ ] Verify no containers left behind after tests

---

## Useful Commands for Debugging

```bash
# List all containers
docker ps -a

# View container logs
docker logs <container_id>

# Execute command in running container
docker exec <container_id> <command>

# Inspect container details
docker inspect <container_id>

# Check Docker daemon connection
docker version

# Monitor resource usage
docker stats

# Clean up resources
docker container prune    # Remove stopped containers
docker volume prune       # Remove unused volumes
docker image prune        # Remove unused images
```

---

## Additional Resources

- **Official Docs**: https://docs.rs/bollard/
- **GitHub**: https://github.com/fussybeaver/bollard
- **Docker API**: https://docs.docker.com/engine/api/
- **Examples**: https://github.com/fussybeaver/bollard/tree/main/examples
- **Tokio Guide**: https://tokio.rs/

