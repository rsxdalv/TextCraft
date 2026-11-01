//! The dialog surfaces: Find / Replace, Go to line (`docs/ui-reference.md`
//! §5–§6), Open / Save As (§7), and the prompts that guard against data loss
//! (§9).
//!
//! The dialogs own no logic of their own: they edit the state the shell already
//! holds, and their buttons call [`actions`], the same place a menu item or a
//! keyboard shortcut lands.

use std::path::PathBuf;

use egui::{Align2, Context, Key, RichText, Vec2, WidgetType};

use crate::ui::a11y;
use crate::ui::actions;
use crate::ui::i18n;
use crate::ui::state::{AppLanguage, FileDialogMode, Prompt, Shell};
use nc_textproc::Direction;

/// Draws whichever dialog is open. The file dialog is drawn before the prompt,
/// so a prompt raised over it sits on top.
pub fn show(shell: &mut Shell, ctx: &Context) {
    if shell.find.open {
        find(shell, ctx);
    }
    if shell.go_to.open {
        go_to(shell, ctx);
    }
    if shell.link_dialog.open {
        link(shell, ctx);
    }
    if shell.image_dialog.open {
        image(shell, ctx);
    }
    if shell.page_setup_dialog.open {
        page_setup(shell, ctx);
    }
    if shell.spelling.open {
        spelling(shell, ctx);
    }
    if shell.file_dialog.is_some() {
        file_dialog(shell, ctx);
    }
    if shell.prompt.is_some() {
        prompt(shell, ctx);
    }
}

/// The Find and Replace dialog (`docs/ui-reference.md` §5). Modeless, anchored
/// to the top-right of the editor surface.
fn find(shell: &mut Shell, ctx: &Context) {
    let lang = shell.language;
    let mut open = shell.find.open;
    egui::Window::new(i18n::tr(lang, "Find and Replace"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::RIGHT_TOP, Vec2::new(-24.0, 70.0))
        .show(ctx, |ui| {
            ui.set_min_width(360.0);

            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "Find what"));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut shell.find.query)
                        .id(egui::Id::new("find_query"))
                        .desired_width(240.0),
                );
                a11y::label(&response, WidgetType::TextEdit, i18n::tr(lang, "Find what"));
                if shell.find.focus {
                    response.request_focus();
                    shell.find.focus = false;
                }
                if response.lost_focus()
                    && ui.input(|input| input.key_pressed(Key::Enter))
                    && !shell.find.query.is_empty()
                {
                    let direction = shell.find.direction;
                    actions::search_step(shell, direction);
                }
            });

            if shell.find.replace {
                ui.horizontal(|ui| {
                    ui.label(i18n::tr(lang, "Replace with"));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut shell.find.replacement)
                            .desired_width(240.0),
                    );
                    a11y::label(
                        &response,
                        WidgetType::TextEdit,
                        i18n::tr(lang, "Replace with"),
                    );
                });
            }

            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut shell.find.options.match_case,
                    i18n::tr(lang, "Match case"),
                );
                ui.checkbox(
                    &mut shell.find.options.whole_word,
                    i18n::tr(lang, "Match whole word only"),
                );
            });

            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "Direction"));
                ui.radio_value(
                    &mut shell.find.direction,
                    Direction::Forward,
                    i18n::tr(lang, "Down"),
                );
                ui.radio_value(
                    &mut shell.find.direction,
                    Direction::Backward,
                    i18n::tr(lang, "Up"),
                );
            });

            ui.checkbox(&mut shell.find.wraps, i18n::tr(lang, "Wrap around"));
            ui.separator();

            let has_query = !shell.find.query.is_empty();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(has_query, egui::Button::new(i18n::tr(lang, "Find Next")))
                    .clicked()
                {
                    let direction = shell.find.direction;
                    actions::search_step(shell, direction);
                }
                if shell.find.replace {
                    if ui
                        .add_enabled(has_query, egui::Button::new(i18n::tr(lang, "Replace")))
                        .clicked()
                    {
                        actions::replace(shell);
                    }
                    if ui
                        .add_enabled(has_query, egui::Button::new(i18n::tr(lang, "Replace All")))
                        .clicked()
                    {
                        actions::replace_all(shell);
                    }
                }
                if ui.button(i18n::tr(lang, "Close")).clicked() {
                    shell.find.open = false;
                }
            });
        });

    if !open || ctx.input(|input| input.key_pressed(Key::Escape)) {
        shell.find.open = false;
    }
}

/// The Go to line dialog (`docs/ui-reference.md` §6). A small modal card.
fn go_to(shell: &mut Shell, ctx: &Context) {
    let lang = shell.language;
    let mut open = shell.go_to.open;
    egui::Window::new(i18n::tr(lang, "Go to line"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(240.0);
            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "Line number"));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut shell.go_to.line)
                        .id(egui::Id::new("go_to_line"))
                        .desired_width(90.0),
                );
                a11y::label(
                    &response,
                    WidgetType::TextEdit,
                    i18n::tr(lang, "Line number"),
                );
                if shell.go_to.focus {
                    response.request_focus();
                    shell.go_to.focus = false;
                }
            });

            let valid = nc_textproc::parse_line_number(&shell.go_to.line).is_some();
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(valid, egui::Button::new(i18n::tr(lang, "Go to")))
                    .clicked()
                    || (valid && ui.input(|input| input.key_pressed(Key::Enter)))
                {
                    actions::go_to(shell);
                }
                if ui.button(i18n::tr(lang, "Cancel")).clicked() {
                    shell.go_to.open = false;
                }
            });
        });

    if !open || ctx.input(|input| input.key_pressed(Key::Escape)) {
        shell.go_to.open = false;
    }
}

/// The Page setup dialog (`NC-F007`, `docs/ui-reference.md` §8): the sheet the printer
/// will mark. It edits a draft, so `Cancel` throws the changes away and `Print` commits
/// them and prints with them — exactly the two buttons §8 names.
fn page_setup(shell: &mut Shell, ctx: &Context) {
    let lang = shell.language;
    let mut open = shell.page_setup_dialog.open;
    egui::Window::new(i18n::tr(lang, "Page setup"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            let draft = &mut shell.page_setup_dialog.draft;

            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "Paper size"));
                egui::ComboBox::from_id_salt("page_paper")
                    .selected_text(draft.paper.label())
                    .show_ui(ui, |ui| {
                        for paper in nc_textproc::PaperSize::ALL {
                            ui.selectable_value(&mut draft.paper, paper, paper.label());
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "Orientation"));
                ui.radio_value(
                    &mut draft.orientation,
                    nc_textproc::Orientation::Portrait,
                    i18n::tr(lang, "Portrait"),
                );
                ui.radio_value(
                    &mut draft.orientation,
                    nc_textproc::Orientation::Landscape,
                    i18n::tr(lang, "Landscape"),
                );
            });

            ui.label(i18n::tr(lang, "Margins (mm)"));
            let margins = &mut draft.margins;
            ui.horizontal(|ui| {
                margin_field(ui, "margin_left", i18n::tr(lang, "Left"), &mut margins.left);
                margin_field(
                    ui,
                    "margin_right",
                    i18n::tr(lang, "Right"),
                    &mut margins.right,
                );
            });
            ui.horizontal(|ui| {
                margin_field(ui, "margin_top", i18n::tr(lang, "Top"), &mut margins.top);
                margin_field(
                    ui,
                    "margin_bottom",
                    i18n::tr(lang, "Bottom"),
                    &mut margins.bottom,
                );
            });

            let header_hint = match lang {
                AppLanguage::Portuguese => "Cabeçalho — &p número da página, &d data",
                AppLanguage::Spanish => "Encabezado — &p número de página, &d fecha",
                AppLanguage::French => "En-tête — &p numéro de page, &d date",
                AppLanguage::German => "Kopfzeile — &p Seitenzahl, &d Datum",
                AppLanguage::Italian => "Intestazione — &p numero di pagina, &d data",
                AppLanguage::English => "Header — &p page number, &d date",
            };
            let footer_hint = match lang {
                AppLanguage::Portuguese => "Rodapé — &p número da página, &d data",
                AppLanguage::Spanish => "Pie de página — &p número de página, &d fecha",
                AppLanguage::French => "Pied de page — &p numéro de page, &d date",
                AppLanguage::German => "Fußzeile — &p Seitenzahl, &d Datum",
                AppLanguage::Italian => "Piè di pagina — &p numero di pagina, &d data",
                AppLanguage::English => "Footer — &p page number, &d date",
            };

            let response = ui.add(
                egui::TextEdit::singleline(&mut draft.header)
                    .id(egui::Id::new("page_header"))
                    .hint_text(header_hint),
            );
            a11y::label(&response, WidgetType::TextEdit, i18n::tr(lang, "Header"));
            let response = ui.add(
                egui::TextEdit::singleline(&mut draft.footer)
                    .id(egui::Id::new("page_footer"))
                    .hint_text(footer_hint),
            );
            a11y::label(&response, WidgetType::TextEdit, i18n::tr(lang, "Footer"));

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(i18n::tr(lang, "Print")).clicked() {
                    shell.page_setup = shell.page_setup_dialog.draft.clone();
                    shell.page_setup_dialog.open = false;
                    actions::print(shell);
                }
                if ui.button(i18n::tr(lang, "Cancel")).clicked() {
                    shell.page_setup_dialog.open = false;
                }
            });
        });

    if !open {
        // Closed by its own cross: the draft is discarded, as Cancel would.
        shell.page_setup_dialog.open = false;
    }
}

/// One margin input, in millimetres, bounded to what any named sheet can hold.
fn margin_field(ui: &mut egui::Ui, id: &str, label: &str, value: &mut u32) {
    ui.label(label);
    ui.push_id(id, |ui| {
        ui.add(egui::DragValue::new(value).range(0..=100).suffix(" mm"));
    });
}

/// The spelling dialog (`NC-P004`): the corrections offered for one misspelled
/// word. Picking one replaces the word; `Esc` or Dismiss leaves it. It belongs to
/// the tab it was opened from, and closes if that tab stops being the shown one.
fn spelling(shell: &mut Shell, ctx: &Context) {
    if shell.spelling.tab != shell.active {
        actions::close_spelling(shell);
        return;
    }
    let lang = shell.language;
    let mut open = shell.spelling.open;
    let word = shell.spelling.word.clone();
    let suggestions = shell.spelling.suggestions.clone();
    let mut chosen: Option<usize> = None;

    let misspelled_msg = match lang {
        AppLanguage::Portuguese => format!("\"{word}\" está com erro ortográfico."),
        AppLanguage::Spanish => format!("\"{word}\" está mal escrita."),
        AppLanguage::French => format!("« {word} » est mal orthographié."),
        AppLanguage::German => format!("„{word}“ ist falsch geschrieben."),
        AppLanguage::Italian => format!("\"{word}\" non è corretta."),
        AppLanguage::English => format!("\"{word}\" is misspelled."),
    };

    egui::Window::new(i18n::tr(lang, "Spelling"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(240.0);
            ui.label(RichText::new(misspelled_msg));
            ui.separator();
            ui.label(i18n::tr(lang, "Suggestions"));
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, suggestion) in suggestions.iter().enumerate() {
                        if ui.button(RichText::new(suggestion)).clicked() {
                            chosen = Some(index);
                        }
                    }
                });
            ui.separator();
            if ui.button(i18n::tr(lang, "Dismiss")).clicked() {
                actions::close_spelling(shell);
            }
        });

    if let Some(index) = chosen {
        actions::apply_spelling(shell, index);
    }
    if !open || ctx.input(|input| input.key_pressed(Key::Escape)) {
        actions::close_spelling(shell);
    }
}

/// Open / Save As through the operating system's own dialog: its places, search,
/// previews and keyboard handling, which a hand-drawn list would never match. It is
/// modal and blocks this frame until it is answered; the in-window dialog below is
/// what runs where there is no one to answer a native one (tests).
fn native_file_dialog(shell: &mut Shell) {
    let Some(dialog) = shell.file_dialog.as_mut() else {
        return;
    };
    let picker = rfd::FileDialog::new().set_directory(&dialog.directory);
    match dialog.mode {
        FileDialogMode::Open => {
            let picked = picker
                .set_title("Open")
                .add_filter("Text documents", &["txt", "md", "markdown", "log"])
                .add_filter("All files", &["*"])
                .pick_files();
            shell.file_dialog = None;
            if let Some(paths) = picked {
                for path in paths {
                    actions::open_path(shell, &path, None);
                }
            }
        }
        FileDialogMode::SaveAs => {
            let picked = picker
                .set_title("Save as")
                .set_file_name(dialog.file_name.as_str())
                .add_filter("Text documents", &["txt"])
                .add_filter("Markdown", &["md"])
                .add_filter("All files", &["*"])
                .save_file();
            match picked {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        dialog.directory = parent.to_path_buf();
                    }
                    dialog.file_name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    actions::confirm_file_dialog(shell);
                }
                None => shell.file_dialog = None,
            }
        }
    }
}

/// The Open / Save As dialog (`docs/ui-reference.md` §7): one frame in two
/// modes, differing in their title, primary button, and encoding default.
fn file_dialog(shell: &mut Shell, ctx: &Context) {
    if shell.native_dialogs {
        native_file_dialog(shell);
        return;
    }
    let lang = shell.language;
    let Some(dialog) = shell.file_dialog.as_mut() else {
        return;
    };

    // Copied out first: `shell.file_dialog` is borrowed mutably for the rest of
    // this function, so the palette has to be in hand before that borrow.
    let palette = shell.palette;

    let title = match dialog.mode {
        FileDialogMode::Open => i18n::tr(lang, "Open"),
        FileDialogMode::SaveAs => i18n::tr(lang, "Save as"),
    };
    let primary = match dialog.mode {
        FileDialogMode::Open => i18n::tr(lang, "Open"),
        FileDialogMode::SaveAs => i18n::tr(lang, "Save"),
    };

    let mut window_open = true;
    let mut confirm = false;
    let mut cancel = false;

    egui::Window::new(title)
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(460.0);

            ui.horizontal(|ui| {
                let up = ui.button("‹");
                a11y::label(&up, WidgetType::Button, i18n::tr(lang, "Up one level"));
                if up.clicked() {
                    dialog.go_up();
                }
                ui.label(dialog.directory.display().to_string());
            });

            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut enter: Option<PathBuf> = None;
                    let mut open_file: Option<String> = None;
                    for entry in &dialog.entries {
                        let label = if entry.is_dir {
                            format!("{}/", entry.name)
                        } else {
                            entry.name.clone()
                        };
                        let response = ui.selectable_label(dialog.file_name == entry.name, label);
                        if response.double_clicked() {
                            if entry.is_dir {
                                enter = Some(entry.path.clone());
                            } else {
                                open_file = Some(entry.name.clone());
                            }
                        } else if response.clicked() {
                            if entry.is_dir {
                                enter = Some(entry.path.clone());
                            } else {
                                dialog.file_name = entry.name.clone();
                            }
                        }
                    }
                    if let Some(path) = enter {
                        dialog.enter(&path);
                    }
                    if let Some(name) = open_file {
                        dialog.file_name = name;
                        confirm = true;
                    }
                });

            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "File name"));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut dialog.file_name)
                        .id(egui::Id::new("file_name"))
                        .desired_width(300.0),
                );
                a11y::label(&response, WidgetType::TextEdit, i18n::tr(lang, "File name"));
                if dialog.focus {
                    response.request_focus();
                    dialog.focus = false;
                }
                if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                    confirm = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label(i18n::tr(lang, "Encoding"));
                let combo = egui::ComboBox::from_id_salt("file_encoding")
                    .selected_text(dialog.encoding.label())
                    .show_ui(ui, |ui| {
                        for choice in dialog.choices() {
                            ui.selectable_value(&mut dialog.encoding, *choice, choice.label());
                        }
                    });
                a11y::label(
                    &combo.response,
                    WidgetType::ComboBox,
                    i18n::tr(lang, "Encoding"),
                );
            });

            if let Some(error) = &dialog.error {
                ui.label(RichText::new(error).color(palette.danger));
            }

            ui.separator();
            ui.horizontal(|ui| {
                let ready = !dialog.file_name.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new(primary)).clicked() {
                    confirm = true;
                }
                if ui.button(i18n::tr(lang, "Cancel")).clicked() {
                    cancel = true;
                }
            });
        });

    // `Esc` closes the dialog — unless a prompt is up over it, which takes the
    // key for its own Cancel.
    let escape = shell.prompt.is_none() && ctx.input(|input| input.key_pressed(Key::Escape));
    if cancel || !window_open || escape {
        shell.file_dialog = None;
    } else if confirm {
        actions::confirm_file_dialog(shell);
    }
}

/// What a prompt's buttons do.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// Save the document.
    Save,
    /// Discard the edits.
    DontSave,
    /// Replace the existing file.
    Replace,
    /// Write the characters that do not fit as `?`.
    SaveAnyway,
    /// Go back to Save As to pick another encoding.
    ChooseEncoding,
    /// Read the file again, discarding the editor's text.
    Reload,
    /// Keep the editor's text and write it over the file next time.
    KeepMine,
    /// Do nothing.
    Cancel,
}

/// One button on a prompt.
struct PromptButton {
    /// The button's label.
    label: &'static str,
    /// What it means.
    answer: Answer,
}

/// The prompts that guard against data loss (`docs/ui-reference.md` §9). A
/// small centred card, raised before something is destroyed.
fn prompt(shell: &mut Shell, ctx: &Context) {
    let Some(prompt) = shell.prompt.clone() else {
        return;
    };
    let lang = shell.language;
    let (title, body) = prompt_title_and_body(shell, &prompt);
    let buttons = prompt_buttons(&prompt);

    let mut answer = None;
    let mut window_open = true;

    egui::Window::new(title)
        .open(&mut window_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.label(body);
            ui.separator();
            ui.horizontal(|ui| {
                for button in buttons {
                    if ui.button(i18n::tr(lang, button.label)).clicked() {
                        answer = Some(button.answer);
                    }
                }
            });
        });

    if !window_open || ctx.input(|input| input.key_pressed(Key::Escape)) {
        answer = Some(Answer::Cancel);
    }

    match answer {
        Some(Answer::Save) => actions::answer_unsaved_save(shell),
        Some(Answer::DontSave) => actions::answer_unsaved_dont_save(shell),
        Some(Answer::Replace) => actions::answer_overwrite_replace(shell),
        Some(Answer::SaveAnyway) => actions::answer_unrepresentable_save(shell),
        Some(Answer::ChooseEncoding) => actions::answer_unrepresentable_choose_encoding(shell),
        Some(Answer::Reload) => actions::answer_outside_change_reload(shell),
        Some(Answer::KeepMine) => actions::answer_outside_change_keep(shell),
        Some(Answer::Cancel) => actions::answer_cancel(shell),
        None => {}
    }
}

/// A prompt's title and the sentence it asks.
fn prompt_title_and_body(shell: &Shell, prompt: &Prompt) -> (&'static str, String) {
    let lang = shell.language;
    match prompt {
        Prompt::UnsavedChanges { tab } => {
            let name = shell
                .tabs
                .get(*tab)
                .map(|tab| tab.title())
                .unwrap_or_else(|| i18n::tr(lang, "Untitled").to_owned());
            (
                i18n::tr(lang, "Unsaved changes"),
                match lang {
                    AppLanguage::Portuguese => format!("Salvar alterações em \"{name}\"?"),
                    AppLanguage::Spanish => format!("¿Guardar cambios en \"{name}\"?"),
                    AppLanguage::French => {
                        format!("Enregistrer les modifications apportées à « {name} » ?")
                    }
                    AppLanguage::German => format!("Änderungen an „{name}“ speichern?"),
                    AppLanguage::Italian => format!("Salvare le modifiche a \"{name}\"?"),
                    AppLanguage::English => format!("Save changes to \"{name}\"?"),
                },
            )
        }
        Prompt::Overwrite { path, .. } => {
            let name = file_name(path);
            (
                i18n::tr(lang, "Confirm save as"),
                match lang {
                    AppLanguage::Portuguese => {
                        format!("\"{name}\" já existe. Deseja substituí-lo?")
                    }
                    AppLanguage::Spanish => format!("\"{name}\" ya existe. ¿Desea reemplazarlo?"),
                    AppLanguage::French => {
                        format!("« {name} » existe déjà. Voulez-vous le remplacer ?")
                    }
                    AppLanguage::German => {
                        format!("„{name}“ ist bereits vorhanden. Möchten Sie die Datei ersetzen?")
                    }
                    AppLanguage::Italian => format!("\"{name}\" esiste già. Sostituirlo?"),
                    AppLanguage::English => {
                        format!("\"{name}\" already exists. Do you want to replace it?")
                    }
                },
            )
        }
        Prompt::Unrepresentable {
            encoding,
            characters,
            ..
        } => (
            i18n::tr(lang, "Characters cannot be saved"),
            match lang {
                AppLanguage::Portuguese => format!(
                    "{} caracteres não podem ser salvos em {encoding}. Salvar irá substituí-los por \"?\". Salvar mesmo assim?",
                    characters.len()
                ),
                AppLanguage::Spanish => format!(
                    "{} caracteres no se pueden guardar en {encoding}. Al guardar se reemplazarán con \"?\". ¿Guardar de todos modos?",
                    characters.len()
                ),
                AppLanguage::French => format!(
                    "{} caractères ne peuvent pas être enregistrés dans {encoding}. L'enregistrement les remplacera par « ? ». Enregistrer quand même ?",
                    characters.len()
                ),
                AppLanguage::German => format!(
                    "{} Zeichen können nicht in {encoding} gespeichert werden. Beim Speichern werden sie durch „?“ ersetzt. Trotzdem speichern?",
                    characters.len()
                ),
                AppLanguage::Italian => format!(
                    "{} caratteri non possono essere salvati in {encoding}. Il salvataggio li sostituirà con \"?\". Salvare comunque?",
                    characters.len()
                ),
                AppLanguage::English => format!(
                    "{} characters cannot be saved in {encoding}. Saving will replace them with \"?\". Save anyway?",
                    characters.len()
                ),
            },
        ),
        Prompt::OutsideChange { path, .. } => {
            let name = file_name(path);
            (
                i18n::tr(lang, "File changed on disk"),
                match lang {
                    AppLanguage::Portuguese => {
                        format!("\"{name}\" foi alterado no disco desde a última leitura.")
                    }
                    AppLanguage::Spanish => {
                        format!("\"{name}\" ha cambiado en el disco desde la última lectura.")
                    }
                    AppLanguage::French => format!(
                        "« {name} » a été modifié sur le disque depuis sa dernière lecture."
                    ),
                    AppLanguage::German => format!(
                        "„{name}“ wurde seit dem letzten Lesen auf dem Datenträger geändert."
                    ),
                    AppLanguage::Italian => {
                        format!("\"{name}\" è stato modificato sul disco dall'ultima leitura.")
                    }
                    AppLanguage::English => {
                        format!("\"{name}\" has changed on disk since it was last read.")
                    }
                },
            )
        }
    }
}

/// A prompt's buttons, left to right.
fn prompt_buttons(prompt: &Prompt) -> &'static [PromptButton] {
    const UNSAVED: &[PromptButton] = &[
        PromptButton {
            label: "Save",
            answer: Answer::Save,
        },
        PromptButton {
            label: "Don't save",
            answer: Answer::DontSave,
        },
        PromptButton {
            label: "Cancel",
            answer: Answer::Cancel,
        },
    ];
    const OVERWRITE: &[PromptButton] = &[
        PromptButton {
            label: "Replace",
            answer: Answer::Replace,
        },
        PromptButton {
            label: "Cancel",
            answer: Answer::Cancel,
        },
    ];
    const UNREPRESENTABLE: &[PromptButton] = &[
        PromptButton {
            label: "Save anyway",
            answer: Answer::SaveAnyway,
        },
        PromptButton {
            label: "Choose another encoding",
            answer: Answer::ChooseEncoding,
        },
        PromptButton {
            label: "Cancel",
            answer: Answer::Cancel,
        },
    ];
    const OUTSIDE_CHANGE: &[PromptButton] = &[
        PromptButton {
            label: "Reload",
            answer: Answer::Reload,
        },
        PromptButton {
            label: "Keep my version",
            answer: Answer::KeepMine,
        },
        PromptButton {
            label: "Cancel",
            answer: Answer::Cancel,
        },
    ];

    match prompt {
        Prompt::UnsavedChanges { .. } => UNSAVED,
        Prompt::Overwrite { .. } => OVERWRITE,
        Prompt::Unrepresentable { .. } => UNREPRESENTABLE,
        Prompt::OutsideChange { .. } => OUTSIDE_CHANGE,
    }
}

/// The last component of a path, for a prompt that names a file.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The Insert Link dialog.
fn link(shell: &mut Shell, ctx: &Context) {
    let lang = shell.language;
    let mut open = shell.link_dialog.open;
    let mut insert = false;
    let mut cancel = false;
    egui::Window::new(i18n::tr(lang, "Insert Link"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            egui::Grid::new("link_dialog_grid")
                .num_columns(2)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label(i18n::tr(lang, "Text to display:"));
                    let text_resp = ui.add(
                        egui::TextEdit::singleline(&mut shell.link_dialog.text)
                            .id(egui::Id::new("link_dialog_text"))
                            .desired_width(220.0),
                    );
                    a11y::label(
                        &text_resp,
                        WidgetType::TextEdit,
                        i18n::tr(lang, "Text to display"),
                    );

                    ui.end_row();

                    ui.label(i18n::tr(lang, "Link URL:"));
                    let url_resp = ui.add(
                        egui::TextEdit::singleline(&mut shell.link_dialog.url)
                            .id(egui::Id::new("link_dialog_url"))
                            .desired_width(220.0),
                    );
                    a11y::label(&url_resp, WidgetType::TextEdit, i18n::tr(lang, "Link URL"));
                    if shell.link_dialog.focus {
                        if shell.link_dialog.text.is_empty() {
                            text_resp.request_focus();
                        } else {
                            url_resp.request_focus();
                        }
                        shell.link_dialog.focus = false;
                    }
                    if url_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        insert = true;
                    }
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(i18n::tr(lang, "Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui.button(i18n::tr(lang, "Insert")).clicked() {
                        insert = true;
                    }
                });
            });
        });

    if !open || cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
        shell.link_dialog.open = false;
    }

    if insert {
        let label = shell.link_dialog.text.clone();
        let url = shell.link_dialog.url.trim().to_string();
        shell.link_dialog.open = false;
        actions::apply_custom_link(shell, &label, &url);
    }
}

/// The Insert Image dialog.
fn image(shell: &mut Shell, ctx: &Context) {
    let lang = shell.language;
    let mut open = shell.image_dialog.open;
    let mut insert = false;
    let mut cancel = false;
    let mut browse = false;
    egui::Window::new(i18n::tr(lang, "Insert Image"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            egui::Grid::new("image_dialog_grid")
                .num_columns(2)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label(i18n::tr(lang, "Alt text:"));
                    let alt_resp = ui.add(
                        egui::TextEdit::singleline(&mut shell.image_dialog.alt)
                            .id(egui::Id::new("image_dialog_alt"))
                            .desired_width(240.0),
                    );
                    a11y::label(&alt_resp, WidgetType::TextEdit, i18n::tr(lang, "Alt text"));

                    ui.end_row();

                    ui.label(i18n::tr(lang, "Path or URL:"));
                    ui.horizontal(|ui| {
                        let path_resp = ui.add(
                            egui::TextEdit::singleline(&mut shell.image_dialog.path_or_url)
                                .id(egui::Id::new("image_dialog_path"))
                                .desired_width(170.0),
                        );
                        a11y::label(
                            &path_resp,
                            WidgetType::TextEdit,
                            i18n::tr(lang, "Path or URL"),
                        );
                        if shell.image_dialog.focus {
                            if shell.image_dialog.alt.is_empty() {
                                alt_resp.request_focus();
                            } else {
                                path_resp.request_focus();
                            }
                            shell.image_dialog.focus = false;
                        }
                        if path_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            insert = true;
                        }
                        if ui.button(i18n::tr(lang, "Browse...")).clicked() {
                            browse = true;
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(i18n::tr(lang, "Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui.button(i18n::tr(lang, "Insert")).clicked() {
                        insert = true;
                    }
                });
            });
        });

    if browse {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"],
            )
            .pick_file()
        {
            shell.image_dialog.path_or_url = path.to_string_lossy().into_owned();
        }
    }

    if !open || cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
        shell.image_dialog.open = false;
    }

    if insert {
        let alt = shell.image_dialog.alt.clone();
        let path_or_url = shell.image_dialog.path_or_url.trim().to_string();
        shell.image_dialog.open = false;
        actions::apply_custom_image(shell, &alt, &path_or_url);
    }
}
