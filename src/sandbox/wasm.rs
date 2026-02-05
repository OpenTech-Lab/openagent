//! WebAssembly sandbox using Wasmtime
//!
//! Provides high-security code execution in a zero-access virtual machine.
//! This is the recommended execution environment.

use async_trait::async_trait;
use std::time::{Duration, Instant};
use tracing::{debug, info};
use wasmtime::*;

use crate::error::{Error, Result};
use crate::sandbox::executor::{CodeExecutor, ExecutionRequest, ExecutionResult, Language};

/// WebAssembly executor using Wasmtime
pub struct WasmExecutor {
    /// Wasmtime engine
    engine: Engine,
}

impl WasmExecutor {
    /// Create a new Wasm executor
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
        config.consume_fuel(true); // Enable fuel for limiting execution

        let engine = Engine::new(&config)?;

        info!("Wasm executor initialized");
        Ok(WasmExecutor { engine })
    }

    /// Execute Python code using a Python WASM runtime
    async fn execute_python(&self, _request: &ExecutionRequest) -> Result<ExecutionResult> {
        // For now, return a placeholder
        // Full Python WASM support would require embedding a Python WASM runtime
        // like RustPython or Pyodide compiled to WASM

        debug!("Python WASM execution requested");

        Ok(ExecutionResult {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: "Python WASM execution is not yet fully implemented. \
                    Consider using OS or Container mode for Python code.".to_string(),
            execution_time: Duration::from_millis(0),
            timed_out: false,
            memory_used: None,
        })
    }

    /// Execute JavaScript code using a JS WASM runtime
    async fn execute_javascript(&self, _request: &ExecutionRequest) -> Result<ExecutionResult> {
        // For now, return a placeholder
        // Full JS WASM support would require embedding QuickJS or similar

        debug!("JavaScript WASM execution requested");

        Ok(ExecutionResult {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: "JavaScript WASM execution is not yet fully implemented. \
                    Consider using OS or Container mode for JavaScript code.".to_string(),
            execution_time: Duration::from_millis(0),
            timed_out: false,
            memory_used: None,
        })
    }

    /// Execute a raw WASM module
    pub async fn execute_wasm_module(
        &self,
        wasm_bytes: &[u8],
        func_name: &str,
        args: &[Val],
        timeout: Duration,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();

        // Create a store with fuel limits
        let mut store = Store::new(&self.engine, ());

        // Set fuel limit based on timeout (rough approximation)
        let fuel = (timeout.as_millis() * 1000) as u64;
        store.set_fuel(fuel)?;

        // Compile the module
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| Error::Wasm(format!("Failed to compile module: {}", e)))?;

        // Create an instance
        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| Error::Wasm(format!("Failed to instantiate module: {}", e)))?;

        // Get the function
        let func = instance
            .get_func(&mut store, func_name)
            .ok_or_else(|| Error::Wasm(format!("Function '{}' not found", func_name)))?;

        // Prepare result storage
        let result_count = func.ty(&store).results().len();
        let mut results = vec![Val::I32(0); result_count];

        // Call the function
        match func.call(&mut store, args, &mut results) {
            Ok(()) => {
                let execution_time = start.elapsed();
                let fuel_consumed = fuel - store.get_fuel().unwrap_or(0);

                Ok(ExecutionResult {
                    success: true,
                    exit_code: Some(0),
                    stdout: format!("Results: {:?}", results),
                    stderr: String::new(),
                    execution_time,
                    timed_out: false,
                    memory_used: Some(fuel_consumed),
                })
            }
            Err(e) => {
                let execution_time = start.elapsed();

                // Check if it was a fuel exhaustion (timeout)
                if e.to_string().contains("fuel") {
                    Ok(ExecutionResult::timeout(
                        String::new(),
                        "Execution exceeded resource limits".to_string(),
                        timeout,
                    ))
                } else {
                    Ok(ExecutionResult {
                        success: false,
                        exit_code: Some(1),
                        stdout: String::new(),
                        stderr: e.to_string(),
                        execution_time,
                        timed_out: false,
                        memory_used: None,
                    })
                }
            }
        }
    }
}

#[async_trait]
impl CodeExecutor for WasmExecutor {
    fn name(&self) -> &str {
        "wasm"
    }

    fn supports_language(&self, language: Language) -> bool {
        // Currently, we have limited language support in WASM mode
        // Full support would require embedding language runtimes
        matches!(language, Language::Python | Language::JavaScript)
    }

    fn supported_languages(&self) -> Vec<Language> {
        // Note: These are listed as supported but have limited functionality
        vec![Language::Python, Language::JavaScript]
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult> {
        match request.language {
            Language::Python => self.execute_python(&request).await,
            Language::JavaScript => self.execute_javascript(&request).await,
            _ => Ok(ExecutionResult {
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!(
                    "Language {} is not supported in WASM sandbox",
                    request.language
                ),
                execution_time: Duration::from_millis(0),
                timed_out: false,
                memory_used: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_executor_creation() {
        let executor = WasmExecutor::new();
        assert!(executor.is_ok());
    }

    #[test]
    fn test_language_support() {
        let executor = WasmExecutor::new().unwrap();
        assert!(executor.supports_language(Language::Python));
        assert!(executor.supports_language(Language::JavaScript));
        assert!(!executor.supports_language(Language::Rust));
    }
}
