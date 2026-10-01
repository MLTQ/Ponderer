use super::plugin_settings_form::PluginSettingsForm;
use crate::api::{
    PluginManifest, PluginSettingsSchemaManifest, PluginSettingsTabManifest, ScheduledJob,
};
use crate::config::AgentConfig;
use eframe::egui;
use std::collections::{HashMap, HashSet};

const CORE_TAB_GENERAL: &str = "core.general";
const CORE_TAB_BEHAVIOR: &str = "core.behavior";
const CORE_TAB_LOOPS: &str = "core.loops";
const CORE_TAB_MEMORY: &str = "core.memory";
const CORE_TAB_SYSTEM: &str = "core.system";
const CORE_TAB_SCHEDULES: &str = "core.schedules";
const CORE_TAB_APPEARANCE: &str = "core.appearance";
const CORE_TAB_CONNECTIONS: &str = "core.connections";

#[derive(Debug, Clone)]
pub enum ScheduledJobAction {
    Refresh,
    Create {
        name: String,
        prompt: String,
        interval_minutes: u64,
        enabled: bool,
    },
    Update {
        job_id: String,
        name: String,
        prompt: String,
        interval_minutes: u64,
        enabled: bool,
    },
    Delete {
        job_id: String,
    },
}

#[derive(Debug, Clone)]
struct ScheduledJobEditor {
    name: String,
    prompt: String,
    interval_minutes: u64,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct PendingScheduledJobDraft {
    local_id: String,
    editor: ScheduledJobEditor,
}

pub struct SettingsPanel {
    pub config: AgentConfig,
    saved_config: AgentConfig,
    pub save_error: Option<String>,
    pub show: bool,
    selected_tab: String,
    plugin_manifests: Vec<PluginManifest>,
    scheduled_jobs: Vec<ScheduledJob>,
    scheduled_job_editors: HashMap<String, ScheduledJobEditor>,
    pending_new_scheduled_jobs: Vec<PendingScheduledJobDraft>,
    pending_deleted_job_ids: HashSet<String>,
    scheduled_job_actions: Vec<ScheduledJobAction>,
    scheduled_jobs_error: Option<String>,
    new_job_name: String,
    new_job_prompt: String,
    new_job_interval_minutes: u64,
    new_job_enabled: bool,
    next_local_scheduled_job_id: u64,
}

impl SettingsPanel {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            saved_config: config.clone(),
            config,
            save_error: None,
            show: false,
            selected_tab: CORE_TAB_GENERAL.to_string(),
            plugin_manifests: Vec::new(),
            scheduled_jobs: Vec::new(),
            scheduled_job_editors: HashMap::new(),
            pending_new_scheduled_jobs: Vec::new(),
            pending_deleted_job_ids: HashSet::new(),
            scheduled_job_actions: Vec::new(),
            scheduled_jobs_error: None,
            new_job_name: String::new(),
            new_job_prompt: String::new(),
            new_job_interval_minutes: 60,
            new_job_enabled: true,
            next_local_scheduled_job_id: 1,
        }
    }

    pub fn set_plugin_manifests(&mut self, plugin_manifests: Vec<PluginManifest>) {
        self.plugin_manifests = plugin_manifests;
        self.ensure_valid_selected_tab();
    }

    pub fn sync_from_config(&mut self, config: AgentConfig) {
        self.saved_config = config.clone();
        self.save_error = None;
        self.config = config;
    }

    pub fn sync_provider_from_config(&mut self, config: &AgentConfig) {
        // Session provider changes must not erase unrelated unsaved settings.
        self.config.llm_api_url = config.llm_api_url.clone();
        self.config.llm_api_key = config.llm_api_key.clone();
        self.config.llm_model = config.llm_model.clone();
        self.config.reflection_model = config.reflection_model.clone();
        self.config.respond_to.decision_model = config.respond_to.decision_model.clone();
        self.saved_config.llm_api_url = config.llm_api_url.clone();
        self.saved_config.llm_api_key = config.llm_api_key.clone();
        self.saved_config.llm_model = config.llm_model.clone();
        self.saved_config.reflection_model = config.reflection_model.clone();
        self.saved_config.respond_to.decision_model = config.respond_to.decision_model.clone();
    }

    pub fn sync_loose_action(&mut self, enabled: bool) {
        self.config.loose_mode = enabled;
        self.saved_config.loose_mode = enabled;
        if enabled {
            self.config.enable_ambient_loop = true;
            self.saved_config.enable_ambient_loop = true;
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        serde_json::to_value(&self.config).ok() != serde_json::to_value(&self.saved_config).ok()
            || self.has_unsaved_schedule_changes()
    }

    pub fn has_unsaved_schedule_changes(&self) -> bool {
        !self.pending_new_scheduled_jobs.is_empty()
            || !self.pending_deleted_job_ids.is_empty()
            || self.scheduled_jobs.iter().any(|job| {
                self.scheduled_job_editors
                    .get(&job.id)
                    .is_some_and(|editor| {
                        editor.name.trim() != job.name
                            || editor.prompt.trim() != job.prompt
                            || editor.interval_minutes.clamp(1, 10080) != job.interval_minutes
                            || editor.enabled != job.enabled
                    })
            })
    }

    pub fn revert_drafts(&mut self) {
        self.config = self.saved_config.clone();
        self.save_error = None;
        self.scheduled_job_editors.clear();
        self.pending_new_scheduled_jobs.clear();
        self.pending_deleted_job_ids.clear();
    }

    pub fn set_scheduled_jobs(&mut self, scheduled_jobs: Vec<ScheduledJob>) {
        self.scheduled_jobs = scheduled_jobs;
        self.scheduled_job_editors.clear();
        self.pending_new_scheduled_jobs.clear();
        self.pending_deleted_job_ids.clear();
    }

    pub fn set_scheduled_jobs_error(&mut self, error: Option<String>) {
        self.scheduled_jobs_error = error;
    }

    pub fn take_scheduled_job_actions(&mut self) -> Vec<ScheduledJobAction> {
        std::mem::take(&mut self.scheduled_job_actions)
    }

    /// A failed config save must not execute schedule mutations. Editors remain
    /// intact so a subsequent Save can retry the same drafts.
    pub fn discard_queued_schedule_changes(&mut self) {
        self.scheduled_job_actions
            .retain(|action| matches!(action, ScheduledJobAction::Refresh));
    }

    pub fn open_tab(&mut self, tab_id: &str) {
        self.show = true;
        if self.available_tab_ids().iter().any(|id| id == tab_id) {
            self.selected_tab = tab_id.to_string();
        } else {
            self.selected_tab = CORE_TAB_GENERAL.to_string();
        }
    }

    pub fn render_contents(
        &mut self,
        ui: &mut egui::Ui,
        save_allowed: bool,
        mut model_controls: impl FnMut(&mut egui::Ui, &mut AgentConfig),
    ) -> Option<AgentConfig> {
        self.ensure_valid_selected_tab();
        ui.heading("Settings");
        self.render_tab_bar(ui);
        ui.separator();
        let body_height = (ui.available_height() - 85.0).max(60.0);
        egui::ScrollArea::vertical()
            .id_salt("settings_tab_scroll")
            .max_height(body_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let selected_tab = self.selected_tab.clone();
                match selected_tab.as_str() {
                    CORE_TAB_GENERAL => self.render_general_tab(ui, &mut model_controls),
                    CORE_TAB_APPEARANCE => self.render_appearance_tab(ui),
                    CORE_TAB_CONNECTIONS => self.render_connections_tab(ui),
                    CORE_TAB_BEHAVIOR => self.render_behavior_tab(ui),
                    CORE_TAB_LOOPS => {
                        self.render_outreach(ui);
                        ui.separator();
                        self.render_loops_tab(ui);
                    }
                    CORE_TAB_MEMORY => self.render_memory_tab(ui),
                    CORE_TAB_SYSTEM => self.render_system_tab(ui),
                    CORE_TAB_SCHEDULES => self.render_schedules_tab(ui),
                    _ => {
                        if let Some((plugin_id, schema)) =
                            self.dynamic_plugin_schema_for_tab(&selected_tab)
                        {
                            let heading = self
                                .plugin_manifests
                                .iter()
                                .find(|manifest| manifest.id == plugin_id)
                                .map(|manifest| manifest.name.as_str())
                                .unwrap_or("Plugin settings")
                                .to_string();
                            ui.heading(heading);
                            PluginSettingsForm::render(ui, &mut self.config, &plugin_id, &schema);
                        }
                    }
                }
            });
        ui.separator();
        if let Some(error) = &self.save_error {
            ui.colored_label(super::theme::palette(ui).error, error);
        }
        let mut result = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(save_allowed, egui::Button::new("Save & apply"))
                .clicked()
                && self.queue_dirty_scheduled_job_updates()
            {
                self.save_error = None;
                result = Some(self.config.clone());
            }
            if ui
                .add_enabled(save_allowed, egui::Button::new("Revert drafts"))
                .clicked()
            {
                self.revert_drafts();
            }
            ui.small(if self.has_unsaved_changes() {
                "Unsaved changes"
            } else {
                "Saved"
            });
        });
        result
    }

    fn render_appearance_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Appearance");
        ui.horizontal(|ui| {
            ui.label("Base color:").on_hover_text("Derives panel, control, chat and sphere colors. Warning and error colors stay amber and red.");
            ui.color_edit_button_srgb(&mut self.config.appearance.base_color);
            let [r, g, b] = self.config.appearance.base_color;
            ui.monospace(format!("#{r:02X}{g:02X}{b:02X}"));
        });
        ui.checkbox(&mut self.config.appearance.dark, "Dark workbench");
        ui.horizontal_wrapped(|ui| {
            for (name, color) in [
                ("Moss", [168, 209, 139]),
                ("Slate", [144, 179, 213]),
                ("Violet", [183, 156, 220]),
                ("Copper", [216, 166, 117]),
                ("Neutral", [178, 182, 181]),
            ] {
                if ui.small_button(name).clicked() {
                    self.config.appearance.base_color = color;
                }
            }
        });
        let palette = super::theme::Palette::from_config(&self.config.appearance);
        ui.horizontal_wrapped(|ui| {
            for (label, color) in [
                ("accent", palette.accent),
                ("text", palette.text),
                ("muted", palette.muted),
                ("warning", palette.warning),
                ("error", palette.error),
            ] {
                ui.colored_label(color, label);
            }
        });
    }

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (tab_id, label) in [
                (CORE_TAB_GENERAL, "Models"),
                (CORE_TAB_APPEARANCE, "Appearance"),
                (CORE_TAB_CONNECTIONS, "Connections"),
                (CORE_TAB_LOOPS, "Autonomy & contact"),
                (CORE_TAB_MEMORY, "Data & memory"),
                (CORE_TAB_BEHAVIOR, "Turn budgets"),
                (CORE_TAB_SYSTEM, "System prompt"),
                (CORE_TAB_SCHEDULES, "Schedules"),
            ] {
                let selected = self.selected_tab == tab_id;
                if ui.selectable_label(selected, label).clicked() {
                    self.selected_tab = tab_id.to_string();
                }
            }

            for tab in self.skill_tabs() {
                let selected = self.selected_tab == tab.id;
                if ui.selectable_label(selected, tab.title).clicked() {
                    self.selected_tab = tab.id;
                }
            }
        });
    }

    fn render_general_tab(
        &mut self,
        ui: &mut egui::Ui,
        model_controls: &mut impl FnMut(&mut egui::Ui, &mut AgentConfig),
    ) {
        model_controls(ui, &mut self.config);
        ui.add_space(16.0);
        ui.small("Core boundaries, relationship and character are configured in Identity.");
    }

    fn render_connections_tab(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Telegram Bot");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Bot token:    ");
            let mut token_str = self.config.telegram_bot_token.clone().unwrap_or_default();
            if ui
                .add(egui::TextEdit::singleline(&mut token_str).password(true))
                .changed()
            {
                self.config.telegram_bot_token = if token_str.trim().is_empty() {
                    None
                } else {
                    Some(token_str.trim().to_string())
                };
            }
        });
        ui.label(
            egui::RichText::new("Get a token from @BotFather on Telegram. Leave blank to disable.")
                .small()
                .weak(),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Authorized chat ID:");
            let mut id_str = self
                .config
                .telegram_chat_id
                .map(|id| id.to_string())
                .unwrap_or_default();
            if ui.text_edit_singleline(&mut id_str).changed() {
                self.config.telegram_chat_id = id_str.trim().parse::<i64>().ok();
            }
        });
        ui.label(
            egui::RichText::new(
                "Required: your positive private chat ID. Groups and other senders are rejected.\n\
                 To find your ID: message the bot, then open\n\
                 https://api.telegram.org/bot<TOKEN>/getUpdates",
            )
            .small()
            .weak(),
        );
        ui.label(
            egui::RichText::new(
                "Settings apply on Save. Telegram stops when this UI's backend stops.",
            )
            .small()
            .color(super::theme::palette(ui).warning),
        );
    }

    fn render_outreach(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Chosen outreach");
        ui.label("The agent decides whether there is a reason to speak. These are limits, not a schedule.");
        let policy = &mut self.config.outreach;
        ui.checkbox(&mut policy.enabled, "Allow spontaneous messages");
        ui.checkbox(
            &mut policy.telegram_enabled,
            "Allow spontaneous Telegram messages (otherwise local chat)",
        );
        ui.horizontal(|ui| {
            ui.label("Minimum spacing (seconds):");
            ui.add(egui::DragValue::new(&mut policy.min_interval_secs).range(60..=604800));
        });
        ui.horizontal(|ui| {
            ui.label("Maximum messages per rolling 24 hours:");
            ui.add(egui::DragValue::new(&mut policy.max_per_day).range(0..=20));
        });
        ui.horizontal(|ui| {
            ui.label("Quiet hours, local time (equal hours disables):");
            ui.add(egui::DragValue::new(&mut policy.quiet_start_hour).range(0..=23));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut policy.quiet_end_hour).range(0..=23));
        });
        ui.checkbox(
            &mut policy.allow_urgent_during_quiet,
            "Allow evidence-backed urgent contact during quiet hours",
        );
        ui.label("Pause stops queued Telegram sends as well as cognition. Replies are not spontaneous outreach.");
    }

    fn render_behavior_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Behavior");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Poll interval (seconds):");
            ui.add(egui::DragValue::new(&mut self.config.poll_interval_secs).range(10..=600));
        });
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.disable_tool_iteration_limit,
            "Disable tool-iteration limit (unbounded)",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Max tool iterations per turn:");
            ui.add(egui::DragValue::new(&mut self.config.max_tool_iterations).range(1..=500));
        });
        ui.label(
            egui::RichText::new(
                "Applies to autonomous tool loops. Disable limit for fully unbounded loops.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max foreground chat turns:");
            ui.add(egui::DragValue::new(&mut self.config.max_chat_autonomous_turns).range(1..=64));
        });
        ui.checkbox(
            &mut self.config.disable_chat_turn_limit,
            "Disable foreground chat turn limit (model decides)",
        );
        ui.label(
            egui::RichText::new(
                "Foreground continuation follows turn_control; disabling this limit still keeps a fixed emergency ceiling.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max background subtask turns:");
            ui.add(
                egui::DragValue::new(&mut self.config.max_background_subtask_turns).range(1..=256),
            );
        });
        ui.checkbox(
            &mut self.config.disable_background_subtask_turn_limit,
            "Disable background subtask turn limit (model decides)",
        );
        ui.label(
            egui::RichText::new(
                "Background continuation follows turn_control; disabling this limit still keeps a fixed emergency ceiling.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Loop heat shock threshold:");
            ui.add(egui::DragValue::new(&mut self.config.loop_heat_threshold).range(1..=200));
        });
        ui.horizontal(|ui| {
            ui.label("Loop similarity threshold:");
            ui.add(
                egui::DragValue::new(&mut self.config.loop_similarity_threshold)
                    .speed(0.01)
                    .range(0.5..=0.999),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Loop signature window:");
            ui.add(egui::DragValue::new(&mut self.config.loop_signature_window).range(2..=200));
        });
        ui.horizontal(|ui| {
            ui.label("Loop heat cooldown:");
            ui.add(egui::DragValue::new(&mut self.config.loop_heat_cooldown).range(1..=20));
        });
        ui.label(
            egui::RichText::new(
                "Heat rises when consecutive autonomous turns are highly similar; at threshold the agent is forced to yield with a loop-break notice.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max posts per hour:");
            ui.add(egui::DragValue::new(&mut self.config.max_posts_per_hour).range(1..=100));
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Response strategy:");
            egui::ComboBox::from_id_salt("response_type")
                .selected_text(&self.config.respond_to.response_type)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.config.respond_to.response_type,
                        "selective".to_string(),
                        "Selective (LLM decides)",
                    );
                    ui.selectable_value(
                        &mut self.config.respond_to.response_type,
                        "all".to_string(),
                        "All posts",
                    );
                    ui.selectable_value(
                        &mut self.config.respond_to.response_type,
                        "mentions".to_string(),
                        "Only mentions",
                    );
                });
        });
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_screen_capture_in_loop,
            "Allow screen capture in agentic loop (opt-in)",
        );
        ui.label(
            egui::RichText::new(
                "Enables the capture_screen tool so the agent can inspect your current desktop.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_camera_capture_tool,
            "Allow camera snapshots in agentic loop (opt-in)",
        );
        ui.label(
            egui::RichText::new(
                "Enables the capture_camera_snapshot tool so the agent can capture a camera image on demand.",
            )
            .small()
            .weak(),
        );
    }

    fn render_loops_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Living Loop");
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_ambient_loop,
            "Enable ambient loop architecture",
        );
        ui.add_space(4.0);

        ui.label(if self.config.loose_mode {
            "Self-directed (Loose) is armed."
        } else {
            "Self-directed (Loose) is disarmed."
        });
        ui.label(
            egui::RichText::new(
                "Arming permits auto-approved local shell/filesystem work. Use Permissions in the workbench header to review and deliberately arm it; Disarm remains visible while armed.",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("Breath between episodes (seconds):");
            ui.add(
                egui::DragValue::new(&mut self.config.loose_episode_interval_secs).range(1..=60),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Episodes before cooldown:");
            ui.add(
                egui::DragValue::new(&mut self.config.loose_max_consecutive_episodes).range(1..=64),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Cooldown (seconds):");
            ui.add(egui::DragValue::new(&mut self.config.loose_cooldown_secs).range(30..=86400));
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Ambient min tick (seconds):");
            ui.add(egui::DragValue::new(&mut self.config.ambient_min_interval_secs).range(5..=600));
        });
        ui.add_space(4.0);

        ui.checkbox(&mut self.config.enable_journal, "Enable ambient journaling");
        ui.horizontal(|ui| {
            ui.label("Journal min interval (seconds):");
            ui.add(
                egui::DragValue::new(&mut self.config.journal_min_interval_secs).range(30..=7200),
            );
        });
        ui.add_space(4.0);

        ui.checkbox(&mut self.config.enable_concerns, "Enable concern lifecycle");
        ui.checkbox(&mut self.config.enable_dream_cycle, "Enable dream cycle");
        ui.horizontal(|ui| {
            ui.label("Dream min interval (seconds):");
            ui.add(
                egui::DragValue::new(&mut self.config.dream_min_interval_secs).range(300..=86400),
            );
        });
        ui.add_space(16.0);

        ui.separator();
        ui.heading("Autonomous Heartbeat");
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_heartbeat,
            "Enable periodic heartbeat checks",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Heartbeat interval (minutes):");
            ui.add(egui::DragValue::new(&mut self.config.heartbeat_interval_mins).range(5..=1440));
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Checklist file:");
            ui.text_edit_singleline(&mut self.config.heartbeat_checklist_path);
        });
        ui.label("Example: HEARTBEAT.md");
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_memory_evolution,
            "Run memory evolution on heartbeat schedule",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Memory evolution interval (hours):");
            ui.add(
                egui::DragValue::new(&mut self.config.memory_evolution_interval_hours)
                    .range(1..=168),
            );
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Replay trace set (optional):");
            let trace_path = self
                .config
                .memory_eval_trace_set_path
                .get_or_insert_with(String::new);
            ui.text_edit_singleline(trace_path);
        });
        if self
            .config
            .memory_eval_trace_set_path
            .as_ref()
            .is_some_and(|p| p.trim().is_empty())
        {
            self.config.memory_eval_trace_set_path = None;
        }
        ui.label("Blank uses built-in replay traces");
        ui.add_space(16.0);

        ui.separator();
        ui.heading("Self-Reflection & Evolution");
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_self_reflection,
            "Enable self-reflection",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Reflection interval (hours):");
            ui.add(egui::DragValue::new(&mut self.config.reflection_interval_hours).range(1..=168));
        });
        ui.add_space(8.0);

        ui.small("Guiding principles and fixed boundaries live in Identity.");
    }

    fn render_memory_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Memory & Database");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Database path:");
            ui.text_edit_singleline(&mut self.config.database_path);
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Max important posts:");
            ui.add(egui::DragValue::new(&mut self.config.max_important_posts).range(10..=1000));
        });
    }

    fn render_system_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("System Prompt");
        ui.add_space(8.0);

        ui.label("Customize how the agent behaves:");
        ui.text_edit_multiline(&mut self.config.system_prompt);
    }

    fn render_schedules_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scheduled Tasks");
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Recurring schedules queue private operator messages on an interval (1-10080 minutes). Changes on this page apply only when you click Save & Apply.",
            )
            .small()
            .weak(),
        );
        ui.add_space(6.0);

        if let Some(error) = self.scheduled_jobs_error.as_deref() {
            ui.colored_label(super::theme::palette(ui).error, error);
            ui.add_space(6.0);
        }

        ui.horizontal(|ui| {
            if ui.button("Refresh Jobs").clicked() {
                self.scheduled_job_actions.push(ScheduledJobAction::Refresh);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} configured",
                    self.scheduled_jobs.len() + self.pending_new_scheduled_jobs.len()
                        - self
                            .pending_deleted_job_ids
                            .len()
                            .min(self.scheduled_jobs.len())
                ))
                .small()
                .weak(),
            );
        });
        ui.add_space(8.0);

        if self.scheduled_jobs.is_empty() {
            ui.label(
                egui::RichText::new("No schedules yet. Add one below.")
                    .small()
                    .italics()
                    .weak(),
            );
            ui.add_space(6.0);
        }

        for job in self.scheduled_jobs.clone() {
            let mut should_revert = false;
            let mut should_delete = false;
            let mut should_undo_delete = false;
            let pending_delete = self.pending_deleted_job_ids.contains(&job.id);
            {
                let editor = self.editor_for_job(&job);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&job.id[..job.id.len().min(12)])
                                .weak()
                                .small(),
                        );
                        ui.checkbox(&mut editor.enabled, "Enabled");
                        ui.label(
                            egui::RichText::new(format!(
                                "Next: {}",
                                job.next_run_at.format("%Y-%m-%d %H:%M:%S UTC")
                            ))
                            .small()
                            .weak(),
                        );
                    });
                    if let Some(last_run_at) = job.last_run_at {
                        ui.label(
                            egui::RichText::new(format!(
                                "Last run: {}",
                                last_run_at.format("%Y-%m-%d %H:%M:%S UTC")
                            ))
                            .small()
                            .weak(),
                        );
                    }

                    if pending_delete {
                        ui.colored_label(
                            super::theme::palette(ui).error,
                            "Marked for deletion. Save & Apply to remove this schedule.",
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut editor.name);
                            ui.label("Interval (minutes):");
                            ui.add(
                                egui::DragValue::new(&mut editor.interval_minutes).range(1..=10080),
                            );
                        });

                        ui.label("Prompt:");
                        ui.add(
                            egui::TextEdit::multiline(&mut editor.prompt)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );

                        let dirty_after_edit = editor.name.trim() != job.name
                            || editor.prompt.trim() != job.prompt
                            || editor.interval_minutes.clamp(1, 10080) != job.interval_minutes
                            || editor.enabled != job.enabled;
                        if dirty_after_edit {
                            ui.label(
                                egui::RichText::new("Unsaved schedule changes")
                                    .small()
                                    .color(super::theme::palette(ui).warning),
                            );
                        }
                    }

                    ui.horizontal(|ui| {
                        if pending_delete {
                            if ui.button("Undo Delete").clicked() {
                                should_undo_delete = true;
                            }
                        } else {
                            if ui.button("Revert").clicked() {
                                should_revert = true;
                            }
                            if ui
                                .button(
                                    egui::RichText::new("Delete on Save")
                                        .color(super::theme::palette(ui).error),
                                )
                                .clicked()
                            {
                                should_delete = true;
                            }
                        }
                    });
                });
            }
            ui.add_space(6.0);

            if should_revert {
                self.reset_editor_for_job(&job);
                self.pending_deleted_job_ids.remove(&job.id);
            }

            if should_undo_delete {
                self.pending_deleted_job_ids.remove(&job.id);
            }

            if should_delete {
                self.pending_deleted_job_ids.insert(job.id.clone());
            }
        }

        for draft in self.pending_new_scheduled_jobs.clone() {
            let mut should_remove = false;
            {
                let editor = self.editor_for_pending_job(&draft.local_id);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("new").weak().small().italics());
                        ui.label(
                            egui::RichText::new("Will be created on Save & Apply")
                                .small()
                                .color(super::theme::palette(ui).warning),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut editor.name);
                        ui.label("Interval (minutes):");
                        ui.add(egui::DragValue::new(&mut editor.interval_minutes).range(1..=10080));
                    });

                    ui.checkbox(&mut editor.enabled, "Enabled");
                    ui.label("Prompt:");
                    ui.add(
                        egui::TextEdit::multiline(&mut editor.prompt)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );

                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("Discard Draft")
                                    .color(super::theme::palette(ui).error),
                            )
                            .clicked()
                        {
                            should_remove = true;
                        }
                    });
                });
            }
            ui.add_space(6.0);

            if should_remove {
                self.remove_pending_job(&draft.local_id);
            }
        }

        ui.separator();
        ui.add_space(8.0);
        ui.heading("Add Schedule");
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.new_job_name);
            ui.label("Interval (minutes):");
            ui.add(egui::DragValue::new(&mut self.new_job_interval_minutes).range(1..=10080));
            ui.checkbox(&mut self.new_job_enabled, "Enabled");
        });
        ui.label("Prompt:");
        ui.add(
            egui::TextEdit::multiline(&mut self.new_job_prompt)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        if ui.button("Add Schedule").clicked() {
            let name = self.new_job_name.trim().to_string();
            let prompt = self.new_job_prompt.trim().to_string();
            if name.is_empty() || prompt.is_empty() {
                self.scheduled_jobs_error =
                    Some("Name and prompt are required to create a schedule.".to_string());
            } else {
                self.scheduled_jobs_error = None;
                self.pending_new_scheduled_jobs
                    .push(PendingScheduledJobDraft {
                        local_id: format!("new-schedule-{}", self.next_local_scheduled_job_id),
                        editor: ScheduledJobEditor {
                            name,
                            prompt,
                            interval_minutes: self.new_job_interval_minutes.clamp(1, 10080),
                            enabled: self.new_job_enabled,
                        },
                    });
                self.next_local_scheduled_job_id += 1;
                self.new_job_name.clear();
                self.new_job_prompt.clear();
                self.new_job_interval_minutes = 60;
                self.new_job_enabled = true;
            }
        }
    }

    fn editor_for_job(&mut self, job: &ScheduledJob) -> &mut ScheduledJobEditor {
        self.scheduled_job_editors
            .entry(job.id.clone())
            .or_insert_with(|| ScheduledJobEditor {
                name: job.name.clone(),
                prompt: job.prompt.clone(),
                interval_minutes: job.interval_minutes,
                enabled: job.enabled,
            })
    }

    fn reset_editor_for_job(&mut self, job: &ScheduledJob) {
        self.scheduled_job_editors.insert(
            job.id.clone(),
            ScheduledJobEditor {
                name: job.name.clone(),
                prompt: job.prompt.clone(),
                interval_minutes: job.interval_minutes,
                enabled: job.enabled,
            },
        );
    }

    fn editor_for_pending_job(&mut self, local_id: &str) -> &mut ScheduledJobEditor {
        self.pending_new_scheduled_jobs
            .iter_mut()
            .find(|draft| draft.local_id == local_id)
            .map(|draft| &mut draft.editor)
            .expect("pending scheduled job editor should exist")
    }

    fn remove_pending_job(&mut self, local_id: &str) {
        self.pending_new_scheduled_jobs
            .retain(|draft| draft.local_id != local_id);
    }

    fn queue_dirty_scheduled_job_updates(&mut self) -> bool {
        let mut pending_updates = Vec::new();

        for draft in &self.pending_new_scheduled_jobs {
            let name = draft.editor.name.trim().to_string();
            let prompt = draft.editor.prompt.trim().to_string();
            if name.is_empty() || prompt.is_empty() {
                self.scheduled_jobs_error = Some(
                    "All pending schedules need a name and prompt before you save.".to_string(),
                );
                return false;
            }

            pending_updates.push(ScheduledJobAction::Create {
                name,
                prompt,
                interval_minutes: draft.editor.interval_minutes.clamp(1, 10080),
                enabled: draft.editor.enabled,
            });
        }

        for job_id in &self.pending_deleted_job_ids {
            pending_updates.push(ScheduledJobAction::Delete {
                job_id: job_id.clone(),
            });
        }

        for job in &self.scheduled_jobs {
            if self.pending_deleted_job_ids.contains(&job.id) {
                continue;
            }
            let Some(editor) = self.scheduled_job_editors.get(&job.id) else {
                continue;
            };

            let name = editor.name.trim().to_string();
            let prompt = editor.prompt.trim().to_string();
            let interval_minutes = editor.interval_minutes.clamp(1, 10080);
            let enabled = editor.enabled;

            let is_dirty = name != job.name
                || prompt != job.prompt
                || interval_minutes != job.interval_minutes
                || enabled != job.enabled;
            if !is_dirty {
                continue;
            }

            if name.is_empty() || prompt.is_empty() {
                self.scheduled_jobs_error = Some(format!(
                    "Schedule '{}' has unsaved invalid edits. Name and prompt are required.",
                    job.id
                ));
                return false;
            }

            pending_updates.push(ScheduledJobAction::Update {
                job_id: job.id.clone(),
                name,
                prompt,
                interval_minutes,
                enabled,
            });
        }

        if !pending_updates.is_empty() {
            self.scheduled_jobs_error = None;
            self.scheduled_job_actions.extend(pending_updates);
        }

        true
    }

    fn skill_tabs(&self) -> Vec<PluginSettingsTabManifest> {
        let mut tabs = self
            .plugin_manifests
            .iter()
            .filter_map(|manifest| manifest.settings_tab.clone())
            .collect::<Vec<_>>();
        tabs.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.title.cmp(&right.title))
        });
        tabs
    }

    fn available_tab_ids(&self) -> Vec<String> {
        let mut ids = vec![
            CORE_TAB_GENERAL.to_string(),
            CORE_TAB_APPEARANCE.to_string(),
            CORE_TAB_CONNECTIONS.to_string(),
            CORE_TAB_BEHAVIOR.to_string(),
            CORE_TAB_LOOPS.to_string(),
            CORE_TAB_MEMORY.to_string(),
            CORE_TAB_SYSTEM.to_string(),
            CORE_TAB_SCHEDULES.to_string(),
        ];
        ids.extend(self.skill_tabs().into_iter().map(|tab| tab.id));
        ids
    }

    fn ensure_valid_selected_tab(&mut self) {
        if !self
            .available_tab_ids()
            .iter()
            .any(|tab_id| tab_id == &self.selected_tab)
        {
            self.selected_tab = CORE_TAB_GENERAL.to_string();
        }
    }

    fn dynamic_plugin_schema_for_tab(
        &self,
        tab_id: &str,
    ) -> Option<(String, PluginSettingsSchemaManifest)> {
        self.plugin_manifests.iter().find_map(|manifest| {
            let matches_tab = manifest
                .settings_tab
                .as_ref()
                .map(|tab| tab.id == tab_id)
                .unwrap_or(false);
            if !matches_tab {
                return None;
            }
            manifest
                .settings_schema
                .clone()
                .map(|schema| (manifest.id.clone(), schema))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deliberate_loose_action_updates_saved_fields_without_erasing_theme_draft() {
        let mut panel = SettingsPanel::new(AgentConfig::default());
        panel.config.appearance.base_color = [140, 110, 220];
        panel.sync_loose_action(true);
        assert!(panel.config.loose_mode);
        assert!(panel.config.enable_ambient_loop);
        assert!(panel.has_unsaved_changes());
        panel.revert_drafts();
        assert!(panel.config.loose_mode);
        assert!(panel.config.enable_ambient_loop);
        assert!(!panel.has_unsaved_changes());
        panel.sync_loose_action(false);
        assert!(!panel.has_unsaved_changes());
    }
    #[test]
    fn provider_switch_preserves_unsaved_non_provider_fields() {
        let mut panel = SettingsPanel::new(AgentConfig::default());
        panel.config.username = "Unsaved identity".into();
        panel.config.relationship_description = "Unsaved relationship".into();
        panel.config.appearance.base_color = [140, 110, 220];
        let selected = AgentConfig {
            llm_model: "ponderer-local-gguf".into(),
            llm_api_url: "http://127.0.0.1:1234/v1".into(),
            reflection_model: Some("reflection-fixture".into()),
            ..Default::default()
        };
        panel.sync_provider_from_config(&selected);
        assert_eq!(panel.config.username, "Unsaved identity");
        assert_eq!(
            panel.config.relationship_description,
            "Unsaved relationship"
        );
        assert_eq!(panel.config.llm_model, selected.llm_model);
        assert_eq!(panel.config.llm_api_url, selected.llm_api_url);
        assert_eq!(panel.config.reflection_model, selected.reflection_model);
        assert_eq!(panel.config.appearance.base_color, [140, 110, 220]);
        assert!(panel.has_unsaved_changes());
        panel.revert_drafts();
        assert_eq!(panel.config.llm_model, selected.llm_model);
        assert_eq!(
            panel.config.appearance,
            crate::config::AppearanceConfig::default()
        );
        assert!(!panel.has_unsaved_changes());
    }

    #[test]
    fn appearance_draft_reverts_and_saved_palette_survives_sync() {
        let mut panel = SettingsPanel::new(AgentConfig::default());
        assert!(!panel.has_unsaved_changes());
        panel.config.appearance.base_color = [200, 130, 80];
        panel.config.appearance.dark = false;
        assert!(panel.has_unsaved_changes());
        let saved = panel.config.clone();
        panel.sync_from_config(saved.clone());
        assert!(!panel.has_unsaved_changes());
        panel.config.appearance.base_color = [0, 0, 0];
        panel.save_error = Some("fixture failure".into());
        panel.revert_drafts();
        assert_eq!(panel.config.appearance, saved.appearance);
        assert!(panel.save_error.is_none());
    }

    #[test]
    fn failed_save_drops_mutations_but_preserves_schedule_editors() {
        let mut panel = SettingsPanel::new(AgentConfig::default());
        panel.scheduled_job_actions = vec![
            ScheduledJobAction::Refresh,
            ScheduledJobAction::Delete {
                job_id: "fixture".into(),
            },
        ];
        panel.pending_deleted_job_ids.insert("fixture".into());
        assert!(panel.has_unsaved_changes());
        panel.discard_queued_schedule_changes();
        assert!(matches!(
            panel.take_scheduled_job_actions().as_slice(),
            [ScheduledJobAction::Refresh]
        ));
        assert!(panel.pending_deleted_job_ids.contains("fixture"));
        assert!(panel.queue_dirty_scheduled_job_updates());
        assert!(matches!(
            panel.take_scheduled_job_actions().as_slice(),
            [ScheduledJobAction::Delete { .. }]
        ));
        panel.revert_drafts();
        assert!(!panel.has_unsaved_changes());
    }
}
