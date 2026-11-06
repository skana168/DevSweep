#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    #[cfg(feature = "gpui-tests")]
    if let Some(path) = std::env::var_os("DEVSWEEP_CAPTURE") {
        devsweep::gpui_settings::capture_preview(std::path::Path::new(&path));
        return;
    }
    devsweep::gpui_settings::run();
}
