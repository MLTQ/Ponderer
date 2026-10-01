use eframe::egui;
use flume::Receiver;

use super::affect_lab::AffectLabPanel;
use super::avatar::AvatarSet;
use super::character::CharacterPanel;
use super::settings::{ScheduledJobAction, SettingsPanel};
use super::token_monitor::TokenMonitorState;
use crate::api::{
    AgentVisualState, ApiClient, ChatConversation, ChatMessage, ChatTurnPhase, FrontendEvent,
    OrientationSummary, RuntimeIntentionSummary, UpdateScheduledJobRequest,
    DEFAULT_CHAT_CONVERSATION_ID,
};
use crate::config::AgentConfig;

#[path = "workbench.rs"]
mod workbench;

const MAX_LIVE_TOOL_PROGRESS_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workspace {
    Conversation,
    Mind,
    Identity,
    AffectLab,
    Settings,
}

impl Workspace {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Conversation, "Conversation"),
        (Self::Mind, "Mind / journal"),
        (Self::Identity, "Identity"),
        (Self::AffectLab, "Affect lab"),
        (Self::Settings, "Settings"),
    ];
}

pub struct AgentApp {
    events: Vec<FrontendEvent>,
    event_rx: Receiver<FrontendEvent>,
    api_client: ApiClient,
    current_state: AgentVisualState,
    user_input: String,
    runtime: tokio::runtime::Runtime,
    settings_panel: SettingsPanel,
    affect_lab: AffectLabPanel,
    character_panel: CharacterPanel,
    avatars: Option<AvatarSet>,
    avatars_loaded: bool,
    conversations: Vec<ChatConversation>,
    active_conversation_id: String,
    chat_history: Vec<ChatMessage>,
    chat_media_cache: super::chat::ChatMediaCache,
    live_tool_progress: Vec<LiveToolProgress>,
    streaming_chat_preview: Option<StreamingChatPreview>,
    prompt_inspector: Option<PromptInspectorWindow>,
    last_chat_refresh: std::time::Instant,
    workspace: Workspace,
    show_event_tape: bool,
    /// Tool approval requests waiting for the user's response (tool_name, reason).
    pending_approvals: Vec<(String, String)>,
    /// Latest orientation summary received from the backend.
    last_orientation: Option<OrientationSummary>,
    /// Last action taken by the agent (short label string).
    last_action: Option<String>,
    /// Last journal entry summary.
    last_journal: Option<String>,
    /// Latest live LLM token stream content (any conversation, any cycle).
    live_stream_text: Option<String>,
    /// Rolling live token-novelty monitor rendered in the Mind panel.
    token_monitor: TokenMonitorState,
    /// Timestamp when the current visual state was entered (from AgentRuntimeStatus).
    visual_state_since: Option<chrono::DateTime<chrono::Utc>>,
    /// Short description of what the agent is currently doing (from AgentRuntimeStatus).
    current_activity: Option<String>,
    /// Whether dedicated-machine Loose autonomy is deliberately armed.
    loose_mode: bool,
    /// Current or next durable intention exposed by backend runtime status.
    current_intention: Option<RuntimeIntentionSummary>,
    show_loose_arm_confirmation: bool,
    /// Conversation pending delete confirmation (id).
    confirm_delete_conversation_id: Option<String>,
    /// Conversation pending rename: (id, draft_title).
    rename_conversation: Option<(String, String)>,
    /// Full text to show in the Mind event detail pop-out window.
    event_detail_popup: Option<String>,
    /// Only the opt-in snapshot harness constructs this isolated mode; no backend is launched.
    ui_snapshot: bool,
}

struct StreamingChatPreview {
    conversation_id: String,
    content: String,
}

#[derive(Clone)]
struct LiveToolProgress {
    conversation_id: String,
    tool_name: String,
    output_preview: String,
    subtask_id: Option<String>,
}

struct PromptInspectorWindow {
    open: bool,
    turn_id: String,
    prompt_text: String,
    system_prompt_text: String,
    show_system_prompt: bool,
    highlight_sections: bool,
    error: Option<String>,
}

impl AgentApp {
    pub fn new(api_client: ApiClient, fallback_config: AgentConfig) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("UI tokio runtime");
        let (event_tx, event_rx) = flume::unbounded();

        let event_client = api_client.clone();
        runtime.spawn(async move {
            event_client.stream_events_forever(event_tx).await;
        });

        let startup_config = match runtime.block_on(api_client.get_config()) {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    "Failed to load config from backend ({}); using local fallback",
                    error
                );
                fallback_config
            }
        };

        let plugin_manifests = match runtime.block_on(api_client.list_plugins()) {
            Ok(manifests) => manifests,
            Err(error) => {
                tracing::warn!(
                    "Failed to load plugin manifests from backend ({}); settings tabs will use core defaults only",
                    error
                );
                Vec::new()
            }
        };

        let mut app = Self::from_startup(
            api_client,
            startup_config,
            runtime,
            event_rx,
            plugin_manifests,
        );

        app.refresh_status();
        app.refresh_conversations();
        app.refresh_chat_history();
        app.refresh_scheduled_jobs();
        app
    }

    fn from_startup(
        api_client: ApiClient,
        startup_config: AgentConfig,
        runtime: tokio::runtime::Runtime,
        event_rx: Receiver<FrontendEvent>,
        plugin_manifests: Vec<crate::api::PluginManifest>,
    ) -> Self {
        let mut settings_panel = SettingsPanel::new(startup_config.clone());
        settings_panel.set_plugin_manifests(plugin_manifests);

        Self {
            events: Vec::new(),
            event_rx,
            api_client,
            current_state: AgentVisualState::Idle,
            user_input: String::new(),
            runtime,
            settings_panel,
            affect_lab: AffectLabPanel::new(),
            character_panel: CharacterPanel::new(startup_config),
            avatars: None,
            avatars_loaded: false,
            conversations: Vec::new(),
            active_conversation_id: DEFAULT_CHAT_CONVERSATION_ID.to_string(),
            chat_history: Vec::new(),
            chat_media_cache: super::chat::ChatMediaCache::new(),
            live_tool_progress: Vec::new(),
            streaming_chat_preview: None,
            prompt_inspector: None,
            last_chat_refresh: std::time::Instant::now(),
            workspace: Workspace::Conversation,
            show_event_tape: true,
            pending_approvals: Vec::new(),
            last_orientation: None,
            last_action: None,
            last_journal: None,
            live_stream_text: None,
            token_monitor: TokenMonitorState::new(),
            visual_state_since: None,
            current_activity: None,
            loose_mode: false,
            current_intention: None,
            show_loose_arm_confirmation: false,
            confirm_delete_conversation_id: None,
            rename_conversation: None,
            event_detail_popup: None,
            ui_snapshot: false,
        }
    }

    fn push_ui_error(&mut self, message: impl Into<String>) {
        self.events.push(FrontendEvent::Error(message.into()));
    }

    #[cfg(any(test, feature = "ui-snapshot"))]
    pub fn isolated_snapshot(config: AgentConfig, workspace: &str) -> Self {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (_, event_rx) = flume::unbounded();
        let client = ApiClient::new_local("http://127.0.0.1:1".into(), None);
        let mut app = Self::from_startup(client, config, runtime, event_rx, Vec::new());
        app.ui_snapshot = true;
        app.workspace = match workspace {
            "mind" => Workspace::Mind,
            "identity" => Workspace::Identity,
            "lab" => Workspace::AffectLab,
            "settings" | "appearance" => Workspace::Settings,
            _ => Workspace::Conversation,
        };
        if workspace == "appearance" {
            app.settings_panel.open_tab("core.appearance");
        }
        if workspace == "lab" {
            app.affect_lab.load_snapshot_fixture();
        }
        app.chat_history = [
            ("operator", "Keep the sphere visible. I want this to feel like a technical workbench."),
            ("assistant", "The trace is beside the conversation. Intentions, journal entries and raw events stay inspectable. This is synthetic state for an isolated UI snapshot, not a live model response."),
        ].into_iter().enumerate().map(|(index,(role,content))|ChatMessage {
            id:format!("fixture-{index}"), conversation_id:DEFAULT_CHAT_CONVERSATION_ID.into(),
            role:role.into(), content:content.into(), created_at:chrono::Utc::now(),
            processed:true, turn_id:None,
        }).collect();
        app.last_journal = Some("Synthetic fixture: the expected steering effect was not established in held-out outputs.".into());
        app.pending_approvals = vec![("fixture / local tool".into(), "Synthetic approval for checking visibility across every workspace. No tool or outreach will execute.".into())];
        app.events = vec![FrontendEvent::Observation(
            "Isolated native UI fixture / no backend or model launched.".into(),
        )];
        let text = "A deterministic token novelty trace is not a projection of hidden activations . The live scope keeps the source and samples visible .";
        let samples: Vec<_> = text
            .split_whitespace()
            .enumerate()
            .map(|(index, text)| crate::api::TokenMetricSample {
                text: text.into(),
                novelty: 0.25 + (index % 9) as f32 * 0.08,
                logprob: None,
                entropy: None,
            })
            .collect();
        app.token_monitor
            .ingest_generation("fixture-trace", "synthetic snapshot", None, &samples);
        app.token_monitor.generation_finished(
            "fixture-trace",
            "synthetic snapshot",
            None,
            "complete",
        );
        app
    }

    #[cfg(any(test, feature = "ui-snapshot"))]
    pub fn render_snapshot(&mut self, ctx: &egui::Context) {
        super::theme::apply(ctx, &self.settings_panel.config.appearance);
        self.render_workbench(ctx);
    }

    fn refresh_status(&mut self) {
        match self.runtime.block_on(self.api_client.get_agent_status()) {
            Ok(status) => {
                self.current_state = status.visual_state;
                self.visual_state_since = status.visual_state_since;
                self.current_activity = status.current_activity;
                self.loose_mode = status.loose_mode;
                self.current_intention = status.current_intention;
            }
            Err(error) => {
                tracing::warn!("Failed to refresh backend status: {}", error);
            }
        }
    }

    fn refresh_conversations(&mut self) {
        match self
            .runtime
            .block_on(self.api_client.list_conversations(100))
        {
            Ok(conversations) => {
                self.conversations = conversations;
                if self
                    .conversations
                    .iter()
                    .all(|c| c.id != self.active_conversation_id)
                {
                    self.active_conversation_id = self
                        .conversations
                        .first()
                        .map(|c| c.id.clone())
                        .unwrap_or_else(|| DEFAULT_CHAT_CONVERSATION_ID.to_string());
                }
            }
            Err(error) => {
                tracing::warn!("Failed to refresh chat conversations: {}", error);
                self.push_ui_error(format!("Failed to load conversations: {}", error));
            }
        }
    }

    fn refresh_chat_history(&mut self) {
        let conversation_id = self.active_conversation_id.clone();
        match self
            .runtime
            .block_on(self.api_client.list_messages(&conversation_id, 200))
        {
            Ok(history) => {
                self.chat_history = history;
            }
            Err(error) => {
                tracing::warn!(
                    "Failed to refresh chat history for {}: {}",
                    conversation_id,
                    error
                );
                self.push_ui_error(format!("Failed to load chat history: {}", error));
            }
        }
    }

    fn refresh_scheduled_jobs(&mut self) {
        match self
            .runtime
            .block_on(self.api_client.list_scheduled_jobs(200))
        {
            Ok(jobs) => {
                self.settings_panel.set_scheduled_jobs(jobs);
                self.settings_panel.set_scheduled_jobs_error(None);
            }
            Err(error) => {
                tracing::warn!("Failed to refresh scheduled jobs: {}", error);
                self.settings_panel
                    .set_scheduled_jobs_error(Some(format!("Failed to load schedules: {}", error)));
            }
        }
    }

    fn apply_scheduled_job_actions(&mut self, actions: Vec<ScheduledJobAction>) {
        let mut should_refresh = false;

        for action in actions {
            match action {
                ScheduledJobAction::Refresh => {
                    should_refresh = true;
                }
                ScheduledJobAction::Create {
                    name,
                    prompt,
                    interval_minutes,
                    enabled,
                } => match self.runtime.block_on(self.api_client.create_scheduled_job(
                    &name,
                    &prompt,
                    interval_minutes,
                )) {
                    Ok(job) => {
                        if !enabled {
                            let request = UpdateScheduledJobRequest {
                                enabled: Some(false),
                                ..Default::default()
                            };
                            if let Err(error) = self
                                .runtime
                                .block_on(self.api_client.update_scheduled_job(&job.id, &request))
                            {
                                self.settings_panel.set_scheduled_jobs_error(Some(format!(
                                    "Created schedule '{}' but failed to disable it: {}",
                                    name, error
                                )));
                                self.push_ui_error(format!(
                                    "Created schedule '{}' but failed to disable it: {}",
                                    name, error
                                ));
                            }
                        }
                        should_refresh = true;
                    }
                    Err(error) => {
                        self.settings_panel.set_scheduled_jobs_error(Some(format!(
                            "Failed to create schedule '{}': {}",
                            name, error
                        )));
                        self.push_ui_error(format!(
                            "Failed to create schedule '{}': {}",
                            name, error
                        ));
                    }
                },
                ScheduledJobAction::Update {
                    job_id,
                    name,
                    prompt,
                    interval_minutes,
                    enabled,
                } => {
                    let request = UpdateScheduledJobRequest {
                        name: Some(name.clone()),
                        prompt: Some(prompt),
                        interval_minutes: Some(interval_minutes),
                        enabled: Some(enabled),
                    };
                    match self
                        .runtime
                        .block_on(self.api_client.update_scheduled_job(&job_id, &request))
                    {
                        Ok(_) => {
                            should_refresh = true;
                        }
                        Err(error) => {
                            self.settings_panel.set_scheduled_jobs_error(Some(format!(
                                "Failed to update schedule '{}': {}",
                                name, error
                            )));
                            self.push_ui_error(format!(
                                "Failed to update schedule '{}': {}",
                                name, error
                            ));
                        }
                    }
                }
                ScheduledJobAction::Delete { job_id } => {
                    match self
                        .runtime
                        .block_on(self.api_client.delete_scheduled_job(&job_id))
                    {
                        Ok(()) => {
                            should_refresh = true;
                        }
                        Err(error) => {
                            self.settings_panel.set_scheduled_jobs_error(Some(format!(
                                "Failed to delete schedule '{}': {}",
                                job_id, error
                            )));
                            self.push_ui_error(format!(
                                "Failed to delete schedule '{}': {}",
                                job_id, error
                            ));
                        }
                    }
                }
            }
        }

        if should_refresh {
            self.refresh_scheduled_jobs();
        }
    }

    fn send_chat_message(&mut self, content: &str) {
        let active_conversation = self.active_conversation_id.clone();
        self.clear_live_tool_progress(&active_conversation);

        match self
            .runtime
            .block_on(self.api_client.send_message(&active_conversation, content))
        {
            Ok(_message_id) => {
                tracing::info!("Sent chat message to backend: {}", content);
                self.token_monitor.on_human_interaction();
                self.refresh_conversations();
                self.refresh_chat_history();
            }
            Err(error) => {
                tracing::error!("Failed to send chat message: {}", error);
                self.push_ui_error(format!("Failed to send message: {}", error));
            }
        }
    }

    fn open_prompt_inspector_for_turn(&mut self, turn_id: &str) {
        match self
            .runtime
            .block_on(self.api_client.get_turn_prompt(turn_id))
        {
            Ok(prompt) => {
                self.prompt_inspector = Some(PromptInspectorWindow {
                    open: true,
                    turn_id: prompt.turn_id,
                    prompt_text: prompt.prompt_text,
                    system_prompt_text: prompt.system_prompt_text.unwrap_or_default(),
                    show_system_prompt: false,
                    highlight_sections: false,
                    error: None,
                });
            }
            Err(error) => {
                tracing::warn!("Failed to fetch turn prompt {}: {}", turn_id, error);
                self.prompt_inspector = Some(PromptInspectorWindow {
                    open: true,
                    turn_id: turn_id.to_string(),
                    prompt_text: String::new(),
                    system_prompt_text: String::new(),
                    show_system_prompt: false,
                    highlight_sections: false,
                    error: Some(error.to_string()),
                });
                self.push_ui_error(format!("Failed to load turn prompt: {}", error));
            }
        }
    }

    fn create_new_conversation(&mut self) {
        match self
            .runtime
            .block_on(self.api_client.create_conversation(None))
        {
            Ok(conversation) => {
                self.active_conversation_id = conversation.id;
                self.user_input.clear();
                self.streaming_chat_preview = None;
                self.refresh_conversations();
                self.refresh_chat_history();
            }
            Err(error) => {
                tracing::error!("Failed to create conversation: {}", error);
                self.push_ui_error(format!("Failed to create conversation: {}", error));
            }
        }
    }

    fn delete_conversation(&mut self, conversation_id: &str) {
        match self
            .runtime
            .block_on(self.api_client.delete_conversation(conversation_id))
        {
            Ok(()) => {
                // If we deleted the active conversation, switch to a different one.
                if self.active_conversation_id == conversation_id {
                    self.streaming_chat_preview = None;
                    self.live_tool_progress
                        .retain(|e| e.conversation_id != conversation_id);
                    self.refresh_conversations();
                    // Pick the first remaining conversation, or create a new one.
                    if let Some(first) = self.conversations.first() {
                        self.active_conversation_id = first.id.clone();
                    } else {
                        self.create_new_conversation();
                        return;
                    }
                    self.refresh_chat_history();
                } else {
                    self.refresh_conversations();
                }
            }
            Err(error) => {
                tracing::error!("Failed to delete conversation: {}", error);
                self.push_ui_error(format!("Failed to delete conversation: {}", error));
            }
        }
    }

    fn rename_conversation(&mut self, conversation_id: &str, title: &str) {
        match self.runtime.block_on(
            self.api_client
                .update_conversation_title(conversation_id, title),
        ) {
            Ok(_) => {
                self.refresh_conversations();
            }
            Err(error) => {
                tracing::error!("Failed to rename conversation: {}", error);
                self.push_ui_error(format!("Failed to rename conversation: {}", error));
            }
        }
    }

    fn persist_config(&mut self, config: AgentConfig) {
        if self.affect_lab.provider_change_pending() {
            self.settings_panel.discard_queued_schedule_changes();
            self.settings_panel.save_error =
                Some("Wait for the model provider change to finish before saving settings.".into());
            self.push_ui_error(
                "Wait for the model provider change to finish before saving settings.",
            );
            return;
        }
        match self
            .runtime
            .block_on(self.api_client.update_config(&config))
        {
            Ok(saved) => {
                self.settings_panel.sync_from_config(saved.clone());
                self.character_panel.config = saved.clone();
                self.avatars = None;
                self.avatars_loaded = false;
                tracing::info!("Config saved through backend API");
            }
            Err(error) => {
                self.settings_panel.discard_queued_schedule_changes();
                tracing::error!("Failed to persist config via backend API: {}", error);
                self.settings_panel.save_error = Some(format!("Save failed: {error}"));
                self.push_ui_error(format!("Failed to save settings: {}", error));
            }
        }
    }

    fn push_live_tool_progress(&mut self, conversation_id: &str, tool_name: &str, output: &str) {
        self.live_tool_progress.push(LiveToolProgress {
            conversation_id: conversation_id.to_string(),
            tool_name: tool_name.to_string(),
            output_preview: output.to_string(),
            subtask_id: parse_subtask_id(output),
        });
        if self.live_tool_progress.len() > MAX_LIVE_TOOL_PROGRESS_LINES {
            let overflow = self.live_tool_progress.len() - MAX_LIVE_TOOL_PROGRESS_LINES;
            self.live_tool_progress.drain(0..overflow);
        }
    }

    fn clear_live_tool_progress(&mut self, conversation_id: &str) {
        self.live_tool_progress
            .retain(|entry| entry.conversation_id != conversation_id);
    }

    fn load_avatars(&mut self, ctx: &egui::Context, config: &AgentConfig) {
        let idle = config.avatar_idle.as_deref();
        let thinking = config.avatar_thinking.as_deref();
        let active = config.avatar_active.as_deref();

        let avatars = AvatarSet::load(ctx, idle, thinking, active);

        if avatars.has_avatars() {
            tracing::info!("Loaded avatars successfully");
            self.avatars = Some(avatars);
        } else {
            tracing::info!("No avatars configured, using emoji fallback");
            self.avatars = None;
        }
    }
}

fn conversation_display_label(conversation: &ChatConversation) -> String {
    let base = if conversation.message_count == 0 {
        conversation.title.clone()
    } else {
        format!("{} ({})", conversation.title, conversation.message_count)
    };

    let status_suffix = match conversation.runtime_state {
        ChatTurnPhase::Idle => "",
        ChatTurnPhase::Processing => " · processing",
        ChatTurnPhase::Completed => " · done",
        ChatTurnPhase::AwaitingApproval => " · awaiting input",
        ChatTurnPhase::Failed => " · failed",
    };

    if status_suffix.is_empty() {
        base
    } else {
        format!("{}{}", base, status_suffix)
    }
}

impl eframe::App for AgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        super::theme::apply(ctx, &self.settings_panel.config.appearance);
        if !self.avatars_loaded {
            let config = self.settings_panel.config.clone();
            self.load_avatars(ctx, &config);
            self.avatars_loaded = true;
        }

        if self.last_chat_refresh.elapsed() > std::time::Duration::from_secs(2) {
            self.refresh_status();
            self.refresh_conversations();
            self.refresh_chat_history();
            self.last_chat_refresh = std::time::Instant::now();
        }

        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                FrontendEvent::StateChanged(state) => {
                    self.current_state = state.clone();
                }
                FrontendEvent::ChatStreaming {
                    conversation_id,
                    content,
                    done,
                } => {
                    // Capture global live stream regardless of which conversation is active.
                    if *done {
                        self.live_stream_text = None;
                        // Revert Writing back to Thinking so the backend StateChanged that
                        // follows can take over normally.
                        if matches!(self.current_state, AgentVisualState::Writing) {
                            self.current_state = AgentVisualState::Thinking;
                        }
                    } else if !content.trim().is_empty() {
                        self.live_stream_text = Some(content.clone());
                        // Show Writing while tokens are actively streaming to the user.
                        if matches!(self.current_state, AgentVisualState::Thinking) {
                            self.current_state = AgentVisualState::Writing;
                        }
                    }
                    // Per-conversation streaming preview for the chat pane.
                    if *done && content.trim().is_empty() {
                        if self
                            .streaming_chat_preview
                            .as_ref()
                            .is_some_and(|preview| preview.conversation_id == *conversation_id)
                        {
                            self.streaming_chat_preview = None;
                        }
                    } else {
                        self.streaming_chat_preview = Some(StreamingChatPreview {
                            conversation_id: conversation_id.clone(),
                            content: content.clone(),
                        });
                    }
                    continue;
                }
                FrontendEvent::GenerationStarted {
                    generation_id,
                    source,
                    conversation_id,
                } => {
                    self.token_monitor.generation_started(
                        generation_id,
                        source,
                        conversation_id.as_deref(),
                    );
                    continue;
                }
                FrontendEvent::GenerationMetrics {
                    generation_id,
                    source,
                    conversation_id,
                    samples,
                } => {
                    self.token_monitor.ingest_generation(
                        generation_id,
                        source,
                        conversation_id.as_deref(),
                        samples,
                    );
                    continue;
                }
                FrontendEvent::GenerationFinished {
                    generation_id,
                    source,
                    conversation_id,
                    outcome,
                } => {
                    self.token_monitor.generation_finished(
                        generation_id,
                        source,
                        conversation_id.as_deref(),
                        outcome,
                    );
                    continue;
                }
                FrontendEvent::ToolCallProgress {
                    conversation_id,
                    tool_name,
                    output_preview,
                } => {
                    self.push_live_tool_progress(conversation_id, tool_name, output_preview);
                }
                FrontendEvent::ActionTaken { action, .. } => {
                    self.last_action = Some(action.clone());
                    if action.contains("operator") {
                        self.refresh_conversations();
                        self.refresh_chat_history();
                        self.streaming_chat_preview = None;
                    }
                }
                FrontendEvent::OrientationUpdate(summary) => {
                    self.last_orientation = Some(summary.clone());
                }
                FrontendEvent::JournalWritten(summary) => {
                    self.last_journal = Some(summary.clone());
                }
                FrontendEvent::ApprovalRequest { tool_name, reason } => {
                    // Deduplicate: only add if not already pending
                    if !self.pending_approvals.iter().any(|(t, _)| t == tool_name) {
                        self.pending_approvals
                            .push((tool_name.clone(), reason.clone()));
                    }
                    // Don't push ApprovalRequest into the activity log — it gets its own popup
                    continue;
                }
                _ => {}
            }
            self.events.push(event);
        }

        if let Some(config) =
            self.affect_lab
                .tick(&self.api_client, &self.runtime, self.settings_panel.show)
        {
            self.settings_panel.sync_provider_from_config(&config);
            self.character_panel.config = self.settings_panel.config.clone();
        }
        let (approve_tool, dismiss_tool) = self.render_workbench(ctx);

        if self.show_loose_arm_confirmation {
            let mut arm = false;
            let mut cancel = false;
            egui::Window::new("Arm Loose Mode")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_max_width(430.0);
                    ui.label("Ponderer will choose durable goals and pursue them across bounded episodes.");
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "Local shell, filesystem, package, and process actions may run without routine approval. External publication and identity/secrets effects keep their host gates.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("Arm and begin")
                                    .color(super::theme::palette(ui).warning)
                                    .strong(),
                            )
                            .clicked()
                        {
                            arm = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            if arm {
                match self.runtime.block_on(self.api_client.set_loose_mode(true)) {
                    Ok(enabled) => {
                        self.loose_mode = enabled;
                        self.settings_panel.sync_loose_action(enabled);
                        self.settings_panel.config.enable_ambient_loop = true;
                        self.current_state = AgentVisualState::Idle;
                    }
                    Err(error) => {
                        self.push_ui_error(format!("Failed to arm Loose mode: {}", error));
                    }
                }
                self.show_loose_arm_confirmation = false;
            } else if cancel {
                self.show_loose_arm_confirmation = false;
            }
        }

        // Mind event detail pop-out window.
        if let Some(ref text) = self.event_detail_popup.clone() {
            let mut open = true;
            egui::Window::new("Event Detail")
                .collapsible(false)
                .resizable(true)
                .default_size([560.0, 420.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        ui.add_space(4.0);
                        if ui.button("Close").clicked() {
                            self.event_detail_popup = None;
                        }
                        ui.add_space(4.0);
                        ui.separator();
                        // Fill the remaining space (above the Close button) with a
                        // scrollable, selectable, read-only text editor.  TextEdit
                        // handles arbitrarily large text without truncation and lets
                        // the user select/copy the content.
                        let available = ui.available_size();
                        let mut buf = text.clone();
                        egui::ScrollArea::vertical()
                            .id_salt("event_detail_scroll")
                            .show(ui, |ui| {
                                ui.add_sized(
                                    available,
                                    egui::TextEdit::multiline(&mut buf)
                                        .font(egui::TextStyle::Monospace)
                                        .interactive(false)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    });
                });
            if !open {
                self.event_detail_popup = None;
            }
        }

        // Rename-conversation dialog.
        if self.rename_conversation.is_some() {
            let mut open = true;
            let mut confirmed = false;
            let mut cancelled = false;
            egui::Window::new("Rename Conversation")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    if let Some((_, ref mut draft)) = self.rename_conversation {
                        let response = ui.add(
                            egui::TextEdit::singleline(draft)
                                .desired_width(280.0)
                                .hint_text("Conversation title"),
                        );
                        response.request_focus();
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Rename").clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                confirmed = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    }
                });
            if confirmed {
                if let Some((id, draft)) = self.rename_conversation.take() {
                    let title = draft.trim().to_string();
                    if !title.is_empty() {
                        self.rename_conversation(&id, &title);
                    }
                }
            } else if cancelled || !open {
                self.rename_conversation = None;
            }
        }

        // Delete-conversation confirmation dialog.
        if let Some(conv_id) = self.confirm_delete_conversation_id.clone() {
            let title_label = self
                .conversations
                .iter()
                .find(|c| c.id == conv_id)
                .map(|c| c.title.clone())
                .unwrap_or_else(|| conv_id.chars().take(12).collect());
            let mut open = true;
            egui::Window::new("Delete Conversation?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(format!("Delete \"{}\"?", title_label));
                    ui.label(egui::RichText::new("This cannot be undone.").small().weak());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("Delete")
                                    .color(super::theme::palette(ui).error),
                            )
                            .clicked()
                        {
                            let id_to_delete = conv_id.clone();
                            self.confirm_delete_conversation_id = None;
                            self.delete_conversation(&id_to_delete);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_conversation_id = None;
                        }
                    });
                });
            if !open {
                self.confirm_delete_conversation_id = None;
            }
        }

        if let Some(inspector) = self.prompt_inspector.as_mut() {
            let mut open = inspector.open;
            egui::Window::new(format!(
                "Turn Prompt · {}",
                inspector.turn_id.chars().take(12).collect::<String>()
            ))
            .open(&mut open)
            .resizable(true)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("Turn ID: {}", inspector.turn_id))
                        .small()
                        .weak(),
                );
                ui.add_space(6.0);
                if let Some(error) = inspector.error.as_deref() {
                    ui.colored_label(super::theme::palette(ui).error, error);
                } else if inspector.prompt_text.trim().is_empty() {
                    ui.label(
                        egui::RichText::new("No stored prompt text for this turn.")
                            .small()
                            .italics()
                            .weak(),
                    );
                } else {
                    ui.horizontal(|ui| {
                        let system_label = if inspector.show_system_prompt {
                            "Hide System Prompt"
                        } else {
                            "Show System Prompt"
                        };
                        if ui.button(system_label).clicked() {
                            inspector.show_system_prompt = !inspector.show_system_prompt;
                        }
                        let highlight_label = if inspector.highlight_sections {
                            "Hide Highlights"
                        } else {
                            "Highlight Sections"
                        };
                        if ui.button(highlight_label).clicked() {
                            inspector.highlight_sections = !inspector.highlight_sections;
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Context Prompt").strong());
                    if inspector.highlight_sections {
                        render_highlighted_prompt_sections(ui, &inspector.prompt_text);
                    } else {
                        let mut prompt_text = inspector.prompt_text.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut prompt_text)
                                .desired_rows(22)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace)
                                .interactive(false),
                        );
                    }

                    if inspector.show_system_prompt {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("System Prompt").strong());
                        if inspector.system_prompt_text.trim().is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "No stored system prompt for this turn (older turn or migration gap).",
                                )
                                .small()
                                .italics()
                                .weak(),
                            );
                        } else if inspector.highlight_sections {
                            render_highlighted_prompt_sections(ui, &inspector.system_prompt_text);
                        } else {
                            let mut system_prompt = inspector.system_prompt_text.clone();
                            ui.add(
                                egui::TextEdit::multiline(&mut system_prompt)
                                    .desired_rows(16)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace)
                                    .interactive(false),
                            );
                        }
                    }
                }
            });
            inspector.open = open;
            if !inspector.open {
                self.prompt_inspector = None;
            }
        }

        let scheduled_job_actions = self.settings_panel.take_scheduled_job_actions();
        if !scheduled_job_actions.is_empty() {
            self.apply_scheduled_job_actions(scheduled_job_actions);
        }

        if let Some(ref tool) = approve_tool {
            match self.runtime.block_on(self.api_client.approve_tool(tool)) {
                Ok(()) => {
                    tracing::info!("Session approval granted for: {}", tool);
                    self.pending_approvals.retain(|(t, _)| t != tool);
                }
                Err(e) => self.push_ui_error(format!("Failed to approve tool: {}", e)),
            }
        }
        if let Some(ref tool) = dismiss_tool {
            self.pending_approvals.retain(|(t, _)| t != tool);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn render_live_tool_entry(ui: &mut egui::Ui, entry: &LiveToolProgress) {
    let color = super::theme::palette(ui).accent;
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(&entry.tool_name)
                .color(color)
                .strong()
                .small(),
        );
        if let Some(ref subtask_id) = entry.subtask_id {
            ui.label(
                egui::RichText::new(format!("[{}]", subtask_id))
                    .weak()
                    .small(),
            );
        }
        let output = truncate_str(&entry.output_preview, 200);
        let wrapped_output = wrap_text_for_ui_width(&output, ui.available_width());
        ui.add(
            egui::Label::new(
                egui::RichText::new(wrapped_output)
                    .small()
                    .color(super::theme::palette(ui).muted),
            )
            .wrap(),
        );
    });
    ui.add_space(2.0);
}

/// Format elapsed seconds as a compact human-readable duration (e.g. "4m 23s", "1h 2m").
fn format_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn truncate_str(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max_chars + 4));
    for (i, ch) in text.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn max_token_len_for_width(width: f32) -> usize {
    ((width / 7.5).floor() as usize).clamp(20, 140)
}

fn wrap_text_for_ui_width(input: &str, width: f32) -> String {
    let max_token_len = max_token_len_for_width(width.max(120.0));
    let mut out = String::with_capacity(input.len());
    let mut run_len = 0usize;

    for ch in input.chars() {
        if ch.is_whitespace() {
            run_len = 0;
            out.push(ch);
            continue;
        }

        if run_len >= max_token_len {
            out.push('\n');
            run_len = 0;
        }

        out.push(ch);
        run_len += 1;
    }

    out
}

fn last_n_chars(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= n {
        text.to_string()
    } else {
        chars[chars.len() - n..].iter().collect()
    }
}

fn parse_subtask_id(output: &str) -> Option<String> {
    let trimmed = output.trim_start();
    let body = trimmed.strip_prefix('[')?;
    let end = body.find(']')?;
    let id = body[..end].trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

#[derive(Debug, Clone)]
struct PromptSection {
    title: String,
    source: &'static str,
    body: String,
}

fn render_highlighted_prompt_sections(ui: &mut egui::Ui, prompt: &str) {
    let sections = split_prompt_sections(prompt);
    if sections.is_empty() {
        let mut raw = prompt.to_string();
        ui.add(
            egui::TextEdit::multiline(&mut raw)
                .desired_rows(10)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace)
                .interactive(false),
        );
        return;
    }

    for section in sections {
        let colors = super::theme::palette(ui);
        let frame = egui::Frame::group(ui.style())
            .fill(colors.soft)
            .stroke(egui::Stroke::new(1.0_f32, colors.border));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&section.title).strong());
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("source: {}", section.source))
                        .small()
                        .color(colors.accent),
                );
            });
            ui.add_space(3.0);
            let mut body = section.body.clone();
            let rows = body.lines().count().clamp(2, 14);
            ui.add(
                egui::TextEdit::multiline(&mut body)
                    .desired_rows(rows)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace)
                    .interactive(false),
            );
        });
        ui.add_space(6.0);
    }
}

fn split_prompt_sections(prompt: &str) -> Vec<PromptSection> {
    let normalized = prompt.replace("\r\n", "\n");
    let mut sections = Vec::new();

    for chunk in normalized.split("\n\n---\n\n") {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        let title = trimmed
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .unwrap_or("Context");
        let source = classify_prompt_section(title, trimmed);
        sections.push(PromptSection {
            title: title.to_string(),
            source,
            body: trimmed.to_string(),
        });
    }

    if sections.is_empty() && !normalized.trim().is_empty() {
        let trimmed = normalized.trim();
        let source = classify_prompt_section("Prompt", trimmed);
        sections.push(PromptSection {
            title: "Prompt".to_string(),
            source,
            body: trimmed.to_string(),
        });
    }

    sections
}

fn classify_prompt_section(title: &str, body: &str) -> &'static str {
    let title_l = title.to_ascii_lowercase();
    let body_l = body.to_ascii_lowercase();

    if title_l.contains("concern priority context") || body_l.contains("concern priority context") {
        return "Concerns manager (DB)";
    }
    if title_l.contains("working memory") || body_l.contains("working memory") {
        return "Working memory (DB)";
    }
    if title_l.contains("recent conversation context") || body_l.contains("recent private chat") {
        return "Recent chat history";
    }
    if title_l.contains("conversation summary snapshot") {
        return "Compaction summary";
    }
    if title_l.contains("recent action digest") {
        return "Action digest (chat turns)";
    }
    if title_l.contains("previous ooda packet") {
        return "Previous OODA packet";
    }
    if title_l.contains("ooda context") {
        return "Orientation + decision context";
    }
    if title_l.contains("new operator message") {
        return "Operator input";
    }
    if title_l.contains("autonomous continuation context") {
        return "Continuation hint";
    }

    "Additional context"
}

#[cfg(test)]
mod tests {
    use super::parse_subtask_id;

    #[test]
    fn extracts_subtask_id_from_bracket_prefix() {
        let parsed = parse_subtask_id("[abc123] turn 2/8 running");
        assert_eq!(parsed.as_deref(), Some("abc123"));
    }

    #[test]
    fn ignores_non_prefixed_lines() {
        assert!(parse_subtask_id("shell -> output").is_none());
    }
}
