# settings.rs

## Purpose
Implements the Settings modal window where users configure LLM connection, agent identity, behavior parameters, living-loop controls (ambient/journal/concerns/dream), autonomous heartbeat scheduling, self-reflection, memory/database, image generation (ComfyUI), and the system prompt.

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
  - **Behavior**: Poll interval, tool-loop iteration controls (max iterations + optional unbounded mode), private-chat turn budgets (foreground + background) with optional unbounded mode, max posts/hour, response strategy (selective/all/mentions), opt-in screen capture and camera snapshot toggles for the agentic tool loop
  - **Living Loop**: Ambient-loop toggle, ambient minimum tick, journal toggle + interval, concerns toggle, dream-cycle toggle + interval
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
| `AgentConfig` | Fields: `graphchan_api_url`, `llm_api_url`, `llm_model`, `llm_api_key`, `username`, `poll_interval_secs`, `max_tool_iterations`, `disable_tool_iteration_limit`, `max_chat_autonomous_turns`, `max_background_subtask_turns`, `disable_chat_turn_limit`, `disable_background_subtask_turn_limit`, `max_posts_per_hour`, `respond_to.response_type`, `enable_screen_capture_in_loop`, `enable_camera_capture_tool`, `enable_ambient_loop`, `ambient_min_interval_secs`, `enable_journal`, `journal_min_interval_secs`, `enable_concerns`, `enable_dream_cycle`, `dream_min_interval_secs`, `enable_heartbeat`, `heartbeat_interval_mins`, `heartbeat_checklist_path`, `enable_memory_evolution`, `memory_evolution_interval_hours`, `memory_eval_trace_set_path`, `enable_self_reflection`, `reflection_interval_hours`, `guiding_principles`, `database_path`, `max_important_posts`, `enable_image_generation`, `comfyui.api_url`, `comfyui.workflow_type`, `comfyui.model_name`, `system_prompt` | Renaming any field breaks this panel |

## Notes
- The config is edited in-place on `self.config`; only returned on explicit save.
- Guiding principles are stored as `Vec<String>` but edited as newline-separated text.
- Screen and camera capture are explicit opt-in toggles in settings; both are disabled by default for privacy.
- Foreground/background chat turn caps are optional safety rails; defaults are model-decided continuation with caps disabled.
- Save/apply writes `ponderer_config.toml` in the launch/working directory, matching the app's portable runtime-state location.
