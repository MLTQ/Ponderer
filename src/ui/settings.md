# settings.rs

## Purpose
Implements the tabbed Settings window for the desktop UI. It keeps core agent settings in fixed tabs, preserves native tabs for built-in integrations like ComfyUI and OrbWeaver, and appends schema-driven plugin tabs for discovered workflow bundles when the backend advertises them.

## Components

### `SettingsPanel`
- **Does**: Holds the editable `AgentConfig`, visibility state, selected tab, discovered plugin manifests, embedded skill-specific subpanels, plus scheduled-job editor state/action queue for manual schedule CRUD.
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
- **Does**: Synchronizes backend schedule snapshots/errors into the UI and emits queued manual CRUD actions back to `app.rs`.
- **Interacts with**: `api.rs` scheduled-job endpoints (indirectly through `app.rs`)

### `SettingsPanel::render(ctx) -> Option<AgentConfig>`
- **Does**: Draws the tabbed settings window and returns `Some(config)` when the user clicks `Save & Apply`.
- **Interacts with**: `ui/app.rs` for persistence through the backend API.

### Core tab renderers
- **Does**: Render grouped core settings tabs: `General`, `Behavior`, `Living Loop`, `Memory`, `System`, and `Schedules`.
- **Interacts with**: top-level `AgentConfig` fields.
- **Notes**: Behavior tab includes private-chat mode selector (`agentic` multi-turn vs `direct` single-turn) in addition to autonomous loop limits.

### `render_schedules_tab`
- **Does**: Shows all schedules, lets operators edit enabled/name/prompt/interval, and supports add/delete/manual refresh.
- **Interacts with**: local scheduled-job editor map and `ScheduledJobAction` queue consumed by `app.rs`.

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
- Scheduled jobs are managed immediately (live API calls via `app.rs`) and are independent from `Save & Apply`.
