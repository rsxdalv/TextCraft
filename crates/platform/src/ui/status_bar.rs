//! The status bar: caret position, character count, document kind, zoom,
//! line-ending style, and encoding.

use egui::{Align, Align2, FontId, Layout, RichText};

use crate::ui::a11y;
use crate::ui::spacing;
use crate::ui::state::Shell;
use crate::ui::theme::Palette;

/// The height of the status bar, in points.
pub const HEIGHT: f32 = 32.0;

/// Draws the status bar.
pub fn show(shell: &Shell, ui: &mut egui::Ui) {
    egui::Panel::bottom("status_bar")
        .exact_size(HEIGHT)
        .frame(egui::Frame::NONE.fill(shell.palette.chrome))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.x_range(),
                rect.top() + 0.5,
                egui::Stroke::new(1.0, shell.palette.line),
            );
            let tab = shell.active_tab();

            ui.horizontal_centered(|ui| {
                ui.add_space(spacing::LG);
                let palette = shell.palette;
                ui.label(item(
                    format!("Ln {}, Col {}", shell.line, shell.column),
                    palette,
                ));
                divider(ui);
                ui.label(item(format!("{} characters", shell.char_count()), palette));
                if let Some(note) = &shell.notification {
                    divider(ui);
                    ui.label(RichText::new(note).size(13.0).color(shell.palette.text));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(spacing::LG);
                    ui.label(item(
                        tab.map(|t| t.encoding_label()).unwrap_or("UTF-8"),
                        palette,
                    ));
                    divider(ui);
                    ui.label(item(
                        tab.map(|t| t.line_ending_label())
                            .unwrap_or("Windows (CRLF)"),
                        palette,
                    ));
                    divider(ui);
                    ui.label(item(format!("{}%", shell.zoom), palette));
                });
            });

            // The document kind is painted in the middle of the bar and has no
            // widget behind it, so a screen reader is told about it here
            // (`NC-P007`).
            let label = tab.map(|t| t.kind_label()).unwrap_or("Plain text");
            let galley = ui.painter().layout_no_wrap(
                label.to_owned(),
                FontId::proportional(13.0),
                shell.palette.text_dim,
            );
            let text_rect = egui::Rect::from_center_size(rect.center(), galley.size());
            a11y::painted(ui.ctx(), ui.id().with("document_kind"), text_rect, label);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(13.0),
                shell.palette.text_dim,
            );
        });
}

/// A status-bar text item.
fn item(text: impl Into<String>, palette: Palette) -> RichText {
    RichText::new(text.into())
        .size(13.0)
        .color(palette.text_dim)
}

/// A hairline between two status items, with room on both sides.
fn divider(ui: &mut egui::Ui) {
    ui.add_space(spacing::SM);
    ui.separator();
    ui.add_space(spacing::SM);
}
