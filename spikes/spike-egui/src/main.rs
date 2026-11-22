//! Spike (c): eframe/egui, transparent + mouse-passthrough window.
//! Repaints once per second via request_repaint_after; otherwise idle.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;

struct App;

fn clock_string() -> (String, f32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    (
        format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60),
        (secs % 180) as f32 / 180.0,
    )
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let (text, progress) = clock_string();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_unmultiplied(13, 13, 20, 178))
                    .corner_radius(10.0)
                    .inner_margin(12.0),
            )
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(34.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.label(
                    egui::RichText::new("Now Playing: Song - Artist")
                        .size(16.0)
                        .color(egui::Color32::WHITE),
                );
                ui.add(egui::ProgressBar::new(progress).desired_height(8.0));
            });
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

fn main() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([320.0, 130.0])
        .with_position([20.0, 20.0])
        .with_decorations(false)
        .with_transparent(!std::env::args().any(|a| a == "--opaque"))
        .with_always_on_top()
        .with_mouse_passthrough(true)
        .with_taskbar(false);
    eframe::run_native(
        "spike-egui",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(App))),
    )
}
