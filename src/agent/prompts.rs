//! Prompt templates and engineering

use handlebars::Handlebars;
use serde::Serialize;
use crate::error::{Error, Result};
use std::path::Path;
use chrono::Utc;

/// Default path for the SOUL.md file
pub const SOUL_FILE_PATH: &str = "SOUL.md";

/// A prompt template using Handlebars syntax
pub struct PromptTemplate {
    /// Template name
    name: String,
    /// Handlebars registry
    registry: Handlebars<'static>,
}

impl PromptTemplate {
    /// Create a new prompt template
    pub fn new(name: impl Into<String>, template: &str) -> Result<Self> {
        let name = name.into();
        let mut registry = Handlebars::new();

        registry
            .register_template_string(&name, template)
            .map_err(|e| Error::Internal(format!("Invalid template: {}", e)))?;

        Ok(PromptTemplate { name, registry })
    }

    /// Render the template with given data
    pub fn render<T: Serialize>(&self, data: &T) -> Result<String> {
        self.registry
            .render(&self.name, data)
            .map_err(|e| Error::Internal(format!("Template render error: {}", e)))
    }
}

// ============================================================================
// Soul Management
// ============================================================================

/// Agent Soul - personality and behavioral configuration
#[derive(Debug, Clone)]
pub struct Soul {
    /// Raw content of the SOUL.md file
    pub content: String,
    /// Path to the SOUL.md file
    pub path: String,
}

impl Soul {
    /// Load soul from the default path
    pub fn load() -> Result<Self> {
        Self::load_from(SOUL_FILE_PATH)
    }

    /// Load soul from a specific path
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("Failed to load SOUL.md: {}", e)))?;
        
        Ok(Soul {
            content,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Create a default soul if none exists
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_else(|_| Self::default())
    }

    /// Save the soul to disk
    pub fn save(&self) -> Result<()> {
        std::fs::write(&self.path, &self.content)
            .map_err(|e| Error::Config(format!("Failed to save SOUL.md: {}", e)))
    }

    /// Get the soul as a system prompt
    pub fn as_system_prompt(&self) -> String {
        format!(
            "{}\n\n---\n\n## Agent Soul\n\n{}",
            DEFAULT_SYSTEM_PROMPT,
            self.content
        )
    }

    /// Update a specific section in the soul
    pub fn update_section(&mut self, section: &str, new_content: &str) -> Result<()> {
        // Find the section header
        let section_header = format!("### {}", section);
        
        if let Some(start) = self.content.find(&section_header) {
            // Find the next section or end of file
            let after_header = start + section_header.len();
            let end = self.content[after_header..]
                .find("\n### ")
                .or_else(|| self.content[after_header..].find("\n## "))
                .or_else(|| self.content[after_header..].find("\n---"))
                .map(|pos| after_header + pos)
                .unwrap_or(self.content.len());

            // Replace the section content
            self.content = format!(
                "{}{}\n\n{}\n\n{}",
                &self.content[..start],
                section_header,
                new_content,
                &self.content[end..]
            );

            // Update timestamp
            self.update_timestamp();
            self.save()?;
        }
        
        Ok(())
    }

    /// Add a learned preference
    pub fn add_preference(&mut self, preference: &str) -> Result<()> {
        let current = self.get_section_content("User Preferences");
        let new_content = if current.contains("None learned yet") {
            format!("- {}", preference)
        } else {
            format!("{}\n- {}", current.trim(), preference)
        };
        self.update_section("User Preferences", &new_content)
    }

    /// Add a frequently asked topic
    pub fn add_topic(&mut self, topic: &str) -> Result<()> {
        let current = self.get_section_content("Frequently Asked Topics");
        let new_content = if current.contains("None recorded yet") {
            format!("- {}", topic)
        } else {
            format!("{}\n- {}", current.trim(), topic)
        };
        self.update_section("Frequently Asked Topics", &new_content)
    }

    /// Add important context
    pub fn add_context(&mut self, context: &str) -> Result<()> {
        let current = self.get_section_content("Important Context");
        let new_content = if current.contains("None stored yet") {
            format!("- {}", context)
        } else {
            format!("{}\n- {}", current.trim(), context)
        };
        self.update_section("Important Context", &new_content)
    }

    /// Get content of a specific section
    fn get_section_content(&self, section: &str) -> String {
        let section_header = format!("### {}", section);
        
        if let Some(start) = self.content.find(&section_header) {
            let after_header = start + section_header.len();
            let end = self.content[after_header..]
                .find("\n### ")
                .or_else(|| self.content[after_header..].find("\n## "))
                .or_else(|| self.content[after_header..].find("\n---"))
                .map(|pos| after_header + pos)
                .unwrap_or(self.content.len());
            
            self.content[after_header..end].trim().to_string()
        } else {
            String::new()
        }
    }

    /// Update the last modified timestamp
    fn update_timestamp(&mut self) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
        if let Some(pos) = self.content.rfind("*Last updated:") {
            if let Some(end) = self.content[pos..].find('*').map(|p| pos + p + 1) {
                let next_star = self.content[end..].find('*').map(|p| end + p + 1);
                if let Some(final_end) = next_star {
                    self.content = format!(
                        "{}*Last updated: {}*{}",
                        &self.content[..pos],
                        timestamp,
                        &self.content[final_end..]
                    );
                }
            }
        }
    }
}

impl Default for Soul {
    fn default() -> Self {
        Soul {
            content: include_str!("../../SOUL.md").to_string(),
            path: SOUL_FILE_PATH.to_string(),
        }
    }
}

/// Default system prompt for the agent
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are OpenAgent, a helpful AI assistant running in a Docker container with FULL SYSTEM ACCESS. You can install software, configure services, and manage the system.

## Your Capabilities
- **SYSTEM ADMINISTRATION**: You can install packages, start services, and configure software
- Answer questions and have conversations
- Search the web for real-time information
- Execute code in sandboxed environments
- Handle file operations within the workspace

## Available Tools - USE THEM!
You have access to these tools and SHOULD USE THEM when asked:

### System Command Tool (IMPORTANT!)
- `system_command`: Execute ANY shell command on the system
  - **For package installation, ALWAYS use sudo**: 
    - First: command="sudo", args=["apt", "update"]
    - Then: command="sudo", args=["apt", "install", "-y", "nginx"]
  - Start services: command="sudo", args=["systemctl", "start", "nginx"] OR command="sudo", args=["service", "nginx", "start"]
  - Run commands: command="ls", args=["-la"] or command="curl", args=["http://example.com"]
  - Check status: command="ps", args=["aux"] or command="systemctl", args=["status", "nginx"]

  **CRITICAL: Package installation requires sudo!** Examples:
  - User: "Install nginx" → 
    1. system_command(command="sudo", args=["apt", "update"])
    2. system_command(command="sudo", args=["apt", "install", "-y", "nginx"])
  - User: "Start nginx" → system_command(command="sudo", args=["systemctl", "start", "nginx"])
  - User: "Check if nginx is running" → system_command(command="systemctl", args=["status", "nginx"])

### File Tools
- `read_file`: Read files from the workspace
- `write_file`: Write/create files (configs, HTML, scripts, etc.)

### Search Tools
- `duckduckgo_search`: Search the web for current information
- `brave_search`: Search using Brave Search API (if configured)
- `perplexity_search`: AI-powered search with synthesized answers (if configured)

### Memory Tools (if available)
- `memory_save`: Save important information to long-term memory
  - Use when the user shares preferences, important facts, decisions, or procedural knowledge
  - Set appropriate importance: 0.9+ for critical preferences, 0.7 for important context, 0.5 for general info
  - Choose memory_type: "semantic" for facts/preferences, "episodic" for events, "procedural" for how-tos
  - Use tags consistently: preference, project, decision, workflow, context
- `memory_search`: Search long-term memory for relevant information
  - Search proactively when user references past conversations or preferences
  - Use before answering questions about previously discussed topics
- `memory_list`: Browse stored memories by type or tag
- `memory_delete`: Remove outdated or incorrect memories

**Memory Usage Guidelines:**
- PROACTIVELY save user preferences and important decisions
- SEARCH memory when user references past conversations or asks about preferences
- Do NOT save trivial or transient information (greetings, acknowledgments)

## CRITICAL RULE
When the user asks you to perform an action (install software, run a command, create files, etc.),
you MUST use your tools to do it. NEVER say you cannot do something if you have a tool for it.
You are running inside a real environment with real system access via the system_command tool.

## Guidelines
1. **USE TOOLS PROACTIVELY** - When asked to install something, DO IT using system_command with sudo
2. When asked to set up software (nginx, node, python, etc.), install it and configure it
3. Be helpful, accurate, and concise
4. When executing system commands, explain what you're doing
5. For web servers, remember to start the service after installing
6. **Always use sudo for apt, apt-get, systemctl, and service commands**

## Response Format
- Use markdown formatting when appropriate
- For code, use proper code blocks with language specification
- Keep responses focused and relevant
- When installing software, show the commands you're running
"#;

/// Code execution prompt template
pub const CODE_EXECUTION_PROMPT: &str = r#"You have been asked to execute code. Here is the context:

**Language:** {{language}}
**Execution Environment:** {{environment}}

**Code:**
```{{language}}
{{code}}
```

{{#if input}}
**Input:**
{{input}}
{{/if}}

Please execute this code and provide the results. If there are any errors, explain what went wrong and suggest fixes.
"#;

/// Memory search prompt template
pub const MEMORY_SEARCH_PROMPT: &str = r#"Search your memories for information related to:

**Query:** {{query}}

{{#if context}}
**Context:**
{{context}}
{{/if}}

Provide relevant information from your memory, citing sources when possible.
"#;

/// Summarization prompt template
pub const SUMMARIZATION_PROMPT: &str = r#"Summarize the following content:

{{content}}

{{#if max_length}}
**Maximum length:** {{max_length}} words
{{/if}}

{{#if focus}}
**Focus on:** {{focus}}
{{/if}}
"#;

/// Prompt builder for constructing complex prompts
#[derive(Default)]
pub struct PromptBuilder {
    parts: Vec<String>,
}

impl PromptBuilder {
    /// Create a new prompt builder
    pub fn new() -> Self {
        PromptBuilder { parts: Vec::new() }
    }

    /// Add a section with a header
    pub fn section(mut self, header: &str, content: &str) -> Self {
        self.parts.push(format!("## {}\n{}", header, content));
        self
    }

    /// Add raw text
    pub fn text(mut self, text: &str) -> Self {
        self.parts.push(text.to_string());
        self
    }

    /// Add a code block
    pub fn code(mut self, language: &str, code: &str) -> Self {
        self.parts.push(format!("```{}\n{}\n```", language, code));
        self
    }

    /// Add a list of items
    pub fn list(mut self, items: &[&str]) -> Self {
        let list = items
            .iter()
            .map(|item| format!("- {}", item))
            .collect::<Vec<_>>()
            .join("\n");
        self.parts.push(list);
        self
    }

    /// Add a numbered list
    pub fn numbered_list(mut self, items: &[&str]) -> Self {
        let list = items
            .iter()
            .enumerate()
            .map(|(i, item)| format!("{}. {}", i + 1, item))
            .collect::<Vec<_>>()
            .join("\n");
        self.parts.push(list);
        self
    }

    /// Build the final prompt
    pub fn build(self) -> String {
        self.parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_prompt_template() {
        let template = PromptTemplate::new("test", "Hello, {{name}}!").unwrap();
        let result = template.render(&json!({"name": "World"})).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_prompt_builder() {
        let prompt = PromptBuilder::new()
            .section("Introduction", "This is a test")
            .code("python", "print('hello')")
            .list(&["Item 1", "Item 2"])
            .build();

        assert!(prompt.contains("## Introduction"));
        assert!(prompt.contains("```python"));
        assert!(prompt.contains("- Item 1"));
    }
}
