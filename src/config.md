# config.rs

## Purpose
Defines all configuration for the Ponderer agent, including LLM connection, identity, polling behavior, autonomous heartbeat scheduling, explicit per-loop tool capability profiles, image generation (ComfyUI), screen-capture privacy gating, character card fields, and self-reflection settings. Supports loading from TOML files with environment variable fallback.

## Components

### `AgentConfig`
- **Does**: Top-level config struct holding all runtime settings (LLM, Graphchan, identity, heartbeat, reflection, image gen, screen-capture gate, character, avatars)
- **Interacts with**: `main.rs` (loaded at startup), `agent::Agent` (drives behavior), `ui::app::AgentApp` (display/edit)

### `AgentConfig::load()`
- **Does**: Loads config from `ponderer_config.toml` next to the executable, falls back to `agent_config.toml`, then environment variables
- **Rationale**: Supports portable deployment (config travels with binary) and backward compatibility with legacy config filenames

### `AgentConfig::save()`
- **Does**: Serializes config to `ponderer_config.toml` via `toml::to_string_pretty`
- **Interacts with**: UI settings panel for persisting changes

### `AgentConfig::from_env()`
- **Does**: Populates config from environment variables (`GRAPHCHAN_API_URL`, `LLM_API_URL`, `LLM_MODEL`, `LLM_API_KEY`, `AGENT_CHECK_INTERVAL`, `AGENT_ENABLE_HEARTBEAT`, `AGENT_HEARTBEAT_INTERVAL_MINS`, `AGENT_HEARTBEAT_CHECKLIST_PATH`, `AGENT_ENABLE_MEMORY_EVOLUTION`, `AGENT_MEMORY_EVOLUTION_INTERVAL_HOURS`, `AGENT_MEMORY_TRACE_SET_PATH`, `AGENT_ENABLE_SCREEN_CAPTURE`, `AGENT_NAME`)
- **Rationale**: Legacy support for env-var-only configuration

### `ComfyUIConfig`
- **Does**: Holds ComfyUI connection and generation parameters (model, dimensions, sampler, scheduler, CFG, steps)
- **Interacts with**: `comfy_client.rs`, `agent::image_gen`

### `RespondTo`
- **Does**: Controls response behavior (`response_type`: "all" or "selective") with optional separate `decision_model`
- **Interacts with**: `agent::reasoning` for deciding whether to reply

### `CapabilityProfileConfig` / `CapabilityProfileOverride`
- **Does**: Declares optional per-loop tool policy overrides (`private_chat`, `skill_events`, `heartbeat`) for allowlist/denylist replacement
- **Interacts with**: `agent::capability_profiles` policy resolver used by loop-level `ToolContext` construction

## Contracts

| Dependent | Expects | Breaking changes |
|-----------|---------|------------------|
| `main.rs` | `AgentConfig::load()` returns valid config | Changing `load()` return type |
| `agent::Agent` | Fields: `llm_api_url`, `llm_model`, `llm_api_key`, `system_prompt`, `poll_interval_secs`, `enable_heartbeat`, `heartbeat_interval_mins`, `heartbeat_checklist_path`, `enable_memory_evolution`, `memory_evolution_interval_hours`, `memory_eval_trace_set_path`, `capability_profiles`, `enable_self_reflection`, `enable_image_generation`, `enable_screen_capture_in_loop`, `guiding_principles` | Renaming/removing any of these fields |
| `tools/vision.rs` | `enable_screen_capture_in_loop` must be present and default false | Removing/renaming this privacy gate field |
| `comfy_client.rs` | `comfyui.api_url` is a valid HTTP URL | Changing `ComfyUIConfig` structure |
| TOML file | Serde field names and aliases (`agent_name` -> `username`, `check_interval_seconds` -> `poll_interval_secs`) | Removing serde aliases breaks existing config files |

## Notes
- Config path resolution uses `current_exe().parent()`, so the config file must sit next to the binary.
- Default LLM is `llama3.2` at `localhost:11434` (Ollama).
- Heartbeat defaults: disabled, 30-minute interval, checklist path `HEARTBEAT.md`.
- Memory evolution defaults: disabled, 24-hour interval, built-in replay trace set.
- Capability profile overrides default to empty, so loop policies fall back to code-defined defaults.
- Screen capture in loop defaults to disabled and must be explicitly enabled in settings.
- Default Graphchan URL derives from `GRAPHCHAN_PORT` env var if set.
