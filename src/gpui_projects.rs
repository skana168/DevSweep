//! Project discovery presentation. All filesystem traversal runs on the background executor.
use crate::{
    config::LanguageChoice,
    i18n::Lang,
    scan::{self, Project},
};
use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::Button,
    input::{Input, InputState},
};
use gpui_kit::*;
use std::{path::PathBuf, sync::Arc};

actions!(projects, [RefreshProjects]);

pub(crate) struct RootsChanged(pub(crate) Vec<PathBuf>);

pub(crate) struct ProjectDiscovery {
    roots: Vec<PathBuf>,
    language: LanguageChoice,
    input: Entity<InputState>,
    projects: Arc<Vec<Project>>,
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
        self.revision += 1;
        let revision = self.revision;
        let roots = scan::active_roots(&self.roots);
        self.scanning = !roots.is_empty();
        self.errors.clear();
        if roots.is_empty() {
            self.task = None;
            self.projects = Arc::default();
            cx.notify();
            return;
        }
        let work = cx.background_spawn(async move { scan::scan_with_errors(&roots) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let report = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.revision != revision {
                    return;
                }
                this.projects = Arc::new(report.projects().to_vec());
                this.errors = report
                    .errors()
                    .iter()
                    .map(|error| format!("{}: {}", error.path().display(), error.message()))
                    .collect();
                this.scanning = false;
                cx.notify();
            });
        }));
        cx.notify();
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
                    this.refresh(cx);
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
        cx.emit(RootsChanged(self.roots.clone()));
        self.refresh(cx);
    }
}
impl Render for ProjectDiscovery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let strings = Lang::from_choice(self.language).strings();
        let covered = scan::covering_roots(&self.roots);
        let projects = self.projects.clone();
        let language = self.language;
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
            .on_action(cx.listener(|this, _: &RefreshProjects, _, cx| this.refresh(cx)))
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
                    .id("discovery-status")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(status.clone())
                    .child(status),
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
                uniform_list("project-list", projects.len(), move |range, _, _| {
                    let strings = Lang::from_choice(language).strings();
                    range
                        .map(|ix| {
                            let project = &projects[ix];
                            div()
                                .id(SharedString::from(
                                    project.path.to_string_lossy().into_owned(),
                                ))
                                .test_support()
                                .h_16()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(format!(
                                    "{} · {}",
                                    project.name,
                                    strings.stack(project.stack)
                                ))
                                .child(
                                    div()
                                        .id("project-path")
                                        .overflow_x_scroll()
                                        .whitespace_nowrap()
                                        .text_sm()
                                        .child(project.path.to_string_lossy().into_owned()),
                                )
                        })
                        .collect::<Vec<_>>()
                })
                .flex_1()
                .min_h_0(),
            )
    }
}
