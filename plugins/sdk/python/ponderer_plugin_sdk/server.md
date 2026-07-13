# server.py

## Purpose

Owns method dispatch and the process stdio loop. It is the only layer that turns
wire dictionaries into domain callbacks or catches domain exceptions.

## Components

### `PluginServer.handle_line`
- **Does**: Decodes one line, preserves its request ID where possible, dispatches,
  and emits exactly one success/error line without terminating the server.
- **Failure boundary**: Logs exception details to plugin stderr while returning
  only stable, bounded `plugin_error` or `serialization_error` messages to the
  host.
- **State transaction**: Checkpoints plugin state before dispatch, commits
  pending mutations only after the success response serializes, and rolls back
  callback or serialization failures.
- **Interacts with**: framing in `protocol.py` and callbacks in `plugin.py`.

### `PluginServer.dispatch`
- **Does**: Implements `plugin.handshake`, `plugin.configure`,
  `plugin.handle_event`, `plugin.get_prompt_contributions`,
  `plugin.poll_events`, and `plugin.invoke_tool`.
- **Rationale**: Plugins supply domain handlers, never their own RPC routers.
- **State/time**: Configuration restores the host-owned state snapshot; every
  successful callback returns namespaced state mutations; tool callbacks receive host
  invocation scope and deadline metadata.

### `PluginServer.serve` / `serve_stdio`
- **Does**: Read until EOF, skip blank lines, flush every response, and keep
  stdout reserved for protocol traffic.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Ponderer host | One response line per nonblank request; IDs correlate | Framing or response behavior |
| Existing host | Snake-case prompt queries receive snake-case contributions | Compatibility reflection removal |
| Protocol-v1 host | Handshake negotiates and dotted prompt queries stay dotted | Negotiation or slot behavior |
| `testing.py` | `handle_line` exercises the same boundary as stdio | Return type or newline behavior |
