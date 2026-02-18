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

### Runtime DTOs (`AgentVisualState`, `AgentRuntimeStatus`)
- **Does**: Frontend-side models for status badges/sprite selection and pause/stop controls.
- **Interacts with**: `ui/sprite.rs`, `ui/avatar.rs`, `ui/app.rs` header status.

### `FrontendEvent`
- **Does**: Normalized UI event stream derived from backend WS envelopes.
- **Interacts with**: `ui/chat.rs` activity log and `ui/app.rs` streaming preview/tool-progress state.

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
