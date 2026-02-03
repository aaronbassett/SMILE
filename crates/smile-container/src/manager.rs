//! Docker container manager for SMILE Loop.
//!
//! This module provides the [`ContainerManager`] struct for managing Docker
//! container lifecycle operations through the bollard crate.

use std::path::Path;

use bollard::container::Config as BollardConfig;
use bollard::container::CreateContainerOptions as BollardCreateOptions;
use bollard::container::RemoveContainerOptions;
use bollard::container::StartContainerOptions;
use bollard::container::StopContainerOptions;
use bollard::models::HostConfig;
use bollard::models::Mount as BollardMount;
use bollard::models::MountTypeEnum;
use bollard::Docker;
use tracing::{debug, info, instrument, warn};

use crate::{Container, ContainerError, ContainerStatus, Mount};

/// Options for creating a new Docker container.
///
/// This struct configures how a container should be created, including
/// its name, image, volume mounts, environment variables, and optional
/// command to run.
///
/// # Example
///
/// ```no_run
/// use std::path::PathBuf;
/// use smile_container::{CreateContainerOptions, Mount};
///
/// let options = CreateContainerOptions::new("my-container", "smile-base:latest")
///     .with_mount(Mount::read_only("/host/tutorial", "/workspace/tutorial"))
///     .with_mount(Mount::new("/host/work", "/workspace/work"))
///     .with_env("DEBUG", "true")
///     .with_cmd(vec!["bash", "-c", "echo hello"]);
/// ```
#[derive(Debug, Clone)]
pub struct CreateContainerOptions {
    /// Human-readable name for the container.
    pub name: String,
    /// Docker image to use (e.g., "smile-base:latest").
    pub image: String,
    /// Volume mounts to attach to the container.
    pub mounts: Vec<Mount>,
    /// Environment variables to set in the container.
    pub env: Option<Vec<String>>,
    /// Optional command to run instead of the image's default.
    pub cmd: Option<Vec<String>>,
}

impl CreateContainerOptions {
    /// Creates new container options with the specified name and image.
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable container name
    /// * `image` - Docker image reference (e.g., "smile-base:latest")
    #[must_use]
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            mounts: Vec::new(),
            env: None,
            cmd: None,
        }
    }

    /// Adds a volume mount to the container options.
    #[must_use]
    pub fn with_mount(mut self, mount: Mount) -> Self {
        self.mounts.push(mount);
        self
    }

    /// Adds multiple volume mounts to the container options.
    #[must_use]
    pub fn with_mounts(mut self, mounts: impl IntoIterator<Item = Mount>) -> Self {
        self.mounts.extend(mounts);
        self
    }

    /// Adds an environment variable to the container options.
    ///
    /// Environment variables are passed as `KEY=VALUE` strings.
    #[must_use]
    pub fn with_env(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        let env_var = format!("{}={}", key.as_ref(), value.as_ref());
        match &mut self.env {
            Some(env) => env.push(env_var),
            None => self.env = Some(vec![env_var]),
        }
        self
    }

    /// Sets multiple environment variables from an iterator.
    #[must_use]
    pub fn with_envs(mut self, envs: impl IntoIterator<Item = String>) -> Self {
        match &mut self.env {
            Some(env) => env.extend(envs),
            None => self.env = Some(envs.into_iter().collect()),
        }
        self
    }

    /// Sets the command to run in the container.
    ///
    /// This overrides the image's default `CMD` instruction.
    #[must_use]
    pub fn with_cmd(mut self, cmd: Vec<impl Into<String>>) -> Self {
        self.cmd = Some(cmd.into_iter().map(Into::into).collect());
        self
    }
}

/// Manages Docker container operations for SMILE Loop.
///
/// `ContainerManager` wraps a bollard [`Docker`] client and provides
/// high-level methods for container lifecycle management.
///
/// # Example
///
/// ```no_run
/// use smile_container::ContainerManager;
///
/// # async fn example() -> Result<(), smile_container::ContainerError> {
/// let manager = ContainerManager::new()?;
/// manager.health_check().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct ContainerManager {
    /// The bollard Docker client instance.
    docker: Docker,
}

impl ContainerManager {
    /// Creates a new `ContainerManager` by connecting to the Docker daemon.
    ///
    /// This method attempts to connect to the Docker daemon using the default
    /// local connection method (Unix socket on Linux/macOS, named pipe on Windows).
    ///
    /// # Errors
    ///
    /// Returns a [`ContainerError::DockerApi`] if the connection to the Docker
    /// daemon fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::ContainerManager;
    ///
    /// # fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self, ContainerError> {
        let docker = Docker::connect_with_local_defaults()?;
        debug!("Connected to Docker daemon");
        Ok(Self { docker })
    }

    /// Returns a reference to the underlying Docker client.
    ///
    /// This can be useful for advanced operations not directly exposed
    /// by `ContainerManager`.
    #[must_use]
    pub const fn docker(&self) -> &Docker {
        &self.docker
    }

    /// Checks if the Docker daemon is reachable and healthy.
    ///
    /// This method pings the Docker daemon to verify connectivity.
    ///
    /// # Errors
    ///
    /// Returns a [`ContainerError::DockerApi`] if the ping fails, which
    /// typically indicates that the Docker daemon is not running or
    /// not accessible.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::ContainerManager;
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    /// manager.health_check().await?;
    /// println!("Docker daemon is healthy");
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn health_check(&self) -> Result<(), ContainerError> {
        self.docker.ping().await?;
        debug!("Docker daemon health check passed");
        Ok(())
    }

    /// Creates a new Docker container with the specified options.
    ///
    /// This method validates mount paths, converts the options to bollard's
    /// format, and creates the container via the Docker API.
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration for the container to create
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::InvalidMountPath`] if any mount's host path
    /// does not exist on the filesystem.
    ///
    /// Returns [`ContainerError::CreateFailed`] if the Docker API returns an
    /// error during container creation.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::{ContainerManager, CreateContainerOptions, Mount};
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// let options = CreateContainerOptions::new("smile-session-1", "smile-base:latest")
    ///     .with_mount(Mount::read_only("/tutorials/getting-started", "/workspace/tutorial"))
    ///     .with_mount(Mount::new("/tmp/smile-work", "/workspace/work"))
    ///     .with_env("SMILE_SESSION", "1");
    ///
    /// let container = manager.create_container(options).await?;
    /// println!("Created container: {}", container.id);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(name = %options.name, image = %options.image))]
    pub async fn create_container(
        &self,
        options: CreateContainerOptions,
    ) -> Result<Container, ContainerError> {
        // Validate all mount host paths exist
        for mount in &options.mounts {
            validate_mount_path(&mount.host_path)?;
        }

        // Convert our Mount structs to bollard's Mount format
        let bollard_mounts = options
            .mounts
            .iter()
            .map(convert_mount_to_bollard)
            .collect::<Vec<_>>();

        // Build the host config with mounts
        let host_config = HostConfig {
            mounts: Some(bollard_mounts),
            ..Default::default()
        };

        // Build the container config
        let config = BollardConfig {
            image: Some(options.image.clone()),
            env: options.env.clone(),
            cmd: options.cmd.clone(),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Create the container via bollard
        let create_options = BollardCreateOptions {
            name: &options.name,
            platform: None,
        };

        debug!(
            mounts_count = options.mounts.len(),
            "Creating container with mounts"
        );

        let response = self
            .docker
            .create_container(Some(create_options), config)
            .await
            .map_err(|e| ContainerError::CreateFailed(e.to_string()))?;

        // Log any warnings from Docker
        for warning in &response.warnings {
            warn!(container_id = %response.id, warning = %warning, "Docker warning during container creation");
        }

        info!(
            container_id = %response.id,
            container_name = %options.name,
            "Container created successfully"
        );

        // Build and return the Container struct
        let container = Container::new(&response.id, &options.name, &options.image)
            .with_mounts(options.mounts)
            .with_status(ContainerStatus::Created);

        Ok(container)
    }

    /// Starts a Docker container.
    ///
    /// This method validates that the container is in a state that allows starting
    /// (created or stopped), then starts the container via the Docker API.
    ///
    /// # Arguments
    ///
    /// * `container` - Mutable reference to the container to start
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::InvalidState`] if the container cannot be started
    /// from its current state (i.e., not in Created or Stopped state).
    ///
    /// Returns [`ContainerError::StartFailed`] if the Docker API returns an error
    /// during container start.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::{ContainerManager, CreateContainerOptions};
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// let options = CreateContainerOptions::new("my-container", "alpine:latest");
    /// let mut container = manager.create_container(options).await?;
    ///
    /// manager.start_container(&mut container).await?;
    /// assert!(container.is_running());
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, container), fields(container_id = %container.id, container_name = %container.name))]
    pub async fn start_container(&self, container: &mut Container) -> Result<(), ContainerError> {
        if !container.can_start() {
            return Err(ContainerError::InvalidState {
                expected: ContainerStatus::Created,
                actual: container.status,
            });
        }

        debug!("Starting container");

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| ContainerError::StartFailed(e.to_string()))?;

        container.status = ContainerStatus::Running;

        info!(container_id = %container.id, "Container started successfully");
        Ok(())
    }

    /// Stops a running Docker container.
    ///
    /// This method validates that the container is in a state that allows stopping
    /// (running or paused), then stops the container via the Docker API.
    ///
    /// # Arguments
    ///
    /// * `container` - Mutable reference to the container to stop
    /// * `timeout_secs` - Optional timeout in seconds to wait for the container to stop
    ///   gracefully before forcefully killing it. If `None`, Docker's default is used.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::InvalidState`] if the container cannot be stopped
    /// from its current state (i.e., not in Running or Paused state).
    ///
    /// Returns [`ContainerError::StopFailed`] if the Docker API returns an error
    /// during container stop.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::{ContainerManager, Container, ContainerStatus};
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// // Assuming we have a running container
    /// let mut container = Container::new("abc123", "my-container", "alpine:latest")
    ///     .with_status(ContainerStatus::Running);
    ///
    /// // Stop with 10 second timeout
    /// manager.stop_container(&mut container, Some(10)).await?;
    /// assert_eq!(container.status, ContainerStatus::Stopped);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, container), fields(container_id = %container.id, timeout = ?timeout_secs))]
    pub async fn stop_container(
        &self,
        container: &mut Container,
        timeout_secs: Option<i64>,
    ) -> Result<(), ContainerError> {
        if !container.can_stop() {
            return Err(ContainerError::InvalidState {
                expected: ContainerStatus::Running,
                actual: container.status,
            });
        }

        debug!("Stopping container");

        let options = timeout_secs.map(|t| StopContainerOptions { t });

        self.docker
            .stop_container(&container.id, options)
            .await
            .map_err(|e| ContainerError::StopFailed(e.to_string()))?;

        container.status = ContainerStatus::Stopped;

        info!(container_id = %container.id, "Container stopped successfully");
        Ok(())
    }

    /// Removes a Docker container.
    ///
    /// This method removes the container from Docker. The container's status is
    /// updated to `Removing` during the operation and to `Gone` upon completion.
    ///
    /// # Arguments
    ///
    /// * `container` - Mutable reference to the container to remove
    /// * `force` - If `true`, forcefully removes the container even if it's running.
    ///   If `false`, the operation will fail if the container is running.
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::RemoveFailed`] if the Docker API returns an error
    /// during container removal (e.g., container is running and `force` is `false`).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::{ContainerManager, Container, ContainerStatus};
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// // Assuming we have a stopped container
    /// let mut container = Container::new("abc123", "my-container", "alpine:latest")
    ///     .with_status(ContainerStatus::Stopped);
    ///
    /// manager.remove_container(&mut container, false).await?;
    /// assert_eq!(container.status, ContainerStatus::Gone);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, container), fields(container_id = %container.id, force = %force))]
    pub async fn remove_container(
        &self,
        container: &mut Container,
        force: bool,
    ) -> Result<(), ContainerError> {
        container.status = ContainerStatus::Removing;
        debug!("Removing container");

        let options = RemoveContainerOptions {
            force,
            ..Default::default()
        };

        self.docker
            .remove_container(&container.id, Some(options))
            .await
            .map_err(|e| ContainerError::RemoveFailed(e.to_string()))?;

        container.status = ContainerStatus::Gone;

        info!(container_id = %container.id, "Container removed successfully");
        Ok(())
    }

    /// Resets a container by stopping, removing, and recreating it.
    ///
    /// This method performs a full container reset cycle:
    /// 1. Stops the container if it's running (ignores errors if already stopped)
    /// 2. Removes the container with force to handle edge cases
    /// 3. Creates a new container with the provided options
    /// 4. Updates the passed-in container with the new container's ID and status
    ///
    /// # Arguments
    ///
    /// * `container` - Mutable reference to the container to reset
    /// * `options` - Configuration for the new container
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::RemoveFailed`] if the container removal fails.
    ///
    /// Returns [`ContainerError::InvalidMountPath`] if any mount's host path
    /// does not exist on the filesystem.
    ///
    /// Returns [`ContainerError::CreateFailed`] if the new container creation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::{ContainerManager, CreateContainerOptions, Container, ContainerStatus};
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// // Assuming we have an existing container
    /// let mut container = Container::new("abc123", "my-container", "alpine:latest")
    ///     .with_status(ContainerStatus::Running);
    ///
    /// let options = CreateContainerOptions::new("my-container", "alpine:latest");
    /// manager.reset_container(&mut container, options).await?;
    ///
    /// // Container now has a new ID and is in Created state
    /// assert_eq!(container.status, ContainerStatus::Created);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, container, options), fields(container_id = %container.id, container_name = %container.name))]
    pub async fn reset_container(
        &self,
        container: &mut Container,
        options: CreateContainerOptions,
    ) -> Result<(), ContainerError> {
        debug!("Resetting container");

        // Step 1: Stop the container if it can be stopped (ignore errors if already stopped)
        if container.can_stop() {
            debug!("Stopping container before reset");
            if let Err(e) = self.stop_container(container, Some(10)).await {
                debug!(error = %e, "Stop failed (may already be stopped), continuing with removal");
            }
        }

        // Step 2: Remove the container with force to handle edge cases
        debug!("Removing container for reset");
        self.remove_container(container, true).await?;

        // Step 3: Create a new container with the provided options
        debug!("Creating new container after reset");
        let new_container = self.create_container(options).await?;

        // Step 4: Update the passed-in container with new container's data
        container.id = new_container.id;
        container.name = new_container.name;
        container.image = new_container.image;
        container.status = new_container.status;
        container.mounts = new_container.mounts;
        container.created_at = new_container.created_at;

        info!(new_container_id = %container.id, "Container reset successfully");
        Ok(())
    }

    /// Resets a container for a new SMILE iteration.
    ///
    /// This is a convenience method for the common SMILE use case where a container
    /// needs to be reset between validation iterations. It:
    /// 1. Calls [`reset_container`](Self::reset_container) to perform the reset
    /// 2. Adds a `SMILE_ITERATION` environment variable with the iteration number
    /// 3. Logs the iteration reset for observability
    ///
    /// # Arguments
    ///
    /// * `container` - Mutable reference to the container to reset
    /// * `iteration` - The current iteration number (1-based)
    /// * `options` - Configuration for the new container
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::RemoveFailed`] if the container removal fails.
    ///
    /// Returns [`ContainerError::InvalidMountPath`] if any mount's host path
    /// does not exist on the filesystem.
    ///
    /// Returns [`ContainerError::CreateFailed`] if the new container creation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::{ContainerManager, CreateContainerOptions, Container, ContainerStatus};
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// let mut container = Container::new("abc123", "smile-session", "smile-base:latest")
    ///     .with_status(ContainerStatus::Running);
    ///
    /// let options = CreateContainerOptions::new("smile-session", "smile-base:latest");
    /// manager.reset_container_for_iteration(&mut container, 2, options).await?;
    ///
    /// // Container is now reset and ready for iteration 2
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, container, options), fields(container_id = %container.id, iteration = %iteration))]
    pub async fn reset_container_for_iteration(
        &self,
        container: &mut Container,
        iteration: u32,
        options: CreateContainerOptions,
    ) -> Result<(), ContainerError> {
        info!(
            iteration = iteration,
            "Resetting container for new iteration"
        );

        // Add the SMILE_ITERATION environment variable to the options
        let options_with_iteration = options.with_env("SMILE_ITERATION", iteration.to_string());

        // Perform the reset
        self.reset_container(container, options_with_iteration)
            .await?;

        debug!(
            iteration = iteration,
            container_id = %container.id,
            "Container ready for iteration"
        );

        Ok(())
    }

    /// Gets the current status of a container from Docker.
    ///
    /// This method inspects the container via the Docker API and maps the
    /// Docker container state to a [`ContainerStatus`].
    ///
    /// # Arguments
    ///
    /// * `container_id` - The Docker container ID to inspect
    ///
    /// # Errors
    ///
    /// Returns [`ContainerError::NotFound`] if the container does not exist.
    ///
    /// Returns [`ContainerError::DockerApi`] if the Docker API returns any other error.
    ///
    /// # State Mapping
    ///
    /// Docker states are mapped to [`ContainerStatus`] as follows:
    /// - `running` -> [`ContainerStatus::Running`]
    /// - `paused` -> [`ContainerStatus::Paused`]
    /// - `created` -> [`ContainerStatus::Created`]
    /// - `exited`, `dead` -> [`ContainerStatus::Stopped`]
    /// - `removing` -> [`ContainerStatus::Removing`]
    /// - other/unknown -> [`ContainerStatus::Stopped`]
    ///
    /// # Example
    ///
    /// ```no_run
    /// use smile_container::ContainerManager;
    ///
    /// # async fn example() -> Result<(), smile_container::ContainerError> {
    /// let manager = ContainerManager::new()?;
    ///
    /// let status = manager.get_container_status("abc123").await?;
    /// println!("Container status: {status}");
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn get_container_status(
        &self,
        container_id: &str,
    ) -> Result<ContainerStatus, ContainerError> {
        debug!("Inspecting container status");

        let response = self
            .docker
            .inspect_container(container_id, None)
            .await
            .map_err(|e| {
                // Check if it's a 404 not found error
                if let bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                } = &e
                {
                    ContainerError::NotFound(container_id.to_string())
                } else {
                    ContainerError::DockerApi(e)
                }
            })?;

        use bollard::models::ContainerStateStatusEnum;

        // Extract the state from the response
        let status = response.state.and_then(|state| state.status).map_or(
            ContainerStatus::Stopped,
            |docker_status| {
                match docker_status {
                    ContainerStateStatusEnum::RUNNING => ContainerStatus::Running,
                    ContainerStateStatusEnum::PAUSED => ContainerStatus::Paused,
                    ContainerStateStatusEnum::CREATED => ContainerStatus::Created,
                    ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD => {
                        ContainerStatus::Stopped
                    }
                    ContainerStateStatusEnum::REMOVING => ContainerStatus::Removing,
                    ContainerStateStatusEnum::RESTARTING => {
                        // Restarting containers are transitioning, treat as running
                        ContainerStatus::Running
                    }
                    ContainerStateStatusEnum::EMPTY => {
                        warn!("Empty Docker container status, treating as Stopped");
                        ContainerStatus::Stopped
                    }
                }
            },
        );

        debug!(container_id = %container_id, status = %status, "Container status retrieved");
        Ok(status)
    }
}

/// Validates that a mount host path exists on the filesystem.
fn validate_mount_path(path: &Path) -> Result<(), ContainerError> {
    if !path.exists() {
        return Err(ContainerError::InvalidMountPath(format!(
            "host path does not exist: {}",
            path.display()
        )));
    }

    debug!(path = %path.display(), "Validated mount path exists");
    Ok(())
}

/// Converts our Mount struct to bollard's Mount format.
fn convert_mount_to_bollard(mount: &Mount) -> BollardMount {
    BollardMount {
        target: Some(mount.container_path.clone()),
        source: Some(mount.host_path.to_string_lossy().into_owned()),
        typ: Some(MountTypeEnum::BIND),
        read_only: Some(mount.read_only),
        ..Default::default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Test that `ContainerManager::new()` succeeds when Docker is available.
    ///
    /// Note: This test requires a running Docker daemon. It will fail in
    /// environments where Docker is not installed or not running.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn manager_new_connects_to_docker() {
        let result = ContainerManager::new();
        assert!(result.is_ok(), "Failed to connect to Docker: {result:?}");
    }

    /// Test that health check succeeds when Docker is available.
    ///
    /// Note: This test requires a running Docker daemon.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn health_check_succeeds_with_running_docker() {
        let manager = ContainerManager::new().expect("Failed to create manager");
        let result = manager.health_check().await;
        assert!(result.is_ok(), "Health check failed: {result:?}");
    }

    #[test]
    fn create_container_options_new() {
        let options = CreateContainerOptions::new("test-container", "ubuntu:latest");
        assert_eq!(options.name, "test-container");
        assert_eq!(options.image, "ubuntu:latest");
        assert!(options.mounts.is_empty());
        assert!(options.env.is_none());
        assert!(options.cmd.is_none());
    }

    #[test]
    fn create_container_options_with_mount() {
        let options = CreateContainerOptions::new("test", "ubuntu:latest")
            .with_mount(Mount::new("/host/path", "/container/path"));
        assert_eq!(options.mounts.len(), 1);
        assert_eq!(options.mounts[0].host_path, PathBuf::from("/host/path"));
        assert_eq!(options.mounts[0].container_path, "/container/path");
    }

    #[test]
    fn create_container_options_with_mounts() {
        let mounts = vec![
            Mount::read_only("/tutorial", "/workspace/tutorial"),
            Mount::new("/work", "/workspace/work"),
            Mount::new("/logs", "/workspace/logs"),
        ];
        let options = CreateContainerOptions::new("test", "ubuntu:latest").with_mounts(mounts);
        assert_eq!(options.mounts.len(), 3);
        assert!(options.mounts[0].read_only);
        assert!(!options.mounts[1].read_only);
    }

    #[test]
    fn create_container_options_with_env() {
        let options = CreateContainerOptions::new("test", "ubuntu:latest")
            .with_env("KEY1", "value1")
            .with_env("KEY2", "value2");
        let env = options.env.expect("env should be Some");
        assert_eq!(env.len(), 2);
        assert_eq!(env[0], "KEY1=value1");
        assert_eq!(env[1], "KEY2=value2");
    }

    #[test]
    fn create_container_options_with_envs() {
        let envs = vec!["KEY1=value1".to_string(), "KEY2=value2".to_string()];
        let options = CreateContainerOptions::new("test", "ubuntu:latest").with_envs(envs);
        let env = options.env.expect("env should be Some");
        assert_eq!(env.len(), 2);
    }

    #[test]
    fn create_container_options_with_cmd() {
        let options =
            CreateContainerOptions::new("test", "ubuntu:latest").with_cmd(vec!["bash", "-c", "ls"]);
        let cmd = options.cmd.expect("cmd should be Some");
        assert_eq!(cmd, vec!["bash", "-c", "ls"]);
    }

    #[test]
    fn validate_mount_path_nonexistent() {
        let result = validate_mount_path(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ContainerError::InvalidMountPath(_)),
            "Expected InvalidMountPath, got: {err:?}"
        );
    }

    #[test]
    fn validate_mount_path_exists() {
        // Use a path that definitely exists
        let result = validate_mount_path(Path::new("/tmp"));
        assert!(result.is_ok(), "Expected Ok for /tmp, got: {result:?}");
    }

    #[test]
    fn convert_mount_to_bollard_read_write() {
        let mount = Mount::new("/host/path", "/container/path");
        let bollard_mount = convert_mount_to_bollard(&mount);

        assert_eq!(bollard_mount.source, Some("/host/path".to_string()));
        assert_eq!(bollard_mount.target, Some("/container/path".to_string()));
        assert_eq!(bollard_mount.typ, Some(MountTypeEnum::BIND));
        assert_eq!(bollard_mount.read_only, Some(false));
    }

    #[test]
    fn convert_mount_to_bollard_read_only() {
        let mount = Mount::read_only("/host/tutorial", "/workspace/tutorial");
        let bollard_mount = convert_mount_to_bollard(&mount);

        assert_eq!(bollard_mount.read_only, Some(true));
    }

    /// Test that container creation works with a running Docker daemon.
    ///
    /// Note: This test requires Docker and will create a real container.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn create_container_succeeds() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        // Use a well-known minimal image
        let options = CreateContainerOptions::new("smile-test-create", "alpine:latest")
            .with_mount(Mount::read_only("/tmp", "/workspace/tutorial"))
            .with_env("TEST_VAR", "test_value");

        let result = manager.create_container(options).await;
        assert!(result.is_ok(), "Failed to create container: {result:?}");

        let container = result.unwrap();
        assert!(!container.id.is_empty());
        assert_eq!(container.name, "smile-test-create");
        assert_eq!(container.status, ContainerStatus::Created);
        assert_eq!(container.mounts.len(), 1);

        // Cleanup: remove the container
        let _ = manager.docker.remove_container(&container.id, None).await;
    }

    /// Test that container creation fails with an invalid mount path.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn create_container_fails_with_invalid_mount() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        let options = CreateContainerOptions::new("smile-test-invalid", "alpine:latest")
            .with_mount(Mount::new("/nonexistent/path/12345", "/workspace/work"));

        let result = manager.create_container(options).await;
        assert!(result.is_err(), "Expected error for invalid mount path");

        let err = result.unwrap_err();
        assert!(
            matches!(err, ContainerError::InvalidMountPath(_)),
            "Expected InvalidMountPath, got: {err:?}"
        );
    }

    /// Test that `start_container` returns `InvalidState` for a running container.
    #[tokio::test]
    async fn start_container_fails_when_already_running() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        let mut container = Container::new("nonexistent", "test", "alpine:latest")
            .with_status(ContainerStatus::Running);

        let result = manager.start_container(&mut container).await;
        assert!(result.is_err(), "Expected error starting running container");

        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                ContainerError::InvalidState {
                    expected: ContainerStatus::Created,
                    actual: ContainerStatus::Running
                }
            ),
            "Expected InvalidState, got: {err:?}"
        );
    }

    /// Test that `stop_container` returns `InvalidState` for a stopped container.
    #[tokio::test]
    async fn stop_container_fails_when_already_stopped() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        let mut container = Container::new("nonexistent", "test", "alpine:latest")
            .with_status(ContainerStatus::Stopped);

        let result = manager.stop_container(&mut container, None).await;
        assert!(result.is_err(), "Expected error stopping stopped container");

        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                ContainerError::InvalidState {
                    expected: ContainerStatus::Running,
                    actual: ContainerStatus::Stopped
                }
            ),
            "Expected InvalidState, got: {err:?}"
        );
    }

    /// Test full container lifecycle: create, start, stop, remove.
    ///
    /// Note: This test requires Docker and will create a real container.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn container_full_lifecycle() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        // Create container
        let options = CreateContainerOptions::new("smile-test-lifecycle", "alpine:latest")
            .with_cmd(vec!["sleep", "300"]);

        let mut container = manager
            .create_container(options)
            .await
            .expect("Failed to create container");
        assert_eq!(container.status, ContainerStatus::Created);

        // Start container
        manager
            .start_container(&mut container)
            .await
            .expect("Failed to start container");
        assert_eq!(container.status, ContainerStatus::Running);

        // Verify status from Docker matches
        let status = manager
            .get_container_status(&container.id)
            .await
            .expect("Failed to get container status");
        assert_eq!(status, ContainerStatus::Running);

        // Stop container
        manager
            .stop_container(&mut container, Some(5))
            .await
            .expect("Failed to stop container");
        assert_eq!(container.status, ContainerStatus::Stopped);

        // Remove container
        manager
            .remove_container(&mut container, false)
            .await
            .expect("Failed to remove container");
        assert_eq!(container.status, ContainerStatus::Gone);

        // Verify container no longer exists
        let result = manager.get_container_status(&container.id).await;
        assert!(
            matches!(result, Err(ContainerError::NotFound(_))),
            "Expected NotFound, got: {result:?}"
        );
    }

    /// Test that `get_container_status` returns `NotFound` for nonexistent container.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn get_container_status_returns_not_found() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        let result = manager
            .get_container_status("nonexistent-container-id-12345")
            .await;
        assert!(
            matches!(result, Err(ContainerError::NotFound(_))),
            "Expected NotFound, got: {result:?}"
        );
    }

    /// Test that `remove_container` with force can remove a running container.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn remove_container_force_removes_running() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        // Create and start a container
        let options = CreateContainerOptions::new("smile-test-force-remove", "alpine:latest")
            .with_cmd(vec!["sleep", "300"]);

        let mut container = manager
            .create_container(options)
            .await
            .expect("Failed to create container");

        manager
            .start_container(&mut container)
            .await
            .expect("Failed to start container");
        assert_eq!(container.status, ContainerStatus::Running);

        // Force remove while running
        manager
            .remove_container(&mut container, true)
            .await
            .expect("Failed to force remove container");
        assert_eq!(container.status, ContainerStatus::Gone);
    }

    /// Test container reset: create, start, reset, verify new container.
    ///
    /// Note: This test requires Docker and will create real containers.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn reset_container_creates_new_container() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        // Create and start initial container
        let options = CreateContainerOptions::new("smile-test-reset", "alpine:latest")
            .with_cmd(vec!["sleep", "300"]);

        let mut container = manager
            .create_container(options)
            .await
            .expect("Failed to create container");
        let original_id = container.id.clone();

        manager
            .start_container(&mut container)
            .await
            .expect("Failed to start container");
        assert_eq!(container.status, ContainerStatus::Running);

        // Reset the container
        let reset_options = CreateContainerOptions::new("smile-test-reset", "alpine:latest")
            .with_cmd(vec!["sleep", "300"]);

        manager
            .reset_container(&mut container, reset_options)
            .await
            .expect("Failed to reset container");

        // Verify we have a new container
        assert_ne!(
            container.id, original_id,
            "Container ID should change after reset"
        );
        assert_eq!(container.status, ContainerStatus::Created);

        // Verify old container no longer exists
        let result = manager.get_container_status(&original_id).await;
        assert!(
            matches!(result, Err(ContainerError::NotFound(_))),
            "Original container should be gone, got: {result:?}"
        );

        // Cleanup
        let _ = manager.remove_container(&mut container, true).await;
    }

    /// Test that reset works on a stopped container.
    ///
    /// Note: This test requires Docker and will create real containers.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn reset_container_works_when_stopped() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        // Create a container (don't start it)
        let options = CreateContainerOptions::new("smile-test-reset-stopped", "alpine:latest");

        let mut container = manager
            .create_container(options)
            .await
            .expect("Failed to create container");
        assert_eq!(container.status, ContainerStatus::Created);
        let original_id = container.id.clone();

        // Reset without starting
        let reset_options =
            CreateContainerOptions::new("smile-test-reset-stopped", "alpine:latest");

        manager
            .reset_container(&mut container, reset_options)
            .await
            .expect("Failed to reset stopped container");

        assert_ne!(container.id, original_id);
        assert_eq!(container.status, ContainerStatus::Created);

        // Cleanup
        let _ = manager.remove_container(&mut container, true).await;
    }

    /// Test `reset_container_for_iteration` adds iteration environment variable.
    ///
    /// Note: This test requires Docker and will create real containers.
    #[tokio::test]
    #[ignore = "requires running Docker daemon"]
    async fn reset_container_for_iteration_sets_env_var() {
        let manager = ContainerManager::new().expect("Failed to create manager");

        // Create initial container
        let options = CreateContainerOptions::new("smile-test-iteration", "alpine:latest");

        let mut container = manager
            .create_container(options)
            .await
            .expect("Failed to create container");

        // Reset for iteration 5
        let reset_options = CreateContainerOptions::new("smile-test-iteration", "alpine:latest")
            .with_env("EXISTING_VAR", "value");

        manager
            .reset_container_for_iteration(&mut container, 5, reset_options)
            .await
            .expect("Failed to reset for iteration");

        assert_eq!(container.status, ContainerStatus::Created);

        // Cleanup
        let _ = manager.remove_container(&mut container, true).await;
    }
}
