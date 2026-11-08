#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
extern crate gpui_kit as gpui;
mod gpui_app;
mod gpui_theme;
use gpui_kit::*;
actions!(devsweep_window, [Quit]);
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            gpui_theme::install(cx);
            let config = devsweep::config::load();
            let saved_window = if std::env::args().any(|a| a == "--demo" || a == "--demo-tools") {
                None
            } else {
                config.window
            };
            let dimensions = saved_window.map_or(size(px(1437.), px(899.)), |w| {
                size(px(w.width.max(1080.)), px(w.height.max(680.)))
            });
            let (handle, _) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None, dimensions, cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("DevSweep".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(18.), px(19.))),
                    }),
                    window_min_size: Some(size(px(1080.), px(680.))),
                    app_id: Some("DevSweep".into()),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| gpui_app::SweepWindow::new(config, window, cx)),
            )
            .expect("open DevSweep window");
            handle
                .update(cx, |_, window, _| window.activate_window())
                .expect("activate window");
            cx.activate(true);
        });
}
