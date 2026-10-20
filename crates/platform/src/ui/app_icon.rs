//! The TextCraft app icon: a blue rounded tile holding a ruled notepad with two
//! binding rings.
//!
//! The icon is described once, as a list of rounded rectangles in a unit square, and
//! drawn two ways from that list: with the `egui` painter inside the window, and by a
//! small software rasteriser into the RGBA image the operating system shows in the
//! taskbar and Alt+Tab. One description keeps the two from drifting apart.

use egui::{Color32, CornerRadius, Painter, Rect, pos2, vec2};

/// One rounded rectangle in the unit square: centre, half-extents, corner radius.
struct Shape {
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
    radius: f32,
    color: Color32,
}

const BRAND: Color32 = Color32::from_rgb(0x2a, 0x8f, 0xe8);
const BRAND_DARK: Color32 = Color32::from_rgb(0x14, 0x4f, 0x8a);
const PAPER: Color32 = Color32::from_rgb(0xfc, 0xfd, 0xff);

const fn shape(cx: f32, cy: f32, hw: f32, hh: f32, radius: f32, color: Color32) -> Shape {
    Shape {
        cx,
        cy,
        hw,
        hh,
        radius,
        color,
    }
}

/// Back to front.
const SHAPES: [Shape; 8] = [
    // The tile.
    shape(0.5, 0.5, 0.5, 0.5, 0.22, BRAND),
    // The page.
    shape(0.5, 0.55, 0.28, 0.3, 0.05, PAPER),
    // The ruled lines.
    shape(0.5, 0.45, 0.17, 0.022, 0.022, BRAND),
    shape(0.5, 0.57, 0.17, 0.022, 0.022, BRAND),
    shape(0.42, 0.69, 0.09, 0.022, 0.022, BRAND),
    // The binding rings, over the page's top edge.
    shape(0.38, 0.25, 0.035, 0.075, 0.035, BRAND_DARK),
    shape(0.62, 0.25, 0.035, 0.075, 0.035, BRAND_DARK),
    // A highlight where the rings meet the paper.
    shape(
        0.5,
        0.265,
        0.2,
        0.012,
        0.012,
        Color32::from_rgb(0xd6, 0xe9, 0xfb),
    ),
];

/// Paints the icon filling `rect`.
pub fn paint(painter: &Painter, rect: Rect) {
    for s in &SHAPES {
        let center = pos2(
            rect.left() + s.cx * rect.width(),
            rect.top() + s.cy * rect.height(),
        );
        let half = vec2(s.hw * rect.width(), s.hh * rect.height());
        let radius = (s.radius * rect.width()).round().clamp(0.0, 255.0) as u8;
        painter.rect_filled(
            Rect::from_center_size(center, half * 2.0),
            CornerRadius::same(radius),
            s.color,
        );
    }
}

/// The icon as a `size`×`size` RGBA image for the window's taskbar entry.
pub fn window_icon(size: u32) -> egui::IconData {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let scale = size as f32;
    for y in 0..size {
        for x in 0..size {
            let (px, py) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
            // Composite back to front, each shape weighted by its anti-aliased coverage.
            let mut out = [0.0f32; 4];
            for s in &SHAPES {
                let coverage = coverage(s, px, py, 1.0 / scale);
                if coverage <= 0.0 {
                    continue;
                }
                let [r, g, b, _] = s.color.to_array();
                let a = coverage;
                for (channel, value) in out.iter_mut().zip([r as f32, g as f32, b as f32, 255.0]) {
                    *channel = value * a + *channel * (1.0 - a);
                }
            }
            let at = ((y * size + x) * 4) as usize;
            for (slot, value) in rgba[at..at + 4].iter_mut().zip(out) {
                *slot = value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

/// How much of the pixel at (`px`, `py`) the shape covers, 0 to 1, from the signed
/// distance to its rounded rectangle softened over one pixel (`pixel`).
fn coverage(s: &Shape, px: f32, py: f32, pixel: f32) -> f32 {
    let qx = (px - s.cx).abs() - (s.hw - s.radius);
    let qy = (py - s.cy).abs() - (s.hh - s.radius);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let distance = outside + qx.max(qy).min(0.0) - s.radius;
    (0.5 - distance / pixel).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    /// Regenerates `assets/textcraft.ico` and `assets/textcraft.png` from the icon's own description, so the
    /// executable's icon and the in-window one cannot drift apart:
    /// `cargo test -p nc-platform write_icon_file -- --ignored`.
    #[test]
    #[ignore = "writes assets/textcraft.ico and .png"]
    fn write_icon_file() {
        let sizes = [16u32, 24, 32, 48, 64, 128, 256];
        let mut images: Vec<Vec<u8>> = Vec::new();
        for size in sizes {
            let icon = window_icon(size);
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(&icon.rgba, size, size, image::ExtendedColorType::Rgba8)
                .expect("encode png");
            images.push(png);
        }
        // ICONDIR, then one ICONDIRENTRY per image, then the PNG payloads.
        let mut ico = vec![0, 0, 1, 0];
        ico.extend((sizes.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * sizes.len() as u32;
        for (size, png) in sizes.iter().zip(&images) {
            let dimension = if *size >= 256 { 0 } else { *size as u8 };
            ico.extend([dimension, dimension, 0, 0]);
            ico.extend(1u16.to_le_bytes());
            ico.extend(32u16.to_le_bytes());
            ico.extend((png.len() as u32).to_le_bytes());
            ico.extend(offset.to_le_bytes());
            offset += png.len() as u32;
        }
        for png in &images {
            ico.extend(png);
        }
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/textcraft.ico");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("assets dir");
        std::fs::write(&path, ico).expect("write ico");

        // The 256 px PNG the Linux and macOS packages carry.
        let png_path = path.with_file_name("textcraft.png");
        std::fs::write(png_path, images.last().expect("a 256 px image")).expect("write png");
    }

    #[test]
    fn the_window_icon_is_square_rgba_with_transparent_corners() {
        let icon = window_icon(64);
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);
        // The very corner lies outside the rounded tile.
        assert_eq!(icon.rgba[3], 0);
        // The middle of the page is opaque paper.
        let centre = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(icon.rgba[centre + 3], 255);
    }
}
