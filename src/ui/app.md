# app.rs

## Purpose
Defines `AgentApp`, the top-level eframe application for the API-only frontend. It owns UI state, an `ApiClient`, websocket-driven event intake, and REST-driven chat/config control.

## Components

### `AgentApp`
- **Does**: Holds frontend UI state: event log, API client, runtime status, chat list/history, streaming preview, tool-progress drawer data, settings/character/workflow panels, `pending_approvals` for approval popups, and mind-state fields: `last_orientation`, `last_action`, `last_journal`, `live_stream_text` (live LLM token stream, any conversation).
- **Interacts with**: `crate::api::{ApiClient, FrontendEvent, ChatConversation, ChatMessage, AgentVisualState, OrientationSummary}`, UI subpanels.

### `AgentApp::new(api_client, fallback_config)`
- **Does**: Creates a tokio runtime, starts WS event streaming, fetches config from backend (fallback on failure), initializes panels, then loads status/conversations/history.
- **Interacts with**: `ApiClient::stream_events_forever`, `ApiClient::get_config`.

### REST refresh helpers (`refresh_status`, `refresh_conversations`, `refresh_chat_history`)
- **Does**: Pulls current backend state into UI every refresh interval.
- **Interacts with**: `/v1/agent/status`, `/v1/conversations`, `/v1/conversations/:id/messages`.

### Chat actions (`send_chat_message`, `create_new_conversation`)
- **Does**: Sends operator messages and creates new conversations via backend API.
- **Interacts with**: `/v1/conversations/:id/messages`, `/v1/conversations`.

### Prompt inspection (`open_prompt_inspector_for_turn`)
- **Does**: Fetches the exact stored turn prompt payload from backend and opens an egui window showing full context prompt text, optional per-turn system prompt, and source-highlight overlays for context sections.
- **Interacts with**: `/v1/turns/:id/prompt`, `chat::render_private_chat` prompt-button return value.

### `persist_config(config)`
- **Does**: Saves settings/character/workflow config via backend API, syncs local panel state from backend response, and forces avatar reload so mood-avatar changes apply immediately.
- **Interacts with**: `/v1/config`.

### `impl eframe::App for AgentApp` -- `update()`
- **Does**: Main render loop. Processes WS events, updates status/chat on timer, renders chat + activity panels, and dispatches API actions for pause/stop/config/message operations.
- **Interacts with**: `chat::render_private_chat`, `chat::render_event_log`, `sprite::render_agent_sprite`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `main.rs` | `AgentApp::new(ApiClient, AgentConfig)` constructor | Changing constructor signature |
| `api.rs` | Stable method surface for config/chat/status/pause/event-stream | Renaming/removing client methods |
| UI panel modules | `settings_panel.config` remains mutable for cross-panel synchronization | Changing panel state ownership |

### Mind-state header (`visual_state_display`)
- **Does**: Renders a rich status strip under the app title: visual-state emoji + color, orientation disposition chip, and last-action one-liner — all sourced from live WS events rather than polling.

### `render_live_tool_entry` / `tool_badge_color`
- **Does**: Formats each live tool-progress entry as a colored tool-name badge (color by category: shell=amber, files=blue, http=purple, memory=green, comfy=orange, vision=pink) + truncated monospace output.

### Sidebar — three zones
- **Does**: The right panel ("🧠 Mind") is divided into three zones: (1) mind-state group (orientation, last action, last journal), (2) "💭 Live Stream" collapsible section showing the last 600 chars of the active LLM token stream, (3) grouped turn-history log via `render_event_log`.

### `truncate_str` / `last_n_chars`
- **Does**: Local helpers for display truncation. `truncate_str` adds `…` at max_chars; `last_n_chars` returns the trailing N chars of a string.

## Notes
- The app is no longer wired to in-process `Agent`/`AgentDatabase`/`flume` backend channels.
- WS event stream runs continuously with reconnect; polling refresh every 2s is retained for list/history/status consistency.
- Activity panel is now visible by default so autonomous progress and wake/error telemetry are immediately visible without extra clicks.
- Main chat surface now uses fixed vertical regions (chat history, live tool output, composer) to prevent tool/output panels from overlapping chat bubbles or pushing the composer off-screen.
- UI-level API failures are surfaced in the activity log as `FrontendEvent::Error` entries.
- Prompt inspector windows are opened on demand from agent message rows and support toggling system-prompt visibility plus translucent source highlights over prompt sections.
- `FrontendEvent::ApprovalRequest` is NOT pushed to the activity log; it is deduplicated and stored in `pending_approvals`. Each pending approval renders as an `egui::Window` popup (centered, non-collapsible) with "✅ Allow this session" and "✖ Dismiss" buttons. Approval calls `ApiClient::approve_tool`; dismiss just removes the entry from `pending_approvals`.
