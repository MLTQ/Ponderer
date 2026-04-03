# settings.rs

## Purpose
Implements the tabbed Settings window for the desktop UI. It keeps core agent settings in fixed tabs, preserves native tabs for built-in integrations like ComfyUI and OrbWeaver, and appends schema-driven plugin tabs for discovered workflow bundles when the backend advertises them.

## Components

### `SettingsPanel`
- **Does**: Holds the editable `AgentConfig`, visibility state, selected tab, discovered plugin manifests, embedded skill-specific subpanels, plus scheduled-job draft/edit/delete state that is flushed into the action queue only when the shared save button is used.
- **Interacts with**: `AgentConfig`, `api::{BackendPluginManifest,ScheduledJob}`, `comfy_settings.rs`, `orbweaver_settings.rs`, `app.rs` schedule action dispatcher

### `SettingsPanel::set_plugin_manifests`
- **Does**: Stores startup plugin discovery data used to decide which skill tabs to show.
- **Interacts with**: `ui/app.rs` startup backend plugin fetch.

### `SettingsPanel::sync_from_config`
- **Does**: Replaces local config state from a saved backend config and reloads skill-panel derived state (notably the Comfy workflow cache).
- **Interacts with**: `ui/app.rs` after config persistence.

### `SettingsPanel::open` / `SettingsPanel::open_tab`
- **Does**: Opens the settings window, optionally selecting a specific tab (for example the Comfy skill tab).
- **Interacts with**: `ui/app.rs` toolbar actions.

### Scheduled-job state methods (`set_scheduled_jobs`, `set_scheduled_jobs_error`, `take_scheduled_job_actions`)
- **Does**: Synchronizes backend schedule snapshots/errors into the UI and emits queued save-time CRUD actions back to `app.rs`.
- **Interacts with**: `api.rs` scheduled-job endpoints (indirectly through `app.rs`)

### `queue_dirty_scheduled_job_updates`
- **Does**: Collects all staged schedule creates, edits, and deletions, validates them, and enqueues the corresponding `Create` / `Update` / `Delete` actions so the global `Save & Apply` button is the single commit point for the schedules tab.
- **Interacts with**: `render`, scheduled-job editor/draft state, and `app.rs` schedule action dispatcher.

### `SettingsPanel::render(ctx) -> Option<AgentConfig>`
- **Does**: Draws the tabbed settings window and returns `Some(config)` when the user clicks `Save & Apply`. Before returning, it now flushes all staged schedule creates/edits/deletes into the action queue so the settings window has one shared save contract.
- **Interacts with**: `ui/app.rs` for persistence through the backend API.

### Core tab renderers
- **Does**: Render grouped core settings tabs: `General`, `Behavior`, `Living Loop`, `Memory`, `System`, and `Schedules`.
- **Interacts with**: top-level `AgentConfig` fields.
- **Notes**: Behavior tab focuses on autonomous loop limits and loop-heat controls; the stale private-chat mode selector was removed after the config field disappeared.

### `render_schedules_tab`
- **Does**: Shows all schedules, lets operators stage enabled/name/prompt/interval changes, stage new schedules, stage deletions, and manually refresh backend state. The tab no longer applies row-local saves; it relies on the shared `Save & Apply` button.
- **Interacts with**: local scheduled-job editor/draft state and the save-time `ScheduledJobAction` queue consumed by `app.rs`.

### Skill tab renderers
- **Does**: Render plugin-specific tabs for supported built-ins (`skill.comfy`, `skill.orbweaver`) using native Rust panels, and falls back to the generic plugin form renderer for any other manifest that includes a settings schema.
- **Interacts with**: `ComfySettingsPanel`, `OrbWeaverSettingsPanel`, and `plugin_settings_form.rs`.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `config` remains `pub`; `render()` returns `Option<AgentConfig>`; `open_tab()` selects a valid tab ID | Making config private or changing these signatures |
| `api.rs` | `BackendPluginManifest.settings_tab` contains `id`, `title`, `order` when a plugin wants a settings tab | Renaming/removing settings-tab fields |
| `api.rs` / plugin manifests | Generic plugin tabs require `settings_schema` to be present | Removing schema handling or changing field semantics |
| `comfy_settings.rs` | `sync_workflow_to_config` and `render_contents` remain available for the legacy built-in Comfy tab | Removing those integration hooks |

## Notes
- Plugin tabs are discovered once from backend manifests and do not hot-reload during runtime; built-in ComfyUI and OrbWeaver tabs have local fallbacks if plugin discovery fails.
- Unknown plugin settings tabs no longer require native frontend code as long as the backend provides a supported schema.
- The global `Save & Apply` path always syncs the in-memory Comfy workflow into `AgentConfig` before returning the config.
- Scheduled jobs now follow the same top-level save model as the rest of the settings window: creates, edits, and deletions are staged locally and only emitted to `app.rs` when `Save & Apply` is clicked.
