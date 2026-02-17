# config.rs

## Purpose
Defines all configuration for the Ponderer agent, including LLM connection, identity, polling behavior, agentic loop iteration controls, private-chat autonomous turn budgets (foreground + background) with optional unbounded mode, three-loop living-loop toggles (ambient/journal/concerns/dream), autonomous heartbeat scheduling, explicit per-loop tool capability profiles, image generation (ComfyUI), screen/camera capture privacy gating, character card fields, and self-reflection settings. Supports loading from TOML files with environment variable fallback.

## Components

### `AgentConfig`
- **Does**: Top-level config struct holding all runtime settings (LLM, Graphchan, identity, ambient/journal/concerns/dream controls, heartbeat, reflection, image gen, screen-capture gate, character, avatars)
- **Interacts with**: `main.rs` (loaded at startup), `agent::Agent` (drives behavior), `ui::app::AgentApp` (display/edit)

### `AgentConfig::load()`
- **Does**: Loads config by scanning both executable-directory and current-working-directory TOML files, preferring the most recently modified among `ponderer_config.toml` and `agent_config.toml`, then falls back to environment variables
- **Rationale**: Supports portable binary deployment while still tolerating launch-context differences and legacy config filenames

### `AgentConfig::save()`
- **Does**: Serializes config to `ponderer_config.toml` via `toml::to_string_pretty`
- **Interacts with**: UI settings panel for persisting changes

### `AgentConfig::from_env()`
- **Does**: Populates config from environment variables (`GRAPHCHAN_API_URL`, `LLM_API_URL`, `LLM_MODEL`, `LLM_API_KEY`, `AGENT_CHECK_INTERVAL`, `AGENT_MAX_TOOL_ITERATIONS`, `AGENT_DISABLE_TOOL_ITERATION_LIMIT`, `AGENT_MAX_CHAT_AUTONOMOUS_TURNS`, `AGENT_MAX_BACKGROUND_SUBTASK_TURNS`, `AGENT_DISABLE_CHAT_TURN_LIMIT`, `AGENT_DISABLE_BACKGROUND_SUBTASK_TURN_LIMIT`, `AGENT_ENABLE_AMBIENT_LOOP`, `AGENT_AMBIENT_MIN_INTERVAL_SECS`, `AGENT_ENABLE_JOURNAL`, `AGENT_JOURNAL_MIN_INTERVAL_SECS`, `AGENT_ENABLE_CONCERNS`, `AGENT_ENABLE_DREAM_CYCLE`, `AGENT_DREAM_MIN_INTERVAL_SECS`, `AGENT_ENABLE_HEARTBEAT`, `AGENT_HEARTBEAT_INTERVAL_MINS`, `AGENT_HEARTBEAT_CHECKLIST_PATH`, `AGENT_ENABLE_MEMORY_EVOLUTION`, `AGENT_MEMORY_EVOLUTION_INTERVAL_HOURS`, `AGENT_MEMORY_TRACE_SET_PATH`, `AGENT_ENABLE_SCREEN_CAPTURE`, `AGENT_ENABLE_CAMERA_CAPTURE`, `AGENT_NAME`)
- **Rationale**: Legacy support for env-var-only configuration

### `ComfyUIConfig`
- **Does**: Holds ComfyUI connection and generation parameters (model, dimensions, sampler, scheduler, CFG, steps)
- **Interacts with**: `comfy_client.rs`, `agent::image_gen`

### `RespondTo`
- **Does**: Controls response behavior (`response_type`: "all" or "selective") with optional separate `decision_model`
- **Interacts with**: `agent::reasoning` for deciding whether to reply

### `CapabilityProfileConfig` / `CapabilityProfileOverride`
- **Does**: Declares optional per-loop tool policy overrides (`private_chat`, `skill_events`, `heartbeat`, `ambient`, `dream`) for allowlist/denylist replacement
- **Interacts with**: `agent::capability_profiles` policy resolver used by loop-level `ToolContext` construction

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `main.rs` | `AgentConfig::load()` returns valid config | Changing `load()` return type |
| `agent::Agent` | Fields: `llm_api_url`, `llm_model`, `llm_api_key`, `system_prompt`, `poll_interval_secs`, `max_tool_iterations`, `disable_tool_iteration_limit`, `max_chat_autonomous_turns`, `max_background_subtask_turns`, `disable_chat_turn_limit`, `disable_background_subtask_turn_limit`, `enable_ambient_loop`, `ambient_min_interval_secs`, `enable_journal`, `journal_min_interval_secs`, `enable_concerns`, `enable_dream_cycle`, `dream_min_interval_secs`, `enable_heartbeat`, `heartbeat_interval_mins`, `heartbeat_checklist_path`, `enable_memory_evolution`, `memory_evolution_interval_hours`, `memory_eval_trace_set_path`, `capability_profiles`, `enable_self_reflection`, `enable_image_generation`, `enable_screen_capture_in_loop`, `enable_camera_capture_tool`, `guiding_principles` | Renaming/removing any of these fields |
| `tools/vision.rs` | `enable_screen_capture_in_loop` and `enable_camera_capture_tool` must be present and default false | Removing/renaming these privacy gate fields |
| `comfy_client.rs` | `comfyui.api_url` is a valid HTTP URL | Changing `ComfyUIConfig` structure |
| TOML file | Serde field names and aliases (`agent_name` -> `username`, `check_interval_seconds` -> `poll_interval_secs`) | Removing serde aliases breaks existing config files |

## Notes
- Config save path targets the executable directory (`current_exe().parent()`); load discovery scans executable dir, its parent (for Cargo `target/*/deps` runs), and the working directory, then picks the newest valid file.
- When both `ponderer_config.toml` and `agent_config.toml` exist, the newest file wins to avoid stale-file precedence surprises.
- `database_path` is normalized to an executable-directory absolute runtime path on load, and converted back to a portable relative path when saving TOML.
- Default LLM is `llama3.2` at `localhost:11434` (Ollama).
- Living-loop defaults keep phase-5 architecture opt-in (`enable_ambient_loop=false`, `enable_dream_cycle=false`), while `enable_journal`/`enable_concerns` default true for continuity.
- Heartbeat defaults: disabled, 30-minute interval, checklist path `HEARTBEAT.md`.
- Agentic loop defaults: max 10 tool-calling iterations per turn, with optional config to disable the limit entirely.
- Private-chat turn defaults: model-decided continuation (`disable_chat_turn_limit=true`, `disable_background_subtask_turn_limit=true`). Optional safety caps remain configurable at 4 foreground turns and 8 background turns when re-enabled.
- Memory evolution defaults: disabled, 24-hour interval, built-in replay trace set.
- Capability profile overrides default to empty, so loop policies fall back to code-defined defaults.
- Screen and camera capture tools default to disabled and must be explicitly enabled in settings.
- Default Graphchan URL derives from `GRAPHCHAN_PORT` env var if set.
