use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use devsweep::cache::{self, CacheGroup, CacheRule};
use devsweep::config::{self, Config, LanguageChoice, ThemeChoice};
use devsweep::delete::{self, DeleteEvent, DeleteMode, Failure};
use devsweep::plan::{self, DeletePlan, LARGE_TARGET_BYTES, PlanProject};
use devsweep::rules::{self, Rule, Target};
use devsweep::scan::{self, Project, Stack};
use devsweep::size::{self, SizeEvent, SizeRequest};
use eframe::CreationContext;
use eframe::egui::{
    self, Align, Button, Checkbox, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Label,
    Layout, Modifiers, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui,
};
use egui_extras::{Column, TableBuilder, TableRow};

use crate::i18n::{Lang, Strings};
use crate::theme::{self, Tokens};

/// A window that spent this long in the background gets a fresh scan on focus.
const REFRESH_AFTER: Duration = Duration::from_secs(30);

/// A project row carries a name and a path; a target row carries one line.
const PROJECT_ROW_H: f32 = 56.0;
const TARGET_ROW_H: f32 = 30.0;

fn search_id() -> egui::Id {
    egui::Id::unique("project_search")
}

pub struct DevSweepApp {
    lang: Lang,
    config: Config,
    /// Built-in rules with the `rules.toml` overrides applied.
    rules: Vec<Rule>,
    /// Parse error from `rules.toml`; the app runs on built-ins meanwhile.
    rules_error: Option<String>,
    /// One-shot message under the roots list, e.g. a duplicate add.
    root_notice: Option<&'static str>,
    view: View,
    projects: Vec<Project>,
    /// Expanded targets and per-row UI state, keyed by the project's canonical path.
    entries: HashMap<PathBuf, ProjectEntry>,
    caches: Vec<CacheRow>,
    measurement: Option<Measurement>,
    search: String,
    filter: Option<Stack>,
    sort: Sort,
    /// Previous frame's window focus, for the focus-refresh hook.
    was_focused: Option<bool>,
    last_scan: Instant,
    /// Last project whose checkbox was clicked; Shift extends from here.
    selection_anchor: Option<PathBuf>,
    preview_open: bool,
    /// What the open confirm dialog will clean; `None` while it is closed.
    pending: Option<Cleanup>,
    /// The user has ticked "I understand" in the confirm dialog.
    confirm_large_ack: bool,
    execution: Option<Execution>,
    /// Result banner shown after a cleanup.
    flash: Option<Flash>,
    failures_open: bool,
    failures: Vec<Failure>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Projects,
    Caches,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Size,
}

#[derive(Clone, Copy)]
enum Cleanup {
    Projects,
    Cache(&'static CacheRule),
}

struct CacheRow {
    rule: &'static CacheRule,
    path: Option<PathBuf>,
    size: Option<u64>,
    /// The user's opt-in; risky entries start unchecked.
    checked: bool,
    cleaned: bool,
}

struct ProjectEntry {
    targets: Vec<Target>,
    expanded: bool,
}

/// A project's checked state, derived from its targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriState {
    Off,
    Partial,
    On,
}

impl ProjectEntry {
    fn state(&self) -> TriState {
        let checked = self.targets.iter().filter(|target| target.checked).count();
        if checked == 0 {
            TriState::Off
        } else if checked == self.targets.len() {
            TriState::On
        } else {
            TriState::Partial
        }
    }

    /// The parent's checkbox: check the safe targets. Risky ones stay out unless the
    /// project has nothing else, so the row can never dead-end; clearing always clears.
    fn set_checked(&mut self, checked: bool) {
        let safe_only = checked && self.targets.iter().any(|target| !target.dangerous);
        for target in &mut self.targets {
            target.checked = checked && !(safe_only && target.dangerous);
        }
    }
}

struct Measurement {
    done: usize,
    total: usize,
    rx: Receiver<SizeEvent>,
}

struct Execution {
    done: usize,
    total: usize,
    current: Option<String>,
    rx: Receiver<DeleteEvent>,
    abort: Arc<AtomicBool>,
    /// Show the "downloads again next time" hint in the result banner.
    global_cache: bool,
}

struct Flash {
    bytes: u64,
    aborted: bool,
    global_cache: bool,
}

impl DevSweepApp {
    pub fn new(cc: &CreationContext<'_>, config: Config) -> Self {
        theme::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, config.theme);
        let loaded = rules::load(config::rules_path().as_deref());
        let mut app = Self {
            lang: Lang::from_choice(config.language),
            config,
            rules: loaded.rules,
            rules_error: loaded.error,
            root_notice: None,
            view: View::Projects,
            projects: Vec::new(),
            entries: HashMap::new(),
            caches: cache::entries()
                .into_iter()
                .map(|entry| CacheRow {
                    checked: !entry.rule.dangerous,
                    rule: entry.rule,
                    path: entry.path,
                    size: None,
                    cleaned: false,
                })
                .collect(),
            measurement: None,
            search: String::new(),
            filter: None,
            sort: Sort::Name,
            was_focused: None,
            last_scan: Instant::now(),
            selection_anchor: None,
            preview_open: false,
            pending: None,
            confirm_large_ack: false,
            execution: None,
            flash: None,
            failures_open: false,
            failures: Vec::new(),
        };
        app.projects = scan::scan(&app.active_roots());
        app.sync_entries();
        app.measure_missing(&cc.egui_ctx);
        app
    }

    fn active_roots(&self) -> Vec<PathBuf> {
        scan::active_roots(&self.config.roots)
    }

    fn persist(&self) {
        config::save(&self.config);
    }

    fn strings(&self) -> &'static Strings {
        self.lang.strings()
    }

    fn add_directory(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if self.config.roots.contains(&path) {
            self.root_notice = Some(self.strings().root_duplicate);
            return;
        }
        // Only the new subtree is walked; existing projects keep their sizes.
        scan::merge_scan(&mut self.projects, scan::scan(std::slice::from_ref(&path)));
        self.config.roots.push(path);
        self.root_notice = None;
        self.sync_entries();
        self.measure_missing(ctx);
        self.persist();
        self.last_scan = Instant::now();
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        self.projects = scan::scan(&self.active_roots());
        self.sync_entries();
        // A cache recreated since the last scan becomes measurable again.
        for row in &mut self.caches {
            if row.cleaned && row.path.as_ref().is_some_and(|path| path.is_dir()) {
                row.cleaned = false;
                row.size = None;
            }
        }
        self.measure_missing(ctx);
        self.last_scan = Instant::now();
    }

    /// Track expanded/checked state per canonical path; survivors keep it, vanished
    /// projects and their selection drop out.
    fn sync_entries(&mut self) {
        scan::retain_scanned(&mut self.entries, &self.projects);
        let rules = &self.rules;
        for project in &self.projects {
            self.entries
                .entry(project.path.clone())
                .or_insert_with(|| ProjectEntry {
                    targets: rules::expand_targets(project, rules),
                    expanded: false,
                });
        }
    }

    fn project_size(&self, index: usize) -> Option<u64> {
        self.entries
            .get(&self.projects[index].path)
            .and_then(|entry| size::checked_size(&entry.targets))
    }

    /// Indices of projects matching the type filter and search, in display order.
    fn visible(&self) -> Vec<usize> {
        let query = self.search.trim().to_lowercase();
        let mut rows: Vec<usize> = self
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| self.filter.is_none_or(|stack| project.stack == stack))
            .filter(|(_, project)| {
                query.is_empty()
                    || project.name.to_lowercase().contains(&query)
                    || project
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
            })
            .map(|(index, _)| index)
            .collect();

        rows.sort_by(|&a, &b| {
            let name = |index: usize| self.projects[index].name.to_lowercase();
            match self.sort {
                Sort::Name => name(a)
                    .cmp(&name(b))
                    .then_with(|| self.projects[a].path.cmp(&self.projects[b].path)),
                // Unmeasured projects always sink below measured ones.
                Sort::Size => match (self.project_size(a), self.project_size(b)) {
                    (Some(left), Some(right)) => {
                        right.cmp(&left).then_with(|| name(a).cmp(&name(b)))
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => name(a).cmp(&name(b)),
                },
            }
        });
        rows
    }

    /// Sizes already computed for a target survive rescans; only unmeasured targets queue up.
    fn measure_missing(&mut self, ctx: &egui::Context) {
        let mut requests: Vec<SizeRequest> = self
            .projects
            .iter()
            .filter_map(|project| {
                let entry = self.entries.get(&project.path)?;
                let pending: Vec<PathBuf> = entry
                    .targets
                    .iter()
                    .filter(|target| target.size.is_none())
                    .map(|target| target.path.clone())
                    .collect();
                (!pending.is_empty()).then(|| SizeRequest {
                    project: project.path.clone(),
                    paths: pending,
                })
            })
            .collect();

        // Caches are measured as their own requests so each row fills in as it lands.
        requests.extend(self.caches.iter().filter_map(|row| {
            let path = row.path.as_ref()?;
            (row.size.is_none() && !row.cleaned && path.is_dir()).then(|| SizeRequest {
                project: path.clone(),
                paths: vec![path.clone()],
            })
        }));

        if requests.is_empty() {
            self.measurement = None;
            return;
        }

        let total = requests.len();
        let (tx, rx) = std::sync::mpsc::channel();
        size::measure(requests, tx);
        self.measurement = Some(Measurement { done: 0, total, rx });
        ctx.request_repaint();
    }

    fn poll_measurement(&mut self, ctx: &egui::Context) {
        let Some(measurement) = &mut self.measurement else {
            return;
        };
        let mut changed = false;
        let mut finished = false;

        loop {
            match measurement.rx.try_recv() {
                Ok(SizeEvent { project, sizes }) => {
                    measurement.done += 1;
                    if let Some(entry) = self.entries.get_mut(&project) {
                        for (path, size) in &sizes {
                            if let Some(target) =
                                entry.targets.iter_mut().find(|target| &target.path == path)
                            {
                                target.size = Some(*size);
                            }
                        }
                    }
                    if let Some(row) = self
                        .caches
                        .iter_mut()
                        .find(|row| row.path.as_ref() == Some(&project))
                    {
                        if let Some((_, size)) = sizes.first() {
                            if !row.cleaned {
                                row.size = Some(*size);
                            }
                        }
                    }
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }

        if finished {
            self.measurement = None;
        }
        if changed {
            ctx.request_repaint();
        }
        if self.measurement.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn selected_sizes_known(&self) -> bool {
        self.projects
            .iter()
            .filter_map(|project| self.entries.get(&project.path))
            .filter(|entry| entry.state() != TriState::Off)
            .all(|entry| size::checked_size(&entry.targets).is_some())
    }

    /// Sets a project's targets, and with Shift the whole range from the anchor, in the
    /// visible list order.
    fn toggle_selection(
        &mut self,
        visible: &[usize],
        position: usize,
        extend: bool,
        checked: bool,
    ) {
        let path = self.projects[visible[position]].path.clone();

        if extend
            && let Some(anchor) = self.selection_anchor.as_ref().and_then(|anchor| {
                visible
                    .iter()
                    .position(|&index| &self.projects[index].path == anchor)
            })
        {
            let (from, to) = (anchor.min(position), anchor.max(position));
            for &index in &visible[from..=to] {
                if let Some(entry) = self.entries.get_mut(&self.projects[index].path) {
                    entry.set_checked(checked);
                }
            }
            return;
        }

        if let Some(entry) = self.entries.get_mut(&path) {
            entry.set_checked(checked);
        }
        self.selection_anchor = Some(path);
    }

    /// Selectable visible projects: those with something to clean.
    fn selectable(&self, visible: &[usize]) -> Vec<PathBuf> {
        visible
            .iter()
            .map(|&index| &self.projects[index])
            .filter(|project| {
                self.entries
                    .get(&project.path)
                    .is_some_and(|entry| !entry.targets.is_empty())
            })
            .map(|project| project.path.clone())
            .collect()
    }

    /// The header checkbox over `paths`: one state if they all agree, mixed otherwise.
    fn bulk_state(&self, paths: &[PathBuf]) -> TriState {
        let mut states = paths
            .iter()
            .filter_map(|path| self.entries.get(path))
            .map(ProjectEntry::state);
        let Some(first) = states.next() else {
            return TriState::Off;
        };
        if states.all(|state| state == first) {
            first
        } else {
            TriState::Partial
        }
    }

    /// Whether every project already has something chosen; the bulk toggle fills when not.
    fn all_chosen(&self, paths: &[PathBuf]) -> bool {
        !paths.is_empty()
            && paths.iter().all(|path| {
                self.entries
                    .get(path)
                    .is_some_and(|entry| entry.state() != TriState::Off)
            })
    }

    /// ⌘/Ctrl+A: fill every visible project, or clear them when all are already chosen.
    fn toggle_select_all(&mut self, visible: &[usize]) {
        let selectable = self.selectable(visible);
        let checked = !self.all_chosen(&selectable);
        self.set_selected(&selectable, checked);
    }

    fn set_selected(&mut self, paths: &[PathBuf], selected: bool) {
        for path in paths {
            if let Some(entry) = self.entries.get_mut(path) {
                entry.set_checked(selected);
            }
        }
        self.selection_anchor = None;
    }

    fn build_plan(&self) -> DeletePlan {
        let projects: Vec<PlanProject> = self
            .projects
            .iter()
            .filter_map(|project| {
                let entry = self.entries.get(&project.path)?;
                (entry.state() != TriState::Off).then_some(PlanProject {
                    path: &project.path,
                    stack: project.stack,
                    targets: &entry.targets,
                })
            })
            .collect();
        plan::plan(&projects, &self.config.roots, &self.rules)
    }

    /// Plan for the cleanup the confirm dialog will run.
    fn plan_for(&self, cleanup: Cleanup) -> DeletePlan {
        match cleanup {
            Cleanup::Projects => self.build_plan(),
            Cleanup::Cache(rule) => self.cache_plan(rule).unwrap_or(DeletePlan {
                items: Vec::new(),
                skipped: Vec::new(),
                total_bytes: 0,
            }),
        }
    }

    fn cache_plan(&self, rule: &CacheRule) -> Option<DeletePlan> {
        let row = self.caches.iter().find(|row| row.rule.id == rule.id)?;
        cache::plan(rule.id, row.size?)
    }

    /// Sum of the cache sizes that exist on disk. `None` while one is measuring.
    fn cache_total(&self) -> Option<u64> {
        self.caches
            .iter()
            .filter(|row| row.cleaned || row.path.as_ref().is_some_and(|path| path.is_dir()))
            .try_fold(0, |sum, row| row.size.map(|size| sum + size))
    }

    /// Execute the current plan on a worker thread. The UI keeps painting while
    /// per-project events stream back.
    fn start_execution(&mut self, ctx: &egui::Context, cleanup: Cleanup) {
        let plan = self.plan_for(cleanup);
        if plan.items.is_empty() {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let abort = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&abort);
        let mode = self.config.delete_mode;
        std::thread::spawn(move || {
            delete::execute(plan, mode, &flag, tx);
        });

        self.execution = Some(Execution {
            done: 0,
            total: 0,
            current: None,
            rx,
            abort,
            global_cache: matches!(cleanup, Cleanup::Cache(_)),
        });
        self.flash = None;
        self.failures.clear();
        self.failures_open = false;
        ctx.request_repaint();
    }

    fn stop_execution(&self) {
        if let Some(execution) = &self.execution {
            execution.abort.store(true, Ordering::Relaxed);
        }
    }

    fn poll_execution(&mut self, ctx: &egui::Context) {
        let Some(execution) = &mut self.execution else {
            return;
        };
        let global_cache = execution.global_cache;
        let mut changed = false;
        let mut finished = None;
        let mut lost = false;

        loop {
            match execution.rx.try_recv() {
                Ok(DeleteEvent::Started { project, total }) => {
                    execution.total = total;
                    execution.current = Some(project_name(&project));
                    changed = true;
                }
                Ok(DeleteEvent::ProjectDone { project, deleted }) => {
                    if let Some(entry) = self.entries.get_mut(&project) {
                        // Only the targets that actually left the disk go away.
                        entry
                            .targets
                            .retain(|target| !deleted.contains(&target.path));
                        entry.set_checked(false);
                        entry.expanded = false;
                    }
                    for row in &mut self.caches {
                        if row.path.as_ref().is_some_and(|path| deleted.contains(path)) {
                            row.size = Some(0);
                            row.cleaned = true;
                            row.checked = false;
                        }
                    }
                    execution.done += 1;
                    changed = true;
                }
                Ok(DeleteEvent::Finished(report)) => {
                    finished = Some(report);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // A worker died without a report; don't leave the bar stuck.
                    lost = true;
                    break;
                }
            }
        }

        if let Some(report) = finished {
            self.execution = None;
            self.flash = Some(Flash {
                bytes: report.freed_bytes,
                aborted: report.aborted,
                global_cache,
            });
            self.failures = report.failures;
            self.failures_open = !self.failures.is_empty();
            self.selection_anchor = None;
            changed = true;
        } else if lost {
            self.execution = None;
        }
        if changed {
            ctx.request_repaint();
        }
        if self.execution.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// The dry-run preview: what the next cleanup would delete, before the confirm dialog.
    fn preview_dialog(&mut self, ctx: &egui::Context, t: &Tokens, s: &Strings, plan: &DeletePlan) {
        if !self.preview_open {
            return;
        }
        let mut close = false;

        let modal = egui::Modal::new(egui::Id::unique("preview"))
            .frame(
                Frame::new()
                    .fill(t.bg)
                    .stroke(Stroke::new(1.0, t.border_strong))
                    .shadow(theme::shadow(ctx.theme() == egui::Theme::Dark))
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.set_width(640.0);
                ui.horizontal(|ui| {
                    ui.label(theme::title(s.preview, 20.0, t.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let button =
                            Button::new(RichText::new("×").color(t.text_muted)).frame(false);
                        if ui.add(button).clicked() {
                            close = true;
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(size::format_size(plan.total_bytes))
                                .size(16.0)
                                .monospace()
                                .color(t.good),
                        );
                    });
                });
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                ScrollArea::vertical()
                    .max_height(420.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut current: Option<&Path> = None;
                        for item in &plan.items {
                            if current != Some(item.project_path.as_path()) {
                                if current.is_some() {
                                    ui.add_space(8.0);
                                }
                                current = Some(item.project_path.as_path());
                                ui.label(
                                    RichText::new(project_name(&item.project_path))
                                        .size(13.0)
                                        .strong()
                                        .color(t.text),
                                );
                            }
                            ui.horizontal(|ui| {
                                let label = Label::new(
                                    RichText::new(item.target_path.display().to_string())
                                        .size(12.0)
                                        .monospace()
                                        .color(t.text_muted),
                                )
                                .truncate();
                                ui.add(label)
                                    .on_hover_text(item.target_path.display().to_string());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(size::format_size(item.bytes))
                                            .size(12.0)
                                            .monospace()
                                            .color(t.text),
                                    );
                                    if item.large {
                                        ui.label(
                                            RichText::new(format!(
                                                "> {}",
                                                size::format_size(LARGE_TARGET_BYTES)
                                            ))
                                            .size(11.0)
                                            .color(t.danger),
                                        );
                                    }
                                });
                            });
                        }

                        if !plan.skipped.is_empty() {
                            ui.add_space(16.0);
                            ui.label(RichText::new(s.skipped).size(13.0).strong().color(t.danger));
                            for skip in &plan.skipped {
                                let label = Label::new(
                                    RichText::new(skip.path.display().to_string())
                                        .size(11.0)
                                        .monospace()
                                        .color(t.text_muted),
                                )
                                .truncate();
                                ui.add(label).on_hover_text(skip.path.display().to_string());
                                ui.label(
                                    RichText::new(s.skip_reason(skip.reason))
                                        .size(11.0)
                                        .color(t.danger),
                                );
                            }
                        }
                    });

                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if secondary_button(ui, s.close, t) {
                        close = true;
                    }
                });
            });

        if close || modal.should_close() {
            self.preview_open = false;
        }
    }

    /// "Freed 380 MB" after a cleanup, with a link to the failure list.
    fn flash_banner(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        let Some(flash) = self.flash.as_ref() else {
            return;
        };
        let freed = if flash.bytes == 0 {
            "0 MB".to_string()
        } else {
            size::format_size(flash.bytes)
        };
        let mut label = format!("{} {freed}", s.freed);
        if flash.aborted {
            label = format!("{label} · {}", s.aborted);
        }
        if flash.global_cache {
            label = format!("{label} · {}", s.cache_redownload);
        }
        let failure_count = self.failures.len();
        let mut dismiss = false;
        let mut show_failures = false;

        Frame::new()
            .stroke(t.hairline())
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(14.0).color(t.good));
                    if failure_count > 0 {
                        let button = Button::new(
                            RichText::new(format!("{} ({failure_count})", s.failures))
                                .size(13.0)
                                .color(t.danger),
                        )
                        .frame(false);
                        if ui.add(button).clicked() {
                            show_failures = true;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let close =
                            Button::new(RichText::new("×").color(t.text_muted)).frame(false);
                        if ui.add(close).clicked() {
                            dismiss = true;
                        }
                    });
                });
            });
        ui.add_space(12.0);

        if dismiss {
            self.flash = None;
        }
        if show_failures {
            self.failures_open = true;
        }
    }

    fn confirm_dialog(
        &mut self,
        ctx: &egui::Context,
        t: &Tokens,
        s: &Strings,
        plan: &DeletePlan,
        known: bool,
        cleanup: Cleanup,
    ) {
        if self.pending.is_none() {
            return;
        }

        let projects = plan
            .items
            .iter()
            .map(|item| &item.project_path)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let large: Vec<&plan::PlanItem> = plan.items.iter().filter(|item| item.large).collect();
        let total = if known {
            size::format_size(plan.total_bytes)
        } else {
            "…".to_string()
        };
        let title = match cleanup {
            Cleanup::Cache(rule) => format!("{} · {}", s.confirm_title, rule.name),
            Cleanup::Projects => s.confirm_title.to_string(),
        };
        let mut ack = self.confirm_large_ack;
        let mut close = false;
        let mut confirm = false;

        let modal = egui::Modal::new(egui::Id::unique("confirm_delete"))
            .frame(
                Frame::new()
                    .fill(t.bg)
                    .stroke(Stroke::new(1.0, t.border_strong))
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(theme::title(title, 20.0, t.text));
                ui.add_space(12.0);
                if let Cleanup::Projects = cleanup {
                    stat_row(ui, s.projects, &projects.to_string(), t);
                }
                stat_row(ui, s.items, &plan.items.len().to_string(), t);
                stat_row(ui, s.preview_total, &total, t);

                if !large.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new(s.large_warning).size(13.0).color(t.danger));
                    for item in &large {
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                item.target_path.display(),
                                size::format_size(item.bytes)
                            ))
                            .size(11.0)
                            .monospace()
                            .color(t.danger),
                        );
                    }
                    ui.add_space(8.0);
                    ui.checkbox(&mut ack, RichText::new(s.large_ack).size(13.0));
                }

                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Unknown sizes could hide a > 5 GB target, so wait for them.
                    if danger_button(ui, s.confirm, t, known && (large.is_empty() || ack)) {
                        confirm = true;
                    }
                    ui.add_space(4.0);
                    if secondary_button(ui, s.cancel, t) {
                        close = true;
                    }
                });
                if ui.ctx().input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
            });

        self.confirm_large_ack = ack;
        if confirm {
            self.pending = None;
            self.start_execution(ctx, cleanup);
        } else if close || modal.backdrop_response.clicked() {
            self.pending = None;
        }
    }

    fn failures_dialog(&mut self, ctx: &egui::Context, t: &Tokens, s: &Strings) {
        if !self.failures_open {
            return;
        }
        let mut close = false;

        let modal = egui::Modal::new(egui::Id::unique("delete_failures"))
            .frame(
                Frame::new()
                    .fill(t.bg)
                    .stroke(Stroke::new(1.0, t.border_strong))
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(theme::title(
                    format!("{} ({})", s.failures, self.failures.len()),
                    20.0,
                    t.danger,
                ));
                ui.add_space(12.0);
                ScrollArea::vertical()
                    .max_height(320.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for failure in &self.failures {
                            ui.horizontal(|ui| {
                                let rel = failure
                                    .path
                                    .strip_prefix(&failure.project_path)
                                    .unwrap_or(&failure.path);
                                ui.label(
                                    RichText::new(project_name(&failure.project_path))
                                        .size(13.0)
                                        .strong()
                                        .color(t.text),
                                );
                                ui.label(
                                    RichText::new(rel.display().to_string())
                                        .size(12.0)
                                        .monospace()
                                        .color(t.text_muted),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(s.failure_reason(failure.reason))
                                            .size(12.0)
                                            .color(t.danger),
                                    );
                                });
                            });
                        }
                    });
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if secondary_button(ui, s.close, t) {
                        close = true;
                    }
                });
                if ui.ctx().input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
            });

        if close || modal.backdrop_response.clicked() {
            self.failures_open = false;
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        ui.label(theme::title("DevSweep", 22.0, t.text));
        ui.add_space(16.0);
        ui.label(theme::hud(s.roots, t.text_muted));
        ui.add_space(4.0);

        self.roots_editor(ui, t, s);

        ui.add_space(20.0);
        let width = ui.available_width();
        ui.label(theme::hud(s.type_filter, t.text_muted));
        ui.add_space(4.0);
        let filters = std::iter::once((None, s.all)).chain(
            [Stack::Flutter, Stack::Android, Stack::Ios, Stack::Node]
                .map(|stack| (Some(stack), s.stack(stack))),
        );
        for (filter, label) in filters {
            let count = match filter {
                Some(stack) => self
                    .projects
                    .iter()
                    .filter(|project| project.stack == stack)
                    .count(),
                None => self.projects.len(),
            };
            let active = self.view == View::Projects && self.filter == filter;
            let label = RichText::new(format!("{label}  {count}")).size(14.0);
            if ui
                .add_sized([width, 30.0], nav_button(label, active, t))
                .clicked()
            {
                self.view = View::Projects;
                self.filter = filter;
            }
        }

        ui.add_space(12.0);
        let total = self
            .cache_total()
            .map(size::format_size)
            .unwrap_or_else(|| "…".to_string());
        if ui
            .add_sized(
                [width, 32.0],
                nav_button(
                    RichText::new(format!("{}  {total}", s.global_caches)).size(14.0),
                    self.view == View::Caches,
                    t,
                ),
            )
            .clicked()
        {
            self.view = View::Caches;
        }

        ui.add_space(8.0);
        if ui
            .add_sized(
                [width, 32.0],
                nav_button(
                    RichText::new(s.settings).size(14.0),
                    self.view == View::Settings,
                    t,
                ),
            )
            .clicked()
        {
            self.view = View::Settings;
        }
    }

    /// The scan-root list, shared by the sidebar and the settings page.
    fn roots_editor(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        let covering = scan::covering_roots(&self.config.roots);
        let mut remove = None;
        for (index, root) in self.config.roots.iter().enumerate() {
            let covered = covering[index].is_some();
            ui.horizontal(|ui| {
                let name = root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string());
                let color = if covered { t.text_muted } else { t.text };
                let name_width = (ui.available_width() - 24.0).max(40.0);
                let label = Label::new(RichText::new(name).size(14.0).color(color)).truncate();
                let hover = match &covering[index] {
                    Some(parent) => format!("{}\n{}", s.root_covered, parent.display()),
                    None => root.display().to_string(),
                };
                ui.add_sized([name_width, 18.0], label).on_hover_text(hover);
                let response =
                    ui.add(Button::new(RichText::new("×").color(t.text_muted)).frame(false));
                if response.on_hover_text(s.remove).clicked() {
                    remove = Some(index);
                }
            });
            if let Some(parent) = &covering[index] {
                ui.label(
                    RichText::new(format!("{} · {}", s.root_covered, project_name(parent)))
                        .size(11.0)
                        .color(t.text_muted),
                )
                .on_hover_text(parent.display().to_string());
            }
        }
        if let Some(index) = remove {
            self.remove_root(index, ui.ctx());
        }

        ui.add_space(8.0);
        let width = ui.available_width();
        let button = Button::new(theme::button_text(format!("+ {}", s.add_directory), t.text))
            .fill(t.bg)
            .stroke(t.hairline());
        if ui.add_sized([width, 34.0], button).clicked() {
            self.add_directory(ui.ctx());
        }
        if let Some(notice) = self.root_notice {
            ui.add_space(4.0);
            ui.label(RichText::new(notice).size(11.0).color(t.danger));
        }
    }

    fn remove_root(&mut self, index: usize, ctx: &egui::Context) {
        self.config.roots.remove(index);
        self.root_notice = None;
        // Projects under a remaining root stay. Roots that are no longer covered get
        // re-walked, since the removed parent's scan may have supplied their projects.
        self.projects.retain(|project| {
            self.config
                .roots
                .iter()
                .any(|root| project.path.starts_with(root))
        });
        for root in self.active_roots() {
            scan::merge_scan(&mut self.projects, scan::scan(std::slice::from_ref(&root)));
        }
        self.sync_entries();
        self.measure_missing(ctx);
        self.persist();
        self.last_scan = Instant::now();
    }

    fn open_rules(&mut self) {
        let Some(path) = config::rules_path() else {
            return;
        };
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, rules::TEMPLATE);
        }
        let _ = opener::open(&path);
    }

    fn settings_content(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(theme::title(s.settings, 26.0, t.text));
                ui.add_space(16.0);

                ui.label(theme::hud(s.language, t.text_muted));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (choice, label) in [
                        (LanguageChoice::System, s.system),
                        (LanguageChoice::Zh, s.lang_zh),
                        (LanguageChoice::En, s.lang_en),
                    ] {
                        if ui
                            .add(nav_button(
                                RichText::new(label).size(14.0),
                                self.config.language == choice,
                                t,
                            ))
                            .clicked()
                        {
                            self.config.language = choice;
                            self.lang = Lang::from_choice(choice);
                            self.persist();
                        }
                    }
                });
                ui.add_space(20.0);

                ui.label(theme::hud(s.theme, t.text_muted));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (choice, label) in [
                        (ThemeChoice::System, s.system),
                        (ThemeChoice::Dark, s.dark),
                        (ThemeChoice::Light, s.light),
                    ] {
                        if ui
                            .add(nav_button(
                                RichText::new(label).size(14.0),
                                self.config.theme == choice,
                                t,
                            ))
                            .clicked()
                        {
                            self.config.theme = choice;
                            theme::apply(ui.ctx(), choice);
                            self.persist();
                        }
                    }
                });
                ui.add_space(20.0);

                ui.label(theme::hud(s.delete_mode, t.text_muted));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (mode, label) in [
                        (DeleteMode::Trash, s.delete_trash),
                        (DeleteMode::Direct, s.delete_direct),
                    ] {
                        if ui
                            .add(nav_button(
                                RichText::new(label).size(14.0),
                                self.config.delete_mode == mode,
                                t,
                            ))
                            .clicked()
                        {
                            self.config.delete_mode = mode;
                            self.persist();
                        }
                    }
                });
                ui.add_space(20.0);

                ui.label(theme::hud(s.roots, t.text_muted));
                ui.add_space(4.0);
                self.roots_editor(ui, t, s);
                ui.add_space(20.0);

                ui.label(theme::hud(s.rules_file, t.text_muted));
                ui.add_space(4.0);
                if let Some(path) = config::rules_path() {
                    ui.label(
                        RichText::new(path.display().to_string())
                            .size(11.0)
                            .monospace()
                            .color(t.text_muted),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if secondary_button(ui, s.open_rules, t) {
                        self.open_rules();
                    }
                    ui.label(
                        RichText::new(s.rules_restart)
                            .size(11.0)
                            .color(t.text_muted),
                    );
                });
                if let Some(error) = &self.rules_error {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("{} {error}", s.rules_error))
                            .size(12.0)
                            .color(t.danger),
                    );
                }
            });
    }

    fn content(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        s: &Strings,
        plan: &DeletePlan,
        visible: &[usize],
    ) {
        if self.config.roots.is_empty() {
            if self.card(ui, t, s.empty_title, s.empty_body, Some(s.add_directory)) {
                self.add_directory(ui.ctx());
            }
            return;
        }

        self.flash_banner(ui, t, s);

        ui.horizontal(|ui| {
            ui.label(theme::title(
                format!("{} · {}", s.projects, visible.len()),
                26.0,
                t.text,
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count = plan.items.len();
                let label = if count == 0 {
                    s.clean.to_string()
                } else {
                    let total = if self.selected_sizes_known() {
                        size::format_size(plan.total_bytes)
                    } else {
                        "…".to_string()
                    };
                    format!("{} {} {} · {}", s.clean, count, s.items, total)
                };
                let running = self.execution.is_some();
                if danger_button(ui, &label, t, count > 0 && !running) {
                    self.pending = Some(Cleanup::Projects);
                    self.confirm_large_ack = false;
                    self.preview_open = false;
                }

                ui.add_space(4.0);
                if action_button(
                    ui,
                    s.preview,
                    Kind::Secondary,
                    t,
                    count > 0,
                    egui::vec2(96.0, 38.0),
                ) {
                    self.preview_open = true;
                }

                ui.add_space(8.0);
                // Taken out so the closure does not hold a second borrow of `self`.
                let mut search = std::mem::take(&mut self.search);
                ui.add(
                    TextEdit::singleline(&mut search)
                        .id(search_id())
                        .hint_text(s.search_hint)
                        .desired_width(190.0)
                        .min_size(egui::vec2(0.0, 38.0))
                        .margin(egui::Margin::symmetric(10, 0))
                        .align(egui::Align2::LEFT_CENTER),
                );
                self.search = search;
            });
        });
        ui.add_space(12.0);

        if self.projects.is_empty() {
            self.card(ui, t, s.no_projects_title, s.no_projects_body, None);
            return;
        }
        if visible.is_empty() {
            if self.card(
                ui,
                t,
                s.no_match_title,
                s.no_match_body,
                Some(s.clear_filters),
            ) {
                self.search.clear();
                self.filter = None;
            }
            return;
        }

        let selectable = self.selectable(visible);
        let bulk = self.bulk_state(&selectable);
        let all_chosen = self.all_chosen(&selectable);
        let mut select_all = false;
        let mut sort_clicked = None;
        let mut toggled = None;

        // Rules run across the whole table, padding included.
        let span = ui.available_rect_before_wrap().x_range();
        let show_items = ui.available_width() >= 700.0;
        let sort = self.sort;

        Frame::new()
            .stroke(t.hairline())
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| {
                let mut table = TableBuilder::new(ui)
                    .striped(false)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::exact(40.0))
                    .column(Column::remainder().at_least(220.0).clip(true))
                    .column(Column::exact(120.0));
                if show_items {
                    table = table.column(Column::exact(80.0));
                }
                table
                    .column(Column::exact(120.0))
                    .header(42.0, |mut header| {
                        header.col(|ui| {
                            select_all = tri_checkbox(ui, t, bulk, !selectable.is_empty())
                                .on_hover_text(s.select_all)
                                .clicked();
                            row_rule(ui, t, span);
                        });
                        header.col(|ui| {
                            if sort_header(ui, s.project_column, "↑", sort == Sort::Name, t) {
                                sort_clicked = Some(Sort::Name);
                            }
                        });
                        header.col(|ui| {
                            ui.label(theme::hud(s.stack_column, t.text_muted));
                        });
                        if show_items {
                            header.col(|ui| {
                                ui.label(theme::hud(s.items, t.text_muted));
                            });
                        }
                        header.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if sort_header(ui, s.reclaimable, "↓", sort == Sort::Size, t) {
                                    sort_clicked = Some(Sort::Size);
                                }
                            });
                        });
                    })
                    .body(|mut body| {
                        let projects = &self.projects;
                        let entries = &mut self.entries;
                        for (position, &index) in visible.iter().enumerate() {
                            let project = &projects[index];
                            let Some(entry) = entries.get_mut(&project.path) else {
                                continue;
                            };
                            let last_project = position + 1 == visible.len();
                            let children = entry.expanded && !entry.targets.is_empty();

                            let mut toggle = None;
                            body.row(PROJECT_ROW_H, |mut row| {
                                toggle = project_table_row(
                                    &mut row,
                                    project,
                                    entry,
                                    last_project && !children,
                                    show_items,
                                    t,
                                    s,
                                    span,
                                );
                            });
                            toggled = toggle.map(|checked| (position, checked));
                            if children {
                                let targets = &mut entry.targets;
                                let last_target = targets.len() - 1;
                                for (i, target) in targets.iter_mut().enumerate() {
                                    body.row(TARGET_ROW_H, |mut row| {
                                        target_table_row(
                                            &mut row,
                                            project,
                                            target,
                                            last_project && i == last_target,
                                            show_items,
                                            t,
                                            s,
                                            span,
                                        );
                                    });
                                }
                            }
                        }
                    });
            });

        if select_all {
            self.set_selected(&selectable, !all_chosen);
        }
        if let Some(key) = sort_clicked {
            self.sort = key;
        }
        if let Some((position, checked)) = toggled {
            let extend = ui.input(|input| input.modifiers.shift);
            self.toggle_selection(visible, position, extend, checked);
        }
    }

    fn caches_content(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        self.flash_banner(ui, t, s);

        ui.horizontal(|ui| {
            ui.label(theme::title(s.global_caches, 26.0, t.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let total = self
                    .cache_total()
                    .map(size::format_size)
                    .unwrap_or_else(|| "…".to_string());
                ui.label(RichText::new(total).size(18.0).monospace().color(t.good));
            });
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(s.cache_redownload)
                .size(12.0)
                .color(t.text_muted),
        );
        ui.add_space(12.0);

        let mut clean = None;
        let busy = self.execution.is_some();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (group, title) in [
                    (CacheGroup::Dependency, s.cache_group_dependency),
                    (CacheGroup::Toolchain, s.cache_group_toolchain),
                ] {
                    let mut heading = false;
                    for row in self.caches.iter_mut().filter(|row| row.rule.group == group) {
                        if !heading {
                            ui.label(theme::hud(title, t.text_muted));
                            ui.add_space(6.0);
                            heading = true;
                        }
                        if let Some(rule) = cache_row(ui, row, t, s, busy) {
                            clean = Some(rule);
                        }
                        ui.add_space(6.0);
                    }
                    if heading {
                        ui.add_space(12.0);
                    }
                }
            });

        if let Some(rule) = clean {
            self.pending = Some(Cleanup::Cache(rule));
            self.confirm_large_ack = false;
        }
    }

    fn card(&self, ui: &mut Ui, t: &Tokens, title: &str, body: &str, action: Option<&str>) -> bool {
        let mut clicked = false;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.18).clamp(24.0, 140.0));
            Frame::new()
                .fill(t.surface)
                .stroke(t.hairline())
                .inner_margin(32)
                .show(ui, |ui| {
                    ui.set_width(440.0);
                    ui.vertical_centered(|ui| {
                        ui.label(theme::title(title, 22.0, t.text));
                        ui.add_space(8.0);
                        ui.label(RichText::new(body).size(14.0).color(t.text_muted));
                        if let Some(action) = action {
                            ui.add_space(20.0);
                            clicked = primary_button(ui, action, t, true);
                        }
                    });
                });
        });
        clicked
    }
}

impl eframe::App for DevSweepApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_measurement(ctx);
        self.poll_execution(ctx);

        // Remember the window size; `on_exit` writes it out with the rest of the config.
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            self.config.window = Some(config::Window {
                width: rect.width(),
                height: rect.height(),
            });
        }

        // Refresh stale data when the window comes back, but only after the grace period.
        let focused = ctx.input(|input| input.viewport().focused);
        if focused == Some(true)
            && self.was_focused == Some(false)
            && self.last_scan.elapsed() >= REFRESH_AFTER
        {
            self.rescan(ctx);
        }
        if focused.is_some() {
            self.was_focused = focused;
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let t = theme::tokens(ui.ctx().theme());
        let s = self.strings();

        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(t.surface)
                    .inner_margin(egui::Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(measurement) = &self.measurement {
                        ui.label(
                            RichText::new(format!(
                                "{} {}/{}",
                                s.measuring, measurement.done, measurement.total
                            ))
                            .size(11.0)
                            .color(t.text_muted),
                        );
                    }
                    if let Some(execution) = &self.execution {
                        ui.label(
                            RichText::new(format!(
                                "{} {}/{} · {}",
                                s.cleaning,
                                execution.done,
                                execution.total,
                                execution.current.as_deref().unwrap_or_default()
                            ))
                            .size(11.0)
                            .color(t.text),
                        );
                        let aborting = execution.abort.load(Ordering::Relaxed);
                        let button = Button::new(RichText::new(s.abort).size(11.0))
                            .frame(false)
                            .stroke(Stroke::new(1.0, t.border));
                        if ui.add_enabled(!aborting, button).clicked() {
                            self.stop_execution();
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(theme::hud(
                            concat!("DevSweep ", env!("CARGO_PKG_VERSION")),
                            t.text_muted,
                        ));
                    });
                });
            });

        egui::Panel::left("scan_roots")
            .exact_size(220.0)
            .resizable(false)
            .frame(Frame::new().fill(t.surface).inner_margin(16))
            .show(ui, |ui| self.sidebar(ui, t, s));

        let visible = self.visible();
        if self.view == View::Projects {
            // ⌘A belongs to the search box while it has focus.
            let search_focused = ui.memory(|memory| memory.focused()) == Some(search_id());
            if !search_focused {
                let select_all = ui.ctx().input_mut(|input| {
                    input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::A))
                });
                if select_all {
                    self.toggle_select_all(&visible);
                }
            }
            let focus_search = ui.ctx().input_mut(|input| {
                input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F))
            });
            if focus_search {
                ui.memory_mut(|memory| memory.request_focus(search_id()));
            }
        }

        let plan = self.build_plan();

        egui::CentralPanel::default()
            .frame(Frame::new().fill(t.bg).inner_margin(24))
            .show(ui, |ui| match self.view {
                View::Projects => self.content(ui, t, s, &plan, &visible),
                View::Caches => self.caches_content(ui, t, s),
                View::Settings => self.settings_content(ui, t, s),
            });

        self.preview_dialog(ui.ctx(), t, s, &plan);

        if let Some(cleanup) = self.pending {
            let pending_plan = self.plan_for(cleanup);
            let known = match cleanup {
                Cleanup::Projects => self.selected_sizes_known(),
                Cleanup::Cache(_) => true,
            };
            self.confirm_dialog(ui.ctx(), t, s, &pending_plan, known, cleanup);
        }
        self.failures_dialog(ui.ctx(), t, s);
    }

    fn on_exit(&mut self) {
        self.persist();
    }
}

fn nav_button<'a>(label: RichText, active: bool, t: &Tokens) -> Button<'a> {
    // The chosen one is a well with a stronger hairline; craft inverts on hover, not on state.
    Button::new(label)
        .fill(if active {
            t.sunken
        } else {
            Color32::TRANSPARENT
        })
        .stroke(if active {
            Stroke::new(1.0, t.border_strong)
        } else {
            Stroke::NONE
        })
}

/// Which of the reference's button variants.
#[derive(Clone, Copy)]
enum Kind {
    /// A solid block in the inverted colours: the main, non-destructive action.
    Primary,
    /// Outlined: opens something, or cancels.
    Secondary,
    /// Solid red: the click that deletes.
    Danger,
}

fn action_button(
    ui: &mut Ui,
    label: &str,
    kind: Kind,
    t: &Tokens,
    enabled: bool,
    min_size: egui::Vec2,
) -> bool {
    let (fill, ink, stroke) = match kind {
        Kind::Primary => (t.invert_bg, t.invert_fg, Stroke::NONE),
        Kind::Secondary => (t.bg, t.text, t.hairline()),
        Kind::Danger => (t.danger, Color32::WHITE, Stroke::NONE),
    };
    // Disabled fades the whole button, the way the reference does it.
    let (fill, ink) = if enabled {
        (fill, ink)
    } else {
        (fill.gamma_multiply(0.5), ink.gamma_multiply(0.5))
    };
    ui.add_enabled(
        enabled,
        Button::new(theme::button_text(label, ink))
            .fill(fill)
            .stroke(stroke)
            .min_size(min_size),
    )
    .clicked()
}

fn secondary_button(ui: &mut Ui, label: &str, t: &Tokens) -> bool {
    action_button(ui, label, Kind::Secondary, t, true, egui::vec2(88.0, 34.0))
}

/// A clickable column label; the active column carries a direction arrow.
fn sort_header(ui: &mut Ui, label: &str, arrow: &str, active: bool, t: &Tokens) -> bool {
    let text = if active {
        format!("{label} {arrow}")
    } else {
        label.to_string()
    };
    let color = if active { t.text } else { t.text_muted };
    ui.add(Button::new(theme::hud(text, color)).frame(false))
        .clicked()
}

fn stat_row(ui: &mut Ui, label: &str, value: &str, t: &Tokens) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0).color(t.text_muted));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(13.0).monospace().color(t.text));
        });
    });
}

fn primary_button(ui: &mut Ui, label: &str, t: &Tokens, enabled: bool) -> bool {
    action_button(
        ui,
        label,
        Kind::Primary,
        t,
        enabled,
        egui::vec2(160.0, 38.0),
    )
}

fn danger_button(ui: &mut Ui, label: &str, t: &Tokens, enabled: bool) -> bool {
    action_button(ui, label, Kind::Danger, t, enabled, egui::vec2(160.0, 38.0))
}

/// One project row: checkbox, name + path, stack, target count and reclaimable size.
/// Returns the state its checkbox should be set to when it was clicked.
///
/// The checker's "last row" flag skips the rule under the final row, where the
/// table's own frame already draws one.
#[allow(clippy::too_many_arguments)]
fn project_table_row(
    row: &mut TableRow<'_, '_>,
    project: &Project,
    entry: &mut ProjectEntry,
    last: bool,
    show_items: bool,
    t: &Tokens,
    s: &Strings,
    span: egui::Rangef,
) -> Option<bool> {
    let state = entry.state();
    let mut toggle = None;
    row.col(|ui| {
        if !last {
            row_rule(ui, t, span);
        }
        let response = tri_checkbox(ui, t, state, !entry.targets.is_empty());
        if response.clicked() {
            // A full parent clears; anything less fills.
            toggle = Some(state != TriState::On);
        }
    });

    let mut expand = false;
    row.col(|ui| {
        // The whole cell is the target; the labels inside take no clicks.
        let open = ui.interact(
            ui.max_rect(),
            ui.scope_id().with("row-open"),
            Sense::click(),
        );
        ui.vertical(|ui| {
            ui.label(RichText::new(&project.name).size(15.0).color(t.text));
            let path = Label::new(
                RichText::new(project.path.display().to_string())
                    .size(12.0)
                    .monospace()
                    .color(t.text_muted),
            )
            .truncate();
            ui.add(path)
                .on_hover_text(project.path.display().to_string());
        });
        if !entry.targets.is_empty() {
            expand = open
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
        }
    });
    row.col(|ui| {
        chip(ui, s.stack(project.stack), t);
    });
    if show_items {
        row.col(|ui| {
            let count = entry.targets.len();
            let color = if count == 0 { t.text_muted } else { t.text };
            ui.label(
                RichText::new(count.to_string())
                    .size(13.0)
                    .monospace()
                    .color(color),
            );
        });
    }
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (text, color) = match size::checked_size(&entry.targets) {
                Some(bytes) => (size::format_size(bytes), t.good),
                None => ("…".to_string(), t.text_muted),
            };
            ui.label(RichText::new(text).size(14.0).monospace().color(color));
        });
    });

    if expand {
        entry.expanded = !entry.expanded;
    }
    toggle
}

/// A checkbox with a third, "some children are checked" state: filled with a check,
/// filled with a dash, or empty.
fn tri_checkbox(ui: &mut Ui, t: &Tokens, state: TriState, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::click());
    let fade = |color: Color32| {
        if enabled {
            color
        } else {
            color.gamma_multiply(0.5)
        }
    };
    let on = state != TriState::Off;
    let hovered = response.hovered() && enabled;
    let border = if hovered { t.border_strong } else { t.border };
    let (fill, stroke) = if on {
        (fade(t.accent), Stroke::NONE)
    } else {
        (Color32::TRANSPARENT, Stroke::new(1.0, fade(border)))
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::ZERO, fill, stroke, StrokeKind::Inside);
    let ink = fade(Color32::WHITE);
    match state {
        TriState::On => {
            painter.line_segment(
                [
                    rect.lerp_inside(egui::vec2(0.2, 0.52)),
                    rect.lerp_inside(egui::vec2(0.42, 0.74)),
                ],
                Stroke::new(2.0, ink),
            );
            painter.line_segment(
                [
                    rect.lerp_inside(egui::vec2(0.42, 0.74)),
                    rect.lerp_inside(egui::vec2(0.8, 0.28)),
                ],
                Stroke::new(2.0, ink),
            );
        }
        TriState::Partial => {
            painter.hline(
                rect.shrink2(egui::vec2(4.0, 0.0)).x_range(),
                rect.center().y,
                Stroke::new(2.0, ink),
            );
        }
        TriState::Off => {}
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, ""));
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::Default)
    } else {
        response
    }
}

/// One of a project's targets, indented under its project row.
#[allow(clippy::too_many_arguments)]
fn target_table_row(
    row: &mut TableRow<'_, '_>,
    project: &Project,
    target: &mut Target,
    last: bool,
    show_items: bool,
    t: &Tokens,
    s: &Strings,
    span: egui::Rangef,
) {
    let dimmed = target.dangerous && !target.checked;
    row.col(|ui| {
        if !last {
            row_rule(ui, t, span);
        }
        ui.add_space(20.0);
        ui.add(Checkbox::without_text(&mut target.checked));
    });
    row.col(|ui| {
        // Risky entries stay visually weak until the user checks them on purpose.
        let color = if dimmed { t.text_muted } else { t.text };
        let rel = target
            .path
            .strip_prefix(&project.path)
            .unwrap_or(&target.path);
        let text = target
            .label
            .clone()
            .unwrap_or_else(|| rel.display().to_string());
        let label = Label::new(RichText::new(text).size(13.0).monospace().color(color)).truncate();
        ui.add_space(24.0);
        ui.add(label)
            .on_hover_text(target.path.display().to_string());
    });
    row.col(|ui| {
        if dimmed {
            ui.label(theme::hud(s.unchecked, t.text_muted));
        }
    });
    if show_items {
        row.col(|_| {});
    }
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let text = match target.size {
                Some(bytes) => size::format_size(bytes),
                None => "…".to_string(),
            };
            let color = if dimmed { t.text_muted } else { t.text };
            ui.label(RichText::new(text).size(13.0).monospace().color(color));
        });
    });
}

/// The hairline under the row `ui` is drawing, across the whole table. It sits on
/// the row's last pixel and keeps the cell's own vertical clip, so a row scrolled
/// under the sticky header draws nothing instead of striping the header.
fn row_rule(ui: &Ui, t: &Tokens, span: egui::Rangef) {
    let y = ui.max_rect().bottom() - 0.5;
    let clip = egui::Rect::from_x_y_ranges(span, ui.clip_rect().y_range());
    ui.ctx()
        .layer_painter(ui.layer_id())
        .with_clip_rect(clip)
        .hline(span, y, t.hairline());
}

fn cache_row(
    ui: &mut Ui,
    row: &mut CacheRow,
    t: &Tokens,
    s: &Strings,
    busy: bool,
) -> Option<&'static CacheRule> {
    let exists = row.path.as_ref().is_some_and(|path| path.is_dir());
    let missing = !exists && !row.cleaned;
    let mut clean = false;

    Frame::new()
        .fill(t.surface)
        .stroke(t.hairline())
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled(
                    exists && !row.cleaned,
                    Checkbox::without_text(&mut row.checked),
                );

                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 200.0).max(120.0));
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(row.rule.name)
                                .size(14.0)
                                .strong()
                                .color(t.text),
                        );
                        // Only the cargo registry carries a risk note today.
                        if row.rule.id == "cargo.registry" {
                            ui.label(RichText::new(s.cache_cargo_note).size(11.0).color(t.danger));
                        }
                    });
                    if let Some(path) = &row.path {
                        let label = Label::new(
                            RichText::new(path.display().to_string())
                                .size(11.0)
                                .monospace()
                                .color(t.text_muted),
                        )
                        .truncate();
                        ui.add(label).on_hover_text(path.display().to_string());
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let button = Button::new(theme::button_text(s.clean, t.text))
                        .fill(t.bg)
                        .stroke(t.hairline());
                    let armed =
                        exists && !row.cleaned && !busy && row.size.is_some() && row.checked;
                    if ui.add_enabled(armed, button).clicked() {
                        clean = true;
                    }
                    ui.add_space(8.0);
                    let (text, color) = if missing {
                        (s.cache_not_found.to_string(), t.danger)
                    } else {
                        match row.size {
                            Some(bytes) => (size::format_size(bytes), t.good),
                            None => ("…".to_string(), t.text_muted),
                        }
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(88.0, 18.0),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.label(RichText::new(text).size(13.0).monospace().color(color));
                        },
                    );
                });
            });
        });

    clean.then_some(row.rule)
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// A hairline badge around a mono label. Painted rather than framed: a `Frame`
/// inside a table cell stretches to the cell's height.
fn chip(ui: &mut Ui, text: &str, t: &Tokens) {
    let format = egui::TextFormat {
        font_id: egui::FontId::new(theme::HUD_SIZE, theme::mono_family()),
        color: t.text_muted,
        extra_letter_spacing: theme::HUD_SPACING,
        ..Default::default()
    };
    let job = egui::text::LayoutJob::single_section(text.to_uppercase(), format);
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(16.0, 6.0), Sense::hover());
    ui.painter()
        .rect_stroke(rect, CornerRadius::ZERO, t.hairline(), StrokeKind::Inside);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, t.text_muted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(dangerous: bool, checked: bool) -> Target {
        Target {
            rule_id: if dangerous { "risky" } else { "safe" }.to_string(),
            path: PathBuf::from("build"),
            size: Some(1024),
            checked,
            dangerous,
            label: None,
        }
    }

    fn entry(targets: Vec<Target>) -> ProjectEntry {
        ProjectEntry {
            targets,
            expanded: false,
        }
    }

    #[test]
    fn the_parent_state_comes_from_the_children() {
        assert_eq!(entry(vec![]).state(), TriState::Off);
        assert_eq!(entry(vec![target(false, false)]).state(), TriState::Off);
        assert_eq!(entry(vec![target(false, true)]).state(), TriState::On);
        assert_eq!(
            entry(vec![target(false, true), target(false, false)]).state(),
            TriState::Partial
        );
    }

    /// Filling a project leaves its risky targets alone, so the parent reads as partial;
    /// a project with nothing safe to clean still fills, and clearing always clears.
    #[test]
    fn filling_skips_risky_targets_but_never_dead_ends() {
        let mut mixed = entry(vec![target(false, false), target(true, false)]);
        mixed.set_checked(true);
        assert_eq!(mixed.state(), TriState::Partial);
        assert!(mixed.targets[0].checked);
        assert!(!mixed.targets[1].checked);

        mixed.set_checked(false);
        assert_eq!(mixed.state(), TriState::Off);

        let mut risky_only = entry(vec![target(true, false)]);
        risky_only.set_checked(true);
        assert_eq!(risky_only.state(), TriState::On);
        assert!(risky_only.targets[0].checked);

        let mut checked_risky = entry(vec![target(false, true), target(true, true)]);
        checked_risky.set_checked(false);
        assert!(checked_risky.targets.iter().all(|target| !target.checked));
    }
}
