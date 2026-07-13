# plugin.py

## Purpose

Adapts Graphchan's domain client to the shared Ponderer Python SDK. It owns
configuration, event filtering, polling, and the three stable Graphchan tools,
while transport and protocol negotiation remain in the SDK.

## Components

### Tool/effect contract
- **Does**: Loads `graphchan_reply`, `graphchan_list_threads`, and
  `graphchan_post` from the package's canonical `tools.json`, including schemas,
  approval hints, and `network.read` / `external.publish` effects.
- **Rationale**: Static host policy and runtime registration consume one source
  of truth rather than duplicating authority declarations in Python.

### `GraphchanApi` / `GraphchanClientFactory`
- **Does**: Define the narrow client seam used by runtime code and offline fakes.
- **Interacts with**: `GraphchanClient` in `client.py`.

### `GraphchanPlugin`
- **Does**: Registers real handlers so the SDK derives tools, event hooks, and
  polling capabilities; validates settings mappings and the HTTP(S) API URL,
  then atomically commits merged settings and the rebuilt API client.
- **Interacts with**: `Plugin` in the shared SDK and `server.py` entrypoint.

### `GraphchanPlugin.poll`
- **Does**: Clamps the configured limit to 1-200, filters posts attributed to the
  configured agent, isolates malformed/duplicate records within a response, and
  emits typed polling events for records with stable string IDs and bodies.

### Tool handlers
- **Does**: List threads, resolve/publish replies, and create top-level posts
  using SDK `ToolResult` values while preserving legacy tool names and results.

### `build_plugin`
- **Does**: Constructs the production plugin or a test instance with an injected
  client factory.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `server.py` | `build_plugin()` returns the configured SDK adapter | Factory signature |
| Runtime host | Stable tool names/schemas, polling fields, and write approvals | Manifest or result changes |
| Tests | Client construction is injectable and performs no I/O by itself | Removing factory seam |

## Notes

- Polling is a bounded recent-window query, not a source cursor or paginated
  feed. It can miss posts that leave the window between polls and intentionally
  re-emits candidates after restart; the host ledger must deduplicate by post ID.
- `metadata.agent.name` is self-declared Graphchan data. Name matching prevents
  ordinary echo loops but is not authenticated provenance and can be spoofed.
- The emitted author is Graphchan's `author_peer_id` string (or `Anonymous`),
  not a cryptographic identity assertion. The event source prefers the display
  thread title and falls back to the thread ID.
- A rejected configuration leaves the last valid settings/client pair active;
  validation failures never partially mutate live plugin configuration.
