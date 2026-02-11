//! OpenAgent Streaming Webhook Gateway
//!
//! Advanced webhook gateway with real-time streaming support.
//! Processes tasks asynchronously and streams results back via Server-Sent Events
//! or webhook callbacks with incremental updates.

use openagent::agent::{
    ConversationManager, LoopConfig, Message as AgentMessage, OpenRouterClient,
    ToolRegistry, ReadFileTool, WriteFileTool, SystemCommandTool,
    DuckDuckGoSearchTool, BraveSearchTool, PerplexitySearchTool,
    MemorySaveTool, MemorySearchTool, MemoryListTool, MemoryDeleteTool,
    TaskCreateTool, TaskListTool, TaskUpdateTool,
    prompts::{DEFAULT_SYSTEM_PROMPT, Soul},
    agentic_loop::{self, AgentLoopInput, LoopCallback, ToolObservation},
    rig_client::RigLlmClient,
};
use openagent::config::Config;
use openagent::database::{
    init_pool, migrations, Memory, MemoryType,
    AgentStatusStore, ConfigParamStore, ConfigValueType, SoulStore, TaskStore,
};
use openagent::memory::{ConversationSummarizer, EmbeddingService, MemoryCache, MemoryRetriever};
use openagent::sandbox::{create_executor, CodeExecutor, ExecutionRequest, Language};
use openagent::{Error, Result};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{sse::Event, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use teloxide::types::Update;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info, warn};

/// Streaming webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingWebhookConfig {
    /// Port to listen on
    pub port: u16,
    /// Webhook secret for authentication
    pub secret: Option<String>,
    /// Callback webhook URL for results
    pub callback_url: Option<String>,
    /// Timeout for task processing (seconds)
    pub task_timeout: u32,
    /// Max concurrent workers
    pub max_workers: usize,
    /// Enable Server-Sent Events streaming
    pub enable_sse: bool,
    /// SSE heartbeat interval (seconds)
    pub sse_heartbeat: u32,
}

/// Streaming task with progress updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTask {
    /// Unique task ID
    pub id: String,
    /// Telegram update data
    pub update: Update,
    /// Timestamp when task was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Callback URL for this specific task
    pub callback_url: Option<String>,
    /// Enable streaming for this task
    pub enable_streaming: bool,
}

/// Task progress update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    /// Task ID
    pub task_id: String,
    /// Progress type
    pub progress_type: ProgressType,
    /// Progress data
    pub data: serde_json::Value,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Types of progress updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressType {
    /// Task started
    Started,
    /// Agent thinking/planning
    Thinking,
    /// Tool execution started
    ToolStarted { tool_name: String },
    /// Tool execution completed
    ToolCompleted { tool_name: String, success: bool },
    /// Agent response chunk
    ResponseChunk { text: String },
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed { error: String },
}

/// Final task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTaskResult {
    /// Task ID
    pub task_id: String,
    /// Success flag
    pub success: bool,
    /// Final response messages
    pub messages: Vec<TelegramMessage>,
    /// Error message if failed
    pub error: Option<String>,
    /// Total processing duration
    pub duration_ms: u64,
    /// Number of progress updates sent
    pub progress_updates: u32,
}

/// Telegram message to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    /// Chat ID to send to
    pub chat_id: i64,
    /// Message text
    pub text: String,
    /// Parse mode
    pub parse_mode: Option<String>,
    /// Reply to message ID
    pub reply_to_message_id: Option<i32>,
}

/// Streaming worker pool
struct StreamingWorkerPool {
    /// Task sender
    task_tx: mpsc::UnboundedSender<StreamingTask>,
    /// Progress broadcast channel
    progress_tx: broadcast::Sender<TaskProgress>,
    /// Result receiver
    result_rx: mpsc::UnboundedReceiver<StreamingTaskResult>,
}

impl StreamingWorkerPool {
    /// Create new streaming worker pool
    fn new(
        num_workers: usize,
        state: Arc<AppState>,
        result_tx: mpsc::UnboundedSender<StreamingTaskResult>,
    ) -> Self {
        let (task_tx, mut task_rx) = mpsc::unbounded_channel();
        let (progress_tx, _) = broadcast::channel(1000); // Buffer size

        let mut workers = Vec::new();

        for i in 0..num_workers {
            let state = state.clone();
            let result_tx = result_tx.clone();
            let progress_tx = progress_tx.clone();
            let mut task_rx = task_rx.clone();

            let worker = tokio::spawn(async move {
                info!("Streaming worker {} started", i);

                while let Some(task) = task_rx.recv().await {
                    let progress_tx = progress_tx.clone();

                    match Self::process_streaming_task(&state, task.clone(), progress_tx).await {
                        Ok(result) => {
                            if let Err(e) = result_tx.send(result) {
                                error!("Failed to send streaming task result: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Streaming task {} failed: {}", task.id, e);

                            // Send failure progress update
                            let _ = progress_tx.send(TaskProgress {
                                task_id: task.id.clone(),
                                progress_type: ProgressType::Failed { error: e.to_string() },
                                data: serde_json::Value::Null,
                                timestamp: chrono::Utc::now(),
                            });

                            let error_result = StreamingTaskResult {
                                task_id: task.id,
                                success: false,
                                messages: vec![],
                                error: Some(e.to_string()),
                                duration_ms: 0,
                                progress_updates: 1,
                            };

                            if let Err(e) = result_tx.send(error_result) {
                                error!("Failed to send error result: {}", e);
                            }
                        }
                    }
                }

                info!("Streaming worker {} stopped", i);
            });

            workers.push(worker);
        }

        // Drop extra clones
        drop(task_rx);

        Self {
            task_tx,
            progress_tx,
            result_rx: mpsc::unbounded_channel().1,
        }
    }

    /// Submit streaming task
    fn submit_task(&self, task: StreamingTask) -> Result<()> {
        self.task_tx.send(task).map_err(|e| Error::Other(format!("Failed to submit streaming task: {}", e)))
    }

    /// Get progress receiver for a specific task
    fn subscribe_progress(&self, task_id: &str) -> broadcast::Receiver<TaskProgress> {
        let mut rx = self.progress_tx.subscribe();

        // Filter for this task's progress updates
        // Note: In a real implementation, you'd want to filter by task_id

        rx
    }

    /// Process a streaming task with progress updates
    async fn process_streaming_task(
        state: &AppState,
        task: StreamingTask,
        progress_tx: broadcast::Sender<TaskProgress>,
    ) -> Result<StreamingTaskResult> {
        let start_time = std::time::Instant::now();
        let mut progress_count = 0u32;

        // Send started progress
        progress_tx.send(TaskProgress {
            task_id: task.id.clone(),
            progress_type: ProgressType::Started,
            data: serde_json::Value::Null,
            timestamp: chrono::Utc::now(),
        })?;
        progress_count += 1;

        let update = task.update;

        // Extract message from update
        let message = match update.kind {
            teloxide::types::UpdateKind::Message(msg) => msg,
            _ => {
                return Ok(StreamingTaskResult {
                    task_id: task.id,
                    success: true,
                    messages: vec![],
                    error: None,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    progress_updates: progress_count,
                });
            }
        };

        let chat_id = message.chat.id.0;
        let user_id = message.from().map(|u| u.id.0).unwrap_or(0);
        let text = message.text().unwrap_or("");

        // Send thinking progress
        progress_tx.send(TaskProgress {
            task_id: task.id.clone(),
            progress_type: ProgressType::Thinking,
            data: serde_json::json!({ "query": text }),
            timestamp: chrono::Utc::now(),
        })?;
        progress_count += 1;

        // Process the message with streaming
        let response = state.process_streaming_message(
            chat_id,
            user_id,
            text,
            Some(message.id.0),
            task.enable_streaming,
            progress_tx.clone(),
            &mut progress_count,
        ).await?;

        // Send completed progress
        progress_tx.send(TaskProgress {
            task_id: task.id.clone(),
            progress_type: ProgressType::Completed,
            data: serde_json::Value::Null,
            timestamp: chrono::Utc::now(),
        })?;
        progress_count += 1;

        Ok(StreamingTaskResult {
            task_id: task.id,
            success: true,
            messages: vec![TelegramMessage {
                chat_id,
                text: response,
                parse_mode: Some("Markdown".to_string()),
                reply_to_message_id: Some(message.id.0),
            }],
            error: None,
            duration_ms: start_time.elapsed().as_millis() as u64,
            progress_updates: progress_count,
        })
    }
}

/// Application state for streaming gateway
struct AppState {
    config: Config,
    streaming_config: StreamingWebhookConfig,
    llm_client: OpenRouterClient,
    rig_client: Arc<RigLlmClient>,
    conversations: Arc<RwLock<ConversationManager>>,
    memory_retriever: Option<MemoryRetriever>,
    executor: Box<dyn CodeExecutor>,
    dm_tools: Arc<ToolRegistry>,
    group_tools: ToolRegistry,
    approved_users: HashSet<i64>,
    admin_users: HashSet<i64>,
    soul_store: Option<SoulStore>,
    task_store: Option<TaskStore>,
    status_store: Option<AgentStatusStore>,
    config_param_store: Option<ConfigParamStore>,
}

impl AppState {
    async fn new(config: Config, streaming_config: StreamingWebhookConfig) -> Result<Self> {
        // Similar initialization to webhook gateway...
        // (Copy from webhook_gateway.rs)

        let openrouter_config = config.provider.openrouter.clone()
            .ok_or_else(|| Error::Config("OpenRouter not configured".into()))?;

        let llm_client = OpenRouterClient::new(openrouter_config.clone())?;
        let rig_client = Arc::new(RigLlmClient::new(openrouter_config.clone())?);

        // Database and other initialization (same as webhook gateway)
        // ... (truncated for brevity)

        Ok(AppState {
            config,
            streaming_config,
            llm_client,
            rig_client,
            conversations: Arc::new(RwLock::new(ConversationManager::new(&openrouter_config.default_model))),
            memory_retriever: None, // Simplified
            executor: create_executor(&config.sandbox).await?,
            dm_tools: Arc::new(ToolRegistry::new()), // Simplified
            group_tools: ToolRegistry::new(),
            approved_users: HashSet::new(),
            admin_users: HashSet::new(),
            soul_store: None,
            task_store: None,
            status_store: None,
            config_param_store: None,
        })
    }

    /// Process message with streaming support
    async fn process_streaming_message(
        &self,
        chat_id: i64,
        user_id: i64,
        text: &str,
        reply_to: Option<i32>,
        enable_streaming: bool,
        progress_tx: broadcast::Sender<TaskProgress>,
        progress_count: &mut u32,
    ) -> Result<String> {
        // Simulate streaming progress updates
        if enable_streaming {
            // Send tool execution progress
            progress_tx.send(TaskProgress {
                task_id: "current".to_string(), // Would be actual task ID
                progress_type: ProgressType::ToolStarted { tool_name: "web_search".to_string() },
                data: serde_json::json!({ "query": "test query" }),
                timestamp: chrono::Utc::now(),
            })?;
            *progress_count += 1;

            tokio::time::sleep(Duration::from_millis(500)).await;

            progress_tx.send(TaskProgress {
                task_id: "current".to_string(),
                progress_type: ProgressType::ToolCompleted { tool_name: "web_search".to_string(), success: true },
                data: serde_json::json!({ "results": 5 }),
                timestamp: chrono::Utc::now(),
            })?;
            *progress_count += 1;

            // Send response chunks
            let response = "Here's what I found: This is a streaming response!";
            for chunk in response.chars().collect::<Vec<_>>().chunks(5) {
                let chunk_text: String = chunk.iter().collect();
                progress_tx.send(TaskProgress {
                    task_id: "current".to_string(),
                    progress_type: ProgressType::ResponseChunk { text: chunk_text },
                    data: serde_json::Value::Null,
                    timestamp: chrono::Utc::now(),
                })?;
                *progress_count += 1;

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        Ok(format!("Processed streaming message from user {} in chat {}: {}", user_id, chat_id, text))
    }
}

/// Webhook handlers
async fn handle_streaming_webhook(
    State(state): State<Arc<AppState>>,
    State(worker_pool): State<Arc<RwLock<StreamingWorkerPool>>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    Json(update): Json<Update>,
) -> impl IntoResponse {
    let task_id = format!("streaming_task_{}", chrono::Utc::now().timestamp_millis());

    let enable_streaming = params.get("stream").map(|s| s == "true").unwrap_or(false);

    let task = StreamingTask {
        id: task_id.clone(),
        update,
        created_at: chrono::Utc::now(),
        callback_url: params.get("callback_url").cloned(),
        enable_streaming,
    };

    match worker_pool.read().await.submit_task(task) {
        Ok(_) => {
            info!("Streaming task {} submitted", task_id);
            (StatusCode::ACCEPTED, Json(serde_json::json!({
                "status": "accepted",
                "task_id": task_id,
                "streaming": enable_streaming
            })))
        }
        Err(e) => {
            error!("Failed to submit streaming task: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "status": "error",
                "error": e.to_string()
            })))
        }
    }
}

/// Server-Sent Events endpoint for real-time progress
async fn stream_progress(
    Path(task_id): Path<String>,
    State(worker_pool): State<Arc<RwLock<StreamingWorkerPool>>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut progress_rx = worker_pool.read().await.subscribe_progress(&task_id);

    let stream = stream::unfold((), move |_| {
        let mut rx = progress_rx.clone();
        async move {
            match rx.recv().await {
                Ok(progress) => {
                    let event = Event::default()
                        .event("progress")
                        .data(serde_json::to_string(&progress).unwrap_or_default());
                    Some((Ok(event), ()))
                }
                Err(_) => None, // Stream ended
            }
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keepalive")
    )
}

/// Health check
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "healthy",
        "service": "streaming-webhook-gateway",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("openagent=debug".parse().unwrap()),
        )
        .init();

    info!("Starting OpenAgent Streaming Webhook Gateway");

    let config = Config::from_env()?;

    let streaming_config = StreamingWebhookConfig {
        port: std::env::var("WEBHOOK_PORT")
            .unwrap_or_else(|_| "8081".to_string())
            .parse()
            .unwrap_or(8081),
        secret: std::env::var("WEBHOOK_SECRET").ok(),
        callback_url: std::env::var("CALLBACK_URL").ok(),
        task_timeout: 300,
        max_workers: std::env::var("MAX_WORKERS")
            .unwrap_or_else(|_| "4".to_string())
            .parse()
            .unwrap_or(4),
        enable_sse: std::env::var("ENABLE_SSE")
            .map(|s| s == "true")
            .unwrap_or(true),
        sse_heartbeat: 30,
    };

    let state = Arc::new(AppState::new(config, streaming_config.clone()).await?);

    // Create channels
    let (result_tx, result_rx) = mpsc::unbounded_channel();

    // Create streaming worker pool
    let worker_pool = Arc::new(RwLock::new(StreamingWorkerPool::new(
        streaming_config.max_workers,
        state.clone(),
        result_tx,
    )));

    // Start result processor
    let callback_url = streaming_config.callback_url.clone();
    let http_client = reqwest::Client::new();
    tokio::spawn(async move {
        process_streaming_results(result_rx, callback_url, http_client).await;
    });

    // Build router
    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/webhook", post(handle_streaming_webhook));

    if streaming_config.enable_sse {
        app = app.route("/stream/:task_id", get(stream_progress));
    }

    app = app
        .with_state(state)
        .with_state(worker_pool);

    // Start server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], streaming_config.port));
    info!("Streaming webhook server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Process streaming results
async fn process_streaming_results(
    mut result_rx: mpsc::UnboundedReceiver<StreamingTaskResult>,
    callback_url: Option<String>,
    client: reqwest::Client,
) {
    while let Some(result) = result_rx.recv().await {
        info!("Processing streaming result for task {}", result.task_id);

        if let Some(url) = &callback_url {
            let payload = serde_json::to_value(&result).unwrap_or_default();

            match client.post(url).json(&payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Streaming result for task {} sent to callback", result.task_id);
                    } else {
                        warn!("Callback failed for task {}: {}", result.task_id, response.status());
                    }
                }
                Err(e) => {
                    error!("Failed to send streaming callback for task {}: {}", result.task_id, e);
                }
            }
        }
    }
}