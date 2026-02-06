//! Sandbox module - Secure code execution environments
//!
//! Provides three tiers of execution:
//! - Sandbox Mode: WebAssembly virtual machine using Wasmtime (recommended)
//! - OS Mode: Full system access with user authentication (sudo available)
//! - Container Mode: Ephemeral Docker containers (most secure)

mod container;
mod executor;
mod os_sandbox;
mod wasm;

pub use container::ContainerExecutor;
pub use executor::{CodeExecutor, ExecutionResult, ExecutionRequest, Language};
pub use os_sandbox::OsSandbox;
pub use wasm::WasmExecutor;

use crate::config::{ExecutionEnv, SandboxConfig};
use crate::error::Result;

/// Create an executor based on the configuration
pub async fn create_executor(config: &SandboxConfig) -> Result<Box<dyn CodeExecutor>> {
    match config.execution_env {
        ExecutionEnv::Os => {
            // OS mode: full system access, no path restrictions
            let executor = OsSandbox::new_unrestricted(config.allowed_dir.clone());
            Ok(Box::new(executor))
        }
        ExecutionEnv::Sandbox => {
            let executor = WasmExecutor::new()?;
            Ok(Box::new(executor))
        }
        ExecutionEnv::Container => {
            let executor = ContainerExecutor::new(&config.container).await?;
            Ok(Box::new(executor))
        }
    }
}
