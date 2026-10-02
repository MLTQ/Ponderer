use crate::config::AgentConfig;
use eframe::egui;
use std::path::PathBuf;

pub struct CharacterPanel {
    pub config: AgentConfig,
    pub show: bool,
    avatar_texture: Option<egui::TextureHandle>,
    import_error: Option<String>,
}

fn render_mood_avatar_row(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    ui.horizontal(|ui| {
        ui.label(label);

        let mut path = value.clone().unwrap_or_default();
        if ui
            .add_sized([340.0, 22.0], egui::TextEdit::singleline(&mut path))
            .changed()
        {
            *value = if path.trim().is_empty() {
                None
            } else {
                Some(path)
            };
        }

        if ui.button("Browse").clicked() {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Avatar Image", &["png", "jpg", "jpeg", "gif"])
                .pick_file()
            {
                *value = Some(file.to_string_lossy().to_string());
            }
        }

        if ui.button("Clear").clicked() {
            *value = None;
        }
    });
}

impl CharacterPanel {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            show: false,
            avatar_texture: None,
            import_error: None,
        }
    }

    pub fn render_contents(&mut self, ui: &mut egui::Ui) -> Option<AgentConfig> {
        let ctx = ui.ctx().clone();

        let mut new_config = None;
        let mut import_path: Option<PathBuf> = None;
        let mut should_clear = false;
        let mut should_save = false;

        // Build system prompt preview outside the closure to avoid borrowing issues
        let system_prompt_preview = self.build_system_prompt_preview();

        egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Identity");
                    ui.small("Operator-owned boundaries are separate from model-reported reflections.");
                    ui.horizontal(|ui| {
                        ui.label("Agent name:");
                        ui.text_edit_singleline(&mut self.config.username);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Your name:");
                        ui.text_edit_singleline(&mut self.config.operator_name);
                    });
                    ui.label("Relationship context:");
                    ui.add(egui::TextEdit::multiline(&mut self.config.relationship_description).desired_width(f32::INFINITY));
                    ui.label("Fixed boundaries (one per line):");
                    let mut boundaries = self.config.identity_boundaries.join("\n");
                    if ui.add(egui::TextEdit::multiline(&mut boundaries).desired_width(f32::INFINITY)).changed() {
                        self.config.identity_boundaries = boundaries.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect();
                    }
                    ui.label("Guiding principles (one per line):");
                    let mut principles = self.config.guiding_principles.join("\n");
                    if ui.add(egui::TextEdit::multiline(&mut principles).desired_width(f32::INFINITY)).changed() {
                        self.config.guiding_principles = principles.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect();
                    }
                    ui.separator();
                    // Avatar and Import Section
                    ui.horizontal(|ui| {
                        // Show avatar thumbnail if available
                        if let Some(ref texture) = self.avatar_texture {
                            ui.image((texture.id(), egui::vec2(128.0, 128.0)));
                        } else if self.config.character_avatar_path.is_some() {
                            // Try to load the avatar
                            if let Some(ref path) = self.config.character_avatar_path {
                                if let Ok(img) = image::open(path) {
                                    let rgba = img.to_rgba8();
                                    let size = [rgba.width() as usize, rgba.height() as usize];
                                    let pixels = rgba.into_raw();
                                    let color_image =
                                        egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                                    let texture = ctx.load_texture(
                                        "character_avatar",
                                        color_image,
                                        Default::default(),
                                    );
                                    ui.image((texture.id(), egui::vec2(128.0, 128.0)));
                                    self.avatar_texture = Some(texture);
                                }
                            }
                        } else {
                            // Placeholder
                            ui.vertical(|ui| {
                                ui.set_width(128.0);
                                ui.set_height(128.0);
                                ui.centered_and_justified(|ui| {
                                    ui.label("No Avatar");
                                });
                            });
                        }

                        ui.vertical(|ui| {
                            ui.heading("Character card");
                            ui.label("Drop a PNG character card here or click to browse");

                            if ui.button("📁 Browse for Character Card PNG").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PNG Image", &["png"])
                                    .pick_file()
                                {
                                    import_path = Some(path);
                                }
                            }

                            if let Some(ref error) = self.import_error {
                                ui.colored_label(super::theme::palette(ui).error, format!("Error: {}", error));
                            }
                        });
                    });

                    ui.add_space(16.0);
                    ui.separator();

                    // Character Details
                    ui.heading("Character Details");
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.config.character_name);
                    });
                    ui.add_space(4.0);

                    ui.label("Description:");
                    ui.text_edit_multiline(&mut self.config.character_description);
                    ui.add_space(4.0);

                    ui.label("Personality:");
                    ui.text_edit_multiline(&mut self.config.character_personality);
                    ui.add_space(4.0);

                    ui.label("Scenario:");
                    ui.text_edit_multiline(&mut self.config.character_scenario);
                    ui.add_space(4.0);

                    ui.label("Example Dialogue:");
                    ui.text_edit_multiline(&mut self.config.character_example_dialogue);
                    ui.label("Character instructions:");
                    ui.text_edit_multiline(&mut self.config.character_system_prompt);
                    ui.add_space(16.0);

                    ui.separator();
                    ui.heading("Execution-state artwork");
                    ui.add_space(8.0);

                    render_mood_avatar_row(ui, "Idle:", &mut self.config.avatar_idle);
                    render_mood_avatar_row(ui, "Thinking:", &mut self.config.avatar_thinking);
                    render_mood_avatar_row(ui, "Active:", &mut self.config.avatar_active);

                    ui.label(
                        egui::RichText::new(
                            "Used for sprite states: idle/paused, thinking/reading/confused, writing/happy.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(16.0);

                    ui.separator();
                    ui.add_space(8.0);

                    // Preview system prompt
                    ui.collapsing("Preview System Prompt", |ui| {
                        ui.label(&system_prompt_preview);
                    });

                    ui.add_space(8.0);

                    // Save/Cancel buttons
                    ui.horizontal(|ui| {
                        if ui.button("Save identity & configuration").clicked() {
                            should_save = true;
                        }

                        if ui.button("Clear Character").clicked() {
                            should_clear = true;
                        }

                    });
                });

        // Handle save after the window is closed to avoid borrowing issues
        if should_save {
            new_config = Some(self.config.clone());
        }

        // Handle import after the window is closed to avoid borrowing issues
        if let Some(path) = import_path {
            self.import_character_card(path);
        }

        // Handle clear after the window is closed
        if should_clear {
            self.config.normalize_character_prompt();
            self.config.character_name.clear();
            self.config.character_description.clear();
            self.config.character_personality.clear();
            self.config.character_scenario.clear();
            self.config.character_example_dialogue.clear();
            self.config.character_system_prompt.clear();
            self.config.character_avatar_path = None;
            self.avatar_texture = None;
        }

        // Handle drag-and-drop
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(file) = i.raw.dropped_files.first() {
                    if let Some(ref path) = file.path {
                        self.import_character_card(path.clone());
                    }
                }
            }
        });

        new_config
    }

    fn import_character_card(&mut self, path: PathBuf) {
        self.import_error = None;

        // Try to parse the character card
        match crate::character_card::parse_character_card(&path) {
            Ok((parsed, _format, _raw)) => {
                self.config.normalize_character_prompt();
                // Update config with parsed data
                self.config.character_name = parsed.name;
                self.config.character_description = parsed.description;
                self.config.character_personality = parsed.personality;
                self.config.character_scenario = parsed.scenario;
                self.config.character_example_dialogue = parsed.example_dialogue;
                self.config.character_system_prompt = parsed.system_prompt;

                // Store avatar path
                self.config.character_avatar_path = Some(path.to_string_lossy().to_string());

                // Clear texture to force reload
                self.avatar_texture = None;

                tracing::info!(
                    "Successfully imported character card: {}",
                    self.config.character_name
                );
            }
            Err(e) => {
                self.import_error = Some(format!("Failed to parse character card: {}", e));
                tracing::error!("Character card import error: {}", e);
            }
        }
    }

    fn build_system_prompt_preview(&self) -> String {
        self.config.identity_context()
    }
}
