//! Opt-in ticket 06 harness; the shared workspace belongs to ticket 02.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use devsweep::{
    config::{self, ThemeChoice},
    gpui_cache::GlobalCacheView,
};
use gpui_kit::{
    component::{Theme, ThemeMode},
    *,
};

fn main() {
    let config = config::load();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            // Physical window geometry uses the same logical-pixel config as both frontends.
            let dimensions = config
                .window
                .filter(|w| w.width.is_finite() && w.height.is_finite());
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    dimensions.map_or(size(px(1100.), px(720.)), |w| {
                        size(px(w.width.max(640.)), px(w.height.max(480.)))
                    }),
                    cx,
                ))),
                window_min_size: Some(size(px(640.), px(480.))),
                titlebar: Some(TitlebarOptions {
                    title: Some(
                        format!(
                            "DevSweep — {}",
                            devsweep::i18n::Lang::from_choice(config.language)
                                .strings()
                                .global_caches
                        )
                        .into(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                let dark = match config.theme {
                    ThemeChoice::Dark => true,
                    ThemeChoice::Light => false,
                    ThemeChoice::System => matches!(
                        window.appearance(),
                        WindowAppearance::Dark | WindowAppearance::VibrantDark
                    ),
                };
                Theme::change(
                    if dark {
                        ThemeMode::Dark
                    } else {
                        ThemeMode::Light
                    },
                    Some(window),
                    cx,
                );
                Theme::update(cx, |theme| {
                    theme.font_family = if cfg!(windows) {
                        "Segoe UI"
                    } else {
                        ".SystemUIFont"
                    }
                    .into()
                });
                cx.new(|cx| GlobalCacheView::new(&config, vec![], window, cx))
            })
            .expect("open global cache harness");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
