# chat.rs

## Purpose
Provides two scrollable display panels: an event log showing agent activity and a private chat interface for operator-agent communication. Chat rendering also supports inline, collapsible tool-call/thinking details, media payloads, and turn-control metadata emitted by the agent loop.

## Components

### `render_event_log(ui, events)`
- **Does**: Renders a scrollable list of `AgentEvent` variants with color-coded labels: observations (light blue), reasoning traces (gray, grouped), tool progress (khaki), actions (green), orientation updates (light yellow), journal writes (light green), concern lifecycle updates, and errors (red). `StateChanged` and `ChatStreaming` events are skipped (rendered elsewhere).
- **Interacts with**: `AgentEvent` enum from `crate::agent`

### `render_private_chat(ui, messages, streaming_preview, media_cache)`
- **Does**: Renders a chat-style message list from `ChatMessage` records. Operator messages are snapped to the right; agent messages are left-aligned. Shows a processing indicator for unprocessed operator messages, collapsible tool/thinking details when present, inline media cards (image/audio/video/file), and an inline live preview bubble while provider tokens stream.
- **Interacts with**: `ChatMessage` from `crate::database` (fields: `role`, `content`, `created_at`, `processed`)

### `parse_chat_payload(content)`
- **Does**: Parses optional `[tool_calls]...[/tool_calls]`, `[thinking]...[/thinking]`, `[media]...[/media]`, and `[turn_control]...[/turn_control]` metadata blocks, strips legacy `<think>/<thinking>` tags, and returns cleaned display text + structured metadata.
- **Interacts with**: Agent-side message formatter in `agent/mod.rs`

### `ChatMediaCache`
- **Does**: Caches decoded image textures by local file path so image previews can render efficiently inside chat bubbles without re-decoding every frame
- **Interacts with**: `render_media_panel`, `image` crate, `egui::Context::load_texture`

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `render_private_chat` takes `&mut ChatMediaCache` and message slices | Changing signature breaks `AgentApp::update` |
| `AgentEvent` | Variants: `Observation(String)`, `ReasoningTrace(Vec<String>)`, `ToolCallProgress { ... }`, `ActionTaken { action, result }`, `OrientationUpdate(...)`, `JournalWritten(String)`, `ConcernCreated { ... }`, `ConcernTouched { ... }`, `Error(String)`, `StateChanged(...)`, `ChatStreaming { ... }` | Removing/renaming breaks match arms |
| `ChatMessage` | Fields `role`, `content`, `created_at` (with `.format()`), `processed` | Renaming fields breaks rendering |
| `agent/mod.rs` | Metadata delimiters and JSON shape for tool/thinking/media/turn-control blocks | Changing block names or payload fields without parser updates |

## Notes
- `stick_to_bottom(true)` keeps the scroll pinned to newest content.
- Private chat reserves ~140px for the composer section rendered by `app.rs`; this avoids the scroll area consuming all height.
- Each chat message row allocates full panel width before layout, and operator rows add a computed left spacer so bubbles stay snapped to the right edge even in narrow windows.
- Each bubble is rendered inside a fixed-width slot (bounded to panel width) before drawing content, preventing right-aligned operator messages from drifting off-screen in small windows.
- When both metadata sets exist, Thinking and Tool Calls sections render in side-by-side columns to avoid overlap.
- Long unbroken tokens (paths/JSON/chunks without spaces) are force-wrapped based on bubble width, and labels are rendered with explicit wrapping so text always remains visible in the bubble.
- Streaming preview intentionally shows raw in-flight text (including internal narration markers) before final post-processing/persistence.
- Orientation events render as compact summaries (`disposition`, anomaly count, salience count) in the activity log instead of full payload dumps.
- Journal events render as compact summaries (`entry_type: preview`) to confirm ambient journaling without exposing full structured payloads.
- Concern events render as concise create/touch lines with short IDs so concern lifecycle changes are visible without opening database internals.
- Tool and thinking detail blocks are intentionally hidden by default behind `egui::CollapsingHeader` sections to keep chat readable.
- Thinking and tool-call expanders are rendered in a dedicated full-width row beneath each message bubble, stacked vertically so expanded content stays readable.
- Image media entries attempt inline previews from local paths; non-image media shows typed file cards with path + MIME details.
- Turn-control metadata is rendered as a lightweight separator row in the stream (not as a bubble), and malformed open-ended `[turn_control]` blocks are tolerated so chat text remains clean.
