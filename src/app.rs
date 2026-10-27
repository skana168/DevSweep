use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use devsweep::delete::{self, DeleteEvent, DeleteMode, Failure};
use devsweep::plan::{self, DeletePlan, LARGE_TARGET_BYTES, PlanProject};
use devsweep::rules::{self, Target};
use devsweep::scan::{self, Project};
use devsweep::size::{self, SizeEvent, SizeRequest};
use eframe::CreationContext;
use eframe::egui::{
    self, Align, Button, Checkbox, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Label, Layout,
    Modifiers, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder,
};

use crate::i18n::{Lang, Strings};
use crate::theme::{self, Tokens};

pub struct DevSweepApp {
    lang: Lang,
    roots: Vec<PathBuf>,
    projects: Vec<Project>,
    /// Expanded targets and per-row UI state, keyed by the project's canonical path.
    entries: HashMap<PathBuf, ProjectEntry>,
    measurement: Option<Measurement>,
    /// Last project whose checkbox was clicked; Shift extends from here.
    selection_anchor: Option<PathBuf>,
    preview_open: bool,
    confirm_open: bool,
    /// The user has ticked "I understand" in the confirm dialog.
    confirm_large_ack: bool,
    execution: Option<Execution>,
    /// Result banner shown after a cleanup.
    flash: Option<Flash>,
    failures_open: bool,
    failures: Vec<Failure>,
}

struct ProjectEntry {
    targets: Vec<Target>,
    expanded: bool,
    selected: bool,
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
}

struct Flash {
    bytes: u64,
    aborted: bool,
}

impl DevSweepApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            lang: Lang::from_system(),
            roots: Vec::new(),
            projects: Vec::new(),
            entries: HashMap::new(),
            measurement: None,
            selection_anchor: None,
            preview_open: false,
            confirm_open: false,
            confirm_large_ack: false,
            execution: None,
            flash: None,
            failures_open: false,
            failures: Vec::new(),
        }
    }

    fn strings(&self) -> &'static Strings {
        self.lang.strings()
    }

    fn add_directory(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if !self.roots.contains(&path) {
                self.roots.push(path);
                self.rescan(ctx);
            }
        }
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        self.projects = scan::scan(&self.roots);
        let rules = rules::builtin_rules();
        // Known projects keep their expanded/checked state; vanished ones drop out.
        self.entries
            .retain(|path, _| self.projects.iter().any(|project| &project.path == path));
        for project in &self.projects {
            self.entries.entry(project.path.clone()).or_insert_with(|| ProjectEntry {
                targets: rules::expand_targets(project, rules),
                expanded: false,
                selected: false,
            });
        }
        self.measure_missing(ctx);
    }

    /// Sizes already computed for a target survive rescans; only unmeasured targets queue up.
    fn measure_missing(&mut self, ctx: &egui::Context) {
        let requests: Vec<SizeRequest> = self
            .projects
            .iter()
            .filter_map(|project| {
                let entry = self.entries.get(&project.path)?;
                let pending: Vec<Target> = entry
                    .targets
                    .iter()
                    .filter(|target| target.size.is_none())
                    .cloned()
                    .collect();
                (!pending.is_empty()).then(|| SizeRequest {
                    project: project.path.clone(),
                    targets: pending,
                })
            })
            .collect();

        if requests.is_empty() {
            self.measurement = None;
            return;
        }

        let total = requests.len();
        let (tx, rx) = std::sync::mpsc::channel();
        size::measure(requests, tx);
        self.measurement = Some(Measurement {
            done: 0,
            total,
            rx,
        });
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
                        for (path, size) in sizes {
                            if let Some(target) =
                                entry.targets.iter_mut().find(|target| target.path == path)
                            {
                                target.size = Some(size);
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
            .filter(|entry| entry.selected)
            .all(|entry| size::checked_size(&entry.targets).is_some())
    }

    /// Shift extends the selection from the anchor, in list order.
    fn toggle_selection(&mut self, index: usize, extend: bool) {
        let path = self.projects[index].path.clone();
        // The checkbox already flipped this row; use the new value for the range.
        let Some(selected) = self.entries.get(&path).map(|entry| entry.selected) else {
            return;
        };

        if extend {
            if let Some(anchor) = self
                .selection_anchor
                .as_ref()
                .and_then(|anchor| self.projects.iter().position(|p| &p.path == anchor))
            {
                let (from, to) = (anchor.min(index), anchor.max(index));
                for project in &self.projects[from..=to] {
                    if let Some(entry) = self.entries.get_mut(&project.path) {
                        if !entry.targets.is_empty() {
                            entry.selected = selected;
                        }
                    }
                }
                return;
            }
        }

        if let Some(entry) = self.entries.get_mut(&path) {
            entry.selected = selected;
        }
        self.selection_anchor = Some(path);
    }

    /// ⌘/Ctrl+A: select every project when not all are selected, otherwise clear.
    fn toggle_select_all(&mut self) {
        let selectable: Vec<PathBuf> = self
            .projects
            .iter()
            .filter(|project| {
                self.entries
                    .get(&project.path)
                    .is_some_and(|entry| !entry.targets.is_empty())
            })
            .map(|project| project.path.clone())
            .collect();
        let all_selected = !selectable.is_empty()
            && selectable
                .iter()
                .all(|path| self.entries.get(path).is_some_and(|entry| entry.selected));
        for path in selectable {
            if let Some(entry) = self.entries.get_mut(&path) {
                entry.selected = !all_selected;
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
                entry.selected.then_some(PlanProject {
                    path: &project.path,
                    stack: project.stack,
                    targets: &entry.targets,
                })
            })
            .collect();
        plan::plan(&projects, &self.roots, rules::builtin_rules())
    }

    /// Execute the current plan on a worker thread. The UI keeps painting while
    /// per-project events stream back.
    fn start_execution(&mut self, ctx: &egui::Context) {
        let plan = self.build_plan();
        if plan.items.is_empty() {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let abort = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&abort);
        // `DeleteMode::Trash` is the default; ticket 08 reads the mode from config.
        std::thread::spawn(move || {
            delete::execute(plan, DeleteMode::Trash, &flag, tx);
        });

        self.execution = Some(Execution {
            done: 0,
            total: 0,
            current: None,
            rx,
            abort,
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
                        entry.targets.retain(|target| !deleted.contains(&target.path));
                        entry.selected = false;
                        entry.expanded = false;
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

    fn preview_panel(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings, plan: &DeletePlan) {
        if !self.preview_open {
            return;
        }
        let mut close = false;

        egui::Panel::right("preview")
            .exact_size(360.0)
            .resizable(false)
            .frame(Frame::new().fill(t.surface).inner_margin(16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(s.preview).size(16.0).strong().color(t.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let button =
                            Button::new(RichText::new("×").color(t.text_muted)).frame(false);
                        if ui.add(button).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        s.preview_total,
                        size::format_size(plan.total_bytes)
                    ))
                    .size(13.0)
                    .color(t.text_muted),
                );
                ui.add_space(8.0);

                ScrollArea::vertical()
                    .auto_shrink([false, false])
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
                            ui.label(
                                RichText::new(s.skipped).size(13.0).strong().color(t.danger),
                            );
                            for skip in &plan.skipped {
                                let label = Label::new(
                                    RichText::new(skip.path.display().to_string())
                                        .size(11.0)
                                        .monospace()
                                        .color(t.text_muted),
                                )
                                .truncate();
                                ui.add(label)
                                    .on_hover_text(skip.path.display().to_string());
                                ui.label(
                                    RichText::new(s.skip_reason(skip.reason))
                                        .size(11.0)
                                        .color(t.danger),
                                );
                            }
                        }
                    });
            });

        if close {
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
        let failure_count = self.failures.len();
        let mut dismiss = false;
        let mut show_failures = false;

        Frame::new()
            .fill(t.accent.gamma_multiply(0.12))
            .stroke(Stroke::new(1.0, t.accent))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(14.0).strong().color(t.accent));
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

    fn confirm_dialog(&mut self, ctx: &egui::Context, t: &Tokens, s: &Strings, plan: &DeletePlan) {
        if !self.confirm_open {
            return;
        }

        let projects = plan
            .items
            .iter()
            .map(|item| &item.project_path)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let large: Vec<&plan::PlanItem> = plan.items.iter().filter(|item| item.large).collect();
        let known = self.selected_sizes_known();
        let total = if known {
            size::format_size(plan.total_bytes)
        } else {
            "…".to_string()
        };
        let mut ack = self.confirm_large_ack;
        let mut close = false;
        let mut confirm = false;

        let modal = egui::Modal::new(egui::Id::unique("confirm_delete"))
            .frame(
                Frame::new()
                    .fill(t.surface)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(16)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(
                    RichText::new(s.confirm_title)
                        .size(18.0)
                        .strong()
                        .color(t.text),
                );
                ui.add_space(12.0);
                stat_row(ui, s.projects, &projects.to_string(), t);
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
                    if primary_button(ui, s.confirm, t, known && (large.is_empty() || ack)) {
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
            self.confirm_open = false;
            self.start_execution(ctx);
        } else if close || modal.backdrop_response.clicked() {
            self.confirm_open = false;
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
                    .fill(t.surface)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(16)
                    .inner_margin(24),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(
                    RichText::new(format!("{} ({})", s.failures, self.failures.len()))
                        .size(18.0)
                        .strong()
                        .color(t.danger),
                );
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
        ui.label(RichText::new("DevSweep").size(20.0).strong().color(t.text));
        ui.add_space(16.0);
        ui.label(RichText::new(s.roots.to_uppercase()).size(11.0).color(t.text_muted));
        ui.add_space(4.0);

        let mut remove = None;
        for (index, root) in self.roots.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string());
                let name_width = (ui.available_width() - 24.0).max(40.0);
                let label = Label::new(RichText::new(name).size(14.0).color(t.text)).truncate();
                ui.add_sized([name_width, 18.0], label)
                    .on_hover_text(root.display().to_string());
                let response =
                    ui.add(Button::new(RichText::new("×").color(t.text_muted)).frame(false));
                if response.on_hover_text(s.remove).clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            self.roots.remove(index);
            self.rescan(ui.ctx());
        }

        ui.add_space(8.0);
        let width = ui.available_width();
        let button = Button::new(RichText::new(format!("+ {}", s.add_directory)).size(14.0))
            .fill(t.bg)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(8);
        if ui.add_sized([width, 34.0], button).clicked() {
            self.add_directory(ui.ctx());
        }
    }

    fn content(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings, plan: &DeletePlan) {
        if self.roots.is_empty() {
            if self.card(ui, t, s.empty_title, s.empty_body, Some(s.add_directory)) {
                self.add_directory(ui.ctx());
            }
            return;
        }

        self.flash_banner(ui, t, s);

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} · {}", s.projects, self.projects.len()))
                    .size(18.0)
                    .strong()
                    .color(t.text),
            );
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
                // Execution arrives with ticket 05; the button already reflects the plan.
                let running = self.execution.is_some();
                if primary_button(ui, &label, t, count > 0 && !running) {
                    self.confirm_open = true;
                    self.confirm_large_ack = false;
                }

                ui.add_space(4.0);
                let button = Button::new(RichText::new(s.preview).size(14.0))
                    .fill(t.bg)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(8)
                    .min_size(egui::vec2(96.0, 38.0));
                if ui.add_enabled(count > 0, button).clicked() {
                    self.preview_open = true;
                }
            });
        });
        ui.add_space(12.0);

        if self.projects.is_empty() {
            self.card(ui, t, s.no_projects_title, s.no_projects_body, None);
            return;
        }

        let mut toggled = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let entries = &mut self.entries;
                for (index, project) in self.projects.iter().enumerate() {
                    let Some(entry) = entries.get_mut(&project.path) else {
                        continue;
                    };
                    if project_row(ui, project, entry, t, s) {
                        toggled = Some(index);
                    }
                    if entry.expanded {
                        for target in &mut entry.targets {
                            target_row(ui, project, target, t, s);
                        }
                    }
                    ui.add_space(6.0);
                }
            });
        if let Some(index) = toggled {
            let extend = ui.input(|input| input.modifiers.shift);
            self.toggle_selection(index, extend);
        }
    }

    fn card(
        &self,
        ui: &mut Ui,
        t: &Tokens,
        title: &str,
        body: &str,
        action: Option<&str>,
    ) -> bool {
        let mut clicked = false;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.18).clamp(24.0, 140.0));
            Frame::new()
                .fill(t.surface)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(16)
                .inner_margin(32)
                .show(ui, |ui| {
                    ui.set_width(440.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(title).size(20.0).strong().color(t.text));
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
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let t = theme::tokens(ui.ctx().theme());
        let s = self.strings();

        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t.surface).inner_margin(egui::Margin::symmetric(16, 4)))
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
                        ui.label(
                            RichText::new(concat!("DevSweep ", env!("CARGO_PKG_VERSION")))
                                .size(11.0)
                                .color(t.text_muted),
                        );
                    });
                });
            });

        egui::Panel::left("scan_roots")
            .exact_size(220.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t.surface).inner_margin(16))
            .show(ui, |ui| self.sidebar(ui, t, s));

        let select_all = ui.ctx().input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::A))
        });
        if select_all {
            self.toggle_select_all();
        }

        let plan = self.build_plan();
        self.preview_panel(ui, t, s, &plan);

        egui::CentralPanel::default()
            .frame(Frame::new().fill(t.bg).inner_margin(24))
            .show(ui, |ui| self.content(ui, t, s, &plan));

        self.confirm_dialog(ui.ctx(), t, s, &plan);
        self.failures_dialog(ui.ctx(), t, s);
    }
}

fn secondary_button(ui: &mut Ui, label: &str, t: &Tokens) -> bool {
    ui.add(
        Button::new(RichText::new(label).size(14.0))
            .fill(t.bg)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(8)
            .min_size(egui::vec2(88.0, 34.0)),
    )
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
    let text = RichText::new(label)
        .size(15.0)
        .strong()
        .color(Color32::from_rgb(0x0F, 0x17, 0x2A));
    ui.add_enabled(
        enabled,
        Button::new(text)
            .fill(t.accent)
            .stroke(Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::vec2(160.0, 38.0)),
    )
    .clicked()
}

fn project_row(
    ui: &mut Ui,
    project: &Project,
    entry: &mut ProjectEntry,
    t: &Tokens,
    s: &Strings,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(10),
        t.surface,
        Stroke::new(1.0, t.border),
        StrokeKind::Inside,
    );

    // The row senses clicks first so the checkbox, added below, wins the hit test.
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(16.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );

    let has_targets = !entry.targets.is_empty();
    let checkbox = row.add_enabled(has_targets, Checkbox::without_text(&mut entry.selected));

    row.vertical(|ui| {
        ui.set_max_width((ui.available_width() - 200.0).max(120.0));
        ui.label(RichText::new(&project.name).size(15.0).strong().color(t.text));
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

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let (text, color) = match size::checked_size(&entry.targets) {
            Some(bytes) => (size::format_size(bytes), t.accent),
            None => ("…".to_string(), t.text_muted),
        };
        // Fixed width keeps rows from shifting when the number arrives.
        ui.allocate_ui_with_layout(
            egui::vec2(96.0, 18.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.label(RichText::new(text).size(14.0).monospace().color(color));
            },
        );
        ui.add_space(8.0);
        chip(ui, s.stack(project.stack), t);
        if !has_targets {
            ui.label(RichText::new(s.no_targets).size(12.0).color(t.text_muted));
        }
    });

    if response.clicked() && has_targets && !checkbox.clicked() {
        entry.expanded = !entry.expanded;
    }

    checkbox.clicked()
}

fn target_row(ui: &mut Ui, project: &Project, target: &mut Target, t: &Tokens, s: &Strings) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), Sense::hover());
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                rect.min + egui::vec2(44.0, 0.0),
                rect.max - egui::vec2(16.0, 0.0),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );

    row.add(Checkbox::without_text(&mut target.checked));

    // Risky entries stay visually weak until the user checks them on purpose.
    let dimmed = target.dangerous && !target.checked;
    let color = if dimmed { t.text_muted } else { t.text };
    let rel = target.path.strip_prefix(&project.path).unwrap_or(&target.path);
    let label = Label::new(
        RichText::new(rel.display().to_string())
            .size(13.0)
            .monospace()
            .color(color),
    )
    .truncate();
    row.add(label)
        .on_hover_text(target.path.display().to_string());

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let text = match target.size {
            Some(bytes) => size::format_size(bytes),
            None => "…".to_string(),
        };
        ui.label(RichText::new(text).size(13.0).monospace().color(color));
        if dimmed {
            ui.label(RichText::new(s.unchecked).size(11.0).color(t.text_muted));
        }
    });
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn chip(ui: &mut Ui, text: &str, t: &Tokens) {
    Frame::new()
        .fill(t.accent.gamma_multiply(0.15))
        .stroke(Stroke::new(1.0, t.accent))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(t.accent));
        });
}
