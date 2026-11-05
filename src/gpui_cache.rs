//! Independently owned global-cache discovery and selection view.
use crate::{
    cache::{self, CacheEntry, CacheGroup},
    config::{Config, LanguageChoice},
    go_cache::{self, GoCacheContext, GoCacheError, GoCacheSnapshot},
    i18n::{Lang, Strings},
    size,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;

#[derive(Clone)]
enum Measurement {
    Pending,
    Ready(u64),
    Failed(GoCacheError),
}
struct Row {
    entry: CacheEntry,
    measurement: Measurement,
    selected: bool,
    focus: FocusHandle,
}

/// A selection draft only. Ticket 07 must construct and recheck a delete plan.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheSelection {
    pub ids: Vec<&'static str>,
    pub bytes: Option<u64>,
}

/// Fixed cache catalogue, background measurements and a transient selection draft.
/// Mount once in the shell, retain its Entity, and read `selection()` for ticket 07.
pub struct GlobalCacheView {
    rows: Vec<Row>,
    language: LanguageChoice,
    scroll: ScrollHandle,
    _tasks: Vec<Task<()>>,
    go_snapshot: Option<GoCacheSnapshot>,
}
impl GlobalCacheView {
    /// `projects` supplies the same Go path protections as the project workspace.
    pub fn new(
        config: &Config,
        projects: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let entries = cache::entries()
            .into_iter()
            .filter(|entry| !(cfg!(windows) && entry.rule.id == "xcode.derived_data"))
            .collect::<Vec<_>>();
        let context = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|home| GoCacheContext {
                home: home.into(),
                scan_roots: config.roots.clone(),
                projects,
                other_caches: entries
                    .iter()
                    .filter_map(|entry| entry.path.clone())
                    .collect(),
            });
        Self::from_entries(entries, context, config.language, window, cx)
    }
    fn from_entries(
        entries: Vec<CacheEntry>,
        context: Option<GoCacheContext>,
        language: LanguageChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let rows = entries
            .into_iter()
            .map(|entry| Row {
                entry,
                measurement: Measurement::Pending,
                selected: false,
                focus: cx.focus_handle(),
            })
            .collect::<Vec<_>>();
        if let Some(row) = rows.first() {
            row.focus.focus(window, cx);
        }
        let mut this = Self {
            rows,
            language,
            scroll: ScrollHandle::new(),
            _tasks: vec![],
            go_snapshot: None,
        };
        for ix in 0..this.rows.len() {
            if go_cache::is_go_cache(this.rows[ix].entry.rule.id) {
                continue;
            }
            let path = this.rows[ix].entry.path.clone();
            this._tasks.push(cx.spawn(async move |view, cx| {
                let result = cx
                    .background_spawn(async move {
                        let path = path.ok_or(GoCacheError::Unreadable)?;
                        let meta = std::fs::symlink_metadata(&path).map_err(|err| {
                            if err.kind() == std::io::ErrorKind::NotFound {
                                GoCacheError::Missing
                            } else {
                                GoCacheError::Unreadable
                            }
                        })?;
                        if !meta.is_dir() && !meta.file_type().is_symlink() {
                            return Err(GoCacheError::NotDirectory);
                        }
                        if meta.is_dir() {
                            std::fs::read_dir(&path).map_err(|_| GoCacheError::Unreadable)?;
                        }
                        Ok(size::dir_size(&path))
                    })
                    .await;
                view.update(cx, |view, cx| {
                    view.finish(ix, result);
                    cx.notify();
                })
                .ok();
            }));
        }
        if this
            .rows
            .iter()
            .any(|row| go_cache::is_go_cache(row.entry.rule.id))
        {
            this._tasks.push(cx.spawn(async move |view, cx| {
                let snapshot = cx
                    .background_spawn(
                        async move { context.map(|context| go_cache::discover(&context)) },
                    )
                    .await;
                view.update(cx, |view, cx| {
                    for ix in 0..view.rows.len() {
                        let id = view.rows[ix].entry.rule.id;
                        if !go_cache::is_go_cache(id) {
                            continue;
                        }
                        let result = snapshot.as_ref().and_then(|snapshot| snapshot.entry(id));
                        view.rows[ix].entry.path = result.and_then(|entry| entry.path.clone());
                        view.finish(
                            ix,
                            result.map_or(Err(GoCacheError::Unreadable), |entry| entry.size),
                        );
                    }
                    view.go_snapshot = snapshot;
                    cx.notify();
                })
                .ok();
            }));
        }
        this
    }
    fn finish(&mut self, ix: usize, result: Result<u64, GoCacheError>) {
        let row = &mut self.rows[ix];
        row.measurement = match result {
            Ok(bytes) => Measurement::Ready(bytes),
            Err(error) => Measurement::Failed(error),
        };
        row.selected = result.is_ok() && !row.entry.rule.dangerous;
    }
    /// A later clean flow must retain this snapshot and use existing Go recheck APIs.
    pub fn go_snapshot(&self) -> Option<&GoCacheSnapshot> {
        self.go_snapshot.as_ref()
    }
    pub fn set_language(&mut self, language: LanguageChoice, cx: &mut Context<Self>) {
        self.language = language;
        cx.notify();
    }
    pub fn selection(&self) -> CacheSelection {
        let rows = self
            .rows
            .iter()
            .filter(|row| row.selected)
            .collect::<Vec<_>>();
        CacheSelection {
            ids: rows.iter().map(|row| row.entry.rule.id).collect(),
            bytes: rows
                .iter()
                .try_fold(0u64, |sum, row| match row.measurement {
                    Measurement::Ready(bytes) => Some(sum.saturating_add(bytes)),
                    _ => None,
                }),
        }
    }
    fn toggle(&mut self, ix: usize, selected: bool, cx: &mut Context<Self>) {
        if matches!(self.rows[ix].measurement, Measurement::Ready(_)) {
            self.rows[ix].selected = selected;
            cx.notify();
        }
    }
    fn navigate(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self
            .rows
            .iter()
            .position(|row| row.focus.contains_focused(window, cx))
        else {
            return;
        };
        let next = match event.keystroke.key.as_str() {
            "up" => ix.saturating_sub(1),
            "down" => (ix + 1).min(self.rows.len() - 1),
            "home" => 0,
            "end" => self.rows.len() - 1,
            "space" if self.rows[ix].focus.is_focused(window) => {
                self.toggle(ix, !self.rows[ix].selected, cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        self.rows[next].focus.focus(window, cx);
        self.scroll.scroll_to_item(next);
        cx.stop_propagation();
        cx.notify();
    }
    fn render_row(
        &self,
        ix: usize,
        strings: &Strings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let row = &self.rows[ix];
        let title = match row.entry.rule.id {
            "go.build" => strings.go_build_cache,
            "go.module" => strings.go_module_cache,
            _ => row.entry.rule.name,
        };
        let state = match row.measurement {
            Measurement::Pending => strings.measuring.to_string(),
            Measurement::Ready(bytes) => bytes_label(bytes),
            Measurement::Failed(error) => strings.go_cache_error(error).to_string(),
        };
        let note = match row.entry.rule.id {
            "go.module" => strings.go_module_note,
            "cargo.registry" => strings.cache_cargo_note,
            "go.build" => strings.go_build_note,
            "xcode.derived_data" => "",
            _ => strings.cache_redownload,
        };
        let note = if row.entry.rule.id == "cargo.registry" {
            format!(
                "{} · {} · {note}",
                strings.dangerous, strings.cache_redownload
            )
        } else if row.entry.rule.dangerous {
            format!("{} · {note}", strings.dangerous)
        } else {
            note.to_string()
        };
        let path = row
            .entry
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        div()
            .id(SharedString::from(format!(
                "cache-row-{}",
                row.entry.rule.id
            )))
            .track_focus(&row.focus)
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .focus_visible(|style| style.bg(cx.theme().muted))
            .when(
                ix == 0 || self.rows[ix - 1].entry.rule.group != row.entry.rule.group,
                |element| {
                    let id = match row.entry.rule.group {
                        CacheGroup::Dependency => "cache-group-dependency",
                        CacheGroup::Toolchain => "cache-group-toolchain",
                    };
                    element.child(
                        div()
                            .id(id)
                            .test_support()
                            .aria_label(strings.cache_group(row.entry.rule.group))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(strings.cache_group(row.entry.rule.group)),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Checkbox::new(row.entry.rule.id)
                            .label(title)
                            .checked(row.selected)
                            .disabled(!matches!(row.measurement, Measurement::Ready(_)))
                            .on_click(cx.listener(move |view, selected, _, cx| {
                                view.toggle(ix, *selected, cx)
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_right()
                            .id(SharedString::from(format!(
                                "cache-state-{}",
                                row.entry.rule.id
                            )))
                            .test_support()
                            .aria_label(state.clone())
                            .text_sm()
                            .child(state),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!(
                        "cache-note-{}",
                        row.entry.rule.id
                    )))
                    .test_support()
                    .aria_label(note.clone())
                    .text_sm()
                    .text_color(if row.entry.rule.dangerous {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(note),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(path.clone()),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "copy-path-{}",
                            row.entry.rule.id
                        )))
                        .ghost()
                        .icon(IconName::Copy)
                        .label(match Lang::from_choice(self.language) {
                            Lang::Zh => "复制路径",
                            Lang::En => "Copy path",
                        })
                        .disabled(path.is_empty())
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()))
                        }),
                    ),
            )
    }
}
fn bytes_label(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{bytes} B")
    } else {
        size::format_size(bytes)
    }
}
impl Render for GlobalCacheView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Lang::from_choice(self.language);
        let strings = lang.strings();
        let selected = self.selection();
        let done = self
            .rows
            .iter()
            .filter(|row| !matches!(row.measurement, Measurement::Pending))
            .count();
        let progress = match (lang, done == self.rows.len()) {
            (Lang::Zh, false) => format!("体积计算中 {done}/{}", self.rows.len()),
            (Lang::Zh, true) => format!("测量完成 {done}/{}", self.rows.len()),
            (Lang::En, false) => format!("Measuring {done}/{}", self.rows.len()),
            (Lang::En, true) => format!("Measured {done}/{}", self.rows.len()),
        };
        let summary = match lang {
            Lang::Zh => format!(
                "已选 {} 项 · {}",
                selected.ids.len(),
                selected
                    .bytes
                    .map(bytes_label)
                    .unwrap_or_else(|| "未知".into())
            ),
            Lang::En => format!(
                "{} selected · {}",
                selected.ids.len(),
                selected
                    .bytes
                    .map(bytes_label)
                    .unwrap_or_else(|| "Unknown".into())
            ),
        };
        div()
            .id("global-cache-view")
            .size_full()
            .flex()
            .flex_col()
            .min_w_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_key_down(cx.listener(Self::navigate))
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(strings.global_caches)
                    .child(
                        div()
                            .id("cache-progress")
                            .test_support()
                            .aria_label(progress.clone())
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(progress),
                    ),
            )
            .child(
                div()
                    .id("cache-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children((0..self.rows.len()).map(|ix| self.render_row(ix, strings, cx)))
                    .when(self.rows.is_empty(), |list| {
                        let message = match lang {
                            Lang::Zh => "当前平台没有可用的全局缓存条目",
                            Lang::En => "No global cache entries on this platform",
                        };
                        list.child(
                            div()
                                .id("cache-empty")
                                .test_support()
                                .aria_label(message)
                                .p_4()
                                .child(message),
                        )
                    }),
            )
            .child(
                div()
                    .id("cache-selection-summary")
                    .test_support()
                    .aria_label(summary.clone())
                    .p_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(summary),
            )
    }
}

#[cfg(all(test, feature = "gpui-tests"))]
mod tests {
    use super::GlobalCacheView;
    use crate::{cache, config::LanguageChoice};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn real_cache_checkboxes_keep_danger_opt_in_and_report_selected_bytes(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("payload"), [0; 4096]).unwrap();
        let entries = ["gradle.caches", "cargo.registry"].map(|id| cache::CacheEntry {
            rule: cache::builtin_caches().iter().find(|r| r.id == id).unwrap(),
            path: Some(temp.path().to_path_buf()),
        });
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
        });
        let ((handle, _), view) = cx.update(|cx| {
            let mut view = None;
            let handle = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let entity = cx.new(|cx| {
                    GlobalCacheView::from_entries(
                        entries.to_vec(),
                        None,
                        LanguageChoice::En,
                        window,
                        cx,
                    )
                });
                view = Some(entity.clone());
                entity
            })
            .unwrap();
            (handle, view.unwrap())
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("cargo.registry").checked(), Some(false));
            assert_eq!(
                window.find("cache-note-cargo.registry").label(),
                Some("Dangerous · Downloads again next time · Affects Rust builds on this machine")
            );
            window.click("cargo.registry", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(view.read(cx).selection().bytes, Some(8192)));
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("up", cx);
            window.press("space", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            let selected = view.read(cx).selection();
            assert_eq!(selected.ids, vec!["cargo.registry"]);
            assert_eq!(selected.bytes, Some(4096));
        });
    }
    #[gpui_kit::test]
    fn pending_missing_and_invalid_paths_never_become_zero_sized_selections(
        cx: &mut TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-a-directory");
        std::fs::write(&file, "payload").unwrap();
        let entries = ["gradle.caches", "npm.cacache"]
            .into_iter()
            .zip([temp.path().join("missing"), file])
            .map(|(id, path)| cache::CacheEntry {
                rule: cache::builtin_caches().iter().find(|r| r.id == id).unwrap(),
                path: Some(path),
            })
            .collect();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
        });
        let ((handle, _), view) = cx.update(|cx| {
            let mut view = None;
            let handle = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let entity = cx.new(|cx| {
                    GlobalCacheView::from_entries(entries, None, LanguageChoice::En, window, cx)
                });
                view = Some(entity.clone());
                entity
            })
            .unwrap();
            (handle, view.unwrap())
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("gradle.caches", cx);
            assert_eq!(window.find("gradle.caches").checked(), Some(false));
            assert_eq!(window.find("cache-progress").label(), Some("Measuring 0/2"));
            assert_eq!(
                window.find("cache-state-gradle.caches").label(),
                Some("Measuring sizes")
            );
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("cache-progress").label(), Some("Measured 2/2"));
            assert_eq!(
                window.find("cache-state-gradle.caches").label(),
                Some("Cache directory does not exist")
            );
            assert_eq!(
                window.find("cache-state-npm.cacache").label(),
                Some("Safety check rejected: cache path is not a directory")
            );
            window.click("gradle.caches", cx);
            window.press("home", cx);
            window.press("space", cx);
        })
        .unwrap();
        cx.update(|cx| assert!(view.read(cx).selection().ids.is_empty()));
    }

    #[gpui_kit::test]
    fn grouped_long_paths_remain_keyboard_accessible_at_large_text(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("中文项目与依赖缓存".repeat(5));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("payload"), [0; 2048]).unwrap();
        let entries = ["gradle.caches", "cargo.registry"].map(|id| cache::CacheEntry {
            rule: cache::builtin_caches().iter().find(|r| r.id == id).unwrap(),
            path: Some(path.clone()),
        });
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    GlobalCacheView::from_entries(
                        entries.to_vec(),
                        None,
                        LanguageChoice::Zh,
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.simulate_window_resize(
            handle,
            gpui_kit::size(gpui_kit::px(640.), gpui_kit::px(480.)),
        );
        cx.update_window(handle, |_, window, cx| {
            gpui_kit::component::Theme::update(cx, |theme| theme.font_size = gpui_kit::px(20.));
            window.render_frame(cx);
            assert_eq!(window.find("cache-group-dependency").label(), Some("依赖"));
            assert_eq!(window.find("cache-group-toolchain").label(), Some("工具链"));
            window.press("end", cx);
            window.press("space", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("cargo.registry").checked(), Some(true));
            assert!(window.find("cache-selection-summary").visible());
            assert!(window.find("copy-path-cargo.registry").visible());
            window.click("copy-path-cargo.registry", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().unwrap(),
                path.display().to_string()
            );
        })
        .unwrap();
    }
    #[gpui_kit::test]
    fn empty_catalogue_explains_why_there_are_no_choices(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    GlobalCacheView::from_entries(vec![], None, LanguageChoice::En, window, cx)
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("cache-empty").label(),
                Some("No global cache entries on this platform")
            );
            assert_eq!(
                window.find("cache-selection-summary").label(),
                Some("0 selected · 0 B")
            );
            window.press("end", cx);
            window.press("space", cx);
        })
        .unwrap();
    }
    #[cfg(unix)]
    #[gpui_kit::test]
    fn linked_fixed_cache_keeps_existing_measurement_semantics(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("linked-cache");
        let target = temp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("payload"), [0; 1024]).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        // Existing WalkDir API traverses a root link, counts that link, and does
        // not follow descendant links. Preserve that public measurement behavior.
        let expected = 1024 + std::fs::symlink_metadata(&path).unwrap().len();
        cx.update(gpui_kit::init);
        let ((handle, _), view) = cx.update(|cx| {
            let mut view = None;
            let handle = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let entity = cx.new(|cx| {
                    GlobalCacheView::from_entries(
                        vec![cache::CacheEntry {
                            rule: &cache::builtin_caches()[0],
                            path: Some(path),
                        }],
                        None,
                        LanguageChoice::En,
                        window,
                        cx,
                    )
                });
                view = Some(entity.clone());
                entity
            })
            .unwrap();
            (handle, view.unwrap())
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("gradle.caches").checked(), Some(true));
        })
        .unwrap();
        cx.update(|cx| assert_eq!(view.read(cx).selection().bytes, Some(expected)));
    }
}
