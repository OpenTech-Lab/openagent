//! Tools module - Modular tool system for agent capabilities
//!
//! Each tool is a self-contained module that implements the `Tool` trait.
//! Tools are registered into a `ToolRegistry` and made available to the LLM
//! for function calling.
//!
//! ## Built-in Tools
//!
//! - **system_command**: Execute OS commands (with security controls)
//! - **read_file**: Read files from the workspace
//! - **write_file**: Write/create files in the workspace
//! - **duckduckgo_search**: Web search (no API key required)
//! - **brave_search**: Brave Search API (requires API key)
//! - **perplexity_search**: AI-powered search via Perplexity (requires API key)
//!
//! ## Adding a New Tool
//!
//! 1. Create a new file in `src/tools/` (e.g., `my_tool.rs`)
//! 2. Implement the `Tool` trait
//! 3. Add `mod my_tool;` and `pub use` in this file
//! 4. Register it in the binary entry points (gateway.rs, tui.rs)

mod traits;
mod registry;
mod system_command;
mod read_file;
mod write_file;
mod duckduckgo_search;
mod brave_search;
mod perplexity_search;
mod memory;

// Core trait and types
pub use traits::{Tool, ToolResult, ToolCall};

// Registry
pub use registry::ToolRegistry;

// Built-in tools
pub use system_command::SystemCommandTool;
pub use read_file::ReadFileTool;
pub use write_file::WriteFileTool;
pub use duckduckgo_search::DuckDuckGoSearchTool;
pub use brave_search::{BraveSearchTool, BraveSearchConfig};
pub use perplexity_search::{PerplexitySearchTool, PerplexityConfig};

// Memory tools
pub use memory::{MemorySaveTool, MemorySearchTool, MemoryListTool, MemoryDeleteTool};

// Shared types
pub use duckduckgo_search::SearchResult;

/// Format search results for display
pub(crate) fn format_search_results(results: &[SearchResult]) -> String {
    let mut output = String::new();

    for (i, result) in results.iter().enumerate() {
        output.push_str(&format!(
            "{}. **{}**\n   URL: {}\n   {}\n\n",
            i + 1,
            result.title,
            result.url,
            result.snippet
        ));
    }

    output
}

/// URL encoding helper
pub(crate) mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }

    pub fn decode(s: &str) -> Result<String, ()> {
        url::form_urlencoded::parse(s.as_bytes())
            .next()
            .map(|(k, _)| k.to_string())
            .ok_or(())
    }
}
