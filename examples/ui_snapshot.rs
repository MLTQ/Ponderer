//! Opt-in native screenshot harness: synthetic state, no backend, no model, no live configuration.
#[allow(dead_code)]
#[path = "../src/api.rs"]
mod api;
#[allow(dead_code)]
#[path = "../src/ui/mod.rs"]
mod ui;
use eframe::egui;
pub use ponderer_backend::{character_card, config};
use std::path::PathBuf;

struct Snapshot {
    app: ui::app::AgentApp,
    destination: PathBuf,
    frames: usize,
}

impl eframe::App for Snapshot {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.render_snapshot(ctx);
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = image {
            let bytes: Vec<_> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::save_buffer(
                &self.destination,
                &bytes,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .expect("save screenshot");
            println!("{}", self.destination.display());
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.frames += 1;
        if self.frames == 6 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        if self.frames > 120 {
            panic!("native screenshot timed out");
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    let destination = PathBuf::from(args.get(1).expect("destination PNG argument"));
    assert!(!destination.exists(), "refusing to overwrite screenshot");
    let workspace = args.get(2).map(String::as_str).unwrap_or("conversation");
    let mut config = config::AgentConfig::default();
    if let Some(hex) = args.get(3) {
        assert_eq!(hex.len(), 6, "base color must be six hex digits");
        config.appearance.base_color = std::array::from_fn(|index| {
            u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap()
        });
    }
    config.appearance.dark = args.get(4).map(String::as_str) != Some("light");
    let width = args
        .get(5)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1240.0);
    let height = args
        .get(6)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(900.0);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_title("Ponderer / isolated native snapshot"),
        ..Default::default()
    };
    let app = ui::app::AgentApp::isolated_snapshot(config, workspace);
    eframe::run_native(
        "Ponderer UI snapshot",
        options,
        Box::new(move |_| {
            Ok(Box::new(Snapshot {
                app,
                destination,
                frames: 0,
            }))
        }),
    )
}
