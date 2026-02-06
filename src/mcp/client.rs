//! MCP client for connecting to MCP servers
//!
//! Supports stdio transport (spawning a subprocess).

use serde_json::Value;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use super::protocol::{McpRequest, McpResponse, McpTool, McpToolResult};
use crate::error::{Error, Result};

/// MCP client for communicating with an MCP server
pub struct McpClient {
    /// Server process (for stdio transport)
    #[allow(dead_code)]
    child: Mutex<Child>,
    /// Stdin writer
    stdin: Mutex<tokio::process::ChildStdin>,
    /// Stdout reader
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
    /// Request ID counter
    next_id: AtomicU64,
    /// Server name
    name: String,
}

impl McpClient {
    /// Connect to an MCP server via stdio transport
    ///
    /// Spawns the given command as a subprocess and communicates via stdin/stdout.
    pub async fn connect_stdio(command: &str) -> Result<Self> {
        Self::connect_stdio_with_args(command, &[]).await
    }

    /// Connect to an MCP server via stdio with arguments
    pub async fn connect_stdio_with_args(command: &str, args: &[&str]) -> Result<Self> {
        debug!("Connecting to MCP server: {} {:?}", command, args);

        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Connection(format!("Failed to spawn MCP server '{}': {}", command, e)))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| Error::Connection("Failed to capture MCP server stdin".to_string()))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| Error::Connection("Failed to capture MCP server stdout".to_string()))?;

        let client = McpClient {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            next_id: AtomicU64::new(1),
            name: command.to_string(),
        };

        // Initialize the connection
        client.initialize().await?;

        Ok(client)
    }

    /// Send a request and read the response
    async fn send_request(&self, request: McpRequest) -> Result<McpResponse> {
        let json = serde_json::to_string(&request)
            .map_err(|e| Error::InvalidInput(format!("Failed to serialize MCP request: {}", e)))?;

        debug!("MCP request -> {}: {}", self.name, json);

        // Write request
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(json.as_bytes()).await
                .map_err(|e| Error::Connection(format!("Failed to write to MCP server: {}", e)))?;
            stdin.write_all(b"\n").await
                .map_err(|e| Error::Connection(format!("Failed to write newline to MCP server: {}", e)))?;
            stdin.flush().await
                .map_err(|e| Error::Connection(format!("Failed to flush MCP server stdin: {}", e)))?;
        }

        // Read response
        let mut line = String::new();
        {
            let mut stdout = self.stdout.lock().await;
            stdout.read_line(&mut line).await
                .map_err(|e| Error::Connection(format!("Failed to read from MCP server: {}", e)))?;
        }

        debug!("MCP response <- {}: {}", self.name, line.trim());

        let response: McpResponse = serde_json::from_str(line.trim())
            .map_err(|e| Error::InvalidInput(format!("Failed to parse MCP response: {} (raw: {})", e, line.trim())))?;

        if let Some(ref err) = response.error {
            return Err(Error::Provider(format!(
                "MCP error from {}: {} (code {})",
                self.name, err.message, err.code
            )));
        }

        Ok(response)
    }

    /// Initialize the MCP connection
    async fn initialize(&self) -> Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = McpRequest::initialize(id);
        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            debug!("MCP server {} initialized: {:?}", self.name, result);
        }

        Ok(())
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = McpRequest::list_tools(id);
        let response = self.send_request(request).await?;

        let result = response.result.unwrap_or_default();
        let tools: Vec<McpTool> = result.get("tools")
            .and_then(|t| serde_json::from_value(t.clone()).ok())
            .unwrap_or_default();

        debug!("MCP server {} has {} tools", self.name, tools.len());
        Ok(tools)
    }

    /// Call a tool on the MCP server
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<McpToolResult> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = McpRequest::call_tool(id, name, arguments);
        let response = self.send_request(request).await?;

        let result = response.result.unwrap_or_default();
        let tool_result: McpToolResult = serde_json::from_value(result)
            .map_err(|e| Error::InvalidInput(format!("Failed to parse MCP tool result: {}", e)))?;

        if tool_result.is_error {
            warn!("MCP tool {} returned error", name);
        }

        Ok(tool_result)
    }

    /// Get the server name
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // The child process will be killed when dropped
        debug!("Dropping MCP client for {}", self.name);
    }
}
