# Bollard Crate Best Practices: Docker Container Management in Rust

A comprehensive guide to using the bollard crate for Docker container management with focus on error handling, common pitfalls, and production-ready patterns.

## Table of Contents

1. [Overview](#overview)
2. [Installation & Setup](#installation--setup)
3. [Connection Management](#connection-management)
4. [Error Handling Patterns](#error-handling-patterns)
5. [Starting Containers with Volume Mounts](#starting-containers-with-volume-mounts)
6. [Executing Commands Inside Containers](#executing-commands-inside-containers)
7. [Container Lifecycle Management](#container-lifecycle-management)
8. [Container Reset Pattern](#container-reset-pattern)
9. [Host Communication from Containers](#host-communication-from-containers)
10. [Common Pitfalls](#common-pitfalls)
11. [Production Patterns](#production-patterns)

---

## Overview

Bollard is an asynchronous Rust Docker daemon API client built on Tokio and Hyper. It provides comprehensive container management capabilities including:

- Container creation, lifecycle management (start, stop, remove)
- Image management and pulling
- Exec API for running commands in containers
- Volume and network management
- Stats and logs streaming
- Cross-platform support (Unix sockets, Windows named pipes, remote TCP, SSH)

**Key Design Principles:**
- Fully async/await based (requires Tokio runtime)
- Type-safe error handling with custom error types
- Stream-based APIs for long-running operations
- Respects standard Docker environment variables

---

## Installation & Setup

### Cargo.toml

```toml
[package]
name = "docker_manager"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
bollard = "0.15"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1.0"
```

### Project Structure

```
src/
├── main.rs
├── docker_client.rs        # Wrapper around Docker
├── container_manager.rs    # High-level container operations
├── error.rs               # Custom error types
└── exec.rs                # Command execution patterns
```

---

## Connection Management

### Pattern 1: Basic Connection (Recommended)

```rust
use bollard::Docker;
use std::path::Path;

/// Establish connection to Docker daemon with sensible defaults
///
/// Respects DOCKER_HOST and DOCKER_CERT_PATH environment variables.
/// Defaults to Unix socket on Linux/macOS, named pipes on Windows.
pub async fn connect_docker() -> Result<Docker, anyhow::Error> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!("Failed to connect to Docker daemon: {}", e))?;

    // Verify connection is working
    docker.version().await
        .map_err(|e| anyhow::anyhow!("Docker daemon unreachable: {}", e))?;

    Ok(docker)
}
```

### Pattern 2: Connection with Custom Configuration

```rust
use bollard::Docker;
use std::time::Duration;

/// Establish connection with explicit timeout and socket path
pub async fn connect_docker_custom(
    socket_path: &str,
    timeout_secs: u64,
) -> Result<Docker, anyhow::Error> {
    let docker = Docker::connect_with_unix(socket_path)
        .map_err(|e| anyhow::anyhow!("Failed to connect to Docker at {}: {}", socket_path, e))?;

    docker.version().await?;
    Ok(docker)
}
```

### Pattern 3: Connection Pooling Wrapper

```rust
use bollard::Docker;
use std::sync::Arc;

/// Wrapper providing convenient access to Docker with connection pooling
pub struct DockerPool {
    docker: Arc<Docker>,
}

impl DockerPool {
    pub async fn new() -> Result<Self, anyhow::Error> {
        let docker = Docker::connect_with_local_defaults()?;
        docker.version().await?;
        Ok(Self {
            docker: Arc::new(docker),
        })
    }

    pub fn client(&self) -> Arc<Docker> {
        Arc::clone(&self.docker)
    }
}

impl Clone for DockerPool {
    fn clone(&self) -> Self {
        Self {
            docker: Arc::clone(&self.docker),
        }
    }
}
```

---

## Error Handling Patterns

### Custom Error Type (Recommended for Libraries)

```rust
use thiserror::Error;
use bollard::errors::Error as BollardError;

#[derive(Error, Debug)]
pub enum DockerError {
    #[error("Container error: {0}")]
    Container(String),

    #[error("Container not found: {id}")]
    ContainerNotFound { id: String },

    #[error("Image error: {0}")]
    Image(String),

    #[error("Exec error: {0}")]
    Exec(String),

    #[error("Volume error: {0}")]
    Volume(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Command execution failed with status {code}: {stderr}")]
    CommandFailed { code: i64, stderr: String },

    #[error("Bollard error: {0}")]
    Bollard(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<BollardError> for DockerError {
    fn from(err: BollardError) -> Self {
        let msg = format!("{:?}", err);

        // Pattern match on specific Bollard errors if needed
        if msg.contains("No such container") {
            return DockerError::Container("Container not found".to_string());
        }
        if msg.contains("Image not found") {
            return DockerError::Image("Image not found".to_string());
        }

        DockerError::Bollard(msg)
    }
}

pub type DockerResult<T> = Result<T, DockerError>;
```

### Error Context Pattern

```rust
use anyhow::{Context, Result};

/// Wrapper function demonstrating error context
pub async fn get_container_info(
    docker: &Docker,
    container_id: &str,
) -> Result<bollard::models::ContainerInspectResponse> {
    docker
        .inspect_container(container_id)
        .await
        .with_context(|| format!("Failed to inspect container: {}", container_id))
}
```

### Error Recovery Pattern

```rust
use std::time::Duration;
use tokio::time::sleep;

/// Retry logic for transient Docker errors
pub async fn create_container_with_retry<F, T>(
    mut operation: F,
    max_retries: u32,
) -> Result<T, DockerError>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, DockerError>>>>,
{
    let mut attempt = 0;
    let max_wait = Duration::from_secs(10);

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }

                // Exponential backoff with jitter
                let wait_time = Duration::from_millis(100 * 2_u64.pow(attempt - 1))
                    .min(max_wait);

                tracing::warn!(
                    attempt = attempt,
                    error = %e,
                    "Operation failed, retrying in {:?}",
                    wait_time
                );

                sleep(wait_time).await;
            }
        }
    }
}
```

---

## Starting Containers with Volume Mounts

### Pattern 1: Simple Bind Mount (Host Directory)

```rust
use bollard::container::{Config, CreateContainerOptions, HostConfig};
use bollard::models::Mount;
use std::collections::HashMap;

pub async fn start_container_with_bind_mount(
    docker: &Docker,
    container_name: &str,
    image: &str,
    host_path: &str,
    container_path: &str,
) -> Result<String, DockerError> {
    // Configure mount
    let mount = Mount {
        typ: Some(bollard::models::MountTypeEnum::BIND),
        source: Some(host_path.to_string()),
        target: Some(container_path.to_string()),
        read_only: Some(false),
        ..Default::default()
    };

    // Configure container
    let host_config = HostConfig {
        mounts: Some(vec![mount]),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    // Create and start
    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 2: Read-Only Mount

```rust
pub async fn start_container_readonly_mount(
    docker: &Docker,
    container_name: &str,
    image: &str,
    host_path: &str,
    container_path: &str,
) -> Result<String, DockerError> {
    let mount = Mount {
        typ: Some(bollard::models::MountTypeEnum::BIND),
        source: Some(host_path.to_string()),
        target: Some(container_path.to_string()),
        read_only: Some(true), // Read-only
        ..Default::default()
    };

    let host_config = HostConfig {
        mounts: Some(vec![mount]),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 3: Multiple Mounts with Configuration

```rust
pub struct MountConfig {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

pub async fn start_container_multiple_mounts(
    docker: &Docker,
    container_name: &str,
    image: &str,
    mounts: Vec<MountConfig>,
) -> Result<String, DockerError> {
    // Validate paths
    for mount in &mounts {
        let host_path = std::path::Path::new(&mount.host_path);
        if !host_path.exists() {
            return Err(DockerError::InvalidConfig(
                format!("Host path does not exist: {}", mount.host_path)
            ));
        }
    }

    let mount_specs: Vec<Mount> = mounts
        .into_iter()
        .map(|m| Mount {
            typ: Some(bollard::models::MountTypeEnum::BIND),
            source: Some(m.host_path),
            target: Some(m.container_path),
            read_only: Some(m.read_only),
            ..Default::default()
        })
        .collect();

    let host_config = HostConfig {
        mounts: Some(mount_specs),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 4: Named Volumes

```rust
pub async fn start_container_with_named_volume(
    docker: &Docker,
    container_name: &str,
    image: &str,
    volume_name: &str,
    container_path: &str,
) -> Result<String, DockerError> {
    // Named volumes don't require the source to exist on host
    let mount = Mount {
        typ: Some(bollard::models::MountTypeEnum::VOLUME),
        source: Some(volume_name.to_string()),
        target: Some(container_path.to_string()),
        read_only: Some(false),
        ..Default::default()
    };

    let host_config = HostConfig {
        mounts: Some(vec![mount]),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pitfall: Mount Validation

```rust
// DON'T: Assuming host paths exist
let mount = Mount {
    source: Some("/nonexistent/path".to_string()), // This will fail at runtime
    target: Some("/app/data".to_string()),
    typ: Some(bollard::models::MountTypeEnum::BIND),
    ..Default::default()
};

// DO: Validate paths before creating container
pub fn validate_mount_config(host_path: &str) -> Result<(), DockerError> {
    let path = std::path::Path::new(host_path);
    if !path.exists() {
        return Err(DockerError::InvalidConfig(
            format!("Host path does not exist: {}", host_path)
        ));
    }
    Ok(())
}
```

---

## Executing Commands Inside Containers

### Pattern 1: Simple Command Execution

```rust
use bollard::exec::{CreateExecOptions, StartExecOptions};
use futures::stream::StreamExt;

pub async fn execute_command(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
) -> Result<String, DockerError> {
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(cmd),
        ..Default::default()
    };

    let exec = docker
        .create_exec(container_id, exec_config)
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let mut stream = docker
        .start_exec(&exec.id, Some(StartExecOptions::default()))
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let mut output = String::new();
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(bollard::exec::StartExecResults::Attached { log }) => {
                output.push_str(&String::from_utf8_lossy(&log));
            }
            Err(e) => return Err(DockerError::Exec(e.to_string())),
            _ => {}
        }
    }

    Ok(output)
}
```

### Pattern 2: Command Execution with Exit Code

```rust
pub async fn execute_command_with_status(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
) -> Result<(String, i64), DockerError> {
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(cmd),
        ..Default::default()
    };

    let exec = docker
        .create_exec(container_id, exec_config)
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let exec_id = exec.id.clone();

    let stream = docker
        .start_exec(&exec_id, Some(StartExecOptions::default()))
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let mut output = String::new();
    futures::pin_mut!(stream);
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(bollard::exec::StartExecResults::Attached { log }) => {
                output.push_str(&String::from_utf8_lossy(&log));
            }
            Err(e) => return Err(DockerError::Exec(e.to_string())),
            _ => {}
        }
    }

    // Get exit code
    let inspect = docker
        .inspect_exec(&exec_id)
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let exit_code = inspect.exit_code.unwrap_or(-1);

    if exit_code != 0 {
        return Err(DockerError::CommandFailed {
            code: exit_code,
            stderr: output.clone(),
        });
    }

    Ok((output, exit_code))
}
```

### Pattern 3: Command with Timeout

```rust
use tokio::time::{timeout, Duration};

pub async fn execute_command_with_timeout(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
    timeout_secs: u64,
) -> Result<String, DockerError> {
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(cmd),
        ..Default::default()
    };

    let exec = docker
        .create_exec(container_id, exec_config)
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let exec_id = exec.id.clone();

    let future = async {
        let stream = docker
            .start_exec(&exec_id, Some(StartExecOptions::default()))
            .await
            .map_err(|e| DockerError::Exec(e.to_string()))?;

        let mut output = String::new();
        futures::pin_mut!(stream);
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(bollard::exec::StartExecResults::Attached { log }) => {
                    output.push_str(&String::from_utf8_lossy(&log));
                }
                Err(e) => return Err(DockerError::Exec(e.to_string())),
                _ => {}
            }
        }
        Ok::<_, DockerError>(output)
    };

    timeout(Duration::from_secs(timeout_secs), future)
        .await
        .map_err(|_| DockerError::Timeout(
            format!("Command execution exceeded {} seconds", timeout_secs)
        ))?
}
```

### Pattern 4: Capturing Stdout and Stderr Separately

```rust
use std::collections::VecDeque;

pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
}

pub async fn execute_command_capture_streams(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
) -> Result<CommandOutput, DockerError> {
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(cmd),
        ..Default::default()
    };

    let exec = docker
        .create_exec(container_id, exec_config)
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let exec_id = exec.id.clone();

    let stream = docker
        .start_exec(&exec_id, Some(StartExecOptions::default()))
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let mut stdout = String::new();
    let mut stderr = String::new();

    futures::pin_mut!(stream);
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(bollard::exec::StartExecResults::Attached { log }) => {
                // Docker streams stdout and stderr in the same output
                // In practice, you may need to use separate exec calls or
                // parse the multiplexed output if strict separation is needed
                stdout.push_str(&String::from_utf8_lossy(&log));
            }
            Err(e) => return Err(DockerError::Exec(e.to_string())),
            _ => {}
        }
    }

    let inspect = docker
        .inspect_exec(&exec_id)
        .await
        .map_err(|e| DockerError::Exec(e.to_string()))?;

    let exit_code = inspect.exit_code.unwrap_or(-1);

    Ok(CommandOutput {
        stdout,
        stderr,
        exit_code,
    })
}
```

### Pitfall: Not Handling Stream Closure

```rust
// DON'T: Assume stream.next() continues forever
while let Some(msg) = stream.next().await {
    // ...
}
// Missing: Handle when stream ends (command finishes)

// DO: Properly handle stream exhaustion
let mut output = Vec::new();
futures::pin_mut!(stream);
while let Some(result) = stream.next().await {
    match result {
        Ok(bollard::exec::StartExecResults::Attached { log }) => {
            output.extend_from_slice(&log);
        }
        Err(e) => return Err(e.into()),
        _ => {} // Handle other result types
    }
}
// Stream is now exhausted; check exit code or proceed
```

---

## Container Lifecycle Management

### Pattern 1: Start Container

```rust
pub async fn start_container(
    docker: &Docker,
    container_id: &str,
) -> Result<(), DockerError> {
    docker
        .start_container::<String>(container_id, None)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such container") {
                DockerError::ContainerNotFound {
                    id: container_id.to_string(),
                }
            } else {
                DockerError::Container(msg)
            }
        })
}
```

### Pattern 2: Stop Container

```rust
use std::time::Duration;

pub async fn stop_container(
    docker: &Docker,
    container_id: &str,
    timeout: Option<Duration>,
) -> Result<(), DockerError> {
    let wait_options = timeout.map(|d| d.as_secs());

    docker
        .stop_container(container_id, wait_options)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such container") {
                DockerError::ContainerNotFound {
                    id: container_id.to_string(),
                }
            } else {
                DockerError::Container(msg)
            }
        })
}
```

### Pattern 3: Remove Container

```rust
use bollard::container::RemoveContainerOptions;

pub async fn remove_container(
    docker: &Docker,
    container_id: &str,
    force: bool,
) -> Result<(), DockerError> {
    let options = RemoveContainerOptions {
        force,
        ..Default::default()
    };

    docker
        .remove_container(container_id, Some(options))
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such container") {
                DockerError::ContainerNotFound {
                    id: container_id.to_string(),
                }
            } else {
                DockerError::Container(msg)
            }
        })
}
```

### Pattern 4: Get Container Status

```rust
pub async fn get_container_status(
    docker: &Docker,
    container_id: &str,
) -> Result<String, DockerError> {
    let container = docker
        .inspect_container(container_id)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such container") {
                DockerError::ContainerNotFound {
                    id: container_id.to_string(),
                }
            } else {
                DockerError::Container(msg)
            }
        })?;

    Ok(container.state.map(|s| s.status.unwrap_or_default()).unwrap_or_default())
}
```

### Pattern 5: Wait for Container

```rust
use bollard::container::WaitContainerOptions;

pub async fn wait_for_container(
    docker: &Docker,
    container_id: &str,
) -> Result<i64, DockerError> {
    let options = WaitContainerOptions {
        condition: "next-exit",
    };

    let mut stream = docker
        .wait_container(container_id, Some(options))
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(status) => {
                return Ok(status.status_code.unwrap_or(-1));
            }
            Err(e) => return Err(DockerError::Container(e.to_string())),
        }
    }

    Err(DockerError::Container("Container wait stream closed".to_string()))
}
```

---

## Container Reset Pattern

### Pattern 1: Simple Reset (Stop and Remove)

```rust
pub async fn reset_container(
    docker: &Docker,
    container_id: &str,
) -> Result<(), DockerError> {
    // Try to stop the container
    let _ = stop_container(docker, container_id, Some(Duration::from_secs(10))).await;

    // Remove the container (force in case stop failed)
    remove_container(docker, container_id, true).await
}
```

### Pattern 2: Reset with Restart (Stop and Restart)

```rust
pub async fn restart_container(
    docker: &Docker,
    container_id: &str,
    timeout: Option<Duration>,
) -> Result<(), DockerError> {
    use bollard::container::RestartPolicyKind;

    let timeout_secs = timeout.map(|d| d.as_secs());

    docker
        .restart_container(container_id, timeout_secs)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such container") {
                DockerError::ContainerNotFound {
                    id: container_id.to_string(),
                }
            } else {
                DockerError::Container(msg)
            }
        })
}
```

### Pattern 3: Full Container Reset (Stop Old, Start Fresh)

```rust
pub async fn full_reset_container(
    docker: &Docker,
    container_name: &str,
    image: &str,
    config: Config,
) -> Result<String, DockerError> {
    // Step 1: Check if container exists
    match docker.inspect_container(container_name).await {
        Ok(container) => {
            // Container exists, need to remove it
            let container_id = container.id.ok_or_else(|| {
                DockerError::Container("No container ID found".to_string())
            })?;

            tracing::info!("Stopping existing container: {}", container_name);
            let _ = stop_container(docker, &container_id, Some(Duration::from_secs(10))).await;

            tracing::info!("Removing existing container: {}", container_name);
            remove_container(docker, &container_id, true).await?;
        }
        Err(_) => {
            // Container doesn't exist, which is fine
            tracing::info!("Container {} does not exist, creating new one", container_name);
        }
    }

    // Step 2: Create fresh container
    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    tracing::info!("Starting fresh container: {}", container_name);
    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 4: Graceful Reset with Validation

```rust
/// Safely reset a container with validation and cleanup
pub async fn safe_container_reset(
    docker: &Docker,
    container_name: &str,
    image: &str,
    config: Config,
    max_wait_secs: u64,
) -> Result<String, DockerError> {
    // Step 1: Attempt graceful shutdown
    if let Ok(container) = docker.inspect_container(container_name).await {
        if let Some(id) = &container.id {
            tracing::info!("Initiating graceful shutdown of {}", container_name);

            // Send SIGTERM and wait for graceful shutdown
            if let Ok(_) = docker.kill_container::<String>(id, None).await {
                // Wait for it to stop
                tokio::time::sleep(Duration::from_secs(2)).await;
            }

            // Check if it stopped
            match docker.inspect_container(id).await {
                Ok(c) => {
                    let state = c.state.as_ref().and_then(|s| s.status.as_deref());
                    if state != Some("exited") {
                        // Still running, force kill
                        tracing::warn!("Container didn't stop gracefully, force removing");
                        remove_container(docker, id, true).await?;
                    } else {
                        // Stopped gracefully
                        remove_container(docker, id, false).await?;
                    }
                }
                Err(_) => {} // Already gone
            }
        }
    }

    // Step 2: Create new container
    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    // Step 3: Start container
    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    // Step 4: Verify it started
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = get_container_status(docker, &container.id).await?;
    if status != "running" {
        return Err(DockerError::Container(
            format!("Container failed to start. Status: {}", status)
        ));
    }

    tracing::info!("Container {} successfully reset and started", container_name);
    Ok(container.id)
}
```

---

## Host Communication from Containers

### Pattern 1: Using host.docker.internal (macOS/Windows)

```rust
/// Configure container to communicate with host services
pub async fn start_container_with_host_access(
    docker: &Docker,
    container_name: &str,
    image: &str,
    host_port: u16,
    container_port: u16,
) -> Result<String, DockerError> {
    let mut port_bindings = std::collections::HashMap::new();

    port_bindings.insert(
        format!("{}/tcp", container_port),
        Some(vec![bollard::models::PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(host_port.to_string()),
        }]),
    );

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        // On Linux, you may need extra_hosts for host.docker.internal
        extra_hosts: Some(vec![
            "host.docker.internal:host-gateway".to_string(),
        ]),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 2: Network Configuration for Host Access

```rust
pub async fn start_container_on_host_network(
    docker: &Docker,
    container_name: &str,
    image: &str,
) -> Result<String, DockerError> {
    // Note: host network mode doesn't work on Docker Desktop on macOS/Windows
    // Use port binding instead for those platforms

    let host_config = HostConfig {
        network_mode: Some("host".to_string()),
        ..Default::default()
    };

    let config = Config {
        image: Some(image.to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 3: Environment Variables for Service Discovery

```rust
pub async fn start_container_with_service_discovery(
    docker: &Docker,
    container_name: &str,
    image: &str,
    host_service_url: &str,
) -> Result<String, DockerError> {
    let mut env_vars = vec![
        format!("HOST_SERVICE_URL={}", host_service_url),
        // On Linux, use host.docker.internal:port
        // On macOS/Windows, it resolves automatically
        "DOCKER_HOST_ALIAS=host.docker.internal".to_string(),
    ];

    let config = Config {
        image: Some(image.to_string()),
        env: Some(env_vars),
        host_config: Some(HostConfig {
            extra_hosts: Some(vec![
                "host.docker.internal:host-gateway".to_string(),
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: container_name,
    };

    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    Ok(container.id)
}
```

### Pattern 4: Test Container Communication

```rust
pub async fn verify_host_connectivity(
    docker: &Docker,
    container_id: &str,
    host_url: &str,
) -> Result<bool, DockerError> {
    // Test connectivity from container to host
    let cmd = vec!["curl", "-f", host_url];

    match execute_command_with_timeout(docker, container_id, cmd, 5).await {
        Ok(_) => {
            tracing::info!("Container successfully reached host at: {}", host_url);
            Ok(true)
        }
        Err(e) => {
            tracing::error!("Container could not reach host: {}", e);
            Ok(false)
        }
    }
}
```

### Pitfall: Platform-Specific host.docker.internal

```rust
// DON'T: Assume host.docker.internal works on all platforms
// It works on Docker Desktop (Mac/Windows) but needs extra_hosts on Linux

// DO: Use conditional logic based on platform or always include extra_hosts
let extra_hosts = if cfg!(target_os = "linux") {
    Some(vec!["host.docker.internal:host-gateway".to_string()])
} else {
    // macOS/Windows Docker Desktop handles it automatically,
    // but including it doesn't hurt
    Some(vec!["host.docker.internal:host-gateway".to_string()])
};

let host_config = HostConfig {
    extra_hosts,
    ..Default::default()
};
```

---

## Common Pitfalls

### Pitfall 1: Not Awaiting Async Operations

```rust
// DON'T
let container = docker.create_container(Some(options), config); // Future, not awaited!

// DO
let container = docker.create_container(Some(options), config).await?;
```

### Pitfall 2: Forgetting to Start Container After Creation

```rust
// DON'T
let container = docker.create_container(Some(options), config).await?;
// Container is created but not started!

// DO
let container = docker.create_container(Some(options), config).await?;
docker.start_container(&container.id, None).await?;
```

### Pitfall 3: Not Handling Container Conflicts

```rust
// DON'T - This will fail if container name already exists
docker.create_container(Some(options), config).await?;

// DO - Check and remove if exists
if docker.inspect_container(container_name).await.is_ok() {
    let _ = docker.remove_container(container_name, Some(
        RemoveContainerOptions { force: true, ..Default::default() }
    )).await;
}
docker.create_container(Some(options), config).await?;
```

### Pitfall 4: Ignoring Mount Path Existence

```rust
// DON'T - Host path doesn't need to exist for named volumes but does for bind mounts
let mount = Mount {
    typ: Some(bollard::models::MountTypeEnum::BIND),
    source: Some("/nonexistent/path".to_string()),
    target: Some("/app/data".to_string()),
    ..Default::default()
};

// DO - Validate bind mount paths
if mount_type == "bind" {
    let host_path = std::path::Path::new(source);
    if !host_path.exists() {
        return Err(DockerError::InvalidConfig("Host path does not exist".to_string()));
    }
}
```

### Pitfall 5: Stream Not Fully Consumed

```rust
// DON'T - Stream not consumed, leaving processes hanging
let stream = docker.start_exec(&exec_id, Some(options)).await?;

// DO - Fully consume the stream
let mut stream = docker.start_exec(&exec_id, Some(options)).await?;
while let Some(result) = stream.next().await {
    // Process result
}
```

### Pitfall 6: No Timeout on Long-Running Commands

```rust
// DON'T - Command hangs indefinitely
docker.start_exec(&exec_id, Some(options)).await?;

// DO - Add timeout
timeout(Duration::from_secs(30), docker.start_exec(&exec_id, Some(options))).await??;
```

### Pitfall 7: Assuming All Docker Errors Are Recoverable

```rust
// DON'T - Retry everything
retry_with_backoff(|| docker.create_container(...)).await;

// DO - Only retry on transient errors
match docker.create_container(...).await {
    Ok(c) => Ok(c),
    Err(e) => {
        if e.to_string().contains("already exists") {
            // Don't retry, container name conflict
            Err(DockerError::Container("Container already exists".to_string()))
        } else if is_transient_error(&e) {
            retry_with_backoff(|| docker.create_container(...)).await
        } else {
            Err(e.into())
        }
    }
}
```

---

## Production Patterns

### Pattern 1: Container Manager Service

```rust
/// High-level container management with lifecycle tracking
pub struct ContainerManager {
    docker: Arc<Docker>,
    running_containers: Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
}

impl ContainerManager {
    pub async fn new() -> Result<Self, DockerError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| DockerError::Connection(e.to_string()))?;

        docker.version().await
            .map_err(|e| DockerError::Connection(e.to_string()))?;

        Ok(Self {
            docker: Arc::new(docker),
            running_containers: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new()
            )),
        })
    }

    pub async fn start_container_tracked(
        &self,
        name: &str,
        config: Config,
    ) -> Result<String, DockerError> {
        // Reset if already running
        let mut containers = self.running_containers.lock().await;
        if let Some(id) = containers.get(name) {
            let _ = self.cleanup_container(id).await;
        }

        // Create and start
        let options = CreateContainerOptions { name };
        let container = self.docker
            .create_container(Some(options), config)
            .await
            .map_err(|e| DockerError::Container(e.to_string()))?;

        self.docker
            .start_container::<String>(&container.id, None)
            .await
            .map_err(|e| DockerError::Container(e.to_string()))?;

        containers.insert(name.to_string(), container.id.clone());
        Ok(container.id)
    }

    async fn cleanup_container(&self, container_id: &str) -> Result<(), DockerError> {
        let _ = self.docker
            .stop_container(container_id, Some(10))
            .await;
        self.docker
            .remove_container(container_id, Some(
                RemoveContainerOptions { force: true, ..Default::default() }
            ))
            .await
            .map_err(|e| DockerError::Container(e.to_string()))
    }

    pub async fn cleanup_all(&self) -> Result<(), DockerError> {
        let containers = self.running_containers.lock().await.clone();
        for (_, id) in containers {
            let _ = self.cleanup_container(&id).await;
        }
        Ok(())
    }
}

impl Drop for ContainerManager {
    fn drop(&mut self) {
        // Note: Can't use async in Drop, so cleanup must be called explicitly
    }
}
```

### Pattern 2: Graceful Shutdown Handler

```rust
use tokio::signal;
use std::sync::Arc;

/// Setup graceful shutdown for container operations
pub async fn setup_shutdown_handler(
    manager: Arc<ContainerManager>,
) -> Result<(), DockerError> {
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!("Received shutdown signal, cleaning up containers");
                if let Err(e) = manager.cleanup_all().await {
                    tracing::error!("Error during cleanup: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Error setting up signal handler: {}", e);
            }
        }
    });

    Ok(())
}
```

### Pattern 3: Comprehensive Test Container Harness

```rust
/// Test harness for spinning up and managing test containers
pub struct TestContainer {
    docker: Arc<Docker>,
    container_id: String,
    name: String,
}

impl TestContainer {
    pub async fn start(
        name: &str,
        image: &str,
        mounts: Vec<MountConfig>,
    ) -> Result<Self, DockerError> {
        let docker = Arc::new(
            Docker::connect_with_local_defaults()
                .map_err(|e| DockerError::Connection(e.to_string())?,
        );

        let mount_specs: Vec<Mount> = mounts
            .into_iter()
            .map(|m| Mount {
                typ: Some(bollard::models::MountTypeEnum::BIND),
                source: Some(m.host_path),
                target: Some(m.container_path),
                read_only: Some(m.read_only),
                ..Default::default()
            })
            .collect();

        let host_config = HostConfig {
            mounts: Some(mount_specs),
            extra_hosts: Some(vec![
                "host.docker.internal:host-gateway".to_string(),
            ]),
            ..Default::default()
        };

        let config = Config {
            image: Some(image.to_string()),
            host_config: Some(host_config),
            ..Default::default()
        };

        let options = CreateContainerOptions { name };
        let container = docker
            .create_container(Some(options), config)
            .await
            .map_err(|e| DockerError::Container(e.to_string()))?;

        docker
            .start_container::<String>(&container.id, None)
            .await
            .map_err(|e| DockerError::Container(e.to_string()))?;

        Ok(Self {
            docker,
            container_id: container.id,
            name: name.to_string(),
        })
    }

    pub async fn execute(&self, cmd: Vec<&str>) -> Result<String, DockerError> {
        execute_command(&self.docker, &self.container_id, cmd).await
    }
}

impl Drop for TestContainer {
    fn drop(&mut self) {
        let docker = Arc::clone(&self.docker);
        let container_id = self.container_id.clone();

        tokio::spawn(async move {
            let _ = docker
                .remove_container(
                    &container_id,
                    Some(RemoveContainerOptions { force: true, ..Default::default() }),
                )
                .await;
        });
    }
}
```

---

## References & Resources

- **Official Bollard Documentation**: https://docs.rs/bollard/
- **GitHub Repository**: https://github.com/fussybeaver/bollard
- **Docker API Reference**: https://docs.docker.com/engine/api/
- **Rust Async Best Practices**: https://rust-lang.github.io/async-book/
- **Tokio Tutorial**: https://tokio.rs/

---

## Summary of Key Takeaways

1. **Always await async operations** - Bollard is fully async; use `.await` on all Docker operations
2. **Validate mounts before creation** - Bind mounts fail if host paths don't exist
3. **Handle streams completely** - Fully consume exec streams to avoid hung processes
4. **Use custom error types** - Implement type-safe error handling with thiserror
5. **Implement graceful shutdown** - Always cleanup containers before exiting
6. **Test platform-specific features** - host.docker.internal has different behavior on Linux vs macOS/Windows
7. **Add timeouts** - Long-running commands need timeouts to prevent hangs
8. **Verify container state** - Don't assume operations succeeded; check container status
9. **Use connection pooling** - Share Docker client across threads with Arc
10. **Handle transient vs permanent errors** - Only retry on transient failures

