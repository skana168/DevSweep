//! Product palette and geometry, measured from the supplied reference window.
use gpui_kit::{
    component::{Theme, ThemeMode},
    *,
};
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct Palette {
    pub background: Hsla,
    pub sidebar: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub selection: Hsla,
    pub category: Hsla,
    pub low_risk_background: Hsla,
    pub low_risk_foreground: Hsla,
}
pub const SIDEBAR: f32 = 274.;
pub const CATEGORIES: f32 = 349.;
pub const ROW: f32 = 59.;
pub fn palette() -> Palette {
    Palette {
        background: rgb(0xfff9f0).into(),
        sidebar: rgb(0xfff3e1).into(),
        surface: rgb(0xfffefd).into(),
        border: rgb(0xeee2d3).into(),
        foreground: rgb(0x352f28).into(),
        muted: rgb(0x897b6b).into(),
        accent: rgb(0xc45a00).into(),
        selection: rgb(0xffe1bc).into(),
        category: rgb(0xfaeee1).into(),
        low_risk_background: rgb(0xe9efdf).into(),
        low_risk_foreground: rgb(0x536d2b).into(),
    }
}
pub fn install(cx: &mut App) {
    Theme::change(ThemeMode::Light, None, cx);
    let p = palette();
    Theme::update(cx, |theme| {
        theme.colors.background = p.background;
        theme.colors.foreground = p.foreground;
        theme.colors.primary = p.accent;
        theme.colors.primary_foreground = gpui_kit::white();
        theme.colors.border = p.border;
        theme.colors.input = p.border;
        theme.colors.muted_foreground = p.muted;
        theme.radius = px(10.);
        theme.radius_lg = px(16.);
        theme.colors.button_primary = p.accent;
        theme.colors.button_primary_hover = p.accent;
        theme.colors.button_primary_active = p.accent;
        theme.colors.button_primary_foreground = gpui_kit::white();
    });
    cx.text_system()
        .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
            "../assets/ui-icons.ttf"
        ))])
        .expect("load Material Symbols from egui_icons");
}
pub fn icon(glyph: &'static str, size: f32) -> Div {
    div()
        .font_family("Material Symbols Rounded")
        .text_size(px(size))
        .w(px(size + 3.))
        .flex_shrink_0()
        .child(glyph)
}
pub fn text(value: impl Into<SharedString>, size: f32) -> Div {
    div().text_size(px(size)).child(value.into())
}
pub fn row() -> Div {
    div().flex().items_center()
}
pub fn column() -> Div {
    div().flex().flex_col()
}

// Brand glyph outlines are exported from egui_icons' bundled MDI font.
// Godot has no suitable glyph in that collection; its upstream logo is used.
pub fn brand(id: &str) -> AnyElement {
    if id == "godot" {
        static IMAGE: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
        let image = IMAGE.get_or_init(|| {
            std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../assets/brands/godot.svg").to_vec(),
            ))
        });
        return img(image.clone()).size(px(21.)).into_any_element();
    }
    let (data, color): (&[u8], u32) = match id {
        "npm" => (include_bytes!("../assets/brands/npm-square.svg"), 0xcb2431),
        "vscode" => (include_bytes!("../assets/brands/vscode.svg"), 0x298bd0),
        "openai" => (include_bytes!("../assets/brands/openai.svg"), 0x827466),
        "nuget" => (include_bytes!("../assets/brands/nuget.svg"), 0x827466),
        "rust" => (include_bytes!("../assets/brands/rust.svg"), 0x171717),
        "node" => (include_bytes!("../assets/brands/node.svg"), 0x6db34a),
        "gradle" => (include_bytes!("../assets/brands/gradle.svg"), 0x003b40),
        "godot" => (include_bytes!("../assets/brands/godot.svg"), 0x478cbf),
        "swift" => (include_bytes!("../assets/brands/swift.svg"), 0xfa6031),
        "python" => (include_bytes!("../assets/brands/python.svg"), 0x3879ab),
        "dotnet" => (include_bytes!("../assets/brands/dotnet.svg"), 0x78695d),
        "cmake" => (include_bytes!("../assets/brands/cmake.svg"), 0x78695d),
        "flutter" => (include_bytes!("../assets/brands/flutter.svg"), 0x42a5f5),
        "ios" => (include_bytes!("../assets/brands/ios.svg"), 0x78695d),
        "java" => (include_bytes!("../assets/brands/java.svg"), 0x78695d),
        "unity" => (include_bytes!("../assets/brands/unity.svg"), 0x78695d),
        _ => (include_bytes!("../assets/brands/cmake.svg"), 0x78695d),
    };
    svg()
        .data(data)
        .size(px(21.))
        .text_color(rgb(color))
        .into_any_element()
}

/// Reference checkbox presentation, backed by Base's keyboard and mixed-state behavior.
pub fn selection_box(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    mixed: bool,
    disabled: bool,
) -> gpui_kit::base::Checkbox {
    use gpui_kit::base::{Checkbox, CheckboxState};
    let p = palette();
    let state = if mixed {
        CheckboxState::Indeterminate
    } else if checked {
        CheckboxState::Checked
    } else {
        CheckboxState::Unchecked
    };
    Checkbox::new(id)
        .accessibility_label(label)
        .state(state)
        .disabled(disabled)
        .size(px(20.))
        .flex_shrink_0()
        .rounded(px(6.))
        .border_1()
        .border_color(if checked || mixed { p.accent } else { p.border })
        .bg(if checked || mixed {
            p.accent
        } else {
            p.surface
        })
        .text_color(gpui_kit::white())
        .flex()
        .items_center()
        .justify_center()
        .opacity(if disabled { 0.5 } else { 1. })
        .focus(|style| style.border_2().border_color(p.foreground))
        .hover(move |style| {
            style.bg(if checked || mixed {
                p.accent.opacity(0.85)
            } else {
                p.category
            })
        })
        .children((checked || mixed).then(|| {
            icon(
                if mixed {
                    egui_icons::icons::ICON_REMOVE.codepoint
                } else {
                    egui_icons::icons::ICON_CHECK.codepoint
                },
                16.,
            )
        }))
}

// The reference explicitly uses a leading marker in addition to the selected fill.
pub fn selection_marker(height: f32) -> Div {
    div()
        .absolute()
        .left_0()
        .top(px(9.))
        .w(px(2.))
        .h(px(height))
        .rounded(px(2.))
        .bg(palette().accent)
}
