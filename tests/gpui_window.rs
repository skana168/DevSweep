#[cfg(target_os = "macos")]
#[path = "../src/gpui_app.rs"]
mod gpui_app;
#[cfg(target_os = "macos")]
#[path = "../src/gpui_theme.rs"]
mod gpui_theme;
#[cfg(target_os = "macos")]
use gpui_kit::{
    AppContext, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, px, size,
    test::{TestEventExt, TestWindowExt},
};
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
fn main() {
    let _constructor = gpui_app::SweepWindow::new;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(gpui_kit::assets::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        gpui_theme::install(cx);
    });
    let (handle, _) = cx
        .update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1437.), px(899.)),
                    ))),
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        gpui_app::SweepWindow::new_with_mode(Default::default(), true, window, cx)
                    })
                },
            )
        })
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("clean").visible());
        assert!(window.find("group-open-rust").visible());
        window.click("nav-1", cx);
        window.render_frame(cx);
        assert!(window.find("dialog").visible());
        window.press("escape", cx);
        window.click("clean", cx);
        assert!(
            window.find("dialog").visible(),
            "demo Clean must explain that no deletion occurs"
        );
        window.press("escape", cx);
        let before = window.find("group-open-rust").bounds().left();
        window.click("nav-bottom-2", cx);
        assert!(window.find("group-open-rust").bounds().left() < before);
        window.click("nav-bottom-2", cx);
        window.click("nav-bottom-1", cx);
        assert!(window.find("add-root").visible());
        window.click("close-settings", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    let output = std::path::Path::new(".scratch/gpui-ui/gpui-render.png");
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    cx.capture_screenshot(handle)
        .expect("Metal offscreen renderer")
        .save(output)
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click("category-6", cx);
        assert!(window.find("tool-open-gradle-build").visible());
        assert!(window.find("tool-open-vscode").visible());
        assert_eq!(window.find("tool-check-npm-download").checked(), Some(true));
        assert_eq!(window.find("select-category").indeterminate(), Some(true));
        assert_eq!(
            window.find("category-selection-summary").label(),
            Some("7.69 GB / 15.2 GB")
        );
        assert_eq!(
            window.find("window-selection-summary").label(),
            Some("已选 17 项 · 58.5 GB")
        );
        window.click("tool-check-npm-download", cx);
        assert_eq!(
            window.find("tool-check-npm-download").checked(),
            Some(false)
        );
        assert_ne!(
            window.find("category-selection-summary").label(),
            Some("7.69 GB / 15.2 GB")
        );
        window.click("tool-check-npm-download", cx);
        window.click("select-category", cx);
        assert_eq!(window.find("select-category").checked(), Some(true));
        assert_eq!(
            window.find("category-selection-summary").label(),
            Some("15.2 GB / 15.2 GB")
        );
        window.click("select-category", cx);
        assert_eq!(window.find("select-category").checked(), Some(false));
        assert_eq!(
            window.find("category-selection-summary").label(),
            Some("0 B / 15.2 GB")
        );
        for id in [
            "gradle-build",
            "npm-download",
            "npx-tools",
            "nuget",
            "cargo-rustup",
            "vscode",
        ] {
            window.click(format!("tool-check-{id}"), cx);
        }
        window.click("tool-open-codex-archived", cx);
        assert!(window.find("dialog").visible());
        window.press("escape", cx);
        window.click("tool-disclose-gradle-build", cx);
        assert!(window.find("tool-details-gradle-build").visible());
        window.click("tool-disclose-gradle-build", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.blur(cx);
        window.pointer_move(gpui_kit::point(px(0.), px(0.)), None, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let tools_output = std::path::Path::new(".scratch/developer-tools-ui/developer-tools.png");
    std::fs::create_dir_all(tools_output.parent().unwrap()).unwrap();
    cx.capture_screenshot(handle)
        .unwrap()
        .save(tools_output)
        .unwrap();
    for (width, height) in [(1080., 680.), (1916., 1198.)] {
        let (handle, _) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(width), px(height)),
                        ))),
                        show: false,
                        focus: false,
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        cx.new(|cx| {
                            gpui_app::SweepWindow::new_with_mode(
                                Default::default(),
                                true,
                                window,
                                cx,
                            )
                        })
                    },
                )
            })
            .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let clean = window.find("clean");
            assert!(clean.visible());
            assert!(clean.bounds().right() <= px(width));
            assert!(clean.bounds().bottom() <= px(height));
            assert!(window.find("group-open-rust").visible());
            window.click("category-6", cx);
            assert!(window.find("tool-open-gradle-build").visible());
            let footer = window.find("clean").bounds();
            window.scroll(
                "cache-results",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-700.))),
                cx,
            );
            assert!(window.find("tool-open-vscode").visible());
            assert_eq!(window.find("clean").bounds(), footer);
        })
        .unwrap();
    }
    println!(
        "GPUI window rendering and unavailable-action test passed; {}",
        output.display()
    );
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("GPUI screenshot verification targets macOS");
}
