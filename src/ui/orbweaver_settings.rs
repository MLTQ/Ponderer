use crate::config::AgentConfig;
use eframe::egui;

pub struct OrbWeaverSettingsPanel;

impl OrbWeaverSettingsPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn render_contents(&mut self, ui: &mut egui::Ui, config: &mut AgentConfig) {
        ui.heading("OrbWeaver / Graphchan");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Graphchan API URL:");
            ui.text_edit_singleline(&mut config.graphchan_api_url);
        });
        ui.label(
            egui::RichText::new(
                "This is the OrbWeaver/Graphchan endpoint used for polling and posting.",
            )
            .small()
            .weak(),
        );
        ui.label(
            egui::RichText::new(
                "Leave the URL configured here even when the integration is temporarily idle; the runtime will only poll when the endpoint is reachable.",
            )
            .small()
            .weak(),
        );
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("What this skill controls").small().strong());
            ui.label(
                egui::RichText::new(
                    "• Graphchan polling skill\n• Graphchan posting tools\n• Bridged skill actions exposed to the agentic loop",
                )
                .small()
                .weak(),
            );
        });
    }
}

impl Default for OrbWeaverSettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}
