//! One operator-selected base color drives the native workbench and instrument palette.
use crate::config::AppearanceConfig;
use eframe::egui::{self, Color32, Rounding, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub scope: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub soft: Color32,
    pub wire: Color32,
    pub trace_hot: Color32,
    pub warning: Color32,
    pub error: Color32,
}

fn blend(a: Color32, b: Color32, amount: f32) -> Color32 {
    let channel =
        |index| ((a[index] as f32 * (1.0 - amount)) + b[index] as f32 * amount).round() as u8;
    Color32::from_rgb(channel(0), channel(1), channel(2))
}

fn luminance(color: Color32) -> f32 {
    let linear = |channel: u8| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
}

fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Keep even white, black, gray and saturated yellow choices readable.
fn readable(color: Color32, background: Color32, dark: bool) -> Color32 {
    let endpoint = if dark { Color32::WHITE } else { Color32::BLACK };
    for step in 0..=100 {
        let candidate = blend(color, endpoint, step as f32 / 100.0);
        if contrast(candidate, background) >= 4.7 {
            return candidate;
        }
    }
    endpoint
}

impl Palette {
    pub fn from_config(config: &AppearanceConfig) -> Self {
        let base = Color32::from_rgb(
            config.base_color[0],
            config.base_color[1],
            config.base_color[2],
        );
        let gray = |dark, light| Color32::from_gray(if config.dark { dark } else { light });
        let background = blend(gray(16, 239), base, if config.dark { 0.055 } else { 0.07 });
        let panel = blend(gray(22, 249), base, 0.055);
        let raised = blend(gray(31, 229), base, 0.08);
        let scope = blend(gray(7, 230), base, 0.045);
        let soft = blend(panel, base, if config.dark { 0.14 } else { 0.13 });
        let reference = [background, panel, raised, scope, soft]
            .into_iter()
            .max_by(|a, b| {
                let order = luminance(*a).total_cmp(&luminance(*b));
                if config.dark {
                    order
                } else {
                    order.reverse()
                }
            })
            .unwrap();
        let accent = readable(base, reference, config.dark);
        Self {
            background,
            panel,
            raised,
            scope,
            border: blend(gray(60, 183), base, 0.13),
            text: readable(blend(gray(229, 26), base, 0.08), reference, config.dark),
            muted: readable(blend(gray(160, 85), base, 0.12), reference, config.dark),
            accent,
            soft,
            wire: blend(scope, accent, 0.46),
            trace_hot: readable(
                Color32::from_rgb(base.b(), base.r(), base.g()),
                reference,
                config.dark,
            ),
            // Safety meanings remain recognizably amber/red, independent of base hue.
            warning: readable(Color32::from_rgb(224, 172, 78), reference, config.dark),
            error: readable(Color32::from_rgb(235, 114, 97), reference, config.dark),
        }
    }
}

pub fn palette(ui: &egui::Ui) -> Palette {
    ui.ctx()
        .data(|data| data.get_temp::<Palette>(egui::Id::new("workbench_palette")))
        .unwrap_or_else(|| Palette::from_config(&AppearanceConfig::default()))
}

pub fn apply(ctx: &egui::Context, config: &AppearanceConfig) {
    let colors = Palette::from_config(config);
    // Avoid invalidating egui's style and font caches on every token repaint.
    let previous =
        ctx.data(|data| data.get_temp::<AppearanceConfig>(egui::Id::new("workbench_appearance")));
    if previous.as_ref() == Some(config) {
        return;
    }
    let mut visuals = if config.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = colors.background;
    visuals.window_fill = colors.panel;
    visuals.extreme_bg_color = colors.scope;
    visuals.faint_bg_color = colors.soft;
    visuals.code_bg_color = colors.soft;
    visuals.override_text_color = Some(colors.text);
    visuals.hyperlink_color = colors.accent;
    visuals.warn_fg_color = colors.warning;
    visuals.error_fg_color = colors.error;
    visuals.selection.bg_fill = colors.soft;
    visuals.selection.stroke = Stroke::new(1.0_f32, colors.accent);
    visuals.window_rounding = Rounding::same(1.0);
    visuals.menu_rounding = Rounding::same(1.0);
    visuals.window_stroke = Stroke::new(1.0_f32, colors.border);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.rounding = Rounding::same(1.0);
        widget.bg_fill = colors.panel;
        widget.weak_bg_fill = colors.panel;
        widget.bg_stroke = Stroke::new(1.0_f32, colors.border);
        widget.fg_stroke = Stroke::new(1.0_f32, colors.text);
    }
    visuals.widgets.hovered.bg_fill = colors.raised;
    visuals.widgets.hovered.weak_bg_fill = colors.soft;
    visuals.widgets.hovered.bg_stroke.color = colors.accent;
    visuals.widgets.active.bg_fill = colors.soft;
    visuals.widgets.active.weak_bg_fill = colors.soft;
    visuals.widgets.active.bg_stroke.color = colors.accent;
    visuals.widgets.open.bg_fill = colors.soft;
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(9.0, 5.0);
        style.spacing.window_margin = egui::Margin::same(12.0);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::monospace(18.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::monospace(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::monospace(11.0));
    });
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new("workbench_palette"), colors);
        data.insert_temp(egui::Id::new("workbench_appearance"), config.clone());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_base_colors_keep_text_readable_on_all_surfaces() {
        for dark in [true, false] {
            for base_color in [
                [0, 0, 0],
                [255, 255, 255],
                [128, 128, 128],
                [255, 255, 0],
                [0, 0, 255],
                [255, 0, 255],
                [168, 209, 139],
            ] {
                let p = Palette::from_config(&AppearanceConfig { base_color, dark });
                for surface in [p.background, p.panel, p.raised, p.scope, p.soft] {
                    for foreground in [p.text, p.muted, p.accent, p.warning, p.error] {
                        assert!(
                            contrast(foreground, surface) >= 4.5,
                            "{base_color:?} dark={dark}: {foreground:?} on {surface:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn base_color_changes_surfaces_and_scope_but_not_safety_hues() {
        let a = Palette::from_config(&AppearanceConfig::default());
        let b = Palette::from_config(&AppearanceConfig {
            base_color: [140, 110, 220],
            dark: true,
        });
        assert_ne!(a.accent, b.accent);
        assert_ne!(a.scope, b.scope);
        assert_ne!(a.background, b.background);
        assert_ne!(a.wire, b.wire);
        for palette in [a, b] {
            assert!(palette.warning.r() > palette.warning.b());
            assert!(palette.error.r() > palette.error.g());
        }
    }
}
