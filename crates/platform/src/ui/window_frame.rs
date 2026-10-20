//! What the operating system would draw around a borderless window: resize edges
//! and corners, and a hairline outline.
//!
//! The window has no OS decorations (the tab strip and caption buttons share one
//! row), so nothing else gives it a resize border. Thin invisible zones along each
//! edge and corner start an OS resize in their direction, which keeps the native
//! behaviour: snapping, aero-shake, multi-monitor DPI changes.

use egui::{
    Context, CursorIcon, Id, Order, PointerButton, Rect, ResizeDirection, Sense, Stroke,
    StrokeKind, ViewportCommand, pos2, vec2,
};

use crate::ui::theme::Palette;

/// Thickness of an edge zone, in points. A little under a native border, so the
/// zones trim as little of the tab strip and caption buttons as possible.
const EDGE: f32 = 6.0;
/// Side of a corner zone: larger than an edge, because a corner is harder to hit.
const CORNER: f32 = 12.0;

/// Whether the window fills the screen or its monitor, where it has nothing to resize.
fn locked(ctx: &Context) -> bool {
    ctx.input(|i| {
        let viewport = i.viewport();
        viewport.maximized.unwrap_or(false) || viewport.fullscreen.unwrap_or(false)
    })
}

/// The eight zones of a window with content rect `w`: rect, direction, cursor.
/// Corners come last so they win where they overlap an edge.
fn zones(w: Rect) -> [(Rect, ResizeDirection, CursorIcon); 8] {
    use ResizeDirection::*;
    let (e, c) = (EDGE, CORNER);
    let (l, t, r, b) = (w.left(), w.top(), w.right(), w.bottom());
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
    [
        (rect(l + c, t, r - c, t + e), North, CursorIcon::ResizeNorth),
        (rect(l + c, b - e, r - c, b), South, CursorIcon::ResizeSouth),
        (rect(l, t + c, l + e, b - c), West, CursorIcon::ResizeWest),
        (rect(r - e, t + c, r, b - c), East, CursorIcon::ResizeEast),
        (rect(l, t, l + c, t + c), NorthWest, CursorIcon::ResizeNwSe),
        (rect(r - c, t, r, t + c), NorthEast, CursorIcon::ResizeNeSw),
        (rect(l, b - c, l + c, b), SouthWest, CursorIcon::ResizeNeSw),
        (rect(r - c, b - c, r, b), SouthEast, CursorIcon::ResizeNwSe),
    ]
}

fn os_move_id() -> Id {
    Id::new("nc_os_move_in_progress")
}

/// Records that an OS-driven move or resize was just started.
///
/// While the OS runs it, the mouse events — including the button release — go to the
/// OS and never reach `egui`, which would otherwise keep believing the button is down
/// and never start another drag. [`release_after_os_move`] puts the release back.
pub fn mark_os_move(ctx: &Context) {
    ctx.data_mut(|data| data.insert_temp(os_move_id(), true));
}

/// Gives `egui` the button release the OS swallowed, at the first pointer movement
/// after the move or resize ended. Called from the app's raw-input hook.
pub fn release_after_os_move(ctx: &Context, raw: &mut egui::RawInput) {
    if !ctx
        .data(|data| data.get_temp::<bool>(os_move_id()))
        .unwrap_or(false)
    {
        return;
    }
    let Some(pos) = raw.events.iter().find_map(|event| match event {
        egui::Event::PointerMoved(pos) => Some(*pos),
        _ => None,
    }) else {
        return;
    };
    raw.events.insert(
        0,
        egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    ctx.data_mut(|data| data.insert_temp(os_move_id(), false));
}

/// Draws the outline and handles the resize zones. Called last in a frame, so the
/// zones sit above everything else.
pub fn show(ctx: &Context, palette: Palette) {
    if locked(ctx) {
        return;
    }
    let window = ctx.content_rect();
    if window.width() < 4.0 * CORNER || window.height() < 4.0 * CORNER {
        return;
    }

    ctx.layer_painter(egui::LayerId::new(
        Order::Foreground,
        Id::new("nc_window_outline"),
    ))
    .rect_stroke(
        window,
        0.0,
        Stroke::new(1.0, palette.border),
        StrokeKind::Inside,
    );

    for (index, (rect, direction, cursor)) in zones(window).into_iter().enumerate() {
        egui::Area::new(Id::new(("nc_resize_zone", index)))
            .order(Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(ctx, |ui| {
                let response = ui.interact(
                    Rect::from_min_size(rect.min, vec2(rect.width(), rect.height())),
                    ui.id().with("grab"),
                    Sense::DRAG,
                );
                if response.hovered() || response.dragged() {
                    ctx.set_cursor_icon(cursor);
                }
                if response.drag_started_by(PointerButton::Primary) {
                    mark_os_move(ctx);
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_zones_stay_inside_the_window_and_none_swallow_it() {
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let zones = zones(window);
        assert_eq!(zones.len(), 8);
        for (rect, _, _) in zones {
            assert!(window.contains_rect(rect), "{rect:?} leaves the window");
            assert!(rect.width() <= CORNER * 2.0 || rect.height() <= CORNER * 2.0);
        }
        // The middle of the window belongs to the app, not to a resize zone.
        let middle = window.center();
        assert!(zones.iter().all(|(rect, _, _)| !rect.contains(middle)));
    }
}
