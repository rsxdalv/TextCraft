//! The status bar: caret position, character count, word count, document kind,
//! zoom, line-ending style, encoding, and local sync status (`gui2` reference).

use egui::{Align, Layout, RichText, Sense};

use crate::ui::a11y;
use crate::ui::i18n;
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
                ui.label(item(
                    format!(
                        "{} {}",
                        shell.char_count(),
                        i18n::tr(shell.language, "characters")
                    ),
                    palette,
                ));
                divider(ui);
                let words = shell.word_count();
                let words_label = if words == 1 {
                    i18n::tr(shell.language, "1 word").to_owned()
                } else {
                    format!("{words} {}", i18n::tr(shell.language, "words"))
                };
                ui.label(item(words_label, palette));
                if let Some(note) = &shell.notification {
                    divider(ui);
                    ui.label(RichText::new(note).size(13.0).color(shell.palette.text));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(spacing::LG);
                    let dirty = tab.is_some_and(|t| t.is_dirty());
                    let sync_text = if dirty {
                        i18n::tr(shell.language, "Unsaved changes")
                    } else {
                        i18n::tr(shell.language, "Synced locally")
                    };
                    ui.label(RichText::new(sync_text).size(12.5).color(palette.text));
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 3.0, palette.accent);

                    divider(ui);
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
                    divider(ui);
                    ui.label(item(shell.editor_mode.label(), palette));
                    divider(ui);
                    let raw_label = tab.map(|t| t.kind_label()).unwrap_or("Plain text");
                    let label = i18n::tr(shell.language, raw_label);
                    let kind_resp = ui.label(item(label, palette));
                    a11y::painted(
                        ui.ctx(),
                        ui.id().with("document_kind"),
                        kind_resp.rect,
                        label,
                    );
                });
            });
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
