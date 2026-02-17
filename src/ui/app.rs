use eframe::egui;
use flume::Receiver;
use std::sync::Arc;

use super::avatar::AvatarSet;
use super::character::CharacterPanel;
use super::comfy_settings::ComfySettingsPanel;
use super::settings::SettingsPanel;
use crate::agent::{Agent, AgentEvent, AgentVisualState};
use crate::config::AgentConfig;
use crate::database::{AgentDatabase, ChatConversation, ChatMessage, DEFAULT_CHAT_CONVERSATION_ID};

const MAX_LIVE_TOOL_PROGRESS_LINES: usize = 200;

pub struct AgentApp {
    events: Vec<AgentEvent>,
    event_rx: Receiver<AgentEvent>,
    agent: Arc<Agent>,
    current_state: AgentVisualState,
    user_input: String,
    runtime: tokio::runtime::Runtime,
    settings_panel: SettingsPanel,
    character_panel: CharacterPanel,
    comfy_settings_panel: ComfySettingsPanel,
    avatars: Option<AvatarSet>,
    avatars_loaded: bool,
    database: Option<Arc<AgentDatabase>>,
    conversations: Vec<ChatConversation>,
    active_conversation_id: String,
    chat_history: Vec<ChatMessage>,
    chat_media_cache: super::chat::ChatMediaCache,
    live_tool_progress: Vec<LiveToolProgress>,
    streaming_chat_preview: Option<StreamingChatPreview>,
    last_chat_refresh: std::time::Instant,
    show_activity_panel: bool,
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

impl AgentApp {
    pub fn new(
        event_rx: Receiver<AgentEvent>,
        agent: Arc<Agent>,
        config: AgentConfig,
        database: Option<Arc<AgentDatabase>>,
    ) -> Self {
        let mut comfy_settings_panel = ComfySettingsPanel::new();
        comfy_settings_panel.load_workflow_from_config(&config);

        let mut app = Self {
            events: Vec::new(),
            event_rx,
            agent,
            current_state: AgentVisualState::Idle,
            user_input: String::new(),
            runtime: tokio::runtime::Runtime::new().unwrap(),
            settings_panel: SettingsPanel::new(config.clone()),
            character_panel: CharacterPanel::new(config),
            comfy_settings_panel,
            avatars: None, // Will be loaded on first frame when egui context is available
            avatars_loaded: false,
            database,
            conversations: Vec::new(),
            active_conversation_id: DEFAULT_CHAT_CONVERSATION_ID.to_string(),
            chat_history: Vec::new(),
            chat_media_cache: super::chat::ChatMediaCache::new(),
            live_tool_progress: Vec::new(),
            streaming_chat_preview: None,
            last_chat_refresh: std::time::Instant::now(),
            show_activity_panel: false,
        };
        app.refresh_conversations();
        app.refresh_chat_history();
        app
    }

    fn refresh_conversations(&mut self) {
        if let Some(ref db) = self.database {
            match db.list_chat_conversations(100) {
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
                Err(e) => {
                    tracing::warn!("Failed to refresh chat conversations: {}", e);
                }
            }
        }
    }

    fn refresh_chat_history(&mut self) {
        if let Some(ref db) = self.database {
            match db.get_chat_history_for_conversation(&self.active_conversation_id, 200) {
                Ok(history) => {
                    self.chat_history = history;
                }
                Err(e) => {
                    tracing::warn!("Failed to refresh chat history: {}", e);
                }
            }
        }
    }

    fn send_chat_message(&mut self, content: &str) {
        let active_conversation = self.active_conversation_id.clone();
        self.clear_live_tool_progress(&active_conversation);
        if let Some(ref db) = self.database {
            match db.add_chat_message_in_conversation(
                &self.active_conversation_id,
                "operator",
                content,
            ) {
                Ok(_) => {
                    tracing::info!("Sent chat message to agent: {}", content);
                    self.refresh_conversations();
                    self.refresh_chat_history();
                }
                Err(e) => {
                    tracing::error!("Failed to send chat message: {}", e);
                }
            }
        }
    }

    fn create_new_conversation(&mut self) {
        if let Some(ref db) = self.database {
            match db.create_chat_conversation(None) {
                Ok(conversation) => {
                    self.active_conversation_id = conversation.id;
                    self.user_input.clear();
                    self.streaming_chat_preview = None;
                    self.refresh_conversations();
                    self.refresh_chat_history();
                }
                Err(e) => {
                    tracing::error!("Failed to create conversation: {}", e);
                }
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
        crate::database::ChatTurnPhase::Idle => "",
        crate::database::ChatTurnPhase::Processing => " · processing",
        crate::database::ChatTurnPhase::Completed => " · done",
        crate::database::ChatTurnPhase::AwaitingApproval => " · awaiting input",
        crate::database::ChatTurnPhase::Failed => " · failed",
    };

    if status_suffix.is_empty() {
        base
    } else {
        format!("{}{}", base, status_suffix)
    }
}

impl eframe::App for AgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Load avatars on first frame when context is available
        if !self.avatars_loaded {
            let config = self.settings_panel.config.clone();
            self.load_avatars(ctx, &config);
            self.avatars_loaded = true;
        }

        // Periodically refresh chat history
        if self.last_chat_refresh.elapsed() > std::time::Duration::from_secs(2) {
            self.refresh_conversations();
            self.refresh_chat_history();
            self.last_chat_refresh = std::time::Instant::now();
        }

        // Poll for new events from agent (non-blocking)
        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                AgentEvent::StateChanged(state) => {
                    self.current_state = state.clone();
                }
                AgentEvent::ChatStreaming {
                    conversation_id,
                    content,
                    done,
                } => {
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
                AgentEvent::ToolCallProgress {
                    conversation_id,
                    tool_name,
                    output_preview,
                } => {
                    self.push_live_tool_progress(conversation_id, tool_name, output_preview);
                }
                AgentEvent::ActionTaken { action, .. } if action.contains("operator") => {
                    self.refresh_conversations();
                    self.refresh_chat_history();
                    self.streaming_chat_preview = None;
                }
                _ => {}
            }
            self.events.push(event);
        }

        egui::SidePanel::right("activity_panel")
            .resizable(true)
            .default_width(320.0)
            .show_animated(ctx, self.show_activity_panel, |ui| {
                ui.heading("Activity");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Secondary event/reasoning log")
                        .weak()
                        .italics(),
                );
                ui.add_space(8.0);
                super::chat::render_event_log(ui, &self.events);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Header with agent sprite
            ui.horizontal(|ui| {
                super::sprite::render_agent_sprite(ui, &self.current_state, self.avatars.as_mut());
                ui.vertical(|ui| {
                    ui.heading("Ponderer");
                    ui.label(format!("Status: {:?}", self.current_state));
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pause_text = "⏸ Pause";
                    if ui.button(pause_text).clicked() {
                        let agent = self.agent.clone();
                        self.runtime.spawn(async move {
                            agent.toggle_pause().await;
                        });
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

                    // Toggle secondary activity panel
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

                if self.active_conversation_id != previous_conversation_id {
                    self.streaming_chat_preview = None;
                    self.refresh_chat_history();
                }
            });
            ui.add_space(6.0);

            // Chat is now the primary interaction surface.
            let active_streaming_preview = self
                .streaming_chat_preview
                .as_ref()
                .filter(|preview| preview.conversation_id == self.active_conversation_id)
                .map(|preview| preview.content.as_str());
            super::chat::render_private_chat(
                ui,
                &self.chat_history,
                active_streaming_preview,
                &mut self.chat_media_cache,
            );

            let active_progress: Vec<LiveToolProgress> = self
                .live_tool_progress
                .iter()
                .filter(|entry| entry.conversation_id == self.active_conversation_id)
                .cloned()
                .collect();
            if !active_progress.is_empty() {
                ui.add_space(6.0);
                egui::CollapsingHeader::new("Live Agent Turn")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(
                                "Real-time tool output while the agent is still working",
                            )
                            .small()
                            .weak(),
                        );
                        ui.add_space(4.0);
                        let mut global_lines: Vec<String> = Vec::new();
                        let mut subtask_groups: Vec<(String, Vec<String>)> = Vec::new();

                        for entry in &active_progress {
                            let line = format!("{} -> {}", entry.tool_name, entry.output_preview);
                            if let Some(subtask_id) = entry.subtask_id.as_deref() {
                                if let Some((_, lines)) =
                                    subtask_groups.iter_mut().find(|(id, _)| id == subtask_id)
                                {
                                    lines.push(line);
                                } else {
                                    subtask_groups.push((subtask_id.to_string(), vec![line]));
                                }
                            } else {
                                global_lines.push(line);
                            }
                        }

                        if !global_lines.is_empty() {
                            ui.label(egui::RichText::new("General").small().weak());
                            egui::ScrollArea::vertical()
                                .max_height(80.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &global_lines {
                                        ui.monospace(line);
                                    }
                                });
                            ui.add_space(6.0);
                        }

                        for (subtask_id, lines) in &subtask_groups {
                            egui::CollapsingHeader::new(format!("Subtask {}", subtask_id))
                                .default_open(true)
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(120.0)
                                        .stick_to_bottom(true)
                                        .show(ui, |ui| {
                                            for line in lines {
                                                ui.monospace(line);
                                            }
                                        });
                                });
                        }

                        if subtask_groups.is_empty() {
                            egui::ScrollArea::vertical()
                                .max_height(120.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &active_progress {
                                        ui.monospace(format!(
                                            "{} -> {}",
                                            line.tool_name, line.output_preview
                                        ));
                                    }
                                });
                        }
                    });
            }

            ui.separator();
            ui.label(
                egui::RichText::new("Press Enter to send. Shift+Enter inserts a newline.")
                    .small()
                    .weak(),
            );
            ui.add_space(4.0);

            // Multi-line chat input
            ui.horizontal(|ui| {
                ui.label("💬");
                let response = ui.add_sized(
                    [ui.available_width() - 80.0, 72.0],
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
        });

        // Render settings panel
        if let Some(new_config) = self.settings_panel.render(ctx) {
            // User saved new config - persist it to disk
            if let Err(e) = new_config.save() {
                tracing::error!("Failed to save config: {}", e);
            } else {
                tracing::info!("Config saved successfully");
                // Reload agent with new config immediately
                let agent = self.agent.clone();
                let config_clone = new_config.clone();
                self.runtime.spawn(async move {
                    agent.reload_config(config_clone).await;
                });
            }
        }

        // Render character panel
        if let Some(new_config) = self.character_panel.render(ctx) {
            // User saved new character - persist it to disk
            if let Err(e) = new_config.save() {
                tracing::error!("Failed to save config: {}", e);
            } else {
                tracing::info!("Character saved successfully");
                // Update the settings panel with the new config too
                self.settings_panel.config = new_config.clone();
                // Reload agent with new config immediately
                let agent = self.agent.clone();
                let config_clone = new_config;
                self.runtime.spawn(async move {
                    agent.reload_config(config_clone).await;
                });
            }
        }

        // Render ComfyUI workflow panel
        if self
            .comfy_settings_panel
            .render(ctx, &mut self.settings_panel.config)
        {
            // User saved workflow settings
            if let Err(e) = self.settings_panel.config.save() {
                tracing::error!("Failed to save config: {}", e);
            } else {
                tracing::info!("Workflow settings saved successfully");
                // Reload agent with new config immediately
                let agent = self.agent.clone();
                let config_clone = self.settings_panel.config.clone();
                self.runtime.spawn(async move {
                    agent.reload_config(config_clone).await;
                });
            }
        }

        // Request repaint for smooth updates
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
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
