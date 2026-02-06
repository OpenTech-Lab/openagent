//! Install package skill
//!
//! Detects the OS package manager and installs a package.
//! Handles apt, yum, dnf, pacman, brew, etc.

use async_trait::async_trait;

use super::traits::{Skill, SkillContext, SkillResult, SkillStep};
use crate::error::Result;
use crate::tools::{ToolCall, ToolResult as TResult};

/// Skill for installing OS packages
pub struct InstallPackageSkill;

impl InstallPackageSkill {
    pub fn new() -> Self {
        InstallPackageSkill
    }

    /// Run a system command and return the result
    async fn run_cmd(ctx: &SkillContext<'_>, command: &str, args: &[&str]) -> Result<TResult> {
        let call = ToolCall {
            id: "skill".to_string(),
            name: "system_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "args": args,
            }),
        };
        ctx.tools.execute(&call).await
    }
}

impl Default for InstallPackageSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Skill for InstallPackageSkill {
    fn name(&self) -> &str {
        "install_package"
    }

    fn description(&self) -> &str {
        "Install an OS package using the appropriate package manager. Automatically detects apt, yum, dnf, pacman, or brew."
    }

    fn required_tools(&self) -> Vec<&str> {
        vec!["system_command"]
    }

    async fn execute(&self, ctx: &SkillContext<'_>) -> Result<SkillResult> {
        let package = ctx.params.get("package")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if package.is_empty() {
            return Ok(SkillResult::failure(
                "No package name provided",
                vec![SkillStep::err("Validation", "Missing 'package' parameter")],
            ));
        }

        let mut steps = Vec::new();

        // Step 1: Detect package manager
        let pkg_managers = [
            ("apt", "sudo", &["apt", "update"][..]),
            ("yum", "sudo", &["yum", "check-update"][..]),
            ("dnf", "sudo", &["dnf", "check-update"][..]),
            ("pacman", "sudo", &["pacman", "-Sy"][..]),
            ("brew", "brew", &["update"][..]),
        ];

        let mut detected_pm: Option<&str> = None;
        for (pm, _cmd, _args) in &pkg_managers {
            let check = Self::run_cmd(ctx, "which", &[pm]).await?;
            if check.success {
                detected_pm = Some(pm);
                steps.push(SkillStep::ok(
                    format!("Detected package manager: {}", pm),
                    check.content.unwrap_or_default(),
                ));
                break;
            }
        }

        let pm = match detected_pm {
            Some(pm) => pm,
            None => {
                steps.push(SkillStep::err("Detection", "No supported package manager found"));
                return Ok(SkillResult::failure("No package manager found", steps));
            }
        };

        // Step 2: Update package index
        let update_result = match pm {
            "apt" => Self::run_cmd(ctx, "sudo", &["apt", "update"]).await?,
            "yum" => Self::run_cmd(ctx, "sudo", &["yum", "check-update"]).await?,
            "dnf" => Self::run_cmd(ctx, "sudo", &["dnf", "check-update"]).await?,
            "pacman" => Self::run_cmd(ctx, "sudo", &["pacman", "-Sy"]).await?,
            "brew" => Self::run_cmd(ctx, "brew", &["update"]).await?,
            _ => unreachable!(),
        };
        steps.push(SkillStep {
            description: "Update package index".to_string(),
            success: update_result.success,
            output: update_result.content.unwrap_or_default(),
        });

        // Step 3: Install package
        let install_result = match pm {
            "apt" => Self::run_cmd(ctx, "sudo", &["apt", "install", "-y", package]).await?,
            "yum" => Self::run_cmd(ctx, "sudo", &["yum", "install", "-y", package]).await?,
            "dnf" => Self::run_cmd(ctx, "sudo", &["dnf", "install", "-y", package]).await?,
            "pacman" => Self::run_cmd(ctx, "sudo", &["pacman", "-S", "--noconfirm", package]).await?,
            "brew" => Self::run_cmd(ctx, "brew", &["install", package]).await?,
            _ => unreachable!(),
        };
        let install_success = install_result.success;
        steps.push(SkillStep {
            description: format!("Install {}", package),
            success: install_success,
            output: install_result.content.unwrap_or_default(),
        });

        if install_success {
            Ok(SkillResult::success(
                format!("Successfully installed {} via {}", package, pm),
                steps,
            ))
        } else {
            Ok(SkillResult::failure(
                format!("Failed to install {} via {}", package, pm),
                steps,
            ))
        }
    }
}
