# app.rs

## Purpose
Defines `AgentApp`, the top-level eframe application for the API-only frontend. It owns UI state, an `ApiClient`, websocket-driven event intake, and REST-driven chat/config control.

## Components

### `AgentApp`
- **Does**: Holds frontend UI state: event log, API client, runtime status, chat list/history, streaming preview, tool-progress drawer data, and settings/character/workflow panels.
- **Interacts with**: `crate::api::{ApiClient, FrontendEvent, ChatConversation, ChatMessage, AgentVisualState}`, UI subpanels.

### `AgentApp::new(api_client, fallback_config)`
- **Does**: Creates a tokio runtime, starts WS event streaming, fetches config from backend (fallback on failure), initializes panels, then loads status/conversations/history.
- **Interacts with**: `ApiClient::stream_events_forever`, `ApiClient::get_config`.

### REST refresh helpers (`refresh_status`, `refresh_conversations`, `refresh_chat_history`)
- **Does**: Pulls current backend state into UI every refresh interval.
- **Interacts with**: `/v1/agent/status`, `/v1/conversations`, `/v1/conversations/:id/messages`.

### Chat actions (`send_chat_message`, `create_new_conversation`)
- **Does**: Sends operator messages and creates new conversations via backend API.
- **Interacts with**: `/v1/conversations/:id/messages`, `/v1/conversations`.

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

## Notes
- The app is no longer wired to in-process `Agent`/`AgentDatabase`/`flume` backend channels.
- WS event stream runs continuously with reconnect; polling refresh every 2s is retained for list/history/status consistency.
- Composer input is bottom-anchored so expanding live tool output does not push the text box off-screen.
- UI-level API failures are surfaced in the activity log as `FrontendEvent::Error` entries.
