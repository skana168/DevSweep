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
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([880.0, 560.0])
        .with_title("DevSweep");
    // The header is the title bar (see `app::DevSweepApp::header`): macOS keeps its traffic
    // lights over the content, elsewhere the decorations go and the header draws the buttons.
    viewport = if cfg!(target_os = "macos") {
        viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
    } else {
        viewport.with_decorations(false)
    };
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "DevSweep",
        options,
        Box::new(move |cc| Ok(Box::new(app::DevSweepApp::new(cc, config)))),
    )
}
