# comfy_settings.rs

## Purpose
Implements the ComfyUI Workflow panel, allowing users to import workflows from PNG or JSON files, configure which workflow inputs the agent can modify, test the ComfyUI connection, and persist workflow settings to the agent config.

## Components

### `ComfySettingsPanel`
- **Does**: Holds the loaded `ComfyWorkflow`, cached preview texture, import error, and test connection status
- **Interacts with**: `ComfyWorkflow`/`ControllableInput`/`InputType` from `crate::comfy_workflow`, `AgentConfig` from `crate::config`

### `ComfySettingsPanel::new()`
- **Does**: Constructs with no workflow loaded

### `ComfySettingsPanel::render_contents(ui, ctx, config) -> bool`
- **Does**: Renders the same workflow-management UI inline so it can live inside a tab in the main settings window.
- **Interacts with**: `ui/settings.rs` tab renderer and the current workflow/config state.

### `ComfySettingsPanel::import_workflow_png(path)`
- **Does**: Loads a workflow from an embedded PNG via `ComfyWorkflow::from_png`, sets workflow name from filename
- **Interacts with**: `crate::comfy_workflow::ComfyWorkflow::from_png`

### `ComfySettingsPanel::import_workflow_json(path)`
- **Does**: Loads a workflow from a JSON file via `ComfyWorkflow::from_json_file`, sets workflow name from filename
- **Interacts with**: `crate::comfy_workflow::ComfyWorkflow::from_json_file`

### `ComfySettingsPanel::test_workflow(config)`
- **Does**: Creates a `ComfyUIClient` and calls `test_connection()` synchronously via a temporary tokio runtime. Updates `test_status` with success/failure.
- **Interacts with**: `crate::comfy_client::ComfyUIClient`

### `ComfySettingsPanel::sync_workflow_to_config(config)`
- **Does**: Serializes the current `ComfyWorkflow` to JSON and writes it to `config.workflow_settings`. Also copies `preview_image_path` to `config.workflow_path`.
- **Interacts with**: `serde_json`, `AgentConfig` fields `workflow_settings`, `workflow_path`

### `ComfySettingsPanel::load_workflow_from_config(config)`
- **Does**: Deserializes a `ComfyWorkflow` from `config.workflow_settings` JSON string (called at app startup)
- **Interacts with**: `serde_json`, `AgentConfig`

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `settings.rs` | `render_contents(ui, ctx, &mut config) -> bool`; `load_workflow_from_config(&config)` | Changing signatures breaks settings integration |
| `AgentConfig` | Fields: `workflow_settings` (Option<String>), `workflow_path` (Option<String>), `enable_image_generation`, `comfyui.api_url` | Renaming these fields breaks this panel |
| `ComfyWorkflow` | `from_png`, `from_json_file`, `Serialize`/`Deserialize`, fields: `name`, `output_node_id`, `controllable_nodes`, `preview_image_path` | Changing workflow struct breaks import/save |
| `ComfyUIClient` | `new(url)`, `test_connection() -> async Result` | Changing client API breaks test button |

## Notes
- `test_workflow` creates a new `tokio::runtime::Runtime` on each invocation, which is blocking. This is acceptable because it only runs on user click.
- The workflow is stored as a serialized JSON string in `AgentConfig` rather than as a structured field, keeping the config format flexible.
