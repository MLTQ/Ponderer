pub mod actions;
pub mod capability_profiles;
pub mod concerns;
pub mod image_gen;
pub mod journal;
pub mod reasoning;
pub mod trajectory;

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use flume::Sender;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

use crate::agent::capability_profiles::{build_tool_context_for_profile, AgentCapabilityProfile};
use crate::config::AgentConfig;
use crate::database::{AgentDatabase, ChatTurnPhase};
use crate::llm_client::{LlmClient, Message as LlmMessage};
use crate::memory::archive::{MemoryEvalRunRecord, MemoryPromotionPolicy, PromotionOutcome};
use crate::memory::eval::{
    default_replay_trace_set, evaluate_trace_set, load_trace_set, EvalBackendKind, MemoryEvalReport,
};
use crate::memory::WorkingMemoryEntry;
use crate::skills::{Skill, SkillContext, SkillEvent};
use crate::tools::agentic::{AgenticConfig, AgenticLoop, ToolCallRecord};
use crate::tools::ToolOutput;
use crate::tools::ToolRegistry;

const HEARTBEAT_LAST_RUN_STATE_KEY: &str = "heartbeat_last_run_at";
const MEMORY_EVOLUTION_LAST_RUN_STATE_KEY: &str = "memory_evolution_last_run_at";
const CHAT_TOOL_BLOCK_START: &str = "[tool_calls]";
const CHAT_TOOL_BLOCK_END: &str = "[/tool_calls]";
const CHAT_THINKING_BLOCK_START: &str = "[thinking]";
const CHAT_THINKING_BLOCK_END: &str = "[/thinking]";
const CHAT_MEDIA_BLOCK_START: &str = "[media]";
const CHAT_MEDIA_BLOCK_END: &str = "[/media]";
const CHAT_TURN_CONTROL_BLOCK_START: &str = "[turn_control]";
const CHAT_TURN_CONTROL_BLOCK_END: &str = "[/turn_control]";
const CHAT_CONTINUE_MARKER_LEGACY: &str = "[CONTINUE]";
const CHAT_MAX_AUTONOMOUS_TURNS: usize = 4;
const CHAT_CONTEXT_RECENT_LIMIT: usize = 18;
const CHAT_COMPACTION_TRIGGER_MESSAGES: usize = 36;
const CHAT_COMPACTION_RESUMMARY_DELTA: usize = 8;
const CHAT_COMPACTION_SOURCE_MAX_MESSAGES: usize = 140;

#[derive(Debug, Clone)]
pub enum AgentVisualState {
    Idle,
    Reading,
    Thinking,
    Writing,
    Happy,
    Confused,
    Paused,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    StateChanged(AgentVisualState),
    Observation(String),
    ReasoningTrace(Vec<String>),
    ToolCallProgress {
        conversation_id: String,
        tool_name: String,
        output_preview: String,
    },
    ChatStreaming {
        conversation_id: String,
        content: String,
        done: bool,
    },
    ActionTaken {
        action: String,
        result: String,
    },
    Error(String),
}

pub struct AgentState {
    pub visual_state: AgentVisualState,
    pub paused: bool,
    pub actions_this_hour: u32,
    pub last_action_time: Option<chrono::DateTime<chrono::Utc>>,
    pub processed_events: HashSet<String>,
}

impl Default for AgentState {
    fn default() -> Self {
        Self {
            visual_state: AgentVisualState::Idle,
            paused: false,
            actions_this_hour: 0,
            last_action_time: None,
            processed_events: HashSet::new(),
        }
    }
}

pub struct Agent {
    skills: Arc<RwLock<Vec<Box<dyn Skill>>>>,
    tool_registry: Arc<ToolRegistry>,
    config: Arc<RwLock<AgentConfig>>,
    state: Arc<RwLock<AgentState>>,
    event_tx: Sender<AgentEvent>,
    reasoning: Arc<RwLock<reasoning::ReasoningEngine>>,
    image_gen: Arc<RwLock<Option<image_gen::ImageGenerator>>>,
    database: Arc<RwLock<Option<AgentDatabase>>>,
    trajectory_engine: Arc<RwLock<Option<trajectory::TrajectoryEngine>>>,
}

impl Agent {
    pub fn new(
        skills: Vec<Box<dyn Skill>>,
        tool_registry: Arc<ToolRegistry>,
        config: AgentConfig,
        event_tx: Sender<AgentEvent>,
    ) -> Self {
        let reasoning = reasoning::ReasoningEngine::new(
            config.llm_api_url.clone(),
            config.llm_model.clone(),
            config.llm_api_key.clone(),
            config.system_prompt.clone(),
        );

        // Initialize image generator if workflow is configured
        let image_gen = if config.enable_image_generation {
            if let Some(ref workflow_json) = config.workflow_settings {
                match serde_json::from_str::<crate::comfy_workflow::ComfyWorkflow>(workflow_json) {
                    Ok(workflow) => {
                        tracing::info!("Image generation enabled with workflow: {}", workflow.name);
                        Some(image_gen::ImageGenerator::new(
                            config.comfyui.api_url.clone(),
                            Some(workflow),
                        ))
                    }
                    Err(e) => {
                        tracing::error!("Failed to load workflow: {}", e);
                        None
                    }
                }
            } else {
                tracing::warn!("Image generation enabled but no workflow configured");
                None
            }
        } else {
            None
        };

        // Initialize database for memory and persona tracking
        let database = match AgentDatabase::new(&config.database_path) {
            Ok(db) => {
                tracing::info!(
                    "Agent memory database initialized: {}",
                    config.database_path
                );
                Some(db)
            }
            Err(e) => {
                tracing::error!("Failed to initialize agent database: {}", e);
                None
            }
        };

        // Initialize trajectory engine for Ludonarrative Assonantic Tracing
        let trajectory_engine = if config.enable_self_reflection {
            let model = config
                .reflection_model
                .clone()
                .unwrap_or_else(|| config.llm_model.clone());
            tracing::info!("Trajectory engine enabled (model: {})", model);
            Some(trajectory::TrajectoryEngine::new(
                config.llm_api_url.clone(),
                model,
                config.llm_api_key.clone(),
            ))
        } else {
            None
        };

        Self {
            skills: Arc::new(RwLock::new(skills)),
            tool_registry,
            config: Arc::new(RwLock::new(config)),
            state: Arc::new(RwLock::new(AgentState::default())),
            event_tx,
            reasoning: Arc::new(RwLock::new(reasoning)),
            image_gen: Arc::new(RwLock::new(image_gen)),
            database: Arc::new(RwLock::new(database)),
            trajectory_engine: Arc::new(RwLock::new(trajectory_engine)),
        }
    }

    /// Reload config and recreate reasoning engine and image generator
    pub async fn reload_config(&self, new_config: AgentConfig) {
        tracing::info!("Reloading agent configuration...");

        // Create new reasoning engine with updated config
        let new_reasoning = reasoning::ReasoningEngine::new(
            new_config.llm_api_url.clone(),
            new_config.llm_model.clone(),
            new_config.llm_api_key.clone(),
            new_config.system_prompt.clone(),
        );

        // Recreate image generator if needed
        let new_image_gen = if new_config.enable_image_generation {
            if let Some(ref workflow_json) = new_config.workflow_settings {
                match serde_json::from_str::<crate::comfy_workflow::ComfyWorkflow>(workflow_json) {
                    Ok(workflow) => {
                        tracing::info!("Image generation enabled with workflow: {}", workflow.name);
                        Some(image_gen::ImageGenerator::new(
                            new_config.comfyui.api_url.clone(),
                            Some(workflow),
                        ))
                    }
                    Err(e) => {
                        tracing::error!("Failed to load workflow: {}", e);
                        None
                    }
                }
            } else {
                tracing::warn!("Image generation enabled but no workflow configured");
                None
            }
        } else {
            None
        };

        // Recreate trajectory engine if self-reflection settings changed
        let new_trajectory = if new_config.enable_self_reflection {
            let model = new_config
                .reflection_model
                .clone()
                .unwrap_or_else(|| new_config.llm_model.clone());
            Some(trajectory::TrajectoryEngine::new(
                new_config.llm_api_url.clone(),
                model,
                new_config.llm_api_key.clone(),
            ))
        } else {
            None
        };

        // Update all components atomically
        *self.config.write().await = new_config;
        *self.reasoning.write().await = new_reasoning;
        *self.image_gen.write().await = new_image_gen;
        *self.trajectory_engine.write().await = new_trajectory;

        self.emit(AgentEvent::Observation(
            "Configuration reloaded".to_string(),
        ))
        .await;
        tracing::info!("Configuration reloaded successfully");
    }

    pub async fn toggle_pause(&self) {
        let mut state = self.state.write().await;
        state.paused = !state.paused;
        let new_state = if state.paused {
            AgentVisualState::Paused
        } else {
            AgentVisualState::Idle
        };
        state.visual_state = new_state.clone();
        drop(state);

        let _ = self.event_tx.send(AgentEvent::StateChanged(new_state));
    }

    async fn emit(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }

    async fn set_state(&self, visual_state: AgentVisualState) {
        let mut state = self.state.write().await;
        state.visual_state = visual_state.clone();
        drop(state);

        self.emit(AgentEvent::StateChanged(visual_state)).await;
    }

    pub async fn run_loop(self: Arc<Self>) -> Result<()> {
        tracing::info!("Agent loop starting...");

        self.emit(AgentEvent::Observation("Agent starting up...".to_string()))
            .await;

        // Capture initial persona snapshot if this is the first run
        self.maybe_capture_initial_persona().await;

        loop {
            // Check if paused
            {
                let state = self.state.read().await;
                if state.paused {
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }

            self.set_state(AgentVisualState::Idle).await;

            // Check if it's time for persona evolution (Ludonarrative Assonantic Tracing)
            self.maybe_evolve_persona().await;

            // Run periodic autonomous heartbeat tasks (if enabled and due)
            self.maybe_run_heartbeat().await;

            // Get poll interval from config
            let poll_interval = {
                let config = self.config.read().await;
                config.poll_interval_secs
            };
            sleep(Duration::from_secs(poll_interval)).await;

            // Check for rate limiting
            {
                let state = self.state.read().await;
                let config = self.config.read().await;
                if state.actions_this_hour >= config.max_posts_per_hour {
                    self.emit(AgentEvent::Observation(format!(
                        "Rate limit reached ({}/{}), waiting...",
                        state.actions_this_hour, config.max_posts_per_hour
                    )))
                    .await;
                    continue;
                }
            }

            // Main agent logic
            if let Err(e) = self.run_cycle().await {
                tracing::error!("Agent cycle error: {}", e);
                self.emit(AgentEvent::Error(e.to_string())).await;
                self.set_state(AgentVisualState::Confused).await;
                sleep(Duration::from_secs(10)).await;
            }
        }
    }

    /// Capture initial persona snapshot if database is empty
    async fn maybe_capture_initial_persona(&self) {
        let db_lock = self.database.read().await;
        if let Some(ref db) = *db_lock {
            match db.count_persona_snapshots() {
                Ok(0) => {
                    drop(db_lock);
                    self.emit(AgentEvent::Observation(
                        "Capturing initial persona snapshot...".to_string(),
                    ))
                    .await;
                    if let Err(e) = self.capture_persona_snapshot("initial").await {
                        tracing::warn!("Failed to capture initial persona: {}", e);
                    }
                }
                Ok(n) => {
                    tracing::info!("Found {} existing persona snapshots", n);
                }
                Err(e) => {
                    tracing::warn!("Failed to count persona snapshots: {}", e);
                }
            }
        }
    }

    /// Check if it's time to evolve persona and run trajectory inference
    async fn maybe_evolve_persona(&self) {
        let config = self.config.read().await;
        if !config.enable_self_reflection {
            return;
        }

        let reflection_interval_hours = config.reflection_interval_hours;
        drop(config);

        // Check last reflection time
        let db_lock = self.database.read().await;
        let should_reflect = if let Some(ref db) = *db_lock {
            match db.get_last_reflection_time() {
                Ok(Some(last_time)) => {
                    let elapsed = Utc::now() - last_time;
                    elapsed > ChronoDuration::hours(reflection_interval_hours as i64)
                }
                Ok(None) => true, // Never reflected before
                Err(e) => {
                    tracing::warn!("Failed to get last reflection time: {}", e);
                    false
                }
            }
        } else {
            false
        };
        drop(db_lock);

        if should_reflect {
            self.emit(AgentEvent::Observation(
                "Beginning persona evolution cycle...".to_string(),
            ))
            .await;
            self.set_state(AgentVisualState::Thinking).await;

            if let Err(e) = self.run_persona_evolution().await {
                tracing::error!("Persona evolution failed: {}", e);
                self.emit(AgentEvent::Error(format!("Persona evolution error: {}", e)))
                    .await;
            }
        }
    }

    /// Run autonomous heartbeat checks on a configurable schedule.
    ///
    /// Heartbeat only executes when both:
    /// 1) heartbeat mode is enabled, and
    /// 2) the configured interval has elapsed since the last run.
    ///
    /// It looks for pending checklist items from HEARTBEAT.md-style markdown
    /// and reminder-like working-memory entries before invoking the tool-calling loop.
    async fn maybe_run_heartbeat(&self) {
        let config_snapshot = { self.config.read().await.clone() };
        let enabled = config_snapshot.enable_heartbeat;
        let heartbeat_interval_mins = config_snapshot.heartbeat_interval_mins.max(1);
        let heartbeat_checklist_path = config_snapshot.heartbeat_checklist_path.clone();
        let llm_api_url = config_snapshot.llm_api_url.clone();
        let llm_model = config_snapshot.llm_model.clone();
        let llm_api_key = config_snapshot.llm_api_key.clone();
        let system_prompt = config_snapshot.system_prompt.clone();
        let username = config_snapshot.username.clone();

        if !enabled {
            return;
        }

        let (should_run, memory_hints) = {
            let db_lock = self.database.read().await;
            let Some(db) = db_lock.as_ref() else {
                tracing::warn!("Heartbeat enabled but database is unavailable");
                return;
            };

            let now = Utc::now();
            let last_run = db
                .get_state(HEARTBEAT_LAST_RUN_STATE_KEY)
                .ok()
                .flatten()
                .and_then(|raw| raw.parse::<chrono::DateTime<Utc>>().ok());

            let is_due = last_run
                .map(|last| now - last >= ChronoDuration::minutes(heartbeat_interval_mins as i64))
                .unwrap_or(true);

            if !is_due {
                (false, Vec::new())
            } else {
                if let Err(e) = db.set_state(HEARTBEAT_LAST_RUN_STATE_KEY, &now.to_rfc3339()) {
                    tracing::warn!("Failed to persist heartbeat timestamp: {}", e);
                }

                let hints = db
                    .get_all_working_memory()
                    .map(|entries| collect_heartbeat_memory_hints(&entries))
                    .unwrap_or_else(|e| {
                        tracing::warn!("Heartbeat failed to load working memory: {}", e);
                        Vec::new()
                    });
                (true, hints)
            }
        };

        if !should_run {
            return;
        }

        // Memory evolution is scheduled off heartbeat ticks but can use its own interval.
        self.maybe_run_memory_evolution().await;

        let checklist_items = load_pending_checklist_items(&heartbeat_checklist_path)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Heartbeat checklist read failed ({}): {}",
                    heartbeat_checklist_path,
                    e
                );
                Vec::new()
            });

        if checklist_items.is_empty() && memory_hints.is_empty() {
            tracing::debug!("Heartbeat due, but no pending checklist or reminder items");
            return;
        }

        self.emit(AgentEvent::Observation(
            "Running autonomous heartbeat checks...".to_string(),
        ))
        .await;
        self.set_state(AgentVisualState::Thinking).await;

        let mut user_message = String::from(
            "You are running a scheduled heartbeat cycle for routine maintenance.\n\
             If nothing actionable remains, respond exactly with: NO_ACTION\n\
             If action is needed, use tools to complete work, then provide a concise summary.",
        );

        if !checklist_items.is_empty() {
            user_message.push_str("\n\nPending checklist items:\n");
            for item in &checklist_items {
                user_message.push_str(&format!("- {}\n", item));
            }
        }

        if !memory_hints.is_empty() {
            user_message.push_str("\nReminder-like working-memory notes:\n");
            for note in &memory_hints {
                user_message.push_str(&format!("- {}\n", note));
            }
        }

        user_message.push_str(
            "\nUse safe, incremental actions. If blocked by approval or missing access, explain the block in your summary.",
        );

        let heartbeat_system_prompt = format!(
            "{}\n\nYou are in autonomous heartbeat mode. Be concise and execution-focused.",
            system_prompt
        );

        let loop_config = AgenticConfig {
            max_iterations: 8,
            api_url: agentic_api_url(&llm_api_url),
            model: llm_model,
            api_key: llm_api_key,
            temperature: 0.2,
            max_tokens: 2048,
        };
        let agentic_loop = AgenticLoop::new(loop_config, self.tool_registry.clone());

        let working_directory = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        let tool_ctx = build_tool_context_for_profile(
            &config_snapshot,
            AgentCapabilityProfile::Heartbeat,
            working_directory,
            username,
        );

        match agentic_loop
            .run(&heartbeat_system_prompt, &user_message, &tool_ctx)
            .await
        {
            Ok(result) => {
                let summary = result
                    .response
                    .unwrap_or_else(|| "NO_ACTION".to_string())
                    .trim()
                    .to_string();
                let no_action = summary.eq_ignore_ascii_case("NO_ACTION");

                if !result.thinking_blocks.is_empty() {
                    self.emit(AgentEvent::ReasoningTrace(vec![format!(
                        "Heartbeat model emitted {} thinking block(s) (hidden from summary)",
                        result.thinking_blocks.len()
                    )]))
                    .await;
                }

                if no_action && result.tool_calls_made.is_empty() {
                    tracing::debug!("Heartbeat completed with no action");
                    return;
                }

                let tool_count = result.tool_calls_made.len();
                let event_result = if no_action {
                    format!(
                        "No explicit summary; {} tool call(s) attempted.",
                        tool_count
                    )
                } else {
                    format!(
                        "{} tool call(s). {}",
                        tool_count,
                        truncate_for_event(&summary, 240)
                    )
                };

                self.emit(AgentEvent::ActionTaken {
                    action: "Autonomous heartbeat".to_string(),
                    result: event_result,
                })
                .await;

                if !no_action {
                    let db_lock = self.database.read().await;
                    if let Some(db) = db_lock.as_ref() {
                        if let Err(e) =
                            db.add_chat_message("agent", &format!("[heartbeat] {}", summary))
                        {
                            tracing::warn!("Failed to persist heartbeat summary to chat: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Heartbeat loop failed: {}", e);
                self.emit(AgentEvent::Error(format!("Heartbeat error: {}", e)))
                    .await;
            }
        }
    }

    /// Periodically benchmark memory backends and record promotion decisions.
    ///
    /// This is triggered by heartbeat ticks but has its own longer cadence
    /// (default 24h) and independent enable/disable switch.
    async fn maybe_run_memory_evolution(&self) {
        let (enabled, interval_hours, trace_set_path) = {
            let config = self.config.read().await;
            (
                config.enable_memory_evolution,
                config.memory_evolution_interval_hours.max(1),
                config.memory_eval_trace_set_path.clone(),
            )
        };

        if !enabled {
            return;
        }

        let should_run = {
            let db_lock = self.database.read().await;
            let Some(db) = db_lock.as_ref() else {
                tracing::warn!("Memory evolution enabled but database is unavailable");
                return;
            };

            let now = Utc::now();
            let last_run = db
                .get_state(MEMORY_EVOLUTION_LAST_RUN_STATE_KEY)
                .ok()
                .flatten()
                .and_then(|raw| raw.parse::<chrono::DateTime<Utc>>().ok());

            let is_due = last_run
                .map(|last| now - last >= ChronoDuration::hours(interval_hours as i64))
                .unwrap_or(true);

            if is_due {
                if let Err(e) = db.set_state(MEMORY_EVOLUTION_LAST_RUN_STATE_KEY, &now.to_rfc3339())
                {
                    tracing::warn!("Failed to persist memory evolution timestamp: {}", e);
                }
            }

            is_due
        };

        if !should_run {
            return;
        }

        self.emit(AgentEvent::Observation(
            "Running scheduled memory evolution benchmark...".to_string(),
        ))
        .await;

        let trace_set = match load_memory_eval_trace_set(trace_set_path.as_deref()) {
            Ok(trace_set) => trace_set,
            Err(e) => {
                tracing::warn!("Memory evolution trace load failed: {}", e);
                self.emit(AgentEvent::Error(format!(
                    "Memory evolution skipped: failed to load trace set ({})",
                    e
                )))
                .await;
                return;
            }
        };

        let report = match evaluate_trace_set(
            &trace_set,
            &[
                EvalBackendKind::KvV1,
                EvalBackendKind::FtsV2,
                EvalBackendKind::EpisodicV3,
            ],
        ) {
            Ok(report) => report,
            Err(e) => {
                tracing::warn!("Memory evolution evaluation failed: {}", e);
                self.emit(AgentEvent::Error(format!(
                    "Memory evolution evaluation failed: {}",
                    e
                )))
                .await;
                return;
            }
        };

        let run = MemoryEvalRunRecord::from_report(report.clone());
        let candidate_backend_id = select_promotion_candidate_backend(&report, "kv_v1")
            .unwrap_or_else(|| "fts_v2".to_string());
        let policy = MemoryPromotionPolicy::default();

        let decision_result = {
            let db_lock = self.database.read().await;
            if let Some(db) = db_lock.as_ref() {
                if let Err(e) = db.save_memory_eval_run(&run) {
                    Err(format!("Failed to store memory eval run: {}", e))
                } else {
                    match db.evaluate_and_record_memory_promotion(
                        &run.id,
                        "kv_v1",
                        &candidate_backend_id,
                        &policy,
                    ) {
                        Ok(decision) => Ok(decision),
                        Err(e) => Err(format!("Failed to record memory promotion decision: {}", e)),
                    }
                }
            } else {
                Err("Memory evolution write skipped: database unavailable".to_string())
            }
        };

        let decision = match decision_result {
            Ok(decision) => decision,
            Err(msg) => {
                tracing::warn!("{}", msg);
                self.emit(AgentEvent::Error(msg)).await;
                return;
            }
        };

        let decision_label = match decision.outcome {
            PromotionOutcome::Promote => "promote",
            PromotionOutcome::Hold => "hold",
        };

        self.emit(AgentEvent::ActionTaken {
            action: "Memory evolution eval".to_string(),
            result: format!(
                "run={} candidate={} decision={}",
                run.id, candidate_backend_id, decision_label
            ),
        })
        .await;
    }

    /// Run the full persona evolution cycle (Ludonarrative Assonantic Tracing)
    async fn run_persona_evolution(&self) -> Result<()> {
        // 1. Capture current persona snapshot
        self.emit(AgentEvent::Observation(
            "Capturing persona snapshot...".to_string(),
        ))
        .await;
        let snapshot = self
            .capture_persona_snapshot("scheduled_reflection")
            .await?;

        // 2. Get persona history and guiding principles for trajectory inference
        let (history, guiding_principles) = {
            let db_lock = self.database.read().await;
            let config = self.config.read().await;
            let principles = config.guiding_principles.clone();
            drop(config);

            if let Some(ref db) = *db_lock {
                (db.get_persona_history(10)?, principles)
            } else {
                return Err(anyhow::anyhow!("Database not available"));
            }
        };

        // 3. Run trajectory inference
        self.emit(AgentEvent::Observation(
            "Inferring personality trajectory...".to_string(),
        ))
        .await;
        let trajectory_analysis = {
            let engine_lock = self.trajectory_engine.read().await;
            if let Some(ref engine) = *engine_lock {
                engine
                    .infer_trajectory(&history, &guiding_principles)
                    .await?
            } else {
                return Err(anyhow::anyhow!("Trajectory engine not available"));
            }
        };

        // 4. Log the trajectory analysis
        tracing::info!("Trajectory Analysis:");
        tracing::info!("  Narrative: {}", trajectory_analysis.narrative);
        tracing::info!("  Trajectory: {}", trajectory_analysis.trajectory);
        tracing::info!("  Themes: {:?}", trajectory_analysis.themes);
        tracing::info!("  Confidence: {:.2}", trajectory_analysis.confidence);

        self.emit(AgentEvent::Observation(format!(
            "Trajectory inferred: {} (confidence: {:.0}%)",
            &trajectory_analysis.trajectory[..trajectory_analysis.trajectory.len().min(80)],
            trajectory_analysis.confidence * 100.0
        )))
        .await;

        // 5. Update the snapshot with trajectory and save
        let mut updated_snapshot = snapshot;
        updated_snapshot.inferred_trajectory = Some(trajectory_analysis.trajectory.clone());

        {
            let db_lock = self.database.read().await;
            if let Some(ref db) = *db_lock {
                db.save_persona_snapshot(&updated_snapshot)?;
                db.set_last_reflection_time(Utc::now())?;
            }
        }

        // 6. Emit reasoning trace with trajectory insights
        self.emit(AgentEvent::ReasoningTrace(vec![
            "Persona Evolution Complete".to_string(),
            format!("Narrative: {}", trajectory_analysis.narrative),
            format!("Direction: {}", trajectory_analysis.trajectory),
            format!("Themes: {}", trajectory_analysis.themes.join(", ")),
            format!(
                "Tensions: {}",
                if trajectory_analysis.tensions.is_empty() {
                    "None identified".to_string()
                } else {
                    trajectory_analysis.tensions.join(", ")
                }
            ),
        ]))
        .await;

        self.set_state(AgentVisualState::Happy).await;
        sleep(Duration::from_secs(2)).await;

        Ok(())
    }

    /// Capture a persona snapshot
    async fn capture_persona_snapshot(
        &self,
        trigger: &str,
    ) -> Result<crate::database::PersonaSnapshot> {
        let config = self.config.read().await;
        let api_url = config.llm_api_url.clone();
        let model = config
            .reflection_model
            .clone()
            .unwrap_or_else(|| config.llm_model.clone());
        let api_key = config.llm_api_key.clone();
        let system_prompt = config.system_prompt.clone();
        let guiding_principles = config.guiding_principles.clone();
        drop(config);

        // Get recent important posts as formative experiences
        let experiences: Vec<String> = {
            let db_lock = self.database.read().await;
            if let Some(ref db) = *db_lock {
                db.get_recent_important_posts(5)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.why_important)
                    .collect()
            } else {
                vec![]
            }
        };

        let snapshot = trajectory::capture_persona_snapshot(
            &api_url,
            &model,
            api_key.as_deref(),
            &system_prompt,
            trigger,
            &experiences,
            &guiding_principles,
        )
        .await?;

        // Save the snapshot
        {
            let db_lock = self.database.read().await;
            if let Some(ref db) = *db_lock {
                db.save_persona_snapshot(&snapshot)?;
            }
        }

        tracing::info!("Captured persona snapshot: {}", snapshot.self_description);
        Ok(snapshot)
    }

    async fn run_cycle(&self) -> Result<()> {
        // First, check for and process any private chat messages
        self.process_chat_messages().await?;

        // Poll all skills for new events
        self.set_state(AgentVisualState::Reading).await;
        self.emit(AgentEvent::Observation(
            "Polling skills for new events...".to_string(),
        ))
        .await;

        let username = {
            let config = self.config.read().await;
            config.username.clone()
        };

        let skill_ctx = SkillContext {
            username: username.clone(),
        };

        // Collect events from all skills
        let mut all_events: Vec<SkillEvent> = Vec::new();
        {
            let skills = self.skills.read().await;
            for skill in skills.iter() {
                match skill.poll(&skill_ctx).await {
                    Ok(events) => {
                        if !events.is_empty() {
                            tracing::debug!(
                                "Skill '{}' produced {} events",
                                skill.name(),
                                events.len()
                            );
                        }
                        all_events.extend(events);
                    }
                    Err(e) => {
                        tracing::warn!("Skill '{}' poll failed: {}", skill.name(), e);
                        self.emit(AgentEvent::Error(format!(
                            "Skill '{}' error: {}",
                            skill.name(),
                            e
                        )))
                        .await;
                    }
                }
            }
        }

        // Filter out already-processed events and agent's own events
        let processed_events = {
            let state = self.state.read().await;
            state.processed_events.clone()
        };

        let filtered_events: Vec<SkillEvent> = all_events
            .into_iter()
            .filter(|event| {
                let SkillEvent::NewContent {
                    ref id, ref author, ..
                } = event;
                let already_processed = processed_events.contains(id);
                let is_own = author == &username;
                !already_processed && !is_own
            })
            .collect();

        if filtered_events.is_empty() {
            self.emit(AgentEvent::Observation(
                "No new events from skills.".to_string(),
            ))
            .await;
            return Ok(());
        }

        self.emit(AgentEvent::Observation(format!(
            "Found {} new events to analyze",
            filtered_events.len()
        )))
        .await;

        // Get working memory and chat context from database
        let (working_memory_context, chat_context) = {
            let db_lock = self.database.read().await;
            if let Some(ref db) = *db_lock {
                let wm = db.get_working_memory_context().unwrap_or_default();
                let chat = db.get_chat_context(10).unwrap_or_default();
                (wm, chat)
            } else {
                (String::new(), String::new())
            }
        };

        // Reason about events using the same agentic loop used for private chat.
        self.set_state(AgentVisualState::Thinking).await;
        self.emit(AgentEvent::Observation(
            "Analyzing skill events via agentic loop...".to_string(),
        ))
        .await;

        let config_snapshot = { self.config.read().await.clone() };
        let llm_api_url = config_snapshot.llm_api_url.clone();
        let llm_model = config_snapshot.llm_model.clone();
        let llm_api_key = config_snapshot.llm_api_key.clone();
        let system_prompt = config_snapshot.system_prompt.clone();
        let loop_config = AgenticConfig {
            max_iterations: 8,
            api_url: agentic_api_url(&llm_api_url),
            model: llm_model,
            api_key: llm_api_key,
            temperature: 0.35,
            max_tokens: 1536,
        };
        let agentic_loop = AgenticLoop::new(loop_config, self.tool_registry.clone());
        let tool_ctx = build_tool_context_for_profile(
            &config_snapshot,
            AgentCapabilityProfile::SkillEvents,
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            username.clone(),
        );
        let skill_system_prompt = format!(
            "{}\n\nYou are processing external skill events. Decide whether to take action.\nUse tools when needed.\nIf replying on Graphchan, call tool `graphchan_skill` with action=`reply` and params containing `post_id` (or `event_id`) and `content`; include `thread_id` when known.\nYou may use `write_memory` for durable notes and `search_memory` for recall.\nIf no action is needed, explain briefly and return.",
            system_prompt
        );
        let user_message = build_skill_events_agentic_prompt(
            &filtered_events,
            &working_memory_context,
            &chat_context,
        );

        match agentic_loop
            .run(&skill_system_prompt, &user_message, &tool_ctx)
            .await
        {
            Ok(result) => {
                let mut trace_lines = vec![format!(
                    "Skill-event agentic pass ({} event(s), {} tool call(s))",
                    filtered_events.len(),
                    result.tool_calls_made.len()
                )];
                if !result.thinking_blocks.is_empty() {
                    trace_lines.push(format!(
                        "Model emitted {} thinking block(s) (hidden from operator-facing outputs)",
                        result.thinking_blocks.len()
                    ));
                }
                trace_lines.extend(tool_trace_lines(&result.tool_calls_made));
                self.emit(AgentEvent::ReasoningTrace(trace_lines)).await;

                if let Some(response) = result.response.as_deref().filter(|r| !r.trim().is_empty())
                {
                    self.emit(AgentEvent::Observation(format!(
                        "Skill-event summary: {}",
                        truncate_for_event(&response.replace('\n', " "), 220)
                    )))
                    .await;
                }

                let successful_graphchan_calls = result
                    .tool_calls_made
                    .iter()
                    .filter(|call| call.tool_name == "graphchan_skill" && call.output.is_success())
                    .count();
                if successful_graphchan_calls > 0 {
                    let mut state = self.state.write().await;
                    state.actions_this_hour += successful_graphchan_calls as u32;
                    state.last_action_time = Some(chrono::Utc::now());
                    drop(state);
                    self.emit(AgentEvent::ActionTaken {
                        action: "Graphchan action(s) via agentic loop".to_string(),
                        result: format!(
                            "{} successful graphchan_skill call(s)",
                            successful_graphchan_calls
                        ),
                    })
                    .await;
                    self.set_state(AgentVisualState::Happy).await;
                    sleep(Duration::from_secs(2)).await;
                } else {
                    self.emit(AgentEvent::Observation(
                        "No external reply action was required.".to_string(),
                    ))
                    .await;
                }

                // Mark analyzed events as processed so we don't re-analyze them.
                let mut state = self.state.write().await;
                for event in &filtered_events {
                    let SkillEvent::NewContent { ref id, .. } = event;
                    state.processed_events.insert(id.clone());
                }
            }
            Err(e) => {
                self.emit(AgentEvent::Error(format!(
                    "Skill-event agentic loop failed: {}",
                    e
                )))
                .await;
                self.set_state(AgentVisualState::Confused).await;
            }
        }

        self.set_state(AgentVisualState::Idle).await;

        Ok(())
    }

    /// Process any unprocessed chat messages from the operator
    async fn process_chat_messages(&self) -> Result<()> {
        // Get unprocessed operator messages
        let unprocessed_messages = {
            let db_lock = self.database.read().await;
            if let Some(ref db) = *db_lock {
                db.get_unprocessed_operator_messages().unwrap_or_default()
            } else {
                return Ok(());
            }
        };

        if unprocessed_messages.is_empty() {
            return Ok(());
        }

        let mut messages_by_conversation: Vec<(String, Vec<crate::database::ChatMessage>)> =
            Vec::new();
        for msg in unprocessed_messages {
            let conversation_id = msg.conversation_id.clone();
            if let Some((_, bucket)) = messages_by_conversation
                .iter_mut()
                .find(|(id, _)| id == &conversation_id)
            {
                bucket.push(msg);
            } else {
                messages_by_conversation.push((conversation_id, vec![msg]));
            }
        }

        let total_messages = messages_by_conversation
            .iter()
            .map(|(_, msgs)| msgs.len())
            .sum::<usize>();

        self.emit(AgentEvent::Observation(format!(
            "Processing {} private message(s) across {} conversation(s)...",
            total_messages,
            messages_by_conversation.len()
        )))
        .await;
        self.set_state(AgentVisualState::Thinking).await;

        let (working_memory_context, config_snapshot) = {
            let db_lock = self.database.read().await;
            let wm = if let Some(ref db) = *db_lock {
                db.get_working_memory_context().unwrap_or_default()
            } else {
                String::new()
            };

            let config = self.config.read().await;
            (wm, config.clone())
        };
        let llm_api_url = config_snapshot.llm_api_url.clone();
        let llm_model = config_snapshot.llm_model.clone();
        let llm_api_key = config_snapshot.llm_api_key.clone();
        let system_prompt = config_snapshot.system_prompt.clone();
        let username = config_snapshot.username.clone();

        let loop_config = AgenticConfig {
            max_iterations: 10,
            api_url: agentic_api_url(&llm_api_url),
            model: llm_model.clone(),
            api_key: llm_api_key.clone(),
            temperature: 0.35,
            max_tokens: 2048,
        };
        let agentic_loop = AgenticLoop::new(loop_config, self.tool_registry.clone());
        let tool_ctx = build_tool_context_for_profile(
            &config_snapshot,
            AgentCapabilityProfile::PrivateChat,
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            username,
        );

        let chat_system_prompt = format!(
            "{}\n\nYou are in direct operator chat mode. Use tools when they improve correctness or save effort.\nYou may run multiple internal turns before yielding back to the operator.\nDo not use Graphchan posting/reply tools in private chat.\nEvery response MUST end with a turn-control JSON block in this exact envelope:\n{}\n{{\"decision\":\"continue|yield\",\"status\":\"still_working|done|blocked\",\"needs_user_input\":true|false,\"user_message\":\"operator-facing text\",\"reason\":\"short internal rationale\"}}\n{}\nChoose decision='continue' only if you can make immediate progress now without user clarification.\nChoose decision='yield' when done, blocked, or waiting on user input.",
            system_prompt,
            CHAT_TURN_CONTROL_BLOCK_START,
            CHAT_TURN_CONTROL_BLOCK_END
        );

        for (conversation_id, conversation_messages) in messages_by_conversation {
            let mut pending_messages = conversation_messages.clone();
            let mut continuation_hint: Option<String> = None;
            let mut marked_initial_messages = false;
            let conversation_summary_context = self
                .maybe_refresh_conversation_compaction_summary(
                    &conversation_id,
                    &llm_api_url,
                    &llm_model,
                    llm_api_key.as_deref(),
                    &system_prompt,
                )
                .await;

            for turn in 1..=CHAT_MAX_AUTONOMOUS_TURNS {
                let turn_trigger_message_ids: Vec<String> =
                    pending_messages.iter().map(|m| m.id.clone()).collect();
                let turn_id = {
                    let db_lock = self.database.read().await;
                    if let Some(ref db) = *db_lock {
                        match db.begin_chat_turn(
                            &conversation_id,
                            &turn_trigger_message_ids,
                            turn as i64,
                        ) {
                            Ok(id) => Some(id),
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to persist start of chat turn [{}]: {}",
                                    truncate_for_event(&conversation_id, 12),
                                    e
                                );
                                None
                            }
                        }
                    } else {
                        None
                    }
                };

                self.emit(AgentEvent::Observation(format!(
                    "Operator task [{}] turn {}/{}",
                    truncate_for_event(&conversation_id, 12),
                    turn,
                    CHAT_MAX_AUTONOMOUS_TURNS
                )))
                .await;

                let recent_chat_context = {
                    let db_lock = self.database.read().await;
                    if let Some(ref db) = *db_lock {
                        db.get_chat_context_for_conversation(
                            &conversation_id,
                            CHAT_CONTEXT_RECENT_LIMIT,
                        )
                        .unwrap_or_default()
                    } else {
                        String::new()
                    }
                };

                let user_message = build_private_chat_agentic_prompt(
                    &pending_messages,
                    &working_memory_context,
                    &recent_chat_context,
                    conversation_summary_context.as_deref(),
                    continuation_hint.as_deref(),
                );
                let event_tx = self.event_tx.clone();
                let stream_conversation_id = conversation_id.clone();
                let stream_callback = move |content: &str, done: bool| {
                    let _ = event_tx.send(AgentEvent::ChatStreaming {
                        conversation_id: stream_conversation_id.clone(),
                        content: content.to_string(),
                        done,
                    });
                };
                let tool_event_tx = self.event_tx.clone();
                let tool_event_conversation_id = conversation_id.clone();
                let tool_event_callback = move |record: &ToolCallRecord| {
                    let output_preview =
                        truncate_for_event(&record.output.to_llm_string().replace('\n', " "), 220);
                    let _ = tool_event_tx.send(AgentEvent::ToolCallProgress {
                        conversation_id: tool_event_conversation_id.clone(),
                        tool_name: record.tool_name.clone(),
                        output_preview,
                    });
                };

                let result = match agentic_loop
                    .run_with_history_streaming_and_tool_events(
                        &chat_system_prompt,
                        vec![],
                        &user_message,
                        &tool_ctx,
                        &stream_callback,
                        Some(&tool_event_callback),
                    )
                    .await
                {
                    Ok(result) => result,
                    Err(e) => {
                        if let Some(turn_id) = turn_id.as_deref() {
                            let db_lock = self.database.read().await;
                            if let Some(ref db) = *db_lock {
                                if let Err(db_err) = db.fail_chat_turn(turn_id, &e.to_string()) {
                                    tracing::warn!(
                                        "Failed to persist failed chat turn: {}",
                                        db_err
                                    );
                                }
                                if let Err(db_err) = db.append_daily_activity_log(&format!(
                                    "chat [{}] turn {} failed: {}",
                                    truncate_for_event(&conversation_id, 12),
                                    turn,
                                    truncate_for_event(&e.to_string(), 180)
                                )) {
                                    tracing::warn!(
                                        "Failed to append chat failure to activity log: {}",
                                        db_err
                                    );
                                }
                            }
                        }
                        self.emit(AgentEvent::Error(format!(
                            "Private chat turn failed [{}]: {}",
                            truncate_for_event(&conversation_id, 12),
                            e
                        )))
                        .await;
                        break;
                    }
                };

                let base_response = result.response.unwrap_or_else(|| {
                    if result.tool_calls_made.is_empty() {
                        "I do not have a useful response yet.".to_string()
                    } else {
                        "I ran tools for your request. Details are attached below.".to_string()
                    }
                });
                let tool_count = result.tool_calls_made.len();
                let turn_control = parse_turn_control(&base_response, tool_count);
                let should_continue = turn_control.decision == TurnDecision::Continue
                    && !turn_control.needs_user_input
                    && turn < CHAT_MAX_AUTONOMOUS_TURNS
                    && (tool_count > 0 || turn_control.status == "still_working");
                let operator_visible_response = if !turn_control.operator_response.trim().is_empty()
                {
                    turn_control.operator_response.clone()
                } else if should_continue {
                    turn_control
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Still working on your request...".to_string())
                } else {
                    base_response.clone()
                };

                let chat_content = format_chat_message_with_metadata(
                    &operator_visible_response,
                    &result.tool_calls_made,
                    &result.thinking_blocks,
                );

                let mut agent_message_id: Option<String> = None;
                if !should_continue {
                    let db_lock = self.database.read().await;
                    if let Some(ref db) = *db_lock {
                        let add_result = if let Some(turn_id) = turn_id.as_deref() {
                            db.add_chat_message_in_turn(
                                &conversation_id,
                                turn_id,
                                "agent",
                                &chat_content,
                            )
                        } else {
                            db.add_chat_message_in_conversation(
                                &conversation_id,
                                "agent",
                                &chat_content,
                            )
                        };
                        match add_result {
                            Ok(message_id) => {
                                agent_message_id = Some(message_id);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to save agent chat reply: {}", e);
                            }
                        }
                    }
                }

                {
                    let db_lock = self.database.read().await;
                    if let Some(ref db) = *db_lock {
                        if let Some(turn_id) = turn_id.as_deref() {
                            for (idx, record) in result.tool_calls_made.iter().enumerate() {
                                if let Err(e) = db.record_chat_turn_tool_call(
                                    turn_id,
                                    idx,
                                    &record.tool_name,
                                    &record.arguments.to_string(),
                                    &record.output.to_llm_string(),
                                ) {
                                    tracing::warn!(
                                        "Failed to persist chat turn tool call {} for {}: {}",
                                        idx,
                                        record.tool_name,
                                        e
                                    );
                                }
                            }
                        }

                        if !marked_initial_messages
                            && !should_continue
                            && agent_message_id.is_some()
                        {
                            for msg in &conversation_messages {
                                if let Err(e) = db.mark_message_processed(&msg.id) {
                                    tracing::warn!("Failed to mark message as processed: {}", e);
                                }
                                if let Err(e) = db.append_daily_activity_log(&format!(
                                    "operator [{}]: {}",
                                    truncate_for_event(&conversation_id, 12),
                                    truncate_for_event(msg.content.trim(), 220)
                                )) {
                                    tracing::warn!(
                                        "Failed to append operator message to activity log: {}",
                                        e
                                    );
                                }
                            }
                            marked_initial_messages = true;
                        }
                    }
                }

                if let Some(turn_id) = turn_id.as_deref() {
                    let completion_phase = if should_continue {
                        ChatTurnPhase::Completed
                    } else if turn_control.needs_user_input {
                        ChatTurnPhase::AwaitingApproval
                    } else if turn_control.status == "blocked" {
                        ChatTurnPhase::Failed
                    } else {
                        ChatTurnPhase::Completed
                    };
                    let decision_text = match turn_control.decision {
                        TurnDecision::Continue => "continue",
                        TurnDecision::Yield => "yield",
                    };
                    let db_lock = self.database.read().await;
                    if let Some(ref db) = *db_lock {
                        if let Err(e) = db.complete_chat_turn(
                            turn_id,
                            completion_phase,
                            decision_text,
                            &turn_control.status,
                            &operator_visible_response,
                            turn_control.reason.as_deref(),
                            tool_count,
                            agent_message_id.as_deref(),
                        ) {
                            tracing::warn!("Failed to persist completed chat turn: {}", e);
                        }
                        if let Err(e) = db.append_daily_activity_log(&format!(
                            "agent [{}] turn {}: decision={}, status={}, tools={}",
                            truncate_for_event(&conversation_id, 12),
                            turn,
                            decision_text,
                            turn_control.status,
                            tool_count
                        )) {
                            tracing::warn!("Failed to append agent turn to activity log: {}", e);
                        }
                    }
                }

                let mut trace_lines = vec![format!(
                    "Private chat [{}] turn {}/{} via agentic loop ({} tool call(s))",
                    truncate_for_event(&conversation_id, 12),
                    turn,
                    CHAT_MAX_AUTONOMOUS_TURNS,
                    tool_count
                )];
                if !result.thinking_blocks.is_empty() {
                    trace_lines.push(format!(
                        "Model emitted {} thinking block(s) (hidden from main reply)",
                        result.thinking_blocks.len()
                    ));
                }
                trace_lines.extend(tool_trace_lines(&result.tool_calls_made));
                self.emit(AgentEvent::ReasoningTrace(trace_lines)).await;

                if should_continue {
                    self.emit(AgentEvent::ActionTaken {
                        action: "Continuing autonomous operator task".to_string(),
                        result: format!(
                            "[{}] {} tool call(s), status={}. {}",
                            truncate_for_event(&conversation_id, 12),
                            tool_count,
                            turn_control.status,
                            truncate_for_event(&operator_visible_response, 80)
                        ),
                    })
                    .await;

                    pending_messages.clear();
                    continuation_hint = Some(format!(
                        "Previous autonomous turn: status={}, tools={}, summary=\"{}\", reason=\"{}\". Continue only if meaningful progress is still possible without operator input.",
                        turn_control.status,
                        tool_count,
                        truncate_for_event(&operator_visible_response.replace('\n', " "), 220),
                        truncate_for_event(
                            turn_control.reason.as_deref().unwrap_or(""),
                            180
                        )
                    ));
                    continue;
                }

                self.emit(AgentEvent::ActionTaken {
                    action: "Replied to operator".to_string(),
                    result: format!(
                        "[{}] {} tool call(s), status={}. {}",
                        truncate_for_event(&conversation_id, 12),
                        tool_count,
                        turn_control.status,
                        truncate_for_event(&operator_visible_response, 80)
                    ),
                })
                .await;
                break;
            }
        }

        self.set_state(AgentVisualState::Happy).await;
        sleep(Duration::from_millis(500)).await;

        Ok(())
    }
}

impl Agent {
    async fn maybe_refresh_conversation_compaction_summary(
        &self,
        conversation_id: &str,
        llm_api_url: &str,
        llm_model: &str,
        llm_api_key: Option<&str>,
        system_prompt: &str,
    ) -> Option<String> {
        let (message_count, existing_summary) = {
            let db_lock = self.database.read().await;
            let db = db_lock.as_ref()?;
            let count = db
                .count_chat_messages_for_conversation(conversation_id)
                .ok()
                .unwrap_or(0);
            let summary = db
                .get_chat_conversation_summary(conversation_id)
                .ok()
                .flatten();
            (count, summary)
        };

        if message_count < CHAT_COMPACTION_TRIGGER_MESSAGES {
            return None;
        }

        let older_message_count = message_count.saturating_sub(CHAT_CONTEXT_RECENT_LIMIT);
        if older_message_count == 0 {
            return None;
        }

        let mut summary_text = existing_summary.as_ref().map(|s| s.summary_text.clone());
        let covered_count = existing_summary
            .as_ref()
            .map(|s| s.summarized_message_count)
            .unwrap_or(0);
        let needs_refresh = covered_count == 0
            || covered_count > older_message_count
            || older_message_count.saturating_sub(covered_count) >= CHAT_COMPACTION_RESUMMARY_DELTA;

        if needs_refresh {
            let source_limit = older_message_count.min(CHAT_COMPACTION_SOURCE_MAX_MESSAGES);
            let source_messages = {
                let db_lock = self.database.read().await;
                let db = db_lock.as_ref()?;
                db.get_chat_history_slice_for_conversation(
                    conversation_id,
                    CHAT_CONTEXT_RECENT_LIMIT,
                    source_limit,
                )
                .ok()
                .unwrap_or_default()
            };

            if !source_messages.is_empty() {
                let refreshed = self
                    .summarize_conversation_slice_with_llm(
                        &source_messages,
                        llm_api_url,
                        llm_model,
                        llm_api_key,
                        system_prompt,
                    )
                    .await
                    .unwrap_or_else(|| fallback_chat_summary_snapshot(&source_messages));

                if !refreshed.trim().is_empty() {
                    let db_lock = self.database.read().await;
                    if let Some(ref db) = *db_lock {
                        if let Err(e) = db.upsert_chat_conversation_summary(
                            conversation_id,
                            refreshed.trim(),
                            older_message_count,
                        ) {
                            tracing::warn!(
                                "Failed to persist conversation summary snapshot [{}]: {}",
                                truncate_for_event(conversation_id, 12),
                                e
                            );
                        } else {
                            summary_text = Some(refreshed.trim().to_string());
                        }
                    }
                }
            }
        }

        summary_text
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|s| {
                format!(
                    "{}\n\n_Covers approximately {} earlier message(s)._",
                    s, older_message_count
                )
            })
    }

    async fn summarize_conversation_slice_with_llm(
        &self,
        messages: &[crate::database::ChatMessage],
        llm_api_url: &str,
        llm_model: &str,
        llm_api_key: Option<&str>,
        system_prompt: &str,
    ) -> Option<String> {
        if messages.is_empty() {
            return None;
        }

        let transcript = format_chat_summary_transcript(messages);
        if transcript.trim().is_empty() {
            return None;
        }

        let summarizer_system_prompt = format!(
            "{}\n\nYou are summarizing private operator-agent chat history for internal context compaction.\nProduce concise markdown with these sections exactly: `### Objectives`, `### Decisions & Findings`, `### Open Threads`.\nStay factual, avoid roleplay, and keep the summary under 220 words.",
            system_prompt.trim()
        );
        let summarizer_user_prompt = format!(
            "Summarize this older conversation slice so future turns can retain continuity without replaying full history.\n\n{}",
            transcript
        );
        let client = LlmClient::new(
            agentic_api_url(llm_api_url),
            llm_api_key.unwrap_or("").to_string(),
            llm_model.to_string(),
        );
        let result = tokio::time::timeout(
            Duration::from_secs(20),
            client.generate_with_model(
                vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: summarizer_system_prompt,
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: summarizer_user_prompt,
                    },
                ],
                llm_model,
            ),
        )
        .await
        .ok()?
        .ok()?;

        let (without_turn_control, _) = extract_metadata_block(
            &result,
            CHAT_TURN_CONTROL_BLOCK_START,
            CHAT_TURN_CONTROL_BLOCK_END,
        );
        let cleaned = strip_inline_thinking_tags(&without_turn_control);
        let normalized = cleaned.trim();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_string())
        }
    }
}

fn load_pending_checklist_items(path: &str) -> Result<Vec<String>> {
    let raw = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    Ok(parse_pending_checklist_items(&raw))
}

fn load_memory_eval_trace_set(
    trace_set_path: Option<&str>,
) -> Result<crate::memory::eval::MemoryEvalTraceSet> {
    match trace_set_path.map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => load_trace_set(Path::new(path)),
        None => Ok(default_replay_trace_set()),
    }
}

fn select_promotion_candidate_backend(
    report: &MemoryEvalReport,
    baseline_backend_id: &str,
) -> Option<String> {
    if let Some(winner) = report.winner.as_ref() {
        if winner != baseline_backend_id {
            return Some(winner.clone());
        }
    }

    report
        .candidates
        .iter()
        .filter(|c| c.backend_id != baseline_backend_id)
        .max_by(|a, b| {
            let a_get_pass = a.metrics.get_passed as f64 / a.metrics.get_checks.max(1) as f64;
            let b_get_pass = b.metrics.get_passed as f64 / b.metrics.get_checks.max(1) as f64;

            let a_key = (
                ordered_score(a.metrics.recall_at_k),
                ordered_score(a.metrics.recall_at_1),
                ordered_score(a_get_pass),
                std::cmp::Reverse(ordered_score(a.metrics.mean_check_ms)),
            );
            let b_key = (
                ordered_score(b.metrics.recall_at_k),
                ordered_score(b.metrics.recall_at_1),
                ordered_score(b_get_pass),
                std::cmp::Reverse(ordered_score(b.metrics.mean_check_ms)),
            );
            a_key.cmp(&b_key)
        })
        .map(|c| c.backend_id.clone())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatToolCallDetail {
    tool_name: String,
    arguments_preview: String,
    output_kind: String,
    output_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMediaDetail {
    path: String,
    media_kind: String,
    mime_type: Option<String>,
    source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnDecision {
    Continue,
    Yield,
}

#[derive(Debug, Clone)]
struct ParsedTurnControl {
    operator_response: String,
    decision: TurnDecision,
    needs_user_input: bool,
    status: String,
    reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TurnControlBlock {
    decision: Option<String>,
    status: Option<String>,
    needs_user_input: Option<bool>,
    user_message: Option<String>,
    reason: Option<String>,
}

fn build_private_chat_agentic_prompt(
    new_messages: &[crate::database::ChatMessage],
    working_memory_context: &str,
    recent_chat_context: &str,
    summary_snapshot: Option<&str>,
    continuation_hint: Option<&str>,
) -> String {
    let mut prompt = String::new();

    if !working_memory_context.trim().is_empty() {
        prompt.push_str(working_memory_context.trim());
        prompt.push_str("\n\n---\n\n");
    }

    if !recent_chat_context.trim().is_empty() {
        prompt.push_str("## Recent Conversation Context\n\n");
        prompt.push_str(recent_chat_context.trim());
        prompt.push_str("\n\n---\n\n");
    }

    if let Some(summary) = summary_snapshot
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
    {
        prompt.push_str("## Conversation Summary Snapshot\n\n");
        prompt.push_str(summary);
        prompt.push_str("\n\n---\n\n");
    }

    if !new_messages.is_empty() {
        prompt.push_str("## New Operator Message(s)\n\n");
        for msg in new_messages {
            prompt.push_str("- ");
            prompt.push_str(msg.content.trim());
            prompt.push('\n');
        }
        prompt.push('\n');
    }

    if let Some(hint) = continuation_hint
        .map(str::trim)
        .filter(|hint| !hint.is_empty())
    {
        prompt.push_str("## Autonomous Continuation Context\n\n");
        prompt.push_str(hint);
        prompt.push_str("\n\n");
    }

    prompt.push_str(
        "Respond directly to the operator. Use tools when useful. If you use tools, verify results before answering.",
    );
    prompt
}

fn build_skill_events_agentic_prompt(
    events: &[SkillEvent],
    working_memory_context: &str,
    chat_context: &str,
) -> String {
    let mut prompt = String::new();

    if !working_memory_context.trim().is_empty() {
        prompt.push_str(working_memory_context.trim());
        prompt.push_str("\n\n---\n\n");
    }
    if !chat_context.trim().is_empty() {
        prompt.push_str(chat_context.trim());
        prompt.push_str("\n\n---\n\n");
    }

    prompt.push_str("## Incoming Skill Events\n\n");
    for (index, event) in events.iter().enumerate() {
        let SkillEvent::NewContent {
            id,
            source,
            author,
            body,
            parent_ids,
        } = event;
        let parent_summary = if parent_ids.is_empty() {
            "none".to_string()
        } else {
            parent_ids.join(", ")
        };
        prompt.push_str(&format!(
            "{}. event_id={} source=\"{}\" author=\"{}\" parents=[{}]\n   body: {}\n\n",
            index + 1,
            id,
            source,
            author,
            parent_summary,
            body.trim()
        ));
    }

    prompt.push_str(
        "Decide whether to act. If replying to Graphchan, call `graphchan_skill` with action=`reply` and include `post_id`/`event_id` plus `content` (and `thread_id` when available). If no action is needed, explain why briefly.",
    );
    prompt
}

fn format_chat_summary_transcript(messages: &[crate::database::ChatMessage]) -> String {
    let mut transcript = String::from("## Older Conversation Slice\n\n");
    for message in messages {
        let role = if message.role.eq_ignore_ascii_case("operator") {
            "Operator"
        } else {
            "Agent"
        };
        let content = truncate_for_event(&message.content.replace('\n', " "), 260);
        transcript.push_str(&format!("- {}: {}\n", role, content.trim()));
    }
    transcript
}

fn fallback_chat_summary_snapshot(messages: &[crate::database::ChatMessage]) -> String {
    let mut operator_points = Vec::new();
    let mut agent_points = Vec::new();

    for message in messages.iter().rev() {
        let collapsed = message.content.replace('\n', " ");
        let content = truncate_for_event(collapsed.trim(), 180);
        if content.is_empty() {
            continue;
        }
        if message.role.eq_ignore_ascii_case("operator") {
            if operator_points.len() < 4 {
                operator_points.push(content);
            }
        } else if agent_points.len() < 4 {
            agent_points.push(content);
        }
        if operator_points.len() >= 4 && agent_points.len() >= 4 {
            break;
        }
    }

    operator_points.reverse();
    agent_points.reverse();

    let mut summary = String::new();
    summary.push_str("### Objectives\n");
    if operator_points.is_empty() {
        summary.push_str("- Operator intent not explicitly captured in older turns.\n");
    } else {
        for point in &operator_points {
            summary.push_str(&format!("- {}\n", point));
        }
    }

    summary.push_str("\n### Decisions & Findings\n");
    if agent_points.is_empty() {
        summary.push_str("- No stable agent conclusions recorded in the compacted window.\n");
    } else {
        for point in &agent_points {
            summary.push_str(&format!("- {}\n", point));
        }
    }

    summary.push_str("\n### Open Threads\n");
    if let Some(last_operator) = operator_points.last() {
        summary.push_str(&format!(
            "- Revisit latest operator objective: {}\n",
            last_operator
        ));
    } else {
        summary.push_str("- Validate whether additional operator input is needed.\n");
    }

    summary
}

fn format_chat_message_with_metadata(
    response: &str,
    tool_calls: &[ToolCallRecord],
    thinking_blocks: &[String],
) -> String {
    let mut content = response.trim().to_string();
    if content.is_empty() {
        content = if tool_calls.is_empty() {
            "I do not have a useful response yet.".to_string()
        } else {
            "I ran tools for your request.".to_string()
        };
    }

    if !thinking_blocks.is_empty() {
        let thinking_json =
            serde_json::to_string(thinking_blocks).unwrap_or_else(|_| "[]".to_string());
        content.push_str("\n\n");
        content.push_str(CHAT_THINKING_BLOCK_START);
        content.push('\n');
        content.push_str(&thinking_json);
        content.push('\n');
        content.push_str(CHAT_THINKING_BLOCK_END);
    }

    let media_details = extract_media_details(tool_calls);
    if !media_details.is_empty() {
        let media_json = serde_json::to_string(&media_details).unwrap_or_else(|_| "[]".to_string());
        content.push_str("\n\n");
        content.push_str(CHAT_MEDIA_BLOCK_START);
        content.push('\n');
        content.push_str(&media_json);
        content.push('\n');
        content.push_str(CHAT_MEDIA_BLOCK_END);
    }

    if tool_calls.is_empty() {
        return content;
    }

    let details = tool_calls
        .iter()
        .map(|call| ChatToolCallDetail {
            tool_name: call.tool_name.clone(),
            arguments_preview: truncate_for_event(
                &serde_json::to_string_pretty(&call.arguments)
                    .unwrap_or_else(|_| call.arguments.to_string()),
                500,
            ),
            output_kind: tool_output_kind(&call.output).to_string(),
            output_preview: truncate_for_event(&call.output.to_llm_string(), 900),
        })
        .collect::<Vec<_>>();

    let details_json = serde_json::to_string(&details).unwrap_or_else(|_| "[]".to_string());
    content.push_str("\n\n");
    content.push_str(CHAT_TOOL_BLOCK_START);
    content.push('\n');
    content.push_str(&details_json);
    content.push('\n');
    content.push_str(CHAT_TOOL_BLOCK_END);
    content
}

fn tool_output_kind(output: &ToolOutput) -> &'static str {
    match output {
        ToolOutput::Text(_) => "text",
        ToolOutput::Json(_) => "json",
        ToolOutput::Error(_) => "error",
        ToolOutput::NeedsApproval { .. } => "needs_approval",
    }
}

fn extract_media_details(tool_calls: &[ToolCallRecord]) -> Vec<ChatMediaDetail> {
    let mut media = Vec::new();

    for call in tool_calls {
        let ToolOutput::Json(payload) = &call.output else {
            continue;
        };

        let Some(items) = payload.get("media").and_then(serde_json::Value::as_array) else {
            continue;
        };

        for item in items {
            let Some(path) = item
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
            else {
                continue;
            };

            let media_kind = item
                .get("media_kind")
                .or_else(|| item.get("kind"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| infer_media_kind_from_path(path));

            let mime_type = item
                .get("mime_type")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|mime| !mime.is_empty())
                .map(str::to_string);

            media.push(ChatMediaDetail {
                path: path.to_string(),
                media_kind,
                mime_type,
                source: call.tool_name.clone(),
            });
        }
    }

    media
}

fn tool_trace_lines(tool_calls: &[ToolCallRecord]) -> Vec<String> {
    tool_calls
        .iter()
        .map(|call| {
            format!(
                "{} -> {}",
                call.tool_name,
                truncate_for_event(&call.output.to_llm_string(), 80)
            )
        })
        .collect()
}

fn parse_turn_control(response: &str, tool_call_count: usize) -> ParsedTurnControl {
    let (cleaned_response, block_json) = extract_metadata_block(
        response,
        CHAT_TURN_CONTROL_BLOCK_START,
        CHAT_TURN_CONTROL_BLOCK_END,
    );
    let mut fallback_decision = TurnDecision::Yield;
    let mut fallback_reason = None;
    let cleaned_trimmed = cleaned_response.trim().to_string();

    // Backward-compatible marker support while prompts transition.
    if cleaned_trimmed.starts_with(CHAT_CONTINUE_MARKER_LEGACY) {
        let status = cleaned_trimmed[CHAT_CONTINUE_MARKER_LEGACY.len()..].trim();
        fallback_decision = TurnDecision::Continue;
        fallback_reason = Some(if status.is_empty() {
            "Continuing autonomous work...".to_string()
        } else {
            status.to_string()
        });
    }

    let parsed_block = block_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<TurnControlBlock>(raw).ok());

    let decision = parsed_block
        .as_ref()
        .and_then(|b| b.decision.as_deref())
        .map(parse_turn_decision)
        .unwrap_or(fallback_decision);

    let needs_user_input = parsed_block
        .as_ref()
        .and_then(|b| b.needs_user_input)
        .unwrap_or(false);

    let status = parsed_block
        .as_ref()
        .and_then(|b| b.status.as_deref())
        .map(normalize_turn_status)
        .unwrap_or_else(|| {
            if decision == TurnDecision::Continue {
                "still_working".to_string()
            } else if tool_call_count == 0 && needs_user_input {
                "blocked".to_string()
            } else {
                "done".to_string()
            }
        });

    let block_user_message = parsed_block
        .as_ref()
        .and_then(|b| b.user_message.as_deref())
        .map(str::trim)
        .filter(|msg| !msg.is_empty())
        .filter(|msg| !looks_like_hallucinated_user_turn(msg))
        .map(str::to_string);

    let visible_assistant_text = strip_legacy_continue_prefix(&cleaned_trimmed).to_string();
    let operator_response = if !visible_assistant_text.trim().is_empty() {
        visible_assistant_text
    } else {
        block_user_message.unwrap_or_default()
    };

    let reason = parsed_block
        .as_ref()
        .and_then(|b| b.reason.as_deref())
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .or(fallback_reason);

    ParsedTurnControl {
        operator_response,
        decision,
        needs_user_input,
        status,
        reason,
    }
}

fn parse_turn_decision(raw: &str) -> TurnDecision {
    match raw.trim().to_ascii_lowercase().as_str() {
        "continue" => TurnDecision::Continue,
        _ => TurnDecision::Yield,
    }
}

fn normalize_turn_status(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "still_working" | "working" => "still_working".to_string(),
        "blocked" | "needs_input" => "blocked".to_string(),
        _ => "done".to_string(),
    }
}

fn strip_legacy_continue_prefix(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.starts_with(CHAT_CONTINUE_MARKER_LEGACY) {
        trimmed[CHAT_CONTINUE_MARKER_LEGACY.len()..].trim()
    } else {
        trimmed
    }
}

fn strip_inline_thinking_tags(input: &str) -> String {
    let mut output = input.to_string();
    for (start, end) in [("<think>", "</think>"), ("<thinking>", "</thinking>")] {
        while let Some(start_idx) = output.find(start) {
            let Some(relative_end) = output[start_idx + start.len()..].find(end) else {
                output.truncate(start_idx);
                break;
            };
            let end_idx = start_idx + start.len() + relative_end + end.len();
            output.replace_range(start_idx..end_idx, "");
        }
    }
    output
}

fn looks_like_hallucinated_user_turn(message: &str) -> bool {
    let lower = message.trim_start().to_ascii_lowercase();
    let starts_like_user_turn = [
        "user:",
        "operator:",
        "human:",
        "**user**:",
        "**operator**:",
        "you:",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix));

    let embeds_user_turn = [
        "\nuser:",
        "\noperator:",
        "\nhuman:",
        "\n**user**:",
        "\n**operator**:",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    starts_like_user_turn || embeds_user_turn
}

fn extract_metadata_block(
    content: &str,
    start_marker: &str,
    end_marker: &str,
) -> (String, Option<String>) {
    let Some(start_idx) = content.find(start_marker) else {
        return (content.to_string(), None);
    };
    let after_start = start_idx + start_marker.len();
    let remaining = &content[after_start..];
    let Some(relative_end) = remaining.find(end_marker) else {
        return (content.to_string(), None);
    };
    let end_idx = after_start + relative_end;
    let full_end = end_idx + end_marker.len();

    let mut cleaned = String::new();
    cleaned.push_str(content[..start_idx].trim_end());
    if full_end < content.len() {
        let suffix = content[full_end..].trim_start();
        if !suffix.is_empty() {
            if !cleaned.is_empty() {
                cleaned.push('\n');
            }
            cleaned.push_str(suffix);
        }
    }

    let raw = remaining[..relative_end].trim().to_string();
    (cleaned, Some(raw))
}

fn parse_pending_checklist_items(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let pending = trimmed
                .strip_prefix("- [ ] ")
                .or_else(|| trimmed.strip_prefix("* [ ] "));
            pending
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .collect()
}

fn is_heartbeat_memory_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["heartbeat", "reminder", "todo", "task", "check"]
        .iter()
        .any(|needle| key.contains(needle))
}

fn collect_heartbeat_memory_hints(entries: &[WorkingMemoryEntry]) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| is_heartbeat_memory_key(&entry.key) && !entry.content.trim().is_empty())
        .map(|entry| {
            format!(
                "{}: {}",
                entry.key,
                truncate_for_event(entry.content.trim(), 140)
            )
        })
        .collect()
}

fn truncate_for_event(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in input.chars().enumerate() {
        if i >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn infer_media_kind_from_path(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    match ext.as_deref() {
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp") => "image".to_string(),
        Some("wav" | "mp3" | "ogg" | "flac" | "m4a") => "audio".to_string(),
        Some("mp4" | "mov" | "webm" | "mkv" | "avi") => "video".to_string(),
        _ => "file".to_string(),
    }
}

fn ordered_score(v: f64) -> i64 {
    (v * 1_000_000.0) as i64
}

fn agentic_api_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{}/v1", trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unchecked_markdown_checklist_items() {
        let markdown = r#"
# Heartbeat
- [ ] check backups
- [x] already done
* [ ] inspect logs
not a checklist item
"#;

        let items = parse_pending_checklist_items(markdown);
        assert_eq!(items, vec!["check backups", "inspect logs"]);
    }

    #[test]
    fn filters_working_memory_for_heartbeat_like_keys() {
        let entries = vec![
            WorkingMemoryEntry {
                key: "project_todo".to_string(),
                content: "ship heartbeat".to_string(),
                updated_at: Utc::now(),
            },
            WorkingMemoryEntry {
                key: "notes".to_string(),
                content: "general scratchpad".to_string(),
                updated_at: Utc::now(),
            },
            WorkingMemoryEntry {
                key: "reminder_daily".to_string(),
                content: "check disk space".to_string(),
                updated_at: Utc::now(),
            },
        ];

        let hints = collect_heartbeat_memory_hints(&entries);
        assert_eq!(hints.len(), 2);
        assert!(hints.iter().any(|h| h.contains("project_todo")));
        assert!(hints.iter().any(|h| h.contains("reminder_daily")));
    }

    #[test]
    fn normalizes_agentic_api_url() {
        assert_eq!(
            agentic_api_url("http://localhost:11434"),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            agentic_api_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            agentic_api_url("http://localhost:11434/v1/"),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn picks_non_baseline_memory_promotion_candidate() {
        let traces = default_replay_trace_set();
        let report = evaluate_trace_set(
            &traces,
            &[
                EvalBackendKind::KvV1,
                EvalBackendKind::FtsV2,
                EvalBackendKind::EpisodicV3,
            ],
        )
        .unwrap();

        let candidate = select_promotion_candidate_backend(&report, "kv_v1").unwrap();
        assert_ne!(candidate, "kv_v1");
    }

    #[test]
    fn chat_message_format_includes_tool_block_when_tools_exist() {
        let calls = vec![ToolCallRecord {
            tool_name: "shell".to_string(),
            arguments: serde_json::json!({"command": "pwd"}),
            output: ToolOutput::Text("/tmp".to_string()),
        }];

        let formatted = format_chat_message_with_metadata("Done.", &calls, &[]);
        assert!(formatted.contains(CHAT_TOOL_BLOCK_START));
        assert!(formatted.contains(CHAT_TOOL_BLOCK_END));
        assert!(formatted.contains("shell"));
    }

    #[test]
    fn chat_prompt_includes_new_operator_messages() {
        let now = Utc::now();
        let msgs = vec![crate::database::ChatMessage {
            id: "m1".to_string(),
            conversation_id: crate::database::DEFAULT_CHAT_CONVERSATION_ID.to_string(),
            role: "operator".to_string(),
            content: "Please list files".to_string(),
            created_at: now,
            processed: false,
        }];
        let prompt = build_private_chat_agentic_prompt(&msgs, "", "", None, None);
        assert!(prompt.contains("Please list files"));
        assert!(prompt.contains("Use tools"));
    }

    #[test]
    fn chat_prompt_includes_continuation_context_without_fake_operator_turn() {
        let prompt = build_private_chat_agentic_prompt(
            &[],
            "",
            "",
            None,
            Some("Previous autonomous turn: status=still_working, tools=1"),
        );
        assert!(prompt.contains("Autonomous Continuation Context"));
        assert!(!prompt.contains("New Operator Message(s)"));
    }

    #[test]
    fn chat_prompt_includes_summary_snapshot_when_available() {
        let prompt = build_private_chat_agentic_prompt(
            &[],
            "",
            "",
            Some("### Objectives\n- Ship session compaction"),
            None,
        );
        assert!(prompt.contains("Conversation Summary Snapshot"));
        assert!(prompt.contains("Ship session compaction"));
    }

    #[test]
    fn chat_message_format_includes_thinking_block_when_present() {
        let formatted = format_chat_message_with_metadata(
            "Hello!",
            &[],
            &["Private planning text".to_string()],
        );
        assert!(formatted.contains(CHAT_THINKING_BLOCK_START));
        assert!(formatted.contains(CHAT_THINKING_BLOCK_END));
        assert!(formatted.contains("Private planning text"));
    }

    #[test]
    fn chat_message_format_includes_media_block_when_tool_returns_media_json() {
        let calls = vec![ToolCallRecord {
            tool_name: "generate_comfy_media".to_string(),
            arguments: serde_json::json!({"prompt": "hi"}),
            output: ToolOutput::Json(serde_json::json!({
                "media": [
                    {
                        "path": "/tmp/generated_test.png",
                        "media_kind": "image",
                        "mime_type": "image/png"
                    }
                ]
            })),
        }];

        let formatted = format_chat_message_with_metadata("Here you go.", &calls, &[]);
        assert!(formatted.contains(CHAT_MEDIA_BLOCK_START));
        assert!(formatted.contains(CHAT_MEDIA_BLOCK_END));
        assert!(formatted.contains("generated_test.png"));
    }

    #[test]
    fn turn_control_block_is_parsed() {
        let response = "Working...\n[turn_control]\n{\"decision\":\"continue\",\"status\":\"still_working\",\"needs_user_input\":false,\"user_message\":\"Still working...\",\"reason\":\"Need one more tool call\"}\n[/turn_control]";
        let parsed = parse_turn_control(response, 1);
        assert_eq!(parsed.decision, TurnDecision::Continue);
        assert_eq!(parsed.status, "still_working");
        assert!(!parsed.needs_user_input);
        assert_eq!(parsed.operator_response, "Working...");
    }

    #[test]
    fn legacy_continue_marker_remains_supported() {
        let parsed = parse_turn_control("[CONTINUE] still gathering more context", 1);
        assert_eq!(parsed.decision, TurnDecision::Continue);
        assert_eq!(parsed.operator_response, "still gathering more context");
    }

    #[test]
    fn turn_control_defaults_to_yield_without_block() {
        let parsed = parse_turn_control("All done!", 0);
        assert_eq!(parsed.decision, TurnDecision::Yield);
        assert_eq!(parsed.status, "done");
        assert_eq!(parsed.operator_response, "All done!");
    }

    #[test]
    fn turn_control_uses_block_user_message_when_visible_text_missing() {
        let response = "[turn_control]\n{\"decision\":\"yield\",\"status\":\"done\",\"needs_user_input\":false,\"user_message\":\"Completed successfully.\",\"reason\":\"done\"}\n[/turn_control]";
        let parsed = parse_turn_control(response, 0);
        assert_eq!(parsed.operator_response, "Completed successfully.");
    }

    #[test]
    fn turn_control_rejects_hallucinated_user_prefixed_block_message() {
        let response = "[turn_control]\n{\"decision\":\"yield\",\"status\":\"done\",\"needs_user_input\":false,\"user_message\":\"User: please continue with step 2\",\"reason\":\"done\"}\n[/turn_control]";
        let parsed = parse_turn_control(response, 0);
        assert_eq!(parsed.operator_response, "");
    }

    #[test]
    fn strips_inline_thinking_tags_from_summary_text() {
        let raw = "<think>hidden</think>\n### Objectives\n- Keep visible";
        let cleaned = strip_inline_thinking_tags(raw);
        assert!(!cleaned.contains("hidden"));
        assert!(cleaned.contains("### Objectives"));
    }
}
