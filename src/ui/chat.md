# chat.rs

## Purpose
Renders the activity log and private chat stream for the API-only frontend. Supports collapsible tool/thinking metadata, media payload rendering, and turn-control display.

## Components

### `render_event_log(ui, events)`
- **Does**: Renders `FrontendEvent` items with color-coded formatting for observations, reasoning traces, tool progress, actions, orientation summaries, journal writes, concern lifecycle updates, and errors.
- **Interacts with**: `crate::api::FrontendEvent`.

### `render_private_chat(ui, messages, streaming_preview, media_cache)`
- **Does**: Renders chat bubbles from `ChatMessage` records, including right-aligned operator rows, processing hints, metadata expanders, and inline media cards. Uses full available pane height (caller controls composer/live-output layout).
- **Interacts with**: `crate::api::ChatMessage`.

### `parse_chat_payload(content)`
- **Does**: Parses structured metadata blocks (`[tool_calls]`, `[thinking]`, `[media]`, `[turn_control]`) and strips hidden thinking tags from final text.
- **Interacts with**: Backend chat message formatter conventions.

### `ChatMediaCache`
- **Does**: Caches local image textures by path for efficient repeated media rendering.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `render_private_chat` and `render_event_log` signatures remain stable | Signature changes break UI wiring |
| `api.rs` | `FrontendEvent` and `ChatMessage` fields expected by renderer remain compatible | Event/message schema changes require renderer updates |
| Backend message formatter | Metadata block tags remain stable | Renaming tags breaks payload parsing |

## Notes
- Thinking and tool-call expanders render below bubbles in full-width rows for readability.
- Long unbroken tokens are force-wrapped to keep message content visible in narrow windows.
- Streaming preview displays raw in-flight text until backend persists final response.
