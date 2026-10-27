#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod i18n;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([880.0, 560.0])
            .with_title("DevSweep"),
        ..Default::default()
    };
    eframe::run_native(
        "DevSweep",
        options,
        Box::new(|cc| Ok(Box::new(app::DevSweepApp::new(cc)))),
    )
}
