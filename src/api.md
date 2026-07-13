# api.rs

## Purpose
Frontend-only backend API client for Ponderer. Encapsulates authenticated REST calls and websocket event streaming so the GUI can operate without direct in-process access to backend `Agent` or database internals.

## Components

### `ApiClient`
- **Does**: Stores backend base URL/token, performs bounded REST requests for config/chat/agent control, checks backend health for launcher discovery, and maintains WS event streaming with reconnect.
- **Interacts with**: `ponderer_backend` REST/WS routes under `/v1`.

### `ApiClient::new_local`
- **Does**: Builds the client used for discovered or newly launched loopback backends with ambient HTTP proxies disabled, preventing the persisted local bearer token from being forwarded through a proxy.
- **Interacts with**: `main.rs` persistent backend discovery and launch paths.

### `ApiClient::health`
- **Does**: Requires an HTTP-successful, decodable Ponderer health payload whose state is either `ok` or `degraded`.
- **Interacts with**: backend `/v1/health` and desktop discovery.

### Chat DTOs (`ChatConversation`, `ChatMessage`, `ChatTurnPhase`)
- **Does**: Frontend-side models for chat list/history rendering.
- **Interacts with**: `ui/app.rs` conversation picker and chat renderer.
- **Notes**: `ChatMessage.turn_id` is optional and used to fetch turn-level prompt diagnostics.

### Prompt DTOs (`ChatTurnPrompt`)
- **Does**: Carries prompt-inspection payload for one turn (`prompt_text` plus optional `system_prompt_text`).
- **Interacts with**: `ui/app.rs` prompt inspector window.

### Scheduled-job DTOs (`ScheduledJob`, `UpdateScheduledJobRequest`)
- **Does**: Frontend-side models for recurring schedule CRUD payloads.
- **Interacts with**: `ui/settings.rs` schedules tab and `ui/app.rs` schedule action dispatcher.

### Runtime DTOs (`AgentVisualState`, `AgentRuntimeStatus`)
- **Does**: Frontend-side models for status badges/sprite selection and pause/stop controls.
- **Interacts with**: `ui/sprite.rs`, `ui/avatar.rs`, `ui/app.rs` header status.

### Plugin DTOs (`PluginManifest`, settings-tab + settings-schema manifests)
- **Does**: Re-exports the canonical versioned DTOs from `ponderer_backend::plugin_contract` and carries plugin discovery/settings data without maintaining a frontend copy.
- **Interacts with**: `ui/settings.rs`, `ui/plugin_settings_form.rs`, `ui/app.rs` startup discovery, and the backend `/v1/plugins` response.
- **Notes**: `PluginKind` distinguishes core capabilities from subprocess packages; historical `BackendPlugin*` names remain re-exported only for source compatibility.

### `FrontendEvent`
- **Does**: Normalized UI event stream derived from backend WS envelopes. Includes `ApprovalRequest { tool_name, reason }` for interactive approval popups, `TokenMetrics { conversation_id, clear, samples }` for the live token monitor, and `CycleStart { label }` used by `chat.rs` to group the turn-history log into collapsible cycle groups.
- **Interacts with**: `ui/chat.rs` activity log and `ui/app.rs` streaming preview/tool-progress state, approval popup, token-monitor state, and mind-state tracking fields.

### `TokenMetricSample`
- **Does**: Carries a single live token-novelty sample (`text`, optional `logprob`/`entropy`, and derived `novelty`) from the backend WS stream.
- **Interacts with**: `ui/token_monitor.rs` and `ui/app.rs`.

### `ApiClient::approve_tool`
- **Does**: `POST /v1/agent/tools/:tool_name/approve` — grants session-level approval so the named tool can run autonomously without further prompts.
- **Interacts with**: `server.rs` `approve_tool` handler → `Agent::grant_session_tool_approval` → `ToolRegistry::grant_session_approval`.

### `ApiClient::list_plugins`
- **Does**: `GET /v1/plugins` — fetches built-in plus live handshake-enriched plugin manifests so the UI can expose current tools and per-plugin settings tabs.
- **Interacts with**: `ponderer_backend/src/server.rs` plugin list route.

### `ApiClient::list_plugin_statuses`
- **Does**: `GET /v1/plugins/status` — fetches desired/actual runtime states, negotiated protocol/process metadata, restart counters, and recent errors.
- **Interacts with**: `ponderer_backend/src/server.rs` live plugin status route and future plugin diagnostics UI.

### Scheduled-job API methods (`list_scheduled_jobs`, `create_scheduled_job`, `update_scheduled_job`, `delete_scheduled_job`)
- **Does**: Wrap `/v1/scheduled-jobs` CRUD routes.
- **Interacts with**: `ponderer_backend/src/server.rs` scheduled-job handlers.

### Event mapping (`stream_events_forever`, `stream_events_once`, `map_event`)
- **Does**: Reads WS JSON envelopes, maps backend event types to `FrontendEvent`, and reconnects on disconnect/failure.
- **Interacts with**: `ponderer_backend/src/server.rs` event schema.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `ui/app.rs` | `ApiClient` exposes config/chat/status/pause/stop methods and event streaming | Renaming/removing API methods |
| `main.rs` | `ApiClient::health` validates a discovered persistent local backend before reuse | Changing health/auth behavior |
| `ui/chat.rs` | `FrontendEvent` variants remain stable for rendering | Removing/renaming event variants |
| Backend API | Routes and payload shapes under `/v1` match client decoding; plugin payloads use the exact shared backend type | Changing endpoint paths or shared schema fields |

## Notes
- Backend URL defaults to `http://127.0.0.1:8787` (`PONDERER_BACKEND_URL` override).
- REST calls use a 15-second client timeout so a dead backend cannot freeze the GUI render path indefinitely.
- Persistent loopback clients bypass ambient proxy settings; explicitly configured external backend clients retain normal proxy behavior.
- Bearer token comes from `PONDERER_BACKEND_TOKEN`; if absent, requests run unauthenticated (useful only when backend auth mode is disabled).
- WS URL is derived from HTTP base URL (`http -> ws`, `https -> wss`).
- Enum decoding for chat/runtime state is compatibility-tolerant (`snake_case` plus legacy PascalCase aliases) to survive backend/frontend schema drift during upgrades.
- Conversation list decode errors now include payload preview context to simplify diagnosing response-shape mismatches.
- Plugin manifest/settings DTOs are intentionally not redefined here; the backend crate is their single source of truth.
- Plugin runtime status DTOs are also re-exported from `ponderer_backend::plugin_contract`; the desktop can query them without schema duplication.
- `ApiClient::get_turn_prompt` fetches `/v1/turns/:id/prompt` for per-message “View Prompt” inspection (context prompt + optional stored system prompt).
- WS event mapping now decodes `generation_started`, `generation_metrics`, and `generation_finished`, preserving generation identity, source, optional conversation, samples, and outcome for the live monitor.
