use std::path::PathBuf;

use devsweep::scan::{self, Project};
use eframe::CreationContext;
use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, Frame, Label, Layout, RichText, ScrollArea, Stroke,
    Ui,
};

use crate::i18n::{Lang, Strings};
use crate::theme::{self, Tokens};

pub struct DevSweepApp {
    lang: Lang,
    roots: Vec<PathBuf>,
    projects: Vec<Project>,
}

impl DevSweepApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            lang: Lang::from_system(),
            roots: Vec::new(),
            projects: Vec::new(),
        }
    }

    fn strings(&self) -> &'static Strings {
        self.lang.strings()
    }

    fn add_directory(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if !self.roots.contains(&path) {
                self.roots.push(path);
                self.rescan();
            }
        }
    }

    fn rescan(&mut self) {
        self.projects = scan::scan(&self.roots);
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
            self.rescan();
        }

        ui.add_space(8.0);
        let width = ui.available_width();
        let button = Button::new(RichText::new(format!("+ {}", s.add_directory)).size(14.0))
            .fill(t.bg)
            .stroke(Stroke::new(1.0, t.border))
            .corner_radius(8);
        if ui.add_sized([width, 34.0], button).clicked() {
            self.add_directory();
        }
    }

    fn content(&mut self, ui: &mut Ui, t: &Tokens, s: &Strings) {
        if self.roots.is_empty() {
            if self.card(ui, t, s.empty_title, s.empty_body, Some(s.add_directory)) {
                self.add_directory();
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
                for project in &self.projects {
                    project_row(ui, project, t, s);
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
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let t = theme::tokens(ui.ctx().theme());
        let s = self.strings();

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

fn project_row(ui: &mut Ui, project: &Project, t: &Tokens, s: &Strings) {
    Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 96.0).max(120.0));
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
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    chip(ui, s.stack(project.stack), t);
                });
            });
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
