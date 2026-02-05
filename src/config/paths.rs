//! Configuration paths
//!
//! Utilities for resolving configuration file paths.

use std::path::PathBuf;

/// Get the configuration directory
pub fn config_dir() -> PathBuf {
    // Check for explicit override
    if let Ok(dir) = std::env::var("OPENAGENT_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    // Use XDG config directory or fallback
    dirs::config_dir()
        .map(|d| d.join("openagent"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".config").join("openagent"))
                .unwrap_or_else(|| PathBuf::from(".openagent"))
        })
}

/// Get the main configuration file path
pub fn config_path() -> PathBuf {
    // Check for explicit override
    if let Ok(path) = std::env::var("OPENAGENT_CONFIG") {
        return PathBuf::from(path);
    }

    config_dir().join("config.json")
}

/// Get the state directory (for databases, credentials, etc.)
pub fn state_dir() -> PathBuf {
    // Check for explicit override
    if let Ok(dir) = std::env::var("OPENAGENT_STATE_DIR") {
        return PathBuf::from(dir);
    }

    // Use XDG data directory or fallback
    dirs::data_dir()
        .map(|d| d.join("openagent"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local").join("share").join("openagent"))
                .unwrap_or_else(|| PathBuf::from(".openagent"))
        })
}

/// Get the workspace directory
pub fn workspace_dir() -> PathBuf {
    // Check for explicit override
    if let Ok(dir) = std::env::var("OPENAGENT_WORKSPACE") {
        return PathBuf::from(dir);
    }

    state_dir().join("workspace")
}

/// Get the credentials directory
#[allow(dead_code)]
pub fn credentials_dir() -> PathBuf {
    state_dir().join("credentials")
}

/// Get the cache directory
#[allow(dead_code)]
pub fn cache_dir() -> PathBuf {
    // Check for explicit override
    if let Ok(dir) = std::env::var("OPENAGENT_CACHE_DIR") {
        return PathBuf::from(dir);
    }

    dirs::cache_dir()
        .map(|d| d.join("openagent"))
        .unwrap_or_else(|| state_dir().join("cache"))
}

/// Get the logs directory
#[allow(dead_code)]
pub fn logs_dir() -> PathBuf {
    state_dir().join("logs")
}

/// Ensure a directory exists
#[allow(dead_code)]
pub fn ensure_dir(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Ensure all required directories exist
#[allow(dead_code)]
pub fn ensure_all_dirs() -> std::io::Result<()> {
    ensure_dir(&config_dir())?;
    ensure_dir(&state_dir())?;
    ensure_dir(&workspace_dir())?;
    ensure_dir(&credentials_dir())?;
    ensure_dir(&cache_dir())?;
    ensure_dir(&logs_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_paths() {
        // Just ensure these don't panic
        let _ = config_dir();
        let _ = config_path();
        let _ = state_dir();
        let _ = workspace_dir();
    }
}
