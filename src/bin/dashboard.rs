//! OpenAgent Dashboard - Web UI for monitoring agent status, tasks, soul, config, and memories.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use clap::Parser;
use openagent::config::Config;
use openagent::database::{
    AgentStatusStore, ConfigParam, ConfigParamStore, Memory, MemoryStore, SoulSection, SoulStore,
    TaskStore, AgentTask,
};
use openagent::database::{init_pool, migrations};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::info;
use uuid::Uuid;

/// Embedded dashboard HTML
const DASHBOARD_HTML: &str = include_str!("../../static/dashboard.html");

// ---- CLI ----

#[derive(Parser)]
#[command(name = "openagent-dashboard", about = "OpenAgent Web Dashboard")]
struct Args {
    /// Bind address
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Port
    #[arg(long, short, default_value = "3000")]
    port: u16,
}

// ---- App State ----

#[derive(Clone)]
struct DashboardState {
    pool: PgPool,
    status_store: AgentStatusStore,
    task_store: TaskStore,
    soul_store: SoulStore,
    config_store: ConfigParamStore,
    #[allow(dead_code)]
    memory_store: MemoryStore,
}

// ---- Error Handling ----

struct AppError(openagent::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = StatusCode::INTERNAL_SERVER_ERROR;
        let body = Json(serde_json::json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}

impl From<openagent::Error> for AppError {
    fn from(err: openagent::Error) -> Self {
        AppError(err)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError(openagent::Error::Database(err))
    }
}

// ---- Response Types ----

#[derive(Serialize)]
struct StatusResponse {
    agent: AgentStatusInfo,
    tasks: TaskStats,
    memories: MemoryStats,
    soul_sections: i64,
    config_params: i64,
    version: String,
}

#[derive(Serialize)]
struct AgentStatusInfo {
    status: String,
    current_task_id: Option<Uuid>,
    last_heartbeat: chrono::DateTime<chrono::Utc>,
    last_scheduler_run: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct TaskStats {
    pending: i64,
    processing: i64,
    finish: i64,
    fail: i64,
    cancel: i64,
    stop: i64,
    total: i64,
}

#[derive(Serialize)]
struct MemoryStats {
    total: i64,
    by_type: HashMap<String, i64>,
}

// ---- Query Params ----

#[derive(Deserialize)]
struct TasksQuery {
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
struct MemoriesQuery {
    search: Option<String>,
    #[serde(rename = "type")]
    memory_type: Option<String>,
    tag: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
struct ConfigQuery {
    category: Option<String>,
}

// ---- Handlers ----

async fn serve_index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], DASHBOARD_HTML)
}

async fn get_status(State(state): State<DashboardState>) -> Result<Json<StatusResponse>, AppError> {
    // Agent status
    let agent_row = state.status_store.get().await?;

    // Task counts
    let task_counts = state.task_store.count_by_status().await?;
    let get_count = |s: &str| -> i64 { *task_counts.get(s).unwrap_or(&0) };
    let tasks = TaskStats {
        pending: get_count("pending"),
        processing: get_count("processing"),
        finish: get_count("finish"),
        fail: get_count("fail"),
        cancel: get_count("cancel"),
        stop: get_count("stop"),
        total: task_counts.values().sum(),
    };

    // Memory counts
    let mem_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT memory_type, COUNT(*) FROM memories GROUP BY memory_type",
    )
    .fetch_all(&state.pool)
    .await?;
    let by_type: HashMap<String, i64> = mem_rows.into_iter().collect();
    let mem_total: i64 = by_type.values().sum();

    // Soul section count
    let soul_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM agent_soul_sections")
            .fetch_one(&state.pool)
            .await?;

    // Config param count
    let config_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM config_params")
            .fetch_one(&state.pool)
            .await?;

    Ok(Json(StatusResponse {
        agent: AgentStatusInfo {
            status: agent_row.status,
            current_task_id: agent_row.current_task_id,
            last_heartbeat: agent_row.last_heartbeat,
            last_scheduler_run: agent_row.last_scheduler_run,
        },
        tasks,
        memories: MemoryStats {
            total: mem_total,
            by_type,
        },
        soul_sections: soul_count.0,
        config_params: config_count.0,
        version: openagent::VERSION.to_string(),
    }))
}

async fn list_tasks(
    State(state): State<DashboardState>,
    Query(params): Query<TasksQuery>,
) -> Result<Json<Vec<AgentTask>>, AppError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let status = params.status.and_then(|s| {
        use openagent::database::TaskStatus;
        match s.as_str() {
            "pending" => Some(TaskStatus::Pending),
            "processing" => Some(TaskStatus::Processing),
            "finish" => Some(TaskStatus::Finish),
            "fail" => Some(TaskStatus::Fail),
            "cancel" => Some(TaskStatus::Cancel),
            "stop" => Some(TaskStatus::Stop),
            _ => None,
        }
    });

    let tasks = state.task_store.list_all(status, limit, offset).await?;
    Ok(Json(tasks))
}

async fn task_stats(
    State(state): State<DashboardState>,
) -> Result<Json<HashMap<String, i64>>, AppError> {
    let counts = state.task_store.count_by_status().await?;
    Ok(Json(counts))
}

async fn get_task(
    State(state): State<DashboardState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Option<AgentTask>>, AppError> {
    let task = state.task_store.get(id).await?;
    Ok(Json(task))
}

async fn list_soul_sections(
    State(state): State<DashboardState>,
) -> Result<Json<Vec<SoulSection>>, AppError> {
    let sections = state.soul_store.get_all_sections().await?;
    Ok(Json(sections))
}

async fn get_soul_section(
    State(state): State<DashboardState>,
    Path(name): Path<String>,
) -> Result<Json<Option<SoulSection>>, AppError> {
    let section = state.soul_store.get_section(&name).await?;
    Ok(Json(section))
}

async fn list_config_params(
    State(state): State<DashboardState>,
    Query(params): Query<ConfigQuery>,
) -> Result<Json<Vec<ConfigParam>>, AppError> {
    let mut config_params = state
        .config_store
        .get_all(params.category.as_deref())
        .await?;

    // Mask secret values
    for p in &mut config_params {
        if p.is_secret {
            p.value = "********".to_string();
        }
    }

    Ok(Json(config_params))
}

async fn list_memories(
    State(state): State<DashboardState>,
    Query(params): Query<MemoriesQuery>,
) -> Result<Json<Vec<Memory>>, AppError> {
    let limit = params.limit.unwrap_or(30).min(200);
    let offset = params.offset.unwrap_or(0);

    let memories: Vec<Memory> = if let Some(search) = &params.search {
        if search.is_empty() {
            fetch_memories(&state.pool, params.memory_type.as_deref(), params.tag.as_deref(), limit, offset).await?
        } else {
            // Full-text search across all users
            let words: Vec<&str> = search.split_whitespace().collect();
            let tsquery = words.join(" & ");
            sqlx::query_as::<_, Memory>(r#"
                SELECT id, user_id, content, summary, importance, tags, memory_type,
                       metadata, source, created_at, updated_at, accessed_at, access_count
                FROM memories
                WHERE search_vector @@ to_tsquery('simple', $1)
                  AND ($2::text IS NULL OR memory_type = $2)
                  AND ($3::text IS NULL OR $3 = ANY(tags))
                ORDER BY ts_rank(search_vector, to_tsquery('simple', $1)) DESC
                LIMIT $4 OFFSET $5
            "#)
            .bind(&tsquery)
            .bind(params.memory_type.as_deref())
            .bind(params.tag.as_deref())
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
        }
    } else {
        fetch_memories(&state.pool, params.memory_type.as_deref(), params.tag.as_deref(), limit, offset).await?
    };

    Ok(Json(memories))
}

async fn fetch_memories(
    pool: &PgPool,
    memory_type: Option<&str>,
    tag: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Memory>, AppError> {
    let memories: Vec<Memory> = sqlx::query_as(r#"
        SELECT id, user_id, content, summary, importance, tags, memory_type,
               metadata, source, created_at, updated_at, accessed_at, access_count
        FROM memories
        WHERE ($1::text IS NULL OR memory_type = $1)
          AND ($2::text IS NULL OR $2 = ANY(tags))
        ORDER BY importance DESC, created_at DESC
        LIMIT $3 OFFSET $4
    "#)
    .bind(memory_type)
    .bind(tag)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(memories)
}

async fn memory_stats(
    State(state): State<DashboardState>,
) -> Result<Json<MemoryStats>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT memory_type, COUNT(*) FROM memories GROUP BY memory_type",
    )
    .fetch_all(&state.pool)
    .await?;

    let by_type: HashMap<String, i64> = rows.into_iter().collect();
    let total: i64 = by_type.values().sum();

    Ok(Json(MemoryStats { total, by_type }))
}

async fn get_memory(
    State(state): State<DashboardState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Option<Memory>>, AppError> {
    let memory: Option<Memory> = sqlx::query_as(r#"
        SELECT id, user_id, content, summary, importance, tags, memory_type,
               metadata, source, created_at, updated_at, accessed_at, access_count
        FROM memories WHERE id = $1
    "#)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(Json(memory))
}

// ---- Router ----

fn build_router(state: DashboardState) -> Router {
    let api = Router::new()
        .route("/status", get(get_status))
        .route("/tasks", get(list_tasks))
        .route("/tasks/stats", get(task_stats))
        .route("/tasks/{id}", get(get_task))
        .route("/soul", get(list_soul_sections))
        .route("/soul/{name}", get(get_soul_section))
        .route("/config", get(list_config_params))
        .route("/memories", get(list_memories))
        .route("/memories/stats", get(memory_stats))
        .route("/memories/{id}", get(get_memory));

    Router::new()
        .route("/", get(serve_index))
        .nest("/api/v1", api)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
}

// ---- Main ----

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let args = Args::parse();

    // Load config
    let config = Config::from_env()?;

    // Initialize database pool
    let pg_config = config.storage.postgres.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "PostgreSQL not configured. Set storage.postgres in config.json or DATABASE_URL env var."
        )
    })?;

    let pool = init_pool(pg_config).await?;
    info!("Database connected");

    // Run migrations
    migrations::run(&pool).await?;
    info!("Migrations complete");

    // Build stores
    let state = DashboardState {
        pool: pool.clone(),
        status_store: AgentStatusStore::new(pool.clone()),
        task_store: TaskStore::new(pool.clone()),
        soul_store: SoulStore::new(pool.clone()),
        config_store: ConfigParamStore::new(pool.clone()),
        memory_store: MemoryStore::new(pool.clone()),
    };

    // Build router
    let app = build_router(state);

    // Bind and serve
    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    info!("Dashboard listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
