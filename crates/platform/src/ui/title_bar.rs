//! The title bar: app icon, tab strip, new-tab button, and the window's caption
//! buttons, all in one row, as in Windows Notepad.
//!
//! The window has no OS decorations, so this row is also its handle: empty space in
//! it drags the window and a double click there maximizes or restores it. Edge and
//! corner resizing live in [`crate::ui::window_frame`].
//!
//! Layout, from the Notepad reference: the caption buttons hang from the top edge of
//! the row, while the icon, tabs and `+` sit on its bottom edge, so the active tab
//! runs straight down into the menu bar under it.
//!
//! When the tabs do not fit even at their narrowest, the strip scrolls: `‹` and `›`
//! buttons and the mouse wheel move it, and the active tab is kept in view. Tabs
//! are reordered by dragging.

use egui::accesskit::Role;
use egui::{
    Align2, Color32, Context, CornerRadius, FontId, PointerButton, Pos2, Rect, Sense, Stroke,
    UiBuilder, Vec2, ViewportCommand, WidgetType, pos2, vec2,
};

use crate::ui::state::{DocKind, Shell, Tab};
use crate::ui::theme::Palette;
use crate::ui::{a11y, actions, icons, menu_bar, spacing};
use nc_commands::{CommandId, MenuItem, build_tab_context_menu};

/// The height of the title bar, in points.
pub const HEIGHT: f32 = 42.0;

/// Width of one caption button (minimize, maximize, close), as on Windows 11.
const CAPTION_WIDTH: f32 = 46.0;
/// Height of a caption button, which hangs from the top edge.
const CAPTION_HEIGHT: f32 = 32.0;
/// Empty title-bar space always kept free to grab the window by.
const DRAG_GAP: f32 = 48.0;

/// The height of a tab; its bottom edge is the bottom edge of the bar.
const TAB_HEIGHT: f32 = 30.0;
/// The app icon's edge.
const ICON_SIZE: f32 = 16.0;
/// The narrowest a tab is squeezed to before the strip starts scrolling.
const MIN_TAB_WIDTH: f32 = 84.0;
/// The width of every tab while the strip scrolls.
const SCROLL_TAB_WIDTH: f32 = 120.0;
/// A scroll arrow's width.
const ARROW_WIDTH: f32 = 24.0;
/// The new-tab button's width.
const NEW_TAB_WIDTH: f32 = 30.0;

/// The width a tab takes when there is room: its title plus the close button.
fn natural_tab_width(tab: &Tab) -> f32 {
    (tab.title().chars().count() as f32 * 7.5 + 52.0).clamp(120.0, 220.0)
}

/// Where a tab is drawn and whether it is the active one.
#[derive(Clone, Copy)]
struct TabSlot {
    index: usize,
    rect: Rect,
    active: bool,
}

/// What the pointer did to a tab.
#[derive(Default)]
struct TabHit {
    /// The body of the tab was clicked.
    activate: bool,
    /// The tab's close button was clicked.
    close: bool,
    /// The tab was right-clicked, which raises its context menu.
    menu: bool,
    /// A drag of the tab began.
    drag_started: bool,
    /// A drag of the tab ended.
    drag_stopped: bool,
}

fn scroll_id() -> egui::Id {
    egui::Id::new("nc_tab_strip_scroll")
}

fn drag_id() -> egui::Id {
    egui::Id::new("nc_tab_strip_drag")
}

/// Draws the title bar and applies any tab changes it caused.
pub fn show(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context) {
    egui::Panel::top("title_bar")
        .exact_size(HEIGHT)
        .frame(egui::Frame::NONE.fill(shell.palette.title))
        .show(ui, |ui| {
            // Registered before the controls so the empty parts of the bar drag
            // the window while the controls still receive their clicks.
            //
            // `Sense::DRAG` rather than `click_and_drag`: a click that drags nothing
            // is not something this bar does, and the click sense would put the
            // whole title bar in the tab order as an unnamed stop (`NC-P007`). A
            // double click is read from the pointer instead, and only counts when the
            // bar itself, not a control on it, is what the pointer is over.
            let bar = ui.max_rect();
            let drag = ui.interact(bar, ui.id().with("title_drag"), Sense::DRAG);
            if drag.drag_started() {
                crate::ui::window_frame::mark_os_move(ctx);
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if drag.hovered()
                && ctx.input(|i| i.pointer.button_double_clicked(PointerButton::Primary))
            {
                toggle_maximized(ctx);
            }

            let mut to_activate: Option<usize> = None;
            let mut to_close: Option<usize> = None;
            let mut to_move: Option<(usize, usize)> = None;
            // A command chosen from a tab's context menu. Collected during the
            // draw and run after it, the way a tab click is: the items act on
            // the tab the menu was raised from, which right-clicking has already
            // made the active one (`NC-T003`, `docs/ui-reference.md` §11).
            let mut chosen: Option<CommandId> = None;
            let tab_menu = build_tab_context_menu(shell.availability());
            let palette = shell.palette;

            // Everything but the caption buttons sits on the bottom edge.
            let band_top = bar.bottom() - TAB_HEIGHT;
            let center_y = band_top + TAB_HEIGHT / 2.0;

            let icon_rect = Rect::from_center_size(
                pos2(bar.left() + spacing::MD + ICON_SIZE / 2.0, center_y),
                Vec2::splat(ICON_SIZE),
            );
            crate::ui::app_icon::paint(ui.painter(), icon_rect);

            // Tabs share the room between the icon and the grab area. All of them fit
            // down to `MIN_TAB_WIDTH`, or the strip scrolls.
            let strip_left = icon_rect.right() + spacing::MD;
            let right_limit = bar.right() - 3.0 * CAPTION_WIDTH - DRAG_GAP;
            let room = (right_limit - strip_left - NEW_TAB_WIDTH).max(0.0);
            let natural: Vec<f32> = shell.tabs.iter().map(natural_tab_width).collect();
            let fit = craft_ui::tabs::fit(&natural, shell.active, room, MIN_TAB_WIDTH, 0.0);
            let scrolling = !fit.overflow.is_empty();

            let count = shell.tabs.len();
            let (view, widths): (Rect, Vec<f32>) = if scrolling {
                let view_left = strip_left + ARROW_WIDTH;
                let view_width = (room - 2.0 * ARROW_WIDTH).max(0.0);
                (
                    Rect::from_min_size(pos2(view_left, band_top), vec2(view_width, TAB_HEIGHT)),
                    vec![SCROLL_TAB_WIDTH; count],
                )
            } else {
                let widths: Vec<f32> = fit.shown.iter().map(|&(_, width)| width).collect();
                let total: f32 = widths.iter().sum();
                (
                    Rect::from_min_size(pos2(strip_left, band_top), vec2(total, TAB_HEIGHT)),
                    widths,
                )
            };
            let total_width: f32 = widths.iter().sum();

            // Scroll position: moved by the wheel and the arrows, and kept so the
            // active tab is in view whenever it changes.
            let (mut offset, last_active): (f32, usize) = ctx
                .data(|data| data.get_temp(scroll_id()))
                .unwrap_or((0.0, usize::MAX));
            let max_offset = (total_width - view.width()).max(0.0);
            if scrolling {
                if last_active != shell.active {
                    let left = shell.active as f32 * SCROLL_TAB_WIDTH;
                    if left < offset {
                        offset = left;
                    } else if left + SCROLL_TAB_WIDTH > offset + view.width() {
                        offset = left + SCROLL_TAB_WIDTH - view.width();
                    }
                }
                let over_strip = ctx
                    .pointer_hover_pos()
                    .is_some_and(|pos| view.expand2(vec2(ARROW_WIDTH, 0.0)).contains(pos));
                if over_strip {
                    let delta = ctx.input(|input| input.smooth_scroll_delta);
                    if delta != Vec2::ZERO {
                        offset -= delta.x + delta.y;
                        ctx.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
                    }
                }
            } else {
                offset = 0.0;
            }

            if scrolling {
                let left =
                    Rect::from_min_size(pos2(strip_left, band_top), vec2(ARROW_WIDTH, TAB_HEIGHT));
                if scroll_arrow(
                    ui,
                    left,
                    "chevron-left",
                    "Scroll tabs left",
                    offset > 0.0,
                    palette,
                ) {
                    offset -= SCROLL_TAB_WIDTH;
                }
                let right = Rect::from_min_size(
                    pos2(view.right(), band_top),
                    vec2(ARROW_WIDTH, TAB_HEIGHT),
                );
                if scroll_arrow(
                    ui,
                    right,
                    "chevron-right",
                    "Scroll tabs right",
                    offset < max_offset,
                    palette,
                ) {
                    offset += SCROLL_TAB_WIDTH;
                }
            }
            offset = offset.clamp(0.0, max_offset);
            ctx.data_mut(|data| data.insert_temp(scroll_id(), (offset, shell.active)));

            // The tabs, drawn into a child that clips them to the strip.
            let mut strip = ui.new_child(UiBuilder::new().max_rect(view));
            strip.set_clip_rect(view.intersect(ui.clip_rect()));
            let mut rects: Vec<Rect> = Vec::with_capacity(count);
            let mut x = view.left() - offset;
            for &width in &widths {
                rects.push(Rect::from_min_size(
                    pos2(x, band_top),
                    vec2(width, TAB_HEIGHT),
                ));
                x += width;
            }

            let dragging: Option<usize> = ctx.data(|data| data.get_temp(drag_id()));
            for (index, rect) in rects.iter().enumerate() {
                let Some(tab) = shell.tabs.get(index) else {
                    continue;
                };
                let slot = TabSlot {
                    index,
                    rect: *rect,
                    active: index == shell.active,
                };
                let hit = tab_widget(&mut strip, tab, slot, palette, &tab_menu, &mut chosen);
                if hit.close {
                    to_close = Some(index);
                } else if hit.activate || hit.menu {
                    // A right-click selects the tab it landed on
                    // before it offers anything, so every item
                    // then acts on the document the user aimed at
                    // rather than on whichever tab happened to be
                    // active — and an item that cannot be honoured
                    // is greyed rather than wrong.
                    to_activate = Some(index);
                }
                if hit.drag_started {
                    ctx.data_mut(|data| data.insert_temp(drag_id(), index));
                }
                if hit.drag_stopped && dragging == Some(index) {
                    let gap = gap_at(&rects, ctx.pointer_interact_pos());
                    to_move = Some((index, gap));
                    ctx.data_mut(|data| data.remove_by_type::<usize>());
                }
            }

            // A hairline between neighbouring tabs, none beside the active one, whose
            // fill already sets it apart.
            let divider = palette.border.gamma_multiply(0.7);
            for (index, rect) in rects.iter().enumerate().take(count.saturating_sub(1)) {
                if index == shell.active || index + 1 == shell.active {
                    continue;
                }
                let edge = rect.right();
                strip.painter().line_segment(
                    [pos2(edge, center_y - 8.0), pos2(edge, center_y + 8.0)],
                    Stroke::new(1.0, divider),
                );
            }

            // Where a dragged tab would land.
            if dragging.is_some() && ctx.input(|input| input.pointer.any_down()) {
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                let gap = gap_at(&rects, ctx.pointer_interact_pos());
                let edge = if gap == 0 {
                    rects.first().map_or(view.left(), |rect| rect.left())
                } else {
                    rects.get(gap - 1).map_or(view.right(), |rect| rect.right())
                };
                strip.painter().line_segment(
                    [pos2(edge, band_top + 3.0), pos2(edge, bar.bottom() - 3.0)],
                    Stroke::new(2.0, palette.accent),
                );
            }

            // The new-tab button follows the last tab, or the right arrow.
            let plus_left = if scrolling {
                view.right() + ARROW_WIDTH
            } else {
                view.right()
            };
            let plus_rect = Rect::from_center_size(
                pos2(plus_left + NEW_TAB_WIDTH / 2.0, center_y),
                vec2(NEW_TAB_WIDTH, 26.0),
            );
            let new_tab = ui.interact(plus_rect, ui.id().with("new_tab"), Sense::click());
            a11y::label(&new_tab, WidgetType::Button, "New tab");
            if new_tab.hovered() {
                ui.painter()
                    .rect_filled(plus_rect, CornerRadius::same(4), palette.hover);
            }
            let stroke = Stroke::new(1.2, palette.text);
            let c = plus_rect.center();
            ui.painter()
                .line_segment([pos2(c.x - 5.0, c.y), pos2(c.x + 5.0, c.y)], stroke);
            ui.painter()
                .line_segment([pos2(c.x, c.y - 5.0), pos2(c.x, c.y + 5.0)], stroke);
            if new_tab.clicked() {
                shell.open_tab(DocKind::Plain);
            }
            let _ = new_tab.on_hover_text("New tab");

            caption_buttons(shell, ui, ctx, bar);

            if let Some((from, gap)) = to_move {
                shell.move_tab(from, gap);
            } else if let Some(index) = to_activate {
                shell.active = index;
            }
            if let Some(index) = to_close {
                actions::close_tab(shell, index);
            }
            if let Some(command) = chosen {
                actions::run(shell, ctx, command);
            }
        });
}

/// The gap between tabs (0 = before the first) nearest the pointer.
fn gap_at(rects: &[Rect], pointer: Option<Pos2>) -> usize {
    let Some(pointer) = pointer else {
        return rects.len();
    };
    rects
        .iter()
        .filter(|rect| rect.center().x < pointer.x)
        .count()
}

/// A scroll arrow: dimmed and inert when there is nowhere to scroll.
fn scroll_arrow(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: &str,
    label: &str,
    enabled: bool,
    palette: Palette,
) -> bool {
    let response = ui.interact(rect, ui.id().with(label), Sense::click());
    a11y::label(&response, WidgetType::Button, label);
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect.shrink2(vec2(0.0, 2.0)),
            CornerRadius::same(4),
            palette.hover,
        );
    }
    let tint = if enabled {
        palette.text
    } else {
        palette.text_dim.gamma_multiply(0.5)
    };
    icons::paint_centered(ui, rect.center(), icon, 14.0, tint);
    enabled && response.clicked()
}

fn toggle_maximized(ctx: &Context) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}

/// Which caption button.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Caption {
    Minimize,
    Maximize,
    Close,
}

/// The three caption buttons, flush with the right and top edges so each is a large
/// target and the corner button stays easy to hit.
///
/// Registered close, maximize, minimize: the order a keyboard reaches them. Painted
/// minimize, maximize, close from left to right.
fn caption_buttons(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, bar: Rect) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let palette = shell.palette;
    let rect_for = |slot: usize| {
        // `slot` 0 is the rightmost button.
        let right = bar.right() - CAPTION_WIDTH * slot as f32;
        Rect::from_min_size(
            pos2(right - CAPTION_WIDTH, bar.top()),
            vec2(CAPTION_WIDTH, CAPTION_HEIGHT),
        )
    };

    for (caption, slot) in [
        (Caption::Close, 0),
        (Caption::Maximize, 1),
        (Caption::Minimize, 2),
    ] {
        let rect = rect_for(slot);
        let label = match caption {
            Caption::Close => "Close window",
            Caption::Maximize if maximized => "Restore window",
            Caption::Maximize => "Maximize window",
            Caption::Minimize => "Minimize window",
        };
        let response = ui.interact(rect, ui.id().with(("caption", slot)), Sense::click());
        a11y::label(&response, WidgetType::Button, label);

        let (fill, glyph) = match caption {
            Caption::Close if response.is_pointer_button_down_on() => {
                (Color32::from_rgb(0x8f, 0x20, 0x16), Color32::WHITE)
            }
            Caption::Close if response.hovered() => {
                (Color32::from_rgb(0xc4, 0x2b, 0x1c), Color32::WHITE)
            }
            _ if response.is_pointer_button_down_on() => (palette.active, palette.text),
            _ if response.hovered() => (palette.hover, palette.text),
            _ => (Color32::TRANSPARENT, palette.text),
        };
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, 0.0, fill);
        }
        paint_caption_glyph(ui, rect.center(), caption, maximized, glyph);

        if response.clicked() {
            match caption {
                // Closing the window prompts for unsaved documents (`NC-D005`).
                Caption::Close => actions::close_window(shell),
                Caption::Maximize => ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                Caption::Minimize => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
            }
        }
    }
}

/// The thin Windows 11 caption glyphs, drawn so they stay crisp at any scale.
fn paint_caption_glyph(ui: &egui::Ui, c: Pos2, caption: Caption, maximized: bool, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.0, color);
    let h = 5.0;
    // Half a point off the pixel grid puts a one-point line on whole pixels.
    let c = pos2(c.x.round() + 0.5, c.y.round() + 0.5);
    match caption {
        Caption::Minimize => {
            painter.line_segment([pos2(c.x - h, c.y), pos2(c.x + h, c.y)], stroke);
        }
        Caption::Maximize if !maximized => {
            painter.rect_stroke(
                Rect::from_center_size(c, vec2(2.0 * h, 2.0 * h)),
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Caption::Maximize => {
            // Restore: a front square, and the corner of the one behind it.
            let side = 2.0 * h - 2.0;
            let front = Rect::from_min_size(pos2(c.x - h, c.y - h + 2.0), vec2(side, side));
            painter.rect_stroke(
                front,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
            let corner = [
                pos2(c.x - h + 2.0, c.y - h),
                pos2(c.x + h, c.y - h),
                pos2(c.x + h, c.y + h - 2.0),
            ];
            painter.line_segment([corner[0], corner[1]], stroke);
            painter.line_segment([corner[1], corner[2]], stroke);
        }
        Caption::Close => {
            painter.line_segment([pos2(c.x - h, c.y - h), pos2(c.x + h, c.y + h)], stroke);
            painter.line_segment([pos2(c.x - h, c.y + h), pos2(c.x + h, c.y - h)], stroke);
        }
    }
}

/// A single tab, returning what was done to it.
///
/// `tab_menu` is the registry's tab context menu, already resolved against the
/// shell's availability; `chosen` collects a command the user picks from it.
///
/// The active tab is filled with the menu bar's colour so the two read as one
/// surface; the rest have no fill at all, hovered or not, and are told apart by a
/// hairline between them.
fn tab_widget(
    ui: &mut egui::Ui,
    tab: &Tab,
    slot: TabSlot,
    palette: Palette,
    tab_menu: &[MenuItem],
    chosen: &mut Option<CommandId>,
) -> TabHit {
    let TabSlot {
        index,
        rect,
        active,
    } = slot;
    let title = tab.title();
    let response = ui.interact(rect, ui.id().with(("tab", index)), Sense::click_and_drag());

    // What a screen reader is told about the tab: its document's name, which one
    // is being shown, and whether anything in it is unsaved — the last of which
    // is a dot on the strip and nothing else (`NC-P007`).
    a11y::label(&response, WidgetType::SelectableLabel, &title);
    a11y::role(ui.ctx(), response.id, Role::Tab);
    a11y::selected(ui.ctx(), response.id, active);
    if tab.is_dirty() {
        a11y::described(ui.ctx(), response.id, "Unsaved changes");
    }

    let close_rect =
        Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(22.0, 22.0));
    let close = ui.interact(
        close_rect,
        ui.id().with(("tab_close", index)),
        Sense::click(),
    );
    a11y::label(&close, WidgetType::Button, format!("Close {title}"));

    let painter = ui.painter();
    let fill = if active {
        palette.chrome
    } else {
        palette.title
    };
    painter.rect_filled(
        rect,
        CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        fill,
    );

    let text_color = if active {
        palette.text
    } else {
        palette.text_dim
    };
    painter.text(
        pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        &title,
        FontId::proportional(13.0),
        text_color,
    );

    // An unsaved change shows as a dot where the close button sits, the way a
    // tab strip usually reports it (`docs/ui-reference.md` §1).
    // The dot gives way to the close glyph while the pointer is on the button, so the
    // two never overlap and a dirty tab can still be closed by what it looks like.
    if tab.is_dirty() && !close.hovered() {
        painter.circle_filled(close_rect.center(), 3.0, palette.text_dim);
    }

    if (active || response.hovered()) && (!tab.is_dirty() || close.hovered()) {
        if close.hovered() {
            painter.rect_filled(close_rect, CornerRadius::same(4), palette.hover);
        }
        icons::paint_centered(ui, close_rect.center(), "close", 12.0, palette.text_dim);
    }

    // Right-click raises the tab's context menu: the registry's item set, drawn
    // with the same rows as the menu bar, so the strip cannot offer something the
    // menus do not already carry (`NC-T003`, `docs/ui-reference.md` §11).
    let _ = response.context_menu(|ui| {
        for item in tab_menu {
            match item {
                MenuItem::Separator => {
                    let _ = ui.separator();
                }
                MenuItem::Command {
                    spec,
                    accelerator,
                    enabled,
                    checked,
                } => {
                    if menu_bar::command_button(
                        ui,
                        spec.label,
                        *checked,
                        accelerator.as_deref(),
                        *enabled,
                        palette,
                    ) {
                        *chosen = Some(spec.id);
                        ui.close();
                    }
                }
                // The tab menu is a flat list; a submenu would need the menu
                // bar's own renderer, and nothing here is one.
                MenuItem::Submenu { .. } => {}
            }
        }
    });

    TabHit {
        activate: response.clicked() && !close.hovered(),
        close: close.clicked(),
        menu: response.secondary_clicked(),
        drag_started: response.drag_started_by(PointerButton::Primary) && !close.hovered(),
        drag_stopped: response.drag_stopped(),
    }
}
