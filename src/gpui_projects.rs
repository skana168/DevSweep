//! Project discovery presentation. All filesystem traversal runs on the background executor.
use crate::{
    config,
    config::LanguageChoice,
    i18n::Lang,
    projects::ProjectRefresh,
    rules,
    scan::{self, Project, Stack},
    size,
};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Selectable, ThemeStyled,
    button::Button,
    button::ButtonVariants,
    checkbox::Checkbox,
    input::{Input, InputState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::PathBuf,
    sync::Arc,
};

actions!(projects, [RefreshProjects]);

pub(crate) struct RootsChanged(pub(crate) Vec<PathBuf>);

pub(crate) struct ProjectDiscovery {
    roots: Vec<PathBuf>,
    language: LanguageChoice,
    input: Entity<InputState>,
    projects: Arc<Vec<Project>>,
    model: ProjectRefresh,
    filter: Option<Stack>,
    visible: Vec<usize>,
    selected: HashSet<PathBuf>,
    active: Option<PathBuf>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    measuring: bool,
    measured: usize,
    measurement_total: usize,
    measurement_epoch: u64,
    measurement_task: Option<Task<()>>,
    queued_refresh: bool,
    scanning: bool,
    errors: Vec<String>,
    revision: u64,
    task: Option<Task<()>>,
    add_task: Option<Task<()>>,
    adding: bool,
    duplicate: bool,
}
impl EventEmitter<RootsChanged> for ProjectDiscovery {}
impl ProjectDiscovery {
    pub(crate) fn new(
        roots: Vec<PathBuf>,
        language: LanguageChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            roots,
            language,
            input: cx.new(|cx| InputState::new(window, cx)),
            projects: Arc::default(),
            model: ProjectRefresh::default(),
            filter: None,
            visible: Vec::new(),
            selected: HashSet::new(),
            active: None,
            focus: cx.focus_handle().tab_stop(true),
            scroll: UniformListScrollHandle::new(),
            measuring: false,
            measured: 0,
            measurement_total: 0,
            measurement_epoch: 0,
            measurement_task: None,
            queued_refresh: false,
            scanning: false,
            errors: Vec::new(),
            revision: 0,
            task: None,
            add_task: None,
            adding: false,
            duplicate: false,
        };
        this.refresh(cx);
        this
    }
    pub(crate) fn project_paths(&self) -> Vec<PathBuf> {
        self.projects
            .iter()
            .map(|project| project.path.clone())
            .collect()
    }
    pub(crate) fn set_language(&mut self, language: LanguageChoice, cx: &mut Context<Self>) {
        self.language = language;
        cx.notify();
    }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.scanning || self.measuring {
            self.queued_refresh = true;
            return;
        }
        self.discover(cx);
    }
    fn discover(&mut self, cx: &mut Context<Self>) {
        self.queued_refresh = false;
        self.revision += 1;
        let revision = self.revision;
        let roots = scan::active_roots(&self.roots);
        self.scanning = !roots.is_empty();
        self.errors.clear();
        if roots.is_empty() {
            self.task = None;
            self.projects = Arc::default();
            self.model = ProjectRefresh::default();
            self.selected.clear();
            self.rebuild_visible();
            cx.notify();
            return;
        }
        let work = cx.background_spawn(async move {
            let report = scan::scan_with_errors(&roots);
            let loaded = rules::load(config::rules_path().as_deref());
            let found = ProjectRefresh::from_discovered(report.projects().to_vec(), &loaded.rules);
            (report, found, loaded.error)
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let (report, found, rule_error) = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.revision != revision {
                    return;
                }
                let old_stacks: HashMap<_, _> = this
                    .model
                    .entries()
                    .iter()
                    .map(|entry| (&entry.project.path, entry.project.stack))
                    .collect();
                let new_stacks: HashMap<_, _> = found
                    .entries()
                    .iter()
                    .map(|entry| (&entry.project.path, entry.project.stack))
                    .collect();
                if this.measuring
                    && old_stacks
                        .iter()
                        .any(|(path, stack)| new_stacks.get(path) != Some(stack))
                {
                    this.measurement_epoch += 1;
                    this.measurement_task = None;
                    this.measuring = false;
                }
                this.selected.retain(|path| {
                    old_stacks
                        .get(path)
                        .is_some_and(|stack| new_stacks.get(path) == Some(stack))
                });
                this.model.apply_refresh(found);
                this.projects = Arc::new(
                    this.model
                        .entries()
                        .iter()
                        .map(|entry| entry.project.clone())
                        .collect(),
                );
                this.rebuild_visible();
                this.errors = report
                    .errors()
                    .iter()
                    .map(|error| format!("{}: {}", error.path().display(), error.message()))
                    .collect();
                if let Some(error) = rule_error {
                    this.errors.push(format!("rules.toml: {error}"));
                }
                this.scanning = false;
                this.measure_missing(cx);
                this.run_queued_refresh(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn run_queued_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.scanning && !self.measuring && std::mem::take(&mut self.queued_refresh) {
            self.discover(cx);
        }
    }
    fn measure_missing(&mut self, cx: &mut Context<Self>) {
        if self.measuring {
            return;
        }
        let requests = self.model.measurement_requests();
        if requests.is_empty() {
            return;
        }
        self.measured = 0;
        self.measurement_total = requests.len();
        self.measuring = true;
        let epoch = self.measurement_epoch;
        self.measurement_task = Some(cx.spawn(async move |this, cx| {
            let mut requests = requests.into_iter();
            loop {
                // A fixed upper bound keeps the filesystem workload off the UI
                // thread without creating an unbounded task per result row.
                let batch: Vec<_> = requests
                    .by_ref()
                    .take(size::CONCURRENCY)
                    .map(|request| {
                        cx.background_spawn(async move {
                            size::SizeEvent {
                                project: request.project,
                                sizes: request
                                    .paths
                                    .into_iter()
                                    .map(|path| {
                                        let bytes = size::dir_size(&path);
                                        (path, bytes)
                                    })
                                    .collect(),
                            }
                        })
                    })
                    .collect();
                if batch.is_empty() {
                    break;
                }
                for work in batch {
                    let event = work.await;
                    if this
                        .update(cx, |this, cx| {
                            if this.measurement_epoch != epoch {
                                return;
                            }
                            this.model.apply_measurement(&event);
                            this.measured += 1;
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.measurement_epoch != epoch {
                    return;
                }
                this.measuring = false;
                this.measure_missing(cx);
                this.run_queued_refresh(cx);
                cx.notify();
            });
        }));
    }
    fn rebuild_visible(&mut self) {
        self.visible = self
            .projects
            .iter()
            .enumerate()
            .filter_map(|(ix, project)| {
                (self.filter.is_none_or(|stack| stack == project.stack)).then_some(ix)
            })
            .collect();
        if !self
            .visible
            .iter()
            .any(|&ix| Some(&self.projects[ix].path) == self.active.as_ref())
        {
            self.active = self
                .visible
                .first()
                .map(|&ix| self.projects[ix].path.clone());
        }
        if let Some(ix) = self
            .visible
            .iter()
            .position(|&ix| Some(&self.projects[ix].path) == self.active.as_ref())
        {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }
    fn toggle_project(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        if !self.selected.remove(path) {
            self.selected.insert(path.clone());
        }
        cx.notify();
    }
    fn navigate(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.contains_focused(window, cx) || self.visible.is_empty() {
            return;
        }
        let ix = self
            .visible
            .iter()
            .position(|&ix| Some(&self.projects[ix].path) == self.active.as_ref())
            .unwrap_or(0);
        let next = match event.keystroke.key.as_str() {
            "up" => ix.saturating_sub(1),
            "down" => (ix + 1).min(self.visible.len() - 1),
            "home" => 0,
            "end" => self.visible.len() - 1,
            "space" if self.focus.is_focused(window) => {
                let path = self.projects[self.visible[ix]].path.clone();
                self.toggle_project(&path, cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        self.active = Some(self.projects[self.visible[next]].path.clone());
        self.focus.focus(window, cx);
        self.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        cx.stop_propagation();
        cx.notify();
    }
    fn render_project(
        &self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let project = &self.projects[ix];
        let strings = Lang::from_choice(self.language).strings();
        let path = project.path.clone();
        let row_path = path.clone();
        let copy_path = path.to_string_lossy().into_owned();
        let selected = self.selected.contains(&path);
        let active = self.active.as_ref() == Some(&path);
        let bytes = size::total_size(&self.model.entries()[ix].targets);
        let value = bytes.map(|bytes| {
            if bytes < 1024 * 1024 {
                format!("{bytes} B")
            } else {
                size::format_size(bytes)
            }
        });
        let label = match Lang::from_choice(self.language) {
            Lang::En => format!(
                "Project size: {}",
                value.unwrap_or_else(|| "Unknown · Measuring".into())
            ),
            Lang::Zh => format!(
                "项目大小：{}",
                value.unwrap_or_else(|| "未知 · 体积计算中".into())
            ),
        };
        div()
            .id(SharedString::from(path.to_string_lossy().into_owned()))
            .test_support()
            .h_24()
            .w_full()
            .min_w_0()
            .px_2()
            .py_1()
            .flex()
            .flex_col()
            .gap_1()
            .border_1()
            .border_color(cx.theme().border)
            .when(selected, |row| row.bg(cx.theme().muted))
            .hover(|row| row.bg(cx.theme().secondary))
            .when(active && self.focus.contains_focused(window, cx), |row| {
                row.focus_ring_style(window, cx)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.active = Some(row_path.clone());
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new("project-check")
                            .flex_1()
                            .min_w_0()
                            .accessibility_label(format!(
                                "{} · {}",
                                project.name,
                                strings.stack(project.stack)
                            ))
                            .child(div().truncate().child(format!(
                                "{} · {}",
                                project.name,
                                strings.stack(project.stack)
                            )))
                            .checked(selected)
                            .on_change(cx.listener(move |this, checked, window, cx| {
                                if *checked {
                                    this.selected.insert(path.clone());
                                } else {
                                    this.selected.remove(&path);
                                }
                                this.active = Some(path.clone());
                                this.focus.focus(window, cx);
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("project-size")
                            .test_support()
                            .aria_label(label.clone())
                            .flex_shrink_0()
                            .text_sm()
                            .child(label),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .id("project-path")
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(copy_path.clone()),
                    )
                    .child(
                        Button::new("copy-project-path")
                            .ghost()
                            .icon(IconName::Copy)
                            .label(match Lang::from_choice(self.language) {
                                Lang::En => "Copy path",
                                Lang::Zh => "复制路径",
                            })
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()));
                                cx.stop_propagation();
                            }),
                    ),
            )
    }
    fn add_root(&mut self, cx: &mut Context<Self>) {
        if self.adding {
            return;
        }
        let value = self.input.read(cx).value();
        if value.trim().is_empty() {
            return;
        }
        let root = PathBuf::from(value.as_str());
        self.adding = true;
        self.duplicate = false;
        // Match the existing frontend's canonical Scan roots without filesystem I/O on the UI thread.
        let work = cx.background_spawn(async move {
            std::fs::canonicalize(&root).unwrap_or_else(|_| {
                if root.is_absolute() {
                    root.clone()
                } else {
                    std::env::current_dir()
                        .map(|dir| dir.join(&root))
                        .unwrap_or(root)
                }
            })
        });
        self.add_task = Some(cx.spawn(async move |this, cx| {
            let root = work.await;
            let _ = this.update(cx, |this, cx| {
                this.adding = false;
                this.duplicate = this.roots.contains(&root);
                if !this.duplicate {
                    this.roots.push(root);
                    cx.emit(RootsChanged(this.roots.clone()));
                    this.discover(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn remove_root(&mut self, root: &PathBuf, cx: &mut Context<Self>) {
        self.roots.retain(|known| known != root);
        // Removed Projects disappear immediately, including while another request is running.
        self.projects = Arc::new(
            self.projects
                .iter()
                .filter(|project| self.roots.iter().any(|root| project.path.starts_with(root)))
                .cloned()
                .collect(),
        );
        self.model.retain_roots(&self.roots);
        self.selected
            .retain(|path| self.model.entry(path).is_some());
        self.rebuild_visible();
        // Cancel ownership, not just presentation: a removed/re-added path must
        // never accept an earlier measurement, even if its Stack is unchanged.
        self.measurement_epoch += 1;
        self.measurement_task = None;
        self.measuring = false;
        self.measured = 0;
        self.measurement_total = 0;
        cx.emit(RootsChanged(self.roots.clone()));
        self.discover(cx);
    }
}
impl Render for ProjectDiscovery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let strings = Lang::from_choice(self.language).strings();
        let covered = scan::covering_roots(&self.roots);
        let projects = self.projects.clone();

        let status = if self.scanning {
            strings.refresh_scanning.to_owned()
        } else {
            format!("{}: {}", strings.projects, projects.len())
        };
        let empty = if self.roots.is_empty() {
            format!("{} — {}", strings.empty_title, strings.empty_body)
        } else {
            format!(
                "{} — {}",
                strings.no_projects_title, strings.no_projects_body
            )
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .gap_3()
            .on_action(cx.listener(|this, _: &RefreshProjects, _, cx| {
                this.refresh(cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(strings.projects)
                    .child(
                        Button::new("refresh-projects")
                            .label(strings.refresh_projects)
                            .tooltip_with_action(
                                strings.refresh_projects,
                                &RefreshProjects,
                                Some("DevSweep"),
                            )
                            .disabled(self.scanning || self.roots.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .child(strings.roots)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.input)
                                .id("scan-root-path")
                                .aria_label(strings.roots),
                        ),
                    )
                    .child(
                        Button::new("add-scan-root")
                            .label(strings.add_directory)
                            .loading(self.adding)
                            .disabled(self.adding)
                            .on_click(cx.listener(|this, _, _, cx| this.add_root(cx))),
                    ),
            )
            .children(self.duplicate.then(|| {
                div()
                    .id("duplicate-root")
                    .test_support()
                    .child(strings.root_duplicate)
            }))
            .child(
                div()
                    .id("scan-roots")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .max_h_40()
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .children(self.roots.iter().zip(covered).map(|(root, covering)| {
                        let root = root.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id(SharedString::from(format!("root-path:{}", root.display())))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_x_scroll()
                                    .whitespace_nowrap()
                                    .child(root.to_string_lossy().into_owned()),
                            )
                            .children(covering.map(|_| {
                                div()
                                    .id(SharedString::from(format!(
                                        "covered-root:{}",
                                        root.display()
                                    )))
                                    .test_support()
                                    .w_40()
                                    .flex_shrink_0()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(strings.root_covered)
                            }))
                            .child(
                                Button::new(SharedString::from(
                                    root.to_string_lossy().into_owned(),
                                ))
                                .label(strings.remove)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.remove_root(&root, cx)),
                                ),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .flex_shrink_0()
                    .child(
                        Button::new("filter-all")
                            .label(match Lang::from_choice(self.language) {
                                Lang::En => "All types",
                                Lang::Zh => "全部类型",
                            })
                            .selected(self.filter.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.filter = None;
                                this.rebuild_visible();
                                this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                                cx.notify();
                            })),
                    )
                    .children(
                        [
                            Stack::Unity,
                            Stack::Flutter,
                            Stack::Ios,
                            Stack::Android,
                            Stack::Java,
                            Stack::Node,
                            Stack::Python,
                            Stack::Rust,
                            Stack::Dotnet,
                        ]
                        .into_iter()
                        .map(|stack| {
                            Button::new(SharedString::from(format!("filter-{stack:?}")))
                                .label(strings.stack(stack))
                                .selected(self.filter == Some(stack))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.filter = Some(stack);
                                    this.rebuild_visible();
                                    this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .child({
                let label = match Lang::from_choice(self.language) {
                    Lang::En => {
                        format!("{} of {} projects", self.visible.len(), self.projects.len())
                    }
                    Lang::Zh => format!("{} / {} 个项目", self.visible.len(), self.projects.len()),
                };
                div()
                    .id("project-results")
                    .test_support()
                    .aria_label(label.clone())
                    .child(label)
            })
            .children(
                (self.visible.is_empty() && !self.projects.is_empty()).then(|| {
                    div().child(match Lang::from_choice(self.language) {
                        Lang::En => {
                            "No projects of this type. Choose All types to see every project."
                        }
                        Lang::Zh => "没有此类型的项目。选择全部类型查看所有项目。",
                    })
                }),
            )
            .child(
                div()
                    .id("discovery-status")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(status.clone())
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .child(status)
                    .child({
                        let label = match (Lang::from_choice(self.language), self.measuring) {
                            (Lang::En, true) => {
                                format!("Measuring {}/{}", self.measured, self.measurement_total)
                            }
                            (Lang::En, false) => {
                                format!("Measured {}/{}", self.measured, self.measurement_total)
                            }
                            (Lang::Zh, true) => {
                                format!("体积计算中 {}/{}", self.measured, self.measurement_total)
                            }
                            (Lang::Zh, false) => {
                                format!("测量完成 {}/{}", self.measured, self.measurement_total)
                            }
                        };
                        div()
                            .id("project-measurement")
                            .test_support()
                            .aria_label(label.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(label)
                    }),
            )
            .children((!self.errors.is_empty()).then(|| {
                div()
                    .id("discovery-errors")
                    .test_support()
                    .flex()
                    .flex_col()
                    .max_h_40()
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .text_color(cx.theme().danger)
                    .child(format!(
                        "{} {}",
                        strings.refresh_failed,
                        self.errors.join("\n")
                    ))
                    .child(
                        Button::new("copy-discovery-errors")
                            .label(strings.discovery_copy_errors)
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    this.errors.join("\n"),
                                ))
                            })),
                    )
            }))
            .children(
                (projects.is_empty() && !self.scanning && self.errors.is_empty())
                    .then(|| div().id("discovery-empty").test_support().child(empty)),
            )
            .child(
                div()
                    .id("project-list-region")
                    .w_full()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(Self::navigate))
                    .child(
                        uniform_list(
                            "project-list",
                            self.visible.len(),
                            cx.processor(|this, range: Range<usize>, window, cx| {
                                range
                                    .map(|ix| this.render_project(this.visible[ix], window, cx))
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .w_full()
                        .flex_1()
                        .min_h_0(),
                    ),
            )
            .child({
                let count = self.selected.len();
                let summary = match Lang::from_choice(self.language) {
                    Lang::En => format!(
                        "{count} project{} selected",
                        if count == 1 { "" } else { "s" }
                    ),
                    Lang::Zh => format!("已选 {count} 个项目"),
                };
                div()
                    .id("project-selection")
                    .test_support()
                    .aria_label(summary.clone())
                    .flex_shrink_0()
                    .child(summary)
            })
    }
}

#[cfg(all(test, feature = "gpui-tests"))]
mod tests {
    use super::ProjectDiscovery;
    use crate::config::LanguageChoice;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn project_size_is_measured_in_background_and_refresh_keeps_known_size(
        cx: &mut TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target/payload"), [0; 4096]).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    ProjectDiscovery::new(vec![root.clone()], LanguageChoice::En, window, cx)
                })
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("project-size").label(),
                Some("Project size: 4096 B")
            );
            window
                .within("project-list")
                .within(gpui_kit::SharedString::from(
                    root.to_string_lossy().into_owned(),
                ))
                .click("project-check", cx);
            std::fs::write(root.join("target/payload"), [0; 8192]).unwrap();
            std::fs::create_dir_all(root.join("000-new/node_modules")).unwrap();
            std::fs::write(root.join("000-new/package.json"), "{}").unwrap();
            std::fs::write(root.join("000-new/node_modules/payload"), [0; 2048]).unwrap();
            window.click("refresh-projects", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window
                    .within("project-list")
                    .within(gpui_kit::SharedString::from(
                        root.to_string_lossy().into_owned()
                    ))
                    .find("project-size")
                    .label(),
                Some("Project size: 4096 B")
            );
            assert_eq!(
                window
                    .within("project-list")
                    .within(gpui_kit::SharedString::from(
                        root.to_string_lossy().into_owned()
                    ))
                    .find("project-check")
                    .checked(),
                Some(true)
            );
            assert_eq!(
                window
                    .within(gpui_kit::SharedString::from(
                        root.join("000-new").to_string_lossy().into_owned()
                    ))
                    .find("project-size")
                    .label(),
                Some("Project size: 2048 B")
            );
            assert_eq!(
                window
                    .within(gpui_kit::SharedString::from(
                        root.join("000-new").to_string_lossy().into_owned()
                    ))
                    .find("project-check")
                    .checked(),
                Some(false)
            );
        })
        .unwrap();
    }
    #[gpui_kit::test]
    fn stack_filter_and_keyboard_selection_survive_refresh(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for (name, marker) in [("alpha", "Cargo.toml"), ("beta", "package.json")] {
            std::fs::create_dir(root.join(name)).unwrap();
            std::fs::write(root.join(name).join(marker), "{}").unwrap();
        }
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    ProjectDiscovery::new(vec![root.clone()], LanguageChoice::En, window, cx)
                })
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(gpui_kit::SharedString::from(
                    root.join("alpha").to_string_lossy().into_owned(),
                ))
                .click("project-check", cx);
            window.press("down", cx);
            window.press("space", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("2 projects selected")
            );
            window.press("home", cx);
            window.press("space", cx);
            window.press("end", cx);
            window.press("space", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("0 projects selected")
            );
            window.click("filter-Rust", cx);
            assert_eq!(
                window.find("project-results").label(),
                Some("1 of 2 projects")
            );
            window.click(
                gpui_kit::SharedString::from(root.join("alpha").to_string_lossy().into_owned()),
                cx,
            );
            window.press("space", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("1 project selected")
            );
            window.click("filter-all", cx);
            window.click("refresh-projects", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("1 project selected")
            );
            window.click(
                gpui_kit::SharedString::from(root.join("alpha").to_string_lossy().into_owned()),
                cx,
            );
            window.press("down", cx);
            window.press("space", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("2 projects selected")
            );
            window.press("home", cx);
            window.press("space", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("1 project selected")
            );
            window.click("filter-Rust", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("1 project selected")
            );
            assert_eq!(
                window.find("project-results").label(),
                Some("1 of 2 projects")
            );
            window
                .within(gpui_kit::SharedString::from(
                    root.join("alpha").to_string_lossy().into_owned(),
                ))
                .click("project-check", cx);
            assert_eq!(window.find("project-check").checked(), Some(true));
            assert_eq!(
                window.find("project-selection").label(),
                Some("2 projects selected")
            );
            window.press("up", cx);
            window.press("space", cx);
            assert_eq!(window.find("project-check").checked(), Some(false));
        })
        .unwrap();
    }
    #[gpui_kit::test]
    fn pending_measurement_is_unknown_and_removed_root_cannot_restore_old_results(
        cx: &mut TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target/payload"), [0; 4096]).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    ProjectDiscovery::new(vec![root.clone()], LanguageChoice::En, window, cx)
                })
            })
            .unwrap()
        });
        // Advance the real executor only until discovery becomes visible; no
        // private handlers, injected results, or filesystem collaborators.
        let mut pending = false;
        for _ in 0..1000 {
            pending = cx
                .update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    window
                        .try_find("project-size")
                        .is_some_and(|row| row.label() == Some("Project size: Unknown · Measuring"))
                })
                .unwrap();
            if pending {
                break;
            }
            cx.background_executor.tick();
        }
        assert!(
            pending,
            "discovery must present unknown size before measuring"
        );
        cx.update_window(handle, |_, window, cx| {
            assert_eq!(
                window.find("project-measurement").label(),
                Some("Measuring 0/1")
            );
            window.within("project-list").click(
                gpui_kit::SharedString::from(root.to_string_lossy().into_owned()),
                cx,
            );
            window.press("space", cx);
            // A queued Refresh must not reset the draft or pretend to complete.
            window.click("refresh-projects", cx);
            assert_eq!(
                window.find("project-selection").label(),
                Some("1 project selected")
            );
            window.within("scan-roots").click(
                gpui_kit::SharedString::from(root.to_string_lossy().into_owned()),
                cx,
            );
            std::fs::write(root.join("target/payload"), [0; 8192]).unwrap();
            window.click("scan-root-path", cx);
            window.input(root.to_str().unwrap(), cx);
            window.click("add-scan-root", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("project-size").label(),
                Some("Project size: 8192 B")
            );
            assert_eq!(
                window.find("project-selection").label(),
                Some("0 projects selected")
            );
            assert_eq!(
                window.find("project-measurement").label(),
                Some("Measured 1/1")
            );
        })
        .unwrap();
    }
    #[gpui_kit::test]
    fn virtual_project_list_keeps_end_navigation_and_long_paths_at_large_text(
        cx: &mut TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .canonicalize()
            .unwrap()
            .join("很长的扫描根路径".repeat(6));
        std::fs::create_dir(&root).unwrap();
        for ix in 0..160 {
            let project = root.join(format!("项目-{ix:03}-{}", "长名称".repeat(12)));
            std::fs::create_dir(&project).unwrap();
            std::fs::write(project.join("Cargo.toml"), "[package]").unwrap();
        }
        let first = root.join(format!("项目-000-{}", "长名称".repeat(12)));
        let last = root.join(format!("项目-159-{}", "长名称".repeat(12)));
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
            gpui_kit::component::Theme::update(cx, |theme| theme.font_size = gpui_kit::px(20.));
        });
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds::new(
                        gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
                        gpui_kit::size(gpui_kit::px(880.), gpui_kit::px(560.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| {
                        ProjectDiscovery::new(vec![root.clone()], LanguageChoice::Zh, window, cx)
                    })
                },
            )
            .unwrap()
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                window
                    .try_find(gpui_kit::SharedString::from(
                        last.to_string_lossy().into_owned()
                    ))
                    .is_none(),
                "distant rows must be virtualized"
            );
            window.click(
                gpui_kit::SharedString::from(first.to_string_lossy().into_owned()),
                cx,
            );
            window.press("end", cx);
            window.press("space", cx);
            assert!(
                window
                    .find(gpui_kit::SharedString::from(
                        last.to_string_lossy().into_owned()
                    ))
                    .visible()
            );
            window
                .within(gpui_kit::SharedString::from(
                    last.to_string_lossy().into_owned(),
                ))
                .click("copy-project-path", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(last.to_str().unwrap())
            );
            assert_eq!(
                window.find("project-selection").label(),
                Some("已选 1 个项目")
            );
        })
        .unwrap();
        cx.simulate_window_resize(
            handle.into(),
            gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(760.)),
        );
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("home", cx);
            window.press("space", cx);
            assert!(
                window
                    .find(gpui_kit::SharedString::from(
                        first.to_string_lossy().into_owned()
                    ))
                    .visible()
            );
            assert_eq!(
                window.find("project-selection").label(),
                Some("已选 2 个项目")
            );
        })
        .unwrap();
    }
}
