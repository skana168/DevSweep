use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use devsweep::cache::{self, CacheRule};
use devsweep::config::{self, Config, LanguageChoice, ThemeChoice};
use devsweep::delete::{self, DeleteEvent, DeleteMode, DeleteReport, Failure, FailureReason};
use devsweep::go_cache::{self, GoCacheContext, GoCacheError, GoCacheSnapshot};
use devsweep::plan::{self, DeletePlan, PlanProject};
use devsweep::rules::{self, Rule, Target};
use devsweep::scan::{self, Project, Stack};
use devsweep::size::{self, SizeEvent, SizeRequest};
use eframe::CreationContext;
use eframe::egui::{
    self, Align, Button, Checkbox, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Label,
    Layout, Modifiers, Response, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui,
};
use egui_extras::{Column, TableBuilder, TableRow};

use crate::i18n::{Lang, Strings};
use crate::terminal;
use crate::theme::{self, Tokens};

/// A window that spent this long in the background gets a fresh scan on focus.
const REFRESH_AFTER: Duration = Duration::from_secs(30);

/// A table row carries a name, a path and its two row actions.
const ROW_H: f32 = 56.0;

/// The height of a segmented cell and of the settings sheet's controls.
const CELL_H: f32 = 34.0;

/// The header band. It is the title bar: on macOS the system's traffic lights sit over it, and
/// everywhere else it draws the window's own buttons.
const HEADER_H: f32 = 48.0;

/// Room macOS's traffic lights take at the left of the header.
const TRAFFIC_LIGHTS_W: f32 = 72.0;

/// How far in from the window's edges a window without decorations resizes.
const RESIZE_EDGE: f32 = 5.0;

/// The search box's width and its inner horizontal padding. The width must fit
/// the longest hint; `every_search_hint_fits_the_box` holds it to that.
const SEARCH_W: f32 = 210.0;
const SEARCH_PAD: i8 = 10;

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
    go_snapshot: Option<GoCacheSnapshot>,
    go_query: Option<Receiver<GoCacheSnapshot>>,
    go_notice: Option<GoCacheError>,
    measurement: Option<Measurement>,
    search: String,
    filter: Option<Stack>,
    sort: Sort,
    /// Previous frame's window focus, for the focus-refresh hook.
    was_focused: Option<bool>,
    last_scan: Instant,
    /// The project whose clean panel is open, with its live draft plan.
    panel: Option<Panel>,
    /// What the open confirm dialog will clean; `None` while it is closed.
    pending: Option<Cleanup>,
    /// The user has ticked "I understand" in the confirm dialog.
    confirm_large_ack: bool,
    execution: Option<Execution>,
    /// Result banner shown after a cleanup.
    flash: Option<Flash>,
    failures_open: bool,
    failures: Vec<Failure>,
    /// The settings sheet is showing.
    settings_open: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Projects,
    Caches,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Size,
}

#[derive(Clone)]
enum Cleanup {
    Project(PathBuf),
    Cache(&'static CacheRule),
    GoCache(&'static CacheRule, GoCacheSnapshot),
}

/// What a row's buttons ask the app to do, applied outside the table borrow.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowAction {
    Panel,
    Folder,
    Terminal,
}

/// An open clean panel: the project plus the plan its current draft would delete.
struct Panel {
    project: PathBuf,
    plan: DeletePlan,
}

struct CacheRow {
    rule: &'static CacheRule,
    path: Option<PathBuf>,
    size: Option<u64>,
    cleaned: bool,
    go_error: Option<GoCacheError>,
}

/// Go rows always show their query state; other rows need a present cache or a clean result.
fn cache_present(row: &CacheRow) -> bool {
    go_cache::is_go_cache(row.rule.id)
        || row.cleaned
        || row.path.as_ref().is_some_and(|path| path.is_dir())
}

struct ProjectEntry {
    targets: Vec<Target>,
}

/// Resets the panel's draft to each target's rule default: risky rules stay off.
fn reset_draft(targets: &mut [Target]) {
    for target in targets {
        target.checked = target.default_checked;
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
    go_cache: bool,
    go_check: Option<Receiver<(GoCacheSnapshot, Option<GoCacheError>)>>,
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
                    rule: entry.rule,
                    path: entry.path,
                    size: None,
                    cleaned: false,
                    go_error: None,
                })
                .collect(),
            go_snapshot: None,
            go_query: None,
            go_notice: None,
            measurement: None,
            search: String::new(),
            filter: None,
            sort: Sort::Name,
            was_focused: None,
            last_scan: Instant::now(),
            panel: None,
            pending: None,
            confirm_large_ack: false,
            execution: None,
            flash: None,
            failures_open: false,
            failures: Vec::new(),
            settings_open: false,
        };
        app.projects = scan::scan(&app.active_roots());
        app.sync_entries();
        app.refresh_go_caches(&cc.egui_ctx);
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
        self.refresh_go_caches(ctx);
        self.measure_missing(ctx);
        self.persist();
        self.last_scan = Instant::now();
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        self.projects = scan::scan(&self.active_roots());
        self.sync_entries();
        self.refresh_go_caches(ctx);
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

    /// Track the draft targets per canonical path; survivors keep them, vanished
    /// projects drop out.
    fn sync_entries(&mut self) {
        scan::retain_scanned(&mut self.entries, &self.projects);
        let rules = &self.rules;
        for project in &self.projects {
            self.entries
                .entry(project.path.clone())
                .or_insert_with(|| ProjectEntry {
                    targets: rules::expand_targets(project, rules),
                });
        }
    }

    fn project_size(&self, index: usize) -> Option<u64> {
        self.entries
            .get(&self.projects[index].path)
            .and_then(|entry| size::total_size(&entry.targets))
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

    fn go_context(&self) -> Option<GoCacheContext> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(GoCacheContext {
            home: PathBuf::from(home),
            scan_roots: self.config.roots.clone(),
            projects: self
                .projects
                .iter()
                .map(|project| project.path.clone())
                .collect(),
            other_caches: self
                .caches
                .iter()
                .filter(|row| !go_cache::is_go_cache(row.rule.id))
                .filter_map(|row| row.path.clone())
                .collect(),
        })
    }

    fn refresh_go_caches(&mut self, ctx: &egui::Context) {
        if matches!(self.pending.as_ref(), Some(Cleanup::GoCache(..))) {
            self.pending = None;
        }
        if let Some(execution) = &mut self.execution
            && execution.go_cache
        {
            execution.abort.store(true, Ordering::Relaxed);
            execution.go_check = None;
        }
        // Replacing the receiver discards results from an older roots/configuration snapshot.
        self.go_query = None;
        self.go_snapshot = None;
        for row in self
            .caches
            .iter_mut()
            .filter(|row| go_cache::is_go_cache(row.rule.id))
        {
            row.path = None;
            row.size = None;
            row.cleaned = false;
            row.go_error = None;
        }
        let Some(context) = self.go_context() else {
            for row in self
                .caches
                .iter_mut()
                .filter(|row| go_cache::is_go_cache(row.rule.id))
            {
                row.go_error = Some(GoCacheError::Unreadable);
            }
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(go_cache::discover(&context));
            repaint.request_repaint();
        });
        self.go_query = Some(rx);
    }

    fn apply_go_snapshot(&mut self, snapshot: GoCacheSnapshot) {
        for row in self
            .caches
            .iter_mut()
            .filter(|row| go_cache::is_go_cache(row.rule.id))
        {
            if let Some(entry) = snapshot.entry(row.rule.id) {
                row.path = entry.path.clone();
                row.size = entry.size.ok();
                row.go_error = entry.size.err();
                row.cleaned = false;
            }
        }
        self.go_snapshot = Some(snapshot);
    }

    fn poll_go_caches(&mut self, ctx: &egui::Context) {
        let Some(query) = &self.go_query else {
            return;
        };
        match query.try_recv() {
            Ok(snapshot) => {
                self.apply_go_snapshot(snapshot);
                self.go_query = None;
                ctx.request_repaint();
            }
            Err(TryRecvError::Disconnected) => {
                self.go_query = None;
                for row in self
                    .caches
                    .iter_mut()
                    .filter(|row| go_cache::is_go_cache(row.rule.id))
                {
                    row.go_error = Some(GoCacheError::QueryFailed);
                }
                ctx.request_repaint();
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
        }
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
            if go_cache::is_go_cache(row.rule.id) {
                return None;
            }
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
                    if let Some(row) = self.caches.iter_mut().find(|row| {
                        !go_cache::is_go_cache(row.rule.id) && row.path.as_ref() == Some(&project)
                    }) && let Some((_, size)) = sizes.first()
                        && !row.cleaned
                    {
                        row.size = Some(*size);
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

    /// The plan the panel's current draft would delete, for one project.
    fn project_plan(&self, path: &Path) -> DeletePlan {
        let Some(project) = self.projects.iter().find(|project| project.path == path) else {
            return empty_plan();
        };
        let Some(entry) = self.entries.get(path) else {
            return empty_plan();
        };
        plan::plan(
            &[PlanProject {
                path: &project.path,
                stack: project.stack,
                targets: &entry.targets,
            }],
            &self.config.roots,
            &self.rules,
        )
    }

    /// Open the clean panel with a fresh draft: each target back to its rule default.
    fn open_panel(&mut self, path: &Path) {
        let Some(entry) = self.entries.get_mut(path) else {
            return;
        };
        reset_draft(&mut entry.targets);
        let plan = self.project_plan(path);
        self.panel = Some(Panel {
            project: path.to_path_buf(),
            plan,
        });
    }

    /// Plan for the cleanup the confirm dialog will run.
    fn plan_for(&self, cleanup: Cleanup) -> DeletePlan {
        match cleanup {
            Cleanup::Project(path) => self.project_plan(&path),
            Cleanup::Cache(rule) => self.cache_plan(rule).unwrap_or_else(empty_plan),
            Cleanup::GoCache(rule, snapshot) => self
                .go_context()
                .and_then(|context| snapshot.plan(rule.id, &context).ok())
                .unwrap_or_else(empty_plan),
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
            .filter(|row| cache_present(row) && row.go_error.is_none())
            .try_fold(0, |sum, row| row.size.map(|size| sum + size))
    }

    /// Execute the current plan on a worker thread. The UI keeps painting while
    /// per-project events stream back.
    fn start_execution(&mut self, ctx: &egui::Context, cleanup: Cleanup) {
        let go_cleanup = match &cleanup {
            Cleanup::GoCache(rule, snapshot) => {
                let Some(context) = self.go_context() else {
                    return;
                };
                Some((rule.id, snapshot.clone(), context))
            }
            _ => None,
        };
        let global_cache = match &cleanup {
            Cleanup::Cache(_) => true,
            Cleanup::GoCache(rule, _) => rule.id == "go.module",
            Cleanup::Project(_) => false,
        };
        let plan = self.plan_for(cleanup);
        if plan.items.is_empty() {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let (check_tx, check_rx) = std::sync::mpsc::channel();
        let abort = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&abort);
        let mode = self.config.delete_mode;
        let is_go = go_cleanup.is_some();
        self.go_notice = None;
        std::thread::spawn(move || {
            if let Some((id, confirmed, context)) = go_cleanup {
                let fresh = go_cache::discover(&context);
                let checked = confirmed.plan_against(id, &fresh, &context);
                let _ = check_tx.send((fresh, checked.as_ref().err().copied()));
                match checked {
                    Ok(plan) => {
                        delete::execute(plan, mode, &flag, tx);
                    }
                    Err(_) => {
                        let path = plan.items[0].target_path.clone();
                        let _ = tx.send(DeleteEvent::Finished(DeleteReport {
                            freed_bytes: 0,
                            failures: vec![Failure {
                                project_path: path.clone(),
                                path,
                                reason: FailureReason::Other,
                            }],
                            projects_done: 0,
                            projects_total: 1,
                            aborted: flag.load(Ordering::Relaxed),
                        }));
                    }
                }
            } else {
                delete::execute(plan, mode, &flag, tx);
            }
        });

        self.execution = Some(Execution {
            done: 0,
            total: 0,
            current: None,
            rx,
            abort,
            go_cache: is_go,
            go_check: is_go.then_some(check_rx),
            global_cache,
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
        let is_go = execution.go_cache;
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
                        reset_draft(&mut entry.targets);
                    }
                    // Its panel has nothing left to show.
                    if self
                        .panel
                        .as_ref()
                        .is_some_and(|panel| panel.project == project)
                    {
                        self.panel = None;
                    }
                    for row in &mut self.caches {
                        if row.path.as_ref().is_some_and(|path| deleted.contains(path)) {
                            row.size = Some(0);
                            row.cleaned = true;
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

        let checked = execution
            .go_check
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some((snapshot, error)) = checked {
            self.apply_go_snapshot(snapshot);
            self.go_notice = error;
            changed = true;
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
            if is_go {
                self.refresh_go_caches(ctx);
            }
            changed = true;
        } else if lost {
            self.execution = None;
            if is_go {
                self.refresh_go_caches(ctx);
            }
        }
        if changed {
            ctx.request_repaint();
        }
        if self.execution.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// The project's clean panel: details, a draft of what to delete, and the two exits.
    fn clean_panel(&mut self, ctx: &egui::Context, t: &Tokens, s: &Strings) {
        let Some(mut panel) = self.panel.take() else {
            return;
        };
        let path = panel.project.clone();
        let Some(project) = self.projects.iter().find(|project| project.path == path) else {
            return;
        };
        let (name, stack) = (project.name.clone(), project.stack);
        let Some(entry) = self.entries.get_mut(&path) else {
            return;
        };
        let targets = &mut entry.targets;
        let chosen = targets.iter().filter(|target| target.checked).count();
        let mut dirty = false;
        let mut close = false;
        let mut clean = false;

        let modal = egui::Modal::new(egui::Id::unique("clean_panel"))
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
                    ui.label(theme::title(&name, 20.0, t.text));
                    ui.add_space(8.0);
                    chip(ui, s.stack(stack), t);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, "×", t) {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                let full = path.display().to_string();
                ui.add(
                    Label::new(
                        RichText::new(&full)
                            .size(12.0)
                            .monospace()
                            .color(t.text_muted),
                    )
                    .truncate(),
                )
                .on_hover_text(&full);
                ui.add_space(12.0);
                stat_row(ui, s.items, &format!("{chosen} / {}", targets.len()), t);
                stat_row(ui, s.total, &size::format_size(panel.plan.total_bytes), t);
                if stack == Stack::Dotnet {
                    ui.label(
                        RichText::new(s.dotnet_clean_note)
                            .size(12.0)
                            .color(t.danger),
                    );
                }
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for target in targets.iter_mut() {
                            // Risky entries stay visually weak until chosen on purpose.
                            let dimmed = target.dangerous && !target.checked;
                            let color = if dimmed { t.text_muted } else { t.text };
                            let rel = target.path.strip_prefix(&path).unwrap_or(&target.path);
                            let text = target
                                .label
                                .clone()
                                .unwrap_or_else(|| rel.display().to_string());
                            let size = target
                                .size
                                .map(size::format_size)
                                .unwrap_or_else(|| "…".to_string());
                            ui.horizontal(|ui| {
                                dirty |= ui
                                    .add(Checkbox::without_text(&mut target.checked))
                                    .changed();
                                ui.add(
                                    Label::new(
                                        RichText::new(text).size(13.0).monospace().color(color),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(target.path.display().to_string());
                                if target.dangerous {
                                    ui.label(theme::hud(s.dangerous, t.danger));
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(size).size(13.0).monospace().color(color),
                                    );
                                });
                            });
                        }

                        if !panel.plan.skipped.is_empty() {
                            ui.add_space(12.0);
                            ui.label(theme::hud(s.skipped, t.danger));
                            for skip in &panel.plan.skipped {
                                let rel = skip.path.strip_prefix(&path).unwrap_or(&skip.path);
                                ui.horizontal(|ui| {
                                    ui.add(
                                        Label::new(
                                            RichText::new(rel.display().to_string())
                                                .size(11.0)
                                                .monospace()
                                                .color(t.text_muted),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(skip.path.display().to_string());
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(s.skip_reason(skip.reason))
                                                .size(11.0)
                                                .color(t.danger),
                                        );
                                    });
                                });
                            }
                        }
                    });

                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let planned = panel.plan.items.len();
                    let label = if planned == 0 {
                        s.clean.to_string()
                    } else {
                        format!(
                            "{} {planned} {} · {}",
                            s.clean,
                            s.items,
                            size::format_size(panel.plan.total_bytes)
                        )
                    };
                    // Unknown sizes could hide a > 5 GB target, so wait for them.
                    let known = targets
                        .iter()
                        .filter(|target| target.checked)
                        .all(|target| target.size.is_some());
                    if danger_button(ui, &label, t, planned > 0 && known) {
                        clean = true;
                    }
                    ui.add_space(4.0);
                    if secondary_button(ui, s.cancel, t) {
                        close = true;
                    }
                });
            });

        // The draft changed: replan now, so the totals match what is on screen.
        if dirty {
            panel.plan = self.project_plan(&path);
        }
        if clean {
            self.pending = Some(Cleanup::Project(path));
            self.confirm_large_ack = false;
        } else if !close && !modal.should_close() {
            self.panel = Some(panel);
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
                        let text =
                            RichText::new(format!("{} ({failure_count})", s.failures)).size(13.0);
                        if text_button(ui, text, t.danger, t.text_strong).clicked() {
                            show_failures = true;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let close = RichText::new("×");
                        if text_button(ui, close, t.text_muted, t.text_strong).clicked() {
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

        let large: Vec<&plan::PlanItem> = plan.items.iter().filter(|item| item.large).collect();
        let total = if known {
            size::format_size(plan.total_bytes)
        } else {
            "…".to_string()
        };
        let title = match cleanup {
            Cleanup::Cache(rule) => format!("{} · {}", s.confirm_title, rule.name),
            Cleanup::GoCache(rule, _) => format!(
                "{} · {}",
                s.confirm_title,
                if rule.id == "go.build" {
                    s.go_build_cache
                } else {
                    s.go_module_cache
                }
            ),
            Cleanup::Project(ref path) => {
                format!("{} · {}", s.confirm_title, project_name(path))
            }
        };
        let cache_warning = match cleanup {
            Cleanup::GoCache(rule, _) if rule.id == "go.module" => Some(s.go_module_note),
            Cleanup::Cache(rule) if rule.dangerous => Some(s.cache_cargo_note),
            Cleanup::GoCache(rule, _) if rule.id == "go.build" => Some(s.go_build_note),
            _ => None,
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
                stat_row(ui, s.items, &plan.items.len().to_string(), t);
                stat_row(ui, s.total, &total, t);

                if let Some(warning) = cache_warning {
                    ui.add_space(12.0);
                    ui.label(RichText::new(warning).size(13.0).color(t.danger));
                }

                if matches!(cleanup, Cleanup::GoCache(..)) {
                    ui.add_space(8.0);
                    ui.label(RichText::new(s.go_stop_tasks).size(12.0).color(t.danger));
                }

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
                    if danger_button(
                        ui,
                        s.confirm,
                        t,
                        known && !plan.items.is_empty() && (large.is_empty() || ack),
                    ) {
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

    /// The header, which is also the window's title bar: the brand at the left after room for
    /// macOS's traffic lights, the settings entry and (elsewhere) the window's buttons at the
    /// right. The empty space in between drags the window.
    fn header(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        title_bar_drag(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let lights = if cfg!(target_os = "macos") {
                TRAFFIC_LIGHTS_W
            } else {
                0.0
            };
            ui.add_space(lights);
            // Painted into a full-height cell so the wordmark sits on the band's centre line, the
            // way the reference's logo does; a bare label would ride at the top of the row.
            let wordmark: egui::WidgetText = theme::title("DevSweep", 22.0, t.text).into();
            let wordmark = wordmark.into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            );
            let (cell, _) = ui.allocate_exact_size(
                egui::vec2(wordmark.size().x + 32.0, HEADER_H),
                Sense::hover(),
            );
            ui.painter().galley(
                egui::pos2(
                    cell.left() + 16.0,
                    cell.center().y - wordmark.size().y / 2.0,
                ),
                wordmark,
                t.text,
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !cfg!(target_os = "macos") {
                    window_buttons(ui, t, s);
                }
                if settings_icon(ui, t, s.settings, self.settings_open) {
                    self.settings_open = true;
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        ui.label(theme::hud(s.roots, t.text_muted));
        ui.add_space(4.0);

        self.roots_editor(ui, t, s);

        ui.add_space(20.0);
        let width = ui.available_width();
        ui.label(theme::hud(s.type_filter, t.text_muted));
        ui.add_space(4.0);
        let filters = std::iter::once((None, s.all)).chain(
            [
                Stack::Flutter,
                Stack::Android,
                Stack::Ios,
                Stack::Java,
                Stack::Node,
                Stack::Python,
                Stack::Rust,
                Stack::Dotnet,
            ]
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
            if nav_button(ui, label, active, egui::vec2(width, 30.0), t).clicked() {
                self.view = View::Projects;
                self.filter = filter;
            }
        }

        ui.add_space(12.0);
        let total = self
            .cache_total()
            .map(size::format_size)
            .unwrap_or_else(|| "…".to_string());
        let label = RichText::new(format!("{}  {total}", s.global_caches)).size(14.0);
        if nav_button(
            ui,
            label,
            self.view == View::Caches,
            egui::vec2(width, 32.0),
            t,
        )
        .clicked()
        {
            self.view = View::Caches;
        }
    }

    /// The scan-root list, shared by the sidebar and the settings sheet.
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
                if text_button(ui, RichText::new("×"), t.text_muted, t.text_strong)
                    .on_hover_text(s.remove)
                    .clicked()
                {
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
        let (rest, hot) = look(Kind::Secondary, t);
        let add = ui
            .scope(|ui| {
                paint_states(ui, rest, hot);
                ui.add_sized(
                    [width, CELL_H],
                    Button::new(theme::button_text(format!("+ {}", s.add_directory))),
                )
            })
            .inner;
        if add.clicked() {
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
        self.refresh_go_caches(ctx);
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

    /// The settings sheet: hero, two columns of hairline panels, a Done footer.
    fn settings_dialog(&mut self, ctx: &egui::Context, t: &Tokens, s: &Strings) {
        if !self.settings_open {
            return;
        }
        // The window less a margin, capped like the reference's large dialog.
        let screen = ctx.content_rect();
        let width = (screen.width() - 64.0).clamp(320.0, 1000.0);
        let height = (screen.height() - 48.0).clamp(240.0, 820.0);
        let mut done = false;

        let modal = egui::Modal::new(egui::Id::unique("settings"))
            .frame(
                Frame::new()
                    .fill(t.bg)
                    .stroke(Stroke::new(1.0, t.border_strong))
                    .shadow(theme::shadow(ctx.theme() == egui::Theme::Dark)),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.set_max_width(width);
                ui.set_max_height(height);
                let hero = sheet_hero(ui, s, t);
                let footer = CELL_H + 32.0;
                let body = (height - hero - footer - 2.0).max(0.0);
                ScrollArea::vertical()
                    .id_salt("settings_body")
                    .max_height(body)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(egui::Margin::same(24))
                            .show(ui, |ui| {
                                // The panel margin, then the two columns.
                                let content = width - 48.0;
                                ui.set_min_width(content);
                                ui.set_max_width(content);
                                let column = ((content - 32.0) / 2.0).floor();
                                ui.horizontal_top(|ui| {
                                    let size = egui::vec2(column, 0.0);
                                    ui.allocate_ui_with_layout(
                                        size,
                                        Layout::top_down(Align::Min),
                                        |ui| self.settings_left(ui, t, s),
                                    );
                                    // The columns are 32 apart; `item_spacing` already adds 8.
                                    ui.add_space(24.0);
                                    ui.allocate_ui_with_layout(
                                        size,
                                        Layout::top_down(Align::Min),
                                        |ui| self.settings_right(ui, t, s),
                                    );
                                });
                            });
                    });
                ui.painter()
                    .hline(ui.max_rect().x_range(), ui.cursor().top(), t.hairline());
                Frame::new()
                    .inner_margin(egui::Margin::symmetric(24, 16))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // One control row: `with_layout` would otherwise take the
                        // remaining height and pad the footer.
                        ui.set_height(CELL_H);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            done = action_button(
                                ui,
                                s.done,
                                Kind::Primary,
                                t,
                                true,
                                egui::vec2(90.0, CELL_H),
                            );
                        });
                    });
            });

        if done || modal.should_close() {
            self.settings_open = false;
        }
    }

    fn settings_left(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        // A full-width dropdown, as in the reference: the list is the panel's body.
        settings_panel(ui, s.language, t, |ui| {
            let mut language = self.config.language;
            // As tall as a cell, so the dropdown sits level with the other controls.
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = CELL_H;
                egui::ComboBox::from_id_salt("settings_language")
                    .selected_text(language_label(language, s))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (choice, label) in language_choices(s) {
                            ui.selectable_value(&mut language, choice, label);
                        }
                    });
            });
            if language != self.config.language {
                self.config.language = language;
                self.lang = Lang::from_choice(language);
                self.persist();
            }
        });

        settings_panel(ui, s.theme, t, |ui| {
            let mut choice = self.config.theme;
            let options = [
                (ThemeChoice::System, s.system),
                (ThemeChoice::Dark, s.dark),
                (ThemeChoice::Light, s.light),
            ];
            if cells(ui, &mut choice, &options, t) {
                self.config.theme = choice;
                theme::apply(ui.ctx(), choice);
                self.persist();
            }
        });

        settings_panel(ui, s.delete_mode, t, |ui| {
            let mut mode = self.config.delete_mode;
            let options = [
                (DeleteMode::Trash, s.delete_trash),
                (DeleteMode::Direct, s.delete_direct),
            ];
            if cells(ui, &mut mode, &options, t) {
                self.config.delete_mode = mode;
                self.persist();
            }
            // The consequence of each choice, as a note rather than a wide cell label.
            ui.add_space(8.0);
            ui.label(RichText::new(s.delete_note).size(12.0).color(t.text_muted));
        });
    }

    fn settings_right(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        settings_panel(ui, s.roots, t, |ui| self.roots_editor(ui, t, s));

        settings_panel(ui, s.rules_file, t, |ui| {
            if let Some(path) = config::rules_path() {
                let full = path.display().to_string();
                ui.add(
                    Label::new(
                        RichText::new(&full)
                            .size(11.0)
                            .monospace()
                            .color(t.text_muted),
                    )
                    .truncate(),
                )
                .on_hover_text(full);
            }
            ui.add_space(8.0);
            if secondary_button(ui, s.open_rules, t) {
                self.open_rules();
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(s.rules_restart)
                    .size(11.0)
                    .color(t.text_muted),
            );
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

    fn content(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings, visible: &[usize]) {
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
                // Taken out so the closure does not hold a second borrow of `self`.
                let mut search = std::mem::take(&mut self.search);
                ui.add(
                    TextEdit::singleline(&mut search)
                        .id(search_id())
                        .hint_text(s.search_hint)
                        .desired_width(SEARCH_W)
                        .min_size(egui::vec2(0.0, 38.0))
                        .margin(egui::Margin::symmetric(SEARCH_PAD, 0))
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

        let mut sort_clicked = None;
        let mut action = None;

        // Rules run across the whole table, padding included.
        let span = ui.available_rect_before_wrap().x_range();
        let sort = self.sort;
        let running = self.execution.is_some();

        Frame::new()
            .stroke(t.hairline())
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(false)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::remainder().at_least(160.0).clip(true))
                    .column(Column::exact(90.0))
                    .column(Column::exact(100.0))
                    .column(Column::exact(108.0))
                    .column(Column::exact(44.0))
                    .header(42.0, |mut header| {
                        header.col(|ui| {
                            if sort_header(ui, s.project_column, "↑", sort == Sort::Name, t) {
                                sort_clicked = Some(Sort::Name);
                            }
                            row_rule(ui, t, span);
                        });
                        header.col(|ui| {
                            ui.label(theme::hud(s.stack_column, t.text_muted));
                        });
                        header.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if sort_header(ui, s.size_column, "↓", sort == Sort::Size, t) {
                                    sort_clicked = Some(Sort::Size);
                                }
                            });
                        });
                        header.col(|_| {});
                        header.col(|_| {});
                    })
                    .body(|mut body| {
                        let projects = &self.projects;
                        let entries = &mut self.entries;
                        let last_index = visible.last().copied();
                        for &index in visible {
                            let project = &projects[index];
                            let Some(entry) = entries.get_mut(&project.path) else {
                                continue;
                            };
                            let last = Some(index) == last_index;
                            body.row(ROW_H, |mut row| {
                                let hit = project_table_row(
                                    &mut row, project, entry, last, running, t, s, span,
                                )
                                .map(|action| (project.path.clone(), action));
                                // Only one row can be clicked per frame; later rows
                                // must not clear an action an earlier row just set.
                                if hit.is_some() {
                                    action = hit;
                                }
                            });
                        }
                    });
            });

        if let Some(key) = sort_clicked {
            self.sort = key;
        }
        match action {
            Some((path, RowAction::Panel)) => self.open_panel(&path),
            Some((path, RowAction::Folder)) => {
                let _ = opener::open(&path);
            }
            Some((path, RowAction::Terminal)) => terminal::open(&path),
            None => {}
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

        if let Some(error) = self.go_notice {
            ui.label(
                RichText::new(s.go_cache_error(error))
                    .size(12.0)
                    .color(t.danger),
            );
            ui.add_space(8.0);
        }
        let mut clean = None;
        let mut action = None;
        let busy = self.execution.is_some();

        // Rules run across the whole table, padding included.
        let span = ui.available_rect_before_wrap().x_range();
        let shown = self.caches.iter().filter(|row| cache_present(row)).count();

        Frame::new()
            .stroke(t.hairline())
            .inner_margin(egui::Margin::symmetric(16, 0))
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(false)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .column(Column::remainder().at_least(160.0).clip(true))
                    .column(Column::exact(110.0))
                    .column(Column::exact(100.0))
                    .column(Column::exact(108.0))
                    .column(Column::exact(44.0))
                    .header(42.0, |mut header| {
                        header.col(|ui| {
                            ui.label(theme::hud(s.cache_column, t.text_muted));
                            row_rule(ui, t, span);
                        });
                        header.col(|ui| {
                            ui.label(theme::hud(s.group_column, t.text_muted));
                        });
                        header.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(theme::hud(s.size_column, t.text_muted));
                            });
                        });
                        header.col(|_| {});
                        header.col(|_| {});
                    })
                    .body(|mut body| {
                        for (index, row) in self
                            .caches
                            .iter()
                            .filter(|row| cache_present(row))
                            .enumerate()
                        {
                            let last = index + 1 == shown;
                            body.row(ROW_H, |mut table_row| {
                                let (event, event_action) =
                                    cache_table_row(&mut table_row, row, last, busy, t, s, span);
                                clean = clean.or(event);
                                if event_action.is_some() {
                                    action = event_action;
                                }
                            });
                        }
                    });
            });

        if let Some(rule) = clean {
            self.go_notice = None;
            self.pending = if go_cache::is_go_cache(rule.id) {
                self.go_snapshot
                    .clone()
                    .map(|snapshot| Cleanup::GoCache(rule, snapshot))
            } else {
                Some(Cleanup::Cache(rule))
            };
            self.confirm_large_ack = false;
        }
        match action {
            Some((path, RowAction::Folder)) => {
                let _ = opener::open(&path);
            }
            Some((path, RowAction::Terminal)) => terminal::open(&path),
            _ => {}
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
        self.poll_go_caches(ctx);
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

        egui::Panel::top("header")
            .exact_size(HEADER_H)
            .resizable(false)
            .frame(Frame::new().fill(t.bg))
            .show(ui, |ui| self.header(ui, t, s));

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
                        let stopped = ui
                            .scope(|ui| {
                                paint_states(
                                    ui,
                                    (Color32::TRANSPARENT, Stroke::NONE, t.text),
                                    (Color32::TRANSPARENT, Stroke::NONE, t.text_strong),
                                );
                                ui.add_enabled(!aborting, button).clicked()
                            })
                            .inner;
                        if stopped {
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
            let focus_search = ui.ctx().input_mut(|input| {
                input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F))
            });
            if focus_search {
                ui.memory_mut(|memory| memory.request_focus(search_id()));
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(t.bg).inner_margin(24))
            .show(ui, |ui| match self.view {
                View::Projects => self.content(ui, t, s, &visible),
                View::Caches => self.caches_content(ui, t, s),
            });

        self.clean_panel(ui.ctx(), t, s);

        if let Some(cleanup) = self.pending.clone() {
            let pending_plan = self.plan_for(cleanup.clone());
            let known = match &cleanup {
                Cleanup::Project(path) => self.entries.get(path).is_some_and(|entry| {
                    entry
                        .targets
                        .iter()
                        .filter(|target| target.checked)
                        .all(|target| target.size.is_some())
                }),
                Cleanup::Cache(_) | Cleanup::GoCache(..) => true,
            };
            self.confirm_dialog(ui.ctx(), t, s, &pending_plan, known, cleanup);
        }
        self.failures_dialog(ui.ctx(), t, s);
        self.settings_dialog(ui.ctx(), t, s);
        if !cfg!(target_os = "macos") {
            window_edges(ui.ctx(), t);
        }
    }

    fn on_exit(&mut self) {
        self.persist();
    }
}

/// How one control paints at rest and under the pointer.
type Look = (Color32, Stroke, Color32);

/// A nav or filter row: the chosen one inverts, the rest lift under the pointer.
fn nav_button(
    ui: &mut Ui,
    label: RichText,
    active: bool,
    size: egui::Vec2,
    t: &Tokens,
) -> Response {
    ui.scope(|ui| {
        paint_states(
            ui,
            (Color32::TRANSPARENT, Stroke::NONE, t.text_muted),
            (t.sunken, Stroke::new(1.0, t.border_strong), t.text_strong),
        );
        // A selected button paints from `selection`, so one row can invert without
        // turning every text selection in the app white.
        let selection = &mut ui.visuals_mut().selection;
        selection.bg_fill = t.invert_bg;
        selection.stroke = Stroke::new(1.0, t.invert_fg);
        ui.add_sized(size, Button::new(label).selected(active))
    })
    .inner
}

/// Set how the buttons drawn inside `ui` look at rest and under the pointer;
/// pressed and focused take the hovered look, as in the reference.
fn paint_states(ui: &mut Ui, rest: Look, hot: Look) {
    let widgets = &mut ui.visuals_mut().widgets;
    for (slot, (fill, stroke, ink)) in [
        (&mut widgets.inactive, rest),
        (&mut widgets.hovered, hot),
        (&mut widgets.active, hot),
        (&mut widgets.open, hot),
    ] {
        slot.bg_fill = fill;
        slot.weak_bg_fill = fill;
        slot.bg_stroke = stroke;
        slot.fg_stroke = Stroke::new(1.0, ink);
    }
}

/// Custom title bars: dragging the header's empty space moves the window, and double-clicking it
/// maximizes or restores it. Call before drawing the header, so its controls stay on top.
fn title_bar_drag(ui: &mut Ui) {
    let r = ui.max_rect();
    let band = resize_band(ui.ctx());
    let rect = egui::Rect::from_min_max(
        r.min + egui::vec2(band, band),
        r.max - egui::vec2(band, 0.0),
    );
    let resp = ui.interact(
        rect,
        ui.scope_id().with("title-bar"),
        Sense::click_and_drag(),
    );
    if resp.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

/// How wide the resize band along the window's edges is right now: nothing while the window is
/// maximized or full screen, when the controls may reach all the way to the corner.
fn resize_band(ctx: &egui::Context) -> f32 {
    let (maximized, fullscreen) = ctx.input(|i| {
        (
            i.viewport().maximized.unwrap_or(false),
            i.viewport().fullscreen.unwrap_or(false),
        )
    });
    if maximized || fullscreen {
        0.0
    } else {
        RESIZE_EDGE
    }
}

/// A window button in a custom title bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Minimize, maximize (or restore) and close, right to left: close ends up at the right.
fn window_buttons(ui: &mut Ui, t: &Tokens, s: &Strings) {
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    if window_button(ui, t, s, WindowButton::Close).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    let size = if maximized {
        WindowButton::Restore
    } else {
        WindowButton::Maximize
    };
    if window_button(ui, t, s, size).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if window_button(ui, t, s, WindowButton::Minimize).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    // The hairline between the window's buttons and the rest of the header.
    let (rule, _) = ui.allocate_exact_size(egui::vec2(1.0, HEADER_H), Sense::hover());
    ui.painter()
        .vline(rule.center().x, rule.y_range(), t.hairline());
}

/// One caption button. Hit-testing leaves the top resize band alone (and, for close, the right
/// edge), so the window's edges stay resizable.
fn window_button(ui: &mut Ui, t: &Tokens, s: &Strings, button: WindowButton) -> Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(46.0, HEADER_H), Sense::hover());
    let band = resize_band(ui.ctx());
    let right = if button == WindowButton::Close {
        band
    } else {
        0.0
    };
    let hit = egui::Rect::from_min_max(
        rect.min + egui::vec2(0.0, band),
        rect.max - egui::vec2(right, 0.0),
    );
    let id = egui::Id::unique(("window-button", button as u8));
    let resp = ui.interact(hit, id, Sense::click());
    let (fill, ink) = match (button, resp.hovered()) {
        (WindowButton::Close, true) => (t.danger, Color32::WHITE),
        (_, true) => (t.border, t.text_strong),
        (_, false) => (Color32::TRANSPARENT, t.text_muted),
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, fill);
    let c = rect.center();
    let stroke = Stroke::new(1.0, ink);
    let label = match button {
        WindowButton::Minimize => s.minimize,
        WindowButton::Maximize => s.maximize,
        WindowButton::Restore => s.restore,
        WindowButton::Close => s.close,
    };
    // Half the glyph's size: 10 px, like the system's own caption buttons.
    let s = 5.0;
    match button {
        WindowButton::Minimize => {
            p.hline(c.x - s..=c.x + s, c.y, stroke);
        }
        WindowButton::Maximize => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(2.0 * s, 2.0 * s)),
                CornerRadius::ZERO,
                stroke,
                StrokeKind::Inside,
            );
        }
        WindowButton::Restore => {
            // A window in front of another.
            let front = egui::Rect::from_min_size(
                c + egui::vec2(-s, -s + 2.0),
                egui::vec2(2.0 * s - 2.0, 2.0 * s - 2.0),
            );
            p.rect_stroke(front, CornerRadius::ZERO, stroke, StrokeKind::Inside);
            p.hline(c.x - s + 2.0..=c.x + s, c.y - s, stroke);
            p.vline(c.x + s, c.y - s..=c.y + s - 2.0, stroke);
        }
        WindowButton::Close => {
            p.line_segment([c + egui::vec2(-s, -s), c + egui::vec2(s, s)], stroke);
            p.line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], stroke);
        }
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    resp.on_hover_text(label)
}

/// The settings entry at the header's right end: the reference's vector gear in a square cell
/// with a hairline on its left, inverted under the pointer and while the sheet is open.
fn settings_icon(ui: &mut Ui, t: &Tokens, label: &str, active: bool) -> bool {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(HEADER_H, HEADER_H), Sense::hover());
    let resp = ui.interact(rect, egui::Id::unique("header-settings"), Sense::click());
    let hot = active || resp.hovered();
    let p = ui.painter();
    if hot {
        p.rect_filled(rect, CornerRadius::ZERO, t.invert_bg);
    }
    p.vline(rect.left() + 0.5, rect.y_range(), t.hairline());
    let ink = if hot { t.invert_fg } else { t.text_muted };
    let c = rect.center();
    let stroke = Stroke::new(1.5, ink);
    p.circle_stroke(c, 3.2, stroke);
    for i in 0..8 {
        let angle = std::f32::consts::TAU * i as f32 / 8.0;
        let (sin, cos) = angle.sin_cos();
        p.line_segment(
            [
                c + egui::vec2(cos, sin) * 4.8,
                c + egui::vec2(cos, sin) * 6.6,
            ],
            stroke,
        );
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    resp.on_hover_text(label)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// A window without decorations: a hairline round it, and resizing from its edges and corners.
fn window_edges(ctx: &egui::Context, t: &Tokens) {
    use egui::CursorIcon as C;
    use egui::viewport::ResizeDirection as D;
    let (maximized, fullscreen) = ctx.input(|i| {
        (
            i.viewport().maximized.unwrap_or(false),
            i.viewport().fullscreen.unwrap_or(false),
        )
    });
    if maximized || fullscreen {
        return;
    }
    let r = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("window-border"),
    ))
    .rect_stroke(r, CornerRadius::ZERO, t.hairline(), StrokeKind::Inside);
    let Some(p) = ctx
        .input(|i| i.pointer.hover_pos())
        .filter(|p| r.contains(*p))
    else {
        return;
    };
    let near = |d: f32| d < RESIZE_EDGE;
    // A corner takes a little of each edge next to it.
    let corner = |d: f32| d < 3.0 * RESIZE_EDGE;
    let (left, right, top, bottom) = (
        p.x - r.left(),
        r.right() - p.x,
        p.y - r.top(),
        r.bottom() - p.y,
    );
    let hit = if (near(top) && corner(left)) || (near(left) && corner(top)) {
        Some((D::NorthWest, C::ResizeNorthWest))
    } else if (near(top) && corner(right)) || (near(right) && corner(top)) {
        Some((D::NorthEast, C::ResizeNorthEast))
    } else if (near(bottom) && corner(left)) || (near(left) && corner(bottom)) {
        Some((D::SouthWest, C::ResizeSouthWest))
    } else if (near(bottom) && corner(right)) || (near(right) && corner(bottom)) {
        Some((D::SouthEast, C::ResizeSouthEast))
    } else if near(top) {
        Some((D::North, C::ResizeNorth))
    } else if near(bottom) {
        Some((D::South, C::ResizeSouth))
    } else if near(left) {
        Some((D::West, C::ResizeWest))
    } else if near(right) {
        Some((D::East, C::ResizeEast))
    } else {
        None
    };
    if let Some((direction, cursor)) = hit {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

/// Which of the reference's button variants.
#[derive(Clone, Copy)]
enum Kind {
    /// A solid block in the inverted colours: the main, non-destructive action.
    Primary,
    /// Outlined at rest, inverted under the pointer: opens something, or cancels.
    Secondary,
    /// Solid red: the click that deletes.
    Danger,
    /// A hairline square, inverted under the pointer: close, overflow.
    Icon,
}

/// The rest and hovered looks of each variant.
fn look(kind: Kind, t: &Tokens) -> (Look, Look) {
    match kind {
        Kind::Primary => (
            (t.invert_bg, Stroke::NONE, t.invert_fg),
            (t.invert_bg.gamma_multiply(0.8), Stroke::NONE, t.invert_fg),
        ),
        Kind::Secondary => (
            (t.bg, Stroke::new(1.0, t.border_strong), t.text),
            (t.invert_bg, Stroke::NONE, t.invert_fg),
        ),
        Kind::Danger => (
            (t.danger, Stroke::NONE, Color32::WHITE),
            (t.danger.gamma_multiply(0.8), Stroke::NONE, Color32::WHITE),
        ),
        Kind::Icon => (
            (Color32::TRANSPARENT, t.hairline(), t.text_muted),
            (t.invert_bg, Stroke::NONE, t.invert_fg),
        ),
    }
}

fn action_button(
    ui: &mut Ui,
    label: &str,
    kind: Kind,
    t: &Tokens,
    enabled: bool,
    min_size: egui::Vec2,
) -> bool {
    let (rest, hot) = look(kind, t);
    ui.scope(|ui| {
        paint_states(ui, rest, hot);
        // `add_enabled` fades the whole button at half strength, the reference's
        // disabled look.
        ui.add_enabled(
            enabled,
            Button::new(theme::button_text(label)).min_size(min_size),
        )
        .clicked()
    })
    .inner
}

/// A square glyph button: a hairline at rest, inverted under the pointer.
fn icon_button(ui: &mut Ui, glyph: &str, t: &Tokens) -> bool {
    let (rest, hot) = look(Kind::Icon, t);
    ui.scope(|ui| {
        paint_states(ui, rest, hot);
        ui.add(Button::new(RichText::new(glyph)).min_size(egui::vec2(CELL_H, CELL_H)))
            .clicked()
    })
    .inner
}

/// A frameless label button: the text lifts under the pointer, nothing else.
fn text_button(ui: &mut Ui, text: RichText, rest: Color32, hot: Color32) -> Response {
    ui.scope(|ui| {
        paint_states(
            ui,
            (Color32::TRANSPARENT, Stroke::NONE, rest),
            (Color32::TRANSPARENT, Stroke::NONE, hot),
        );
        ui.add(Button::new(text).frame(false))
    })
    .inner
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
    text_button(ui, theme::hud(text, color), color, t.text_strong).clicked()
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

/// One project row: name + path, stack, total size, and the two row actions.
///
/// The "last row" flag skips the rule under the final row, where the table's own
/// frame already draws one.
#[allow(clippy::too_many_arguments)]
fn project_table_row(
    row: &mut TableRow<'_, '_>,
    project: &Project,
    entry: &mut ProjectEntry,
    last: bool,
    running: bool,
    t: &Tokens,
    s: &Strings,
    span: egui::Rangef,
) -> Option<RowAction> {
    let mut action = None;
    let cleanable = !entry.targets.is_empty();

    row.col(|ui| {
        if !last {
            row_rule(ui, t, span);
        }
        // The whole cell opens the panel. Labels are selectable text by default and
        // would swallow the click, so they hand it back to the cell.
        let open = ui.interact(
            ui.max_rect(),
            ui.scope_id().with("row-open"),
            Sense::click(),
        );
        ui.vertical(|ui| {
            ui.add(
                Label::new(RichText::new(&project.name).size(15.0).color(t.text)).selectable(false),
            );
            let path = Label::new(
                RichText::new(project.path.display().to_string())
                    .size(12.0)
                    .monospace()
                    .color(t.text_muted),
            )
            .truncate()
            .selectable(false);
            ui.add(path)
                .on_hover_text(project.path.display().to_string());
        });
        if cleanable
            && open
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
        {
            action = Some(RowAction::Panel);
        }
    });
    row.col(|ui| {
        chip(ui, s.stack(project.stack), t);
    });
    row.col(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (text, color) = match size::total_size(&entry.targets) {
                Some(bytes) => (size::format_size(bytes), t.good),
                None => ("…".to_string(), t.text_muted),
            };
            ui.label(RichText::new(text).size(14.0).monospace().color(color));
        });
    });
    row.col(|ui| {
        // Keep the clean button off the size column's right-aligned numbers.
        ui.add_space(12.0);
        // Projects with nothing to clean keep only the folder actions.
        if cleanable
            && action_button(
                ui,
                s.clean,
                Kind::Secondary,
                t,
                !running,
                egui::vec2(84.0, 34.0),
            )
        {
            action = Some(RowAction::Panel);
        }
    });
    row.col(|ui| {
        if let Some(choice) = more_menu(ui, t, s) {
            action = Some(choice);
        }
    });

    action
}

/// The referto-style overflow trigger: a square "…" button with a two-item menu.
fn more_menu(ui: &mut Ui, t: &Tokens, s: &Strings) -> Option<RowAction> {
    let saved = ui.visuals().clone();
    let (rest, hot) = look(Kind::Icon, t);
    let button = Button::new(RichText::new("…").size(15.0)).min_size(egui::vec2(CELL_H, CELL_H));
    let (response, open) = ui
        .scope(|ui| {
            paint_states(ui, rest, hot);
            egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                // The popup inherits this scope's icon colours; its items keep the page ones.
                *ui.visuals_mut() = saved.clone();
                ui.set_min_width(200.0);
                let mut action = None;
                if ui.button(s.open_folder).clicked() {
                    action = Some(RowAction::Folder);
                    ui.close();
                }
                if ui.button(s.open_terminal).clicked() {
                    action = Some(RowAction::Terminal);
                    ui.close();
                }
                action
            })
        })
        .inner;
    response.on_hover_text(s.more);
    open.and_then(|open| open.inner)
}

/// A plan with nothing in it, for a project or cache that vanished mid-click.
fn empty_plan() -> DeletePlan {
    DeletePlan {
        items: Vec::new(),
        skipped: Vec::new(),
        total_bytes: 0,
    }
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

/// One cache row: name + path, group, size, and the two row actions.
///
/// The "last row" flag skips the rule under the final row, where the table's own
/// frame already draws one.
#[allow(clippy::too_many_arguments)]
fn cache_table_row(
    row: &mut TableRow<'_, '_>,
    cache: &CacheRow,
    last: bool,
    busy: bool,
    t: &Tokens,
    s: &Strings,
    span: egui::Rangef,
) -> (Option<&'static CacheRule>, Option<(PathBuf, RowAction)>) {
    let exists = cache.path.as_ref().is_some_and(|path| path.is_dir());
    let mut clean = false;
    let mut action = None;

    row.col(|ui| {
        if !last {
            row_rule(ui, t, span);
        }
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(match cache.rule.id {
                        "go.build" => s.go_build_cache,
                        "go.module" => s.go_module_cache,
                        _ => cache.rule.name,
                    })
                    .size(14.0)
                    .strong()
                    .color(t.text),
                );
                if cache.rule.id == "go.module" {
                    ui.label(RichText::new(s.dangerous).size(11.0).color(t.danger));
                    ui.add(
                        Label::new(RichText::new(s.go_module_note).size(11.0).color(t.danger))
                            .truncate(),
                    )
                    .on_hover_text(s.go_module_note);
                } else if cache.rule.id == "go.build" {
                    ui.label(
                        RichText::new(s.go_build_note)
                            .size(11.0)
                            .color(t.text_muted),
                    );
                } else if cache.rule.id == "cargo.registry" {
                    ui.label(RichText::new(s.cache_cargo_note).size(11.0).color(t.danger));
                }
            });
            if let Some(error) = cache.go_error {
                ui.add(
                    Label::new(
                        RichText::new(s.go_cache_error(error))
                            .size(11.0)
                            .color(t.danger),
                    )
                    .truncate(),
                )
                .on_hover_text(s.go_cache_error(error));
            } else if go_cache::is_go_cache(cache.rule.id) && cache.path.is_none() {
                ui.label(
                    RichText::new(s.go_cache_querying)
                        .size(11.0)
                        .color(t.text_muted),
                );
            }
            if let Some(path) = &cache.path {
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
    });
    row.col(|ui| {
        chip(ui, s.cache_group(cache.rule.group), t);
    });
    row.col(|ui| {
        let (text, color) = match cache.size {
            Some(bytes) => (size::format_size(bytes), t.good),
            None if cache.go_error.is_some() => ("—".to_string(), t.text_muted),
            None => ("…".to_string(), t.text_muted),
        };
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(text).size(13.0).monospace().color(color));
        });
    });
    row.col(|ui| {
        // Keep the clean button off the size column's right-aligned numbers.
        ui.add_space(12.0);
        let armed = (!go_cache::is_go_cache(cache.rule.id)
            || go_cache::cleanup_supported(cache.rule.id))
            && cache.go_error.is_none()
            && exists
            && !cache.cleaned
            && !busy
            && cache.size.is_some();
        if action_button(
            ui,
            s.clean,
            Kind::Secondary,
            t,
            armed,
            egui::vec2(84.0, CELL_H),
        ) {
            clean = true;
        }
    });
    row.col(|ui| {
        // Nothing to open when the entry is not on this machine.
        if let Some(path) = cache
            .path
            .as_ref()
            .filter(|_| exists && cache.go_error.is_none())
            && let Some(choice) = more_menu(ui, t, s)
        {
            action = Some((path.clone(), choice));
        }
    });

    (clean.then_some(cache.rule), action)
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// A small caps label laid out by hand, for painting into an allocated rect.
fn hud_galley(ui: &Ui, text: &str, color: Color32) -> Arc<egui::Galley> {
    let format = egui::TextFormat {
        font_id: egui::FontId::new(theme::HUD_SIZE, theme::mono_family()),
        color,
        extra_letter_spacing: theme::HUD_SPACING,
        ..Default::default()
    };
    ui.painter()
        .layout_job(egui::text::LayoutJob::single_section(
            text.to_uppercase(),
            format,
        ))
}

/// A segmented control: one hairline around the row, a rule between neighbours,
/// and the chosen cell in the inverted colours. Returns whether the value changed.
fn cells<T: Copy + PartialEq>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &'static str)],
    t: &Tokens,
) -> bool {
    let mut changed = false;
    let mut rects = Vec::with_capacity(options.len());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (option, label) in options {
            let on = *value == *option;
            let width = hud_galley(ui, label, t.text).size().x + 24.0;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, CELL_H), Sense::click());
            let (fill, ink) = match (on, response.hovered()) {
                (true, _) => (t.invert_bg, t.invert_fg),
                (false, true) => (t.sunken, t.text_strong),
                (false, false) => (Color32::TRANSPARENT, t.text_muted),
            };
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::ZERO, fill);
            let galley = hud_galley(ui, label, ink);
            painter.galley(
                egui::pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
                galley,
                ink,
            );
            response
                .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, *label));
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                && !on
            {
                *value = *option;
                changed = true;
            }
            rects.push(rect);
        }
    });
    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
        let painter = ui.painter();
        painter.rect_stroke(
            first.union(*last),
            CornerRadius::ZERO,
            t.hairline(),
            StrokeKind::Inside,
        );
        for rect in rects.iter().skip(1) {
            painter.vline(rect.left(), first.top()..=first.bottom(), t.hairline());
        }
    }
    changed
}

/// A group box in the settings sheet: a hairline frame and a small caps heading.
fn settings_panel(ui: &mut Ui, title: &str, t: &Tokens, body: impl FnOnce(&mut Ui)) {
    Frame::new()
        .stroke(t.hairline())
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(theme::hud(title, t.text_muted));
            ui.add_space(8.0);
            body(ui);
        });
    ui.add_space(14.0);
}

/// The sheet's header: the title and version over an accent wash, under a 2 px
/// rule along the card's top edge. Returns its height.
fn sheet_hero(ui: &mut Ui, s: &Strings, t: &Tokens) -> f32 {
    let top = ui.cursor().top();
    let shown = Frame::new()
        .fill(theme::tint(t.accent, 0.08))
        .inner_margin(egui::Margin::symmetric(24, 18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(theme::title(s.settings, 26.0, t.text));
                ui.label(theme::hud(
                    concat!("DevSweep ", env!("CARGO_PKG_VERSION")),
                    t.text_muted,
                ));
            });
        });
    let rect = shown.response.rect;
    let painter = ui.painter();
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 2.0)),
        CornerRadius::ZERO,
        t.accent,
    );
    painter.hline(rect.x_range(), rect.bottom() - 0.5, t.hairline());
    ui.cursor().top() - top
}

fn language_choices(s: &Strings) -> [(LanguageChoice, &'static str); 3] {
    [
        (LanguageChoice::System, s.system),
        (LanguageChoice::Zh, s.lang_zh),
        (LanguageChoice::En, s.lang_en),
    ]
}

fn language_label(choice: LanguageChoice, s: &Strings) -> &'static str {
    language_choices(s)
        .into_iter()
        .find(|(option, _)| *option == choice)
        .map(|(_, label)| label)
        .unwrap_or(s.system)
}

/// A hairline badge around a mono label. Painted rather than framed: a `Frame`
/// inside a table cell stretches to the cell's height.
fn chip(ui: &mut Ui, text: &str, t: &Tokens) {
    let galley = hud_galley(ui, text, t.text_muted);
    let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(16.0, 6.0), Sense::hover());
    ui.painter()
        .rect_stroke(rect, CornerRadius::ZERO, t.hairline(), StrokeKind::Inside);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, t.text_muted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(rule_id: &str, default_checked: bool, dangerous: bool) -> Target {
        Target {
            rule_id: rule_id.to_string(),
            path: PathBuf::from(rule_id),
            size: Some(1024),
            checked: !default_checked,
            default_checked,
            dangerous,
            label: None,
        }
    }

    /// Opening the panel throws away what the last visit chose and restores the defaults.
    #[test]
    fn the_draft_resets_to_the_rule_defaults() {
        let mut targets = vec![
            target("build", true, false),
            target("pods", false, false),
            target("dist", false, true),
        ];

        reset_draft(&mut targets);

        assert!(targets[0].checked);
        assert!(!targets[1].checked);
        assert!(!targets[2].checked, "a risky rule stays off by default");
    }

    /// Missing caches stay out of the list; one that was just cleaned stays.
    #[test]
    fn only_present_or_cleaned_cache_rows_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let rule = &cache::builtin_caches()[0];
        let row = |path: Option<PathBuf>, cleaned: bool| CacheRow {
            rule,
            path,
            size: None,
            cleaned,
            go_error: None,
        };

        assert!(cache_present(&row(Some(dir.path().to_path_buf()), false)));
        assert!(!cache_present(&row(Some(dir.path().join("gone")), false)));
        assert!(!cache_present(&row(None, false)));
        assert!(cache_present(&row(None, true)));
    }

    /// The hint must fit the box's inner width, or egui elides it. English is the
    /// longest today.
    #[test]
    fn every_search_hint_fits_the_box() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut output = ctx.run_ui(raw_input(Vec::new()), |ui| {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let inner = SEARCH_W - 2.0 * f32::from(SEARCH_PAD);
            for lang in [Lang::Zh, Lang::En] {
                let hint = lang.strings().search_hint;
                let width = ui.fonts_mut(|f| {
                    f.layout_no_wrap(hint.to_owned(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                });
                assert!(
                    width <= inner,
                    "{lang:?} hint is {width}px in a {inner}px box: {hint}"
                );
            }
        });
        output.textures_delta.clear();
    }

    // --- A headless click harness for the projects table. ---
    /// One app with `rows` projects, each carrying one cleanable target.
    fn app_with_rows(rows: usize) -> DevSweepApp {
        let root = PathBuf::from("/tmp/devsweep-ui-test");
        let mut projects = Vec::new();
        let mut entries = HashMap::new();
        for index in 0..rows {
            let path = root.join(format!("p{index}"));
            projects.push(Project {
                name: format!("p{index}"),
                path: path.clone(),
                stack: Stack::Node,
            });
            entries.insert(
                path,
                ProjectEntry {
                    targets: vec![target("node_modules", true, false)],
                },
            );
        }
        DevSweepApp {
            lang: Lang::En,
            config: Config {
                roots: vec![root],
                ..Config::default()
            },
            rules: Vec::new(),
            rules_error: None,
            root_notice: None,
            view: View::Projects,
            projects,
            entries,
            caches: Vec::new(),
            go_snapshot: None,
            go_query: None,
            go_notice: None,
            measurement: None,
            search: String::new(),
            filter: None,
            sort: Sort::Name,
            was_focused: None,
            last_scan: Instant::now(),
            panel: None,
            pending: None,
            confirm_large_ack: false,
            execution: None,
            flash: None,
            failures_open: false,
            failures: Vec::new(),
            settings_open: false,
        }
    }

    fn raw_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn input(pos: egui::Pos2, pressed: bool) -> egui::RawInput {
        raw_input(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            },
        ])
    }

    fn draw(app: &mut DevSweepApp, ctx: &egui::Context, input: egui::RawInput) {
        let mut output = ctx.run_ui(input, |ui| {
            let t = theme::tokens(egui::Theme::Dark);
            let s = app.strings();
            let visible = app.visible();
            egui::CentralPanel::default()
                .frame(Frame::new().fill(t.bg).inner_margin(24))
                .show(ui, |ui| app.content(ui, t, s, &visible));
        });
        // Nothing renders these in a test, but epaint panics if they are dropped.
        output.textures_delta.clear();
    }

    /// Press and release at `pos`, the way a real click arrives. One warm-up frame
    /// first: the table's scroll area only has a clipped layout from its second frame.
    fn click_at(app: &mut DevSweepApp, ctx: &egui::Context, pos: egui::Pos2) {
        draw(app, ctx, input(pos, true));
        draw(app, ctx, input(pos, true));
        draw(app, ctx, input(pos, false));
    }

    /// One frame of the header alone, in the panel it draws in.
    fn header_frame(
        app: &mut DevSweepApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let t = theme::tokens(egui::Theme::Dark);
        let s = Lang::En.strings();
        let mut output = ctx.run_ui(raw_input(events), |ui| {
            egui::Panel::top("header")
                .exact_size(HEADER_H)
                .resizable(false)
                .frame(Frame::new().fill(t.bg))
                .show(ui, |ui| app.header(ui, t, s));
        });
        output.textures_delta.clear();
        output
    }

    fn viewport_commands(output: &egui::FullOutput) -> Vec<&egui::ViewportCommand> {
        output
            .viewport_output
            .values()
            .flat_map(|viewport| &viewport.commands)
            .collect()
    }

    /// Hover, press and release at `pos` in the header, returning the release frame.
    fn header_click(
        app: &mut DevSweepApp,
        ctx: &egui::Context,
        pos: egui::Pos2,
    ) -> egui::FullOutput {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        header_frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
        header_frame(app, ctx, vec![egui::Event::PointerMoved(pos), button(true)]);
        header_frame(
            app,
            ctx,
            vec![egui::Event::PointerMoved(pos), button(false)],
        )
    }

    /// The header's settings entry opens the sheet the sidebar item used to.
    #[test]
    fn the_header_opens_settings() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut app = app_with_rows(0);
        header_frame(&mut app, &ctx, Vec::new());
        let cell = ctx
            .read_response(egui::Id::unique("header-settings"))
            .expect("the header draws a settings cell")
            .rect;
        header_click(&mut app, &ctx, cell.center());
        assert!(app.settings_open);
    }

    /// Dragging the header's empty space moves the window, the way a title bar does.
    #[test]
    fn dragging_the_header_moves_the_window() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut app = app_with_rows(0);
        let start = egui::pos2(400.0, HEADER_H / 2.0);
        header_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start)]);
        header_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        let output = header_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start + egui::vec2(20.0, 0.0))],
        );
        let commands = viewport_commands(&output);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::StartDrag)),
            "{commands:?}"
        );
    }

    /// Windows and Linux have no decorations, so the header's close button is the way out.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_close_button_closes_the_window() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut app = app_with_rows(0);
        header_frame(&mut app, &ctx, Vec::new());
        let close = ctx
            .read_response(egui::Id::unique((
                "window-button",
                WindowButton::Close as u8,
            )))
            .expect("the header draws a close button")
            .rect;
        let output = header_click(&mut app, &ctx, close.center());
        let commands = viewport_commands(&output);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Close)),
            "{commands:?}"
        );
    }

    /// Drives the "…" menu: click the trigger, then click at `item`. Returns the
    /// action the menu hands back, which the caller is responsible for not running.
    fn more_menu_action(ctx: &egui::Context, item: egui::Pos2) -> Option<RowAction> {
        let t = theme::tokens(egui::Theme::Dark);
        let s = Lang::En.strings();
        let trigger = egui::pos2(100.0, 100.0);
        let mut got = None;
        let mut frame = |input: egui::RawInput| {
            let mut output = ctx.run_ui(input, |ui| {
                egui::Area::new(egui::Id::unique("menu-test"))
                    .fixed_pos(trigger)
                    .show(ui.ctx(), |ui| {
                        if let Some(action) = more_menu(ui, t, s) {
                            got = Some(action);
                        }
                    });
            });
            output.textures_delta.clear();
        };

        // Two warm-up frames: layers (the Area, then the popup) only join the hit
        // test one frame after they are created.
        frame(raw_input(Vec::new()));
        frame(raw_input(Vec::new()));
        frame(input(trigger + egui::vec2(17.0, 17.0), true));
        frame(input(trigger + egui::vec2(17.0, 17.0), false));
        frame(raw_input(Vec::new()));
        frame(raw_input(Vec::new()));
        frame(input(item, true));
        frame(input(item, false));
        got
    }

    /// Both menu entries must hand their action back to the row.
    #[test]
    fn the_more_menu_returns_the_clicked_action() {
        let mut found = Vec::new();
        for y in [150.0, 162.0, 174.0, 186.0, 198.0, 210.0] {
            for x in [110.0, 140.0, 180.0, 220.0, 260.0] {
                let ctx = egui::Context::default();
                if let Some(action) = more_menu_action(&ctx, egui::pos2(x, y)) {
                    found.push(action);
                }
            }
        }
        assert!(
            found.contains(&RowAction::Folder),
            "no click reached Open folder"
        );
        assert!(
            found.contains(&RowAction::Terminal),
            "no click reached Open in terminal"
        );
    }

    fn expected(index: usize) -> Option<PathBuf> {
        Some(PathBuf::from(format!("/tmp/devsweep-ui-test/p{index}")))
    }

    /// The first row is not the last rendered one: its action must survive the rows
    /// painted after it. The click lands on the path label, which must not eat it.
    #[test]
    fn clicking_a_row_opens_its_panel() {
        let mut app = app_with_rows(3);
        let ctx = egui::Context::default();
        click_at(&mut app, &ctx, egui::pos2(200.0, 150.0));
        assert_eq!(
            app.panel.as_ref().map(|panel| panel.project.clone()),
            expected(0)
        );
    }

    /// The row's `Clean` button has the same plumbing, without any label in the way.
    #[test]
    fn clicking_a_rows_clean_button_opens_its_panel() {
        let mut app = app_with_rows(3);
        let ctx = egui::Context::default();
        click_at(&mut app, &ctx, egui::pos2(735.0, 150.0));
        assert_eq!(
            app.panel.as_ref().map(|panel| panel.project.clone()),
            expected(0)
        );
    }

    /// The row's size counts every target, risky ones included.
    #[test]
    fn the_row_total_ignores_the_draft() {
        let mut targets = vec![target("build", true, false), target("dist", false, true)];
        reset_draft(&mut targets);

        assert_eq!(size::total_size(&targets), Some(2048));
        targets[1].checked = true;
        assert_eq!(size::total_size(&targets), Some(2048));
    }

    /// Every rect fill in the frame, including the ones grouped inside a shape
    /// vector (layers and clip groups nest them).
    fn painted_rects(output: &egui::FullOutput) -> Vec<(egui::Rect, Color32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(egui::Rect, Color32)>) {
            match shape {
                egui::Shape::Rect(rect) => out.push((rect.rect, rect.fill)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut rects = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut rects);
        }
        rects
    }

    /// Every painted label and the colour it was laid out in.
    fn painted_text(output: &egui::FullOutput) -> Vec<(String, Color32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Color32)>) {
            match shape {
                egui::Shape::Text(text) => {
                    let color = text
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(Color32::PLACEHOLDER, |section| section.format.color);
                    let color = if color == Color32::PLACEHOLDER {
                        text.override_text_color.unwrap_or(text.fallback_color)
                    } else {
                        color
                    };
                    out.push((text.galley.job.text.clone(), color));
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts);
        }
        texts
    }

    fn painted_fills(output: &egui::FullOutput) -> Vec<Color32> {
        painted_rects(output)
            .into_iter()
            .map(|(_, fill)| fill)
            .collect()
    }

    /// The chosen row is the inverted block; the others are flat until the pointer
    /// reaches them, then they take the hover well.
    #[test]
    fn the_chosen_nav_row_inverts_and_the_rest_lift_on_hover() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        theme::apply(&ctx, ThemeChoice::Dark);
        let t = theme::tokens(egui::Theme::Dark);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0));
        let other = std::cell::Cell::new(egui::Rect::NOTHING);
        let draw = |pointer: Option<egui::Pos2>| {
            let events = pointer
                .map(|pos| vec![egui::Event::PointerMoved(pos)])
                .unwrap_or_default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    nav_button(
                        ui,
                        RichText::new("Build").size(14.0),
                        true,
                        egui::vec2(120.0, 30.0),
                        t,
                    );
                    let row = nav_button(
                        ui,
                        RichText::new("Other").size(14.0),
                        false,
                        egui::vec2(120.0, 30.0),
                        t,
                    );
                    other.set(row.rect);
                },
            );
            output.textures_delta.clear();
            output
        };

        let idle = painted_fills(&draw(None));
        assert!(idle.contains(&t.invert_bg), "the chosen row must invert");
        assert!(
            !idle.contains(&t.sunken),
            "a row without the pointer must not hover: {idle:?}"
        );

        let hot = draw(Some(other.get().center()));
        let hovered = painted_fills(&hot);
        assert!(
            hovered.contains(&t.sunken),
            "the row under the pointer must lift: {hovered:?}"
        );
        assert!(
            painted_text(&hot)
                .iter()
                .any(|(text, color)| text.starts_with("Other") && *color == t.text_strong),
            "the hovered label must brighten, not only its fill"
        );
    }

    /// Text on an inverted fill must use the inverted ink, not the page ink:
    /// white on white made the chosen row and the primary button unreadable.
    #[test]
    fn inverted_controls_keep_their_text_readable() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        theme::apply(&ctx, ThemeChoice::Dark);
        let t = theme::tokens(egui::Theme::Dark);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            nav_button(
                ui,
                RichText::new("Build").size(14.0),
                true,
                egui::vec2(120.0, 30.0),
                t,
            );
            primary_button(ui, "Done", t, true);
        });
        output.textures_delta.clear();

        let texts = painted_text(&output);
        let ink = |prefix: &str| {
            texts
                .iter()
                .find(|(text, _)| text.starts_with(prefix))
                .map(|(_, color)| *color)
                .unwrap_or_else(|| panic!("{prefix:?} was never painted: {texts:?}"))
        };
        assert_eq!(
            ink("Build"),
            t.invert_fg,
            "the chosen row's label must not match its fill"
        );
        assert_eq!(
            ink("DONE"),
            t.invert_fg,
            "the primary button's label must not match its fill"
        );
    }

    /// The settings sheet lays out its two columns and marks the chosen theme cell
    /// with the inverted block.
    #[test]
    fn the_settings_sheet_marks_the_chosen_theme() {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        // Areas fade in; without the animation the sheet is opaque on its second frame.
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut app = app_with_rows(1);
        app.settings_open = true;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
        let draw = |app: &mut DevSweepApp| {
            let s = app.strings();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| app.settings_dialog(ui.ctx(), theme::tokens(egui::Theme::Dark), s),
            );
            output.textures_delta.clear();
            output
        };
        // A modal's area only paints from the frame after it appears.
        draw(&mut app);
        let output = draw(&mut app);

        let fills = painted_fills(&output);
        assert!(
            fills.contains(&theme::DARK.invert_bg),
            "the chosen theme cell must invert: {fills:?}"
        );

        // The cells and the dropdown are one row each, not one control per line.
        let tall: Vec<egui::Rect> = painted_rects(&output)
            .into_iter()
            .map(|(rect, _)| rect)
            .filter(|rect| (rect.height() - CELL_H).abs() < 0.5)
            .collect();
        let side_by_side = tall.iter().any(|left| {
            tall.iter().any(|right| {
                left != right
                    && (left.top() - right.top()).abs() < 0.5
                    && (left.left() - right.left()).abs() > 0.5
            })
        });
        assert!(
            side_by_side,
            "the cells must share one line, not one per line: {tall:?}"
        );

        // The sheet fits the window, and the footer closes just under its button.
        let rects = painted_rects(&output);
        let card = rects
            .iter()
            .filter(|(rect, fill)| *fill == theme::DARK.bg && rect.width() > 300.0)
            .max_by(|a, b| a.0.width().total_cmp(&b.0.width()))
            .map(|(rect, _)| *rect)
            .expect("the sheet has a card");
        assert!(
            screen.contains_rect(card),
            "the sheet must fit the window, card {card:?}"
        );
        let done = rects
            .iter()
            .filter(|(rect, fill)| *fill == theme::DARK.invert_bg && rect.height() < 40.0)
            .max_by(|a, b| a.0.top().total_cmp(&b.0.top()))
            .map(|(rect, _)| *rect)
            .expect("the footer has a primary button");
        assert!(
            card.bottom() - done.bottom() <= 18.0,
            "the footer must not leave a gap under Done: card {card:?}, button {done:?}"
        );
        assert!(app.settings_open, "the sheet stays open without a close");
    }
}
