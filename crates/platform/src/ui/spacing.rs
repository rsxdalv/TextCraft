//! The shared spacing scale.
//!
//! Every gap, inset and margin in the shell comes from this one scale, so the
//! window breathes the same way everywhere and a spacing change is a one-line
//! edit here rather than a hunt for literals. Values are in points.

use egui::Margin;

/// 4 pt: the tightest gap, between a control and its own label or icon.
pub const XS: f32 = 4.0;
/// 8 pt: the default gap between neighbouring controls.
pub const SM: f32 = 8.0;
/// 12 pt: the inset from a window edge to a bar's first and last control, and
/// the gap between groups of controls.
pub const MD: f32 = 12.0;
/// 16 pt: the padding of a content surface, and the gap between sections.
pub const LG: f32 = 16.0;
/// 24 pt: the generous margin of a full page such as Settings.
pub const XL: f32 = 24.0;

/// A margin with `x` on the left and right and `y` on the top and bottom.
pub fn margin(x: f32, y: f32) -> Margin {
    Margin::symmetric(x as i8, y as i8)
}

/// A margin of `value` on every side.
pub fn margin_all(value: f32) -> Margin {
    Margin::same(value as i8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_only_grows() {
        let scale = [XS, SM, MD, LG, XL];
        assert!(scale.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn margins_convert_without_losing_the_value() {
        let m = margin(LG, MD);
        assert_eq!((m.left, m.right, m.top, m.bottom), (16, 16, 12, 12));
        assert_eq!(margin_all(SM).left, 8);
    }
}
