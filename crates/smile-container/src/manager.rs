//! Docker container manager for SMILE Loop.
//!
//! This module provides the [`ContainerManager`] struct for managing Docker
//! container lifecycle operations through the bollard crate.

use std::path::Path;

use bollard::container::Config as BollardConfig;
use bollard::container::CreateContainerOptions as BollardCreateOptions;
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
}
