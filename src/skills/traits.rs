//! Core skill traits
//!
//! A Skill is a higher-level capability that can compose multiple tools
//! to accomplish complex tasks.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::error::Result;
use crate::tools::ToolRegistry;

/// Context provided to skills during execution
pub struct SkillContext<'a> {
    /// Access to the tool registry for executing sub-tools
    pub tools: &'a ToolRegistry,
    /// Key-value parameters for the skill
    pub params: HashMap<String, Value>,
}

/// A composable agent skill
#[async_trait]
pub trait Skill: Send + Sync {
    /// Skill name (used for identification and invocation)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// List of tool names this skill depends on
    fn required_tools(&self) -> Vec<&str>;

    /// Execute the skill
    async fn execute(&self, ctx: &SkillContext<'_>) -> Result<SkillResult>;
}

/// Result of a skill execution
#[derive(Debug, Clone)]
pub struct SkillResult {
    /// Whether the skill succeeded
    pub success: bool,
    /// Summary of what was done
    pub summary: String,
    /// Detailed steps that were executed
    pub steps: Vec<SkillStep>,
}

/// Individual step within a skill execution
#[derive(Debug, Clone)]
pub struct SkillStep {
    /// Step description
    pub description: String,
    /// Whether this step succeeded
    pub success: bool,
    /// Output from this step
    pub output: String,
}

impl SkillResult {
    /// Create a successful result
    pub fn success(summary: impl Into<String>, steps: Vec<SkillStep>) -> Self {
        SkillResult {
            success: true,
            summary: summary.into(),
            steps,
        }
    }

    /// Create a failed result
    pub fn failure(summary: impl Into<String>, steps: Vec<SkillStep>) -> Self {
        SkillResult {
            success: false,
            summary: summary.into(),
            steps,
        }
    }
}

impl SkillStep {
    /// Create a successful step
    pub fn ok(description: impl Into<String>, output: impl Into<String>) -> Self {
        SkillStep {
            description: description.into(),
            success: true,
            output: output.into(),
        }
    }

    /// Create a failed step
    pub fn err(description: impl Into<String>, output: impl Into<String>) -> Self {
        SkillStep {
            description: description.into(),
            success: false,
            output: output.into(),
        }
    }
}

/// Registry for skills
pub struct SkillRegistry {
    skills: HashMap<String, Box<dyn Skill>>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// Create a new empty skill registry
    pub fn new() -> Self {
        SkillRegistry {
            skills: HashMap::new(),
        }
    }

    /// Register a skill
    pub fn register<S: Skill + 'static>(&mut self, skill: S) {
        self.skills.insert(skill.name().to_string(), Box::new(skill));
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&dyn Skill> {
        self.skills.get(name).map(|s| s.as_ref())
    }

    /// List all skill names
    pub fn names(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Get skill count
    pub fn count(&self) -> usize {
        self.skills.len()
    }
}
