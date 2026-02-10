# settings.rs

## Purpose
Implements the Settings modal window where users configure LLM connection, agent identity, behavior parameters, autonomous heartbeat scheduling, self-reflection, memory/database, image generation (ComfyUI), and the system prompt.

## Components

### `SettingsPanel`
- **Does**: Holds a mutable copy of `AgentConfig` and a visibility flag. Renders an egui window with grouped sections for all configuration fields.
- **Interacts with**: `AgentConfig` from `crate::config`

### `SettingsPanel::new(config)`
- **Does**: Constructs the panel with a cloned config and `show: false`

### `SettingsPanel::render(ctx) -> Option<AgentConfig>`
- **Does**: Draws the settings window. Returns `Some(config)` when the user clicks "Save & Apply", otherwise `None`. Sections include:
  - **Skill Connections**: Graphchan API URL
  - **LLM Configuration**: API URL, model name, optional API key
  - **Agent Identity**: Username
  - **Behavior**: Poll interval, max posts/hour, response strategy (selective/all/mentions)
  - **Autonomous Heartbeat**: Enable toggle, interval (minutes), checklist file path, optional memory-evolution scheduling controls
  - **Self-Reflection**: Enable toggle, interval, guiding principles (multiline)
  - **Memory & Database**: Database path, max important posts
  - **Image Generation**: Enable toggle, ComfyUI URL, workflow type (sd/sdxl/flux), model name
  - **System Prompt**: Free-form multiline text
- **Interacts with**: `AgentConfig` fields directly; `app.rs` reads the return value to persist and hot-reload

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `app.rs` | `config` field is `pub` for cross-panel sync | Making it private breaks `CharacterPanel` save flow |
| `app.rs` | `render()` returns `Option<AgentConfig>` | Changing return type breaks save logic |
| `AgentConfig` | Fields: `graphchan_api_url`, `llm_api_url`, `llm_model`, `llm_api_key`, `username`, `poll_interval_secs`, `max_posts_per_hour`, `respond_to.response_type`, `enable_heartbeat`, `heartbeat_interval_mins`, `heartbeat_checklist_path`, `enable_memory_evolution`, `memory_evolution_interval_hours`, `memory_eval_trace_set_path`, `enable_self_reflection`, `reflection_interval_hours`, `guiding_principles`, `database_path`, `max_important_posts`, `enable_image_generation`, `comfyui.api_url`, `comfyui.workflow_type`, `comfyui.model_name`, `system_prompt` | Renaming any field breaks this panel |

## Notes
- The config is edited in-place on `self.config`; only returned on explicit save.
- Guiding principles are stored as `Vec<String>` but edited as newline-separated text.
