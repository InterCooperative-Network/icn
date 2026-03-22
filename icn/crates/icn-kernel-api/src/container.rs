//! Container runtime abstraction for the ICN kernel.
//!
//! Defines the kernel-level interface for executing containerized tasks.
//! Implementors provide the actual substrate (containerd, Docker, WASM runtimes).
//!
//! # Meaning Firewall
//!
//! Policy oracles translate domain semantics (trust scores, governance quotas) into
//! `ContainerSpec` resource limits. The runtime executes without domain knowledge —
//! the kernel enforces constraints blindly.

use std::collections::HashMap;

/// Specification for a container task to be executed.
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    /// OCI image reference (e.g., `"docker.io/library/alpine:3.18"`)
    pub image: String,
    /// Command and arguments to execute.
    pub command: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Resource limits for this execution.
    pub resources: ResourceLimits,
}

/// Resource limits for container execution.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Memory limit in bytes.
    pub memory_bytes: u64,
    /// CPU shares (relative weight, OCI cgroup v1 semantics).
    pub cpu_shares: u32,
    /// Maximum wall-clock execution time in seconds.
    pub timeout_secs: u64,
}

/// The result of a completed container execution.
#[derive(Debug, Clone)]
pub struct ContainerResult {
    /// Exit code of the container process.
    pub exit_code: i32,
    /// Standard output (may be truncated).
    pub stdout: Vec<u8>,
    /// Standard error (may be truncated).
    pub stderr: Vec<u8>,
    /// Measured resource usage.
    pub resource_usage: ResourceUsage,
}

/// Measured resource usage from a completed container run.
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// Peak resident memory in bytes.
    pub peak_memory_bytes: u64,
    /// CPU time consumed in milliseconds.
    pub cpu_time_ms: u64,
    /// Wall-clock time elapsed in milliseconds.
    pub wall_time_ms: u64,
}

/// Errors produced by container runtime operations.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// The requested image is not locally available and could not be pulled.
    #[error("image not available: {image}")]
    ImageNotAvailable { image: String },

    /// The container exceeded a resource limit (memory, CPU, or timeout).
    #[error("resource limit exceeded: {detail}")]
    ResourceLimitExceeded { detail: String },

    /// The container process failed to start or terminated abnormally.
    #[error("execution failed: {reason}")]
    ExecutionFailed { reason: String },

    /// The runtime substrate is unavailable or unreachable.
    #[error("runtime unavailable")]
    RuntimeUnavailable,
}

/// Kernel-level interface for executing containerized tasks.
///
/// Implementors provide the actual container execution substrate.
/// The kernel uses this trait to run tasks described by `ContainerSpec`
/// without understanding the domain semantics encoded in the resource limits.
#[async_trait::async_trait]
pub trait ContainerRuntime: Send + Sync {
    /// Execute a container according to the given spec.
    ///
    /// Returns the execution result, or an error if the container cannot
    /// be started or exceeds its resource limits.
    async fn run(&self, spec: ContainerSpec) -> Result<ContainerResult, ContainerError>;

    /// Check whether a container image is locally available.
    ///
    /// Returns `true` if the image can be executed without a network pull.
    async fn image_available(&self, image: &str) -> bool;

    /// Return the name of this runtime implementation.
    fn runtime_name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the ContainerRuntime trait is object-safe.
    fn _assert_object_safe(_: &dyn ContainerRuntime) {}

    #[test]
    fn container_spec_constructible() {
        let spec = ContainerSpec {
            image: "alpine:3.18".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
            env: HashMap::new(),
            resources: ResourceLimits {
                memory_bytes: 256 * 1024 * 1024,
                cpu_shares: 512,
                timeout_secs: 30,
            },
        };
        assert_eq!(spec.image, "alpine:3.18");
        assert_eq!(spec.resources.memory_bytes, 256 * 1024 * 1024);
    }

    #[test]
    fn container_result_constructible() {
        let result = ContainerResult {
            exit_code: 0,
            stdout: b"hello\n".to_vec(),
            stderr: vec![],
            resource_usage: ResourceUsage {
                peak_memory_bytes: 1024,
                cpu_time_ms: 50,
                wall_time_ms: 100,
            },
        };
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn container_error_display() {
        let e = ContainerError::ImageNotAvailable {
            image: "missing:latest".to_string(),
        };
        assert!(e.to_string().contains("missing:latest"));

        let e2 = ContainerError::ResourceLimitExceeded {
            detail: "OOM".to_string(),
        };
        assert!(e2.to_string().contains("OOM"));
    }
}
