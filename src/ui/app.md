# app.rs

## Purpose
Defines `AgentApp`, the top-level eframe application. It owns the agent handle, event receiver, all UI panels, chat state, and the tokio runtime used to dispatch async commands from the GUI thread. The private chat view is the primary interaction surface, with activity/event logs moved to a secondary side panel.

## Components

### `AgentApp`
- **Does**: Central application state holding agent reference, event log, UI panels, avatar set, database handle, conversation list/selection, scoped chat history, chat media texture cache, structured live tool-progress buffer (with optional subtask tags), and live streaming preview state, plus activity panel visibility
- **Interacts with**: `Agent` (via `Arc<Agent>`), `AgentEvent`/`AgentVisualState` from `crate::agent`, `AgentDatabase`/`ChatMessage`/`ChatConversation` from `crate::database`, all UI panel structs
- **Rationale**: Single owner of all GUI state; bridges sync egui rendering with async agent operations

### `AgentApp::new(event_rx, agent, config, database)`
- **Does**: Constructs the app, initializes all panels from config, defers avatar loading to first frame
- **Interacts with**: `SettingsPanel`, `CharacterPanel`, `ComfySettingsPanel`, `AgentConfig`

### `AgentApp::refresh_conversations()`
- **Does**: Refreshes available chat threads (including persisted runtime status) and keeps `active_conversation_id` valid when threads change
- **Interacts with**: `AgentDatabase::list_chat_conversations`

### `AgentApp::refresh_chat_history()`
- **Does**: Pulls chat messages for the currently active conversation into `self.chat_history`
- **Interacts with**: `AgentDatabase::get_chat_history_for_conversation`

### `AgentApp::send_chat_message(content)`
- **Does**: Inserts an "operator" role message into the active conversation, then refreshes conversations/history
- **Interacts with**: `AgentDatabase::add_chat_message_in_conversation`

### `AgentApp::create_new_conversation()`
- **Does**: Creates a fresh chat thread, switches selection to it, and clears composer text
- **Interacts with**: `AgentDatabase::create_chat_conversation`

### `AgentApp::load_avatars(ctx, config)`
- **Does**: Reads avatar paths from config and delegates to `AvatarSet::load`
- **Interacts with**: `AvatarSet`, `AgentConfig` fields `avatar_idle`, `avatar_thinking`, `avatar_active`

### `conversation_display_label(conversation)` (private)
- **Does**: Builds conversation picker labels from title/message count and appends a runtime-state suffix (`processing`, `done`, `awaiting input`, `failed`) when non-idle
- **Interacts with**: `ChatConversation.runtime_state` from `database.rs`

### `impl eframe::App for AgentApp` -- `update()`
- **Does**: Main render loop. Loads avatars on first frame, polls `event_rx` for `AgentEvent`s, maps `ChatStreaming` events into a live preview bubble, records `ToolCallProgress` updates into a per-conversation live buffer, refreshes chat on operator-related actions, renders header with sprite, toolbar buttons, primary private chat (including media rendering via shared cache), a live "Agent Turn" drawer with expandable per-subtask sections, secondary activity side panel, multiline chat input, and all modal panels (settings, character, comfy workflow). Persists config and hot-reloads the agent on save.
- **Interacts with**: `sprite::render_agent_sprite`, `chat::render_event_log`, `chat::render_private_chat`, `SettingsPanel::render`, `CharacterPanel::render`, `ComfySettingsPanel::render`, `Agent::reload_config`, `Agent::toggle_pause`

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Binary entry point | `AgentApp::new` signature with `Receiver<AgentEvent>`, `Arc<Agent>`, `AgentConfig`, `Option<Arc<AgentDatabase>>` | Changing constructor args breaks startup |
| `SettingsPanel` | `config` field is public and mutated by `CharacterPanel` saves | Making `config` private breaks cross-panel sync |
| `Agent` | `reload_config` and `toggle_pause` are async | Removing these methods breaks UI buttons |
| `AgentDatabase` | Conversation APIs (`list_chat_conversations`, `create_chat_conversation`, `get_chat_history_for_conversation`, `add_chat_message_in_conversation`) plus `ChatConversation.runtime_state` semantics | Removing runtime-state field or changing chat API signatures breaks picker/status behavior |
| `agent::AgentEvent` | Streaming updates include `conversation_id`, partial/full `content`, and `done` state; tool updates include `ToolCallProgress` with preview text | Changing event payload shape breaks preview/drawer rendering |

## Notes
- A dedicated `tokio::runtime::Runtime` is created inside the app because eframe's render loop is synchronous. All async agent calls are dispatched through `self.runtime.spawn`.
- Chat conversations and active-conversation history auto-refresh every 2 seconds via `last_chat_refresh` timer.
- Conversation picker labels include DB-backed runtime state so users can see when a thread is still processing, waiting for input, or failed.
- Avatar loading is deferred to the first `update()` call because `egui::Context` is not available at construction time.
- Chat input is multiline; plain `Enter` sends while `Shift+Enter` inserts a newline.
- Live provider tokens are shown inline in chat via `streaming_chat_preview` and replaced by persisted messages once the agent saves final output.
- Tool execution progress is rendered in a dedicated drawer under chat, with expandable subtask groups when progress lines carry a `[subtask-id]` prefix (used by background chat subtasks).
