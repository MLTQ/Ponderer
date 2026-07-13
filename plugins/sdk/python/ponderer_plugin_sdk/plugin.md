# plugin.py

## Purpose

Provides the domain-author-facing registration and callback surface. It infers
capabilities from real handlers so handshake declarations cannot drift from
runtime behavior.

## Components

### `Plugin`
- **Does**: Owns metadata, default/current settings, and registered tool, event,
  prompt, and polling callbacks plus explicitly requested host capabilities,
  a restored state snapshot, pending state mutations, and current invocation
  context.
- **Interacts with**: `PluginServer` in `server.py` and wire dataclasses in
  `models.py`.

### `Plugin.tool`
- **Does**: Registers exactly one callback for a `ToolManifest`.
- **Rationale**: One registration is both the handshake declaration and dispatch
  target, preventing the duplicate tool lists present in older Orbs.

### `Plugin.on_event` / `Plugin.on_prompt` / `Plugin.on_poll`
- **Does**: Register lifecycle hooks, bounded prompt providers, and a single
  external-event poller; their presence determines capabilities.

### Callback dispatch methods
- **Does**: Normalize typed models and migration-friendly mapping returns, reject
  wrong-slot prompt contributions, and attach the trusted plugin ID.

### `state` / `set_state` / `delete_state`
- **Does**: Exposes a read-only view of the host-restored namespace and stages
  schema-versioned mutations for the next successful protocol response.
- **Isolation**: Values are deep-copied at the API boundary so nested caller
  mutation cannot silently change local state without a new `set_state` call.
- **Interacts with**: `PluginServer`, which drains mutations after configure,
  event, prompt, poll, and tool callbacks.

### State response transaction helpers
- **Does**: Checkpoint state per request, expose pending mutations without
  consuming them, commit after successful response serialization, and restore
  the checkpoint when callback or serialization fails.
- **Rationale**: A failed tool must not mutate process state or leak a stale
  mutation into an unrelated later response.

### `invocation_context`
- **Does**: Exposes host-supplied scope/time metadata only while a tool callback
  is executing and clears it in a `finally` block.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Domain plugins | Decorators return the original callable; registration is ordered and unique | Decorator or duplicate behavior |
| `server.py` | Public dispatch methods return SDK wire models | Method signatures or return types |
| Host handshake | Contribution names match callbacks; requested authority stays an explicit constructor input | Capability inference changes |

## Notes

Callbacks are synchronous because the current subprocess protocol serializes
one request at a time. Plugins can manage their own workers internally when a
domain operation needs concurrency.
