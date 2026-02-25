use eframe::egui::{self, Color32, RichText, ScrollArea};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::api::{ChatMessage, FrontendEvent};

const CHAT_TOOL_BLOCK_START: &str = "[tool_calls]";
const CHAT_TOOL_BLOCK_END: &str = "[/tool_calls]";
const CHAT_THINKING_BLOCK_START: &str = "[thinking]";
const CHAT_THINKING_BLOCK_END: &str = "[/thinking]";
const CHAT_MEDIA_BLOCK_START: &str = "[media]";
const CHAT_MEDIA_BLOCK_END: &str = "[/media]";
const CHAT_TURN_CONTROL_BLOCK_START: &str = "[turn_control]";
const CHAT_TURN_CONTROL_BLOCK_END: &str = "[/turn_control]";

#[derive(Debug, Clone, Default, Deserialize)]
struct ChatToolCallDetail {
    tool_name: String,
    arguments_preview: String,
    output_kind: String,
    output_preview: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ChatMediaDetail {
    path: String,
    #[serde(default)]
    media_kind: String,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ChatTurnControlBlock {
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    needs_user_input: Option<bool>,
    #[serde(default)]
    user_message: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ChatTurnControlDetail {
    decision: String,
    status: String,
    needs_user_input: bool,
    reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ChatRenderPayload {
    display_content: String,
    tool_details: Vec<ChatToolCallDetail>,
    thinking_details: Vec<String>,
    media_details: Vec<ChatMediaDetail>,
    turn_control: Option<ChatTurnControlDetail>,
}

#[derive(Default)]
pub struct ChatMediaCache {
    image_textures: HashMap<String, egui::TextureHandle>,
}

impl ChatMediaCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn load_image_texture(
        &mut self,
        ctx: &egui::Context,
        path: &str,
    ) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.image_textures.get(path) {
            return Some(tex.clone());
        }

        let image = image::open(path).ok()?;
        let rgba = image.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
        let tex = ctx.load_texture(
            format!("chat_media_{}", path),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.image_textures.insert(path.to_string(), tex.clone());
        Some(tex)
    }
}

pub fn render_event_log(
    ui: &mut egui::Ui,
    events: &[FrontendEvent],
    detail_popup: &mut Option<String>,
) {
    ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        if events.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Waiting for agent activity...")
                        .weak()
                        .italics(),
                );
            });
            return;
        }

        // Find CycleStart positions to group events into turns.
        let mut cycle_starts: Vec<(usize, String)> = Vec::new();
        for (i, event) in events.iter().enumerate() {
            if let FrontendEvent::CycleStart { label } = event {
                cycle_starts.push((i, label.clone()));
            }
        }

        if cycle_starts.is_empty() {
            // No cycle markers yet — render flat for backwards compat.
            for (i, event) in events.iter().enumerate() {
                render_single_event(ui, event, i, detail_popup);
            }
            return;
        }

        // Render preamble events (before first CycleStart).
        let preamble_end = cycle_starts[0].0;
        for (i, event) in events[..preamble_end].iter().enumerate() {
            render_single_event(ui, event, i, detail_popup);
        }

        // Render each cycle as a collapsible group; most recent open by default.
        let cycle_count = cycle_starts.len();
        for (gi, (start_idx, label)) in cycle_starts.iter().enumerate() {
            let content_start = start_idx + 1;
            let content_end = if gi + 1 < cycle_count {
                cycle_starts[gi + 1].0
            } else {
                events.len()
            };
            let group_events = &events[content_start..content_end];
            let is_last = gi == cycle_count - 1;

            let visible_count = group_events
                .iter()
                .filter(|e| {
                    !matches!(
                        e,
                        FrontendEvent::StateChanged(_)
                            | FrontendEvent::ChatStreaming { .. }
                            | FrontendEvent::CycleStart { .. }
                    )
                })
                .count();

            egui::CollapsingHeader::new(
                RichText::new(format!("{} ({})", label, visible_count)).small(),
            )
            .id_salt(("cycle_group", gi))
            .default_open(is_last)
            .show(ui, |ui| {
                for (ei, event) in group_events.iter().enumerate() {
                    render_single_event(ui, event, gi * 10000 + ei, detail_popup);
                }
            });
            ui.add_space(2.0);
        }
    });
}

/// Threshold above which an expand button is shown.
const EXPAND_THRESHOLD: usize = 100;

/// Render one line with optional text truncation + an expand button.
/// If `full_text` exceeds `EXPAND_THRESHOLD` chars, show a truncated version
/// followed by a small "⋯" button that populates `detail_popup`.
fn event_line(
    ui: &mut egui::Ui,
    full_text: &str,
    color: Color32,
    detail_popup: &mut Option<String>,
) {
    if full_text.chars().count() > EXPAND_THRESHOLD {
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(truncate_for_ui(full_text, EXPAND_THRESHOLD))
                        .color(color)
                        .small(),
                )
                .wrap(),
            );
            if ui
                .add(
                    egui::Button::new(RichText::new("⋯").small().weak())
                        .frame(false)
                        .small(),
                )
                .on_hover_text("View full message")
                .clicked()
            {
                *detail_popup = Some(full_text.to_string());
            }
        });
    } else {
        ui.add(
            egui::Label::new(RichText::new(full_text).color(color).small()).wrap(),
        );
    }
}

fn render_single_event(
    ui: &mut egui::Ui,
    event: &FrontendEvent,
    event_idx: usize,
    detail_popup: &mut Option<String>,
) {
    match event {
        FrontendEvent::Observation(text) => {
            event_line(ui, text, Color32::LIGHT_BLUE, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::ReasoningTrace(steps) => {
            egui::CollapsingHeader::new(
                RichText::new(format!("💭 Reasoning ({} steps)", steps.len())).small(),
            )
            .id_salt((event_idx, "reasoning"))
            .default_open(false)
            .show(ui, |ui| {
                for (step_idx, step) in steps.iter().enumerate() {
                    if step.chars().count() > 80 {
                        let header = truncate_for_ui(step, 80);
                        egui::CollapsingHeader::new(
                            RichText::new(format!("• {}", header))
                                .color(Color32::GRAY)
                                .small(),
                        )
                        .id_salt((event_idx, step_idx))
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(step.as_str()).color(Color32::GRAY).small(),
                                )
                                .wrap(),
                            );
                        });
                    } else {
                        ui.label(
                            RichText::new(format!("• {}", step))
                                .color(Color32::GRAY)
                                .small(),
                        );
                    }
                }
            });
            ui.add_space(4.0);
        }
        FrontendEvent::ToolCallProgress {
            tool_name,
            output_preview,
            ..
        } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("🛠 {}", tool_name))
                        .color(Color32::KHAKI)
                        .small()
                        .strong(),
                );
                if output_preview.chars().count() > EXPAND_THRESHOLD {
                    ui.add(
                        egui::Label::new(
                            RichText::new(truncate_for_ui(output_preview, EXPAND_THRESHOLD))
                                .weak()
                                .small(),
                        )
                        .wrap(),
                    );
                    let full = format!("🛠 {}\n\n{}", tool_name, output_preview);
                    if ui
                        .add(
                            egui::Button::new(RichText::new("⋯").small().weak())
                                .frame(false)
                                .small(),
                        )
                        .on_hover_text("View full output")
                        .clicked()
                    {
                        *detail_popup = Some(full);
                    }
                } else {
                    ui.label(RichText::new(output_preview.as_str()).weak().small());
                }
            });
            ui.add_space(2.0);
        }
        FrontendEvent::ActionTaken { action, result } => {
            let full = format!("✅ {}: {}", action, result);
            event_line(ui, &full, Color32::GREEN, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::OrientationUpdate(orientation) => {
            let full = format!(
                "🧭 {} · {} anomalies · {} salient",
                orientation.disposition, orientation.anomaly_count, orientation.salience_count
            );
            event_line(ui, &full, Color32::LIGHT_YELLOW, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::JournalWritten(summary) => {
            let full = format!("📓 {}", summary);
            event_line(ui, &full, Color32::LIGHT_GREEN, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::ConcernCreated { id, summary } => {
            let full = format!(
                "🧷 [{}] {}",
                id.chars().take(8).collect::<String>(),
                summary
            );
            event_line(ui, &full, Color32::LIGHT_BLUE, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::ConcernTouched { id, summary } => {
            let full = format!(
                "🔁 [{}] {}",
                id.chars().take(8).collect::<String>(),
                summary
            );
            event_line(ui, &full, Color32::LIGHT_YELLOW, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::Error(e) => {
            let full = format!("❌ {}", e);
            event_line(ui, &full, Color32::RED, detail_popup);
            ui.add_space(3.0);
        }
        FrontendEvent::StateChanged(_)
        | FrontendEvent::ChatStreaming { .. }
        | FrontendEvent::ApprovalRequest { .. }
        | FrontendEvent::CycleStart { .. } => {
            // Handled by caller (state in header, streaming in chat pane,
            // approvals as popup, cycle starts used for grouping only).
        }
    }
}

/// Render the private chat interface between operator and agent
pub fn render_private_chat(
    ui: &mut egui::Ui,
    messages: &[ChatMessage],
    streaming_preview: Option<&str>,
    media_cache: &mut ChatMediaCache,
) -> Option<String> {
    let mut requested_prompt_turn_id: Option<String> = None;
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.heading("Private Chat");
        ui.add_space(4.0);
        ui.label(
            RichText::new("Direct communication with your agent")
                .weak()
                .italics(),
        );
        ui.add_space(8.0);

        // Never force a minimum here: parent layout may already reserve space
        // for live-tool output + composer, and forcing a min height causes
        // overlap/pileups when agent tool activity expands.
        let chat_scroll_height = ui.available_height().max(0.0);
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(chat_scroll_height)
            .show(ui, |ui| {
                let has_live_preview =
                    streaming_preview.map_or(false, |p| !p.trim().is_empty());

                if messages.is_empty() && !has_live_preview {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(
                                "No messages yet. Type below to start chatting with your agent.",
                            )
                            .weak()
                            .italics(),
                        );
                    });
                    return;
                }

                for msg in messages {
                    let is_operator = msg.role == "operator";
                    let time_str = msg.created_at.format("%H:%M").to_string();
                    let payload = parse_chat_payload(&msg.content);
                    let row_width = ui.available_width();
                    let bubble_cap = (row_width - 8.0).max(120.0);
                    let max_bubble_width = (row_width * 0.7).max(120.0).min(bubble_cap);
                    let bubble_width = max_bubble_width.min(row_width);
                    ui.horizontal_top(|ui| {
                        if is_operator {
                            let spacer = (row_width - bubble_width).max(0.0);
                            if spacer > 0.0 {
                                ui.add_space(spacer);
                            }
                        }

                        ui.vertical(|ui| {
                            let prompt_clicked = render_chat_message_bubble(
                                ui,
                                msg,
                                &time_str,
                                &payload,
                                is_operator,
                                bubble_width,
                                media_cache,
                            );
                            if prompt_clicked {
                                if let Some(turn_id) = msg.turn_id.as_deref() {
                                    requested_prompt_turn_id = Some(turn_id.to_string());
                                }
                            }
                        });
                    });

                    if let Some(turn_control) = payload.turn_control.as_ref() {
                        render_turn_control_separator(ui, turn_control);
                        ui.add_space(6.0);
                    }

                    if !payload.thinking_details.is_empty() || !payload.tool_details.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_top(|ui| {
                            render_message_detail_panels(ui, &msg.id, &payload);
                        });
                    }

                    ui.add_space(8.0);
                }

                // Render streaming preview AFTER messages so it appears at the bottom.
                if let Some(preview) = streaming_preview {
                    let trimmed = preview.trim();
                    if !trimmed.is_empty() {
                        let row_width = ui.available_width();
                        let bubble_cap = (row_width - 8.0).max(120.0);
                        let max_bubble_width = (row_width * 0.7).max(120.0).min(bubble_cap);
                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                render_streaming_preview_bubble(ui, trimmed, max_bubble_width);
                            });
                        });
                        ui.add_space(8.0);
                    }
                }
            });
    });
    requested_prompt_turn_id
}

fn render_chat_message_bubble(
    ui: &mut egui::Ui,
    msg: &ChatMessage,
    time_str: &str,
    payload: &ChatRenderPayload,
    is_operator: bool,
    max_bubble_width: f32,
    media_cache: &mut ChatMediaCache,
) -> bool {
    let mut prompt_clicked = false;
    ui.group(|ui| {
        let inner_width = (max_bubble_width - 14.0).max(100.0);
        ui.set_min_width(inner_width);
        ui.set_width(inner_width);
        ui.set_max_width(inner_width);
        let wrap_token_len = max_token_len_for_width(inner_width);

        let (role_label, role_color, bg_color) = if is_operator {
            (
                "You",
                Color32::from_rgb(100, 149, 237),
                Color32::from_rgb(30, 40, 60),
            )
        } else {
            (
                "Agent",
                Color32::from_rgb(144, 238, 144),
                Color32::from_rgb(30, 50, 40),
            )
        };

        ui.visuals_mut().widgets.noninteractive.bg_fill = bg_color;

        ui.horizontal(|ui| {
            ui.label(RichText::new(role_label).color(role_color).strong());
            ui.label(RichText::new(time_str).weak().small());
            if !is_operator && msg.turn_id.is_some() {
                ui.add_space(6.0);
                if ui.small_button("View Prompt").clicked() {
                    prompt_clicked = true;
                }
            }
        });

        ui.add(
            egui::Label::new(force_wrap_long_tokens(
                payload.display_content.as_str(),
                wrap_token_len,
            ))
            .wrap(),
        );

        if !payload.media_details.is_empty() {
            ui.add_space(6.0);
            render_media_panel(
                ui,
                &payload.media_details,
                (inner_width - 12.0).max(80.0),
                media_cache,
            );
        }

        // Show processing status for operator messages
        if is_operator && !msg.processed {
            ui.label(
                RichText::new("⏳ Waiting for agent...")
                    .weak()
                    .small()
                    .italics(),
            );
        }
    });
    prompt_clicked
}

fn render_message_detail_panels(ui: &mut egui::Ui, message_id: &str, payload: &ChatRenderPayload) {
    let details_width = (ui.available_width() - 8.0).max(120.0);
    ui.set_min_width(details_width);
    ui.set_width(details_width);
    ui.set_max_width(details_width);
    let wrap_token_len = max_token_len_for_width(details_width);

    let has_thinking = !payload.thinking_details.is_empty();
    let has_tool_calls = !payload.tool_details.is_empty();

    if has_thinking {
        render_thinking_panel(ui, message_id, &payload.thinking_details, wrap_token_len);
        if has_tool_calls {
            ui.add_space(4.0);
        }
    }

    if has_tool_calls {
        render_tool_calls_panel(ui, message_id, &payload.tool_details, wrap_token_len);
    }
}

fn render_media_panel(
    ui: &mut egui::Ui,
    media_details: &[ChatMediaDetail],
    max_width: f32,
    media_cache: &mut ChatMediaCache,
) {
    ui.label(RichText::new("Media").small().color(Color32::LIGHT_GREEN));

    for media in media_details {
        let kind = normalize_media_kind(&media.media_kind);
        let filename = Path::new(&media.path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(media.path.as_str());

        ui.group(|ui| {
            let source = media.source.clone().unwrap_or_else(|| "tool".to_string());
            ui.label(RichText::new(format!("{} ({})", filename, source)).strong());
            ui.label(RichText::new(&media.path).weak().small());

            if let Some(mime) = media.mime_type.as_deref().filter(|m| !m.trim().is_empty()) {
                ui.label(RichText::new(mime).weak().small().italics());
            }

            match kind {
                "image" => {
                    if let Some(texture) = media_cache.load_image_texture(ui.ctx(), &media.path) {
                        let mut size = texture.size_vec2();
                        if size.x > max_width {
                            let scale = max_width / size.x;
                            size *= scale;
                        }
                        if size.y > 240.0 {
                            let scale = 240.0 / size.y;
                            size *= scale;
                        }
                        ui.image((texture.id(), size));
                    } else {
                        ui.label(RichText::new("Preview unavailable").small().weak());
                    }
                }
                "audio" => {
                    ui.label(RichText::new("Audio file generated").small());
                }
                "video" => {
                    ui.label(RichText::new("Video file generated").small());
                }
                _ => {
                    ui.label(RichText::new("File generated").small());
                }
            }
        });
        ui.add_space(4.0);
    }
}

fn normalize_media_kind(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "image" => "image",
        "audio" => "audio",
        "video" => "video",
        _ => "file",
    }
}

fn render_thinking_panel(
    ui: &mut egui::Ui,
    message_id: &str,
    thinking_details: &[String],
    wrap_token_len: usize,
) {
    egui::CollapsingHeader::new(format!("Thinking ({})", thinking_details.len()))
        .id_salt((message_id, "thinking"))
        .default_open(false)
        .show(ui, |ui| {
            for thought in thinking_details {
                ui.group(|ui| {
                    ui.monospace(force_wrap_long_tokens(thought.trim(), wrap_token_len));
                });
                ui.add_space(4.0);
            }
        });
}

fn render_tool_calls_panel(
    ui: &mut egui::Ui,
    message_id: &str,
    tool_details: &[ChatToolCallDetail],
    wrap_token_len: usize,
) {
    egui::CollapsingHeader::new(format!("Tool calls ({})", tool_details.len()))
        .id_salt((message_id, "tool_calls"))
        .default_open(false)
        .show(ui, |ui| {
            for detail in tool_details {
                ui.group(|ui| {
                    ui.label(
                        RichText::new(format!("{} [{}]", detail.tool_name, detail.output_kind))
                            .strong(),
                    );
                    if !detail.arguments_preview.trim().is_empty() {
                        ui.label(
                            RichText::new("Arguments")
                                .small()
                                .color(Color32::LIGHT_BLUE),
                        );
                        ui.monospace(force_wrap_long_tokens(
                            detail.arguments_preview.trim(),
                            wrap_token_len,
                        ));
                    }
                    if !detail.output_preview.trim().is_empty() {
                        ui.label(RichText::new("Output").small().color(Color32::LIGHT_GREEN));
                        ui.monospace(force_wrap_long_tokens(
                            detail.output_preview.trim(),
                            wrap_token_len,
                        ));
                    }
                });
                ui.add_space(4.0);
            }
        });
}

fn render_streaming_preview_bubble(ui: &mut egui::Ui, preview: &str, max_bubble_width: f32) {
    ui.group(|ui| {
        let inner_width = (max_bubble_width - 14.0).max(100.0);
        ui.set_min_width(inner_width);
        ui.set_width(inner_width);
        ui.set_max_width(inner_width);
        let wrap_token_len = max_token_len_for_width(inner_width);
        ui.visuals_mut().widgets.noninteractive.bg_fill = Color32::from_rgb(30, 50, 40);

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Agent")
                    .color(Color32::from_rgb(144, 238, 144))
                    .strong(),
            );
            ui.label(RichText::new("live").weak().small().italics());
        });

        ui.add(egui::Label::new(force_wrap_long_tokens(preview, wrap_token_len)).wrap());
    });
}

fn render_turn_control_separator(ui: &mut egui::Ui, detail: &ChatTurnControlDetail) {
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "Turn control: {} · {} · needs_input={}",
                detail.decision, detail.status, detail.needs_user_input
            ))
            .small()
            .weak()
            .italics(),
        );
        if let Some(reason) = detail.reason.as_deref() {
            let reason = reason.trim();
            if !reason.is_empty() {
                ui.label(
                    RichText::new(format!("({})", truncate_for_ui(reason, 160)))
                        .small()
                        .weak(),
                );
            }
        }
    });
}

fn max_token_len_for_width(width: f32) -> usize {
    // Rough monospace-ish estimate to keep long unbroken tokens from expanding bubbles.
    ((width / 7.5).floor() as usize).clamp(20, 140)
}

fn force_wrap_long_tokens(input: &str, max_token_len: usize) -> String {
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

fn parse_chat_payload(content: &str) -> ChatRenderPayload {
    let (without_tools, raw_tools) =
        extract_block(content, CHAT_TOOL_BLOCK_START, CHAT_TOOL_BLOCK_END);
    let tool_details = raw_tools
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<ChatToolCallDetail>>(raw).ok())
        .unwrap_or_default();

    let (without_block_thinking, raw_thinking) = extract_block(
        &without_tools,
        CHAT_THINKING_BLOCK_START,
        CHAT_THINKING_BLOCK_END,
    );
    let mut thinking_details = raw_thinking
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default();

    let (without_media_blocks, raw_media) = extract_block(
        &without_block_thinking,
        CHAT_MEDIA_BLOCK_START,
        CHAT_MEDIA_BLOCK_END,
    );
    let media_details = raw_media
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<ChatMediaDetail>>(raw).ok())
        .unwrap_or_default();

    let (without_turn_control, raw_turn_control) =
        extract_turn_control_block(&without_media_blocks);
    let turn_control = raw_turn_control.as_deref().map(parse_turn_control_detail);

    let (display_content, inline_thinking) = strip_inline_thinking_tags(&without_turn_control);
    thinking_details.extend(inline_thinking);

    ChatRenderPayload {
        display_content: display_content.trim().to_string(),
        tool_details,
        thinking_details,
        media_details,
        turn_control,
    }
}

fn extract_block(content: &str, start_marker: &str, end_marker: &str) -> (String, Option<String>) {
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

fn extract_turn_control_block(content: &str) -> (String, Option<String>) {
    let Some(start_idx) = content.find(CHAT_TURN_CONTROL_BLOCK_START) else {
        return (content.to_string(), None);
    };
    let after_start = start_idx + CHAT_TURN_CONTROL_BLOCK_START.len();
    let remaining = &content[after_start..];

    if let Some(relative_end) = remaining.find(CHAT_TURN_CONTROL_BLOCK_END) {
        let end_idx = after_start + relative_end;
        let full_end = end_idx + CHAT_TURN_CONTROL_BLOCK_END.len();
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
        return (cleaned, Some(raw));
    }

    // Tolerate malformed output where the closing marker is missing.
    let cleaned = content[..start_idx].trim_end().to_string();
    let raw = remaining.trim().to_string();
    (cleaned, Some(raw))
}

fn parse_turn_control_detail(raw: &str) -> ChatTurnControlDetail {
    let cleaned = strip_optional_json_code_fence(raw);
    if let Ok(parsed) = serde_json::from_str::<ChatTurnControlBlock>(cleaned) {
        return ChatTurnControlDetail {
            decision: parsed
                .decision
                .unwrap_or_else(|| "yield".to_string())
                .trim()
                .to_string(),
            status: parsed
                .status
                .unwrap_or_else(|| "done".to_string())
                .trim()
                .to_string(),
            needs_user_input: parsed.needs_user_input.unwrap_or(false),
            reason: parsed
                .reason
                .or(parsed.user_message)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        };
    }

    ChatTurnControlDetail {
        decision: "unknown".to_string(),
        status: "unparsed".to_string(),
        needs_user_input: false,
        reason: Some(truncate_for_ui(cleaned.trim(), 160)),
    }
}

fn strip_optional_json_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let after_start = &trimmed[3..];
    let end_rel = after_start.find("```");
    let inner = end_rel
        .map(|idx| after_start[..idx].trim())
        .unwrap_or_else(|| after_start.trim());

    let mut lines = inner.lines();
    let first = lines.next().unwrap_or_default().trim();
    let first_lower = first.to_ascii_lowercase();
    if first_lower == "json" || first_lower == "jsonc" {
        return inner[first.len()..].trim();
    }
    inner
}

fn truncate_for_ui(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn strip_inline_thinking_tags(content: &str) -> (String, Vec<String>) {
    fn strip_tag(text: String, open_tag: &str, close_tag: &str) -> (String, Vec<String>) {
        let mut rest = text;
        let mut extracted = Vec::new();

        while let Some(start) = rest.find(open_tag) {
            let inner_start = start + open_tag.len();
            if let Some(rel_end) = rest[inner_start..].find(close_tag) {
                let end = inner_start + rel_end;
                let thought = rest[inner_start..end].trim();
                if !thought.is_empty() {
                    extracted.push(thought.to_string());
                }
                rest.replace_range(start..end + close_tag.len(), "");
            } else {
                let thought = rest[inner_start..].trim();
                if !thought.is_empty() {
                    extracted.push(thought.to_string());
                }
                rest.replace_range(start..rest.len(), "");
            }
        }

        (rest, extracted)
    }

    let (without_thinking, mut thoughts_a) =
        strip_tag(content.to_string(), "<thinking>", "</thinking>");
    let (without_think, mut thoughts_b) = strip_tag(without_thinking, "<think>", "</think>");
    thoughts_a.append(&mut thoughts_b);
    (without_think, thoughts_a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_embedded_tool_details() {
        let content = "Done.\n\n[tool_calls]\n[{\"tool_name\":\"shell\",\"arguments_preview\":\"{}\",\"output_kind\":\"text\",\"output_preview\":\"ok\"}]\n[/tool_calls]";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Done.");
        assert_eq!(payload.tool_details.len(), 1);
        assert_eq!(payload.tool_details[0].tool_name, "shell");
    }

    #[test]
    fn leaves_plain_message_unchanged() {
        let content = "Hello there";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Hello there");
        assert!(payload.tool_details.is_empty());
        assert!(payload.thinking_details.is_empty());
    }

    #[test]
    fn parses_embedded_thinking_block() {
        let content = "Answer\n\n[thinking]\n[\"step one\",\"step two\"]\n[/thinking]";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Answer");
        assert_eq!(payload.thinking_details.len(), 2);
    }

    #[test]
    fn strips_inline_thinking_tags() {
        let content = "<think>internal</think>\nVisible";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Visible");
        assert_eq!(payload.thinking_details, vec!["internal"]);
    }

    #[test]
    fn parses_embedded_media_block() {
        let content = "Generated.\n\n[media]\n[{\"path\":\"/tmp/a.png\",\"media_kind\":\"image\",\"mime_type\":\"image/png\",\"source\":\"generate_comfy_media\"}]\n[/media]";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Generated.");
        assert_eq!(payload.media_details.len(), 1);
        assert_eq!(payload.media_details[0].path, "/tmp/a.png");
        assert_eq!(payload.media_details[0].media_kind, "image");
    }

    #[test]
    fn parses_turn_control_block_and_removes_it_from_message() {
        let content = "Working...\n[turn_control]\n{\"decision\":\"continue\",\"status\":\"still_working\",\"needs_user_input\":false,\"reason\":\"checking files\"}\n[/turn_control]";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Working...");
        let turn_control = payload.turn_control.expect("turn control parsed");
        assert_eq!(turn_control.decision, "continue");
        assert_eq!(turn_control.status, "still_working");
        assert!(!turn_control.needs_user_input);
    }

    #[test]
    fn parses_turn_control_without_closing_marker() {
        let content = "Still going.\n[turn_control]\n{\"decision\":\"continue\",\"status\":\"still_working\",\"needs_user_input\":false}\n";
        let payload = parse_chat_payload(content);
        assert_eq!(payload.display_content, "Still going.");
        let turn_control = payload.turn_control.expect("turn control parsed");
        assert_eq!(turn_control.decision, "continue");
        assert_eq!(turn_control.status, "still_working");
    }
}
