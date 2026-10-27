#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod i18n;
mod terminal;
mod theme;

fn main() -> eframe::Result {
    let config = devsweep::config::load();
    let size = config
        .window
        .map_or([1100.0, 720.0], |window| [window.width, window.height]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([880.0, 560.0])
            .with_title("DevSweep"),
        ..Default::default()
    };
    eframe::run_native(
        "DevSweep",
        options,
        Box::new(move |cc| Ok(Box::new(app::DevSweepApp::new(cc, config)))),
    )
}
