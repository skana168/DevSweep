//! DevSweep's look: the ArtCraft design language from storytold/craft-launcher
//! (Apache-2.0), re-implemented for this app's own window. Hard edges, hairline
//! rules, Archivo display type, Inter body text and Geist Mono labels; Chinese comes
//! from a bundled GB2312 subset of Maple Mono, since none of the Latin faces has it.

use std::sync::Arc;

use devsweep::config::ThemeChoice;
use eframe::egui::epaint::text::VariationCoords;
use eframe::egui::{
    self, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak,
    RichText, Stroke, TextStyle, Theme, Visuals,
};

pub struct Tokens {
    /// Page and card surface.
    pub bg: Color32,
    /// The panel fill; equal to `bg` because separation is drawn with hairlines, not fills.
    pub surface: Color32,
    /// Wells, badges and hovers.
    pub sunken: Color32,
    /// Hairlines.
    pub border: Color32,
    /// Outlined buttons, dialogs and stronger rules.
    pub border_strong: Color32,
    /// Body text.
    pub text: Color32,
    /// Headings, and text lifted by the pointer.
    pub text_strong: Color32,
    /// Secondary text.
    pub text_muted: Color32,
    /// The brand blue, for surfaces.
    pub accent: Color32,
    /// The brand blue, for text and icons.
    pub accent_ink: Color32,
    /// Primary buttons and hover inversion.
    pub invert_bg: Color32,
    pub invert_fg: Color32,
    /// Freed space: the payoff number.
    pub good: Color32,
    pub warn: Color32,
    pub danger: Color32,
}

const fn ink_alpha(rgb: u32, alpha: u8) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgba_unmultiplied_const(r, g, b, alpha)
}

const fn hex(rgb: u32) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

pub const DARK: Tokens = Tokens {
    bg: hex(0x121316),
    surface: hex(0x121316),
    sunken: hex(0x060607),
    border: ink_alpha(0xf2f1ee, 38),
    border_strong: ink_alpha(0xf2f1ee, 102),
    text: hex(0xf2f1ee),
    text_strong: hex(0xffffff),
    text_muted: ink_alpha(0xf2f1ee, 153),
    accent: hex(0x2d81ff),
    accent_ink: hex(0x74aaff),
    invert_bg: hex(0xffffff),
    invert_fg: hex(0x0b0b0c),
    good: hex(0x3fd6b8),
    warn: hex(0xffc14d),
    danger: hex(0xff6259),
};

pub const LIGHT: Tokens = Tokens {
    bg: hex(0xf2f1ee),
    surface: hex(0xf2f1ee),
    sunken: hex(0xe9e8e4),
    border: ink_alpha(0x101014, 41),
    border_strong: ink_alpha(0x101014, 102),
    text: hex(0x101014),
    text_strong: hex(0x000000),
    text_muted: ink_alpha(0x101014, 158),
    accent: hex(0x2d81ff),
    accent_ink: hex(0x1659c4),
    invert_bg: hex(0x101014),
    invert_fg: hex(0xf2f1ee),
    good: hex(0x0a7563),
    warn: hex(0x8a5800),
    danger: hex(0xb3261e),
};

pub fn tokens(theme: Theme) -> &'static Tokens {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

impl Tokens {
    pub fn hairline(&self) -> Stroke {
        Stroke::new(1.0, self.border)
    }
}

/// `color` at `alpha` (0–1), laid over whatever is behind it.
pub fn tint(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha * 255.0) as u8)
}

/// Apply the saved preference; `ThemeChoice::System` follows the OS.
pub fn apply(ctx: &Context, choice: ThemeChoice) {
    let preference = match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
        ThemeChoice::Light => egui::ThemePreference::Light,
    };
    ctx.set_theme(preference);
}

// ---- type ----------------------------------------------------------------------

/// Archivo 620 at 118 % width: titles and the wordmark.
pub fn display_family() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Inter 600.
pub fn semibold_family() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Geist Mono 700.
pub fn mono_bold_family() -> FontFamily {
    FontFamily::Name("mono-bold".into())
}

/// A page or dialog title.
pub fn title(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text.into())
        .size(size)
        .family(display_family())
        .color(color)
}

/// The `.hud-label` metrics: 11 px mono, upper case, 0.15 em apart.
pub const HUD_SIZE: f32 = 11.0;
pub const HUD_SPACING: f32 = HUD_SIZE * 0.15;

/// A `.hud-label`.
pub fn hud(text: impl Into<String>, color: Color32) -> RichText {
    RichText::new(text.into().to_uppercase())
        .size(HUD_SIZE)
        .family(mono())
        .extra_letter_spacing(HUD_SPACING)
        .color(color)
}

/// A button's label: 12 px mono bold, upper case, 0.12 em apart. The colour comes
/// from the button's own state, so it can follow the pointer.
pub fn button_text(text: impl Into<String>) -> RichText {
    RichText::new(text.into().to_uppercase())
        .size(12.0)
        .family(mono_bold_family())
        .extra_letter_spacing(1.4)
}

/// Geist Mono 500: labels drawn by hand rather than through `RichText`.
pub fn mono_family() -> FontFamily {
    mono()
}

/// Geist Mono 500: numbers and paths.
fn mono() -> FontFamily {
    FontFamily::Name("mono".into())
}

fn install_fonts(ctx: &Context) {
    const INTER: &[u8] = include_bytes!("../assets/fonts/Inter-Variable.ttf");
    const ARCHIVO: &[u8] = include_bytes!("../assets/fonts/Archivo-Variable.ttf");
    const GEIST_MONO: &[u8] = include_bytes!("../assets/fonts/GeistMono-Variable.ttf");
    const MAPLE: &[u8] = include_bytes!("../assets/fonts/MapleMono-CN-Regular.ttf");

    let mut fonts = FontDefinitions::default();
    // egui's Latin faces and its small monochrome icon face stay behind ours.
    let fallback: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    // Each face at the weight (and width) the design uses.
    let mut add =
        |name: &str, bytes: &'static [u8], coords: &[([u8; 4], f32)], family: FontFamily| {
            // Fractional-x glyphs smear their stems without hinting, so no subpixel binning.
            let tweak = FontTweak {
                coords: VariationCoords::new(coords.iter().copied()),
                subpixel_binning: Some(false),
                ..Default::default()
            };
            fonts.font_data.insert(
                name.to_owned(),
                Arc::new(FontData::from_static(bytes).tweak(tweak)),
            );
            let mut stack = vec![name.to_owned()];
            stack.extend(fallback.iter().cloned());
            fonts.families.insert(family, stack);
        };
    add(
        "Inter",
        INTER,
        &[(*b"wght", 400.0), (*b"opsz", 14.0)],
        FontFamily::Proportional,
    );
    add(
        "Inter-Medium",
        INTER,
        &[(*b"wght", 500.0), (*b"opsz", 14.0)],
        FontFamily::Name("medium".into()),
    );
    add(
        "Inter-SemiBold",
        INTER,
        &[(*b"wght", 600.0), (*b"opsz", 14.0)],
        semibold_family(),
    );
    add(
        "Archivo-Display",
        ARCHIVO,
        &[(*b"wght", 620.0), (*b"wdth", 118.0)],
        display_family(),
    );
    add(
        "GeistMono",
        GEIST_MONO,
        &[(*b"wght", 400.0)],
        FontFamily::Monospace,
    );
    add("GeistMono-Medium", GEIST_MONO, &[(*b"wght", 500.0)], mono());
    add(
        "GeistMono-Bold",
        GEIST_MONO,
        &[(*b"wght", 700.0)],
        mono_bold_family(),
    );

    // Chinese, behind every family: the UI defaults to it and paths are user data.
    let tweak = FontTweak {
        subpixel_binning: Some(false),
        ..Default::default()
    };
    fonts.font_data.insert(
        "MapleMono-CN".to_owned(),
        Arc::new(FontData::from_static(MAPLE).tweak(tweak)),
    );
    for stack in fonts.families.values_mut() {
        stack.push("MapleMono-CN".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn visuals(theme: Theme, t: &Tokens) -> Visuals {
    let mut v = if theme == Theme::Dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = t.bg;
    v.window_fill = t.bg;
    v.extreme_bg_color = t.sunken;
    v.text_edit_bg_color = Some(t.sunken);
    v.faint_bg_color = t.sunken;
    v.code_bg_color = t.sunken;
    v.window_stroke = Stroke::new(1.0, t.border_strong);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = shadow(theme == Theme::Dark);
    v.popup_shadow = shadow(theme == Theme::Dark);
    // No `override_text_color`: it outranks a widget's own state colour, and the
    // selected/inverted controls need their text to follow the state.
    v.hyperlink_color = t.accent_ink;
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.warn_fg_color = t.warn;
    v.error_fg_color = t.danger;
    for (w, fill, stroke, fg) in [
        // Body text lives here: plain labels take the non-interactive colour.
        (&mut v.widgets.noninteractive, t.bg, t.border, t.text),
        (&mut v.widgets.inactive, t.bg, t.border, t.text),
        (
            &mut v.widgets.hovered,
            t.sunken,
            t.border_strong,
            t.text_strong,
        ),
        (
            &mut v.widgets.active,
            t.sunken,
            t.border_strong,
            t.text_strong,
        ),
        (
            &mut v.widgets.open,
            t.sunken,
            t.border_strong,
            t.text_strong,
        ),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.corner_radius = CornerRadius::ZERO;
        w.fg_stroke = Stroke::new(1.0, fg);
        w.expansion = 0.0;
    }
    v
}

/// The shadow under menus and dialogs.
pub fn shadow(dark: bool) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 140 } else { 36 }),
    }
}

/// Install the fonts and the token palette for both themes; [`apply`] picks which is used.
pub fn install(ctx: &Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Dark, visuals(Theme::Dark, &DARK));
    ctx.set_visuals_of(Theme::Light, visuals(Theme::Light, &LIGHT));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        // Between a checkbox and its label.
        style.spacing.icon_spacing = 10.0;
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(24.0, display_family())),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, semibold_family())),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every family ends at the Chinese face, and that face really has every hanzi the UI can
    /// show: the default language is Chinese, the Latin faces carry none of it, and the bundled
    /// face is a GB2312 subset rather than the full font.
    #[test]
    fn every_family_can_draw_the_ui_chinese() {
        let ctx = Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        let hanzi: String = include_str!("i18n.rs")
            .chars()
            .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
            .collect();
        assert!(hanzi.chars().count() > 100, "the fixture lost its Chinese");
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            display_family(),
            semibold_family(),
            mono(),
            mono_bold_family(),
        ] {
            ctx.fonts_mut(|fonts| {
                assert!(
                    fonts.has_glyphs(&FontId::new(14.0, family.clone()), &hanzi),
                    "{family:?} cannot draw the UI's Chinese"
                );
            });
        }
    }
}
