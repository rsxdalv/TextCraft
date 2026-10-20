//! The shell's SVG icons, prepared through `craft-ui`.
//!
//! Drawn icons replace text glyphs (`▾`, `✨`, `🔗`, …) that the bundled fonts do not
//! carry and that therefore rendered as empty boxes. The SVGs are original, monochrome
//! and tinted at draw time, so they follow the theme.

use std::sync::OnceLock;

use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use craft_ui::icons::{SvgIconSet, SvgStyle};
use egui::{Color32, Context, CornerRadius, Rect, Response, Stroke, Ui, Vec2, vec2};

use crate::ui::theme::Palette;

/// Edge length of an icon drawn inside a button, in points.
pub const SIZE: f32 = 16.0;

static ICONS: &[(&str, &[u8])] = &[
    (
        "chevron-down",
        include_bytes!("../../assets/icons/chevron-down.svg"),
    ),
    ("list", include_bytes!("../../assets/icons/list.svg")),
    ("sparkle", include_bytes!("../../assets/icons/sparkle.svg")),
    ("link", include_bytes!("../../assets/icons/link.svg")),
    ("table", include_bytes!("../../assets/icons/table.svg")),
    ("chat", include_bytes!("../../assets/icons/chat.svg")),
    (
        "overflow",
        include_bytes!("../../assets/icons/overflow.svg"),
    ),
    ("close", include_bytes!("../../assets/icons/close.svg")),
    (
        "chevron-left",
        include_bytes!("../../assets/icons/chevron-left.svg"),
    ),
    (
        "chevron-right",
        include_bytes!("../../assets/icons/chevron-right.svg"),
    ),
    ("check", include_bytes!("../../assets/icons/check.svg")),
];

fn set() -> &'static SvgIconSet {
    static SET: OnceLock<SvgIconSet> = OnceLock::new();
    SET.get_or_init(|| {
        SvgIconSet::new(
            ICONS,
            SvgStyle {
                current_color: "#ffffff",
                stroke_width: None,
            },
            "bytes://textcraft/icons/",
            "overflow",
        )
    })
}

/// Installs the SVG image loader. Called once, before the first frame.
pub fn install(ctx: &Context) {
    egui_extras::install_image_loaders(ctx);
}

/// A named icon, `size` points square, tinted `tint`.
pub fn image(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    egui::Image::new(set().source(name))
        .fit_to_exact_size(Vec2::splat(size))
        .tint(tint)
}

/// A named icon at the standard size, for use inside a button's atoms.
pub fn glyph(name: &str, tint: Color32) -> egui::Image<'static> {
    image(name, SIZE, tint)
}

/// A flat icon button: no frame until hovered. `label` is what a screen reader hears.
pub fn button(ui: &mut Ui, label: &str, name: &str, palette: Palette) -> Response {
    let normal = ButtonVisuals {
        fill: Color32::TRANSPARENT,
        stroke: Stroke::NONE,
        icon: palette.text,
    };
    let style = IconButtonStyle {
        normal,
        hovered: ButtonVisuals {
            fill: palette.hover,
            ..normal
        },
        pressed: Some(ButtonVisuals {
            fill: palette.active,
            ..normal
        }),
        selected: ButtonVisuals {
            fill: palette.active,
            ..normal
        },
        corner_radius: CornerRadius::same(4),
        focus_stroke: Stroke::new(1.0, palette.accent),
    };
    IconButton::new(label, vec2(30.0, 26.0), &style).show(ui, |ui, rect, tint| {
        paint(ui, rect, name, tint);
    })
}

/// Paints the icon `size` points square, centred on `center`.
pub fn paint_centered(ui: &Ui, center: egui::Pos2, name: &str, size: f32, tint: Color32) {
    let target = Rect::from_center_size(center, Vec2::splat(size));
    image(name, size, tint).paint_at(ui, target);
}

/// Paints the icon centred in `rect`.
fn paint(ui: &Ui, rect: Rect, name: &str, tint: Color32) {
    let target = Rect::from_center_size(rect.center(), Vec2::splat(SIZE));
    image(name, SIZE, tint).paint_at(ui, target);
}
