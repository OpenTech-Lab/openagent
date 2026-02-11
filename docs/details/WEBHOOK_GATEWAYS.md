# OpenAgent Webhook Gateways

OpenAgent now supports webhook-based architectures as alternatives to the polling-based Telegram gateway. This eliminates timeout issues and provides better scalability.

## Available Gateways

### 1. Webhook Gateway (`openagent-webhook-gateway`)

Basic webhook implementation that receives Telegram updates via HTTP POST and processes them asynchronously.

**Features:**
- HTTP webhook endpoint for Telegram updates
- Asynchronous task processing with worker pool
- Configurable callback webhooks for results
- Health check endpoint

**Usage:**
```bash
# Set environment variables
export WEBHOOK_PORT=8080
export WEBHOOK_SECRET=your_secret_here
export CALLBACK_URL=https://your-app.com/webhook/callback
export MAX_WORKERS=4

# Run the gateway
cargo run --bin openagent-webhook-gateway
```

**API Endpoints:**
- `POST /webhook` - Receive Telegram updates
- `GET /health` - Health check

**Telegram Webhook Setup:**
Set your Telegram bot's webhook to point to: `https://your-domain.com/webhook`

### 2. Streaming Webhook Gateway (`openagent-streaming-webhook-gateway`)

Advanced webhook gateway with real-time streaming support for progress updates.

**Features:**
- All webhook gateway features
- Server-Sent Events (SSE) for real-time progress updates
- Incremental progress reporting (thinking, tool execution, response chunks)
- Configurable streaming modes

**Usage:**
```bash
# Set environment variables
export WEBHOOK_PORT=8081
export ENABLE_SSE=true
export CALLBACK_URL=https://your-app.com/webhook/callback

# Run the streaming gateway
cargo run --bin openagent-streaming-webhook-gateway
```

**API Endpoints:**
- `POST /webhook?stream=true` - Enable streaming for a task
- `GET /stream/{task_id}` - SSE stream for task progress
- `GET /health` - Health check

**Progress Events:**
- `started` - Task processing began
- `thinking` - Agent is planning
- `tool_started` - Tool execution began
- `tool_completed` - Tool execution finished
- `response_chunk` - Incremental response text
- `completed` - Task finished successfully
- `failed` - Task failed with error

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `WEBHOOK_PORT` | Port to listen on | 8080/8081 |
| `WEBHOOK_SECRET` | Secret for webhook authentication | None |
| `CALLBACK_URL` | URL to send results to | None |
| `MAX_WORKERS` | Number of concurrent workers | 4 |
| `ENABLE_SSE` | Enable Server-Sent Events (streaming gateway only) | true |

### Telegram Bot Setup

1. Create a bot with @BotFather
2. Set webhook URL:
   ```bash
   curl -X POST "https://api.telegram.org/bot<TOKEN>/setWebhook" \
        -d "url=https://your-domain.com/webhook"
   ```

## Architecture Benefits

### vs Polling Gateway
- **No timeouts**: Webhooks don't suffer from polling timeouts
- **Better scalability**: Asynchronous processing with worker pools
- **Real-time updates**: Streaming gateway provides live progress
- **Webhook integrations**: Easy integration with other services
- **Resource efficient**: No constant polling overhead

### Worker Pool
- Configurable number of concurrent workers
- Task queue prevents overwhelming the system
- Graceful handling of high load
- Individual task timeouts

### Streaming Support
- Real-time progress updates via SSE
- Incremental response streaming
- Tool execution visibility
- Better user experience for long-running tasks

## Example Integration

### Basic Webhook Response
```json
{
  "status": "accepted",
  "task_id": "task_1234567890"
}
```

### Streaming Progress Event
```json
{
  "task_id": "task_1234567890",
  "progress_type": "response_chunk",
  "data": null,
  "timestamp": "2024-01-15T10:30:00Z"
}
```

### Final Result Callback
```json
{
  "task_id": "task_1234567890",
  "success": true,
  "messages": [
    {
      "chat_id": 123456789,
      "text": "Here's the information you requested...",
      "parse_mode": "Markdown",
      "reply_to_message_id": 42
    }
  ],
  "duration_ms": 2500,
  "progress_updates": 8
}
```

## Migration from Polling

To migrate from the polling gateway:

1. Deploy webhook gateway alongside existing polling gateway
2. Update Telegram webhook URL to point to new gateway
3. Test with subset of users
4. Gradually migrate all traffic
5. Decommission polling gateway

The webhook gateways are fully compatible with existing OpenAgent configuration and tools.</content>
<parameter name="filePath">/home/toyofumi/Project/openagent/WEBHOOK_GATEWAYS.md