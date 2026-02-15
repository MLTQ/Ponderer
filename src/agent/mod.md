# mod.rs

## Purpose
Coordinates the core autonomous agent loop: polling skills, reasoning over events, managing visual state, running periodic heartbeat checks, handling private operator chat, and persisting long-lived behavior through the shared database. It is the runtime orchestrator that binds skills, tools, reasoning, memory, and UI events.

## Components

### `Agent`
- **Does**: Owns runtime dependencies (skills, tools, config, database, reasoning engines) and exposes lifecycle operations (`new`, `run_loop`, `reload_config`, `toggle_pause`)
- **Interacts with**: `config::AgentConfig`, `database::AgentDatabase`, `skills::*`, `tools::ToolRegistry`, `agent::reasoning`, `agent::trajectory`

### Living Loop foundation modules (`journal`, `concerns`)
- **Does**: Provide typed records for journal entries and concern tracking used by new ll.1 database tables
- **Interacts with**: `database.rs` CRUD/persistence methods; future ambient/orientation loop work

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
- **Does**: Polls skills, filters new events, then runs a dedicated agentic pass over those events so tool calls (including bridged skill actions) happen in the same loop architecture as private chat
- **Interacts with**: `Skill::poll`, `tools::agentic::AgenticLoop`, `ToolRegistry` (notably `graphchan_skill`), `AgentDatabase` memory/chat helpers

### `process_chat_messages`
- **Does**: Handles unread operator chat messages by conversation thread, streams live token output during each LLM call, emits per-tool progress updates, and can run multiple autonomous turns per thread before final handoff using a structured `[turn_control]...[/turn_control]` protocol
- **Interacts with**: `database::chat_messages`, `database::chat_conversations`, `database::chat_turns`, `database::chat_turn_tool_calls`, `tools::agentic::AgenticLoop::run_with_history_streaming_and_tool_events`, `ToolRegistry`
- **Rationale**: Uses continuation hints (not synthetic operator messages) for multi-turn autonomy, scopes private-chat tools away from Graphchan posting, compacts long sessions through persisted summary snapshots, and only persists the final yielded assistant reply to avoid duplicate/confusing intermediate chat bubbles.

### `capability_profiles`
- **Does**: Resolves explicit loop capability policies (`private_chat`, `skill_events`, `heartbeat`) into per-loop `ToolContext` objects with autonomous mode and allow/deny tool lists
- **Interacts with**: `config::AgentConfig.capability_profiles`, `tools::ToolContext`

### Persona evolution helpers
- **Does**: Capture persona snapshots and run trajectory inference on schedule
- **Interacts with**: `agent::trajectory`, `database::persona_history`, reflection timestamps in `agent_state`

### Chat formatting helpers
- **Does**: Builds operator-chat prompts and serializes tool-call/thinking/media metadata into `[tool_calls]...[/tool_calls]`, `[thinking]...[/thinking]`, and `[media]...[/media]` blocks for inline UI rendering
- **Interacts with**: `ui/chat.rs` parser for collapsible tool details and media previews

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `main.rs` | `Agent::new(...).run_loop()` drives autonomous behavior without extra orchestration | Changing constructor or loop entrypoint signatures |
| `ui/app.rs` | `AgentEvent` variants remain stable enough for chat/state rendering, including `ChatStreaming { conversation_id, content, done }` and `ToolCallProgress { ... }` | Renaming/removing emitted event types |
| `database.rs` | Chat and memory APIs are available and synchronous; private chat relies on conversation-scoped context plus turn lifecycle APIs (`begin_chat_turn`, `record_chat_turn_tool_call`, `complete_chat_turn`, `fail_chat_turn`, `add_chat_message_in_turn`) | Changing DB API names, turn-state semantics, or message persistence order |
| `tools/mod.rs` | `ToolRegistry` can be shared and used in autonomous context, including bridged skill tools | Removing registry injection or bridged tool names used by prompts |
| `tools/agentic.rs` | `AgenticLoop` accepts OpenAI-compatible endpoint and ToolContext for autonomous runs | Changing loop constructor/run signatures |
| `agent/capability_profiles.rs` | Loop context policies are resolved centrally and applied consistently across heartbeat, skill events, and private chat | Bypassing policy resolver or changing profile semantics |
| `memory/eval.rs` | Replay evaluation functions remain deterministic and serializable | Breaking report schema or candidate IDs |
| `ui/chat.rs` | Embedded metadata block delimiters remain stable (`[tool_calls]`, `[thinking]`) | Changing envelope formats without parser update |
| `tools/comfy.rs` | Tool JSON with `media` arrays is transformed into chat-visible media payloads | Changing media extraction shape in formatter |

## Notes
- Current behavior combines periodic skill polling with persona maintenance, optional heartbeat automation, and private chat handling.
- Skill-event handling now goes through the same multi-step tool-calling loop used by private chat, so skill actions and regular tools share one decision engine.
- Private chat replies are now scoped per conversation ID to avoid cross-thread prompt contamination.
- Long-running private chats are compacted as `summary snapshot + recent context + new messages`, with snapshots stored in DB and refreshed after configurable message deltas.
- Private chat emits a structured turn-control block per assistant response; the loop continues only when decision=`continue`, user input is not needed, and turn budget remains.
- Private chat continuation now also requires meaningful forward progress signals (`tool_count > 0` or `status=still_working`) before another autonomous turn is allowed.
- Turn-control parsing treats visible assistant text as authoritative; block `user_message` is only fallback when visible text is empty and does not resemble a hallucinated `User:`/`Operator:` transcript.
- Tool-call progress is streamed as events during a turn so the UI can show real-time execution output (for example shell output snippets) before final reply persistence.
- Each autonomous private-chat turn is persisted in DB before/after execution, including tool-call lineage and terminal state (`completed`, `awaiting_approval`, or `failed`), but only the final yielded assistant message is added to chat history.
- Tool access is now enforced by explicit capability profiles per loop (`private_chat`, `skill_events`, `heartbeat`), with optional config overrides for allow/deny lists.
- Operator messages and per-turn agent outcomes now append to daily memory log keys (`activity-log-YYYY-MM-DD`) for longitudinal context.
- Heartbeat mode is guarded by config + due-time checks and is intentionally quiet when no pending tasks/reminders are found.
- Memory evolution scheduling is heartbeat-triggered but independently rate-limited by its own interval key in `agent_state`.
- The run loop is intentionally conservative around errors: failures emit events and continue after short backoff.
