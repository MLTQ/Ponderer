# models.py

## Purpose

Defines typed, stdlib-only dataclasses for every plugin-facing protocol-v1
payload. Domain plugins use these helpers instead of assembling wire dictionaries.

## Components

### `PluginEffect` / `ToolCategory` / `ToolManifest`
- **Does**: Describe one LLM-callable tool, its semantic effects, and serialize
  the protocol-v1 host schema.
- **Interacts with**: tool decorators in `plugin.py` and proxy registration in Rust.

### `PromptSlot` / `PromptKind`
- **Does**: Name canonical dotted prompt slots and contribution authority kinds.
- **Rationale**: `PromptSlot.from_wire` also accepts the original host's snake-case
  names, while `legacy_wire_name` supports reflected compatibility responses.

### `PluginMetadata` / `Capabilities` / `Handshake`
- **Does**: Build an identity-checked, versioned handshake from actual registered
  handlers plus explicit requested authority rather than duplicated contribution
  lists.

### Lifecycle and prompt models
- **Does**: `LifecycleEvent`, `EventAck`, `PromptContext`, `PromptQuery`, and
  `PromptContribution` type plugin callbacks and their responses.

### `StateMutation` / `InvocationContext`
- **Does**: Represent versioned durable state upserts/deletes and the host's
  conversation, loop, user, working-directory, invocation-time, and deadline
  scope for a tool call.
- **Rationale**: Plugins persist through an authenticated host namespace instead
  of inventing private database files, and they can reason about elapsed time
  without trusting their own process lifetime.
- **Validation**: State values must be strict JSON before local state changes,
  so non-finite numbers or Python-only objects cannot create a false impression
  that a mutation reached the host.

### `PollEvent` / `ToolResult`
- **Does**: Normalize external observations and text/JSON/error tool results into
  the envelopes consumed by the current host; event acknowledgements and tool
  results may carry state mutations.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `plugin.py` | Models validate mappings and expose `to_wire`/`from_value` | Constructor fields or coercion behavior |
| `server.py` | Every result can be serialized without plugin-specific knowledge | Wire field names |
| Existing host | Tool categories/result kinds stay stable; added effects and capability requests default empty | Enum value changes |
| Protocol-v1 host | Prompt slots serialize dotted; snake aliases remain accepted | Slot normalization rules |
