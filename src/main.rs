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
        .with_title("DevSweep")
        .with_icon(app_icon());
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

fn app_icon() -> eframe::egui::IconData {
    #[cfg(target_os = "macos")]
    {
        if std::env::current_exe().is_ok_and(|exe| has_bundle_icon(&exe)) {
            // An empty icon preserves the bundle's ICNS and macOS icon treatment.
            return eframe::egui::IconData::default();
        }
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-macos.png"))
            .expect("decode bundled macOS app icon")
    }
    #[cfg(not(target_os = "macos"))]
    {
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
            .expect("decode bundled app icon")
    }
}

#[cfg(target_os = "macos")]
fn has_bundle_icon(exe: &std::path::Path) -> bool {
    let Some(contents) = exe
        .parent()
        .filter(|path| path.ends_with("Contents/MacOS"))
        .and_then(std::path::Path::parent)
    else {
        return false;
    };
    contents
        .parent()
        .is_some_and(|path| path.extension().is_some_and(|ext| ext == "app"))
        && contents.join("Info.plist").is_file()
        && contents.join("Resources/DevSweep.icns").is_file()
}

#[cfg(test)]
mod tests {
    #[test]
    fn direct_launch_uses_the_platform_icon() {
        let icon = super::app_icon();
        let size = if cfg!(target_os = "macos") { 1024 } else { 256 };
        assert_eq!((icon.width, icon.height), (size, size));
        assert_eq!(icon.rgba.len(), (size * size * 4) as usize);
        assert_eq!(&icon.rgba[..4], &[0, 0, 0, 0]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_a_complete_app_bundle_keeps_its_system_icon() {
        let temp = tempfile::tempdir().unwrap();
        let contents = temp.path().join("DevSweep.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::create_dir_all(contents.join("Resources")).unwrap();
        let exe = contents.join("MacOS/DevSweep");
        assert!(!super::has_bundle_icon(&exe));
        std::fs::write(contents.join("Info.plist"), b"fixture").unwrap();
        assert!(!super::has_bundle_icon(&exe));
        std::fs::write(contents.join("Resources/DevSweep.icns"), b"fixture").unwrap();
        assert!(super::has_bundle_icon(&exe));
        assert!(!super::has_bundle_icon(
            &temp.path().join("target/debug/devsweep")
        ));
        let non_app = temp.path().join("NotAnApp/Contents");
        std::fs::create_dir_all(non_app.parent().unwrap()).unwrap();
        std::fs::rename(&contents, &non_app).unwrap();
        assert!(!super::has_bundle_icon(&non_app.join("MacOS/DevSweep")));
    }
}
