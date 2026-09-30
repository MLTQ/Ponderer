use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{json, Value};

use crate::api::{AffectLabStart, ApiClient, CACHE_TYPES, MAX_CONTEXT_SIZE};
use crate::config::AgentConfig;

enum LabReply {
    Status(Value),
    Provider(Box<AgentConfig>),
    Error(String),
}

pub struct AffectLabPanel {
    pub show: bool,
    settings: AffectLabStart,
    status: Value,
    reply_tx: flume::Sender<LabReply>,
    reply_rx: flume::Receiver<LabReply>,
    pending: bool,
    last_poll: Instant,
    error: Option<String>,
    strengths: BTreeMap<String, f64>,
    layer_start: i64,
    layer_end: i64,
    dirty: bool,
    concept: String,
    test_prompt: String,
    test_strength: f64,
    test_tokens: u32,
    custom_name: String,
    custom_pairs: String,
}

impl AffectLabPanel {
    pub fn new() -> Self {
        let (reply_tx, reply_rx) = flume::unbounded();
        let model_path = dirs::home_dir()
            .map(|home| home.join(".lmstudio/models/OBLITERATUS/Qwen3.8-27B-OBLITERATED"))
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            show: false,
            settings: AffectLabStart { model_path, ..Default::default() },
            status: json!({"running": false}),
            reply_tx,
            reply_rx,
            pending: false,
            last_poll: Instant::now() - Duration::from_secs(2),
            error: None,
            strengths: BTreeMap::new(),
            layer_start: 1,
            layer_end: 1,
            dirty: false,
            concept: "contentment".into(),
            test_prompt: "A project has finished. What would you choose to do next, and why? Use one sentence.".into(),
            test_strength: 0.25,
            test_tokens: 32,
            custom_name: String::new(),
            custom_pairs: "[\n  {\"target\": \"Target state in situation one\", \"control\": \"Matched control in situation one\"},\n  {\"target\": \"Target state in situation two\", \"control\": \"Matched control in situation two\"}\n]".into(),
        }
    }

    fn request(
        &mut self,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
        action: &str,
        body: Value,
    ) {
        if self.pending {
            return;
        }
        self.pending = true;
        self.error = None;
        let client = client.clone();
        let action = action.to_owned();
        let sender = self.reply_tx.clone();
        runtime.spawn(async move {
            let result = if action == "status" {
                client.affect_lab_status().await.map(LabReply::Status)
            } else if action == "stop" || action == "use-for-agent" {
                client
                    .affect_lab_provider_action(&action)
                    .await
                    .map(|config| LabReply::Provider(Box::new(config)))
            } else {
                client
                    .affect_lab_action(&action, body)
                    .await
                    .map(LabReply::Status)
            };
            let _ = sender.send(result.unwrap_or_else(|error| LabReply::Error(error.to_string())));
        });
    }

    fn accept_status(&mut self, value: Value) {
        if !self.dirty {
            if let Some(profile) = value.get("requested_profile") {
                self.strengths = profile["strengths"]
                    .as_object()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|(key, value)| value.as_f64().map(|v| (key.clone(), v)))
                            .collect()
                    })
                    .unwrap_or_default();
                self.layer_start = profile["layer_start"].as_i64().unwrap_or(1);
                self.layer_end = profile["layer_end"].as_i64().unwrap_or(1);
            }
        }
        self.status = value;
    }

    fn can_stop(&self) -> bool {
        self.status["running"].as_bool().unwrap_or(false)
            || self.status["used_by_agent"].as_bool().unwrap_or(false)
            // A failed worker still needs Stop to clear the manager before retrying,
            // even when it was never selected as the agent's provider.
            || self.status["error"].is_string()
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) -> Option<AgentConfig> {
        let mut provider = None;
        while let Ok(reply) = self.reply_rx.try_recv() {
            self.pending = false;
            match reply {
                LabReply::Status(value) => self.accept_status(value),
                LabReply::Provider(config) => {
                    provider = Some(*config);
                    self.last_poll = Instant::now() - Duration::from_secs(2);
                }
                LabReply::Error(error) => self.error = Some(error),
            }
        }
        if !self.show {
            return provider;
        }
        if !self.pending && self.last_poll.elapsed() >= Duration::from_secs(1) {
            self.last_poll = Instant::now();
            self.request(client, runtime, "status", Value::Null);
        }
        let mut open = self.show;
        egui::Window::new("Affect Lab")
            .open(&mut open)
            .default_width(620.0)
            .default_height(700.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.content(ui, client, runtime));
            });
        self.show = open;
        provider
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.label("Experimental activation steering for a local GGUF model.");
        ui.label(egui::RichText::new("The local provider and extraction jobs stop with the owning UI. Strength is an intervention setting, not a measured feeling.").small().weak());
        if let Some(error) = self
            .error
            .as_deref()
            .or_else(|| self.status["error"].as_str())
        {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        let running = self.status["running"].as_bool().unwrap_or(false);
        let job_running = self.status["job"]["phase"].as_str() == Some("running");
        ui.separator();
        ui.label("Model file or LM Studio model directory");
        ui.add_enabled(
            !running,
            egui::TextEdit::singleline(&mut self.settings.model_path).desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("Choose GGUF…"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("GGUF model", &["gguf"])
                    .pick_file()
                {
                    self.settings.model_path = path.to_string_lossy().into_owned();
                }
            }
            ui.add_enabled_ui(!running, |ui| {
                ui.label("CPU threads");
                ui.add(egui::DragValue::new(&mut self.settings.threads).range(1..=128));
                ui.label("GPU layers");
                ui.add(egui::DragValue::new(&mut self.settings.gpu_layers).range(0..=999));
            });
        });
        ui.collapsing("Inference and memory settings", |ui| {
            ui.add_enabled(
                !running,
                egui::TextEdit::singleline(&mut self.settings.server_binary)
                    .desired_width(f32::INFINITY),
            );
            ui.small("Use a llama-server build matching the model. GPU layers 0 uses CPU.");
            ui.horizontal(|ui| {
                ui.label("Context size");
                ui.add_enabled(
                    !running,
                    egui::DragValue::new(&mut self.settings.context_size).range(1024..=MAX_CONTEXT_SIZE),
                );
                if ui.add_enabled(!running, egui::Button::new("200k / Q4_1 preset")).clicked() {
                    self.settings.apply_200k_preset();
                }
            });
            ui.add_enabled_ui(!running, |ui| {
                ui.checkbox(&mut self.settings.unified_kv_cache, "Unified KV cache");
                ui.horizontal(|ui| {
                    ui.label("K cache");
                    egui::ComboBox::from_id_salt("affect_cache_k")
                        .selected_text(&self.settings.cache_type_k)
                        .show_ui(ui, |ui| {
                            for value in CACHE_TYPES {
                                ui.selectable_value(&mut self.settings.cache_type_k, (*value).into(), *value);
                            }
                        });
                    ui.label("V cache");
                    egui::ComboBox::from_id_salt("affect_cache_v")
                        .selected_text(&self.settings.cache_type_v)
                        .show_ui(ui, |ui| {
                            for value in CACHE_TYPES {
                                ui.selectable_value(&mut self.settings.cache_type_v, (*value).into(), *value);
                            }
                        });
                    ui.label("Flash attention");
                    egui::ComboBox::from_id_salt("affect_flash")
                        .selected_text(&self.settings.flash_attention)
                        .show_ui(ui, |ui| {
                            for value in ["auto", "on", "off"] {
                                ui.selectable_value(&mut self.settings.flash_attention, value.into(), value);
                            }
                        });
                });
            });
            ui.small("Quantized V cache requires flash attention. The preset leaves GPU layers and executable unchanged; use an engine supporting Q4_1 flash-attention kernels on your device.");
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !running && !self.pending,
                    egui::Button::new("Start local provider"),
                )
                .clicked()
            {
                self.dirty = false;
                self.request(
                    client,
                    runtime,
                    "start",
                    serde_json::to_value(&self.settings).unwrap_or_default(),
                );
            }
            if ui
                .add_enabled(
                    self.can_stop() && !self.pending,
                    egui::Button::new("Stop / restore provider"),
                )
                .clicked()
            {
                self.request(client, runtime, "stop", json!({}));
            }
            if self.pending {
                ui.spinner();
            }
        });
        if !running {
            return;
        }
        let model = &self.status["model"];
        ui.label(format!(
            "{} · {} · {} layers · {} dimensions",
            model["name"].as_str().unwrap_or("GGUF"),
            model["architecture"].as_str().unwrap_or(""),
            model["layers"],
            model["embedding"]
        ));
        let trained_context = model["trained_context"].as_u64().unwrap_or(0);
        ui.small(format!(
            "Inference: {} tokens · K {} / V {} · flash {} · unified KV {}",
            self.status["context_size"],
            self.status["inference_settings"]["cache_type_k"],
            self.status["inference_settings"]["cache_type_v"],
            self.status["inference_settings"]["flash_attention"],
            self.status["inference_settings"]["unified_kv_cache"]
        ));
        if trained_context > 0 && u64::from(self.settings.context_size) > trained_context {
            ui.colored_label(egui::Color32::YELLOW, format!("Requested context exceeds the model's declared {trained_context}-token context. No extra RoPE scaling is configured."));
        }
        let used = self.status["used_by_agent"].as_bool().unwrap_or(false);
        ui.horizontal(|ui| {
            ui.label(if used {
                "Agent uses this provider for this session."
            } else {
                "Agent still uses its configured provider."
            });
            if ui
                .add_enabled(
                    !used && !self.pending && !job_running,
                    egui::Button::new("Use for this session"),
                )
                .clicked()
            {
                self.request(client, runtime, "use-for-agent", json!({}));
            }
        });
        ui.small(
            "Text inference only. Other model overrides are restored when you stop the provider.",
        );
        ui.separator();
        ui.label(egui::RichText::new("Build a direction").strong());
        let concepts: Vec<String> = self.status["concepts"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("affect_concept")
            .selected_text(&self.concept)
            .show_ui(ui, |ui| {
                for concept in concepts {
                    ui.selectable_value(&mut self.concept, concept.clone(), concept);
                }
            });
        if ui
            .add_enabled(
                !job_running && !self.pending,
                egui::Button::new("Build matched-pair vector"),
            )
            .clicked()
        {
            self.request(client, runtime, "build", json!({"concept": self.concept}));
        }
        ui.collapsing("Custom concept and matched examples", |ui| {
            ui.label("Concept name (lowercase letters, digits, underscores)");
            ui.text_edit_singleline(&mut self.custom_name);
            ui.label("JSON array of target/control pairs; use matching situations and separate examples for evaluation.");
            ui.add(egui::TextEdit::multiline(&mut self.custom_pairs).desired_rows(6).desired_width(f32::INFINITY).code_editor());
            if ui.add_enabled(!job_running && !self.pending, egui::Button::new("Build custom direction")).clicked() {
                match serde_json::from_str::<Value>(&self.custom_pairs) {
                    Ok(pairs) => {
                        self.concept = self.custom_name.clone();
                        self.request(client, runtime, "build", json!({"concept": self.custom_name, "pairs": pairs}));
                    }
                    Err(error) => self.error = Some(format!("Invalid pair JSON: {error}")),
                }
            }
        });
        if let Some(progress) = self.status["job"]["progress"].as_str() {
            ui.label(progress);
        }
        if job_running
            && ui
                .add_enabled(!self.pending, egui::Button::new("Cancel experiment"))
                .clicked()
        {
            self.request(client, runtime, "cancel", json!({}));
        }
        if let Some(error) = self.status["job"]["error"].as_str() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        ui.separator();
        ui.label(egui::RichText::new("Manual state").strong());
        let vectors = self.status["vectors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if vectors.is_empty() {
            ui.small("Build a vector to enable its control.");
        }
        for vector in &vectors {
            let Some(concept) = vector["concept"].as_str() else {
                continue;
            };
            let strength = self.strengths.entry(concept.to_owned()).or_default();
            if ui
                .add(
                    egui::Slider::new(strength, 0.0..=1.0)
                        .text(format!("{concept} (experimental)")),
                )
                .changed()
            {
                self.dirty = true;
            }
        }
        let max_layer = self.status["steerable_layer_end"].as_i64().unwrap_or(1);
        ui.horizontal(|ui| {
            ui.label("Layers");
            if ui
                .add(egui::DragValue::new(&mut self.layer_start).range(1..=max_layer))
                .changed()
            {
                self.dirty = true;
            }
            ui.label("through");
            if ui
                .add(egui::DragValue::new(&mut self.layer_end).range(1..=max_layer))
                .changed()
            {
                self.dirty = true;
            }
        });
        let total: f64 = self.strengths.values().sum();
        ui.small(format!("Combined strength: {total:.2} / 1.00. Changes apply at the next request and reload inference state."));
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.pending && total <= 1.0 && self.layer_start <= self.layer_end, egui::Button::new("Apply state")).clicked() {
                self.dirty = false;
                self.request(client, runtime, "profile", json!({"strengths": self.strengths, "layer_start": self.layer_start, "layer_end": self.layer_end}));
            }
            if ui.add_enabled(!self.pending, egui::Button::new("Neutral / reset")).clicked() {
                self.strengths.clear();
                self.dirty = false;
                self.request(client, runtime, "profile", json!({"strengths": {}}));
            }
        });
        ui.small(format!(
            "Applied profile: {}",
            self.status["applied_profile"]
        ));
        ui.separator();
        ui.label(egui::RichText::new("Compare neutral and steered output").strong());
        ui.add(
            egui::TextEdit::multiline(&mut self.test_prompt)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.add(egui::Slider::new(&mut self.test_strength, 0.01..=1.0).text("Comparison strength"));
        ui.horizontal(|ui| {
            ui.label("Token budget");
            ui.add(egui::DragValue::new(&mut self.test_tokens).range(4..=256));
            let built = vectors.iter().any(|v| v["concept"].as_str() == Some(self.concept.as_str()));
            if ui.add_enabled(built && !self.pending && !job_running, egui::Button::new("Run comparison")).clicked() {
                self.request(client, runtime, "compare", json!({"concept": self.concept, "strengths": [0, self.test_strength], "prompt": self.test_prompt, "max_tokens": self.test_tokens}));
            }
        });
        if let Some(records) = self.status["last_comparison"]["records"].as_array() {
            for record in records {
                ui.group(|ui| {
                    ui.label(format!(
                        "Strength {} · {} s · {}",
                        record["strength"],
                        record["seconds"],
                        record["finish_reason"].as_str().unwrap_or("")
                    ));
                    if let Some(content) = record["content"].as_str() {
                        ui.add(egui::Label::new(content).selectable(true).wrap());
                    }
                });
            }
            if let Some(path) = self.status["last_comparison"]["path"].as_str() {
                ui.small(format!("Saved report: {path}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exited_unselected_worker_can_be_stopped_before_retrying() {
        let mut panel = AffectLabPanel::new();
        assert!(!panel.can_stop());
        panel.accept_status(json!({
            "running": false,
            "used_by_agent": false,
            "error": "Local provider exited; Stop restores the previous provider",
        }));
        assert!(panel.can_stop());
    }
}
