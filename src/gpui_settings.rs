//! GPUI settings entry point; the egui executable remains the default during migration.
use crate::{
    config::{self, Config, LanguageChoice, ThemeChoice},
    delete::DeleteMode,
    gpui_cache::GlobalCacheView,
    gpui_projects::{CleanProjects, ProjectDiscovery, RefreshProjects, RootsChanged},
    i18n::Lang,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Selectable, Theme, ThemeMode, WindowExt,
    button::Button,
    radio::{Radio, RadioGroup},
};
use gpui_kit::*;
use std::{borrow::Cow, path::PathBuf};

actions!(settings, [OpenSettings, Quit]);

struct BrandAssets;
impl AssetSource for BrandAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "brand.png" {
            #[cfg(target_os = "macos")]
            let bytes = include_bytes!("../assets/icon-macos.png").as_slice();
            #[cfg(not(target_os = "macos"))]
            let bytes = include_bytes!("../assets/icon.png").as_slice();
            Ok(Some(Cow::Borrowed(bytes)))
        } else {
            gpui_kit::assets::Assets.load(path)
        }
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

fn init(cx: &mut App) {
    gpui_kit::init(cx);
    cx.bind_keys([
        KeyBinding::new("secondary-shift-backspace", CleanProjects, Some("DevSweep")),
        KeyBinding::new("secondary-r", RefreshProjects, Some("DevSweep")),
        KeyBinding::new("secondary-,", OpenSettings, Some("DevSweep")),
        KeyBinding::new("secondary-q", Quit, None),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    Theme::update(cx, |theme| {
        // Native font families include the OS CJK fallback; paths use the same readable UI font.
        theme.font_family = if cfg!(target_os = "windows") {
            "Segoe UI"
        } else {
            ".SystemUIFont"
        }
        .into();
    });
}

/// Start the opt-in GPUI frontend with the existing configuration file.
pub fn run() {
    let config = config::load();
    gpui_kit::application()
        .with_assets(BrandAssets)
        .run(move |cx| {
            init(cx);
            gpui_kit::open_window(window_options(&config, cx), cx, |window, cx| {
                cx.new(|cx| SettingsApp::new(config, config::config_path(), window, cx))
            })
            .expect("open DevSweep window");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}

fn window_options(config: &Config, cx: &App) -> WindowOptions {
    // Window geometry is a physical platform boundary, stored in logical pixels by both frontends.
    let size = config.window.map_or(size(px(1100.), px(720.)), |w| {
        size(
            px(if w.width.is_finite() {
                w.width.max(880.)
            } else {
                1100.
            }),
            px(if w.height.is_finite() {
                w.height.max(560.)
            } else {
                720.
            }),
        )
    });
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
        window_min_size: Some(gpui_kit::size(px(880.), px(560.))),
        titlebar: Some(TitlebarOptions {
            title: Some("DevSweep".into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

struct SettingsApp {
    config: Config,
    path: Option<PathBuf>,
    focus: FocusHandle,
    projects: Entity<ProjectDiscovery>,
    global_caches: bool,
    caches: Option<Entity<GlobalCacheView>>,
    cache_projects: Vec<PathBuf>,
    cache_invalidated: bool,
    cache_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}
impl SettingsApp {
    fn new(
        config: Config,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let bounds = cx.observe_window_bounds(window, |this, window, _| {
            let size = window.bounds().size;
            this.config.window = Some(config::Window {
                width: size.width.into(),
                height: size.height.into(),
            });
            config::save_to(this.path.clone(), &this.config);
        });
        let appearance =
            cx.observe_window_appearance(window, |this, window, cx| this.apply_theme(window, cx));
        let projects =
            cx.new(|cx| ProjectDiscovery::new(config.roots.clone(), config.language, window, cx));
        projects.update(cx, |projects, _| {
            projects.set_delete_mode(config.delete_mode)
        });
        let roots = cx.subscribe(&projects, |this, _, event: &RootsChanged, cx| {
            this.config.roots = event.0.clone();
            this.invalidate_caches(cx);
            config::save_to(this.path.clone(), &this.config);
            cx.notify();
        });
        let workspace = cx.observe_in(&projects, window, |this, _, window, cx| {
            let paths = this.projects.read(cx).project_paths();
            if paths != this.cache_projects {
                this.invalidate_caches(cx);
                this.cache_projects = paths;
            }
            if this.global_caches {
                this.ensure_caches(window, cx);
            }
            cx.notify();
        });
        let this = Self {
            config,
            path,
            focus,
            projects,
            global_caches: false,
            caches: None,
            cache_projects: vec![],
            cache_invalidated: false,
            cache_subscription: None,
            _subscriptions: vec![bounds, appearance, roots, workspace],
        };
        this.apply_theme(window, cx);
        this
    }
    fn ensure_caches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.caches.is_none() {
            self.cache_projects = self.projects.read(cx).project_paths();
            self.caches = Some(cx.new(|cx| {
                GlobalCacheView::new(&self.config, self.cache_projects.clone(), window, cx)
            }));
            self.cache_invalidated = false;
            // Retain the result until an explicit navigation/refresh; do not replace a
            // finishing worker's view merely because its protection context changed.
            self.cache_subscription =
                Some(cx.observe(self.caches.as_ref().unwrap(), |_, _, cx| cx.notify()));
        }
    }
    fn cache_locked(&self, cx: &App) -> bool {
        self.caches
            .as_ref()
            .is_some_and(|cache| cache.read(cx).clean_locked())
    }
    fn invalidate_caches(&mut self, cx: &mut Context<Self>) {
        if self.cache_locked(cx) || (self.cache_invalidated && self.caches.is_some()) {
            self.cache_invalidated = true;
            if let Some(cache) = &self.caches {
                cache.update(cx, |cache, cx| cache.invalidate_clean(cx));
            }
        } else {
            self.caches = None;
            self.cache_subscription = None;
        }
    }
    fn apply_theme(&self, window: &mut Window, cx: &mut App) {
        let dark = match self.config.theme {
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
    }
    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let owner = cx.entity();
        let panel = cx.new(|_| SettingsPanel {
            owner: owner.clone(),
        });
        window.open_dialog(cx, move |dialog, _, cx| {
            let strings = Lang::from_choice(owner.read(cx).config.language).strings();
            dialog.title(strings.settings).child(panel.clone())
        });
    }
}
impl Render for SettingsApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let strings = Lang::from_choice(self.config.language).strings();
        div()
            .id("workspace")
            .test_support()
            .track_focus(&self.focus)
            .key_context("DevSweep")
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(|this, _: &CleanProjects, window, cx| {
                if this.global_caches {
                    if let Some(cache) = &this.caches {
                        cache.update(cx, |cache, cx| cache.open_clean(window, cx));
                    }
                } else if !this.cache_locked(cx) {
                    this.projects
                        .update(cx, |projects, cx| projects.request_clean(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &RefreshProjects, _, cx| {
                if !this.global_caches && !this.cache_locked(cx) {
                    this.projects
                        .update(cx, |projects, cx| projects.refresh(cx));
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .p_4()
            .gap_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(img("brand.png").size_10())
                    .child("DevSweep")
                    .child(
                        Button::new("settings")
                            .icon(IconName::Settings)
                            .label(format!("{}…", strings.settings))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_settings(&OpenSettings, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .child(
                        div()
                            .w_48()
                            .min_w_48()
                            .max_w_64()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                Button::new("nav-projects")
                                    .label(strings.projects)
                                    .selected(!self.global_caches)
                                    .disabled(self.cache_locked(cx))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.global_caches = false;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("nav-global-caches")
                                    .label(strings.global_caches)
                                    .selected(self.global_caches)
                                    .disabled(self.projects.read(cx).clean_locked())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.cache_invalidated && !this.cache_locked(cx) {
                                            this.caches = None;
                                        }
                                        this.ensure_caches(window, cx);
                                        this.global_caches = true;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(if self.global_caches {
                                self.caches
                                    .as_ref()
                                    .expect("cache navigation initializes view")
                                    .clone()
                                    .into_any_element()
                            } else {
                                self.projects.clone().into_any_element()
                            }),
                    ),
            )
    }
}

struct SettingsPanel {
    owner: Entity<SettingsApp>,
}
impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = self.owner.read(cx).config.clone();
        let strings = Lang::from_choice(config.language).strings();
        let language_owner = self.owner.clone();
        let theme_owner = self.owner.clone();
        let delete_owner = self.owner.clone();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(strings.language)
            .child(
                RadioGroup::new("language")
                    .child(Radio::new("language-system").label(strings.system))
                    .child(Radio::new("language-zh").label(strings.lang_zh))
                    .child(Radio::new("language-en").label(strings.lang_en))
                    .selected_index(Some(match config.language {
                        LanguageChoice::System => 0,
                        LanguageChoice::Zh => 1,
                        LanguageChoice::En => 2,
                    }))
                    .on_change(cx.listener(move |_, ix, window, cx| {
                        language_owner.update(cx, |owner, cx| {
                            owner.config.language = [
                                LanguageChoice::System,
                                LanguageChoice::Zh,
                                LanguageChoice::En,
                            ][*ix];
                            owner.projects.update(cx, |projects, cx| {
                                projects.set_language(owner.config.language, cx)
                            });
                            if let Some(caches) = &owner.caches {
                                caches.update(cx, |caches, cx| {
                                    caches.set_language(owner.config.language, cx)
                                });
                            }
                            config::save_to(owner.path.clone(), &owner.config);
                            cx.notify();
                        });
                        window.refresh();
                        cx.notify();
                    })),
            )
            .child(strings.theme)
            .child(
                RadioGroup::new("theme")
                    .child(Radio::new("theme-system").label(strings.system))
                    .child(Radio::new("theme-light").label(strings.light))
                    .child(Radio::new("theme-dark").label(strings.dark))
                    .selected_index(Some(match config.theme {
                        ThemeChoice::System => 0,
                        ThemeChoice::Light => 1,
                        ThemeChoice::Dark => 2,
                    }))
                    .on_change(cx.listener(move |_, ix, window, cx| {
                        theme_owner.update(cx, |owner, cx| {
                            owner.config.theme =
                                [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark][*ix];
                            owner.apply_theme(window, cx);
                            config::save_to(owner.path.clone(), &owner.config);
                            cx.notify();
                        });
                        cx.notify();
                    })),
            )
            .child(strings.delete_mode)
            .child(
                RadioGroup::vertical("delete-mode")
                    .child(Radio::new("delete-trash").label(strings.delete_trash))
                    .child(Radio::new("delete-direct").label(strings.delete_direct))
                    .selected_index(Some(if config.delete_mode == DeleteMode::Trash {
                        0
                    } else {
                        1
                    }))
                    .on_change(cx.listener(move |_, ix, _, cx| {
                        delete_owner.update(cx, |owner, cx| {
                            owner.config.delete_mode = if *ix == 0 {
                                DeleteMode::Trash
                            } else {
                                DeleteMode::Direct
                            };
                            owner.projects.update(cx, |projects, _| {
                                projects.set_delete_mode(owner.config.delete_mode)
                            });
                            if let Some(cache) = &owner.caches {
                                cache.update(cx, |cache, _| {
                                    cache.set_delete_mode(owner.config.delete_mode)
                                });
                            }
                            config::save_to(owner.path.clone(), &owner.config);
                            cx.notify();
                        });
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(strings.delete_note),
            )
    }
}

#[cfg(all(test, feature = "gpui-tests"))]
mod tests {
    use crate::config::{self, LanguageChoice, ThemeChoice};
    use crate::delete::DeleteMode;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, SharedString, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn clean_shortcut_reviews_saved_mode_and_never_executes(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let project = root.join("web");
        std::fs::create_dir_all(project.join("node_modules")).unwrap();
        std::fs::write(project.join("package.json"), "{}").unwrap();
        std::fs::write(project.join("node_modules/payload"), [0; 512]).unwrap();
        let path = Some(root.join("config.toml"));
        config::save_to(
            path.clone(),
            &config::Config {
                roots: vec![root.clone()],
                language: LanguageChoice::En,
                delete_mode: DeleteMode::Direct,
                ..Default::default()
            },
        );
        let config = config::load_from(path.clone());
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(super::window_options(&config, cx), cx, |window, cx| {
                cx.new(|cx| super::SettingsApp::new(config, path.clone(), window, cx))
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("project-list")
                .within(SharedString::from(project.to_string_lossy().into_owned()))
                .click("project-check", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.press("secondary-shift-backspace", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("clean-mode-direct").checked(), Some(true));
            assert!(
                window
                    .find("clean-consequence")
                    .label()
                    .unwrap()
                    .contains("cannot be recovered")
            );
            window.press("secondary-shift-backspace", cx);
            window.press("enter", cx);
            assert!(project.join("node_modules/payload").exists());
            window.click("cancel-clean", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("execute-clean").is_none());
            window.press("secondary-,", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.within("delete-mode").click(0usize, cx);
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.press("secondary-shift-backspace", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("clean-mode-trash").checked(), Some(true));
            window.press("escape", cx);
        })
        .unwrap();
        assert!(project.join("node_modules/payload").exists());
        assert_eq!(config::load_from(path).delete_mode, DeleteMode::Trash);
    }
    #[gpui_kit::test]
    fn main_navigation_retains_cache_selection_and_updates_settings(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("payload"), [0; 4096]).unwrap();
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut app = super::SettingsApp::new(
                        config::Config {
                            language: LanguageChoice::En,
                            ..Default::default()
                        },
                        Some(temp.path().join("config.toml")),
                        window,
                        cx,
                    );
                    // Real catalogue entry and filesystem measurement, without depending on host caches.
                    app.caches = Some(cx.new(|cx| {
                        crate::gpui_cache::GlobalCacheView::from_entries(
                            vec![crate::cache::CacheEntry {
                                rule: crate::cache::builtin_caches()
                                    .iter()
                                    .find(|rule| rule.id == "gradle.caches")
                                    .unwrap(),
                                path: Some(temp.path().to_path_buf()),
                            }],
                            None,
                            LanguageChoice::En,
                            window,
                            cx,
                        )
                    }));
                    app
                })
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("nav-global-caches", cx);
            assert_eq!(window.find("gradle.caches").checked(), Some(true));
            window.click("gradle.caches", cx);
            assert_eq!(window.find("gradle.caches").checked(), Some(false));
            window.click("nav-projects", cx);
            assert!(window.find("discovery-empty").visible());
            window.click("nav-global-caches", cx);
            assert_eq!(window.find("gradle.caches").checked(), Some(false));
            window.click("settings", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.within("language").click(1usize, cx);
            window.within("theme").click(2usize, cx);
            window.within("delete-mode").click(1usize, cx);
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("cache-group-dependency").label(), Some("依赖"));
            assert_eq!(window.find("gradle.caches").checked(), Some(false));
            assert!(gpui_kit::component::Theme::global(cx).mode.is_dark());
            window.click("nav-projects", cx);
            window.click("nav-global-caches", cx);
            assert_eq!(window.find("gradle.caches").checked(), Some(false));
            window.click("gradle.caches", cx);
            window.press("secondary-shift-backspace", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("cache-mode-direct").checked(), Some(true));
            assert!(
                window
                    .find("cache-consequence")
                    .label()
                    .unwrap()
                    .contains("不可恢复")
            );
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("nav-projects", cx);
            assert!(window.find("discovery-empty").visible());
        })
        .unwrap();
        assert!(temp.path().join("payload").exists());
    }

    #[gpui_kit::test]
    fn covered_root_survives_parent_removal_and_restart(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(parent.join("package.json"), "{}").unwrap();
        std::fs::write(child.join("Cargo.toml"), "").unwrap();
        let child = child.canonicalize().unwrap();
        let path = Some(temp.path().join("config.toml"));
        let config = config::Config {
            roots: vec![child.clone()],
            language: LanguageChoice::En,
            ..Default::default()
        };
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(super::window_options(&config, cx), cx, |window, cx| {
                cx.new(|cx| super::SettingsApp::new(config, path.clone(), window, cx))
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("discovery-status").label(), Some("Projects: 1"));
            window.click("scan-root-path", cx);
            window.input(parent.to_str().unwrap(), cx);
            window.click("add-scan-root", cx);
        })
        .unwrap();
        cx.run_until_parked();
        let parent = parent.canonicalize().unwrap();
        assert_eq!(
            config::load_from(path.clone()).roots,
            vec![child.clone(), parent.clone()]
        );
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("discovery-status").label(), Some("Projects: 2"));
            assert!(
                window
                    .find(SharedString::from(format!(
                        "covered-root:{}",
                        child.display()
                    )))
                    .visible()
            );
            window.within("scan-roots").click(
                SharedString::from(parent.to_string_lossy().into_owned()),
                cx,
            );
        })
        .unwrap();
        cx.run_until_parked();
        let restored = config::load_from(path.clone());
        assert_eq!(restored.roots, vec![child.clone()]);
        let (reopened, _) = cx.update(|cx| {
            gpui_kit::open_window(super::window_options(&restored, cx), cx, |window, cx| {
                cx.new(|cx| super::SettingsApp::new(restored, path, window, cx))
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(reopened, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("discovery-status").label(), Some("Projects: 1"));
            assert!(
                window
                    .within("project-list")
                    .find(SharedString::from(child.to_string_lossy().into_owned()))
                    .visible()
            );
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn discovery_failure_can_be_copied_and_refresh_recovers_without_blocking_settings(
        cx: &mut TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        let path = Some(temp.path().join("config.toml"));
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let config = config::Config {
            roots: vec![missing.clone()],
            language: LanguageChoice::En,
            ..Default::default()
        };
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(super::window_options(&config, cx), cx, |window, cx| {
                cx.new(|cx| super::SettingsApp::new(config, path.clone(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("settings", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("dialog").visible());
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("discovery-errors").visible());
            assert!(window.try_find("discovery-empty").is_none());
            window.click("copy-discovery-errors", cx);
            assert!(
                cx.read_from_clipboard()
                    .unwrap()
                    .text()
                    .unwrap()
                    .contains("missing")
            );
        })
        .unwrap();
        std::fs::create_dir(&missing).unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.press("secondary-r", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("discovery-errors").is_none());
            assert!(window.find("discovery-empty").visible());
        })
        .unwrap();
        std::fs::write(missing.join("package.json"), "{}").unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.click("refresh-projects", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("discovery-status").label(), Some("Projects: 1"));
            assert!(
                window
                    .within("project-list")
                    .find(SharedString::from(
                        missing
                            .canonicalize()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned()
                    ))
                    .visible()
            );
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn project_roots_discover_refresh_and_persist_through_real_controls(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("中文项目");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname='demo'\n").unwrap();
        let path = Some(temp.path().join("config.toml"));
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    super::SettingsApp::new(config::Config::default(), path.clone(), window, cx)
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("discovery-empty").visible());
            window.click("scan-root-path", cx);
            window.input(root.to_str().unwrap(), cx);
            window.press("tab", cx);
            assert_eq!(window.find("add-scan-root").focused(), Some(true));
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(config::load_from(path.clone()).roots, vec![root.clone()]);
        cx.update_window(handle, |_, window, cx| {
            window.click("add-scan-root", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(config::load_from(path.clone()).roots, vec![root.clone()]);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("duplicate-root").visible());
        })
        .unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .within("project-list")
                    .find(SharedString::from(
                        root.canonicalize().unwrap().to_string_lossy().into_owned()
                    ))
                    .visible()
            );
            window.click("nav-global-caches", cx);
            assert!(window.find("cache-progress").visible());
            window.click("nav-projects", cx);
            window.press("secondary-r", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("scan-roots")
                .click(SharedString::from(root.to_string_lossy().into_owned()), cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(config::load_from(path).roots.is_empty());
    }

    #[gpui_kit::test]
    fn settings_changes_survive_restart_and_escape_restores_focus(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let path = Some(temp.path().join("config.toml"));
        std::fs::write(path.as_ref().unwrap(), "roots = ['/work/中文项目']\n").unwrap();
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    super::SettingsApp::new(
                        config::load_from(path.clone()),
                        path.clone(),
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("secondary-,", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.within("language").click(2usize, cx);
            window.within("theme").click(2usize, cx);
            window.within("delete-mode").click(1usize, cx);
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("dialog").is_none());
            assert_eq!(window.find("workspace").focused(), Some(true));
        })
        .unwrap();
        let restored = config::load_from(path.clone());
        assert_eq!(restored.language, LanguageChoice::En);
        assert_eq!(restored.theme, ThemeChoice::Dark);
        assert_eq!(restored.delete_mode, DeleteMode::Direct);
        assert_eq!(
            restored.roots,
            vec![std::path::PathBuf::from("/work/中文项目")]
        );
        let (reopened, _) = cx.update(|cx| {
            gpui_kit::open_window(super::window_options(&restored, cx), cx, |window, cx| {
                cx.new(|cx| super::SettingsApp::new(restored, path.clone(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(reopened, |_, window, cx| {
            window.render_frame(cx);
            window.click("settings", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(reopened, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.within("language").find(2usize).checked(), Some(true));
            assert_eq!(window.within("theme").find(2usize).checked(), Some(true));
            assert_eq!(
                window.within("delete-mode").find(1usize).checked(),
                Some(true)
            );
            window.within("delete-mode").click(0usize, cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            config::load_from(path.clone()).delete_mode,
            DeleteMode::Trash
        );
        cx.update_window(reopened, |_, window, cx| {
            // Kit radios are individual Tab stops; reach the choice through real keyboard navigation.
            for _ in 0..12 {
                if window.within("delete-mode").find(1usize).focused() == Some(true) {
                    break;
                }
                window.press("tab", cx);
            }
            assert_eq!(
                window.within("delete-mode").find(1usize).focused(),
                Some(true)
            );
            window.press("space", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(config::load_from(path).delete_mode, DeleteMode::Direct);
    }
    #[gpui_kit::test]
    fn resized_window_and_saved_settings_are_restored_in_a_new_window(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let path = Some(temp.path().join("config.toml"));
        cx.update(|cx| {
            super::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    super::SettingsApp::new(config::Config::default(), path.clone(), window, cx)
                })
            })
            .unwrap()
        });
        cx.simulate_window_resize(
            handle,
            gpui_kit::size(gpui_kit::px(1150.), gpui_kit::px(760.)),
        );
        cx.run_until_parked();
        let restored = config::load_from(path.clone());
        assert_eq!(
            restored.window,
            Some(config::Window {
                width: 1150.,
                height: 760.
            })
        );
        let (reopened, _) = cx.update(|cx| {
            gpui_kit::open_window(super::window_options(&restored, cx), cx, |window, cx| {
                cx.new(|cx| super::SettingsApp::new(restored.clone(), path.clone(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(reopened, |_, window, cx| {
            assert_eq!(
                window.bounds().size,
                gpui_kit::size(gpui_kit::px(1150.), gpui_kit::px(760.))
            );
            window.render_frame(cx);
            window.press("tab", cx);
            assert_eq!(window.find("settings").focused(), Some(true));
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(reopened, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("dialog").is_some());
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(reopened, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("settings").focused(), Some(true));
        })
        .unwrap();
    }
}
