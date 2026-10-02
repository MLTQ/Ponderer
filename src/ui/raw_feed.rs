//! Bounded provider-output history, independent of chat previews and novelty samples.
use eframe::egui::{self, RichText};
use std::collections::VecDeque;

const MAX_GENERATIONS: usize = 16;
const MAX_CHARS: usize = 65_536;

#[derive(Default)]
pub struct RawFeed {
    generations: VecDeque<RawGeneration>,
}

struct RawGeneration {
    id: String,
    source: String,
    conversation: Option<String>,
    chunks: Vec<(String, String)>,
    outcome: Option<String>,
    chars: usize,
    truncated: bool,
}

impl RawFeed {
    pub fn start(&mut self, id: &str, source: &str, conversation: Option<&str>) {
        if self.generations.iter().any(|g| g.id == id) {
            return;
        }
        if self.generations.len() == MAX_GENERATIONS {
            self.generations.pop_front();
        }
        self.generations.push_back(RawGeneration {
            id: id.into(),
            source: source.into(),
            conversation: conversation.map(str::to_owned),
            chunks: Vec::new(),
            outcome: None,
            chars: 0,
            truncated: false,
        });
    }

    pub fn push(
        &mut self,
        id: &str,
        source: &str,
        conversation: Option<&str>,
        channel: &str,
        text: &str,
    ) {
        self.start(id, source, conversation);
        let g = self.generations.iter_mut().find(|g| g.id == id).unwrap();
        let remaining = MAX_CHARS.saturating_sub(g.chars);
        let retained: String = text.chars().take(remaining).collect();
        let count = retained.chars().count();
        g.truncated |= count < text.chars().count();
        g.chars += count;
        if retained.is_empty() {
            return;
        }
        if let Some((previous_channel, previous_text)) = g.chunks.last_mut() {
            if previous_channel == channel {
                previous_text.push_str(&retained);
                return;
            }
        }
        g.chunks.push((channel.into(), retained));
    }

    pub fn finish(&mut self, id: &str, source: &str, conversation: Option<&str>, outcome: &str) {
        self.start(id, source, conversation);
        self.generations
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap()
            .outcome = Some(outcome.into());
    }

    pub fn render(&mut self, ui: &mut egui::Ui) {
        if ui.small_button("Clear output").clicked() {
            self.generations.clear();
        }
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .id_salt("raw_provider_output")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.generations.is_empty() {
                    ui.weak("No model output received yet.");
                }
                for g in &self.generations {
                    ui.separator();
                    ui.small(format!(
                        "{} · {} · {}",
                        g.source,
                        g.conversation.as_deref().unwrap_or("background"),
                        g.outcome.as_deref().unwrap_or("generating")
                    ));
                    if g.chunks.is_empty() {
                        ui.weak("Waiting for output.");
                    }
                    for (channel, text) in &g.chunks {
                        ui.small(channel);
                        ui.add(
                            egui::Label::new(RichText::new(text).monospace().small())
                                .wrap()
                                .selectable(true),
                        );
                    }
                    if g.truncated {
                        ui.weak("Output truncated at 65,536 characters.");
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_chunks_survive_completion_and_background_interleaving() {
        let mut feed = RawFeed::default();
        feed.push("chat", "operator_chat", Some("one"), "content", "  Héllo\n");
        feed.push("dream", "dream", None, "content", "{\"ok\":true}");
        feed.push(
            "chat",
            "operator_chat",
            Some("one"),
            "tool_0_arguments",
            "{\"q\":",
        );
        feed.push(
            "chat",
            "operator_chat",
            Some("one"),
            "tool_0_arguments",
            "\"灯\"}",
        );
        feed.finish("chat", "operator_chat", Some("one"), "completed");
        assert_eq!(feed.generations.len(), 2);
        assert_eq!(feed.generations[0].chunks[0].1, "  Héllo\n");
        assert_eq!(feed.generations[0].chunks[1].1, "{\"q\":\"灯\"}");
        assert_eq!(feed.generations[1].chunks[0].1, "{\"ok\":true}");
        assert_eq!(feed.generations[0].outcome.as_deref(), Some("completed"));
    }

    #[test]
    fn retention_is_bounded_and_utf8_safe() {
        let mut feed = RawFeed::default();
        for i in 0..20 {
            feed.push(&i.to_string(), "reasoning", None, "content", "灯");
        }
        assert_eq!(feed.generations.len(), MAX_GENERATIONS);
        feed.push("19", "reasoning", None, "content", &"灯".repeat(MAX_CHARS));
        assert_eq!(feed.generations.back().unwrap().chars, MAX_CHARS);
        assert!(feed.generations.back().unwrap().truncated);
    }
}
