use crate::api::{AffectLabStart, ApiClient, CACHE_TYPES, MAX_CONTEXT_SIZE};
use crate::config::AgentConfig;
use eframe::egui;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

enum LabResult {
    Status(Value),
    Provider(Box<AgentConfig>),
}
struct LabReply {
    epoch: u64,
    poll: bool,
    action: String,
    mix_version: u64,
    result: Result<LabResult, String>,
}
#[derive(Clone, Default)]
struct ExamplePair {
    prompt: Option<String>,
    target: String,
    control: String,
}

pub struct AffectLabPanel {
    pub show: bool,
    pub open_settings: bool,
    settings: AffectLabStart,
    local_view: bool,
    status: Value,
    reply_tx: flume::Sender<LabReply>,
    reply_rx: flume::Receiver<LabReply>,
    pending_action: Option<String>,
    poll_pending: bool,
    epoch: u64,
    last_poll: Instant,
    error: Option<String>,
    poll_error: Option<String>,
    strengths: BTreeMap<String, f64>,
    layer_start: i64,
    layer_end: i64,
    gain: f64,
    dirty: bool,
    mix_version: u64,
    failed_mix: Option<u64>,
    last_edit: Instant,
    tab: usize,
    concept: String,
    examples: BTreeMap<String, Vec<ExamplePair>>,
    custom_name: String,
    discovery_label: String,
    discovery_definition: String,
    experiment_tokens: u32,
    study_concepts: Vec<String>,
    test_prompts: Vec<String>,
    test_tokens: u32,
    review_id: String,
    review_affect: String,
    review_quality: String,
    review_notes: String,
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
            open_settings: false,
            settings: AffectLabStart {
                model_path,
                ..Default::default()
            },
            local_view: false,
            status: json!({"running": false}),
            reply_tx,
            reply_rx,
            pending_action: None,
            poll_pending: false,
            epoch: 0,
            last_poll: Instant::now() - Duration::from_secs(2),
            error: None,
            poll_error: None,
            strengths: BTreeMap::new(),
            layer_start: 1,
            layer_end: 1,
            gain: 1.0,
            dirty: false,
            mix_version: 0,
            failed_mix: None,
            last_edit: Instant::now(),
            tab: 0,
            concept: "contentment".into(),
            examples: BTreeMap::new(),
            custom_name: String::new(),
            discovery_label: "melodramatic".into(),
            discovery_definition: String::new(),
            experiment_tokens: 256,
            study_concepts: vec![
                "contentment".into(),
                "satisfaction".into(),
                "excitement".into(),
            ],
            test_prompts: Vec::new(),
            test_tokens: 96,
            review_id: String::new(),
            review_affect: "unreviewed".into(),
            review_quality: "unreviewed".into(),
            review_notes: String::new(),
        }
    }

    fn request(
        &mut self,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
        action: &str,
        body: Value,
    ) {
        let poll = action == "status";
        if (poll && self.poll_pending) || (!poll && self.pending_action.is_some()) {
            return;
        }
        if poll {
            self.poll_pending = true;
        } else {
            self.epoch += 1;
            self.pending_action = Some(action.into());
            self.error = None;
        }
        let (epoch, mix_version) = (self.epoch, self.mix_version);
        let client = client.clone();
        let action = action.to_owned();
        let sender = self.reply_tx.clone();
        runtime.spawn(async move {
            let result: anyhow::Result<LabResult> = async {
                if poll {
                    return client.affect_lab_status().await.map(LabResult::Status);
                }
                if action == "stop" || action == "use-for-agent" {
                    return client
                        .affect_lab_provider_action(&action)
                        .await
                        .map(|c| LabResult::Provider(Box::new(c)));
                }
                let status = client.affect_lab_action(&action, body).await?;
                // Starting the worker inspects metadata; loading actually allocates
                // weights/KV in an asynchronous, cancellable UI-owned job.
                if action == "start" {
                    client
                        .affect_lab_action("load", json!({}))
                        .await
                        .map(LabResult::Status)
                } else {
                    Ok(LabResult::Status(status))
                }
            }
            .await;
            let _ = sender.send(LabReply {
                epoch,
                poll,
                action,
                mix_version,
                result: result.map_err(|e| e.to_string()),
            });
        });
    }

    fn accept_status(&mut self, value: Value) {
        if !self.dirty {
            if let Some(profile) = value.get("requested_profile") {
                self.strengths = profile["strengths"]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, v)| v.as_f64().map(|s| (k.clone(), s)))
                            .collect()
                    })
                    .unwrap_or_default();
                self.layer_start = profile["layer_start"].as_i64().unwrap_or(1);
                self.layer_end = profile["layer_end"].as_i64().unwrap_or(1);
                self.gain = profile["gain"].as_f64().unwrap_or(1.0);
            }
        }
        if let Some(library) = value["example_library"].as_array() {
            for item in library {
                if let Some(name) = item["concept"].as_str() {
                    self.examples.entry(name.into()).or_insert_with(|| {
                        item["pairs"]
                            .as_array()
                            .map(|pairs| {
                                pairs
                                    .iter()
                                    .filter_map(|p| {
                                        Some(ExamplePair {
                                            prompt: p["prompt"].as_str().map(str::to_owned),
                                            target: p["target"].as_str()?.into(),
                                            control: p["control"].as_str()?.into(),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    });
                }
            }
        }
        if self.test_prompts.is_empty() {
            self.test_prompts = read_strings(&value["test_prompts"]);
        }
        let report = &value["last_comparison"];
        if let Some(id) = report["id"].as_str() {
            if id != self.review_id {
                self.review_id = id.into();
                self.review_affect = report["review"]["affect"]
                    .as_str()
                    .unwrap_or("unreviewed")
                    .into();
                self.review_quality = report["review"]["quality"]
                    .as_str()
                    .unwrap_or("unreviewed")
                    .into();
                self.review_notes = report["review"]["notes"].as_str().unwrap_or("").into();
            }
        }
        self.status = value;
    }

    fn handle_reply(&mut self, reply: LabReply) -> Option<AgentConfig> {
        if reply.poll {
            self.poll_pending = false;
        }
        // A poll launched before a mutation cannot roll its status back.
        if reply.epoch != self.epoch {
            return None;
        }
        if !reply.poll {
            self.pending_action = None;
        }
        match reply.result {
            Ok(LabResult::Status(value)) => {
                if reply.poll {
                    self.poll_error = None;
                }
                if reply.action == "profile" && reply.mix_version == self.mix_version {
                    self.dirty = false;
                    self.failed_mix = None;
                }
                self.accept_status(value);
            }
            Ok(LabResult::Provider(config)) => {
                if reply.action == "stop" {
                    self.dirty = false;
                    self.strengths.clear();
                    self.status = json!({"running": false});
                }
                self.last_poll = Instant::now() - Duration::from_secs(2);
                return Some(*config);
            }
            Err(error) => {
                if reply.poll {
                    self.poll_error = Some(error);
                } else {
                    if reply.action == "profile" {
                        self.failed_mix = Some(reply.mix_version);
                    }
                    self.error = Some(error);
                }
            }
        }
        None
    }

    pub fn tick(
        &mut self,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
        settings_visible: bool,
    ) -> Option<AgentConfig> {
        let mut provider = None;
        while let Ok(reply) = self.reply_rx.try_recv() {
            if let Some(c) = self.handle_reply(reply) {
                provider = Some(c);
            }
        }
        if self.mix_due() {
            self.request(client, runtime, "profile", self.profile());
        }
        if (self.show || settings_visible)
            && self.pending_action.is_none()
            && !self.poll_pending
            && self.last_poll.elapsed() >= Duration::from_secs(1)
        {
            self.last_poll = Instant::now();
            self.request(client, runtime, "status", Value::Null);
        }
        provider
    }
    fn running(&self) -> bool {
        self.status["running"].as_bool().unwrap_or(false)
    }
    fn job_running(&self) -> bool {
        self.status["job"]["phase"].as_str() == Some("running")
    }
    fn can_stop(&self) -> bool {
        self.running()
            || self.status["used_by_agent"].as_bool().unwrap_or(false)
            || self.status["error"].is_string()
    }
    fn available(&self) -> bool {
        self.pending_action.is_none() && !self.job_running()
    }
    pub fn provider_change_pending(&self) -> bool {
        matches!(
            self.pending_action.as_deref(),
            Some("stop" | "use-for-agent")
        )
    }
    fn total(&self) -> f64 {
        self.strengths.values().map(|s| s.abs()).sum()
    }
    fn profile(&self) -> Value {
        json!({"strengths": self.strengths.iter().filter(|(_, s)| **s != 0.0).collect::<BTreeMap<_, _>>(), "layer_start": self.layer_start, "layer_end": self.layer_end, "gain": self.gain})
    }
    fn edited(&mut self) {
        self.dirty = true;
        self.mix_version += 1;
        self.last_edit = Instant::now();
        self.failed_mix = None;
    }
    fn mix_due(&self) -> bool {
        self.running()
            && self.available()
            && self.dirty
            && self.failed_mix != Some(self.mix_version)
            && self.last_edit.elapsed() >= Duration::from_millis(450)
            && self.total() <= 1.0 + 1e-8
            && self.layer_start <= self.layer_end
    }
    fn errors(&self, ui: &mut egui::Ui) {
        if let Some(error) = self
            .error
            .as_deref()
            .or_else(|| self.status["error"].as_str())
        {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if let Some(error) = &self.poll_error {
            ui.small(format!("Status temporarily unavailable: {error}"));
        }
    }
    fn activity(
        &mut self,
        ui: &mut egui::Ui,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        // Status polling never disables controls, inserts spinners or changes layout.
        ui.label(
            self.pending_action
                .as_ref()
                .map(|a| format!("Sending {a}…"))
                .unwrap_or_else(|| {
                    self.status["job"]["progress"]
                        .as_str()
                        .unwrap_or("Ready")
                        .into()
                }),
        );
        if self.job_running()
            && ui
                .add_enabled(
                    self.pending_action.is_none(),
                    egui::Button::new("Cancel job"),
                )
                .clicked()
        {
            self.request(client, runtime, "cancel", json!({}));
        }
        if let Some(error) = self.status["job"]["error"].as_str() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }

    pub fn render_connection(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut AgentConfig,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Model connection");
        let selected = config.llm_model == "ponderer-local-gguf"
            || self.status["used_by_agent"].as_bool().unwrap_or(false);
        egui::ComboBox::from_id_salt("model_connection_kind")
            .selected_text(if self.local_view {
                "Local GGUF · activation steering"
            } else {
                "API · remote or externally hosted"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.local_view,
                    false,
                    "API · remote or externally hosted",
                );
                ui.selectable_value(
                    &mut self.local_view,
                    true,
                    "Local GGUF · activation steering",
                );
            });
        ui.small("This dropdown changes the editor, not the running provider.");
        self.errors(ui);
        if !self.local_view {
            if selected {
                ui.label("The local GGUF is selected for this session. Restore the API provider before editing its connection.");
                if ui
                    .add_enabled(
                        self.pending_action.is_none(),
                        egui::Button::new("Stop local model / restore API"),
                    )
                    .clicked()
                {
                    self.request(client, runtime, "stop", json!({}));
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label("API URL");
                    ui.text_edit_singleline(&mut config.llm_api_url);
                });
                ui.small("For example: http://localhost:11434 (Ollama)");
                ui.horizontal(|ui| {
                    ui.label("Model");
                    ui.text_edit_singleline(&mut config.llm_model);
                });
                ui.horizontal(|ui| {
                    ui.label("API key (optional)");
                    let mut key = config.llm_api_key.clone().unwrap_or_default();
                    if ui
                        .add(egui::TextEdit::singleline(&mut key).password(true))
                        .changed()
                    {
                        config.llm_api_key = (!key.is_empty()).then_some(key);
                    }
                });
                ui.small("Save & Apply saves this API connection. Hosted APIs cannot inject activation vectors.");
            }
            return;
        }
        let editable = !self.can_stop() && self.pending_action.is_none();
        ui.label("GGUF file or LM Studio model directory");
        ui.add_enabled(
            editable,
            egui::TextEdit::singleline(&mut self.settings.model_path).desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(editable, egui::Button::new("Choose GGUF…"))
            .clicked()
        {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("GGUF model", &["gguf"])
                .pick_file()
            {
                self.settings.model_path = p.to_string_lossy().into_owned();
            }
        }
        ui.add_enabled_ui(editable, |ui| {
            ui.label("llama-server executable"); ui.add(egui::TextEdit::singleline(&mut self.settings.server_binary).desired_width(f32::INFINITY));
            if let Some(cuda) = dirs::home_dir().map(|home| home.join("Code/llama.cpp-cuda/build/bin/llama-server")).filter(|p| p.is_file()) {
                if ui.small_button("Use detected CUDA engine").clicked() { self.settings.server_binary = cuda.to_string_lossy().into_owned(); }
            }
            ui.horizontal(|ui| {
                ui.label("GPU layers"); ui.add(egui::DragValue::new(&mut self.settings.gpu_layers).range(0..=999));
                ui.label("CPU threads"); ui.add(egui::DragValue::new(&mut self.settings.threads).range(1..=128));
            });
            ui.small("Zero GPU layers means CPU. Offload requires a GPU-capable executable as well; extraction currently uses CPU.");
            ui.collapsing("Context, KV cache and attention", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Context tokens"); ui.add(egui::DragValue::new(&mut self.settings.context_size).range(1024..=MAX_CONTEXT_SIZE));
                    if ui.button("200k / Q4_1 preset").clicked() { self.settings.apply_200k_preset(); }
                });
                ui.checkbox(&mut self.settings.unified_kv_cache, "Unified KV cache");
                ui.horizontal_wrapped(|ui| {
                    for (label, value) in [("K cache", &mut self.settings.cache_type_k), ("V cache", &mut self.settings.cache_type_v)] {
                        ui.label(label); egui::ComboBox::from_id_salt(label).selected_text(value.as_str()).show_ui(ui, |ui| {
                            for kind in CACHE_TYPES { ui.selectable_value(value, (*kind).into(), *kind); }
                        });
                    }
                    ui.label("Flash attention"); egui::ComboBox::from_id_salt("affect_flash").selected_text(&self.settings.flash_attention).show_ui(ui, |ui| {
                        for kind in ["auto", "on", "off"] { ui.selectable_value(&mut self.settings.flash_attention, kind.into(), kind); }
                    });
                });
                ui.small("Quantized V needs flash attention and compatible kernels. The preset leaves executable/GPU layers unchanged. Full-window performance remains unbenchmarked.");
            });
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    editable && !self.settings.model_path.is_empty(),
                    egui::Button::new("Load local model"),
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
                    self.running() && self.available() && !self.status["native_pid"].is_number(),
                    egui::Button::new("Load weights / retry"),
                )
                .clicked()
            {
                self.request(client, runtime, "load", json!({}));
            }
            if ui
                .add_enabled(
                    self.running() && !selected && self.available(),
                    egui::Button::new("Use for this session"),
                )
                .clicked()
            {
                self.request(client, runtime, "use-for-agent", json!({}));
            }
            if ui
                .add_enabled(
                    self.can_stop() && self.pending_action.is_none(),
                    egui::Button::new("Stop / restore provider"),
                )
                .clicked()
            {
                self.request(client, runtime, "stop", json!({}));
            }
        });
        if self.running() {
            ui.label(format!(
                "{} · {} · native process {} · {}",
                self.status["model"]["name"].as_str().unwrap_or("GGUF"),
                self.status["model"]["architecture"].as_str().unwrap_or(""),
                self.status["native_pid"],
                if selected {
                    "agent selected"
                } else {
                    "agent still on API"
                }
            ));
            ui.small(format!(
                "{} tokens · GPU layers {} · K {} / V {} · flash {} · unified KV {}",
                self.status["context_size"],
                self.status["gpu_layers"],
                self.status["inference_settings"]["cache_type_k"],
                self.status["inference_settings"]["cache_type_v"],
                self.status["inference_settings"]["flash_attention"],
                self.status["inference_settings"]["unified_kv_cache"]
            ));
            let trained = self.status["model"]["trained_context"]
                .as_u64()
                .unwrap_or(0);
            if trained > 0 && self.status["context_size"].as_u64().unwrap_or(0) > trained {
                ui.colored_label(egui::Color32::YELLOW, format!("Requested context exceeds declared {trained}-token context; no extra RoPE scaling."));
            }
            if ui.button("Open Affect Lab").clicked() {
                self.show = true;
            }
        }
        self.activity(ui, client, runtime);
        ui.small("Local options are session-only; Save & Apply does not load weights. All managed processes stop with this UI. Other model overrides are restored on Stop.");
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        if !self.show {
            return;
        }
        let mut open = self.show;
        egui::Window::new("Affect Lab").open(&mut open).default_width(740.0).default_height(700.0).show(ctx, |ui| {
            ui.label("Experimental activation steering — controls, examples and evidence.");
            ui.small("Model-specific interventions, not measured feelings or proof of subjective experience."); self.errors(ui);
            if !self.running() {
                ui.label("Load a local GGUF in Settings → General → Model connection to enable this lab. API connections cannot inject vectors.");
                if ui.button("Open model settings").clicked() { self.open_settings = true; self.local_view = true; }
                return;
            }
            ui.horizontal_wrapped(|ui| {
                for (i, label) in ["Affect mixer", "Example library", "Test & evidence", "Discover lever"].iter().enumerate() { ui.selectable_value(&mut self.tab, i, *label); }
                if ui.button("Model settings").clicked() { self.open_settings = true; self.local_view = true; }
            });
            self.activity(ui, client, runtime); ui.separator();
            egui::ScrollArea::vertical().id_salt("affect_lab_content").show(ui, |ui| match self.tab {
                0 => self.mixer(ui), 1 => self.library(ui, client, runtime), 2 => self.evidence(ui, client, runtime), _ => self.discovery(ui, client, runtime),
            });
        });
        self.show = open;
    }

    fn mixer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mix affects");
        ui.small("Changes are sent after a 450 ms pause and take effect at the next request. Changing the mix reloads the native engine and discards KV cache; avoid frequent changes during long tasks.");
        let vectors = self.status["vectors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let concepts: std::collections::BTreeSet<String> = read_strings(&self.status["concepts"])
            .into_iter()
            .chain(self.examples.keys().cloned())
            .collect();
        for concept in concepts {
            let built = vectors
                .iter()
                .any(|v| v["concept"].as_str() == Some(&concept));
            let mut value = self.strengths.get(&concept).copied().unwrap_or(0.0);
            let remaining = (1.0 - self.total() + value.abs()).clamp(0.0, 1.0);
            let slot_available =
                value != 0.0 || self.strengths.values().filter(|s| **s != 0.0).count() < 8;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        built && slot_available,
                        egui::Slider::new(&mut value, -remaining..=remaining)
                            .text(&concept)
                            .fixed_decimals(2),
                    )
                    .changed()
                {
                    self.strengths.insert(concept.clone(), value);
                    self.edited();
                }
                if !built {
                    if ui.small_button("Review examples →").clicked() {
                        self.concept = concept.clone();
                        self.tab = 1;
                    }
                } else {
                    ui.weak(if slot_available {
                        "experimental · uncalibrated"
                    } else {
                        "8 controls active; lower one first"
                    });
                }
            });
        }
        ui.label(format!("Combined intervention: {:.2} / 1.00", self.total()));
        ui.small("Signed native multipliers, not measured mood levels. Positive/negative polarity must be tested; zero = neutral. Absolute strengths share a budget of one: opposite signs cannot cancel it. Equal strengths need not have equal effects.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Neutral / reset all").clicked() {
                self.strengths.clear();
                self.gain = 1.0;
                // Reset must remain possible even after invalid draft layer edits.
                self.layer_start = self.status["requested_profile"]["layer_start"]
                    .as_i64()
                    .unwrap_or(1);
                self.layer_end = self.status["requested_profile"]["layer_end"]
                    .as_i64()
                    .unwrap_or(1);
                self.edited();
            }
            if self.failed_mix == Some(self.mix_version) && ui.button("Retry sending mix").clicked()
            {
                self.failed_mix = None;
            }
            if ui.button("Test this mix →").clicked() {
                self.tab = 2;
            }
        });
        ui.label(if self.dirty {
            "Mix not acknowledged yet"
        } else {
            "Mix acknowledged for the next request"
        });
        let active = &self.status["applied_profile"]["strengths"];
        ui.small(format!(
            "Native engine's current mix: {}",
            if active.is_null() {
                "not loaded".into()
            } else {
                active.to_string()
            }
        ));
        if !self.status["used_by_agent"].as_bool().unwrap_or(false) {
            ui.colored_label(egui::Color32::YELLOW, "The agent still uses its API. This mix affects lab tests only until you select the local provider in Settings.");
        }
        ui.collapsing("Advanced: layer range", |ui| {
            if ui.add(egui::Slider::new(&mut self.gain, 1.0..=4.0).text("Experimental amplification")).changed() { self.edited(); }
            ui.small("Amplification multiplies the entire mix; default 1, hard limit 4. Larger interventions can distort tasks. Retest before using them for agent inference.");
            let max = self.status["steerable_layer_end"].as_i64().unwrap_or(1);
            ui.horizontal(|ui| {
                ui.label("Layers"); let a = ui.add(egui::DragValue::new(&mut self.layer_start).range(1..=max)).changed();
                ui.label("through"); let b = ui.add(egui::DragValue::new(&mut self.layer_end).range(1..=max)).changed();
                if a || b { self.edited(); }
            });
            if self.layer_start > self.layer_end { ui.colored_label(egui::Color32::LIGHT_RED, "Start layer must not exceed end layer."); }
            ui.small("Layer changes alter the intervention; retest rather than comparing scores across ranges.");
        });
    }

    fn library(
        &mut self,
        ui: &mut egui::Ui,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Examples → affect control");
        ui.small("Matching situations differ in the proposed target/control state. Extraction averages their activation differences and normalizes each layer. Topic, wording and persona can still be confounds. Review starter examples; keep evaluation prompts separate.");
        egui::ComboBox::from_id_salt("example_concept")
            .selected_text(&self.concept)
            .show_ui(ui, |ui| {
                for name in self.examples.keys() {
                    ui.selectable_value(&mut self.concept, name.clone(), name);
                }
            });
        ui.small("Examples below are editable drafts; changes affect a built control only after rebuilding it.");
        let source = self.status["example_library"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|v| v["concept"].as_str() == Some(&self.concept))
            })
            .cloned();
        if let Some(source) = source {
            if ui
                .small_button("Restore saved / starter examples")
                .clicked()
            {
                let pairs = source["pairs"]
                    .as_array()
                    .map(|pairs| {
                        pairs
                            .iter()
                            .filter_map(|p| {
                                Some(ExamplePair {
                                    prompt: p["prompt"].as_str().map(str::to_owned),
                                    target: p["target"].as_str()?.into(),
                                    control: p["control"].as_str()?.into(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.examples.insert(self.concept.clone(), pairs);
            }
        }
        if let Some(vector) = self.status["vectors"].as_array().and_then(|items| {
            items
                .iter()
                .find(|v| v["concept"].as_str() == Some(&self.concept))
        }) {
            ui.collapsing("Built control provenance / geometry", |ui| {
                ui.small(format!("Model SHA-256: {}", vector["model_sha256"]));
                ui.small(format!("Recipe SHA-256: {}", vector["recipe_sha256"]));
                ui.small(format!("Geometry: {}", vector["geometry"]));
                ui.small("Finite unit-length directions indicate file integrity only; behavioral specificity is uncalibrated.");
            });
        }
        ui.horizontal(|ui| {
            ui.label("New affect name"); ui.text_edit_singleline(&mut self.custom_name);
            if ui.button("Add affect").clicked() {
                let name = self.custom_name.trim();
                if valid_concept_name(name) && !self.examples.contains_key(name) {
                    self.concept = name.into(); self.examples.insert(name.into(), vec![ExamplePair::default(); 2]); self.custom_name.clear();
                } else { self.error = Some("Use a new lowercase name, starting with a letter: letters, digits, underscores, max 41 characters.".into()); }
            }
        });
        if let Some(pairs) = self.examples.get_mut(&self.concept) {
            let mut remove = None;
            for (i, pair) in pairs.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("Matched situation {}", i + 1));
                            if ui.small_button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.label("Target state");
                        if let Some(prompt) = &mut pair.prompt {
                            ui.label("Shared user task (same in both conditions)");
                            ui.add(
                                egui::TextEdit::multiline(prompt)
                                    .char_limit(512)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        ui.add(
                            egui::TextEdit::multiline(&mut pair.target)
                                .char_limit(512)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                        ui.label("Control / contrast state");
                        ui.add(
                            egui::TextEdit::multiline(&mut pair.control)
                                .char_limit(512)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            }
            if let Some(i) = remove {
                pairs.remove(i);
            }
            if ui
                .add_enabled(pairs.len() < 64, egui::Button::new("Add matched situation"))
                .clicked()
            {
                pairs.push(ExamplePair::default());
            }
        }
        let pairs = self
            .examples
            .get(&self.concept)
            .cloned()
            .unwrap_or_default();
        let valid = (2..=64).contains(&pairs.len())
            && pairs.iter().all(|p| {
                !p.target.is_empty()
                    && !p.control.is_empty()
                    && p.target != p.control
                    && !p.target.contains('\0')
                    && !p.control.contains('\0')
                    && p.prompt
                        .as_ref()
                        .is_none_or(|task| !task.trim().is_empty() && !task.contains('\0'))
            });
        let built = self.status["vectors"].as_array().is_some_and(|v| {
            v.iter()
                .any(|v| v["concept"].as_str() == Some(&self.concept))
        });
        ui.small(format!("{} matched examples · {}. Rebuilding replaces this affect's control; repeat its tests.", pairs.len(), if built { "control exists" } else { "not built yet" }));
        if ui
            .add_enabled(
                self.available() && valid,
                egui::Button::new(format!(
                    "{} {} control from these examples",
                    if built { "Rebuild" } else { "Create" },
                    self.concept
                )),
            )
            .clicked()
        {
            let values: Vec<Value> = pairs
                .iter()
                .map(|p| json!({"prompt": p.prompt, "target": p.target, "control": p.control}))
                .collect();
            self.request(
                client,
                runtime,
                "build",
                json!({"concept": self.concept, "pairs": values}),
            );
        }
        if !valid {
            ui.small(
                "Provide 2–64 different, nonempty target/control pairs; each text ≤512 characters.",
            );
        }
        if ui.button("Back to mixer").clicked() {
            self.tab = 0;
        }
    }

    fn discovery(
        &mut self,
        ui: &mut egui::Ui,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Find a more / less lever from a label");
        ui.small("The neutral local model defines the construct, generates eight matched response pairs, derives a vector, searches two layer ranges and both signs, and confirms on new tasks with two seeds and matched shuffled controls. It does not change your agent mix. This can take several minutes and waits for current inference.");
        ui.horizontal(|ui| {
            ui.label("Mood or style");
            ui.add(
                egui::TextEdit::singleline(&mut self.discovery_label)
                    .char_limit(80)
                    .hint_text("melodramatic, curious, serene…"),
            );
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.discovery_definition)
                .char_limit(1000)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("Optional: what observable behavior do you mean by this word?"),
        );
        ui.add(
            egui::Slider::new(&mut self.experiment_tokens, 64..=256).text("Tokens per test output"),
        );
        ui.small("Probe tasks are fixed independently of the label. Half are ordinary; half share a mild style cue across every condition so suppression can be measured without a floor effect. Native vector polarity is calibrated, not assumed.");
        if ui
            .add_enabled(
                self.available() && !self.discovery_label.trim().is_empty(),
                egui::Button::new("Discover and test signed lever"),
            )
            .clicked()
        {
            self.request(client, runtime, "discover", json!({"label": self.discovery_label.trim(), "definition": self.discovery_definition, "max_tokens": self.experiment_tokens}));
        }
        let report = self.status["last_discovery"].clone();
        if report["label"].as_str() == Some(self.discovery_label.trim())
            && report["current_artifacts"].as_bool().unwrap_or(false)
            && !report["plan"].is_null()
            && ui
                .add_enabled(
                    self.available(),
                    egui::Button::new("Retest this exact vector (no rebuild)"),
                )
                .clicked()
        {
            self.request(client, runtime, "discover", json!({"label": self.discovery_label.trim(), "reuse_concept": report["concept"], "max_tokens": self.experiment_tokens}));
        }
        if report.is_null() {
            return;
        }
        ui.separator();
        ui.label(format!(
            "{} · {} · control {}",
            report["label"].as_str().unwrap_or(""),
            report["phase"].as_str().unwrap_or(""),
            report["concept"].as_str().unwrap_or("")
        ));
        if let Some(definition) = report["plan"]["definition"].as_str() {
            ui.label(definition);
        }
        if let Some(opposite) = report["plan"]["opposite"].as_str() {
            ui.small(format!("Low end: {opposite}"));
        }
        if let Some(criteria) = report["plan"]["rubric"].as_array() {
            for criterion in criteria {
                ui.small(format!("• {}", criterion.as_str().unwrap_or("")));
            }
        }
        if report["lever_found"].as_bool().unwrap_or(false) {
            ui.colored_label(egui::Color32::LIGHT_GREEN, "Both directions passed this exploratory model-judged confirmation. Not independent validation.");
            let current = report["current_artifacts"].as_bool().unwrap_or(false)
                && report["current_inference_settings"]
                    .as_bool()
                    .unwrap_or(false);
            if !current {
                ui.colored_label(egui::Color32::YELLOW, "Historical evidence: vectors or inference settings changed. Rerun before adopting these settings.");
            }
            ui.horizontal(|ui| {
                for (key, title) in [
                    ("less", "Use tested less setting"),
                    ("more", "Use tested more setting"),
                ] {
                    if ui
                        .add_enabled(self.available() && current, egui::Button::new(title))
                        .clicked()
                    {
                        let profile = &report["recommendations"][key];
                        self.strengths = profile["strengths"]
                            .as_object()
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(|(k, v)| v.as_f64().map(|v| (k.clone(), v)))
                                    .collect()
                            })
                            .unwrap_or_default();
                        self.layer_start = profile["layer_start"].as_i64().unwrap_or(1);
                        self.layer_end = profile["layer_end"].as_i64().unwrap_or(1);
                        self.gain = profile["gain"].as_f64().unwrap_or(1.0);
                        self.edited();
                        self.tab = 0;
                    }
                }
            });
        } else if report["phase"] == "complete" {
            ui.colored_label(egui::Color32::YELLOW, "No reliable two-sided lever passed the confirmation criteria. The derived vector remains experimental.");
        }
        if let Some(reasons) = report["failure_reasons"].as_array() {
            for reason in reasons {
                ui.label(reason.as_str().unwrap_or(""));
            }
        }
        if let Some(error) = report["error"].as_str() {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        ui.collapsing("More / less effects vs neutral and placebo", |ui| {
            ui.small("Scores are 0–4 rubric judgments of observable output. Paired bootstrap intervals are exploratory and conditional on the chosen settings and this judge.");
            if let Some(effects) = report["effects"].as_object() {
                for (condition, effect) in effects { ui.label(format!("{condition}: Δ {} · interval {} · {} paired outputs", effect["mean_delta"], effect["bootstrap_95"], effect["pairs"])); }
            }
            if let Some(strata) = report["strata"].as_object() {
                for (stratum, effects) in strata {
                    ui.small(format!("{stratum} tasks: more Δ {} · less Δ {}", effects["more"]["mean_delta"], effects["less"]["mean_delta"]));
                }
            }
            if let Some(summary) = report["confirmation_summary"].as_object() { evidence_summary(ui, summary); }
        });
        ui.collapsing("Selection search — settings and actual outputs", |ui| {
            if let Some(summary) = report["selection_summary"].as_object() {
                evidence_summary(ui, summary);
            }
            output_gallery(ui, &report["selection"], "discovery_selection");
        });
        ui.collapsing("Untouched confirmation tasks — actual outputs", |ui| {
            output_gallery(ui, &report["confirmation"], "discovery_confirmation");
        });
        ui.collapsing("Accuracy / format outputs", |ui| {
            output_gallery(ui, &report["integrity_controls"], "discovery_integrity");
        });
        ui.small("The same local model creates examples and judges outputs, although judging runs neutral and hides conditions. Independent human/other-model review is still necessary. A failed discovery is reported as failed, not relabeled a success.");
        if let Some(path) = report["path"].as_str() {
            ui.small(format!(
                "Reproducible report, prompts and fingerprints: {path}"
            ));
            if ui.small_button("Copy discovery report path").clicked() {
                ui.ctx().copy_text(path.into());
            }
        }
    }

    fn response_study(
        &mut self,
        ui: &mut egui::Ui,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Explore individual controls and combinations");
        ui.small("Pick up to four built controls. Test their positive/negative directions and combinations on six tasks with two seeds, including accuracy/format checks. The neutral model judges shuffled anonymous outputs; raw responses and quality failures remain visible. This is a larger study, not a smoke test.");
        ui.add(
            egui::Slider::new(&mut self.experiment_tokens, 64..=256).text("Tokens per test output"),
        );
        let built: Vec<String> = self.status["vectors"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v["concept"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let selected: Vec<_> = self
            .study_concepts
            .iter()
            .filter(|c| built.contains(c))
            .cloned()
            .collect();
        ui.horizontal_wrapped(|ui| {
            for name in &built {
                let mut on = self.study_concepts.contains(name);
                if ui
                    .add_enabled(on || selected.len() < 4, egui::Checkbox::new(&mut on, name))
                    .changed()
                {
                    if on {
                        self.study_concepts.push(name.clone());
                    } else {
                        self.study_concepts.retain(|c| c != name);
                    }
                }
            }
        });
        if ui
            .add_enabled(
                self.available() && !selected.is_empty(),
                egui::Button::new("Run signed-control and mixture study"),
            )
            .clicked()
        {
            self.request(
                client,
                runtime,
                "study",
                json!({"concepts": selected, "max_tokens": self.experiment_tokens}),
            );
        }
        let report = self.status["last_study"].clone();
        if !report.is_null() {
            ui.label(format!(
                "Last study: {}",
                report["phase"].as_str().unwrap_or("")
            ));
            if let Some(summary) = report["summary"].as_object() {
                evidence_summary(ui, summary);
            }
            output_gallery(ui, &report["records"], "response_study");
            if let Some(error) = report["error"].as_str() {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if let Some(path) = report["path"].as_str() {
                ui.small(format!("Raw study report: {path}"));
            }
        }
    }

    fn evidence(
        &mut self,
        ui: &mut egui::Ui,
        client: &ApiClient,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Does the mix do what you intend?");
        ui.small("Compare your slider mix with neutral and half strength on identical held-out prompts: temperature 0, seed 42, fresh cache for each condition. Your agent mix is preserved. Tests run serially, may reload weights three times, and wait for active inference.");
        ui.label(format!("Mix to test: {}", self.profile()));
        ui.collapsing("Larger response-variation study", |ui| {
            self.response_study(ui, client, runtime);
        });
        ui.collapsing("Review / edit held-out prompts", |ui| {
            for (i, prompt) in self.test_prompts.iter_mut().enumerate() { ui.push_id(i, |ui| {
                ui.label(format!("Prompt {}", i + 1)); ui.add(egui::TextEdit::multiline(prompt).char_limit(2000).desired_rows(2).desired_width(f32::INFINITY));
            }); }
            if ui.button("Restore starter test suite").clicked() { self.test_prompts = read_strings(&self.status["test_prompts"]); }
            ui.small("Review third-person leakage manually. Arithmetic and exact-JSON checks are automatic only on unchanged starter prompts. These are tiny smoke checks, not a quality benchmark.");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Output tokens / prompt"); ui.add(egui::DragValue::new(&mut self.test_tokens).range(4..=256));
            let valid = !self.test_prompts.is_empty() && self.test_prompts.iter().all(|p| !p.is_empty());
            if ui.add_enabled(self.available() && !self.dirty && self.total() > 0.0 && valid, egui::Button::new("Compare neutral / half / full mix")).clicked() {
                self.request(client, runtime, "compare", json!({"profile": self.profile(), "prompts": self.test_prompts, "max_tokens": self.test_tokens}));
            }
        });
        if self.total() == 0.0 {
            ui.small("Choose at least one nonzero slider in the mixer first.");
        }
        let report = self.status["last_comparison"].clone();
        let Some(records) = report["records"].as_array() else {
            ui.separator();
            ui.label("No comparison yet. A finite, normalized vector proves file integrity, not that it captures the intended state.");
            validation_help(ui);
            return;
        };
        ui.separator();
        ui.label("Last completed comparison (may differ from your current sliders)");
        let checks: Vec<_> = records
            .iter()
            .filter(|r| r["integrity_pass"].is_boolean())
            .collect();
        let passes = checks
            .iter()
            .filter(|r| r["integrity_pass"] == true)
            .count();
        let truncated = records
            .iter()
            .filter(|r| r["finish_reason"].as_str() == Some("length"))
            .count();
        ui.label(format!(
            "Accuracy / format smoke checks: {passes}/{} passed · truncated outputs: {truncated}",
            checks.len()
        ));
        ui.small(format!(
            "Tested full profile: {}",
            records
                .last()
                .map(|r| &r["profile"])
                .unwrap_or(&Value::Null)
        ));
        for neutral in records
            .iter()
            .filter(|r| r["strength"].as_f64() == Some(0.0))
        {
            ui.group(|ui| {
                ui.label(egui::RichText::new(neutral["prompt"].as_str().unwrap_or("")).strong());
                for record in records
                    .iter()
                    .filter(|r| r["prompt_index"] == neutral["prompt_index"])
                {
                    let scale = record["strength"].as_f64().unwrap_or(0.0);
                    let label = if scale == 0.0 {
                        "Neutral".into()
                    } else if scale == 0.5 {
                        "Half mix".into()
                    } else if scale == 1.0 {
                        "Full mix".into()
                    } else {
                        format!("Scale {scale}")
                    };
                    let check = match record["integrity_pass"].as_bool() {
                        Some(true) => " · check passed",
                        Some(false) => " · CHECK FAILED",
                        None => "",
                    };
                    ui.label(format!(
                        "{label} · {} s · {}{check}",
                        record["seconds"],
                        record["finish_reason"].as_str().unwrap_or("")
                    ));
                    ui.add(
                        egui::Label::new(record["content"].as_str().unwrap_or(""))
                            .selectable(true)
                            .wrap(),
                    );
                    if scale != 0.0 {
                        ui.small(if record["content"] == neutral["content"] {
                            "Identical to neutral"
                        } else {
                            "Different from neutral — relevance needs review"
                        });
                    }
                }
            });
        }
        ui.heading("Your assessment");
        ui.small("Judge intended state/choice changes separately from quality. Emotion words alone are weak evidence; check third-person leakage, correctness and truncation.");
        review_choice(
            ui,
            "Intended affect is more evident?",
            &mut self.review_affect,
        );
        review_choice(ui, "Task quality is preserved?", &mut self.review_quality);
        ui.add(
            egui::TextEdit::multiline(&mut self.review_notes)
                .char_limit(4000)
                .hint_text("Observations, confounds, next test…")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                self.available(),
                egui::Button::new("Save assessment with this report"),
            )
            .clicked()
        {
            self.request(client, runtime, "review", json!({"id": self.review_id, "affect": self.review_affect, "quality": self.review_quality, "notes": self.review_notes}));
        }
        ui.small(format!("Saved assessment: {}", report["review"]));
        if let Some(path) = report["path"].as_str() {
            ui.small(format!("Report: {path}"));
        }
        ui.collapsing("Reproducibility / exact identities", |ui| {
            ui.small(format!("Model SHA-256: {}", report["model_sha256"]));
            ui.small(format!("Vectors/recipes: {}", report["vectors"]));
            ui.small(format!("Inference: {}", report["inference_settings"]));
            ui.small(format!("Generation: {}", report["generation"]));
        });
        validation_help(ui);
    }
}

fn evidence_summary(ui: &mut egui::Ui, summary: &serde_json::Map<String, Value>) {
    egui::Grid::new(ui.id().with("evidence_summary"))
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Condition");
            ui.strong("Construct scores / 4");
            ui.strong("Quality / 4");
            ui.strong("Checks · cut off");
            ui.end_row();
            for (condition, values) in summary {
                ui.label(condition);
                ui.label(values["affects"].to_string());
                ui.label(values["quality"].to_string());
                ui.label(format!(
                    "{}/{} · {}",
                    values["checks_passed"], values["checks_total"], values["truncated"]
                ));
                ui.end_row();
            }
        });
}

fn output_gallery(ui: &mut egui::Ui, records: &Value, salt: &str) {
    let Some(records) = records.as_array() else {
        return;
    };
    ui.push_id(salt, |ui| {
        for baseline in records.iter().filter(|r| r["condition"] == "neutral") {
            let title = format!(
                "Seed {} · {}",
                baseline["seed"],
                baseline["prompt"].as_str().unwrap_or("")
            );
            ui.collapsing(title, |ui| {
                for record in records.iter().filter(|r| {
                    r["prompt_index"] == baseline["prompt_index"] && r["seed"] == baseline["seed"]
                }) {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new(record["condition"].as_str().unwrap_or(""))
                                .strong(),
                        );
                        ui.small(format!(
                            "{} · {} · scores {} · {} seconds",
                            record["profile"],
                            record["finish_reason"],
                            record["scores"],
                            record["seconds"]
                        ));
                        if let Some(pass) = record["integrity_pass"].as_bool() {
                            ui.label(if pass {
                                "Exact task check passed"
                            } else {
                                "EXACT TASK CHECK FAILED"
                            });
                        }
                        ui.add(
                            egui::Label::new(record["content"].as_str().unwrap_or(""))
                                .selectable(true)
                                .wrap(),
                        );
                    });
                }
            });
        }
    });
}

fn read_strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn valid_concept_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 41
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn review_choice(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(value.as_str())
            .show_ui(ui, |ui| {
                for option in ["unreviewed", "yes", "mixed", "no"] {
                    ui.selectable_value(value, option.into(), option);
                }
            });
    });
}
fn validation_help(ui: &mut egui::Ui) {
    ui.collapsing("How to establish that an affect control is useful", |ui| {
        ui.label("1. Use varied, reviewed matched situations, separate from evaluation.\n2. Seek the intended behavior on unseen tasks at several strengths without losing accuracy or format.\n3. Test controls alone before mixing; mixtures can interact.\n4. Repeat with new examples, layers and tasks. Compare shuffled/placebo directions in a separate calibration study.");
        ui.small("Discover lever adds matched shuffled-vector controls, anonymous neutral judging, two confirmation seeds and task-clustered exploratory intervals. The larger mixture study measures response variation, not isolated causal specificity. Human/independent-model review is still needed; none of these tests establish subjective experience.");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply(
        panel: &AffectLabPanel,
        poll: bool,
        epoch: u64,
        action: &str,
        value: Value,
    ) -> LabReply {
        LabReply {
            epoch,
            poll,
            action: action.into(),
            mix_version: panel.mix_version,
            result: Ok(LabResult::Status(value)),
        }
    }
    #[test]
    fn exited_unselected_worker_can_be_stopped_before_retrying() {
        let mut p = AffectLabPanel::new();
        assert!(!p.can_stop());
        p.accept_status(
            json!({"running": false, "used_by_agent": false, "error": "Local provider exited"}),
        );
        assert!(p.can_stop());
    }
    #[test]
    fn quiet_poll_does_not_disable_actions_or_clear_errors() {
        let mut p = AffectLabPanel::new();
        p.poll_pending = true;
        p.error = Some("Action failed".into());
        assert!(p.available());
        let r = reply(&p, true, 0, "status", json!({"running": true}));
        p.handle_reply(r);
        assert!(p.available());
        assert_eq!(p.error.as_deref(), Some("Action failed"));
    }
    #[test]
    fn stale_poll_cannot_overwrite_newer_action() {
        let mut p = AffectLabPanel::new();
        p.epoch = 2;
        p.pending_action = Some("build".into());
        let r = reply(&p, true, 1, "status", json!({"running": true}));
        p.handle_reply(r);
        assert!(!p.running());
        assert_eq!(p.pending_action.as_deref(), Some("build"));
    }
    #[test]
    fn mix_ack_preserves_newer_slider_edits() {
        let mut p = AffectLabPanel::new();
        p.strengths.insert("contentment".into(), 0.25);
        p.edited();
        let r = reply(
            &p,
            false,
            0,
            "profile",
            json!({"running": true, "requested_profile": {"strengths": {"contentment": 0.25}, "layer_start": 1, "layer_end": 2}}),
        );
        p.strengths.insert("contentment".into(), 0.5);
        p.edited();
        p.handle_reply(r);
        assert!(p.dirty);
        assert_eq!(p.strengths["contentment"], 0.5);
    }
    #[test]
    fn mix_is_debounced_bounded_and_does_not_retry_failed_versions_forever() {
        let mut p = AffectLabPanel::new();
        p.status = json!({"running": true});
        p.edited();
        assert!(!p.mix_due());
        p.last_edit -= Duration::from_secs(1);
        assert!(p.mix_due());
        p.failed_mix = Some(p.mix_version);
        assert!(!p.mix_due());
        p.edited();
        p.last_edit -= Duration::from_secs(1);
        p.strengths.insert("contentment".into(), 1.1);
        assert!(!p.mix_due());
    }
    #[test]
    fn signed_mix_retains_negative_values_and_uses_absolute_budget() {
        let mut p = AffectLabPanel::new();
        p.accept_status(json!({"running": true, "requested_profile": {"strengths": {"contentment": -0.4, "excitement": 0.6}, "layer_start": 1, "layer_end": 2}}));
        assert_eq!(p.total(), 1.0);
        assert_eq!(p.profile()["strengths"]["contentment"], -0.4);
        p.edited();
        p.last_edit -= Duration::from_secs(1);
        assert!(p.mix_due());
        p.strengths.insert("contentment".into(), -0.6);
        assert!(!p.mix_due());
    }
    #[test]
    fn shared_example_tasks_survive_status_parsing() {
        let mut p = AffectLabPanel::new();
        p.accept_status(json!({"example_library": [{"concept": "melodramatic", "pairs": [{"prompt": "Tiny inconvenience", "target": "Grand tragedy", "control": "Small adjustment"}]}]}));
        assert_eq!(
            p.examples["melodramatic"][0].prompt.as_deref(),
            Some("Tiny inconvenience")
        );
        assert_eq!(p.experiment_tokens, 256);
    }
    #[test]
    fn polling_preserves_example_drafts() {
        let mut p = AffectLabPanel::new();
        p.examples.insert(
            "contentment".into(),
            vec![ExamplePair {
                prompt: None,
                target: "edited".into(),
                control: "control".into(),
            }],
        );
        p.accept_status(json!({"example_library": [{"concept": "contentment", "pairs": [{"target": "starter", "control": "neutral"}]}]}));
        assert_eq!(p.examples["contentment"][0].target, "edited");
        assert!(valid_concept_name("calm_2"));
        assert!(!valid_concept_name("../bad"));
    }

    #[test]
    fn background_poll_renders_identical_mixer_geometry_and_controls() {
        let mut p = AffectLabPanel::new();
        p.accept_status(json!({"running": true, "used_by_agent": true, "concepts": ["contentment", "excitement"], "vectors": [{"concept": "contentment"}, {"concept": "excitement"}], "requested_profile": {"strengths": {"contentment": 0.2, "excitement": 0.3}, "layer_start": 1, "layer_end": 2}}));
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.0);
        let render = |p: &mut AffectLabPanel| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| p.mixer(ui));
                },
            )
        };
        let _ = render(&mut p);
        let _ = render(&mut p);
        let idle = format!("{:?}", render(&mut p).shapes);
        p.poll_pending = true;
        assert_eq!(idle, format!("{:?}", render(&mut p).shapes));
        assert!(p.available());
    }

    #[test]
    fn all_lab_panes_and_connection_editors_render_headlessly() {
        let mut p = AffectLabPanel::new();
        p.show = true;
        p.accept_status(json!({"running": true, "concepts": ["contentment"], "vectors": [{"concept": "contentment"}], "example_library": [{"concept": "contentment", "pairs": [{"target": "content", "control": "neutral"}, {"target": "pleased", "control": "composed"}]}], "test_prompts": ["held-out task"], "last_comparison": {"id": "fixture", "records": [
            {"prompt_index": 0, "prompt": "held-out task", "strength": 0, "content": "neutral", "seconds": 0.1, "profile": {"strengths": {}}, "finish_reason": "stop"},
            {"prompt_index": 0, "prompt": "held-out task", "strength": 1, "content": "changed", "seconds": 0.1, "profile": {"strengths": {"contentment": 0.25}}, "finish_reason": "stop"}
        ]}}));
        let ctx = egui::Context::default();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = ApiClient::new("http://127.0.0.1:1".into(), None);
        p.status["last_discovery"] = json!({"label": "melodramatic", "concept": "melodramatic", "phase": "complete", "lever_found": false, "plan": {"definition": "Theatrical expression", "opposite": "Measured", "rubric": ["Imagery"]}, "failure_reasons": ["No reliable effect"]});
        for tab in 0..4 {
            p.tab = tab;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                p.render(ctx, &client, &runtime)
            });
            assert!(!output.shapes.is_empty());
            // Empty text/placeholders have nonfinite sentinel bounds in epaint;
            // validate actual rasterizable geometry instead.
            for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
                if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                    assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()), "tab {tab}");
                }
            }
        }
        let mut config = AgentConfig::default();
        for local in [false, true] {
            p.local_view = local;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    p.render_connection(ui, &mut config, &client, &runtime)
                });
            });
            assert!(!output.shapes.is_empty());
        }
    }
}
