# settings.rs

Visible headings are concise (Settings, Appearance); base-color explanations are
attached to the control tooltip. Live preview, saved/revert behavior, provider
isolation and warning colors are unchanged.

Behavior includes an explicit Direct/Agentic conversation-mode dropdown. Direct
uses one compact chat pass (still with optional tools); Agentic retains task
continuations and richer OODA context. It is an ordinary saved/revertible draft,
not silently changed by local-provider selection, and does not disable the
ambient loop. Unlimited tool iterations still have an independent no-progress
guard.

## Purpose
Implements the embedded Settings workspace for the native desktop UI. It keeps core agent settings in fixed tabs and appends schema-driven settings tabs from discovered plugin manifests.

## Components

### `SettingsPanel`
- **Does**: Holds the editable `AgentConfig`, visibility state, selected tab, discovered plugin manifests, plus scheduled-job draft/edit/delete state that is flushed into the action queue only when the shared save button is used.
- **Interacts with**: `AgentConfig`, `api::{PluginManifest,ScheduledJob}`, `plugin_settings_form.rs`, and the `app.rs` schedule action dispatcher.

### `SettingsPanel::set_plugin_manifests`
- **Does**: Stores startup plugin discovery data used to decide which plugin tabs to show.
- **Interacts with**: `ui/app.rs` startup backend plugin fetch.

### `SettingsPanel::sync_from_config`
- **Does**: Replaces local config state from a saved backend config.
- **Interacts with**: `ui/app.rs` after config persistence.

### `SettingsPanel::open_tab`
- **Does**: Selects a valid core or discovered plugin tab. Workspace navigation is owned by `workbench.rs`.

### Scheduled-job state methods (`set_scheduled_jobs`, `set_scheduled_jobs_error`, `take_scheduled_job_actions`)
- **Does**: Synchronizes backend schedule snapshots/errors into the UI and emits queued save-time CRUD actions back to `app.rs`.
- **Interacts with**: `api.rs` scheduled-job endpoints (indirectly through `app.rs`)

### `queue_dirty_scheduled_job_updates`
- **Does**: Collects all staged schedule creates, edits, and deletions, validates them, and enqueues the corresponding `Create` / `Update` / `Delete` actions so the global `Save & Apply` button is the single commit point for the schedules tab.
- **Interacts with**: `render`, scheduled-job editor/draft state, and `app.rs` schedule action dispatcher.

### `SettingsPanel::render_contents(ui, save_allowed, model_controls) -> Option<AgentConfig>`
- **Does**: Draws the embedded workspace and returns `Some(config)` on `Save & apply`. Validated schedule changes queue at the same save point. Revert restores the saved configuration and clears schedule drafts. Failed config saves discard queued schedule mutations without losing editors.
- **Interacts with**: `ui/app.rs` for persistence through the backend API.
- **Model controls**: Models delegates model connection editing to the shared
  Affect Lab controller. An API/local-GGUF dropdown switches editors; loading and
  provider selection are explicit session actions. API keys are masked. The
  app merges only provider fields after a switch, preserving other settings drafts.

### Core tab renderers
- **Does**: Render Models, Appearance, Connections, Autonomy & contact, Data & memory, Turn budgets, System prompt, and Schedules. Identity/principles/boundaries live in Identity. Loose arming requires the main Permissions confirmation; this tab cannot bypass it.
- **Interacts with**: top-level `AgentConfig` fields.
- **Notes**: Behavior tab focuses on autonomous loop limits and loop-heat controls. It explicitly explains that disabling configurable chat limits leaves host emergency ceilings in place.

### `render_schedules_tab`
- **Does**: Shows all schedules, lets operators stage enabled/name/prompt/interval changes, stage new schedules, stage deletions, and manually refresh backend state. The tab no longer applies row-local saves; it relies on the shared `Save & Apply` button.
- **Interacts with**: local scheduled-job editor/draft state and the save-time `ScheduledJobAction` queue consumed by `app.rs`.

### Plugin tab renderer
- **Does**: Renders every plugin-specific tab from its canonical manifest settings schema through the generic form renderer.
- **Interacts with**: `plugin_settings_form.rs` and manifests returned by the backend.

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` / `workbench.rs` | Shared mutable config draft; `render_contents` returns `Option<AgentConfig>`; provider-only sync preserves drafts; `open_tab()` selects valid tab IDs | Changing these signatures or overwriting unrelated drafts |
| `api.rs` | `PluginManifest.settings_tab` contains `id`, `title`, `order` when a plugin wants a settings tab | Renaming/removing settings-tab fields |
| `api.rs` / plugin manifests | Generic plugin tabs require `settings_schema` to be present | Removing schema handling or changing field semantics |
| Plugin packages | Settings UI remains entirely manifest/schema driven | Adding a new hard-coded integration tab |

## Notes
- Appearance has a native sRGB picker, hex readout, presets and dark/light toggle. `theme.rs` derives surfaces, text and instrument colors; changes preview before persistence.
- `saved_config`, `has_unsaved_changes`, `sync_from_config`, and `revert_drafts` distinguish live preview from successful backend persistence. Provider-only sync updates the provider baseline without erasing unsaved identity/appearance fields.
- Plugin tabs come only from backend manifests; there are no integration-specific fallback tabs.
- Unknown plugin settings tabs no longer require native frontend code as long as the backend provides a supported schema.
- The global `Save & Apply` path returns schema-updated `AgentConfig` without integration-specific synchronization hooks.
- Scheduled jobs now follow the same top-level save model as the rest of the settings window: creates, edits, and deletions are staged locally and only emitted to `app.rs` when `Save & Apply` is clicked.
