# mod.rs

## Purpose
Coordinates the core autonomous agent loop: polling skills, reasoning over events, managing visual state, running periodic heartbeat checks, handling private operator chat, and persisting long-lived behavior through the shared database. It is the runtime orchestrator that binds skills, tools, reasoning, memory, and UI events.

## Components

### `Agent`
- **Does**: Owns runtime dependencies (skills, tools, config, database, reasoning engines) and exposes lifecycle operations (`new`, `run_loop`, `reload_config`, `toggle_pause`)
- **Interacts with**: `config::AgentConfig`, `database::AgentDatabase`, `skills::*`, `tools::ToolRegistry`, `agent::reasoning`, `agent::trajectory`

### `AgentState`
- **Does**: Tracks in-memory runtime state (visual mode, pause flag, rate counters, processed event IDs)
- **Interacts with**: `run_loop`, `run_cycle`, and UI-facing event emission

### `AgentEvent` / `AgentVisualState`
- **Does**: Defines UI/event bus payloads describing current state, observations, reasoning traces, actions, and errors
- **Interacts with**: `ui::app` via shared flume channel

### `run_loop`
- **Does**: Main background loop; checks pause/rate limits, runs periodic maintenance, then executes the main cycle
- **Interacts with**: `maybe_evolve_persona`, `maybe_run_heartbeat`, `run_cycle`, sleep scheduling based on config

### `maybe_run_heartbeat`
- **Does**: Schedules autonomous heartbeat cycles, reads pending checklist/reminder signals, and invokes the tool-calling loop only when work exists
- **Interacts with**: `tools::agentic::AgenticLoop`, `ToolRegistry`, `AgentDatabase::agent_state` and working memory

### `maybe_run_memory_evolution`
- **Does**: Runs periodic replay evaluation for memory backends, stores eval artifacts, and records promotion-policy outcomes
- **Interacts with**: `memory::eval`, `memory::archive`, `AgentDatabase` memory eval/promotion APIs

### `run_cycle`
- **Does**: Polls skills, filters events, calls reasoning, executes chosen action (reply/update memory/no-op), and marks events processed
- **Interacts with**: `reasoning::ReasoningEngine`, `Skill::poll/execute`, `AgentDatabase` memory/chat helpers

### `process_chat_messages`
- **Does**: Handles operator-private chat messages using LLM JSON decisions and optional memory updates
- **Interacts with**: `database::chat_messages`, `reasoning::process_chat`

### Persona evolution helpers
- **Does**: Capture persona snapshots and run trajectory inference on schedule
- **Interacts with**: `agent::trajectory`, `database::persona_history`, reflection timestamps in `agent_state`

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `main.rs` | `Agent::new(...).run_loop()` drives autonomous behavior without extra orchestration | Changing constructor or loop entrypoint signatures |
| `ui/app.rs` | `AgentEvent` variants remain stable enough for chat/state rendering | Renaming/removing emitted event types |
| `database.rs` | Chat and memory APIs are available and synchronous | Changing DB API names or semantics |
| `tools/mod.rs` | `ToolRegistry` can be shared and used in autonomous context | Removing registry injection from `Agent` |
| `tools/agentic.rs` | `AgenticLoop` accepts OpenAI-compatible endpoint and ToolContext for autonomous runs | Changing loop constructor/run signatures |
| `memory/eval.rs` | Replay evaluation functions remain deterministic and serializable | Breaking report schema or candidate IDs |

## Notes
- Current behavior combines periodic skill polling with persona maintenance, optional heartbeat automation, and private chat handling.
- Heartbeat mode is guarded by config + due-time checks and is intentionally quiet when no pending tasks/reminders are found.
- Memory evolution scheduling is heartbeat-triggered but independently rate-limited by its own interval key in `agent_state`.
- The run loop is intentionally conservative around errors: failures emit events and continue after short backoff.
