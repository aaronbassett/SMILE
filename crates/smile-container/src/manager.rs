//! Docker container manager for SMILE Loop.
//!
//! This module provides the [`ContainerManager`] struct for managing Docker
//! container lifecycle operations through the bollard crate.

use bollard::Docker;
use tracing::{debug, instrument};

use crate::ContainerError;

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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

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
}
