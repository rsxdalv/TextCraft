//! The top bar: the `File`/`Edit`/`View` menus, the formatting toolbar, and the
//! account / settings controls.

use egui::{Align, Color32, Context, Event, Key, Layout, Modifiers, RichText, WidgetType};

use crate::ui::state::{ParagraphStyle, Shell};
use crate::ui::theme::Palette;
use crate::ui::{a11y, actions, icons, spacing, widgets};
use nc_commands::{CommandId, MenuItem, all_disabled, build_menu_bar, by_id};

/// The height of the top bar, in points.
pub const HEIGHT: f32 = 34.0;

/// Width of the control that holds the formatting toolbar when it does not fit.
const OVERFLOW_WIDTH: f32 = 30.0;

/// The widths the bar's groups took the last time each was drawn inline, kept in
/// the context so the next frame can decide whether the formatting group still fits
/// before drawing it. A group not yet measured is assumed to fit.
#[derive(Clone, Copy, Default)]
struct Measured {
    format: f32,
    right: f32,
}

fn measured_id() -> egui::Id {
    egui::Id::new("nc_top_bar_widths")
}

/// Whether a group of `natural` width fits in `available`, by `craft-ui`'s toolbar rule.
fn fits(natural: f32, available: f32) -> bool {
    if natural <= 0.0 {
        return true;
    }
    craft_ui::layout::fit_toolbar(&[natural], available, spacing::SM, OVERFLOW_WIDTH)
        .is_some_and(|fit| fit.overflow.is_empty())
}

/// Draws the top bar.
///
/// When the window is too narrow for the formatting group, it moves into a `»`
/// menu rather than being clipped or dropped, so every command stays reachable.
pub fn show(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context) {
    let measured: Measured = ctx
        .data(|data| data.get_temp(measured_id()))
        .unwrap_or_default();
    let mut now = measured;

    let bar = ui.max_rect();
    ui.painter().hline(
        bar.x_range(),
        bar.bottom() - 0.5,
        egui::Stroke::new(1.0, shell.palette.line),
    );

    ui.horizontal_centered(|ui| {
        ui.add_space(spacing::XS);
        let palette = shell.palette;
        menus(shell, ui, ctx, palette);
        // `Settings ▸ Formatting` off takes the formatting half of the bar away, which is
        // the whole of what that switch's acceptance row asks for (`NC-P005`). The divider
        // goes with it: it is the seam between the menus and this group, and a divider to
        // nothing is a border around an absence. What the setting does *not* do is recorded
        // in `docs/ui-reference.md` §4 — the accelerators keep formatting, because nothing
        // in the capture shows a formatting key that has stopped.
        if shell.formatting {
            let available = ui.available_width() - measured.right - spacing::MD;
            if fits(measured.format, available) {
                // Centred in the window, unless the menus on the left leave no room
                // for that, in which case it sits right after them.
                let centred = bar.center().x - measured.format / 2.0;
                let gap = (centred - ui.cursor().left()).max(spacing::MD);
                ui.add_space(gap.min((available - measured.format).max(0.0)));
                let start = ui.cursor().left();
                format_toolbar(shell, ui, ctx, palette);
                now.format = ui.cursor().left() - start;
            } else {
                ui.add_space(spacing::MD);
                let more = ui.menu_button(icons::glyph("overflow", palette.text), |ui| {
                    ui.horizontal(|ui| format_toolbar(shell, ui, ctx, palette));
                });
                a11y::label(&more.response, WidgetType::ComboBox, "More formatting");
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(spacing::MD);
            let before = ui.available_width();
            right_toolbar(shell, ui, ctx, palette);
            now.right = before - ui.available_width();
        });
    });

    if now.format != measured.format || now.right != measured.right {
        ctx.data_mut(|data| data.insert_temp(measured_id(), now));
    }
}

/// The `File` / `Edit` / `View` menu buttons.
///
/// Two behaviours the stock `egui` menu button does not have, both expected of a
/// desktop menu bar:
///
/// - **Anchored below.** A dropdown always opens directly under its button. When the
///   window is too short for the whole list the list scrolls; it is never flipped to
///   open above the bar or slid up over it.
/// - **Hover switching.** Once one menu is open, moving the pointer over a neighbouring
///   button opens that one instead (`File` → `Edit` → `View`) without another click.
fn menus(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let models = build_menu_bar(shell.availability());
    let marks = mnemonics(models.iter().map(|model| model.menu.label()));
    let show_keys = access_keys_visible(ctx);
    let bar_bottom = ui.max_rect().bottom();

    // Every button first, so hover switching can see all of them before any popup is drawn.
    let saved_gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, 4.0);
    let mut buttons = Vec::with_capacity(models.len());
    for (model, mark) in models.iter().zip(&marks) {
        let id = egui::Id::new(("nc_menu_button", model.menu.label()));
        let open = egui::Popup::is_id_open(ctx, id);
        let response = ui.add(
            egui::Button::new(access_text(ui, model.menu.label(), *mark, show_keys))
                .min_size(egui::vec2(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT))
                .selected(open),
        );
        buttons.push(response);
    }
    ui.spacing_mut().item_spacing.x = saved_gap;

    let popup_id = |index: usize| egui::Popup::default_response_id(&buttons[index]);
    let open_index = (0..buttons.len()).find(|&i| egui::Popup::is_id_open(ctx, popup_id(i)));

    // Alt+letter opens the menu whose access key it is.
    if let Some(letter) = alt_letter(ctx) {
        let target = models
            .iter()
            .zip(&marks)
            .position(|(model, mark)| mark_matches(model.menu.label(), *mark, letter));
        if let Some(target) = target {
            if let Some(open) = open_index {
                egui::Popup::close_id(ctx, popup_id(open));
            }
            egui::Popup::open_id(ctx, popup_id(target));
            set_keyboard_menu(ctx, true);
        }
    } else if let Some(open) = open_index {
        // Hover switching.
        for (index, response) in buttons.iter().enumerate() {
            if index != open && response.hovered() {
                egui::Popup::close_id(ctx, popup_id(open));
                egui::Popup::open_id(ctx, popup_id(index));
            }
        }
    } else {
        set_keyboard_menu(ctx, false);
    }

    for (response, model) in buttons.iter().zip(&models) {
        // Room between the button and the bottom of the window, so a long menu in a
        // short window scrolls instead of being moved up over its own button.
        let room = (ctx.content_rect().bottom() - bar_bottom - spacing::MD).max(80.0);
        // Anchored to the button stretched down to the bar's lower edge, so the list
        // starts flush under the bar and not partway up it.
        let anchor = egui::Rect::from_min_max(
            response.rect.min,
            egui::pos2(response.rect.max.x, bar_bottom),
        );
        let _ = egui::Popup::menu(response)
            .anchor(anchor)
            .align(egui::RectAlign::BOTTOM_START)
            .align_alternatives(&[])
            .show(|ui| {
                menu_style(ui, palette);
                ui.set_max_height(room);
                // A solid scroll bar takes its own column instead of floating over the
                // items' right edge.
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                egui::ScrollArea::vertical()
                    .max_height(room)
                    .auto_shrink([true, true])
                    .show(ui, |ui| menu_items(ui, &model.items, shell, ctx, palette));
            });
    }
}

/// The access key of each label: the first letter no earlier label already took, as a
/// byte index into the label, or `None` when every letter is taken.
///
/// Assigned from the labels themselves rather than listed, so a new command or a
/// relabelled one gets a working key without anyone maintaining a table.
fn mnemonics<'a>(labels: impl IntoIterator<Item = &'a str>) -> Vec<Option<usize>> {
    let mut taken: Vec<char> = Vec::new();
    labels
        .into_iter()
        .map(|label| {
            let found = label
                .char_indices()
                .find(|(_, c)| c.is_alphabetic() && !taken.contains(&c.to_ascii_lowercase()));
            found.map(|(index, c)| {
                taken.push(c.to_ascii_lowercase());
                index
            })
        })
        .collect()
}

/// Whether `letter` is the access key `mark` picked out of `label`.
fn mark_matches(label: &str, mark: Option<usize>, letter: char) -> bool {
    mark.and_then(|index| label[index..].chars().next())
        .is_some_and(|c| c.eq_ignore_ascii_case(&letter))
}

/// The key id for "a menu was opened from the keyboard", which keeps the access-key
/// underlines on screen after Alt is let go, as Windows does.
fn keyboard_menu_id() -> egui::Id {
    egui::Id::new("nc_keyboard_menu")
}

fn set_keyboard_menu(ctx: &Context, on: bool) {
    ctx.data_mut(|data| data.insert_temp(keyboard_menu_id(), on));
}

/// Whether access-key underlines are drawn: while Alt is held, and while a menu opened
/// by its access key is still open.
fn access_keys_visible(ctx: &Context) -> bool {
    ctx.input(|input| input.modifiers.alt)
        || ctx
            .data(|data| data.get_temp::<bool>(keyboard_menu_id()))
            .unwrap_or(false)
}

/// The letter of an Alt+letter press this frame, taking the press so nothing else sees it.
fn alt_letter(ctx: &Context) -> Option<char> {
    ctx.input_mut(|input| {
        let found = input.events.iter().find_map(|event| match event {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.alt && !modifiers.ctrl && !modifiers.command => {
                letter_of(*key).map(|c| (*key, c))
            }
            _ => None,
        });
        found.map(|(key, c)| {
            input.consume_key(Modifiers::ALT, key);
            c
        })
    })
}

/// The letter typed this frame with no modifier, while a menu is open.
fn typed_letter(ctx: &Context) -> Option<(Key, char)> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.alt && !modifiers.ctrl && !modifiers.command => {
                letter_of(*key).map(|c| (*key, c))
            }
            _ => None,
        })
    })
}

/// The letter a key types, if it is a letter key.
fn letter_of(key: Key) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// A label with its access key underlined, when keys are being shown.
fn access_text(ui: &egui::Ui, label: &str, mark: Option<usize>, show: bool) -> egui::WidgetText {
    let Some(index) = mark.filter(|_| show) else {
        return label.into();
    };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let plain = egui::TextFormat::simple(font.clone(), Color32::PLACEHOLDER);
    let underlined = egui::TextFormat {
        underline: egui::Stroke::new(1.0, Color32::PLACEHOLDER),
        ..plain.clone()
    };
    let width = label[index..].chars().next().map_or(0, char::len_utf8);
    let mut job = egui::text::LayoutJob::default();
    job.append(&label[..index], 0.0, plain.clone());
    job.append(&label[index..index + width], 0.0, underlined);
    job.append(&label[index + width..], 0.0, plain);
    job.into()
}

/// The look of an open menu: rows 29 pt tall with 11 pt of side padding, 4 pt between
/// them, a soft fill on the hovered row and nothing else, and hairline separators.
/// Matched to the Windows 11 Notepad menus.
fn menu_style(ui: &mut egui::Ui, palette: Palette) {
    let hover = palette.popup.lerp_to_gamma(palette.hover, 0.5);
    let style = ui.style_mut();
    style.spacing.button_padding = egui::vec2(11.0, 6.0);
    style.spacing.item_spacing.y = 4.0;
    for widget in [
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.bg_stroke = egui::Stroke::NONE;
        widget.weak_bg_fill = hover;
        widget.bg_fill = hover;
        widget.corner_radius = egui::CornerRadius::same(5);
    }
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, palette.popup.lerp_to_gamma(palette.border, 0.6));
}

/// The minimum size of a top-level menu button: a target generous enough to hit
/// without aiming, and wide enough that `File`, `Edit` and `View` read as a set.
const MENU_BUTTON_WIDTH: f32 = 40.0;
const MENU_BUTTON_HEIGHT: f32 = 26.0;

/// Renders one dropdown's items.
fn menu_items(
    ui: &mut egui::Ui,
    items: &[MenuItem],
    shell: &mut Shell,
    ctx: &Context,
    palette: Palette,
) {
    let marks = mnemonics(items.iter().filter_map(|item| match item {
        MenuItem::Command { spec, .. } => Some(spec.label),
        MenuItem::Submenu { title, .. } => Some(*title),
        MenuItem::Separator => None,
    }));
    let show_keys = access_keys_visible(ctx);
    let typed = typed_letter(ctx);
    // Once any row in a menu is a toggle, every row reserves the check column, so the
    // labels line up whether or not their own row is ticked.
    let reserve = items.iter().any(|item| {
        matches!(
            item,
            MenuItem::Command {
                checked: Some(_),
                ..
            }
        )
    });
    let blank = || icons::glyph("check", Color32::TRANSPARENT);
    let mut next_mark = marks.iter();
    for item in items {
        match item {
            MenuItem::Separator => {
                ui.add(egui::Separator::default().spacing(0.0));
            }
            MenuItem::Command {
                spec,
                accelerator,
                enabled,
                checked,
            } => {
                let mark = next_mark.next().copied().flatten();
                let pressed = typed
                    .is_some_and(|(_, letter)| *enabled && mark_matches(spec.label, mark, letter));
                if command_button_with_access(
                    ui,
                    spec.label,
                    *checked,
                    accelerator.as_deref(),
                    *enabled,
                    palette,
                    RowStyle {
                        access: mark.filter(|_| show_keys),
                        reserve_check: reserve,
                    },
                ) || pressed
                {
                    if let Some((key, _)) = typed.filter(|_| pressed) {
                        ctx.input_mut(|input| input.consume_key(Modifiers::NONE, key));
                    }
                    actions::run(shell, ctx, spec.id);
                    ui.close();
                }
            }
            MenuItem::Submenu { title, items } => {
                next_mark.next();
                // Recent opens even with nothing in it, because what it then says
                // is the point (`recent_submenu`); every other submenu whose
                // entries are all unavailable stays shut rather than opening onto
                // a list nobody can act on.
                if is_recent_submenu(items) {
                    recent_submenu(title, shell, ui, items, ctx, palette);
                } else if all_disabled(items) {
                    let button = if reserve {
                        egui::Button::new((blank(), *title))
                    } else {
                        egui::Button::new(*title)
                    };
                    let _ = ui.add_enabled(false, button);
                } else {
                    let body = |ui: &mut egui::Ui| menu_items(ui, items, shell, ctx, palette);
                    let _ = if reserve {
                        ui.menu_button((blank(), *title), body)
                    } else {
                        ui.menu_button(*title, body)
                    };
                }
            }
        }
    }
}

/// Whether this submenu is the one the registry defines for recent documents —
/// the one holding `cmd.file.recent_empty`, whose placeholder text the real list
/// stands in for. Recognized by its command rather than its label, so relabelling
/// the menu cannot leave the list unreachable.
fn is_recent_submenu(items: &[MenuItem]) -> bool {
    items
        .iter()
        .any(|item| item.command().map(|spec| spec.id.as_str()) == Some("cmd.file.recent_empty"))
}

/// `File ▸ Recent`: the documents opened this session, newest first, each of which
/// opens again on click (`NC-F010`). The `title` is the registry's own, so the
/// submenu is labelled whatever the menu model says it should be.
///
/// The list is read out before any of it is acted on, because opening a file
/// rewrites it and an entry may move to the front. When there is nothing to list
/// the submenu still opens, onto the rows the registry resolved for it —
/// `cmd.file.recent_empty`, "No recent files". A submenu the registry says exists
/// but which will not open is a dead surface; the empty-state message the registry
/// wrote is the thing to show instead of a title that looks broken.
fn recent_submenu(
    title: &'static str,
    shell: &mut Shell,
    ui: &mut egui::Ui,
    when_empty: &[MenuItem],
    ctx: &Context,
    palette: Palette,
) {
    let paths = shell.recent_files.clone();
    let _ = ui.menu_button(title, |ui| {
        if paths.is_empty() {
            menu_items(ui, when_empty, shell, ctx, palette);
            return;
        }
        for path in &paths {
            let name = path.file_name().map_or_else(
                || path.to_string_lossy().into_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
            let button = egui::Button::new(name).min_size(egui::vec2(210.0, 0.0));
            if ui
                .add(button)
                .on_hover_text(path.display().to_string())
                .clicked()
            {
                actions::open_path(shell, path, None);
                ui.close();
            }
        }
    });
}

/// One menu row: the registry's label, its accelerator right-aligned, greyed
/// when the command's availability rule says so — and whether it was chosen.
///
/// Shared with the tab context menu (`title_bar`) so the two surfaces cannot
/// drift apart in what a row looks like or when it is clickable.
pub(crate) fn command_button(
    ui: &mut egui::Ui,
    label: &str,
    checked: Option<bool>,
    accelerator: Option<&str>,
    enabled: bool,
    palette: Palette,
) -> bool {
    let row = RowStyle::default();
    command_button_with_access(ui, label, checked, accelerator, enabled, palette, row)
}

/// How a menu row differs from the plain one.
#[derive(Clone, Copy, Default)]
struct RowStyle {
    /// Byte index of the access key to underline in the label.
    access: Option<usize>,
    /// Leave room for the check column even when this row has no check of its own.
    reserve_check: bool,
}

/// [`command_button`] with an access key to underline and an optional check column.
fn command_button_with_access(
    ui: &mut egui::Ui,
    label: &str,
    checked: Option<bool>,
    accelerator: Option<&str>,
    enabled: bool,
    palette: Palette,
    row: RowStyle,
) -> bool {
    let RowStyle {
        access,
        reserve_check,
    } = row;
    let text = access_text(ui, label, access, access.is_some());
    // A toggle row always carries the check column, drawn invisibly when unchecked, so
    // ticking an item never moves its label.
    let mut button = match checked {
        Some(on) => {
            let tint = if on {
                ui.visuals().text_color()
            } else {
                Color32::TRANSPARENT
            };
            egui::Button::new((icons::glyph("check", tint), text))
        }
        None if reserve_check => {
            egui::Button::new((icons::glyph("check", Color32::TRANSPARENT), text))
        }
        None => egui::Button::new(text),
    }
    .min_size(egui::vec2(240.0, 0.0));
    if let Some(accel) = accelerator {
        button = button.shortcut_text(RichText::new(accel).size(12.5).color(palette.text_dim));
    }
    ui.add_enabled(enabled, button).clicked()
}

/// The formatting controls in the middle of the bar.
fn format_toolbar(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    paragraph_menu(shell, ui, ctx, palette);
    list_menu(shell, ui, ctx, palette);

    let bold = widgets::rich_button(ui, RichText::new("B").strong()).on_hover_text("Bold");
    a11y::label(&bold, WidgetType::Button, "Bold");
    if bold.clicked() {
        actions::run(shell, ctx, CommandId("cmd.md.bold"));
    }
    let italic = widgets::rich_button(ui, RichText::new("I").italics()).on_hover_text("Italic");
    a11y::label(&italic, WidgetType::Button, "Italic");
    if italic.clicked() {
        actions::run(shell, ctx, CommandId("cmd.md.italic"));
    }
    let strike =
        widgets::rich_button(ui, RichText::new("S").strikethrough()).on_hover_text("Strikethrough");
    a11y::label(&strike, WidgetType::Button, "Strikethrough");
    if strike.clicked() {
        actions::run(shell, ctx, CommandId("cmd.md.strike"));
    }
    let link = icons::button(ui, "Insert a link", "link", palette).on_hover_text("Insert a link");
    a11y::label(&link, WidgetType::Button, "Insert a link");
    if link.clicked() {
        actions::run(shell, ctx, CommandId("cmd.md.link"));
    }
    let table =
        icons::button(ui, "Insert a table", "table", palette).on_hover_text("Insert a table");
    a11y::label(&table, WidgetType::Button, "Insert a table");
    if table.clicked() {
        actions::run(shell, ctx, CommandId("cmd.md.table"));
    }
    let clear = widgets::rich_button(ui, RichText::new("Aa")).on_hover_text("Clear all formatting");
    a11y::label(&clear, WidgetType::Button, "Clear all formatting");
    if clear.clicked() {
        actions::run(shell, ctx, CommandId("cmd.md.clear"));
    }
}

/// The `H1` paragraph-style dropdown.
///
/// The item list is the registry's, in §3's order, and choosing one fires its
/// command rather than setting the style here: the dropdown, the menu, and the
/// shortcut have to write the same bytes, and only `nc-markdown` decides what a
/// style is.
fn paragraph_menu(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let current = shell
        .active_tab()
        .map(|tab| tab.paragraph)
        .unwrap_or(ParagraphStyle::Body);
    let menu = ui.menu_button(
        ("H1", icons::glyph("chevron-down", palette.text_dim)),
        |ui| {
            for style in ParagraphStyle::ALL {
                if ui
                    .selectable_label(style == current, style.label())
                    .clicked()
                {
                    actions::run(shell, ctx, style.command());
                    ui.close();
                }
            }
        },
    );
    // The visible label is the current style; what the button is for is the
    // paragraph style as a whole (`NC-P007`).
    a11y::label(&menu.response, WidgetType::ComboBox, "Paragraph style");
}

/// The bulleted / numbered list dropdown (`docs/ui-reference.md` §3: *Bulleted
/// list · Numbered list · Increase indent · Decrease indent, indent items greyed
/// when they cannot apply*).
///
/// All four are the registry's commands, drawn with the registry's labels, so the
/// dropdown cannot offer something the menu does not. What the registry can answer
/// for "cannot apply" today is the document kind — these are Markdown-only
/// commands — and that is what gates them. Whether a particular line has an indent
/// to take away is not a rule `Availability` models, and this does not pretend to
/// know it.
fn list_menu(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let context = shell.availability();
    let menu = ui.menu_button(
        (
            icons::glyph("list", palette.text),
            icons::glyph("chevron-down", palette.text_dim),
        ),
        |ui| {
            for id in [
                "cmd.md.bullets",
                "cmd.md.numbering",
                "cmd.md.indent_increase",
                "cmd.md.indent_decrease",
            ] {
                let Some(spec) = by_id(id) else {
                    continue;
                };
                let enabled = spec.is_available(context);
                if ui
                    .add_enabled(enabled, egui::Button::new(spec.label))
                    .clicked()
                {
                    actions::run(shell, ctx, spec.id);
                    ui.close();
                }
            }
        },
    );
    a11y::label(&menu.response, WidgetType::ComboBox, "Lists");
}

/// The sparkle / feedback / account / settings controls on the right.
fn right_toolbar(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let settings = widgets::icon_button(ui, "⚙").on_hover_text("Settings");
    a11y::label(&settings, WidgetType::Button, "Settings");
    if settings.clicked() {
        actions::run(shell, ctx, CommandId("cmd.app.settings"));
    }
    avatar(ui, palette);
    let feedback =
        icons::button(ui, "Send feedback", "chat", palette).on_hover_text("Send feedback");
    a11y::label(&feedback, WidgetType::Button, "Send feedback");

    // The AI ("sparkle") menu. Its entries are service-gated, so they show
    // greyed until the service is configured.
    let context = shell.availability();
    let ai = ui.menu_button(
        (
            icons::glyph("sparkle", palette.text),
            icons::glyph("chevron-down", palette.text_dim),
        ),
        |ui| {
            for id in ["cmd.ai.rewrite", "cmd.ai.summarize", "cmd.ai.write"] {
                if let Some(spec) = by_id(id) {
                    let enabled = spec.is_available(context);
                    if ui
                        .add_enabled(enabled, egui::Button::new(spec.label))
                        .clicked()
                    {
                        ui.close();
                    }
                }
            }
        },
    );
    a11y::label(&ai.response, WidgetType::ComboBox, "AI");
}

/// The round account avatar.
fn avatar(ui: &mut egui::Ui, palette: Palette) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    a11y::label(&response, WidgetType::Button, "Account");
    ui.painter()
        .circle_filled(rect.center(), 13.0, palette.border);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "👤",
        egui::FontId::proportional(13.0),
        palette.text,
    );
    let _ = response.on_hover_text("Account");
}

#[cfg(test)]
mod access_key_tests {
    use super::*;

    #[test]
    fn each_label_takes_the_first_letter_nobody_else_has() {
        assert_eq!(
            mnemonics(["File", "Edit", "View"]),
            [Some(0), Some(0), Some(0)]
        );
        // `Save` and `Save as` and `Select all`: the later ones move on.
        assert_eq!(
            mnemonics(["Save", "Save as", "Select all"]),
            [Some(0), Some(1), Some(1)]
        );
        // A label with nothing left has no key.
        assert_eq!(mnemonics(["Aa", "A"]), [Some(0), None]);
    }

    #[test]
    fn a_key_matches_its_letter_in_either_case() {
        assert!(mark_matches("Edit", Some(0), 'e'));
        assert!(!mark_matches("Edit", Some(0), 'f'));
        assert!(!mark_matches("Edit", None, 'e'));
    }
}
