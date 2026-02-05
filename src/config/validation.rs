//! Configuration validation
//!
//! Validates configuration and reports issues.

use super::types::Config;

/// Result of configuration validation
#[derive(Debug, Clone)]
pub struct ConfigValidationResult {
    /// Whether the config is valid
    pub valid: bool,
    /// Validation errors (critical)
    pub errors: Vec<ValidationIssue>,
    /// Validation warnings (non-critical)
    pub warnings: Vec<ValidationIssue>,
}

impl ConfigValidationResult {
    /// Create a valid result
    pub fn valid() -> Self {
        ConfigValidationResult {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add an error
    pub fn with_error(mut self, issue: ValidationIssue) -> Self {
        self.valid = false;
        self.errors.push(issue);
        self
    }

    /// Add a warning
    pub fn with_warning(mut self, issue: ValidationIssue) -> Self {
        self.warnings.push(issue);
        self
    }
}

/// A validation issue
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Path to the config field
    pub path: String,
    /// Issue message
    pub message: String,
    /// Suggested fix
    pub suggestion: Option<String>,
}

impl ValidationIssue {
    /// Create a new issue
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        ValidationIssue {
            path: path.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Add a suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Validate the configuration
pub fn validate_config(config: &Config) -> ConfigValidationResult {
    let mut result = ConfigValidationResult::valid();

    // Validate provider configuration
    result = validate_provider_config(config, result);

    // Validate channel configurations
    result = validate_channel_config(config, result);

    // Validate storage configuration
    result = validate_storage_config(config, result);

    // Validate sandbox configuration
    result = validate_sandbox_config(config, result);

    result
}

fn validate_provider_config(config: &Config, mut result: ConfigValidationResult) -> ConfigValidationResult {
    // Check that at least one provider is configured
    let has_provider = config.provider.openrouter.is_some()
        || config.provider.anthropic.is_some()
        || config.provider.openai.is_some()
        || !config.provider.custom.is_empty();

    if !has_provider {
        result = result.with_warning(
            ValidationIssue::new(
                "provider",
                "No LLM provider configured. Agent will not be able to generate responses.",
            )
            .with_suggestion("Set OPENROUTER_API_KEY environment variable or configure provider.openrouter in config"),
        );
    }

    result
}

fn validate_channel_config(config: &Config, mut result: ConfigValidationResult) -> ConfigValidationResult {
    // Check that at least one channel is configured (warning only)
    let has_channel = config.channels.telegram.is_some()
        || config.channels.discord.is_some()
        || config.channels.slack.is_some()
        || config.channels.whatsapp.is_some()
        || config.channels.webchat.enabled;

    if !has_channel {
        result = result.with_warning(
            ValidationIssue::new(
                "channels",
                "No messaging channel configured. Agent will only be accessible via CLI.",
            )
            .with_suggestion("Configure at least one channel (telegram, discord, slack, or webchat)"),
        );
    }

    result
}

fn validate_storage_config(config: &Config, mut result: ConfigValidationResult) -> ConfigValidationResult {
    use super::types::storage::StorageBackendType;

    // Validate PostgreSQL config if selected
    if config.storage.backend == StorageBackendType::Postgres && config.storage.postgres.is_none() {
        result = result.with_error(
            ValidationIssue::new(
                "storage.postgres",
                "PostgreSQL backend selected but not configured",
            )
            .with_suggestion("Set DATABASE_URL environment variable or configure storage.postgres"),
        );
    }

    result
}

fn validate_sandbox_config(config: &Config, mut result: ConfigValidationResult) -> ConfigValidationResult {
    use super::types::sandbox::ExecutionEnv;

    // Check that allowed_dir exists
    if !config.sandbox.allowed_dir.exists() {
        result = result.with_warning(
            ValidationIssue::new(
                "sandbox.allowed_dir",
                format!(
                    "Sandbox directory does not exist: {}",
                    config.sandbox.allowed_dir.display()
                ),
            )
            .with_suggestion("Create the directory or change sandbox.allowed_dir"),
        );
    }

    // Check container configuration
    if config.sandbox.execution_env == ExecutionEnv::Container {
        if config.sandbox.container.image.is_empty() {
            result = result.with_error(
                ValidationIssue::new(
                    "sandbox.container.image",
                    "Container execution selected but no image specified",
                )
                .with_suggestion("Set sandbox.container.image to a valid Docker image"),
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_default_config() {
        let config = Config::default();
        let result = validate_config(&config);

        // Default config should have warnings but no errors
        assert!(result.errors.is_empty());
    }
}
