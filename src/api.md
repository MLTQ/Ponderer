# api.rs

## Purpose
Frontend-only backend API client for Ponderer. Encapsulates authenticated REST calls and websocket event streaming so the GUI can operate without direct in-process access to backend `Agent` or database internals.

## Components

### `ApiClient`
- **Does**: Stores backend base URL/token, performs REST requests for config/chat/agent control, and maintains WS event streaming with reconnect.
- **Interacts with**: `ponderer_backend` REST/WS routes under `/v1`.

### Chat DTOs (`ChatConversation`, `ChatMessage`, `ChatTurnPhase`)
- **Does**: Frontend-side models for chat list/history rendering.
- **Interacts with**: `ui/app.rs` conversation picker and chat renderer.
- **Notes**: `ChatMessage.turn_id` is optional and used to fetch turn-level prompt diagnostics.

### Prompt DTOs (`ChatTurnPrompt`)
- **Does**: Carries prompt-inspection payload for one turn (`prompt_text` plus optional `system_prompt_text`).
- **Interacts with**: `ui/app.rs` prompt inspector window.

### Runtime DTOs (`AgentVisualState`, `AgentRuntimeStatus`)
- **Does**: Frontend-side models for status badges/sprite selection and pause/stop controls.
- **Interacts with**: `ui/sprite.rs`, `ui/avatar.rs`, `ui/app.rs` header status.

### `FrontendEvent`
- **Does**: Normalized UI event stream derived from backend WS envelopes. Includes `ApprovalRequest { tool_name, reason }` for interactive approval popups.
- **Interacts with**: `ui/chat.rs` activity log and `ui/app.rs` streaming preview/tool-progress state and approval popup.

### `ApiClient::approve_tool`
- **Does**: `POST /v1/agent/tools/:tool_name/approve` — grants session-level approval so the named tool can run autonomously without further prompts.
- **Interacts with**: `server.rs` `approve_tool` handler → `Agent::grant_session_tool_approval` → `ToolRegistry::grant_session_approval`.

### Event mapping (`stream_events_forever`, `stream_events_once`, `map_event`)
- **Does**: Reads WS JSON envelopes, maps backend event types to `FrontendEvent`, and reconnects on disconnect/failure.
- **Interacts with**: `ponderer_backend/src/server.rs` event schema.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `ui/app.rs` | `ApiClient` exposes config/chat/status/pause/stop methods and event streaming | Renaming/removing API methods |
| `ui/chat.rs` | `FrontendEvent` variants remain stable for rendering | Removing/renaming event variants |
| Backend API | Routes and payload shapes under `/v1` match client decoding | Changing endpoint paths or schema fields |

## Notes
- Backend URL defaults to `http://127.0.0.1:8787` (`PONDERER_BACKEND_URL` override).
- Bearer token comes from `PONDERER_BACKEND_TOKEN`; if absent, requests run unauthenticated (useful only when backend auth mode is disabled).
- WS URL is derived from HTTP base URL (`http -> ws`, `https -> wss`).
- Enum decoding for chat/runtime state is compatibility-tolerant (`snake_case` plus legacy PascalCase aliases) to survive backend/frontend schema drift during upgrades.
- Conversation list decode errors now include payload preview context to simplify diagnosing response-shape mismatches.
- `ApiClient::get_turn_prompt` fetches `/v1/turns/:id/prompt` for per-message “View Prompt” inspection (context prompt + optional stored system prompt).
