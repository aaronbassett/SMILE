// Complete Bollard Usage Examples
// This file demonstrates production-ready patterns for using bollard crate

use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, HostConfig, RemoveContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecOptions};
use bollard::models::Mount;
use bollard::Docker;
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::time::{timeout, sleep};
use tracing::{debug, error, info, warn};

// ============================================================================
// ERROR HANDLING
// ============================================================================

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

    #[error("Command execution failed with status {code}: {stderr}")]
    CommandFailed { code: i64, stderr: String },

    #[error("Bollard error: {0}")]
    Bollard(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

impl From<bollard::errors::Error> for DockerError {
    fn from(err: bollard::errors::Error) -> Self {
        let msg = format!("{:?}", err);

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

// ============================================================================
// CONNECTION MANAGEMENT
// ============================================================================

/// Establish connection to Docker daemon with verification
pub async fn connect_docker() -> Result<Docker> {
    let docker = Docker::connect_with_local_defaults()
        .context("Failed to connect to Docker daemon")?;

    // Verify connection
    docker
        .version()
        .await
        .context("Docker daemon unreachable")?;

    info!("Connected to Docker daemon");
    Ok(docker)
}

// ============================================================================
// MOUNT CONFIGURATION
// ============================================================================

pub struct MountConfig {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

impl MountConfig {
    pub fn new(host_path: impl Into<String>, container_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            container_path: container_path.into(),
            read_only: false,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Validate mount configuration
    pub fn validate(&self) -> DockerResult<()> {
        let path = Path::new(&self.host_path);

        if !path.exists() {
            return Err(DockerError::InvalidConfig(format!(
                "Host path does not exist: {}",
                self.host_path
            )));
        }

        if !self.container_path.starts_with('/') {
            return Err(DockerError::InvalidConfig(format!(
                "Container path must be absolute: {}",
                self.container_path
            )));
        }

        Ok(())
    }

    fn to_mount(&self) -> Mount {
        Mount {
            typ: Some(bollard::models::MountTypeEnum::BIND),
            source: Some(self.host_path.clone()),
            target: Some(self.container_path.clone()),
            read_only: Some(self.read_only),
            ..Default::default()
        }
    }
}

/// Start container with validated mounts
pub async fn start_container_with_mounts(
    docker: &Docker,
    container_name: &str,
    image: &str,
    mounts: Vec<MountConfig>,
) -> DockerResult<String> {
    // Validate all mounts
    for mount in &mounts {
        mount.validate()?;
    }

    // Convert to Mount specs
    let mount_specs: Vec<Mount> = mounts.iter().map(|m| m.to_mount()).collect();

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

    let options = CreateContainerOptions {
        name: container_name,
    };

    // Create container
    let container = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    debug!("Created container: {}", container.id);

    // Start container
    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|e| DockerError::Container(e.to_string()))?;

    info!("Started container {} ({})", container_name, container.id);

    Ok(container.id)
}

// ============================================================================
// COMMAND EXECUTION
// ============================================================================

pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
}

/// Execute command in container with full output capture
pub async fn execute_command_complete(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
    timeout_secs: u64,
) -> DockerResult<CommandOutput> {
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(cmd.clone()),
        ..Default::default()
    };

    // Create exec instance
    let exec = docker
        .create_exec(container_id, exec_config)
        .await
        .map_err(|e| DockerError::Exec(format!("Failed to create exec: {}", e)))?;

    let exec_id = exec.id.clone();
    debug!("Created exec: {} for command: {:?}", exec_id, cmd);

    // Execute with timeout
    let output = timeout(
        Duration::from_secs(timeout_secs),
        execute_and_capture(&docker, &exec_id),
    )
    .await
    .map_err(|_| {
        DockerError::Timeout(format!(
            "Command execution exceeded {} seconds",
            timeout_secs
        ))
    })??;

    // Get exit code
    let inspect = docker
        .inspect_exec(&exec_id)
        .await
        .map_err(|e| DockerError::Exec(format!("Failed to inspect exec: {}", e)))?;

    let exit_code = inspect.exit_code.unwrap_or(-1);
    debug!("Command completed with exit code: {}", exit_code);

    Ok(CommandOutput {
        stdout: output,
        stderr: String::new(), // Docker streams combined; see note below
        exit_code,
    })
}

async fn execute_and_capture(
    docker: &Docker,
    exec_id: &str,
) -> DockerResult<String> {
    let stream = docker
        .start_exec(exec_id, Some(StartExecOptions::default()))
        .await
        .map_err(|e| DockerError::Exec(format!("Failed to start exec: {}", e)))?;

    let mut output = Vec::new();
    futures::pin_mut!(stream);

    while let Some(result) = stream.next().await {
        match result {
            Ok(bollard::exec::StartExecResults::Attached { log }) => {
                output.extend_from_slice(&log);
            }
            Err(e) => {
                return Err(DockerError::Exec(format!("Stream error: {}", e)));
            }
            _ => {}
        }
    }

    Ok(String::from_utf8_lossy(&output).to_string())
}

/// Execute command and verify success
pub async fn execute_command_checked(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<&str>,
) -> DockerResult<String> {
    let output = execute_command_complete(docker, container_id, cmd.clone(), 30).await?;

    if output.exit_code != 0 {
        return Err(DockerError::CommandFailed {
            code: output.exit_code,
            stderr: output.stdout,
        });
    }

    Ok(output.stdout)
}

// ============================================================================
// CONTAINER LIFECYCLE
// ============================================================================

pub async fn get_container_status(
    docker: &Docker,
    container_id: &str,
) -> DockerResult<String> {
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

    Ok(container
        .state
        .and_then(|s| s.status)
        .unwrap_or_else(|| "unknown".to_string()))
}

pub async fn stop_container(
    docker: &Docker,
    container_id: &str,
    timeout: Duration,
) -> DockerResult<()> {
    docker
        .stop_container(container_id, Some(timeout.as_secs()))
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

    info!("Stopped container: {}", container_id);
    Ok(())
}

pub async fn remove_container(
    docker: &Docker,
    container_id: &str,
    force: bool,
) -> DockerResult<()> {
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
        })?;

    info!("Removed container: {}", container_id);
    Ok(())
}

// ============================================================================
// CONTAINER RESET
// ============================================================================

/// Reset container: stop old one and start fresh
pub async fn reset_container_full(
    docker: &Docker,
    container_name: &str,
    image: &str,
    mounts: Vec<MountConfig>,
) -> DockerResult<String> {
    // Attempt to remove existing container
    if let Ok(container) = docker.inspect_container(container_name).await {
        if let Some(id) = container.id {
            info!("Removing existing container: {}", container_name);
            let _ = stop_container(docker, &id, Duration::from_secs(10)).await;
            let _ = remove_container(docker, &id, true).await;
        }
    }

    // Start fresh container
    info!("Starting fresh container: {}", container_name);
    start_container_with_mounts(docker, container_name, image, mounts).await
}

/// Safe container reset with validation
pub async fn reset_container_safe(
    docker: &Docker,
    container_name: &str,
    image: &str,
    mounts: Vec<MountConfig>,
) -> DockerResult<String> {
    // Step 1: Graceful shutdown
    if let Ok(container) = docker.inspect_container(container_name).await {
        if let Some(id) = &container.id {
            info!("Attempting graceful shutdown of {}", container_name);

            if let Err(e) = stop_container(docker, id, Duration::from_secs(10)).await {
                warn!("Graceful stop failed: {}, force removing", e);
                let _ = remove_container(docker, id, true).await;
            } else {
                // Verify it stopped
                sleep(Duration::from_millis(500)).await;
                let status = get_container_status(docker, id).await.unwrap_or_default();

                if status != "exited" {
                    warn!(
                        "Container didn't stop gracefully (status: {}), force removing",
                        status
                    );
                    let _ = remove_container(docker, id, true).await;
                } else {
                    let _ = remove_container(docker, id, false).await;
                }
            }
        }
    }

    // Step 2: Create and start fresh
    info!("Creating fresh container: {}", container_name);
    let container_id = start_container_with_mounts(docker, container_name, image, mounts).await?;

    // Step 3: Verify startup
    sleep(Duration::from_millis(500)).await;
    let status = get_container_status(docker, &container_id).await?;

    if status != "running" {
        return Err(DockerError::Container(format!(
            "Container failed to start. Status: {}",
            status
        )));
    }

    info!(
        "Container {} successfully reset and verified",
        container_name
    );
    Ok(container_id)
}

// ============================================================================
// HOST COMMUNICATION
// ============================================================================

pub async fn start_container_with_host_access(
    docker: &Docker,
    container_name: &str,
    image: &str,
    host_service_url: &str,
) -> DockerResult<String> {
    let env_vars = vec![
        format!("HOST_SERVICE_URL={}", host_service_url),
        "DOCKER_HOST=host.docker.internal".to_string(),
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

    info!("Started container with host service access");
    Ok(container.id)
}

/// Verify container can reach host service
pub async fn verify_host_connectivity(
    docker: &Docker,
    container_id: &str,
    host_url: &str,
) -> DockerResult<bool> {
    let cmd = vec!["curl", "-sf", host_url];

    match execute_command_checked(docker, container_id, cmd).await {
        Ok(_) => {
            info!(
                "Container {} successfully reached host at: {}",
                container_id, host_url
            );
            Ok(true)
        }
        Err(e) => {
            error!(
                "Container {} could not reach host at {}: {}",
                container_id, host_url, e
            );
            Ok(false)
        }
    }
}

// ============================================================================
// HIGH-LEVEL CONTAINER MANAGER
// ============================================================================

/// Production-grade container manager with lifecycle tracking
pub struct ContainerManager {
    docker: Arc<Docker>,
    running_containers: Arc<tokio::sync::Mutex<HashMap<String, String>>>,
}

impl ContainerManager {
    pub async fn new() -> DockerResult<Self> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| DockerError::Connection(e.to_string()))?;

        docker
            .version()
            .await
            .map_err(|e| DockerError::Connection(e.to_string()))?;

        info!("Container manager initialized");

        Ok(Self {
            docker: Arc::new(docker),
            running_containers: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        })
    }

    pub async fn start_tracked(
        &self,
        name: &str,
        image: &str,
        mounts: Vec<MountConfig>,
    ) -> DockerResult<String> {
        // Reset if already running
        let mut containers = self.running_containers.lock().await;
        if let Some(id) = containers.get(name) {
            info!("Container {} already running, resetting", name);
            let _ = stop_container(&self.docker, id, Duration::from_secs(10)).await;
            let _ = remove_container(&self.docker, id, true).await;
        }

        // Start new container
        let container_id =
            start_container_with_mounts(&self.docker, name, image, mounts).await?;

        containers.insert(name.to_string(), container_id.clone());
        Ok(container_id)
    }

    pub async fn execute(
        &self,
        container_name: &str,
        cmd: Vec<&str>,
    ) -> DockerResult<CommandOutput> {
        let containers = self.running_containers.lock().await;
        let container_id = containers
            .get(container_name)
            .ok_or_else(|| DockerError::Container(
                format!("Container {} not tracked", container_name)
            ))?
            .clone();

        drop(containers);

        execute_command_complete(&self.docker, &container_id, cmd, 30).await
    }

    pub async fn cleanup_all(&self) -> DockerResult<()> {
        let containers = self.running_containers.lock().await.clone();

        for (name, id) in containers {
            info!("Cleaning up container: {}", name);
            let _ = stop_container(&self.docker, &id, Duration::from_secs(10)).await;
            let _ = remove_container(&self.docker, &id, true).await;
        }

        Ok(())
    }
}

impl Drop for ContainerManager {
    fn drop(&mut self) {
        debug!("ContainerManager dropped (async cleanup should be called explicitly)");
    }
}

// ============================================================================
// TEST HARNESS
// ============================================================================

/// Test container with automatic cleanup
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
    ) -> DockerResult<Self> {
        let docker = Arc::new(connect_docker().await.map_err(|e| {
            DockerError::Connection(e.to_string())
        })?);

        let container_id =
            start_container_with_mounts(&docker, name, image, mounts).await?;

        info!("Started test container: {}", name);

        Ok(Self {
            docker,
            container_id,
            name: name.to_string(),
        })
    }

    pub async fn execute(&self, cmd: Vec<&str>) -> DockerResult<CommandOutput> {
        execute_command_complete(&self.docker, &self.container_id, cmd, 30).await
    }

    pub async fn cleanup(self) -> DockerResult<()> {
        info!("Cleaning up test container: {}", self.name);
        let _ = stop_container(&self.docker, &self.container_id, Duration::from_secs(5)).await;
        remove_container(&self.docker, &self.container_id, true).await
    }
}

impl Drop for TestContainer {
    fn drop(&mut self) {
        let docker = Arc::clone(&self.docker);
        let container_id = self.container_id.clone();

        tokio::spawn(async move {
            debug!("Async cleanup of test container in background");
            let _ = remove_container(&docker, &container_id, true).await;
        });
    }
}

// ============================================================================
// MAIN EXAMPLE
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    tracing_subscriber::fmt::init();

    // Connect to Docker
    let docker = connect_docker().await?;

    // Example 1: Start container with mounts
    let mounts = vec![
        MountConfig::new("/tmp/data", "/app/data"),
        MountConfig::new("/tmp/config", "/app/config").read_only(),
    ];

    let container_id = start_container_with_mounts(
        &docker,
        "test-container",
        "alpine:latest",
        mounts,
    )
    .await?;

    info!("Started container: {}", container_id);

    // Example 2: Execute command
    let output = execute_command_checked(
        &docker,
        &container_id,
        vec!["echo", "Hello from container"],
    )
    .await?;

    info!("Command output: {}", output);

    // Example 3: Cleanup
    stop_container(&docker, &container_id, Duration::from_secs(10)).await?;
    remove_container(&docker, &container_id, false).await?;

    info!("Container cleaned up");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_config_validation() {
        let mount = MountConfig::new("/tmp", "/app");
        assert!(mount.validate().is_ok());

        let invalid_mount = MountConfig::new("/nonexistent/path", "/app");
        assert!(invalid_mount.validate().is_err());

        let relative_mount = MountConfig::new("/tmp", "relative/path");
        assert!(relative_mount.validate().is_err());
    }
}
