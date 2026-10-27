use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use devsweep::rules::{self, Target};
use devsweep::scan::{self, Project};
use devsweep::size::{self, SizeEvent, SizeRequest};
use eframe::CreationContext;
use eframe::egui::{
    self, Align, Button, Checkbox, Color32, CornerRadius, Frame, Label, Layout, RichText, ScrollArea,
    Sense, Stroke, StrokeKind, Ui, UiBuilder,
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

impl DevSweepApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            lang: Lang::from_system(),
            roots: Vec::new(),
            projects: Vec::new(),
            entries: HashMap::new(),
            measurement: None,
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

    fn content(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        if self.roots.is_empty() {
            if self.card(ui, t, s.empty_title, s.empty_body, Some(s.add_directory)) {
                self.add_directory(ui.ctx());
            }
            return;
        }

        ui.label(
            RichText::new(format!("{} · {}", s.projects, self.projects.len()))
                .size(18.0)
                .strong()
                .color(t.text),
        );
        ui.add_space(12.0);

        if self.projects.is_empty() {
            self.card(ui, t, s.no_projects_title, s.no_projects_body, None);
            return;
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let entries = &mut self.entries;
                for project in &self.projects {
                    let Some(entry) = entries.get_mut(&project.path) else {
                        continue;
                    };
                    project_row(ui, project, entry, t, s);
                    if entry.expanded {
                        for target in &mut entry.targets {
                            target_row(ui, project, target, t, s);
                        }
                    }
                    ui.add_space(6.0);
                }
            });
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
                            clicked = primary_button(ui, action, t);
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

        egui::CentralPanel::default()
            .frame(Frame::new().fill(t.bg).inner_margin(24))
            .show(ui, |ui| self.content(ui, t, s));
    }
}

fn primary_button(ui: &mut Ui, label: &str, t: &Tokens) -> bool {
    let text = RichText::new(label)
        .size(15.0)
        .strong()
        .color(Color32::from_rgb(0x0F, 0x17, 0x2A));
    ui.add(
        Button::new(text)
            .fill(t.accent)
            .stroke(Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::vec2(160.0, 38.0)),
    )
    .clicked()
}

fn project_row(ui: &mut Ui, project: &Project, entry: &mut ProjectEntry, t: &Tokens, s: &Strings) {
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
    row.add_enabled(has_targets, Checkbox::without_text(&mut entry.selected));

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

    if response.clicked() && has_targets {
        entry.expanded = !entry.expanded;
    }
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
