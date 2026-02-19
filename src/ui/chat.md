# chat.rs

## Purpose
Renders the activity log and private chat stream for the API-only frontend. Supports collapsible tool/thinking metadata, media payload rendering, and turn-control display.

## Components

### `render_event_log(ui, events)`
- **Does**: Renders `FrontendEvent` items with color-coded formatting for observations, reasoning traces, tool progress, actions, orientation summaries, journal writes, concern lifecycle updates, and errors. `ReasoningTrace` items render as nested `CollapsingHeader` widgets (outer: step count; inner: per-step, truncated at 80 chars with expand-to-full). `ApprovalRequest` events are silently skipped here — they are rendered as a popup window by `app.rs`.
- **Interacts with**: `crate::api::FrontendEvent`.

### `render_private_chat(ui, messages, streaming_preview, media_cache) -> Option<String>`
- **Does**: Renders chat bubbles from `ChatMessage` records, including right-aligned operator rows, per-agent-message `View Prompt` controls (when `turn_id` exists), processing hints, metadata expanders, and inline media cards. Returns requested `turn_id` when the operator clicks a prompt-inspection button.
- **Interacts with**: `crate::api::ChatMessage`.

### `parse_chat_payload(content)`
- **Does**: Parses structured metadata blocks (`[tool_calls]`, `[thinking]`, `[media]`, `[turn_control]`) and strips hidden thinking tags from final text.
- **Interacts with**: Backend chat message formatter conventions.

### `ChatMediaCache`
- **Does**: Caches local image textures by path for efficient repeated media rendering.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `render_private_chat` returns optional turn-id prompt-inspection request and `render_event_log` signature remains stable | Signature changes break UI wiring |
| `api.rs` | `FrontendEvent` and `ChatMessage` fields expected by renderer remain compatible | Event/message schema changes require renderer updates |
| Backend message formatter | Metadata block tags remain stable | Renaming tags breaks payload parsing |

## Notes
- Thinking and tool-call expanders render below bubbles in full-width rows for readability.
- Long unbroken tokens are force-wrapped to keep message content visible in narrow windows.
- Streaming preview displays raw in-flight text until backend persists final response.
- Message rows use auto-height layout primitives (no fixed zero-height row allocations) to prevent bubble overlap/pileups when the pane is bottom-stuck.
- Chat content is rendered in a dedicated top-down layout scope so it is not affected by the parent composer's bottom-up anchoring.
- Chat scroll height now uses the exact remaining parent space (no forced minimum) to avoid overlap when the live tool panel expands.
- `CollapsingHeader` widgets use `id_salt((event_idx, "reasoning"))` and `(event_idx, step_idx)` tuples so open/closed state persists independently per item even when the event list grows.
- `FrontendEvent::ApprovalRequest` has a catch-all no-op arm in `render_event_log`; the popup is owned and rendered by `app.rs`.
