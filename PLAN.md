OpenAgent Framework Refactoring Plan 

 Context

 OpenAgent currently uses a custom ReAct loop (run_agentic_loop()) with a raw HTTP client (OpenRouterClient) for LLM orchestration. The technical
 direction calls for:

 1. Adopting rig-core (Rust AI framework) for type-safe LLM orchestration via OpenRouter
 2. Replacing the ReAct loop with a Planner-Worker-Reflector state machine for more reliable, structured reasoning
 3. Properly wiring the existing RAG pipeline (fastembed + moka + pgvector)
 4. Bridging existing 13 tools to rig-core's tool system without rewriting them

 The migration is incremental: a feature flag controls which loop runs, so both old and new paths coexist safely.

 ---
 Phase 1: Foundation (No behavior changes)

 1.1 Add dependencies to Cargo.toml

 rig-core = { version = "0.30", features = ["derive", "openrouter"] }
 schemars = "0.8"

 1.2 Create src/agent/rig_client.rs — Rig OpenRouter client wrapper

 - Wrap rig::providers::openrouter::Client configured with API key from OpenRouterConfig
 - Expose agent_builder() for creating rig agents with tools
 - Expose complete() for raw completion calls (used by planner/reflector)
 - Handle OpenRouter-specific headers (HTTP-Referer, X-Title)

 1.3 Create src/agent/tool_bridge.rs — Adapter: OpenAgent Tool → rig-core ToolDyn

 - RigToolAdapter wraps Arc<dyn Tool> and implements rig's ToolDyn trait
 - Maps: name() → name(), to_definition() → definition(), execute(Value) → call(String)
 - Error mapping: crate::Error → rig::tool::ToolError

 1.4 Modify src/tools/registry.rs — Arc-based storage + rig bridge

 - Change internal HashMap<String, Box<dyn Tool>> → HashMap<String, Arc<dyn Tool>>
 - Add to_rig_toolset(&self) -> rig::tool::ToolSet method
 - Backward-compatible: register() still takes T: Tool + 'static

 1.5 Update src/agent/mod.rs — Add new module exports

 - pub mod rig_client;
 - pub mod tool_bridge;

 Verification: cargo build passes. All existing tests pass. No runtime behavior changes.

 ---
 Phase 2: Planner-Worker-Reflector State Machine

 2.1 Create src/agent/state_machine.rs — Core state machine

 State enum:
 AgentState::Planning { messages, attempt }
 AgentState::Executing { plan, step_index, results, messages }
 AgentState::Reflecting { plan, results, messages }
 AgentState::Complete { response, trace, messages, usage }
 AgentState::Failed { error, trace }

 Data types:
 - ExecutionPlan { goal, steps, reasoning }
 - PlanStep { description, tool_name, tool_args, depends_on }
 - StepResult { step_index, success, content, duration_ms }

 Transition flow:
 Planning → Executing → Reflecting → Complete
               ↑              |
               +-- Replanning ←+ (max 3 replans)

 Key struct: PlannerWorkerReflector<C: LoopCallback> with fields:
 - rig_client: &RigLlmClient
 - legacy_client: &OpenRouterClient (fallback for direct completion)
 - tools: &ToolRegistry
 - config: LoopConfig
 - callback: C
 - user_id, chat_id

 Main method: async fn run(messages) -> Result<AgentLoopOutput>
 - Returns the same AgentLoopOutput type as the legacy loop
 - Preserves LoopTrace recording for observability

 2.2 Create src/agent/planner.rs — Planning phase

 - Injects structured prompt asking LLM for JSON execution plan
 - Tool descriptions injected into prompt (names + descriptions)
 - Parses ExecutionPlan from LLM response (with fallback to direct answer)
 - If no tools needed → transitions directly to Complete
 - Uses GenerationOptions::precise() (temperature: 0.0)

 2.3 Create src/agent/reflector.rs — Reflection phase

 - Summarizes execution results for LLM review
 - LLM decides: answer achieved → Complete, or needs more work → replan
 - Replan detection via "REPLAN:" prefix in response
 - Max replan count enforced (default: 3)
 - Uses GenerationOptions::balanced() (temperature: 0.5)

 2.4 Modify src/agent/prompts.rs — Add new prompt templates

 - PLANNER_PROMPT: Structured planning prompt with tool catalog
 - REFLECTOR_PROMPT: Result review and decision prompt
 - REPLAN_PROMPT: Context injection for replanning cycle

 2.5 Modify src/agent/agentic_loop.rs — Add dispatch function

 - Add LoopConfig::use_state_machine: bool field (default: false)
 - Add new public function run_agent() that routes to either:
   - PlannerWorkerReflector::run() when use_state_machine = true
   - Legacy run_agentic_loop() when use_state_machine = false
 - Callback system fully preserved in both paths

 2.6 Modify src/agent/types.rs — Add plan types

 - ExecutionPlan, PlanStep, StepResult structs (with Serialize/Deserialize)
 - AgentState enum

 Verification: Unit tests for state transitions. Mock LLM returning structured plans → verify Planning → Executing → Reflecting → Complete. Verify
 replan cap works.

 ---
 Phase 3: Binary Integration

 3.1 Modify src/bin/gateway.rs

 - Add RigLlmClient to AppState (alongside existing OpenRouterClient)
 - In handle_chat(): use run_agent() instead of run_agentic_loop()
 - LoopConfig::gateway() gets use_state_machine from config_params table
 - GatewayCallback unchanged — same lifecycle events

 3.2 Modify src/bin/tui.rs

 - Add RigLlmClient to TuiState
 - In agent_loop(): use run_agent() instead of run_agentic_loop()
 - Add --planner CLI flag to enable state machine mode

 3.3 Modify src/scheduler.rs

 - Add RigLlmClient field to Scheduler
 - In execute_task(): use state machine via run_agent()
 - NoOpCallback unchanged

 3.4 Runtime feature flag

 - Add agent:use_state_machine to config_params table (default: false)
 - Seeded on startup in gateway alongside existing scheduler params
 - Togglable via dashboard API without restart

 Verification:
 - Start gateway with use_state_machine=false → legacy behavior
 - Set use_state_machine=true via dashboard → planner-worker-reflector activates
 - TUI with --planner flag → state machine mode
 - Scheduler processes tasks using configured mode

 ---
 Phase 4: Cleanup & Optimization

 4.1 Implement LlmProvider trait on RigLlmClient

 - File: src/core/provider.rs
 - Makes the currently-unused trait actually useful

 4.2 Consolidate duplicate types

 - src/agent/types.rs vs src/core/provider.rs — deduplicate
 - Keep agent/types.rs as canonical, make core/provider.rs re-export

 4.3 RAG integration with rig-core context

 - Implement rig's VectorStoreIndex trait backed by MemoryRetriever
 - Use rig's dynamic_context() agent builder for automatic memory injection
 - Replaces manual system prompt manipulation in gateway/tui

 4.4 Deprecation notices

 - Mark run_agentic_loop() as #[deprecated(note = "Use run_agent() instead")]
 - Mark OpenRouterClient as #[deprecated(note = "Use RigLlmClient instead")]

 Verification: cargo build clean (no warnings except deprecated usage). Full integration test: gateway sends message → planner creates plan → tools
 execute → reflector verifies → response returned.

 ---
 Files Modified/Created Summary
 ┌────────────────────────────┬───────────────────────────────────┬───────┐
 │            File            │              Action               │ Phase │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ Cargo.toml                 │ Add rig-core, schemars            │ 1     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/rig_client.rs    │ NEW                               │ 1     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/tool_bridge.rs   │ NEW                               │ 1     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/tools/registry.rs      │ Modify (Arc storage + rig bridge) │ 1     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/mod.rs           │ Modify (new exports)              │ 1     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/state_machine.rs │ NEW                               │ 2     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/planner.rs       │ NEW                               │ 2     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/reflector.rs     │ NEW                               │ 2     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/prompts.rs       │ Modify (new templates)            │ 2     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/agentic_loop.rs  │ Modify (dispatch + config)        │ 2     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/agent/types.rs         │ Modify (plan types)               │ 2     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/bin/gateway.rs         │ Modify (RigLlmClient + run_agent) │ 3     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/bin/tui.rs             │ Modify (RigLlmClient + --planner) │ 3     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/scheduler.rs           │ Modify (RigLlmClient + run_agent) │ 3     │
 ├────────────────────────────┼───────────────────────────────────┼───────┤
 │ src/core/provider.rs       │ Modify (implement trait)          │ 4     │
 └────────────────────────────┴───────────────────────────────────┴───────┘
 ---
 Risk Mitigation

 - Feature flag: use_state_machine defaults to false — zero risk to production
 - Legacy preserved: run_agentic_loop() remains fully functional
 - Same output types: AgentLoopOutput unchanged — no breaking changes downstream
 - Callback compat: LoopCallback trait unchanged — gateway/tui callbacks work in both modes
 - Incremental: Each phase is independently deployable and testable