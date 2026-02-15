# capability_profiles.rs

## Purpose
Defines explicit tool-capability profiles for agent loop contexts (`private_chat`, `skill_events`, `heartbeat`) and resolves config overrides into executable `ToolContext` policies.

## Components

### `AgentCapabilityProfile`
- **Does**: Enumerates the policy domains used by the agent loops
- **Interacts with**: `agent/mod.rs` when constructing per-loop tool contexts

### `ToolCapabilityPolicy`
- **Does**: Holds resolved policy values (`autonomous`, allowlist, denylist) before conversion to `ToolContext`
- **Interacts with**: `tools/mod.rs` runtime gating via `ToolContext::allows_tool`

### `resolve_capability_policy`
- **Does**: Merges profile defaults with config overrides for one profile
- **Interacts with**: `config.rs` (`CapabilityProfileConfig`)
- **Rationale**: Keeps policy semantics centralized and testable outside the main loop

### `build_tool_context_for_profile`
- **Does**: Builds a ready-to-use `ToolContext` for a loop using resolved policy
- **Interacts with**: `agent/mod.rs` heartbeat, skill-event, and private-chat flows

### Policy tests
- **Does**: Verifies default tool blocks/permissions per loop and override behavior
- **Interacts with**: Guards against regressions where loop contexts accidentally gain or lose tool capabilities

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `agent/mod.rs` | Stable profile names and `build_tool_context_for_profile` behavior | Renaming profile variants or changing merge semantics |
| `config.rs` | Override fields map to policy replacement semantics | Changing override field names/types without migration |
| `tools/mod.rs` | Case-insensitive tool-name lists in `ToolContext` | Removing normalization or changing allow/deny precedence |

## Notes
- `allowed_tools` and `disallowed_tools` overrides are replacement-based when provided.
- Tool names are normalized (trimmed, deduplicated case-insensitively) before policy application.
