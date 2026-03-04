# plugin_settings_form.rs

## Purpose
Renders schema-driven settings controls for plugin tabs. This is the generic UI path that lets new workflow plugins define their own fields without requiring a frontend recompile.

## Components

### `PluginSettingsForm`
- **Does**: Applies schema defaults into `AgentConfig.plugin_settings` and renders each declared field using generic egui controls.
- **Interacts with**: `api.rs` plugin manifest DTOs and `settings.rs`.

### Value helpers
- **Does**: Normalize per-plugin settings storage into a JSON object, seed default values, and parse dynamic numeric fields.
- **Interacts with**: `config::AgentConfig` persisted config payload.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `settings.rs` | `render(ui, config, plugin_id, schema)` mutates `AgentConfig.plugin_settings` in place | Changing the render signature |
| Plugin manifests | Supported field kinds map to stable egui controls | Removing a field kind or changing its storage semantics |

## Notes
- This intentionally supports only a small field vocabulary for now: booleans, strings, textareas, numbers, selects, and path-like text inputs.
