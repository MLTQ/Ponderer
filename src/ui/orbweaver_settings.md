# orbweaver_settings.rs

## Purpose
Implements the OrbWeaver skill settings panel used inside the main tabbed settings window. It owns the configuration UI for the Graphchan/OrbWeaver integration instead of leaving that input in the generic settings form.

## Components

### `OrbWeaverSettingsPanel`
- **Does**: Renders the embedded OrbWeaver settings content for the current `AgentConfig`.
- **Interacts with**: `ui/settings.rs` tab renderer and `AgentConfig.graphchan_api_url`.

### `OrbWeaverSettingsPanel::render_contents`
- **Does**: Draws the Graphchan API URL input and explanatory text describing what the integration controls.
- **Interacts with**: `AgentConfig` fields edited in place.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `ui/settings.rs` | `render_contents(ui, config)` renders inline content without owning a window | Changing signature or switching to standalone-window semantics |
| `AgentConfig` | `graphchan_api_url` remains available | Renaming/removing the field |

## Notes
- This is intentionally an embedded tab panel, not a separate modal.
