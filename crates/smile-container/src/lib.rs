//! SMILE Container Management
//!
//! Docker container lifecycle management via bollard.
//!
//! This crate provides types and utilities for managing Docker containers
//! used in the SMILE tutorial validation system.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during container operations.
#[derive(Debug, Error)]
pub enum ContainerError {
    /// Failed to create container.
    #[error("failed to create container: {0}")]
    CreateFailed(String),

    /// Failed to start container.
    #[error("failed to start container: {0}")]
    StartFailed(String),

    /// Failed to stop container.
    #[error("failed to stop container: {0}")]
    StopFailed(String),

    /// Failed to remove container.
    #[error("failed to remove container: {0}")]
    RemoveFailed(String),

    /// Container not found.
    #[error("container not found: {0}")]
    NotFound(String),

    /// Docker API error.
    #[error("docker API error: {0}")]
    DockerApi(#[from] bollard::errors::Error),

    /// Invalid container state for the requested operation.
    #[error("invalid container state: expected {expected}, found {actual}")]
    InvalidState {
        /// The expected container state.
        expected: ContainerStatus,
        /// The actual container state.
        actual: ContainerStatus,
    },

    /// Mount path error.
    #[error("invalid mount path: {0}")]
    InvalidMountPath(String),
}

/// Status of a Docker container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContainerStatus {
    /// Container has been created but not started.
    #[default]
    Created,
    /// Container is currently running.
    Running,
    /// Container is paused.
    Paused,
    /// Container has been stopped.
    Stopped,
    /// Container is being removed.
    Removing,
    /// Container no longer exists.
    Gone,
}

impl std::fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Stopped => write!(f, "stopped"),
            Self::Removing => write!(f, "removing"),
            Self::Gone => write!(f, "gone"),
        }
    }
}

/// A mount point binding a host path to a container path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mount {
    /// Path on the host filesystem.
    pub host_path: PathBuf,
    /// Path inside the container.
    pub container_path: String,
    /// Whether the mount is read-only.
    pub read_only: bool,
}

impl Mount {
    /// Creates a new read-write mount.
    #[must_use]
    pub fn new(host_path: impl Into<PathBuf>, container_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            container_path: container_path.into(),
            read_only: false,
        }
    }

    /// Creates a new read-only mount.
    #[must_use]
    pub fn read_only(host_path: impl Into<PathBuf>, container_path: impl Into<String>) -> Self {
        Self {
            host_path: host_path.into(),
            container_path: container_path.into(),
            read_only: true,
        }
    }
}

/// A Docker container managed by SMILE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Container {
    /// Unique identifier assigned by Docker.
    pub id: String,
    /// Human-readable name for the container.
    pub name: String,
    /// Docker image used to create this container.
    pub image: String,
    /// Current status of the container.
    pub status: ContainerStatus,
    /// Volume mounts for the container.
    pub mounts: Vec<Mount>,
    /// Timestamp when the container was created.
    pub created_at: DateTime<Utc>,
}

impl Container {
    /// Creates a new container representation.
    ///
    /// # Arguments
    ///
    /// * `id` - Docker container ID
    /// * `name` - Container name
    /// * `image` - Docker image name
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            image: image.into(),
            status: ContainerStatus::default(),
            mounts: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Adds a mount to the container.
    #[must_use]
    pub fn with_mount(mut self, mount: Mount) -> Self {
        self.mounts.push(mount);
        self
    }

    /// Adds multiple mounts to the container.
    #[must_use]
    pub fn with_mounts(mut self, mounts: impl IntoIterator<Item = Mount>) -> Self {
        self.mounts.extend(mounts);
        self
    }

    /// Sets the container status.
    #[must_use]
    pub const fn with_status(mut self, status: ContainerStatus) -> Self {
        self.status = status;
        self
    }

    /// Returns whether the container is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status == ContainerStatus::Running
    }

    /// Returns whether the container can be started.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        matches!(
            self.status,
            ContainerStatus::Created | ContainerStatus::Stopped
        )
    }

    /// Returns whether the container can be stopped.
    #[must_use]
    pub const fn can_stop(&self) -> bool {
        matches!(
            self.status,
            ContainerStatus::Running | ContainerStatus::Paused
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_status_default_is_created() {
        assert_eq!(ContainerStatus::default(), ContainerStatus::Created);
    }

    #[test]
    fn container_status_serializes_to_snake_case() {
        let status = ContainerStatus::Running;
        let json = serde_json::to_string(&status).unwrap_or_default();
        assert_eq!(json, r#""running""#);
    }

    #[test]
    fn container_status_display() {
        assert_eq!(ContainerStatus::Created.to_string(), "created");
        assert_eq!(ContainerStatus::Running.to_string(), "running");
        assert_eq!(ContainerStatus::Paused.to_string(), "paused");
        assert_eq!(ContainerStatus::Stopped.to_string(), "stopped");
        assert_eq!(ContainerStatus::Removing.to_string(), "removing");
        assert_eq!(ContainerStatus::Gone.to_string(), "gone");
    }

    #[test]
    fn mount_new_creates_read_write_mount() {
        let mount = Mount::new("/host/path", "/container/path");
        assert!(!mount.read_only);
        assert_eq!(mount.host_path, PathBuf::from("/host/path"));
        assert_eq!(mount.container_path, "/container/path");
    }

    #[test]
    fn mount_read_only_creates_read_only_mount() {
        let mount = Mount::read_only("/host/path", "/container/path");
        assert!(mount.read_only);
    }

    #[test]
    fn container_new_sets_defaults() {
        let container = Container::new("abc123", "test-container", "ubuntu:latest");
        assert_eq!(container.id, "abc123");
        assert_eq!(container.name, "test-container");
        assert_eq!(container.image, "ubuntu:latest");
        assert_eq!(container.status, ContainerStatus::Created);
        assert!(container.mounts.is_empty());
    }

    #[test]
    fn container_with_mount_adds_mount() {
        let container = Container::new("abc123", "test", "ubuntu:latest")
            .with_mount(Mount::new("/host", "/container"));
        assert_eq!(container.mounts.len(), 1);
    }

    #[test]
    fn container_is_running() {
        let container =
            Container::new("abc123", "test", "ubuntu:latest").with_status(ContainerStatus::Running);
        assert!(container.is_running());
    }

    #[test]
    fn container_can_start() {
        let created = Container::new("abc123", "test", "ubuntu:latest");
        assert!(created.can_start());

        let stopped =
            Container::new("abc123", "test", "ubuntu:latest").with_status(ContainerStatus::Stopped);
        assert!(stopped.can_start());

        let running =
            Container::new("abc123", "test", "ubuntu:latest").with_status(ContainerStatus::Running);
        assert!(!running.can_start());
    }

    #[test]
    fn container_can_stop() {
        let running =
            Container::new("abc123", "test", "ubuntu:latest").with_status(ContainerStatus::Running);
        assert!(running.can_stop());

        let paused =
            Container::new("abc123", "test", "ubuntu:latest").with_status(ContainerStatus::Paused);
        assert!(paused.can_stop());

        let stopped =
            Container::new("abc123", "test", "ubuntu:latest").with_status(ContainerStatus::Stopped);
        assert!(!stopped.can_stop());
    }
}
