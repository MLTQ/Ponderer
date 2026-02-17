use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    #[serde(default)]
    pub ambient: CapabilityProfileOverride,
    #[serde(default)]
    pub dream: CapabilityProfileOverride,
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
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: u32,
    #[serde(default)]
    pub disable_tool_iteration_limit: bool,
    #[serde(default = "default_max_chat_autonomous_turns")]
    pub max_chat_autonomous_turns: u32,
    #[serde(default = "default_max_background_subtask_turns")]
    pub max_background_subtask_turns: u32,
    #[serde(default)]
    pub disable_chat_turn_limit: bool,
    #[serde(default)]
    pub disable_background_subtask_turn_limit: bool,
    #[serde(default)]
    pub enable_ambient_loop: bool,
    #[serde(default = "default_ambient_min_interval_secs")]
    pub ambient_min_interval_secs: u64,
    #[serde(default)]
    pub enable_journal: bool,
    #[serde(default = "default_journal_min_interval_secs")]
    pub journal_min_interval_secs: u64,
    #[serde(default)]
    pub enable_concerns: bool,
    #[serde(default)]
    pub enable_dream_cycle: bool,
    #[serde(default = "default_dream_min_interval_secs")]
    pub dream_min_interval_secs: u64,
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
    pub enable_camera_capture_tool: bool,
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

fn default_max_tool_iterations() -> u32 {
    10
}

fn default_max_chat_autonomous_turns() -> u32 {
    4
}

fn default_max_background_subtask_turns() -> u32 {
    8
}

fn default_ambient_min_interval_secs() -> u64 {
    30
}

fn default_journal_min_interval_secs() -> u64 {
    300
}

fn default_dream_min_interval_secs() -> u64 {
    3600
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
            max_tool_iterations: default_max_tool_iterations(),
            disable_tool_iteration_limit: false,
            max_chat_autonomous_turns: default_max_chat_autonomous_turns(),
            max_background_subtask_turns: default_max_background_subtask_turns(),
            disable_chat_turn_limit: true,
            disable_background_subtask_turn_limit: true,
            enable_ambient_loop: false,
            ambient_min_interval_secs: default_ambient_min_interval_secs(),
            enable_journal: true,
            journal_min_interval_secs: default_journal_min_interval_secs(),
            enable_concerns: true,
            enable_dream_cycle: false,
            dream_min_interval_secs: default_dream_min_interval_secs(),
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
            enable_camera_capture_tool: false,
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
    /// Get the directory containing the running executable.
    fn get_base_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Get the path to the primary config file (relative to executable directory).
    pub fn config_path() -> PathBuf {
        Self::get_base_dir().join("ponderer_config.toml")
    }

    /// Load config from known config locations.
    pub fn load() -> Self {
        for path in Self::candidate_config_paths() {
            if let Ok(contents) = fs::read_to_string(&path) {
                match toml::from_str::<AgentConfig>(&contents) {
                    Ok(mut config) => {
                        config.normalize_portable_paths();
                        tracing::info!("Loaded config from {:?}", path);
                        return config;
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse {:?}: {}", path, e);
                    }
                }
            }
        }

        tracing::warn!("No config file found, using defaults + env vars");
        let mut config = Self::from_env();
        config.normalize_portable_paths();
        config
    }

    /// Save config to file (in executable directory)
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let persisted = self.portable_persisted_copy();
        let toml_string =
            toml::to_string_pretty(&persisted).context("Failed to serialize config")?;

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

        if let Ok(limit) = env::var("AGENT_MAX_TOOL_ITERATIONS") {
            if let Ok(iterations) = limit.parse() {
                config.max_tool_iterations = iterations;
            }
        }

        if let Ok(disabled) = env::var("AGENT_DISABLE_TOOL_ITERATION_LIMIT") {
            let disabled = disabled.eq_ignore_ascii_case("1")
                || disabled.eq_ignore_ascii_case("true")
                || disabled.eq_ignore_ascii_case("yes");
            config.disable_tool_iteration_limit = disabled;
        }

        if let Ok(limit) = env::var("AGENT_MAX_CHAT_AUTONOMOUS_TURNS") {
            if let Ok(turns) = limit.parse() {
                config.max_chat_autonomous_turns = turns;
            }
        }

        if let Ok(limit) = env::var("AGENT_MAX_BACKGROUND_SUBTASK_TURNS") {
            if let Ok(turns) = limit.parse() {
                config.max_background_subtask_turns = turns;
            }
        }

        if let Ok(disabled) = env::var("AGENT_DISABLE_CHAT_TURN_LIMIT") {
            let disabled = disabled.eq_ignore_ascii_case("1")
                || disabled.eq_ignore_ascii_case("true")
                || disabled.eq_ignore_ascii_case("yes");
            config.disable_chat_turn_limit = disabled;
        }

        if let Ok(disabled) = env::var("AGENT_DISABLE_BACKGROUND_SUBTASK_TURN_LIMIT") {
            let disabled = disabled.eq_ignore_ascii_case("1")
                || disabled.eq_ignore_ascii_case("true")
                || disabled.eq_ignore_ascii_case("yes");
            config.disable_background_subtask_turn_limit = disabled;
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_AMBIENT_LOOP") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_ambient_loop = enabled;
        }

        if let Ok(interval) = env::var("AGENT_AMBIENT_MIN_INTERVAL_SECS") {
            if let Ok(seconds) = interval.parse() {
                config.ambient_min_interval_secs = seconds;
            }
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_JOURNAL") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_journal = enabled;
        }

        if let Ok(interval) = env::var("AGENT_JOURNAL_MIN_INTERVAL_SECS") {
            if let Ok(seconds) = interval.parse() {
                config.journal_min_interval_secs = seconds;
            }
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_CONCERNS") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_concerns = enabled;
        }

        if let Ok(enabled) = env::var("AGENT_ENABLE_DREAM_CYCLE") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_dream_cycle = enabled;
        }

        if let Ok(interval) = env::var("AGENT_DREAM_MIN_INTERVAL_SECS") {
            if let Ok(seconds) = interval.parse() {
                config.dream_min_interval_secs = seconds;
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

        if let Ok(enabled) = env::var("AGENT_ENABLE_CAMERA_CAPTURE") {
            let enabled = enabled.eq_ignore_ascii_case("1")
                || enabled.eq_ignore_ascii_case("true")
                || enabled.eq_ignore_ascii_case("yes");
            config.enable_camera_capture_tool = enabled;
        }

        if let Ok(name) = env::var("AGENT_NAME") {
            config.username = name;
        }

        config
    }

    fn normalize_portable_paths(&mut self) {
        let portable_name = normalize_portable_path(&self.database_path, default_database_path());
        self.database_path = Self::get_base_dir()
            .join(portable_name)
            .to_string_lossy()
            .to_string();
    }

    fn candidate_config_paths() -> Vec<PathBuf> {
        let roots = Self::config_search_roots();
        let mut declared: Vec<PathBuf> = Vec::new();
        for root in roots {
            declared.push(root.join("ponderer_config.toml"));
            declared.push(root.join("agent_config.toml"));
        }

        let mut existing: Vec<(PathBuf, Option<SystemTime>, usize)> = declared
            .iter()
            .enumerate()
            .filter_map(|(idx, path)| {
                fs::metadata(path)
                    .ok()
                    .map(|meta| (path.clone(), meta.modified().ok(), idx))
            })
            .collect();

        if existing.is_empty() {
            return declared;
        }

        existing.sort_by(|a, b| match (a.1, b.1) {
            (Some(a_time), Some(b_time)) => b_time.cmp(&a_time).then_with(|| a.2.cmp(&b.2)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.2.cmp(&b.2),
        });

        existing.into_iter().map(|(path, _, _)| path).collect()
    }

    fn config_search_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let exe_dir = Self::get_base_dir();
        roots.push(exe_dir.clone());
        if let Some(parent) = exe_dir.parent().map(|p| p.to_path_buf()) {
            if !roots.iter().any(|existing| existing == &parent) {
                roots.push(parent);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            if !roots.iter().any(|existing| existing == &cwd) {
                roots.push(cwd);
            }
        }
        roots
    }

    fn portable_persisted_copy(&self) -> Self {
        let mut clone = self.clone();
        let base_dir = Self::get_base_dir();
        let path = PathBuf::from(&clone.database_path);
        if path.is_absolute() {
            if let Ok(stripped) = path.strip_prefix(&base_dir) {
                clone.database_path = stripped.to_string_lossy().to_string();
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                clone.database_path = name.to_string();
            }
        }
        clone
    }
}

fn normalize_portable_path(raw_path: &str, default_name: String) -> String {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return default_name;
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            tracing::warn!(
                "Config path '{}' is absolute; using portable local filename '{}'",
                raw_path,
                name
            );
            return name.to_string();
        }
        return default_name;
    }

    trimmed.to_string()
}
