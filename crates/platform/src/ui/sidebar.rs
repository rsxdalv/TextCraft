//! The collapsible workspace sidebar (`View ▸ Workspace sidebar`, `gui2` reference).
//!
//! Lists the open documents in the current window with a header showing `WORKSPACE`
//! and `<N> files`, highlights the active tab in pine-green (`palette.accent`,
//! `#2D6A4F`) with a `4pt` darker left border (`palette.accent_border`, `#1E4D38`),
//! and offers a `+ New File` button at the bottom.

use egui::{Align2, Color32, Context, CornerRadius, FontId, Rect, Sense, Stroke, WidgetType};
use nc_commands::CommandId;

use crate::ui::state::Shell;
use crate::ui::{a11y, actions, spacing};

/// Width of the workspace sidebar in points (`w-56` in `gui2`).
pub const WIDTH: f32 = 220.0;

/// Height of the `WORKSPACE` header row.
const HEADER_HEIGHT: f32 = 34.0;
/// Height of one file row in the sidebar list (intentionally distinct from
/// `title_bar::TAB_HEIGHT = 30.0` so geometry assertions on the tab strip stay unambiguous).
const ITEM_HEIGHT: f32 = 26.0;
/// Height of the bottom footer area holding `+ New File`.
const FOOTER_HEIGHT: f32 = 44.0;
/// Height of the `+ New File` button inside the footer.
const NEW_BTN_HEIGHT: f32 = 28.0;

/// Draws the collapsible workspace sidebar on the left of the editor.
pub fn show(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context) {
    let palette = shell.palette;
    egui::Panel::left("workspace_sidebar")
        .resizable(false)
        .exact_size(WIDTH)
        .frame(egui::Frame::NONE.fill(palette.sidebar_bg))
        .show(ui, |ui| {
            let panel_rect = ui.max_rect();
            ui.painter().vline(
                panel_rect.right() - 0.5,
                panel_rect.y_range(),
                Stroke::new(1.0, palette.line),
            );

            // Header: WORKSPACE + file count.
            let (header_rect, _) = ui.allocate_exact_size(
                egui::vec2(panel_rect.width(), HEADER_HEIGHT),
                Sense::hover(),
            );
            ui.painter().text(
                egui::pos2(header_rect.left() + spacing::MD, header_rect.center().y),
                Align2::LEFT_CENTER,
                "WORKSPACE",
                FontId::proportional(10.5),
                palette.text,
            );
            let count = shell.tabs.len();
            let count_label = if count == 1 {
                "1 file".to_owned()
            } else {
                format!("{count} files")
            };
            ui.painter().text(
                egui::pos2(header_rect.right() - spacing::MD, header_rect.center().y),
                Align2::RIGHT_CENTER,
                count_label,
                FontId::monospace(10.5),
                palette.text_dim,
            );
            ui.painter().hline(
                header_rect.x_range(),
                header_rect.bottom() - 0.5,
                Stroke::new(1.0, palette.line),
            );

            // Reserve space for the bottom footer and fill the middle with the file list.
            let list_height = (panel_rect.bottom() - header_rect.bottom() - FOOTER_HEIGHT).max(0.0);
            let mut clicked_tab: Option<usize> = None;
            ui.allocate_ui(egui::vec2(panel_rect.width(), list_height), |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("workspace_sidebar_scroll")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(spacing::XS);
                        for (index, tab) in shell.tabs.iter().enumerate() {
                            let is_active = index == shell.active;
                            let name = tab.workspace_name();
                            let (row_rect, response) = ui.allocate_exact_size(
                                egui::vec2(panel_rect.width() - 1.0, ITEM_HEIGHT),
                                Sense::click(),
                            );
                            a11y::label(&response, WidgetType::Button, format!("Switch to {name}"));
                            if is_active {
                                ui.painter().rect_filled(row_rect, 0.0, palette.accent);
                                let border_strip = Rect::from_min_max(
                                    row_rect.min,
                                    egui::pos2(row_rect.left() + 4.0, row_rect.bottom()),
                                );
                                ui.painter()
                                    .rect_filled(border_strip, 0.0, palette.accent_border);
                            } else if response.hovered() {
                                ui.painter().rect_filled(row_rect, 0.0, palette.chrome);
                            }

                            let fg = if is_active {
                                Color32::WHITE
                            } else {
                                palette.text
                            };
                            let icon_color = if is_active {
                                Color32::WHITE
                            } else {
                                palette.text_dim
                            };
                            paint_file_icon(
                                ui.painter(),
                                egui::pos2(
                                    row_rect.left() + spacing::MD + 6.0,
                                    row_rect.center().y,
                                ),
                                icon_color,
                            );
                            ui.painter().text(
                                egui::pos2(
                                    row_rect.left() + spacing::MD + 18.0,
                                    row_rect.center().y,
                                ),
                                Align2::LEFT_CENTER,
                                &name,
                                FontId::proportional(12.0),
                                fg,
                            );
                            if response.clicked() {
                                clicked_tab = Some(index);
                            }
                        }
                    });
            });
            if let Some(index) = clicked_tab {
                shell.active = index;
            }

            // Footer: + New File button.
            let footer_top = panel_rect.bottom() - FOOTER_HEIGHT;
            ui.painter().hline(
                panel_rect.x_range(),
                footer_top + 0.5,
                Stroke::new(1.0, palette.line),
            );
            let btn_rect = Rect::from_min_max(
                egui::pos2(
                    panel_rect.left() + spacing::SM,
                    footer_top + (FOOTER_HEIGHT - NEW_BTN_HEIGHT) * 0.5,
                ),
                egui::pos2(
                    panel_rect.right() - spacing::SM - 1.0,
                    footer_top + (FOOTER_HEIGHT + NEW_BTN_HEIGHT) * 0.5,
                ),
            );
            let new_resp = ui.interact(btn_rect, ui.id().with("sidebar_new_file"), Sense::click());
            a11y::label(&new_resp, WidgetType::Button, "New file");
            let fill = if new_resp.hovered() {
                palette.chrome
            } else {
                palette.bg
            };
            ui.painter()
                .rect_filled(btn_rect, CornerRadius::same(4), fill);
            ui.painter().rect_stroke(
                btn_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, palette.border),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                btn_rect.center(),
                Align2::CENTER_CENTER,
                "+ New File",
                FontId::proportional(12.0),
                palette.accent,
            );
            if new_resp.clicked() {
                actions::run(shell, ctx, CommandId("cmd.file.new_tab"));
            }
        });
}

/// Paints a compact 10×12 document outline icon centered at `center`.
fn paint_file_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let rect = Rect::from_center_size(center, egui::vec2(10.0, 12.0));
    painter.rect_stroke(
        rect,
        CornerRadius::same(1),
        Stroke::new(1.2, color),
        egui::StrokeKind::Inside,
    );
    painter.hline(
        (rect.left() + 2.5)..=(rect.right() - 2.5),
        rect.top() + 4.5,
        Stroke::new(1.0, color),
    );
    painter.hline(
        (rect.left() + 2.5)..=(rect.right() - 2.5),
        rect.top() + 7.5,
        Stroke::new(1.0, color),
    );
}
