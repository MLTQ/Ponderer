use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RespondTo {
    #[serde(rename = "type")]
    pub response_type: String,
    #[serde(default)]
    pub decision_model: Option<String>,
}

impl Default for RespondTo {
    fn default() -> Self {
        Self {
            response_type: "selective".to_string(),
            decision_model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComfyUIConfig {
    #[serde(default = "default_comfyui_url")]
    pub api_url: String,
    #[serde(default = "default_workflow_type")]
    pub workflow_type: String,
    #[serde(default = "default_model_name")]
    pub model_name: String,
    #[serde(default)]
    pub vae_name: Option<String>,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_steps")]
    pub steps: u32,
    #[serde(default = "default_cfg_scale")]
    pub cfg_scale: f32,
    #[serde(default = "default_sampler")]
    pub sampler: String,
    #[serde(default = "default_scheduler")]
    pub scheduler: String,
    #[serde(default)]
    pub negative_prompt: String,
}

fn default_comfyui_url() -> String {
    "http://127.0.0.1:8188".to_string()
}

fn default_workflow_type() -> String {
    "sd".to_string()
}

fn default_model_name() -> String {
    "v1-5-pruned-emaonly.safetensors".to_string()
}

fn default_width() -> u32 {
    512
}

fn default_height() -> u32 {
    512
}

fn default_steps() -> u32 {
    20
}

fn default_cfg_scale() -> f32 {
    7.0
}

fn default_sampler() -> String {
    "euler".to_string()
}

fn default_scheduler() -> String {
    "normal".to_string()
}

impl Default for ComfyUIConfig {
    fn default() -> Self {
        Self {
            api_url: default_comfyui_url(),
            workflow_type: default_workflow_type(),
            model_name: default_model_name(),
            vae_name: None,
            width: default_width(),
            height: default_height(),
            steps: default_steps(),
            cfg_scale: default_cfg_scale(),
            sampler: default_sampler(),
            scheduler: default_scheduler(),
            negative_prompt: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilityProfileOverride {
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disallowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilityProfileConfig {
    #[serde(default)]
    pub private_chat: CapabilityProfileOverride,
    #[serde(default)]
    pub skill_events: CapabilityProfileOverride,
    #[serde(default)]
    pub heartbeat: CapabilityProfileOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    // Graphchan connection (for graphchan skill)
    #[serde(default = "default_graphchan_url")]
    pub graphchan_api_url: String,

    // LLM configuration (OpenAI-compatible: Ollama, LM Studio, vLLM, OpenAI, etc.)
    #[serde(default = "default_llm_url")]
    pub llm_api_url: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
    #[serde(default)]
    pub llm_api_key: Option<String>,

    // Agent Identity
    #[serde(default = "default_username", alias = "agent_name")]
    pub username: String,

    // System prompt
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,

    // Polling and Response
    #[serde(default = "default_poll_interval", alias = "check_interval_seconds")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub enable_heartbeat: bool,
    #[serde(default = "default_heartbeat_interval_mins")]
    pub heartbeat_interval_mins: u64,
    #[serde(default = "default_heartbeat_checklist_path")]
    pub heartbeat_checklist_path: String,
    #[serde(default)]
    pub enable_memory_evolution: bool,
    #[serde(default = "default_memory_evolution_interval_hours")]
    pub memory_evolution_interval_hours: u64,
    #[serde(default)]
    pub memory_eval_trace_set_path: Option<String>,

    #[serde(default)]
    pub respond_to: RespondTo,

    #[serde(default)]
    pub capability_profiles: CapabilityProfileConfig,

    // Self-reflection and evolution
    #[serde(default)]
    pub enable_self_reflection: bool,
    #[serde(default = "default_reflection_interval")]
    pub reflection_interval_hours: u64,
    #[serde(default)]
    pub reflection_model: Option<String>,

    // Guiding principles
    #[serde(default)]
    pub guiding_principles: Vec<String>,

    // Memory and database
    #[serde(default = "default_database_path")]
    pub database_path: String,
    #[serde(default = "default_max_important_posts")]
    pub max_important_posts: u32,

    // Image generation
    #[serde(default)]
    pub enable_image_generation: bool,
    #[serde(default)]
    pub enable_screen_capture_in_loop: bool,
    #[serde(default)]
    pub comfyui: ComfyUIConfig,

    // Workflow settings
    #[serde(default)]
    pub workflow_path: Option<String>,
    #[serde(default)]
    pub workflow_settings: Option<String>, // JSON string of workflow settings

    // Character Card (optional)
    #[serde(default)]
    pub character_name: String,
    #[serde(default)]
    pub character_description: String,
    #[serde(default)]
    pub character_personality: String,
    #[serde(default)]
    pub character_scenario: String,
    #[serde(default)]
    pub character_example_dialogue: String,
    #[serde(default)]
    pub character_avatar_path: Option<String>,

    // Animated avatars for UI (local display only, not transmitted)
    #[serde(default)]
    pub avatar_idle: Option<String>, // Path to idle avatar (PNG/JPG/GIF)
    #[serde(default)]
    pub avatar_thinking: Option<String>, // Path to thinking avatar
    #[serde(default)]
    pub avatar_active: Option<String>, // Path to active/working avatar

    // Legacy fields for backward compatibility
    #[serde(default)]
    pub max_posts_per_hour: u32,
}

fn default_graphchan_url() -> String {
    env::var("GRAPHCHAN_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .map(|port| format!("http://localhost:{}", port))
        .unwrap_or_else(|| "http://localhost:8080".to_string())
}

fn default_llm_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_llm_model() -> String {
    "llama3.2".to_string()
}

fn default_username() -> String {
    "Ponderer".to_string()
}

fn default_system_prompt() -> String {
    "You are a thoughtful AI companion. \
     You engage in meaningful conversations, help with tasks, and grow through your interactions. \
     Only respond when you have something valuable to contribute."
        .to_string()
}

fn default_poll_interval() -> u64 {
    60
}

fn default_reflection_interval() -> u64 {
    24
}

fn default_heartbeat_interval_mins() -> u64 {
    30
}

fn default_heartbeat_checklist_path() -> String {
    "HEARTBEAT.md".to_string()
}

fn default_memory_evolution_interval_hours() -> u64 {
    24
}

fn default_database_path() -> String {
    "ponderer_memory.db".to_string()
}

fn default_max_important_posts() -> u32 {
    100
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            graphchan_api_url: default_graphchan_url(),
            llm_api_url: default_llm_url(),
            llm_model: default_llm_model(),
            llm_api_key: None,
            username: default_username(),
            system_prompt: default_system_prompt(),
            poll_interval_secs: default_poll_interval(),
            enable_heartbeat: false,
            heartbeat_interval_mins: default_heartbeat_interval_mins(),
            heartbeat_checklist_path: default_heartbeat_checklist_path(),
            enable_memory_evolution: false,
            memory_evolution_interval_hours: default_memory_evolution_interval_hours(),
            memory_eval_trace_set_path: None,
            respond_to: RespondTo::default(),
            capability_profiles: CapabilityProfileConfig::default(),
            enable_self_reflection: false,
            reflection_interval_hours: default_reflection_interval(),
            reflection_model: None,
            guiding_principles: vec![
                "helpful".to_string(),
                "curious".to_string(),
                "thoughtful".to_string(),
            ],
            database_path: default_database_path(),
            max_important_posts: default_max_important_posts(),
            enable_image_generation: false,
            enable_screen_capture_in_loop: false,
            comfyui: ComfyUIConfig::default(),
            workflow_path: None,
            workflow_settings: None,
            character_name: String::new(),
            character_description: String::new(),
            character_personality: String::new(),
            character_scenario: String::new(),
            character_example_dialogue: String::new(),
            character_avatar_path: None,
            avatar_idle: None,
            avatar_thinking: None,
            avatar_active: None,
            max_posts_per_hour: 10,
        }
    }
}

impl AgentConfig {
    /// Get the directory containing the executable
    fn get_base_dir() -> PathBuf {
        match std::env::current_exe() {
            Ok(exe_path) => exe_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
            Err(_) => PathBuf::from("."),
        }
    }

    /// Get the path to the config file (relative to executable)
    pub fn config_path() -> PathBuf {
        Self::get_base_dir().join("ponderer_config.toml")
    }

    /// Load config from ponderer_config.toml (next to executable), falling back to agent_config.toml
    pub fn load() -> Self {
        let path = Self::config_path();

        // Try ponderer_config.toml first
        if let Ok(contents) = fs::read_to_string(&path) {
            match toml::from_str::<AgentConfig>(&contents) {
                Ok(config) => {
                    tracing::info!("Loaded config from {:?}", path);
                    return config;
                }
                Err(e) => {
                    tracing::error!("Failed to parse {:?}: {}", path, e);
                }
            }
        }

        // Fall back to agent_config.toml (backward compatibility)
        let legacy_path = Self::get_base_dir().join("agent_config.toml");
        if let Ok(contents) = fs::read_to_string(&legacy_path) {
            match toml::from_str::<AgentConfig>(&contents) {
                Ok(config) => {
                    tracing::info!("Loaded config from legacy {:?}", legacy_path);
                    return config;
                }
                Err(e) => {
                    tracing::error!("Failed to parse {:?}: {}", legacy_path, e);
                }
            }
        }

        tracing::warn!("No config file found, using defaults + env vars");
        Self::from_env()
    }

    /// Save config to file (next to executable)
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        let toml_string = toml::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&path, toml_string)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        tracing::info!("Saved config to {:?}", path);
        Ok(())
    }

    /// Load from environment variables (legacy support)
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(url) = env::var("GRAPHCHAN_API_URL") {
            config.graphchan_api_url = url;
        }

        if let Ok(url) = env::var("LLM_API_URL") {
            config.llm_api_url = url;
        }

        if let Ok(model) = env::var("LLM_MODEL") {
            config.llm_model = model;
        }

        if let Ok(key) = env::var("LLM_API_KEY") {
            config.llm_api_key = Some(key);
        }

        if let Ok(interval) = env::var("AGENT_CHECK_INTERVAL") {
            if let Ok(seconds) = interval.parse() {
                config.poll_interval_secs = seconds;
            }
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_HEARTBEAT") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_heartbeat = enabled;
        }

        if let Ok(interval) = env::var("AGENT_HEARTBEAT_INTERVAL_MINS") {
            if let Ok(minutes) = interval.parse() {
                config.heartbeat_interval_mins = minutes;
            }
        }

        if let Ok(path) = env::var("AGENT_HEARTBEAT_CHECKLIST_PATH") {
            if !path.trim().is_empty() {
                config.heartbeat_checklist_path = path;
            }
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_MEMORY_EVOLUTION") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_memory_evolution = enabled;
        }

        if let Ok(interval) = env::var("AGENT_MEMORY_EVOLUTION_INTERVAL_HOURS") {
            if let Ok(hours) = interval.parse() {
                config.memory_evolution_interval_hours = hours;
            }
        }

        if let Ok(path) = env::var("AGENT_MEMORY_TRACE_SET_PATH") {
            if !path.trim().is_empty() {
                config.memory_eval_trace_set_path = Some(path);
            }
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_SCREEN_CAPTURE") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_screen_capture_in_loop = enabled;
        }

        if let Ok(name) = env::var("AGENT_NAME") {
            config.username = name;
        }

        config
    }
}
