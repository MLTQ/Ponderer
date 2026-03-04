use super::comfy_settings::ComfySettingsPanel;
use super::orbweaver_settings::OrbWeaverSettingsPanel;
use super::plugin_settings_form::PluginSettingsForm;
use crate::api::{BackendPluginManifest, PluginSettingsSchemaManifest, PluginSettingsTabManifest};
use crate::config::AgentConfig;
use eframe::egui;

const CORE_TAB_GENERAL: &str = "core.general";
const CORE_TAB_BEHAVIOR: &str = "core.behavior";
const CORE_TAB_LOOPS: &str = "core.loops";
const CORE_TAB_MEMORY: &str = "core.memory";
const CORE_TAB_SYSTEM: &str = "core.system";

pub struct SettingsPanel {
    pub config: AgentConfig,
    pub show: bool,
    selected_tab: String,
    plugin_manifests: Vec<BackendPluginManifest>,
    comfy_panel: ComfySettingsPanel,
    orbweaver_panel: OrbWeaverSettingsPanel,
}

impl SettingsPanel {
    pub fn new(config: AgentConfig) -> Self {
        let mut comfy_panel = ComfySettingsPanel::new();
        comfy_panel.load_workflow_from_config(&config);

        Self {
            config,
            show: false,
            selected_tab: CORE_TAB_GENERAL.to_string(),
            plugin_manifests: Vec::new(),
            comfy_panel,
            orbweaver_panel: OrbWeaverSettingsPanel::new(),
        }
    }

    pub fn set_plugin_manifests(&mut self, plugin_manifests: Vec<BackendPluginManifest>) {
        self.plugin_manifests = plugin_manifests;
        self.ensure_valid_selected_tab();
    }

    pub fn sync_from_config(&mut self, config: AgentConfig) {
        self.config = config.clone();
        self.comfy_panel.load_workflow_from_config(&config);
    }

    pub fn open(&mut self) {
        self.show = true;
        self.ensure_valid_selected_tab();
    }

    pub fn open_tab(&mut self, tab_id: &str) {
        self.show = true;
        if self.available_tab_ids().iter().any(|id| id == tab_id) {
            self.selected_tab = tab_id.to_string();
        } else {
            self.selected_tab = CORE_TAB_GENERAL.to_string();
        }
    }

    pub fn render(&mut self, ctx: &egui::Context) -> Option<AgentConfig> {
        if !self.show {
            return None;
        }

        self.ensure_valid_selected_tab();

        let mut new_config = None;
        let mut should_close = false;
        let mut is_open = self.show;

        egui::Window::new("⚙ Settings")
            .open(&mut is_open)
            .default_width(760.0)
            .default_height(640.0)
            .show(ctx, |ui| {
                self.render_tab_bar(ui);
                let selected_tab = self.selected_tab.clone();
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("settings_tab_scroll")
                    .show(ui, |ui| match selected_tab.as_str() {
                        CORE_TAB_GENERAL => self.render_general_tab(ui),
                        CORE_TAB_BEHAVIOR => self.render_behavior_tab(ui),
                        CORE_TAB_LOOPS => self.render_loops_tab(ui),
                        CORE_TAB_MEMORY => self.render_memory_tab(ui),
                        CORE_TAB_SYSTEM => self.render_system_tab(ui),
                        "skill.orbweaver" => {
                            let (orbweaver_panel, config) =
                                (&mut self.orbweaver_panel, &mut self.config);
                            orbweaver_panel.render_contents(ui, config);
                        }
                        "skill.comfy" => self.render_comfy_tab(ui, ctx),
                        _ => {
                            if let Some((plugin_id, schema)) =
                                self.dynamic_plugin_schema_for_tab(&selected_tab)
                            {
                                ui.heading("Plugin Settings");
                                ui.add_space(8.0);
                                PluginSettingsForm::render(
                                    ui,
                                    &mut self.config,
                                    &plugin_id,
                                    &schema,
                                );
                            } else {
                                ui.label(
                                    "Settings for this plugin are not available in this build.",
                                );
                            }
                        }
                    });

                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.button("💾 Save & Apply").clicked() {
                        let (comfy_panel, config) = (&mut self.comfy_panel, &mut self.config);
                        comfy_panel.sync_workflow_to_config(config);
                        new_config = Some(self.config.clone());
                    }

                    if ui.button("Close").clicked() {
                        should_close = true;
                    }
                });
            });

        self.show = is_open && !should_close;
        new_config
    }

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (tab_id, label) in [
                (CORE_TAB_GENERAL, "General"),
                (CORE_TAB_BEHAVIOR, "Behavior"),
                (CORE_TAB_LOOPS, "Living Loop"),
                (CORE_TAB_MEMORY, "Memory"),
                (CORE_TAB_SYSTEM, "System"),
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

    fn render_general_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("LLM Configuration");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("API URL:");
            ui.text_edit_singleline(&mut self.config.llm_api_url);
        });
        ui.label("Example: http://localhost:11434 (Ollama)");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Model:   ");
            ui.text_edit_singleline(&mut self.config.llm_model);
        });
        ui.label("Example: llama3.2, qwen2.5, mistral");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("API Key: ");
            let mut key_str = self.config.llm_api_key.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut key_str).changed() {
                self.config.llm_api_key = if key_str.is_empty() {
                    None
                } else {
                    Some(key_str)
                };
            }
        });
        ui.label("Optional - only needed for OpenAI/Claude");
        ui.add_space(16.0);

        ui.separator();
        ui.heading("Agent Identity");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Username:");
            ui.text_edit_singleline(&mut self.config.username);
        });
        ui.label("Name displayed in posts");
        ui.add_space(16.0);

        ui.separator();
        ui.heading("Telegram Bot");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Bot token:    ");
            let mut token_str = self.config.telegram_bot_token.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut token_str).changed() {
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
                "Optional but recommended — restricts the bot to your account only.\n\
                 To find your ID: message the bot, then open\n\
                 https://api.telegram.org/bot<TOKEN>/getUpdates",
            )
            .small()
            .weak(),
        );
        ui.label(
            egui::RichText::new("Telegram settings take effect after restart.")
                .small()
                .color(egui::Color32::from_rgb(200, 180, 100)),
        );
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
                "Foreground continuation follows turn_control; limit is optional safety cap.",
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
                "Background continuation follows turn_control; limit is optional safety cap.",
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

        ui.label("Guiding principles (one per line):");
        let mut principles_text = self.config.guiding_principles.join("\n");
        if ui.text_edit_multiline(&mut principles_text).changed() {
            self.config.guiding_principles = principles_text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
                .collect();
        }
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

    fn render_comfy_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("ComfyUI Integration");
        ui.add_space(8.0);

        ui.checkbox(
            &mut self.config.enable_image_generation,
            "Enable image generation",
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("ComfyUI URL:");
            ui.text_edit_singleline(&mut self.config.comfyui.api_url);
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Workflow type:");
            egui::ComboBox::from_id_salt("workflow_type")
                .selected_text(&self.config.comfyui.workflow_type)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.config.comfyui.workflow_type,
                        "sd".to_string(),
                        "Stable Diffusion 1.5",
                    );
                    ui.selectable_value(
                        &mut self.config.comfyui.workflow_type,
                        "sdxl".to_string(),
                        "SDXL",
                    );
                    ui.selectable_value(
                        &mut self.config.comfyui.workflow_type,
                        "flux".to_string(),
                        "Flux",
                    );
                });
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Model name:");
            ui.text_edit_singleline(&mut self.config.comfyui.model_name);
        });
        ui.add_space(12.0);

        ui.separator();
        let (comfy_panel, config) = (&mut self.comfy_panel, &mut self.config);
        let _ = comfy_panel.render_contents(ui, ctx, config);
    }

    fn skill_tabs(&self) -> Vec<PluginSettingsTabManifest> {
        let mut tabs = self
            .plugin_manifests
            .iter()
            .filter_map(|manifest| manifest.settings_tab.clone())
            .collect::<Vec<_>>();
        if !tabs.iter().any(|tab| tab.id == "skill.comfy") {
            tabs.push(PluginSettingsTabManifest {
                id: "skill.comfy".to_string(),
                title: "ComfyUI".to_string(),
                order: 200,
            });
        }
        if !tabs.iter().any(|tab| tab.id == "skill.orbweaver") {
            tabs.push(PluginSettingsTabManifest {
                id: "skill.orbweaver".to_string(),
                title: "OrbWeaver".to_string(),
                order: 210,
            });
        }
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
            CORE_TAB_BEHAVIOR.to_string(),
            CORE_TAB_LOOPS.to_string(),
            CORE_TAB_MEMORY.to_string(),
            CORE_TAB_SYSTEM.to_string(),
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
            if !matches_tab || tab_id == "skill.comfy" || tab_id == "skill.orbweaver" {
                return None;
            }
            manifest
                .settings_schema
                .clone()
                .map(|schema| (manifest.id.clone(), schema))
        })
    }
}
