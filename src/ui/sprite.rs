use eframe::egui::{self, RichText};

use super::avatar::AvatarSet;
use crate::api::AgentVisualState;

pub fn render_agent_sprite(
    ui: &mut egui::Ui,
    state: &AgentVisualState,
    avatars: Option<&mut AvatarSet>,
) {
    // Try to render avatar if available
    if let Some(avatar_set) = avatars {
        if let Some(avatar) = avatar_set.get_for_state(state) {
            // Update animation
            avatar.update();

            // Render avatar
            let texture = avatar.current_texture();
            let size = egui::vec2(36.0, 36.0);

            ui.add(egui::Image::new(texture).fit_to_exact_size(size));

            // Request repaint for animations
            if avatar.is_animated() {
                ui.ctx().request_repaint();
            }

            return;
        }
    }

    // Fallback to emoji if no avatar
    ui.label(
        RichText::new("P_")
            .monospace()
            .size(25.0)
            .color(super::theme::palette(ui).accent),
    );
}
