# sprite.rs

## Purpose
Renders a compact 36px header avatar when available, or a palette-derived technical `P_` mark. Actual execution state is displayed separately in the workbench header.

## Components

### `render_agent_sprite(ui, state, avatars)`
- **Does**: Renders animated avatar frames for the current `AgentVisualState` or falls back to the mark.
- **Interacts with**: `AvatarSet::get_for_state`, `crate::api::AgentVisualState`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `render_agent_sprite` signature stability | Signature change breaks header rendering |
| `api.rs` | `AgentVisualState` variants used here remain available | Variant rename/removal breaks mapping |
| `avatar.rs` | Avatar public methods used for rendering remain stable | API changes break animated avatar path |
