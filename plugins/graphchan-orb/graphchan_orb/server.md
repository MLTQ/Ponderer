# server.py

## Purpose
Implements Graphchan-Orb's newline-delimited JSON-RPC runtime-process protocol. It declares tools and skill polling, applies settings, filters the agent's own posts, and maps host calls onto `GraphchanClient`.

## Components

### `main` / `_handle_line` / `_dispatch`
- **Does**: Reads one request per stdin line and returns one correlated success/error response line.
- **Interacts with**: Ponderer's `RuntimePluginHost`.

### `_handshake`
- **Does**: Declares plugin identity, skill polling, settings event hook, and three tool manifests.
- **Interacts with**: runtime proxy registration and autonomous approval gating.
- **Rationale**: `graphchan_reply` and `graphchan_post` require approval because both create externally visible content; listing is read-only.

### `_configure` / `_handle_event`
- **Does**: Validate object settings, merge safe defaults, rebuild the HTTP client, and accept host `settings_changed` events.
- **Interacts with**: `settings.schema.json`, `GraphchanClient`.

### `_poll_events`
- **Does**: Fetch recent posts with a bounded limit, omit posts matching configured `agent_name`, and normalize host event fields.
- **Interacts with**: Ponderer's skill polling flow.

### Tool handlers
- **Does**: List threads, resolve and publish replies, and publish top-level posts using host-compatible text/JSON/error envelopes.
- **Interacts with**: `plugin.invoke_tool`, `GraphchanClient`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Runtime host | Handshake id matches `plugin.toml`; RPC responses preserve request ids | Identity or envelope changes |
| Skill polling | `plugin.poll_events` returns `{events: [...]}` with id/source/author/body/parent_ids | Field changes |
| Tool proxy | Tool results use `kind=text|json|error`, with `text` or `data` | Result-envelope changes |
| Operators | Plugin disabled by default; write tools approval-gated | Default or manifest approval changes |

## Notes
- The host does not filter the plugin's own posts, so filtering is performed here using the configured agent name.
- Poll limits are clamped to 1–200 even when settings contain invalid values.
