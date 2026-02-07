//! Task management tools - AI-callable tools for creating, listing, and updating tasks
//!
//! These tools allow the LLM to manage tasks on behalf of users.
//! Tasks are only created when the user explicitly requests something to be tracked,
//! not for every chat message.
//! The agentic loop injects `_user_id` and `_chat_id` into tool arguments before execution.

use async_trait::async_trait;
use serde_json::Value;
use tracing::info;
use uuid::Uuid;

use crate::database::{TaskStore, TaskStatus};
use crate::error::{Error, Result};
use crate::tools::traits::{Tool, ToolResult};

/// Tool to create a new task
pub struct TaskCreateTool {
    store: TaskStore,
}

impl TaskCreateTool {
    pub fn new(store: TaskStore) -> Self {
        TaskCreateTool { store }
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }

    fn description(&self) -> &str {
        "Create a new task to track something the user wants done. Use this when the user explicitly asks you to remember, schedule, or track a task, todo, or action item."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short title for the task (max 100 chars)"
                },
                "description": {
                    "type": "string",
                    "description": "Detailed description of what needs to be done"
                },
                "priority": {
                    "type": "integer",
                    "description": "Priority level: 0 (normal), 1 (high), 2 (urgent). Default: 0"
                }
            },
            "required": ["title", "description"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let user_id = args
            .get("_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let chat_id = args
            .get("_chat_id")
            .and_then(|v| v.as_i64());

        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'title' parameter".into()))?;

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'description' parameter".into()))?;

        let priority = args
            .get("priority")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let task = self
            .store
            .create(user_id, chat_id, title, description, priority)
            .await?;

        info!("Task created: {} (id={})", title, task.id);

        Ok(ToolResult::success(format!(
            "Task created successfully.\nID: {}\nTitle: {}\nPriority: {}\nStatus: pending",
            task.id, task.title, task.priority
        )))
    }
}

/// Tool to list tasks
pub struct TaskListTool {
    store: TaskStore,
}

impl TaskListTool {
    pub fn new(store: TaskStore) -> Self {
        TaskListTool { store }
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn description(&self) -> &str {
        "List tasks for the current user. Can filter by status."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["pending", "processing", "finish", "fail", "cancel", "stop"],
                    "description": "Filter by task status. Omit to show all tasks."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of tasks to return. Default: 20"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let user_id = args
            .get("_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let status = args
            .get("status")
            .and_then(|v| v.as_str())
            .map(TaskStatus::from_str);

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(20);

        let tasks = self
            .store
            .get_by_user(user_id, status, limit)
            .await?;

        if tasks.is_empty() {
            return Ok(ToolResult::success("No tasks found."));
        }

        let mut output = format!("Found {} task(s):\n\n", tasks.len());
        for task in &tasks {
            output.push_str(&format!(
                "- [{}] {} (priority: {}, id: {})\n  {}\n",
                task.status,
                task.title,
                task.priority,
                &task.id.to_string()[..8],
                if task.description.len() > 100 {
                    format!("{}...", &task.description[..100])
                } else {
                    task.description.clone()
                }
            ));
            if let Some(ref result) = task.result {
                output.push_str(&format!("  Result: {}\n", truncate_str(result, 200)));
            }
            if let Some(ref err) = task.error_message {
                output.push_str(&format!("  Error: {}\n", truncate_str(err, 200)));
            }
            output.push('\n');
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool to update a task's status
pub struct TaskUpdateTool {
    store: TaskStore,
}

impl TaskUpdateTool {
    pub fn new(store: TaskStore) -> Self {
        TaskUpdateTool { store }
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "task_update"
    }

    fn description(&self) -> &str {
        "Update a task's status. Use this to mark tasks as finished, failed, or cancelled."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task UUID (or first 8 characters)"
                },
                "action": {
                    "type": "string",
                    "enum": ["finish", "fail", "cancel", "stop"],
                    "description": "Action to perform on the task"
                },
                "result": {
                    "type": "string",
                    "description": "Result message (for finish action)"
                },
                "error": {
                    "type": "string",
                    "description": "Error message (for fail action)"
                }
            },
            "required": ["task_id", "action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let task_id_str = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'task_id' parameter".into()))?;

        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'action' parameter".into()))?;

        // Try to parse as UUID, or look up by prefix
        let task_id = if let Ok(id) = Uuid::parse_str(task_id_str) {
            id
        } else {
            // Try to find by prefix - get user's tasks and match
            let user_id = args
                .get("_user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let tasks = self.store.get_by_user(user_id, None, 100).await?;
            let matched = tasks
                .iter()
                .find(|t| t.id.to_string().starts_with(task_id_str));

            match matched {
                Some(t) => t.id,
                None => {
                    return Ok(ToolResult::failure(format!(
                        "No task found matching '{}'",
                        task_id_str
                    )));
                }
            }
        };

        // Verify task exists
        let task = self.store.get(task_id).await?;
        let task = match task {
            Some(t) => t,
            None => {
                return Ok(ToolResult::failure(format!(
                    "Task {} not found",
                    task_id
                )));
            }
        };

        // Check if already terminal
        if task.status_enum().is_terminal() {
            return Ok(ToolResult::failure(format!(
                "Task is already in terminal state: {}",
                task.status
            )));
        }

        match action {
            "finish" => {
                let result = args.get("result").and_then(|v| v.as_str());
                self.store.finish(task_id, result).await?;
                info!("Task {} marked as finished", task_id);
                Ok(ToolResult::success(format!(
                    "Task '{}' marked as finished.",
                    task.title
                )))
            }
            "fail" => {
                let error = args
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No error message provided");
                self.store.fail(task_id, error).await?;
                info!("Task {} marked as failed", task_id);
                Ok(ToolResult::success(format!(
                    "Task '{}' marked as failed: {}",
                    task.title, error
                )))
            }
            "cancel" => {
                self.store.cancel(task_id).await?;
                info!("Task {} cancelled", task_id);
                Ok(ToolResult::success(format!(
                    "Task '{}' cancelled.",
                    task.title
                )))
            }
            "stop" => {
                self.store.stop(task_id).await?;
                info!("Task {} stopped", task_id);
                Ok(ToolResult::success(format!(
                    "Task '{}' stopped.",
                    task.title
                )))
            }
            _ => Ok(ToolResult::failure(format!(
                "Unknown action: {}. Use finish, fail, cancel, or stop.",
                action
            ))),
        }
    }
}

fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() > max {
        &s[..max]
    } else {
        s
    }
}
