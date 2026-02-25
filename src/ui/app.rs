use eframe::egui;
use flume::Receiver;

use super::avatar::AvatarSet;
use super::character::CharacterPanel;
use super::comfy_settings::ComfySettingsPanel;
use super::settings::SettingsPanel;
use crate::api::{
    AgentVisualState, ApiClient, ChatConversation, ChatMessage, ChatTurnPhase, FrontendEvent,
    OrientationSummary, DEFAULT_CHAT_CONVERSATION_ID,
};
use crate::config::AgentConfig;

const MAX_LIVE_TOOL_PROGRESS_LINES: usize = 200;

pub struct AgentApp {
    events: Vec<FrontendEvent>,
    event_rx: Receiver<FrontendEvent>,
    api_client: ApiClient,
    current_state: AgentVisualState,
    user_input: String,
    runtime: tokio::runtime::Runtime,
    settings_panel: SettingsPanel,
    character_panel: CharacterPanel,
    comfy_settings_panel: ComfySettingsPanel,
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
    show_activity_panel: bool,
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
    /// Conversation pending delete confirmation (id).
    confirm_delete_conversation_id: Option<String>,
    /// Full text to show in the Mind event detail pop-out window.
    event_detail_popup: Option<String>,
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

        let mut comfy_settings_panel = ComfySettingsPanel::new();
        comfy_settings_panel.load_workflow_from_config(&startup_config);

        let mut app = Self {
            events: Vec::new(),
            event_rx,
            api_client,
            current_state: AgentVisualState::Idle,
            user_input: String::new(),
            runtime,
            settings_panel: SettingsPanel::new(startup_config.clone()),
            character_panel: CharacterPanel::new(startup_config),
            comfy_settings_panel,
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
            show_activity_panel: true,
            pending_approvals: Vec::new(),
            last_orientation: None,
            last_action: None,
            last_journal: None,
            live_stream_text: None,
            confirm_delete_conversation_id: None,
            event_detail_popup: None,
        };

        app.refresh_status();
        app.refresh_conversations();
        app.refresh_chat_history();
        app
    }

    fn push_ui_error(&mut self, message: impl Into<String>) {
        self.events.push(FrontendEvent::Error(message.into()));
    }

    fn refresh_status(&mut self) {
        match self.runtime.block_on(self.api_client.get_agent_status()) {
            Ok(status) => {
                self.current_state = status.visual_state;
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

    fn send_chat_message(&mut self, content: &str) {
        let active_conversation = self.active_conversation_id.clone();
        self.clear_live_tool_progress(&active_conversation);

        match self
            .runtime
            .block_on(self.api_client.send_message(&active_conversation, content))
        {
            Ok(_message_id) => {
                tracing::info!("Sent chat message to backend: {}", content);
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

    fn persist_config(&mut self, config: AgentConfig) {
        match self
            .runtime
            .block_on(self.api_client.update_config(&config))
        {
            Ok(saved) => {
                self.settings_panel.config = saved.clone();
                self.character_panel.config = saved.clone();
                self.comfy_settings_panel.load_workflow_from_config(&saved);
                self.avatars = None;
                self.avatars_loaded = false;
                tracing::info!("Config saved through backend API");
            }
            Err(error) => {
                tracing::error!("Failed to persist config via backend API: {}", error);
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
                    } else if !content.trim().is_empty() {
                        self.live_stream_text = Some(content.clone());
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
                    if !self
                        .pending_approvals
                        .iter()
                        .any(|(t, _)| t == tool_name)
                    {
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

        // --- Approval requests ---
        // Rendered inside the activity panel (not as a floating egui::Window) so they appear
        // reliably on all platforms. egui::Window::anchor is fragile on Linux/Wayland: the
        // viewport rect may not be known on the first frame, causing the window to be placed
        // off-screen and remembered there by ID, never recovering.
        let mut approve_tool: Option<String> = None;
        let mut dismiss_tool: Option<String> = None;

        egui::SidePanel::right("activity_panel")
            .resizable(true)
            .default_width(340.0)
            .show_animated(ctx, self.show_activity_panel, |ui| {
                // Pending approvals at the very top — hard to miss.
                for (tool_name, reason) in &self.pending_approvals {
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 160, 50),
                            format!("! Approval needed: {}", tool_name),
                        );
                        ui.add_space(2.0);
                        ui.add(egui::Label::new(egui::RichText::new(reason.as_str()).small()).wrap());
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(
                                    egui::RichText::new("Allow this session")
                                        .color(egui::Color32::from_rgb(80, 200, 100)),
                                )
                                .clicked()
                            {
                                approve_tool = Some(tool_name.clone());
                            }
                            if ui.button("Dismiss").clicked() {
                                dismiss_tool = Some(tool_name.clone());
                            }
                        });
                    });
                    ui.add_space(4.0);
                }

                ui.heading("🧠 Mind");
                ui.add_space(4.0);

                // Zone 1: Current mind state snapshot.
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    if let Some(ref o) = self.last_orientation {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("🧭 {}", o.disposition))
                                    .color(egui::Color32::LIGHT_YELLOW)
                                    .small()
                                    .strong(),
                            );
                            if o.anomaly_count > 0 {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "· {} anomal{}",
                                        o.anomaly_count,
                                        if o.anomaly_count == 1 { "y" } else { "ies" }
                                    ))
                                    .weak()
                                    .small(),
                                );
                            }
                        });
                    }
                    if let Some(ref action) = self.last_action {
                        ui.label(
                            egui::RichText::new(format!("⚡ {}", truncate_str(action, 80)))
                                .small()
                                .color(egui::Color32::LIGHT_GREEN),
                        );
                    }
                    if let Some(ref journal) = self.last_journal {
                        ui.label(
                            egui::RichText::new(format!("📓 {}", truncate_str(journal, 80)))
                                .small()
                                .weak()
                                .italics(),
                        );
                    }
                    if self.last_orientation.is_none()
                        && self.last_action.is_none()
                        && self.last_journal.is_none()
                    {
                        ui.label(
                            egui::RichText::new("Waiting for agent state...")
                                .weak()
                                .italics()
                                .small(),
                        );
                    }
                });

                ui.add_space(4.0);

                // Zone 2: Live LLM token stream.
                egui::CollapsingHeader::new(
                    egui::RichText::new("💭 Live Stream").small().strong(),
                )
                .id_salt("live_stream_header")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(110.0)
                        .stick_to_bottom(true)
                        .id_salt("live_stream_scroll")
                        .show(ui, |ui| {
                            if let Some(ref text) = self.live_stream_text {
                                let preview = last_n_chars(text, 600);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(preview)
                                            .small()
                                            .color(egui::Color32::from_gray(200)),
                                    )
                                    .wrap(),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("—")
                                        .weak()
                                        .small()
                                        .italics(),
                                );
                            }
                        });
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Zone 3: Grouped turn history log.
                super::chat::render_event_log(ui, &self.events, &mut self.event_detail_popup);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                super::sprite::render_agent_sprite(ui, &self.current_state, self.avatars.as_mut());
                ui.vertical(|ui| {
                    ui.heading("Ponderer");
                    ui.horizontal_wrapped(|ui| {
                        let (state_text, state_color) = visual_state_display(&self.current_state);
                        ui.label(
                            egui::RichText::new(state_text)
                                .color(state_color)
                                .small()
                                .strong(),
                        );
                        if let Some(ref o) = self.last_orientation {
                            ui.label(egui::RichText::new("|").weak().small());
                            ui.label(
                                egui::RichText::new(format!("🧭 {}", o.disposition))
                                    .color(egui::Color32::LIGHT_YELLOW)
                                    .small(),
                            );
                        }
                        if let Some(ref action) = self.last_action {
                            ui.label(egui::RichText::new("|").weak().small());
                            ui.label(
                                egui::RichText::new(truncate_str(action, 50))
                                    .weak()
                                    .small(),
                            );
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pause_text = "⏸ Pause";
                    if ui.button(pause_text).clicked() {
                        match self.runtime.block_on(self.api_client.toggle_pause()) {
                            Ok(paused) => {
                                self.current_state = if paused {
                                    AgentVisualState::Paused
                                } else {
                                    AgentVisualState::Idle
                                };
                            }
                            Err(error) => {
                                tracing::error!("Failed to toggle pause: {}", error);
                                self.push_ui_error(format!("Failed to toggle pause: {}", error));
                            }
                        }
                    }

                    if ui.button("⏹ Stop Turn").clicked() {
                        match self.runtime.block_on(self.api_client.stop_agent_turn()) {
                            Ok(_) => {
                                let active = self.active_conversation_id.clone();
                                self.streaming_chat_preview = None;
                                self.clear_live_tool_progress(&active);
                                self.refresh_conversations();
                                self.refresh_chat_history();
                                self.current_state = AgentVisualState::Idle;
                            }
                            Err(error) => {
                                tracing::error!("Failed to stop active turn: {}", error);
                                self.push_ui_error(format!(
                                    "Failed to stop active turn: {}",
                                    error
                                ));
                            }
                        }
                    }

                    if ui.button("⚙ Settings").clicked() {
                        self.settings_panel.show = true;
                    }

                    if ui.button("🎭 Character").clicked() {
                        self.character_panel.show = true;
                    }

                    if ui.button("🎨 Workflow").clicked() {
                        self.comfy_settings_panel.show = true;
                    }

                    let activity_btn_text = if self.show_activity_panel {
                        "📋 Hide Activity"
                    } else {
                        "📋 Show Activity"
                    };
                    if ui.button(activity_btn_text).clicked() {
                        self.show_activity_panel = !self.show_activity_panel;
                    }
                });
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Conversation:");
                let previous_conversation_id = self.active_conversation_id.clone();
                let selected_text = self
                    .conversations
                    .iter()
                    .find(|c| c.id == self.active_conversation_id)
                    .map(conversation_display_label)
                    .unwrap_or_else(|| "Default chat".to_string());

                egui::ComboBox::from_id_salt("chat_conversation_picker")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for conversation in &self.conversations {
                            ui.selectable_value(
                                &mut self.active_conversation_id,
                                conversation.id.clone(),
                                conversation_display_label(conversation),
                            );
                        }
                    });

                if ui.button("New Chat").clicked() {
                    self.create_new_conversation();
                }

                if ui
                    .button(
                        egui::RichText::new("Delete").color(egui::Color32::from_rgb(200, 80, 80)),
                    )
                    .on_hover_text("Delete this conversation")
                    .clicked()
                {
                    self.confirm_delete_conversation_id =
                        Some(self.active_conversation_id.clone());
                }

                if self.active_conversation_id != previous_conversation_id {
                    self.streaming_chat_preview = None;
                    self.refresh_chat_history();
                }
            });
            ui.add_space(6.0);

            let active_streaming_preview = self
                .streaming_chat_preview
                .as_ref()
                .filter(|preview| preview.conversation_id == self.active_conversation_id)
                .map(|preview| preview.content.clone());
            let active_progress: Vec<LiveToolProgress> = self
                .live_tool_progress
                .iter()
                .filter(|entry| entry.conversation_id == self.active_conversation_id)
                .cloned()
                .collect();
            let composer_reserved = 112.0_f32;
            let live_reserved = if active_progress.is_empty() {
                0.0
            } else {
                220.0
            };
            let chat_height = (ui.available_height() - composer_reserved - live_reserved).max(0.0);

            let mut requested_prompt_turn_id: Option<String> = None;
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), chat_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    requested_prompt_turn_id = super::chat::render_private_chat(
                        ui,
                        &self.chat_history,
                        active_streaming_preview.as_deref(),
                        &mut self.chat_media_cache,
                    );
                },
            );
            if let Some(turn_id) = requested_prompt_turn_id {
                self.open_prompt_inspector_for_turn(&turn_id);
            }

            if !active_progress.is_empty() {
                ui.add_space(6.0);
                egui::CollapsingHeader::new(
                    egui::RichText::new("⚡ Live Agent Turn").strong(),
                )
                .id_salt("live_agent_turn")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(170.0)
                        .stick_to_bottom(true)
                        .id_salt("live_turn_scroll")
                        .show(ui, |ui| {
                            for entry in &active_progress {
                                render_live_tool_entry(ui, entry);
                            }
                        });
                });
            }

            ui.add_space(6.0);
            ui.separator();
            ui.label(
                egui::RichText::new("Press Enter to send. Shift+Enter inserts a newline.")
                    .small()
                    .weak(),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("💬");
                let response = ui.add_sized(
                    [ui.available_width() - 80.0, 68.0],
                    egui::TextEdit::multiline(&mut self.user_input)
                        .hint_text("Message Ponderer...")
                        .desired_rows(3),
                );

                let send_shortcut = response.has_focus()
                    && ui.input(|i| {
                        i.key_pressed(egui::Key::Enter)
                            && !i.modifiers.shift
                            && !i.modifiers.ctrl
                            && !i.modifiers.command
                            && !i.modifiers.alt
                    });
                let send_clicked = ui.button("Send").clicked();

                if (send_shortcut || send_clicked) && !self.user_input.trim().is_empty() {
                    let msg = self.user_input.trim().to_string();
                    self.streaming_chat_preview = None;
                    self.send_chat_message(&msg);
                    self.user_input.clear();
                }
            });
            ui.add_space(8.0);
        });

        // Mind event detail pop-out window.
        if let Some(ref text) = self.event_detail_popup.clone() {
            let mut open = true;
            egui::Window::new("Event Detail")
                .collapsible(false)
                .resizable(true)
                .default_size([520.0, 380.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(text.as_str()).small().monospace(),
                                )
                                .wrap(),
                            );
                        });
                    ui.add_space(6.0);
                    if ui.button("Close").clicked() {
                        self.event_detail_popup = None;
                    }
                });
            if !open {
                self.event_detail_popup = None;
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
                    ui.label(
                        egui::RichText::new("This cannot be undone.")
                            .small()
                            .weak(),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("Delete")
                                    .color(egui::Color32::from_rgb(200, 80, 80)),
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
                &inspector.turn_id.chars().take(12).collect::<String>()
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
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
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

        if let Some(new_config) = self.settings_panel.render(ctx) {
            self.persist_config(new_config);
        }

        if let Some(new_config) = self.character_panel.render(ctx) {
            self.persist_config(new_config);
        }

        if self
            .comfy_settings_panel
            .render(ctx, &mut self.settings_panel.config)
        {
            let updated = self.settings_panel.config.clone();
            self.persist_config(updated);
        }

        if let Some(ref tool) = approve_tool {
            match self.runtime.block_on(self.api_client.approve_tool(tool)) {
                Ok(()) => tracing::info!("Session approval granted for: {}", tool),
                Err(e) => self.push_ui_error(format!("Failed to approve tool: {}", e)),
            }
            self.pending_approvals.retain(|(t, _)| t != tool);
        }
        if let Some(ref tool) = dismiss_tool {
            self.pending_approvals.retain(|(t, _)| t != tool);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn visual_state_display(state: &AgentVisualState) -> (&'static str, egui::Color32) {
    match state {
        AgentVisualState::Idle => ("💤 Idle", egui::Color32::from_gray(150)),
        AgentVisualState::Reading => ("👁 Reading", egui::Color32::from_rgb(200, 200, 100)),
        AgentVisualState::Thinking => ("🤔 Thinking", egui::Color32::LIGHT_BLUE),
        AgentVisualState::Writing => ("✍ Writing", egui::Color32::LIGHT_GREEN),
        AgentVisualState::Happy => ("😊 Happy", egui::Color32::from_rgb(100, 255, 150)),
        AgentVisualState::Confused => ("😕 Confused", egui::Color32::from_rgb(255, 150, 100)),
        AgentVisualState::Paused => ("⏸ Paused", egui::Color32::GRAY),
    }
}

fn render_live_tool_entry(ui: &mut egui::Ui, entry: &LiveToolProgress) {
    let color = tool_badge_color(&entry.tool_name);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(&entry.tool_name)
                .color(color)
                .strong()
                .small(),
        );
        if let Some(ref subtask_id) = entry.subtask_id {
            ui.label(egui::RichText::new(format!("[{}]", subtask_id)).weak().small());
        }
        let output = truncate_str(&entry.output_preview, 200);
        ui.add(
            egui::Label::new(
                egui::RichText::new(output)
                    .small()
                    .color(egui::Color32::from_gray(200)),
            )
            .wrap(),
        );
    });
    ui.add_space(2.0);
}

fn tool_badge_color(tool_name: &str) -> egui::Color32 {
    let name = tool_name.to_ascii_lowercase();
    if name.starts_with("shell") || name.contains("run_command") || name.contains("bash") {
        egui::Color32::from_rgb(255, 200, 60)
    } else if name.contains("file") || name.starts_with("read") || name.starts_with("write") {
        egui::Color32::from_rgb(100, 160, 255)
    } else if name.starts_with("http") || name.starts_with("web") || name.starts_with("fetch") {
        egui::Color32::from_rgb(200, 100, 255)
    } else if name.starts_with("memory")
        || name.starts_with("recall")
        || name.starts_with("remember")
    {
        egui::Color32::from_rgb(80, 220, 130)
    } else if name.starts_with("comfy") || name.contains("generate") || name.contains("image") {
        egui::Color32::from_rgb(255, 140, 70)
    } else if name.starts_with("vision") || name.contains("camera") || name.contains("screen") {
        egui::Color32::from_rgb(255, 100, 150)
    } else {
        egui::Color32::from_rgb(180, 180, 180)
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
    color: egui::Color32,
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
        let frame = egui::Frame::group(ui.style())
            .fill(section.color.gamma_multiply(0.15))
            .stroke(egui::Stroke::new(1.0, section.color.gamma_multiply(0.65)));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&section.title).strong());
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("source: {}", section.source))
                        .small()
                        .color(section.color),
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
        let (source, color) = classify_prompt_section(title, trimmed);
        sections.push(PromptSection {
            title: title.to_string(),
            source,
            color,
            body: trimmed.to_string(),
        });
    }

    if sections.is_empty() && !normalized.trim().is_empty() {
        let trimmed = normalized.trim();
        let (source, color) = classify_prompt_section("Prompt", trimmed);
        sections.push(PromptSection {
            title: "Prompt".to_string(),
            source,
            color,
            body: trimmed.to_string(),
        });
    }

    sections
}

fn classify_prompt_section(title: &str, body: &str) -> (&'static str, egui::Color32) {
    let title_l = title.to_ascii_lowercase();
    let body_l = body.to_ascii_lowercase();
    let pick = |source: &'static str, color: egui::Color32| (source, color);

    if title_l.contains("concern priority context") || body_l.contains("concern priority context") {
        return pick(
            "Concerns manager (DB)",
            egui::Color32::from_rgb(230, 179, 90),
        );
    }
    if title_l.contains("working memory") || body_l.contains("working memory") {
        return pick(
            "Working memory (DB)",
            egui::Color32::from_rgb(130, 180, 255),
        );
    }
    if title_l.contains("recent conversation context") || body_l.contains("recent private chat") {
        return pick(
            "Recent chat history",
            egui::Color32::from_rgb(120, 210, 170),
        );
    }
    if title_l.contains("conversation summary snapshot") {
        return pick("Compaction summary", egui::Color32::from_rgb(180, 160, 240));
    }
    if title_l.contains("recent action digest") {
        return pick(
            "Action digest (chat turns)",
            egui::Color32::from_rgb(255, 150, 120),
        );
    }
    if title_l.contains("previous ooda packet") {
        return pick(
            "Previous OODA packet",
            egui::Color32::from_rgb(255, 200, 120),
        );
    }
    if title_l.contains("ooda context") {
        return pick(
            "Orientation + decision context",
            egui::Color32::from_rgb(140, 235, 140),
        );
    }
    if title_l.contains("new operator message") {
        return pick("Operator input", egui::Color32::from_rgb(120, 200, 255));
    }
    if title_l.contains("autonomous continuation context") {
        return pick("Continuation hint", egui::Color32::from_rgb(220, 220, 120));
    }

    pick("Additional context", egui::Color32::from_gray(170))
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
