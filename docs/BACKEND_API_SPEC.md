# Ponderer Backend API Spec (v1)

This document defines the backend contract used by the decoupled frontend.

Base URL default: `http://127.0.0.1:8787`

Prefix: `/v1`

## Auth

Auth mode is controlled by env var:

- `PONDERER_BACKEND_AUTH_MODE=required` (default)
- `PONDERER_BACKEND_AUTH_MODE=disabled`

When mode is `required`, send:

```http
Authorization: Bearer <PONDERER_BACKEND_TOKEN>
```

All `/v1/*` routes require auth in required mode (including `/v1/health`).

## REST endpoints

### Health and config

- `GET /v1/health`
  - Response: `{ "status": "ok" }`

- `GET /v1/config`
  - Response: `AgentConfig` JSON

- `PUT /v1/config`
  - Body: full `AgentConfig` JSON
  - Response: persisted `AgentConfig` JSON

### Plugins

- `GET /v1/plugins`
  - Response: array of `BackendPluginManifest`
  - Includes built-in manifests such as `builtin.core`, `builtin.comfy`, and `builtin.orbweaver`, plus any filesystem workflow bundles discovered under the plugin directory
  - `BackendPluginManifest.kind` distinguishes built-ins from workflow bundles
  - `BackendPluginManifest.settings_tab` is optional and contains `{ "id", "title", "order" }` when the plugin wants a dedicated settings tab in the frontend
  - `BackendPluginManifest.settings_schema` is optional and, when present, contains declarative form fields the frontend can render without shipping plugin-specific UI code

### Conversations and messages

- `GET /v1/conversations?limit=<n>`
  - Response: `ChatConversation[]`

- `POST /v1/conversations`
  - Body: `{ "title": "optional" }`
  - Response: created `ChatConversation`

- `GET /v1/conversations/:id`
  - Response: `ChatConversation`

- `GET /v1/conversations/:id/summary`
  - Response: `ChatConversationSummary | null`

- `GET /v1/conversations/:id/messages?limit=<n>`
  - Response: `ChatMessage[]` (chronological)

- `POST /v1/conversations/:id/messages`
  - Body: `{ "content": "..." }`
  - Response: `{ "status": "queued", "message_id": "..." }`

### Turn and tool diagnostics

- `GET /v1/conversations/:id/turns?limit=<n>`
  - Response: `ChatTurn[]`

- `GET /v1/turns/:id/tool-calls`
  - Response: `ChatTurnToolCall[]`

### Scheduled jobs

- `GET /v1/scheduled-jobs?limit=<n>`
  - Response: `ScheduledJob[]`

- `POST /v1/scheduled-jobs`
  - Body: `{ "name": "...", "prompt": "...", "interval_minutes": 60 }`
  - Response: created `ScheduledJob`

- `GET /v1/scheduled-jobs/:id`
  - Response: `ScheduledJob`

- `PUT /v1/scheduled-jobs/:id`
  - Body: `{ "name"?: "...", "prompt"?: "...", "interval_minutes"?: 60, "enabled"?: true|false }`
  - Response: updated `ScheduledJob`

- `DELETE /v1/scheduled-jobs/:id`
  - Response: `204 No Content`

### Background processes

- `GET /v1/processes`
  - Response: `ProcessInfo[]`

- `GET /v1/processes/:id`
  - Response: `ProcessInfo`

- `POST /v1/processes/:id/stop`
  - Response: updated `ProcessInfo`

### Agent control

- `GET /v1/agent/status`
  - Response: `AgentRuntimeStatus`

- `PUT /v1/agent/pause`
  - Body: `{ "paused": true|false }`
  - Response: `{ "paused": true|false }`

- `POST /v1/agent/toggle-pause`
  - Response: `{ "paused": true|false }`

## WebSocket event stream

- Endpoint: `GET /v1/ws/events` (same bearer auth rule)
- URL conversion: `http -> ws`, `https -> wss`

Envelope:

```json
{
  "event_type": "chat_streaming",
  "emitted_at": "2026-02-17T05:19:24.986007Z",
  "payload": { ... }
}
```

### Event types and payloads

- `state_changed`
  - `{ "state": "Idle|Reading|Thinking|Writing|Happy|Confused|Paused" }`
- `observation`
  - `{ "text": "..." }`
- `reasoning_trace`
  - `{ "steps": ["..."] }`
- `tool_call_progress`
  - `{ "conversation_id": "...", "tool_name": "...", "output_preview": "..." }`
- `chat_streaming`
  - `{ "conversation_id": "...", "content": "...", "done": true|false }`
- `action_taken`
  - `{ "action": "...", "result": "..." }`
- `orientation_update`
  - orientation snapshot JSON payload
- `journal_written`
  - `{ "summary": "..." }`
- `concern_created`
  - `{ "id": "...", "summary": "..." }`
- `concern_touched`
  - `{ "id": "...", "summary": "..." }`
- `error`
  - `{ "error": "..." }`

## Plugin extension contract

Backend extensions implement `BackendPlugin` in:

- `ponderer_backend/src/plugin.rs`

Core interfaces:

- `BackendPlugin::manifest() -> BackendPluginManifest`
- `BackendPlugin::register_tools(tool_registry, config)`
- `BackendPlugin::build_skills(config)`

Runtime registration path:

- `BackendRuntimeBuilder::with_plugin(...)`
- `BackendRuntimeBuilder::with_plugins(...)`

Loaded plugin manifests are exposed via `GET /v1/plugins`.

## Frontend integration pattern

Reference client implementation:

- `src/api.rs`
- `src/ui/app.rs`

Pattern:

1. REST for state snapshots (config, conversations, history, status).
2. WS for live events (streaming tokens, tool progress, activity).
3. Periodic REST refresh for reconciliation.
4. Treat WS disconnect as recoverable; reconnect with backoff.

`ScheduledJob` includes: `id`, `name`, `prompt`, `interval_minutes`, `conversation_id`, `enabled`, `last_run_at`, `next_run_at`, `created_at`, `updated_at`.

`ProcessInfo` includes: `id`, `command`, `working_directory`, `pid`, `status`, `exit_code`, `started_at`, `finished_at`, `recent_output`.

## Smoke validation

Use:

```bash
./scripts/validate_backend_standalone.sh
```

This validates standalone startup, auth boundaries, conversation/message APIs, status, and plugins.
