use eframe::egui::{self, Color32, Context, Stroke, Theme};

pub struct Tokens {
    pub bg: Color32,
    pub surface: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub danger: Color32,
}

pub const DARK: Tokens = Tokens {
    bg: Color32::from_rgb(0x0F, 0x17, 0x2A),
    surface: Color32::from_rgb(0x1B, 0x23, 0x36),
    border: Color32::from_rgb(0x47, 0x55, 0x69),
    text: Color32::from_rgb(0xF8, 0xFA, 0xFC),
    text_muted: Color32::from_rgb(0x94, 0xA3, 0xB8),
    accent: Color32::from_rgb(0x22, 0xC5, 0x5E),
    danger: Color32::from_rgb(0xEF, 0x44, 0x44),
};

pub const LIGHT: Tokens = Tokens {
    bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface: Color32::from_rgb(0xF1, 0xF5, 0xF9),
    border: Color32::from_rgb(0xCB, 0xD5, 0xE1),
    text: Color32::from_rgb(0x0F, 0x17, 0x2A),
    text_muted: Color32::from_rgb(0x64, 0x74, 0x8B),
    accent: Color32::from_rgb(0x16, 0xA3, 0x4A),
    danger: Color32::from_rgb(0xDC, 0x26, 0x26),
};

pub fn tokens(theme: Theme) -> &'static Tokens {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// Install the token palette for both themes. The active theme still follows the system.
pub fn install(ctx: &Context) {
    for (theme, tokens) in [(Theme::Dark, &DARK), (Theme::Light, &LIGHT)] {
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);

            let visuals = &mut style.visuals;
            visuals.dark_mode = theme == Theme::Dark;
            visuals.panel_fill = tokens.bg;
            visuals.window_fill = tokens.surface;
            visuals.faint_bg_color = tokens.surface;
            visuals.extreme_bg_color = tokens.surface;
            visuals.window_stroke = Stroke::new(1.0, tokens.border);
            visuals.override_text_color = Some(tokens.text);
            visuals.weak_text_color = Some(tokens.text_muted);
            visuals.hyperlink_color = tokens.accent;
            visuals.error_fg_color = tokens.danger;

            visuals.widgets.noninteractive.bg_fill = tokens.surface;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, tokens.border);
            visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, tokens.text);
            visuals.widgets.inactive.bg_fill = tokens.surface;
            visuals.widgets.inactive.weak_bg_fill = tokens.surface;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, tokens.border);
            visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, tokens.text);
            visuals.widgets.hovered.bg_fill = tokens.border;
            visuals.widgets.hovered.weak_bg_fill = tokens.border;
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, tokens.accent);
            visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, tokens.text);
            visuals.widgets.active.bg_fill = tokens.border;
            visuals.widgets.active.weak_bg_fill = tokens.border;
            visuals.widgets.active.bg_stroke = Stroke::new(1.0, tokens.accent);
            visuals.widgets.active.fg_stroke = Stroke::new(1.0, tokens.text);

            visuals.selection.bg_fill = tokens.accent.gamma_multiply(0.35);
            visuals.selection.stroke = Stroke::new(1.0, tokens.text);
        });
    }
}
