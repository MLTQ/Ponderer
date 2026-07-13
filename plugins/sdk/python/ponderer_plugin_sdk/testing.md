# testing.py

## Purpose

Provides a serialized fake host plus reusable baseline conformance tests. Domain
plugin tests can exercise the production framing/dispatch path without spawning a
process or contacting external services.

## Components

### `FakeHost`
- **Does**: Assigns request IDs, emits legacy or versioned JSON envelopes through
  `PluginServer.handle_line`, verifies correlation/framing, and exposes helpers
  for every host method. Versioned handshakes include the canonical host
  descriptor used by the Rust runtime; configure and tool helpers can also send
  durable state snapshots and invocation context.

### `HostCallError`
- **Does**: Preserves error codes/messages from failed fake-host calls.

### `validate_handshake`
- **Does**: Checks identity, protocol, capability/tool agreement, semantic
  effects, schemas, categories, and canonical prompt-slot names without raising
  on the first issue.

### `PluginConformanceMixin`
- **Does**: Adds legacy/v1 handshake, empty configuration, and correlated unknown
  method tests to a plugin's `unittest.TestCase`.
- **Rationale**: It is a mixin rather than a concrete test case so SDK discovery
  does not instantiate an abstract plugin fixture.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| Plugin test suites | Implementing `make_plugin()` is sufficient for baseline tests | Mixin hook or test assumptions |
| SDK tests | Fake calls cross JSON encode/decode and enforce one newline | Bypassing server framing |
| Future migration tooling | Findings are deterministic strings in discovery order | Finding wording/order |
