//! Docker container-based execution
//!
//! Provides isolated execution in ephemeral containers with network isolation.
//! This offers strong security guarantees for complex, environment-dependent tasks.

use async_trait::async_trait;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, WaitContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use futures::StreamExt;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::config::ContainerConfig;
use crate::error::{Error, Result};
use crate::sandbox::executor::{CodeExecutor, ExecutionRequest, ExecutionResult, Language};

/// Docker container executor
pub struct ContainerExecutor {
    /// Docker client
    docker: Docker,
    /// Container configuration
    config: ContainerConfig,
}

impl ContainerExecutor {
    /// Create a new container executor
    pub async fn new(config: &ContainerConfig) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| Error::Container(format!("Failed to connect to Docker: {}", e)))?;

        // Verify connection
        docker
            .ping()
            .await
            .map_err(|e| Error::Container(format!("Docker ping failed: {}", e)))?;

        info!("Container executor connected to Docker");

        let executor = ContainerExecutor {
            docker,
            config: config.clone(),
        };

        // Ensure image is available
        executor.ensure_image().await?;

        Ok(executor)
    }

    /// Ensure the required Docker image is available
    async fn ensure_image(&self) -> Result<()> {
        let images = self
            .docker
            .list_images::<String>(None)
            .await
            .map_err(|e| Error::Container(format!("Failed to list images: {}", e)))?;

        let target_image = &self.config.image;
        let image_exists = images.iter().any(|img| {
            img.repo_tags.iter().any(|tag| tag.contains(target_image))
        });

        if !image_exists {
            info!("Pulling Docker image: {}", self.config.image);

            let options = CreateImageOptions {
                from_image: self.config.image.clone(),
                ..Default::default()
            };

            let mut stream = self.docker.create_image(Some(options), None, None);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(info) => {
                        if let Some(status) = info.status {
                            debug!("Pull status: {}", status);
                        }
                    }
                    Err(e) => {
                        return Err(Error::Container(format!("Failed to pull image: {}", e)));
                    }
                }
            }

            info!("Image pulled successfully");
        }

        Ok(())
    }

    /// Get the command for a language
    fn get_command(&self, language: Language, code: &str) -> Vec<String> {
        match language {
            Language::Python => vec![
                "python3".to_string(),
                "-c".to_string(),
                code.to_string(),
            ],
            Language::JavaScript => vec![
                "node".to_string(),
                "-e".to_string(),
                code.to_string(),
            ],
            Language::Bash => vec![
                "bash".to_string(),
                "-c".to_string(),
                code.to_string(),
            ],
            Language::TypeScript => vec![
                "deno".to_string(),
                "eval".to_string(),
                code.to_string(),
            ],
            Language::Rust => vec![
                "rustc".to_string(),
                "--edition=2021".to_string(),
                "-o".to_string(),
                "/tmp/program".to_string(),
                "-".to_string(),
            ],
            Language::Go => vec![
                "go".to_string(),
                "run".to_string(),
                "-".to_string(),
            ],
        }
    }

    /// Create and run a container for code execution
    async fn run_container(
        &self,
        request: &ExecutionRequest,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();

        let container_name = format!("openagent-exec-{}", uuid::Uuid::new_v4());
        let cmd = self.get_command(request.language, &request.code);

        // Prepare environment variables
        let env: Vec<String> = request
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Parse memory limit
        let memory = parse_memory_limit(&self.config.memory_limit);

        // Container configuration
        let container_config = Config {
            image: Some(self.config.image.clone()),
            cmd: Some(cmd),
            env: Some(env),
            network_disabled: Some(self.config.network == "none"),
            host_config: Some(bollard::service::HostConfig {
                memory,
                nano_cpus: Some((self.config.cpu_limit * 1_000_000_000.0) as i64),
                network_mode: Some(self.config.network.clone()),
                auto_remove: Some(false), // We'll remove manually after getting logs
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create container
        let create_options = CreateContainerOptions {
            name: &container_name,
            platform: None,
        };

        self.docker
            .create_container(Some(create_options), container_config)
            .await
            .map_err(|e| Error::Container(format!("Failed to create container: {}", e)))?;

        debug!("Created container: {}", container_name);

        // Start container
        self.docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| Error::Container(format!("Failed to start container: {}", e)))?;

        // Wait for container with timeout
        let wait_result = tokio::time::timeout(
            request.timeout,
            self.wait_for_container(&container_name),
        )
        .await;

        let execution_time = start.elapsed();

        // Get logs
        let (stdout, stderr) = self.get_container_logs(&container_name).await?;

        // Cleanup container
        self.remove_container(&container_name).await?;

        match wait_result {
            Ok(Ok(exit_code)) => Ok(ExecutionResult {
                success: exit_code == 0,
                exit_code: Some(exit_code),
                stdout,
                stderr,
                execution_time,
                timed_out: false,
                memory_used: None,
            }),
            Ok(Err(e)) => Ok(ExecutionResult {
                success: false,
                exit_code: None,
                stdout,
                stderr: format!("{}\n{}", stderr, e),
                execution_time,
                timed_out: false,
                memory_used: None,
            }),
            Err(_) => {
                warn!("Container execution timed out");
                Ok(ExecutionResult::timeout(stdout, stderr, request.timeout))
            }
        }
    }

    /// Wait for a container to finish
    async fn wait_for_container(&self, name: &str) -> Result<i32> {
        let options = WaitContainerOptions {
            condition: "not-running",
        };

        let mut stream = self.docker.wait_container(name, Some(options));

        if let Some(result) = stream.next().await {
            match result {
                Ok(response) => Ok(response.status_code as i32),
                Err(e) => Err(Error::Container(format!("Wait failed: {}", e))),
            }
        } else {
            Err(Error::Container("Container wait stream ended".to_string()))
        }
    }

    /// Get container logs
    async fn get_container_logs(&self, name: &str) -> Result<(String, String)> {
        let options = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..Default::default()
        };

        let mut stream = self.docker.logs(name, Some(options));

        let mut stdout = String::new();
        let mut stderr = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(LogOutput::StdOut { message }) => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(LogOutput::StdErr { message }) => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                Err(e) => {
                    warn!("Error reading logs: {}", e);
                }
                _ => {}
            }
        }

        Ok((stdout, stderr))
    }

    /// Remove a container
    async fn remove_container(&self, name: &str) -> Result<()> {
        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };

        self.docker
            .remove_container(name, Some(options))
            .await
            .map_err(|e| Error::Container(format!("Failed to remove container: {}", e)))?;

        debug!("Removed container: {}", name);
        Ok(())
    }
}

/// Parse a memory limit string (e.g., "512m", "1g") to bytes
fn parse_memory_limit(limit: &str) -> Option<i64> {
    let limit = limit.to_lowercase();
    let (num_str, unit) = if limit.ends_with("g") || limit.ends_with("gb") {
        (limit.trim_end_matches(|c| c == 'g' || c == 'b'), "g")
    } else if limit.ends_with("m") || limit.ends_with("mb") {
        (limit.trim_end_matches(|c| c == 'm' || c == 'b'), "m")
    } else if limit.ends_with("k") || limit.ends_with("kb") {
        (limit.trim_end_matches(|c| c == 'k' || c == 'b'), "k")
    } else {
        (limit.as_str(), "b")
    };

    let num: i64 = num_str.parse().ok()?;

    Some(match unit {
        "g" => num * 1024 * 1024 * 1024,
        "m" => num * 1024 * 1024,
        "k" => num * 1024,
        _ => num,
    })
}

#[async_trait]
impl CodeExecutor for ContainerExecutor {
    fn name(&self) -> &str {
        "container"
    }

    fn supports_language(&self, language: Language) -> bool {
        // Container mode supports all languages (assuming proper runtime in image)
        matches!(
            language,
            Language::Python
                | Language::JavaScript
                | Language::TypeScript
                | Language::Bash
                | Language::Rust
                | Language::Go
        )
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::Bash,
            Language::Rust,
            Language::Go,
        ]
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult> {
        self.run_container(&request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_limit() {
        assert_eq!(parse_memory_limit("512m"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_limit("1g"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_memory_limit("1024k"), Some(1024 * 1024));
        assert_eq!(parse_memory_limit("1024"), Some(1024));
    }
}
