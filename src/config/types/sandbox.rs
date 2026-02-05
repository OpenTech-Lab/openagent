//! Sandbox configuration types
//!
//! Configuration for code execution sandboxes (OS, Wasm, Container)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Execution environment
    #[serde(default)]
    pub execution_env: ExecutionEnv,
    /// Allowed directory for file operations
    #[serde(default = "default_allowed_dir")]
    pub allowed_dir: PathBuf,
    /// Container configuration
    #[serde(default)]
    pub container: ContainerConfig,
    /// Wasm configuration
    #[serde(default)]
    pub wasm: WasmConfig,
    /// OS sandbox configuration
    #[serde(default)]
    pub os: OsSandboxConfig,
    /// Default timeout for execution
    #[serde(default = "default_timeout")]
    pub default_timeout_secs: u64,
    /// Maximum output size in bytes
    #[serde(default = "default_max_output")]
    pub max_output_bytes: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        SandboxConfig {
            execution_env: ExecutionEnv::default(),
            allowed_dir: default_allowed_dir(),
            container: ContainerConfig::default(),
            wasm: WasmConfig::default(),
            os: OsSandboxConfig::default(),
            default_timeout_secs: default_timeout(),
            max_output_bytes: default_max_output(),
        }
    }
}

fn default_allowed_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".openagent").join("workspace"))
        .unwrap_or_else(|| PathBuf::from("./workspace"))
}

fn default_timeout() -> u64 {
    30
}

fn default_max_output() -> usize {
    1024 * 1024 // 1MB
}

/// Execution environment type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionEnv {
    /// OS sandbox (restricted paths)
    Os,
    /// WebAssembly sandbox (recommended)
    #[default]
    Sandbox,
    /// Container sandbox (Docker)
    Container,
}

impl std::str::FromStr for ExecutionEnv {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "os" => Ok(ExecutionEnv::Os),
            "sandbox" | "wasm" => Ok(ExecutionEnv::Sandbox),
            "container" | "docker" => Ok(ExecutionEnv::Container),
            _ => Err(crate::error::Error::Config(format!(
                "Invalid execution environment: {}. Valid: os, sandbox, container",
                s
            ))),
        }
    }
}

impl std::fmt::Display for ExecutionEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionEnv::Os => write!(f, "os"),
            ExecutionEnv::Sandbox => write!(f, "sandbox"),
            ExecutionEnv::Container => write!(f, "container"),
        }
    }
}

/// Container (Docker) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Docker image to use
    #[serde(default = "default_image")]
    pub image: String,
    /// Network mode
    #[serde(default = "default_network")]
    pub network: String,
    /// Memory limit
    #[serde(default = "default_memory")]
    pub memory_limit: String,
    /// CPU limit (number of CPUs)
    #[serde(default = "default_cpu")]
    pub cpu_limit: f64,
    /// Enable GPU access
    #[serde(default)]
    pub enable_gpu: bool,
    /// Additional volumes to mount
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    /// Environment variables
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        ContainerConfig {
            image: default_image(),
            network: default_network(),
            memory_limit: default_memory(),
            cpu_limit: default_cpu(),
            enable_gpu: false,
            volumes: Vec::new(),
            env: std::collections::HashMap::new(),
        }
    }
}

fn default_image() -> String {
    "python:3.12-slim".to_string()
}

fn default_network() -> String {
    "none".to_string()
}

fn default_memory() -> String {
    "512m".to_string()
}

fn default_cpu() -> f64 {
    1.0
}

/// Volume mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Host path
    pub host: PathBuf,
    /// Container path
    pub container: String,
    /// Read-only
    #[serde(default)]
    pub readonly: bool,
}

/// WebAssembly sandbox configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WasmConfig {
    /// Maximum memory in pages (64KB each)
    #[serde(default = "default_wasm_memory")]
    pub max_memory_pages: u32,
    /// Enable WASI
    #[serde(default = "default_true")]
    pub enable_wasi: bool,
    /// Allowed WASI directories
    #[serde(default)]
    pub wasi_dirs: Vec<PathBuf>,
    /// Fuel limit (for execution limiting)
    #[serde(default = "default_fuel")]
    pub fuel_limit: u64,
}

fn default_wasm_memory() -> u32 {
    256 // 16MB
}

fn default_true() -> bool {
    true
}

fn default_fuel() -> u64 {
    1_000_000_000
}

/// OS sandbox configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsSandboxConfig {
    /// Drop privileges to this user
    pub run_as_user: Option<String>,
    /// Use seccomp filtering
    #[serde(default)]
    pub use_seccomp: bool,
    /// Allowed executables
    #[serde(default)]
    pub allowed_executables: Vec<String>,
    /// Denied executables
    #[serde(default)]
    pub denied_executables: Vec<String>,
}

/// Sandbox mode for sessions
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    /// No sandboxing
    #[default]
    Off,
    /// Sandbox non-main sessions
    NonMain,
    /// Sandbox all sessions
    All,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_env_parsing() {
        assert_eq!("os".parse::<ExecutionEnv>().unwrap(), ExecutionEnv::Os);
        assert_eq!(
            "sandbox".parse::<ExecutionEnv>().unwrap(),
            ExecutionEnv::Sandbox
        );
        assert_eq!(
            "docker".parse::<ExecutionEnv>().unwrap(),
            ExecutionEnv::Container
        );
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.execution_env, ExecutionEnv::Sandbox);
        assert_eq!(config.default_timeout_secs, 30);
    }
}
