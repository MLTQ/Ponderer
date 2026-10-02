//! Native egui workspaces. The instrument rail is independent of navigation and approvals.
use super::{
    conversation_display_label, format_elapsed, render_live_tool_entry, AgentApp, LiveToolProgress,
    Workspace,
};
use crate::api::AgentVisualState;
use crate::ui::{chat, theme, token_monitor};
use eframe::egui::{self, RichText};

fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .monospace()
            .small()
            .color(theme::palette(ui).accent),
    );
}

impl AgentApp {
    pub(super) fn select_workspace(&mut self, workspace: Workspace) {
        if self.workspace == workspace {
            return;
        }
        self.workspace = workspace;
        self.settings_panel.show = workspace == Workspace::Settings;
        self.affect_lab.show = workspace == Workspace::AffectLab;
        self.character_panel.show = workspace == Workspace::Identity;
        if workspace == Workspace::Settings
            && !self.ui_snapshot
            && !self.settings_panel.has_unsaved_schedule_changes()
        {
            self.refresh_scheduled_jobs();
        }
    }

    pub(super) fn render_workbench(
        &mut self,
        ctx: &egui::Context,
    ) -> (Option<String>, Option<String>) {
        let mut approve = None;
        let mut dismiss = None;
        egui::TopBottomPanel::top("workbench_chrome").show(ctx, |ui| {
            let colors = theme::palette(ui);
            ui.horizontal_wrapped(|ui| {
                crate::ui::sprite::render_agent_sprite(ui, &self.current_state, self.avatars.as_mut());
                ui.label(RichText::new(self.settings_panel.config.character_display_name().to_uppercase()).monospace().size(16.0));
                ui.separator();
                if self.ui_snapshot {ui.small("Isolated snapshot · synthetic data");}
                ui.monospace(format!("{:?}", self.current_state));
                if let Some(since) = self.visual_state_since {
                    let elapsed = chrono::Utc::now().signed_duration_since(since).num_seconds().max(0) as u64;
                    ui.label(RichText::new(format_elapsed(elapsed)).monospace().small().color(if elapsed > 30 {colors.warning} else {colors.muted}));
                }
                let paused = self.current_state == AgentVisualState::Paused;
                if ui.button(if paused { "Resume loop" } else { "Pause loop" }).clicked() {
                    match self.runtime.block_on(self.api_client.toggle_pause()) {
                        Ok(paused) => self.current_state = if paused {AgentVisualState::Paused} else {AgentVisualState::Idle},
                        Err(error) => self.push_ui_error(format!("Failed to toggle pause: {error}")),
                    }
                }
                if ui.button(RichText::new("Stop turn").color(colors.error)).clicked() {
                    match self.runtime.block_on(self.api_client.stop_agent_turn()) {
                        Ok(_) => {
                            let active = self.active_conversation_id.clone();
                            self.streaming_chat_preview = None;
                            self.clear_live_tool_progress(&active);
                            self.refresh_conversations();
                            self.refresh_chat_history();
                            self.current_state = AgentVisualState::Idle;
                        }
                        Err(error) => self.push_ui_error(format!("Failed to stop active turn: {error}")),
                    }
                }
                if self.loose_mode {
                    if ui.button(RichText::new("Disarm self-directed").color(colors.warning)).clicked() {
                        match self.runtime.block_on(self.api_client.set_loose_mode(false)) {
                            Ok(enabled) => {self.loose_mode = enabled; self.settings_panel.sync_loose_action(enabled); self.current_state = AgentVisualState::Idle;}
                            Err(error) => self.push_ui_error(format!("Failed to disarm self-directed work: {error}")),
                        }
                    }
                } else {
                    ui.menu_button("Permissions", |ui| {
                        ui.small("Reflection does not expand tool permissions.");
                        ui.label("Self-directed (Loose) permits local shell and filesystem work without routine approval. External and identity gates remain.");
                        if ui.button("Arm self-directed work").clicked() {
                            self.show_loose_arm_confirmation = true;
                            ui.close_menu();
                        }
                        if ui.button("Autonomy & contact settings").clicked() {
                            self.select_workspace(Workspace::Settings);
                            self.settings_panel.open_tab("core.loops");
                            ui.close_menu();
                        }
                    });
                }
            });
            if let Some(activity) = &self.current_activity {ui.small(activity);}
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for (workspace, title) in Workspace::ALL {
                    if ui.selectable_label(self.workspace == workspace, title).clicked() {self.select_workspace(workspace);}
                }
                if self.settings_panel.has_unsaved_changes() {ui.colored_label(colors.warning, "UNSAVED DRAFT");}
                if let Some(error) = &self.settings_panel.save_error {ui.colored_label(colors.error, error);}
            });
        });

        // Never place approvals inside a hideable panel or behind a workspace tab.
        if !self.pending_approvals.is_empty() {
            egui::TopBottomPanel::top("workbench_approvals").show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for (tool, reason) in &self.pending_approvals {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("Approval: {tool}"))
                                        .monospace()
                                        .color(theme::palette(ui).warning),
                                );
                                if ui.button("Allow this session").clicked() {
                                    approve = Some(tool.clone());
                                }
                                if ui.button("Dismiss").clicked() {
                                    dismiss = Some(tool.clone());
                                }
                            });
                            ui.add(egui::Label::new(reason).wrap());
                        }
                    });
            });
        }

        egui::TopBottomPanel::bottom("workbench_footer").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.small(format!("Model: {}", self.settings_panel.config.llm_model));
            });
        });
        egui::TopBottomPanel::bottom(if self.show_event_tape {
            "event_tape_open"
        } else {
            "event_tape_closed"
        })
        .resizable(self.show_event_tape)
        .default_height(if self.show_event_tape { 150.0 } else { 32.0 })
        .height_range(if self.show_event_tape {
            70.0..=220.0
        } else {
            24.0..=40.0
        })
        .show(ctx, |ui| {
            if self.show_event_tape {
                ui.set_min_height(ui.available_height());
            }
            ui.horizontal(|ui| {
                label(ui, "EVENT TAPE");
                if ui
                    .small_button(if self.show_event_tape {
                        "Collapse"
                    } else {
                        "Expand"
                    })
                    .clicked()
                {
                    self.show_event_tape = !self.show_event_tape;
                }
                ui.small(format!("{} recorded events", self.events.len()));
            });
            if self.show_event_tape {
                chat::render_event_log(ui, &self.events, &mut self.event_detail_popup);
            }
        });

        egui::SidePanel::right("instrument_rail").resizable(true).default_width(320.0)
            .width_range(250.0..=520.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("instrument_rail_scroll").show(ui, |ui| {
                    label(ui, "Novelty");
                    token_monitor::render(ui, &mut self.token_monitor);
                    ui.small(format!("{} samples · {} paths · latest {:.3}", self.token_monitor.trace_len(), self.token_monitor.path_count(), self.token_monitor.last_novelty()));
                    if let Some(readout) = self.token_monitor.latest_readout() {
                        egui::Grid::new("token_readout").num_columns(2).show(ui, |ui| {
                            ui.small("selected novelty");ui.monospace(format!("{:.3}",readout.novelty));ui.end_row();
                            ui.small("logprob");ui.monospace(readout.logprob.map(|n|format!("{n:.3}")).unwrap_or_else(||"n/a".into()));ui.end_row();
                            ui.small("entropy");ui.monospace(readout.entropy.map(|n|format!("{n:.3}")).unwrap_or_else(||"n/a".into()));ui.end_row();
                        });
                        ui.small(format!("Metric: {}", readout.metric_source));
                        ui.small(format!("Generation: {}", readout.source));
                    } else {ui.small("No token samples yet.");}
                    egui::CollapsingHeader::new("About this plot").show(ui, |ui| {
                        ui.small("A deterministic novelty walk, not a projection of hidden activations. Lexical fallback is used when provider probabilities are missing.");
                        ui.small("Drag to orbit, scroll to zoom, double-click to reset. Camera motion does not add samples.");
                    });
                    ui.separator();
                    self.render_intention(ui);
                    ui.separator();
                    label(ui, "Journal");
                    if let Some(journal) = &self.last_journal {ui.add(egui::Label::new(journal).wrap());} else {ui.weak("No journal entry received in this session.");}
                    if let Some(action) = &self.last_action {ui.separator();label(ui,"Last action");ui.add(egui::Label::new(action).wrap());}
                    egui::CollapsingHeader::new("Raw stream").show(ui, |ui| {
                        self.raw_feed.render(ui);
                    });
                });
            });

        let mut config_to_save = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 9.0;
            match self.workspace {
                Workspace::Conversation => self.render_conversation(ui),
                Workspace::Mind => self.render_mind(ui),
                Workspace::Identity => {
                    // One shared draft: switching workspaces cannot overwrite unsaved appearance/provider edits.
                    self.character_panel.config = self.settings_panel.config.clone();
                    config_to_save = self.character_panel.render_contents(ui);
                    self.settings_panel.config = self.character_panel.config.clone();
                }
                Workspace::AffectLab => {
                    self.affect_lab
                        .render_contents(ui, &self.api_client, &self.runtime)
                }
                Workspace::Settings => {
                    config_to_save = self.settings_panel.render_contents(
                        ui,
                        !self.affect_lab.provider_change_pending(),
                        |ui, config| {
                            self.affect_lab.render_connection(
                                ui,
                                config,
                                &self.api_client,
                                &self.runtime,
                            );
                        },
                    );
                }
            }
        });
        if let Some(config) = config_to_save {
            self.persist_config(config);
        }
        if self.affect_lab.open_settings {
            self.affect_lab.open_settings = false;
            self.select_workspace(Workspace::Settings);
            self.settings_panel.open_tab("core.general");
        } else if self.affect_lab.show && self.workspace == Workspace::Settings {
            self.select_workspace(Workspace::AffectLab);
        }
        (approve, dismiss)
    }

    fn render_intention(&self, ui: &mut egui::Ui) {
        label(ui, if self.loose_mode { "Goal" } else { "Intention" });
        if let Some(intention) = &self.current_intention {
            ui.add(egui::Label::new(&intention.summary).wrap());
            ui.small(format!(
                "{} · {} attempts",
                intention.status, intention.attempt_count
            ));
            ui.add(egui::Label::new(format!("Reason: {}", intention.motivation)).wrap());
            if let Some(outcome) = &intention.last_outcome {
                ui.add(egui::Label::new(format!("Outcome: {outcome}")).wrap());
            }
        } else {
            ui.weak("No durable intention reported.");
        }
    }

    fn render_mind(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mind");
        egui::ScrollArea::vertical()
            .id_salt("mind_workspace")
            .show(ui, |ui| {
                self.render_intention(ui);
                if let Some(orientation) = &self.last_orientation {
                    ui.separator();
                    label(ui, "Orientation");
                    ui.label(&orientation.disposition);
                    ui.small(format!("{} anomalies", orientation.anomaly_count));
                }
                ui.separator();
                label(ui, "LATEST REFLECTION");
                if let Some(journal) = &self.last_journal {
                    ui.add(egui::Label::new(journal).wrap());
                } else {
                    ui.weak("No journal entry received in this session.");
                }
                if let Some(action) = &self.last_action {
                    ui.separator();
                    label(ui, "LAST ACTION");
                    ui.add(egui::Label::new(action).wrap());
                }
                ui.separator();
                label(ui, "RECORDED ACTIVITY");
                chat::render_event_log(ui, &self.events, &mut self.event_detail_popup);
            });
    }

    fn render_conversation(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            label(ui, "CONVERSATION");
            let previous = self.active_conversation_id.clone();
            let selected = self
                .conversations
                .iter()
                .find(|conversation| conversation.id == self.active_conversation_id)
                .map(conversation_display_label)
                .unwrap_or_else(|| "Default chat".into());
            egui::ComboBox::from_id_salt("chat_conversation_picker")
                .width(240.0)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for conversation in &self.conversations {
                        ui.selectable_value(
                            &mut self.active_conversation_id,
                            conversation.id.clone(),
                            conversation_display_label(conversation),
                        );
                    }
                });
            if ui.button("New chat").clicked() {
                self.create_new_conversation();
            }
            ui.menu_button("Conversation actions", |ui| {
                if ui.button("Rename").clicked() {
                    let title = self
                        .conversations
                        .iter()
                        .find(|conversation| conversation.id == self.active_conversation_id)
                        .map(|conversation| conversation.title.clone())
                        .unwrap_or_default();
                    self.rename_conversation = Some((self.active_conversation_id.clone(), title));
                    ui.close_menu();
                }
                if ui
                    .button(RichText::new("Delete").color(theme::palette(ui).error))
                    .clicked()
                {
                    self.confirm_delete_conversation_id = Some(self.active_conversation_id.clone());
                    ui.close_menu();
                }
            });
            if self.active_conversation_id != previous {
                self.streaming_chat_preview = None;
                self.refresh_chat_history();
            }
        });
        ui.separator();
        let preview = self
            .streaming_chat_preview
            .as_ref()
            .filter(|preview| preview.conversation_id == self.active_conversation_id)
            .map(|preview| preview.content.clone());
        let progress: Vec<LiveToolProgress> = self
            .live_tool_progress
            .iter()
            .filter(|entry| entry.conversation_id == self.active_conversation_id)
            .cloned()
            .collect();
        // Explicit regions keep growing tool text/media from moving the composer.
        let region = ui.available_rect_before_wrap();
        let composer_height = 108.0_f32.min(region.height());
        let composer_rect = egui::Rect::from_min_max(
            egui::pos2(region.left(), region.bottom() - composer_height),
            region.max,
        );
        let progress_height = if progress.is_empty() {
            0.0
        } else {
            ((region.height() - composer_height - 16.0).max(0.0) * 0.45).min(180.0)
        };
        let progress_bottom = composer_rect.top() - 8.0;
        let progress_rect = egui::Rect::from_min_max(
            egui::pos2(region.left(), progress_bottom - progress_height),
            egui::pos2(region.right(), progress_bottom),
        );
        let chat_bottom = if progress.is_empty() {
            composer_rect.top() - 8.0
        } else {
            progress_rect.top() - 8.0
        };
        let chat_rect = egui::Rect::from_min_max(
            region.min,
            egui::pos2(region.right(), chat_bottom.max(region.top())),
        );
        let layout = egui::Layout::top_down(egui::Align::Min);
        if chat_rect.height() > 0.0 {
            let mut chat_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("chat_region")
                    .max_rect(chat_rect)
                    .layout(layout),
            );
            chat_ui.set_clip_rect(chat_rect.intersect(ui.clip_rect()));
            if let Some(turn) = chat::render_private_chat(
                &mut chat_ui,
                &self.chat_history,
                preview.as_deref(),
                &mut self.chat_media_cache,
            ) {
                self.open_prompt_inspector_for_turn(&turn);
            }
        }
        if !progress.is_empty() && progress_height > 0.0 {
            let mut progress_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("progress_region")
                    .max_rect(progress_rect)
                    .layout(layout),
            );
            progress_ui.set_clip_rect(progress_rect.intersect(ui.clip_rect()));
            egui::CollapsingHeader::new("Live tools")
                .id_salt("live_agent_turn")
                .default_open(true)
                .show(&mut progress_ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height().max(0.0))
                        .stick_to_bottom(true)
                        .id_salt("live_turn_scroll")
                        .show(ui, |ui| {
                            for entry in &progress {
                                render_live_tool_entry(ui, entry);
                            }
                        });
                });
        }
        let mut composer_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("composer_region")
                .max_rect(composer_rect)
                .layout(layout),
        );
        composer_ui.set_clip_rect(composer_rect.intersect(ui.clip_rect()));
        composer_ui.separator();
        composer_ui.horizontal(|ui| {
            let response = ui.add_sized(
                [(ui.available_width() - 75.0).max(80.0), 64.0],
                egui::TextEdit::multiline(&mut self.user_input)
                    .hint_text("Continue the thread...")
                    .desired_rows(3),
            );
            if response.changed() {
                self.token_monitor.on_human_interaction();
            }
            let shortcut = response.has_focus()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Enter)
                        && !input.modifiers.shift
                        && !input.modifiers.ctrl
                        && !input.modifiers.command
                        && !input.modifiers.alt
                });
            let clicked = ui
                .button("Send")
                .on_hover_text("Enter to send; Shift+Enter for a new line.")
                .clicked();
            if (shortcut || clicked) && !self.user_input.trim().is_empty() {
                let message = self.user_input.trim().to_string();
                self.streaming_chat_preview = None;
                self.send_chat_message(&message);
                self.user_input.clear();
            }
        });
        ui.allocate_rect(region, egui::Sense::hover());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentConfig;

    fn frame(
        app: &mut AgentApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.render_snapshot(ctx),
        )
    }

    fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect, egui::Rect)> {
        fn collect(
            shape: &egui::Shape,
            clip: egui::Rect,
            result: &mut Vec<(String, egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::Shape::Text(text) => {
                    result.push((text.galley.text().into(), text.visual_bounding_rect(), clip))
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, clip, result)
                    }
                }
                _ => {}
            }
        }
        let mut result = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, shape.clip_rect, &mut result);
        }
        result
    }

    fn click(app: &mut AgentApp, ctx: &egui::Context, size: egui::Vec2, title: &str) {
        let output = frame(app, ctx, size, Vec::new());
        let (_, rect, clip) = texts(&output)
            .into_iter()
            .find(|(text, rect, clip)| text == title && rect.intersects(*clip))
            .unwrap_or_else(|| panic!("missing control {title}"));
        let pos = rect.intersect(clip).center();
        frame(app, ctx, size, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                size,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
    }

    #[test]
    fn all_workspaces_keep_scope_and_approvals_visible_at_supported_sizes() {
        for size in [egui::vec2(840.0, 640.0), egui::vec2(1240.0, 900.0)] {
            for workspace in ["conversation", "mind", "identity", "lab", "appearance"] {
                let mut app = AgentApp::isolated_snapshot(AgentConfig::default(), workspace);
                let ctx = egui::Context::default();
                frame(&mut app, &ctx, size, Vec::new());
                let output = frame(&mut app, &ctx, size, Vec::new());
                let labels = texts(&output);
                let scope = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Rect(rect) = &shape.shape {
                            if (rect.rect.height() - 220.0).abs() < 1.0 && rect.rect.width() > 180.0
                            {
                                return Some((rect.rect, shape.clip_rect));
                            }
                        }
                        None
                    })
                    .expect("scope canvas missing");
                assert!(
                    scope.1.expand(1.0).contains_rect(scope.0),
                    "{workspace} {size:?}: scope clipped"
                );
                for expected in ["Novelty", "Allow this session", "Dismiss"] {
                    let (_, rect, clip) = labels
                        .iter()
                        .find(|(text, _, _)| text == expected)
                        .unwrap_or_else(|| panic!("{workspace}: missing {expected}"));
                    assert!(
                        clip.expand(1.0).contains_rect(*rect),
                        "{workspace} {size:?}: clipped {expected}: {rect:?} by {clip:?}"
                    );
                }
                for removed in [
                    "PERSONAL AGENT / LOCAL WORKBENCH",
                    "UI OWNS RUNTIME",
                    "SURPRISAL / NOVELTY",
                    "CLOSE UI -> STOP AGENT + LOCAL HOST",
                ] {
                    assert!(!labels.iter().any(|(text, _, _)| text == removed));
                }
                if workspace == "conversation" {
                    let (_, rect, clip) =
                        labels.iter().find(|(text, _, _)| text == "Send").unwrap();
                    assert!(
                        clip.expand(1.0).contains_rect(*rect),
                        "composer clipped at {size:?}"
                    );
                }
                if workspace == "appearance" {
                    for expected in ["Save & apply", "Revert drafts"] {
                        let (_, rect, clip) =
                            labels.iter().find(|(text, _, _)| text == expected).unwrap();
                        assert!(
                            clip.expand(1.0).contains_rect(*rect),
                            "{expected} clipped at {size:?}"
                        );
                    }
                }
                for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
                    if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
                    }
                }
            }
        }
    }

    #[test]
    fn native_navigation_and_palette_controls_preserve_shared_drafts() {
        let ctx = egui::Context::default();
        let size = egui::vec2(1240.0, 900.0);
        let mut app = AgentApp::isolated_snapshot(AgentConfig::default(), "appearance");
        frame(&mut app, &ctx, size, Vec::new());
        click(&mut app, &ctx, size, "Copper");
        assert_eq!(
            app.settings_panel.config.appearance.base_color,
            [216, 166, 117]
        );
        app.settings_panel.config.username = "Unsaved agent name".into();
        click(&mut app, &ctx, size, "Identity");
        assert_eq!(app.workspace, Workspace::Identity);
        assert_eq!(
            app.character_panel.config.appearance.base_color,
            [216, 166, 117]
        );
        assert_eq!(app.character_panel.config.username, "Unsaved agent name");
        click(&mut app, &ctx, size, "Settings");
        assert_eq!(app.workspace, Workspace::Settings);
        assert!(app.settings_panel.has_unsaved_changes());
        click(&mut app, &ctx, size, "Revert drafts");
        assert!(!app.settings_panel.has_unsaved_changes());
        assert_eq!(
            app.settings_panel.config.appearance,
            AgentConfig::default().appearance
        );
        click(&mut app, &ctx, size, "Affect lab");
        assert_eq!(app.workspace, Workspace::AffectLab);
        assert!(app.affect_lab.show);
        assert!(!app.settings_panel.show);
        click(&mut app, &ctx, size, "Open model settings");
        assert_eq!(app.workspace, Workspace::Settings);
        app.affect_lab.show = true; // Existing Models button's navigation request.
        frame(&mut app, &ctx, size, Vec::new());
        assert_eq!(app.workspace, Workspace::AffectLab);
        click(&mut app, &ctx, size, "Conversation");
        assert!(!app.affect_lab.show);
    }

    #[test]
    fn live_tool_output_cannot_push_composer_out_of_compact_window() {
        let ctx = egui::Context::default();
        let size = egui::vec2(840.0, 640.0);
        let mut app = AgentApp::isolated_snapshot(AgentConfig::default(), "conversation");
        app.live_tool_progress.push(LiveToolProgress {
            conversation_id: app.active_conversation_id.clone(),
            tool_name: "fixture".into(),
            output_preview: "longtoken".repeat(300),
            subtask_id: None,
        });
        frame(&mut app, &ctx, size, Vec::new());
        let output = frame(&mut app, &ctx, size, Vec::new());
        let labels = texts(&output);
        let (_, rect, clip) = labels
            .iter()
            .find(|(text, _, _)| text == "Send")
            .expect("composer missing during tool output");
        assert!(
            clip.expand(1.0).contains_rect(*rect),
            "tool output clipped composer"
        );
    }
}
