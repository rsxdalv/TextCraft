//! The top bar: the `File`/`Edit`/`View` menus, the formatting toolbar, and the
//! account / settings controls.

use egui::{
    Align, Color32, Context, Event, Key, Layout, Modifiers, RichText, StrokeKind, WidgetType,
};

use crate::ui::fonts::FAMILIES;
use crate::ui::state::{
    AppLanguage, AppTheme, DocKind, EditorMode, MarkdownViewMode, ParagraphStyle, Shell, TextColor,
};
use crate::ui::theme::Palette;
use crate::ui::{a11y, actions, i18n, icons, spacing, widgets};
use nc_commands::{CommandId, MenuItem, all_disabled, build_menu_bar, by_id};

/// The height of the top bar, in points.
pub const HEIGHT: f32 = 34.0;

/// Width of the control that holds the formatting toolbar when it does not fit.
const OVERFLOW_WIDTH: f32 = 30.0;

/// Number of semantic control groups in the central formatting toolbar.
const GROUP_COUNT: usize = 5;

/// Width of the vertical hairline divider plus spacing between groups.
const GROUP_GAP: f32 = 9.0;

/// The widths the bar's groups took the last time each was drawn inline, kept in
/// the context so the next frame can decide which groups fit inline before drawing them.
#[derive(Clone, Copy, PartialEq)]
struct Measured {
    groups: [f32; GROUP_COUNT],
    right: f32,
}

impl Default for Measured {
    fn default() -> Self {
        Self {
            groups: [210.0, 146.0, 110.0, 136.0, 112.0],
            right: 190.0,
        }
    }
}

fn measured_id(mode: EditorMode) -> egui::Id {
    egui::Id::new(("nc_top_bar_widths", mode == EditorMode::Code))
}

/// Computes how many of the `GROUP_COUNT` toolbar groups fit in `available`.
fn fit_groups(groups: &[f32; GROUP_COUNT], available: f32) -> (usize, bool) {
    let total: f32 =
        groups.iter().sum::<f32>() + GROUP_GAP * (GROUP_COUNT.saturating_sub(1) as f32);
    if total <= available {
        return (GROUP_COUNT, false);
    }
    if let Some(fit) =
        craft_ui::layout::fit_toolbar(groups, available.max(0.0), GROUP_GAP, OVERFLOW_WIDTH)
    {
        (fit.shown.len(), !fit.overflow.is_empty())
    } else {
        (0, true)
    }
}

/// Draws the top bar.
///
/// When the window is too narrow for all formatting groups at once, smart compacting
/// shrinks labels first and then moves only the rightmost overflowing groups into the
/// `...` (`More formatting`) menu so primary formatting icons stay visible on the bar.
pub fn show(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context) {
    let mode_for_measure = shell.editor_mode;
    let measured: Measured = ctx
        .data(|data| data.get_temp(measured_id(mode_for_measure)))
        .unwrap_or_default();
    let mut now = measured;

    let bar = ui.max_rect();
    let compact = bar.width() < 1320.0;
    let icon_only_theme = bar.width() < 1100.0;
    ui.painter().hline(
        bar.x_range(),
        bar.bottom() - 0.5,
        egui::Stroke::new(1.0, shell.palette.line),
    );

    ui.horizontal_centered(|ui| {
        ui.add_space(spacing::XS);
        let palette = shell.palette;
        menus(shell, ui, ctx, palette, compact);
        // `Settings ▸ Formatting` off takes the formatting half of the bar away, which is
        // the whole of what that switch's acceptance row asks for (`NC-P005`). The divider
        // goes with it: it is the seam between the menus and this group, and a divider to
        // nothing is a border around an absence. What the setting does *not* do is recorded
        // in `docs/ui-reference.md` §4 — the accelerators keep formatting, because nothing
        // in the capture shows a formatting key that has stopped.
        if shell.formatting {
            let side_pad = if compact { spacing::SM } else { spacing::MD };
            let available = (ui.available_width() - measured.right - side_pad).max(0.0);
            let (shown_count, has_overflow) = fit_groups(&measured.groups, available);
            let inline_width: f32 = measured.groups[..shown_count].iter().sum::<f32>()
                + GROUP_GAP * (shown_count.saturating_sub(1) as f32)
                + if has_overflow {
                    OVERFLOW_WIDTH + if shown_count > 0 { GROUP_GAP } else { 0.0 }
                } else {
                    0.0
                };

            let centred = bar.center().x - inline_width / 2.0;
            let gap = (centred - ui.cursor().left()).max(side_pad);
            ui.add_space(
                gap.min((available - inline_width).max(0.0))
                    .max(spacing::XS),
            );

            let saved_gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, 2.0);
            for group_idx in 0..shown_count {
                if group_idx > 0 {
                    toolbar_divider(ui, palette);
                }
                let start = ui.cursor().left();
                draw_toolbar_group(group_idx, shell, ui, ctx, palette, compact);
                let w = ui.cursor().left() - start;
                if w > 0.0 {
                    now.groups[group_idx] = w;
                }
            }

            if has_overflow {
                if shown_count > 0 {
                    toolbar_divider(ui, palette);
                }
                let more = ui
                    .menu_button(icons::glyph("overflow", palette.text), |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 4.0);
                        for (idx, group_idx) in (shown_count..GROUP_COUNT).enumerate() {
                            if idx > 0 {
                                ui.add(egui::Separator::default().spacing(2.0));
                            }
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                draw_toolbar_group(group_idx, shell, ui, ctx, palette, false);
                            });
                        }
                    })
                    .response
                    .on_hover_text("More formatting");
                a11y::label(&more, WidgetType::ComboBox, "More formatting");
            }
            ui.spacing_mut().item_spacing.x = saved_gap;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(if compact { spacing::SM } else { spacing::MD });
            let before = ui.available_width();
            right_toolbar(shell, ui, ctx, palette, compact, icon_only_theme);
            now.right = before - ui.available_width();
        });
    });

    if now != measured {
        ctx.data_mut(|data| data.insert_temp(measured_id(mode_for_measure), now));
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
fn menus(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette, compact: bool) {
    let models = build_menu_bar(shell.availability());
    let labels: Vec<&'static str> = models
        .iter()
        .map(|model| i18n::tr(shell.language, model.menu.label()))
        .collect();
    let marks = mnemonics(labels.iter().copied());
    let show_keys = access_keys_visible(ctx);
    let bar_bottom = ui.max_rect().bottom();

    // Every button first, so hover switching can see all of them before any popup is drawn.
    let gap = if compact { 2.0 } else { 4.0 };
    let min_w = if compact { 30.0 } else { MENU_BUTTON_WIDTH };
    let saved_gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.x, gap);
    let saved_pad = ui.spacing().button_padding;
    if compact {
        ui.spacing_mut().button_padding = egui::vec2(5.0, 4.0);
    }
    let mut buttons = Vec::with_capacity(models.len());
    for (idx, (model, mark)) in models.iter().zip(&marks).enumerate() {
        let label = labels[idx];
        let id = egui::Id::new(("nc_menu_button", model.menu.label()));
        let open = egui::Popup::is_id_open(ctx, id);
        let response = ui.add(
            egui::Button::new(access_text(ui, label, *mark, show_keys))
                .min_size(egui::vec2(min_w, MENU_BUTTON_HEIGHT))
                .selected(open),
        );
        buttons.push(response);
    }
    ui.spacing_mut().button_padding = saved_pad;
    ui.spacing_mut().item_spacing.x = saved_gap;

    let popup_id = |index: usize| egui::Popup::default_response_id(&buttons[index]);
    let open_index = (0..buttons.len()).find(|&i| egui::Popup::is_id_open(ctx, popup_id(i)));

    // Alt+letter opens the menu whose access key it is.
    if let Some(letter) = alt_letter(ctx) {
        let target = labels
            .iter()
            .zip(&marks)
            .position(|(label, mark)| mark_matches(label, *mark, letter));
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
/// without aiming, and wide enough that `File`, `Edit`, `View`, `Insert`, and `Format` read as a set.
const MENU_BUTTON_WIDTH: f32 = 36.0;
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
        MenuItem::Command { spec, .. } => Some(i18n::tr(shell.language, spec.label)),
        MenuItem::Submenu { title, .. } => Some(i18n::tr(shell.language, title)),
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
                let label = i18n::tr(shell.language, spec.label);
                let pressed =
                    typed.is_some_and(|(_, letter)| *enabled && mark_matches(label, mark, letter));
                if command_button_with_access(
                    ui,
                    label,
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
                let title = i18n::tr(shell.language, title);
                // Recent opens even with nothing in it, because what it then says
                // is the point (`recent_submenu`); every other submenu whose
                // entries are all unavailable stays shut rather than opening onto
                // a list nobody can act on.
                if is_recent_submenu(items) {
                    recent_submenu(title, shell, ui, items, ctx, palette);
                } else if all_disabled(items) {
                    let button = if reserve {
                        egui::Button::new((blank(), title))
                    } else {
                        egui::Button::new(title)
                    };
                    let _ = ui.add_enabled(false, button);
                } else {
                    let body = |ui: &mut egui::Ui| menu_items(ui, items, shell, ctx, palette);
                    let _ = if reserve {
                        ui.menu_button((blank(), title), body)
                    } else {
                        ui.menu_button(title, body)
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
        ui.add(egui::Separator::default().spacing(0.0));
        let clear_text = i18n::tr(shell.language, "Clear recent files");
        let clear_button = egui::Button::new(clear_text).min_size(egui::vec2(210.0, 0.0));
        if ui.add(clear_button).clicked() {
            actions::run(shell, ctx, CommandId("cmd.file.clear_recent"));
            ui.close();
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

/// A vertical hairline divider between Ribbon control groups (`gui2`).
fn toolbar_divider(ui: &mut egui::Ui, palette: Palette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(5.0, 16.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, palette.line),
    );
}

/// Promotes an untitled tab to Markdown when a formatting toolbar action is triggered,
/// so formatting works immediately in Rich Text mode without requiring `.md` save first.
fn promote_untitled_to_markdown(shell: &mut Shell) {
    if let Some(tab) = shell.active_tab_mut() {
        if !tab.file.has_path() {
            tab.kind = DocKind::Markdown;
        }
    }
}

/// Draws one of the `GROUP_COUNT` semantic groups of the central toolbar.
fn draw_toolbar_group(
    group_idx: usize,
    shell: &mut Shell,
    ui: &mut egui::Ui,
    ctx: &Context,
    palette: Palette,
    compact: bool,
) {
    if shell.editor_mode == EditorMode::Code {
        match group_idx {
            0 => group_code_navigation(shell, ui, ctx, palette),
            1 => group_code_structure(shell, ui, ctx, palette),
            2 => group_code_options(shell, ui, ctx, palette),
            3 => group_typography(shell, ui, ctx, palette, compact),
            4 => group_view_mode(shell, ui, ctx, palette, compact),
            _ => {}
        }
        return;
    }
    match group_idx {
        0 => group_inline(shell, ui, ctx, palette),
        1 => group_blocks(shell, ui, ctx, palette),
        2 => group_insert(shell, ui, ctx, palette),
        3 => group_typography(shell, ui, ctx, palette, compact),
        4 => group_view_mode(shell, ui, ctx, palette, compact),
        _ => {}
    }
}

/// Code Editor Group 0: Search & Line Navigation (`Find`, `Replace`, `Go to Ln`).
fn group_code_navigation(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let lang = shell.language;
    let find_tip = format!("{} (Ctrl+F)", i18n::tr(lang, "Find"));
    let rep_tip = format!("{} (Ctrl+H)", i18n::tr(lang, "Replace"));
    let goto_tip = format!("{} (Ctrl+G)", i18n::tr(lang, "Go to line"));
    for (label, hover, cmd_id) in [
        (i18n::tr(lang, "Find"), find_tip.as_str(), "cmd.edit.find"),
        (
            i18n::tr(lang, "Replace"),
            rep_tip.as_str(),
            "cmd.edit.replace",
        ),
        ("Ln :", goto_tip.as_str(), "cmd.edit.go_to"),
    ] {
        let resp = ui
            .add(
                egui::Button::new(
                    RichText::new(label)
                        .monospace()
                        .size(11.0)
                        .color(palette.text),
                )
                .min_size(egui::vec2(38.0, 22.0)),
            )
            .on_hover_text(hover);
        a11y::label(&resp, WidgetType::Button, hover);
        if resp.clicked() {
            actions::run(shell, ctx, CommandId(cmd_id));
        }
    }
}

/// Code Editor Group 1: Indentation & Code Blocks (`<<`, `>>`, `</>`, `---`).
fn group_code_structure(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let lang = shell.language;
    let dec_tip = i18n::tr(lang, "Decrease indent");
    let outdent = ui
        .add(
            egui::Button::new(RichText::new("⇤").size(12.0).color(palette.text))
                .min_size(egui::vec2(24.0, 22.0)),
        )
        .on_hover_text(dec_tip);
    a11y::label(&outdent, WidgetType::Button, dec_tip);
    if outdent.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.indent_decrease"));
    }

    let inc_tip = i18n::tr(lang, "Increase indent");
    let indent = ui
        .add(
            egui::Button::new(RichText::new("⇥").size(12.0).color(palette.text))
                .min_size(egui::vec2(24.0, 22.0)),
        )
        .on_hover_text(inc_tip);
    a11y::label(&indent, WidgetType::Button, inc_tip);
    if indent.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.indent_increase"));
    }

    let code_label = i18n::tr(lang, "Code");
    let code_tip = i18n::tr(lang, "Wrap in code block");
    let code = icons::button(ui, code_label, "code", palette).on_hover_text(code_tip);
    a11y::label(&code, WidgetType::Button, code_label);
    if code.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.code"));
    }
}

/// Code Editor Group 2: Word Wrap & Workspace Sidebar toggles.
fn group_code_options(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let lang = shell.language;
    let wrap_active = shell.word_wrap;
    let wrap_text = i18n::tr(lang, "Wrap");
    let wrap_tip = i18n::tr(lang, "Toggle word wrap");
    let wrap_a11y = i18n::tr(lang, "Word wrap");
    let mut wrap_btn = egui::Button::new(RichText::new(wrap_text).monospace().size(11.0).color(
        if wrap_active {
            Color32::WHITE
        } else {
            palette.text
        },
    ))
    .min_size(egui::vec2(40.0, 22.0));
    if wrap_active {
        wrap_btn = wrap_btn.fill(palette.accent);
    }
    let wrap_resp = ui.add(wrap_btn).on_hover_text(wrap_tip);
    a11y::label(&wrap_resp, WidgetType::Button, wrap_a11y);
    if wrap_resp.clicked() {
        actions::run(shell, ctx, CommandId("cmd.view.word_wrap"));
    }
}

fn format_toggle_button(
    ui: &mut egui::Ui,
    label: RichText,
    active: bool,
    palette: Palette,
    tooltip: &str,
) -> egui::Response {
    let mut btn = egui::Button::new(if active {
        label.color(Color32::WHITE)
    } else {
        label.color(palette.text)
    })
    .min_size(egui::vec2(26.0, 26.0));
    if active {
        btn = btn.fill(palette.accent);
    }
    ui.add(btn).on_hover_text(tooltip)
}

fn highlight_toggle_button(
    ui: &mut egui::Ui,
    active: bool,
    palette: Palette,
    tooltip: &str,
) -> egui::Response {
    let glyph = icons::glyph(
        "highlight",
        if active { Color32::WHITE } else { palette.text },
    );
    let mut btn = egui::Button::new(glyph).min_size(egui::vec2(26.0, 26.0));
    if active {
        btn = btn.fill(palette.accent);
    }
    ui.add(btn).on_hover_text(tooltip)
}

/// Group 0: Paragraph style + inline formatting (`H1▾`, `B`, `I`, `U`, `S`, `Highlight`, `Text color`).
fn group_inline(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    paragraph_menu(shell, ui, ctx, palette);

    let (text, range) = shell
        .active_tab()
        .map(|tab| {
            let r = tab.document().selection().range();
            (
                tab.text(),
                nc_markdown::Range {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .unwrap_or(("", nc_markdown::Range { start: 0, end: 0 }));

    let is_bold = nc_markdown::is_inline_active(text, range, nc_markdown::Inline::Bold);
    let is_italic = nc_markdown::is_inline_active(text, range, nc_markdown::Inline::Italic);
    let is_underline = nc_markdown::is_inline_active(text, range, nc_markdown::Inline::Underline);
    let is_strike = nc_markdown::is_inline_active(text, range, nc_markdown::Inline::Strike);
    let is_highlight = nc_markdown::is_inline_active(text, range, nc_markdown::Inline::Highlight);

    let bold_tip = i18n::tr(shell.language, "Bold");
    let bold = format_toggle_button(ui, RichText::new("B").strong(), is_bold, palette, bold_tip);
    a11y::label(&bold, WidgetType::Button, bold_tip);
    if bold.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.bold"));
    }
    let italic_tip = i18n::tr(shell.language, "Italic");
    let italic = format_toggle_button(
        ui,
        RichText::new("I").italics(),
        is_italic,
        palette,
        italic_tip,
    );
    a11y::label(&italic, WidgetType::Button, italic_tip);
    if italic.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.italic"));
    }
    let underline_tip = i18n::tr(shell.language, "Underline");
    let underline = format_toggle_button(
        ui,
        RichText::new("U").underline(),
        is_underline,
        palette,
        underline_tip,
    );
    a11y::label(&underline, WidgetType::Button, underline_tip);
    if underline.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.underline"));
    }
    let strike_tip = i18n::tr(shell.language, "Strikethrough");
    let strike = format_toggle_button(
        ui,
        RichText::new("S").strikethrough(),
        is_strike,
        palette,
        strike_tip,
    );
    a11y::label(&strike, WidgetType::Button, strike_tip);
    if strike.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.strike"));
    }
    let highlight_tip = i18n::tr(shell.language, "Highlight");
    let highlight = highlight_toggle_button(ui, is_highlight, palette, highlight_tip);
    a11y::label(&highlight, WidgetType::Button, highlight_tip);
    if highlight.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.highlight"));
    }
    text_color_menu(shell, ui, ctx, palette);
}

/// Group 1: Lists & block formatting (`Lists▾`, `Task list`, `Blockquote`, `Code`, `Horizontal rule`).
fn group_blocks(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    list_menu(shell, ui, ctx, palette);
    let check_tip = i18n::tr(shell.language, "Task list");
    let checklist = icons::button(ui, check_tip, "checklist", palette).on_hover_text(check_tip);
    a11y::label(&checklist, WidgetType::Button, check_tip);
    if checklist.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.checklist"));
    }
    let quote_tip = i18n::tr(shell.language, "Blockquote");
    let quote = icons::button(ui, quote_tip, "quote", palette).on_hover_text(quote_tip);
    a11y::label(&quote, WidgetType::Button, quote_tip);
    if quote.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.quote"));
    }
    let code_tip = i18n::tr(shell.language, "Code");
    let code = icons::button(ui, code_tip, "code", palette).on_hover_text(code_tip);
    a11y::label(&code, WidgetType::Button, code_tip);
    if code.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.code"));
    }
    let hr_tip = i18n::tr(shell.language, "Horizontal rule");
    let hr = icons::button(ui, hr_tip, "hr", palette).on_hover_text(hr_tip);
    a11y::label(&hr, WidgetType::Button, hr_tip);
    if hr.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.hr"));
    }
}

/// Group 2: Insertions & Clear formatting (`Link`, `Image`, `Table`, `Aa`).
fn group_insert(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let link_tip = i18n::tr(shell.language, "Insert a link");
    let link = icons::button(ui, link_tip, "link", palette).on_hover_text(link_tip);
    a11y::label(&link, WidgetType::Button, link_tip);
    if link.clicked() {
        promote_untitled_to_markdown(shell);
        actions::open_link_dialog(shell);
    }
    let image_tip = i18n::tr(shell.language, "Insert an image");
    let image = icons::button(ui, image_tip, "image", palette).on_hover_text(image_tip);
    a11y::label(&image, WidgetType::Button, image_tip);
    if image.clicked() {
        promote_untitled_to_markdown(shell);
        actions::open_image_dialog(shell);
    }
    let table_tip = i18n::tr(shell.language, "Insert a table");
    let table = icons::button(ui, table_tip, "table", palette).on_hover_text(table_tip);
    a11y::label(&table, WidgetType::Button, table_tip);
    if table.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.table"));
    }
    let clear_tip = i18n::tr(shell.language, "Clear all formatting");
    let clear = widgets::rich_button(ui, RichText::new("Aa")).on_hover_text(clear_tip);
    a11y::label(&clear, WidgetType::Button, clear_tip);
    if clear.clicked() {
        promote_untitled_to_markdown(shell);
        actions::run(shell, ctx, CommandId("cmd.md.clear"));
    }
}

/// Compact display label for a font family when horizontal bar space is tight.
fn compact_font_label(family: &'static str) -> &'static str {
    match family {
        "JetBrains Mono" => "JetBrains",
        "IBM Plex Mono" => "IBM Plex",
        "Atkinson Hyperlegible Mono" => "Atkinson",
        "Fira Mono" => "Fira",
        other => other,
    }
}

/// Group 3: Font family dropdown + Font size stepper (`- 14px +`).
fn group_typography(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    ctx: &Context,
    palette: Palette,
    compact: bool,
) {
    let current_family = actions::detect_current_font_family(shell);
    let display_family = if compact {
        compact_font_label(current_family)
    } else {
        current_family
    };
    let mut menu_config = egui::containers::menu::MenuConfig::default();
    menu_config.close_behavior = egui::PopupCloseBehavior::CloseOnClickOutside;
    let (font_resp, _) = egui::containers::menu::MenuButton::new((
        RichText::new(display_family).size(11.0),
        icons::glyph("chevron-down", palette.text_dim),
    ))
    .config(menu_config)
    .ui(ui, |ui| {
        ui.set_min_width(220.0);
        ui.horizontal(|ui| {
            ui.add(icons::glyph("search", palette.text_dim));
            let edit = egui::TextEdit::singleline(&mut shell.font_search_filter)
                .hint_text("Search fonts...")
                .desired_width(150.0);
            ui.add(edit);
            if !shell.font_search_filter.is_empty() && ui.small_button("✕").clicked() {
                shell.font_search_filter.clear();
            }
        });
        ui.separator();

        let filter = shell.font_search_filter.to_lowercase();
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                let filtered_bundled: Vec<&str> = FAMILIES
                    .into_iter()
                    .filter(|f| filter.is_empty() || f.to_lowercase().contains(&filter))
                    .collect();

                if !filtered_bundled.is_empty() {
                    ui.label(
                        RichText::new(i18n::tr(shell.language, "Bundled Fonts"))
                            .small()
                            .color(palette.text_dim),
                    );
                    for &family in &filtered_bundled {
                        if ui
                            .selectable_label(current_family == family, family)
                            .clicked()
                        {
                            promote_untitled_to_markdown(shell);
                            actions::apply_custom_font_family(shell, ctx, family);
                            ui.close();
                        }
                    }
                }

                let sys_all = crate::ui::fonts::system_fonts();
                let filtered_sys: Vec<&crate::ui::fonts::SystemFont> = sys_all
                    .iter()
                    .filter(|f| filter.is_empty() || f.name.to_lowercase().contains(&filter))
                    .collect();

                if !filtered_sys.is_empty() {
                    if !filtered_bundled.is_empty() {
                        ui.separator();
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} ({})",
                            i18n::tr(shell.language, "System Fonts"),
                            sys_all.len()
                        ))
                        .small()
                        .color(palette.text_dim),
                    );
                    for f in &filtered_sys {
                        if ui
                            .selectable_label(current_family == f.name, f.name)
                            .clicked()
                        {
                            promote_untitled_to_markdown(shell);
                            actions::apply_custom_font_family(shell, ctx, f.name);
                            ui.close();
                        }
                    }
                }

                if filtered_bundled.is_empty() && filtered_sys.is_empty() {
                    ui.label(
                        RichText::new(match shell.language {
                            crate::ui::state::AppLanguage::Portuguese => "Nenhuma fonte encontrada",
                            _ => "No matching fonts found",
                        })
                        .italics()
                        .color(palette.text_dim),
                    );
                }
            });
    });
    let font_menu = font_resp.on_hover_text(format!("Font family ({})", current_family));
    a11y::label(&font_menu, WidgetType::ComboBox, "Font family");

    let dec_tip = i18n::tr(shell.language, "Decrease font size");
    let decrease = ui
        .add(egui::Button::new(RichText::new("-").size(12.0)).min_size(egui::vec2(18.0, 22.0)))
        .on_hover_text(dec_tip);
    a11y::label(&decrease, WidgetType::Button, dec_tip);
    if decrease.clicked() {
        promote_untitled_to_markdown(shell);
        actions::step_font_size(shell, ctx, false);
    }
    let px = actions::detect_current_font_size(shell);
    ui.label(
        RichText::new(format!("{px}px"))
            .monospace()
            .size(11.0)
            .color(palette.text),
    );
    let inc_tip = i18n::tr(shell.language, "Increase font size");
    let increase = ui
        .add(egui::Button::new(RichText::new("+").size(12.0)).min_size(egui::vec2(18.0, 22.0)))
        .on_hover_text(inc_tip);
    a11y::label(&increase, WidgetType::Button, inc_tip);
    if increase.clicked() {
        promote_untitled_to_markdown(shell);
        actions::step_font_size(shell, ctx, true);
    }
}

/// Group 4: Segmented Markdown view selector (`Raw | Split | Preview`).
fn group_view_mode(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    ctx: &Context,
    palette: Palette,
    compact: bool,
) {
    let current_mode = shell
        .active_tab()
        .filter(|tab| tab.kind == DocKind::Markdown)
        .map(|tab| tab.view_mode())
        .unwrap_or(MarkdownViewMode::Raw);
    let min_w = if compact { 30.0 } else { 34.0 };
    for (mode, text, a11y_name, cmd_id) in [
        (
            MarkdownViewMode::Raw,
            "Raw",
            "Raw view",
            "cmd.view.md_source",
        ),
        (
            MarkdownViewMode::Split,
            "Split",
            "Split view",
            "cmd.view.md_split",
        ),
        (
            MarkdownViewMode::Preview,
            "Preview",
            "Preview",
            "cmd.view.md_formatted",
        ),
    ] {
        let active = current_mode == mode;
        let translated = i18n::tr(shell.language, text);
        let label = RichText::new(translated).size(11.0).color(if active {
            palette.text
        } else {
            palette.text_dim
        });
        let mut button = egui::Button::new(if active { label.strong() } else { label })
            .min_size(egui::vec2(min_w, 22.0));
        if active {
            button = button.fill(palette.bg);
        }
        let a11y_label = i18n::tr(shell.language, a11y_name);
        let resp = ui.add(button).on_hover_text(a11y_label);
        a11y::label(&resp, WidgetType::Button, a11y_label);
        if resp.clicked() {
            if mode != MarkdownViewMode::Raw {
                if let Some(tab) = shell.active_tab_mut() {
                    tab.kind = DocKind::Markdown;
                }
            }
            actions::run(shell, ctx, CommandId(cmd_id));
        }
    }
}

/// The `Text color` toolbar dropdown: shows the `A` text-color icon with an active
/// colour bar indicator underneath, and opens a modern market-standard colour palette menu
/// (Automatic reset, 16-colour grid swatches, and custom RGB/Hex color picker).
fn text_color_menu(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let swatch_color = match shell.text_color {
        TextColor::Default => palette.text_dim,
        other => other
            .rgb()
            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
            .unwrap_or(palette.accent),
    };
    let menu = ui.menu_button(icons::glyph("text-color", palette.text), |ui| {
        ui.set_min_width(210.0);
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);

        // 1. Reset / Default (Automatic)
        let default_active = shell.text_color == TextColor::Default;
        let auto_btn = ui.add(
            egui::Button::new(
                RichText::new(i18n::tr(shell.language, "Automatic (Reset)"))
                    .size(12.0)
                    .color(if default_active {
                        palette.accent
                    } else {
                        palette.text
                    }),
            )
            .min_size(egui::vec2(200.0, 24.0))
            .selected(default_active),
        );
        if auto_btn.clicked() {
            shell.text_color = TextColor::Default;
            promote_untitled_to_markdown(shell);
            actions::run(shell, ctx, CommandId("cmd.md.text_color"));
            ui.close();
        }

        ui.separator();
        ui.label(
            RichText::new(i18n::tr(shell.language, "Standard Colors"))
                .size(10.0)
                .color(palette.text_dim),
        );

        // 2. 2D Swatch Grid (8 columns x 2 rows = 16 colors)
        egui::Grid::new("color_palette_grid")
            .spacing(egui::vec2(5.0, 5.0))
            .show(ui, |ui| {
                for (col_idx, &(rgb, label)) in TextColor::PALETTE_GRID.iter().enumerate() {
                    let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    let is_active = shell.text_color.rgb() == Some(rgb);

                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                    let painter = ui.painter();

                    let is_hovered = resp.hovered();
                    // Draw swatch square with rounded corners
                    painter.rect_filled(rect, egui::CornerRadius::same(3), color);
                    let border_color = if is_active {
                        palette.accent
                    } else if is_hovered {
                        palette.text
                    } else {
                        palette.border
                    };
                    let stroke_width = if is_active || is_hovered { 1.5 } else { 0.5 };
                    painter.rect_stroke(
                        rect,
                        egui::CornerRadius::same(3),
                        egui::Stroke::new(stroke_width, border_color),
                        StrokeKind::Inside,
                    );

                    if is_active {
                        let center = rect.center();
                        let dot_color = if (rgb[0] as u32 + rgb[1] as u32 + rgb[2] as u32) > 380 {
                            Color32::BLACK
                        } else {
                            Color32::WHITE
                        };
                        painter.circle_filled(center, 3.0, dot_color);
                    }

                    if resp.on_hover_text(label).clicked() {
                        shell.text_color = TextColor::from_rgb(rgb);
                        shell.custom_color_rgb = rgb;
                        promote_untitled_to_markdown(shell);
                        actions::run(shell, ctx, CommandId("cmd.md.text_color"));
                        ui.close();
                    }

                    if (col_idx + 1) % 8 == 0 {
                        ui.end_row();
                    }
                }
            });

        ui.separator();
        ui.label(
            RichText::new(i18n::tr(shell.language, "Custom Color"))
                .size(10.0)
                .color(palette.text_dim),
        );

        // 3. Custom color picker with egui::color_edit_button_srgb
        ui.horizontal(|ui| {
            let color_changed = ui
                .color_edit_button_srgb(&mut shell.custom_color_rgb)
                .changed();
            let hex_str = format!(
                "#{:02X}{:02X}{:02X}",
                shell.custom_color_rgb[0], shell.custom_color_rgb[1], shell.custom_color_rgb[2]
            );
            ui.label(
                RichText::new(&hex_str)
                    .monospace()
                    .size(11.0)
                    .color(palette.text),
            );

            let apply_btn = ui.add(
                egui::Button::new(RichText::new(i18n::tr(shell.language, "Apply")).size(11.0))
                    .min_size(egui::vec2(50.0, 20.0)),
            );
            if apply_btn.clicked()
                || (color_changed && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                shell.text_color = TextColor::Custom(shell.custom_color_rgb);
                promote_untitled_to_markdown(shell);
                actions::run(shell, ctx, CommandId("cmd.md.text_color"));
                ui.close();
            }
        });
    });
    let btn_rect = menu.response.rect;
    let bar_rect = egui::Rect::from_center_size(
        egui::pos2(btn_rect.center().x, btn_rect.bottom() - 4.0),
        egui::vec2(14.0, 3.0),
    );
    ui.painter()
        .rect_filled(bar_rect, egui::CornerRadius::same(1), swatch_color);
    let response = menu
        .response
        .on_hover_text(format!("Text color ({})", shell.text_color.label()));
    a11y::label(&response, WidgetType::ComboBox, "Text color");
}

/// Detects the paragraph style at the caret position in the active document.
fn detect_current_paragraph(shell: &Shell) -> ParagraphStyle {
    if let Some(tab) = shell.active_tab() {
        if tab.kind == DocKind::Markdown {
            let text = tab.text();
            let caret_byte = tab.document().selection().head().min(text.len());
            let caret_byte = if text.is_char_boundary(caret_byte) {
                caret_byte
            } else {
                (0..caret_byte)
                    .rev()
                    .find(|&b| text.is_char_boundary(b))
                    .unwrap_or(0)
            };
            let line_start = text[..caret_byte].rfind('\n').map_or(0, |idx| idx + 1);
            let line_end = text[caret_byte..]
                .find('\n')
                .map_or(text.len(), |idx| caret_byte + idx);
            let line = &text[line_start..line_end];
            return match nc_markdown::Heading::of(line) {
                nc_markdown::Heading::Level(1) => ParagraphStyle::Title,
                nc_markdown::Heading::Level(2) => ParagraphStyle::Subtitle,
                nc_markdown::Heading::Level(3) => ParagraphStyle::Heading,
                nc_markdown::Heading::Level(4) => ParagraphStyle::Subheading,
                nc_markdown::Heading::Level(5) => ParagraphStyle::Section,
                nc_markdown::Heading::Level(6) => ParagraphStyle::Subsection,
                _ => ParagraphStyle::Body,
            };
        }
        return tab.paragraph;
    }
    ParagraphStyle::Body
}

/// The paragraph-style dropdown: shows the active style (e.g. `H1`, `H2`, `Body`) and allows changing it.
fn paragraph_menu(shell: &mut Shell, ui: &mut egui::Ui, ctx: &Context, palette: Palette) {
    let current = detect_current_paragraph(shell);
    let menu = ui.menu_button(
        (
            RichText::new(current.compact_label())
                .size(12.0)
                .color(palette.text),
            icons::glyph("chevron-down", palette.text_dim),
        ),
        |ui| {
            for style in ParagraphStyle::ALL {
                if ui
                    .selectable_label(
                        style == current,
                        format!(
                            "{} ({})",
                            i18n::tr(shell.language, style.label()),
                            style.compact_label()
                        ),
                    )
                    .clicked()
                {
                    promote_untitled_to_markdown(shell);
                    actions::run(shell, ctx, style.command());
                    ui.close();
                }
            }
        },
    );
    // The visible label is the current style; what the button is for is the
    // paragraph style as a whole (`NC-P007`).
    a11y::label(
        &menu.response,
        WidgetType::ComboBox,
        i18n::tr(shell.language, "Paragraph style"),
    );
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
                let can_promote = shell.active_tab().is_some_and(|t| !t.file.has_path());
                let enabled = spec.is_available(context) || can_promote;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::new(i18n::tr(shell.language, spec.label)),
                    )
                    .clicked()
                {
                    promote_untitled_to_markdown(shell);
                    actions::run(shell, ctx, spec.id);
                    ui.close();
                }
            }
        },
    );
    a11y::label(
        &menu.response,
        WidgetType::ComboBox,
        i18n::tr(shell.language, "Lists"),
    );
}

/// The sparkle / feedback / account / settings controls on the right.
fn right_toolbar(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    ctx: &Context,
    palette: Palette,
    compact: bool,
    icon_only_theme: bool,
) {
    let saved_gap = std::mem::replace(
        &mut ui.spacing_mut().item_spacing.x,
        if compact { 4.0 } else { 6.0 },
    );
    let settings_tip = i18n::tr(shell.language, "Settings");
    let settings = widgets::icon_button(ui, "⚙").on_hover_text(settings_tip);
    a11y::label(&settings, WidgetType::Button, settings_tip);
    if settings.clicked() {
        actions::run(shell, ctx, CommandId("cmd.app.settings"));
    }
    let icon_only_mode = ui.max_rect().width() < 860.0;
    editor_mode_button(shell, ui, ctx, palette, icon_only_mode);
    let feedback_tip = i18n::tr(shell.language, "Send feedback");
    let feedback = icons::button(ui, feedback_tip, "chat", palette).on_hover_text(feedback_tip);
    a11y::label(&feedback, WidgetType::Button, feedback_tip);

    // Quick Light / Dark theme toggle button (`gui2`: ☀️ Light / 🌙 Dark)
    let is_dark = palette == Palette::DARK;
    let (theme_icon, theme_label) = if is_dark {
        ("moon", i18n::tr(shell.language, "Dark"))
    } else {
        ("sun", i18n::tr(shell.language, "Light"))
    };
    let toggle_tip = i18n::tr(shell.language, "Toggle theme");
    let theme_btn = if icon_only_theme {
        icons::button(ui, toggle_tip, theme_icon, palette).on_hover_text(toggle_tip)
    } else {
        ui.add(
            egui::Button::new((
                icons::glyph(theme_icon, palette.text),
                RichText::new(theme_label).size(11.5).color(palette.text),
            ))
            .min_size(egui::vec2(52.0, 26.0)),
        )
        .on_hover_text(toggle_tip)
    };
    a11y::label(&theme_btn, WidgetType::Button, toggle_tip);
    if theme_btn.clicked() {
        shell.theme = if is_dark {
            AppTheme::Light
        } else {
            AppTheme::Dark
        };
        shell.palette = Palette::for_choice(shell.theme, None);
    }

    // The AI ("sparkle") menu. Its entries are service-gated, so they show
    // greyed until the service is configured.
    let context = shell.availability();
    let ai = ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for widget in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            widget.weak_bg_fill = palette.accent;
            widget.bg_fill = palette.accent;
        }
        ui.menu_button(
            (
                icons::glyph("sparkle", Color32::WHITE),
                RichText::new("AI")
                    .size(11.5)
                    .strong()
                    .color(Color32::WHITE),
                icons::glyph("chevron-down", Color32::WHITE),
            ),
            |ui| {
                for id in ["cmd.ai.rewrite", "cmd.ai.summarize", "cmd.ai.write"] {
                    if let Some(spec) = by_id(id) {
                        let enabled = spec.is_available(context);
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(i18n::tr(shell.language, spec.label)),
                            )
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                }
            },
        )
    });
    a11y::label(&ai.inner.response, WidgetType::ComboBox, "AI");
    ui.spacing_mut().item_spacing.x = saved_gap;
}

/// The editor-mode toggle button (replacing the account avatar): switches the application
/// between Rich Text Editor mode (`EditorMode::Text`) and Full Code Editor mode (`EditorMode::Code`).
fn editor_mode_button(
    shell: &mut Shell,
    ui: &mut egui::Ui,
    ctx: &Context,
    palette: Palette,
    icon_only: bool,
) {
    let mode = shell.editor_mode;
    let is_code = mode == EditorMode::Code;
    let fg = if is_code {
        Color32::WHITE
    } else {
        palette.text
    };
    let lang = shell.language;
    let hover_tip = match (mode, lang) {
        (EditorMode::Text, AppLanguage::Portuguese) => {
            "Modo do editor: Editor de Texto (clique para alternar para Editor de Código)"
        }
        (EditorMode::Code, AppLanguage::Portuguese) => {
            "Modo do editor: Editor de Código (clique para alternar para Editor de Texto)"
        }
        (EditorMode::Text, AppLanguage::Spanish) => {
            "Modo del editor: Editor de Texto (clic para cambiar a Editor de Código)"
        }
        (EditorMode::Code, AppLanguage::Spanish) => {
            "Modo del editor: Editor de Código (clic para cambiar a Editor de Texto)"
        }
        (EditorMode::Text, AppLanguage::French) => {
            "Mode éditeur : Éditeur de Texte (cliquer pour basculer vers Éditeur de Code)"
        }
        (EditorMode::Code, AppLanguage::French) => {
            "Mode éditeur : Éditeur de Code (cliquer pour basculer vers Éditeur de Texte)"
        }
        (EditorMode::Text, AppLanguage::German) => {
            "Editor-Modus: Text-Editor (klicken, um zum Code-Editor zu wechseln)"
        }
        (EditorMode::Code, AppLanguage::German) => {
            "Editor-Modus: Code-Editor (klicken, um zum Text-Editor zu wechseln)"
        }
        (EditorMode::Text, AppLanguage::Italian) => {
            "Modalità editor: Editor di Testo (fai clic per passare all'Editor di Codice)"
        }
        (EditorMode::Code, AppLanguage::Italian) => {
            "Modalità editor: Editor di Codice (fai clic per passare all'Editor di Testo)"
        }
        (EditorMode::Text, AppLanguage::English) => {
            "Editor mode: Text Editor (click to switch to Code Editor)"
        }
        (EditorMode::Code, AppLanguage::English) => {
            "Editor mode: Code Editor (click to switch to Text Editor)"
        }
    };

    let response = if icon_only {
        let mut btn =
            egui::Button::new(icons::glyph(mode.icon(), fg)).min_size(egui::vec2(28.0, 26.0));
        if is_code {
            btn = btn.fill(palette.accent);
        } else {
            btn = btn.stroke(egui::Stroke::new(1.0, palette.border));
        }
        ui.add(btn).on_hover_text(hover_tip)
    } else {
        let mut btn = egui::Button::new((
            icons::glyph(mode.icon(), fg),
            RichText::new(i18n::tr(lang, mode.short_label()))
                .size(11.5)
                .strong()
                .color(fg),
        ))
        .min_size(egui::vec2(56.0, 26.0));
        if is_code {
            btn = btn.fill(palette.accent);
        } else {
            btn = btn
                .fill(palette.bg)
                .stroke(egui::Stroke::new(1.0, palette.border));
        }
        ui.add(btn).on_hover_text(hover_tip)
    };
    a11y::label(&response, WidgetType::Button, i18n::tr(lang, "Editor mode"));
    if response.clicked() {
        actions::run(shell, ctx, CommandId("cmd.view.toggle_editor_mode"));
    }
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
