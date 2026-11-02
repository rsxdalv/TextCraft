//! `NC-M003` Bold / Italic (`Ctrl+B` / `Ctrl+I`) and `NC-M004` Strikethrough.
//!
//! Acceptance: **"Serialization exact"** for bold and italic, and **"`~~`
//! serialization"** for strikethrough. "Exact" is taken seriously here: each test
//! compares whole strings, so a stray space or a doubled marker fails the build.

use nc_markdown::{Inline, Range, toggle_inline};

/// The selection covering `needle`, which is how these tests say what the user
/// selected without hard-coding byte offsets.
fn selection_of(haystack: &str, needle: &str) -> Range {
    let start = haystack
        .find(needle)
        .expect("the selection is present in the text");
    Range::new(start, start + needle.len()).expect("whole characters")
}

#[test]
fn bold_writes_exactly_two_asterisks() {
    let text = "press the button";
    let edited = toggle_inline(text, selection_of(text, "button"), Inline::Bold).unwrap();
    assert_eq!(edited.text, "press the **button**");
}

#[test]
fn italic_writes_exactly_one_asterisk() {
    let text = "make it so";
    let edited = toggle_inline(text, selection_of(text, "so"), Inline::Italic).unwrap();
    assert_eq!(edited.text, "make it *so*");
}

#[test]
fn strikethrough_writes_exactly_two_tildes() {
    let text = "keep the old value";
    let edited = toggle_inline(text, selection_of(text, "old value"), Inline::Strike).unwrap();
    assert_eq!(edited.text, "keep the ~~old value~~");
}

#[test]
fn the_selection_carries_the_markers_so_one_more_press_undoes_it() {
    let text = "press the button";
    let once = toggle_inline(text, selection_of(text, "button"), Inline::Bold).unwrap();
    assert_eq!(once.selection, selection_of(&once.text, "**button**"));

    let twice = toggle_inline(&once.text, once.selection, Inline::Bold).unwrap();
    assert_eq!(
        twice.text, text,
        "wrapping and unwrapping cost one press each"
    );
    assert_eq!(twice.selection, selection_of(text, "button"));
}

#[test]
fn selecting_the_words_of_an_already_marked_run_unmarks_them() {
    let text = "press the **button** now";
    let edited = toggle_inline(text, selection_of(text, "button"), Inline::Bold).unwrap();

    let expected = "press the button now";
    assert_eq!(edited.text, expected);
    assert_eq!(edited.selection, selection_of(expected, "button"));
}

#[test]
fn a_caret_gets_both_markers_and_room_to_type_between_them() {
    let text = "press the button";
    let inside_word = text.find("button").unwrap() + 3;

    let edited = toggle_inline(text, Range::caret(inside_word), Inline::Bold).unwrap();

    assert_eq!(edited.text, "press the but****ton");
    assert!(edited.selection.is_empty(), "a caret stays a caret");
    let at = edited.selection.start;
    assert_eq!(&edited.text[at - 2..at], "**", "the caret sits inside");
    assert_eq!(&edited.text[at..at + 2], "**", "the markers, not before");
}

#[test]
fn a_selection_spanning_multiple_bytes_per_character_is_wrapped_at_its_edges() {
    let text = "cafés and naïve résumé";
    let edited = toggle_inline(text, selection_of(text, "naïve"), Inline::Bold).unwrap();
    assert_eq!(edited.text, "cafés and **naïve** résumé");
}

#[test]
fn a_selection_that_splits_a_character_does_nothing_at_all() {
    let text = "naïve";
    let continuation_byte = text.find('ï').unwrap() + 1;
    assert!(
        toggle_inline(text, Range::caret(continuation_byte), Inline::Bold).is_none(),
        "a range that is not a character boundary must not produce a partial edit"
    );
}

#[test]
fn markers_are_added_where_asked_and_nothing_else_is_touched() {
    // The commands are deliberately tolerant rather than clever: they add their
    // own bytes and leave the rest of the line exactly as it was, even where that
    // produces nesting a renderer would resolve its own way.
    let text = "# Title with a *star* here";
    let edited = toggle_inline(text, selection_of(text, "a *star*"), Inline::Italic).unwrap();
    assert_eq!(edited.text, "# Title with *a *star** here");
}

#[test]
fn a_selection_at_the_very_start_and_end_of_the_text_works() {
    let text = "whole";
    let edited = toggle_inline(text, Range::new(0, text.len()).unwrap(), Inline::Bold).unwrap();
    assert_eq!(edited.text, "**whole**");
    assert_eq!(edited.selection, Range::new(0, text.len() + 4).unwrap());
}

#[test]
fn underline_and_highlight_wrap_and_unwrap_reversibly() {
    let text = "mark this word";
    let once_u = toggle_inline(text, selection_of(text, "this"), Inline::Underline).unwrap();
    assert_eq!(once_u.text, "mark <u>this</u> word");
    let twice_u = toggle_inline(&once_u.text, once_u.selection, Inline::Underline).unwrap();
    assert_eq!(twice_u.text, text);

    let once_h = toggle_inline(text, selection_of(text, "this"), Inline::Highlight).unwrap();
    assert_eq!(once_h.text, "mark ==this== word");
    let twice_h = toggle_inline(&once_h.text, once_h.selection, Inline::Highlight).unwrap();
    assert_eq!(twice_h.text, text);
}

#[test]
fn checklist_quote_code_hr_and_image_format_as_expected() {
    use nc_markdown::{
        insert_horizontal_rule, insert_image, toggle_checklist, toggle_code, toggle_quote,
    };

    let tasks = "first\nsecond";
    let checked = toggle_checklist(tasks, Range::new(0, tasks.len()).unwrap()).unwrap();
    assert_eq!(checked.text, "- [ ] first\n- [ ] second");
    let unchecked = toggle_checklist(&checked.text, checked.selection).unwrap();
    assert_eq!(unchecked.text, tasks);

    let quoted = toggle_quote(tasks, Range::new(0, tasks.len()).unwrap()).unwrap();
    assert_eq!(quoted.text, "> first\n> second");
    let unquoted = toggle_quote(&quoted.text, quoted.selection).unwrap();
    assert_eq!(unquoted.text, tasks);

    let inline_code = toggle_code(
        "run cargo test",
        selection_of("run cargo test", "cargo test"),
    )
    .expect("inline code");
    assert_eq!(inline_code.text, "run `cargo test`");

    let block_code = toggle_code(
        "fn main() {\n}",
        Range::new(0, "fn main() {\n}".len()).unwrap(),
    )
    .unwrap();
    assert_eq!(block_code.text, "```\nfn main() {\n}\n```");

    let hr = insert_horizontal_rule("above", Range::caret(5)).unwrap();
    assert_eq!(hr.text, "above\n---\n");

    let img = insert_image("see diagram", selection_of("see diagram", "diagram"), "").unwrap();
    assert_eq!(img.text, "see ![diagram]()");
}

#[test]
fn set_font_family_wraps_selection_and_updates_existing_span() {
    use nc_markdown::set_font_family;

    let text = "hello world";
    let wrapped = set_font_family(text, selection_of(text, "world"), "Fira Mono").unwrap();
    assert_eq!(
        wrapped.text,
        "hello <span style=\"font-family: 'Fira Mono'\">world</span>"
    );

    // Updating existing font family span
    let updated = set_font_family(
        &wrapped.text,
        selection_of(&wrapped.text, "world"),
        "IBM Plex Mono",
    )
    .unwrap();
    assert_eq!(
        updated.text,
        "hello <span style=\"font-family: 'IBM Plex Mono'\">world</span>"
    );
}

#[test]
fn set_font_size_wraps_selection_and_updates_existing_span() {
    use nc_markdown::set_font_size;

    let text = "hello world";
    let wrapped = set_font_size(text, selection_of(text, "world"), 18).unwrap();
    assert_eq!(
        wrapped.text,
        "hello <span style=\"font-size: 18px\">world</span>"
    );

    // Updating existing font size span
    let updated = set_font_size(&wrapped.text, selection_of(&wrapped.text, "world"), 20).unwrap();
    assert_eq!(
        updated.text,
        "hello <span style=\"font-size: 20px\">world</span>"
    );
}

#[test]
fn bold_does_not_activate_or_conflict_with_italic() {
    use nc_markdown::is_inline_active;

    let text = "hello **world** there";
    let outer_sel = selection_of(text, "**world**");
    let inner_sel = selection_of(text, "world");

    // Bold is active on bold text, but italic MUST NOT be active
    assert!(is_inline_active(text, outer_sel, Inline::Bold));
    assert!(is_inline_active(text, inner_sel, Inline::Bold));
    assert!(!is_inline_active(text, outer_sel, Inline::Italic));
    assert!(!is_inline_active(text, inner_sel, Inline::Italic));

    // Italic text has italic active, but NOT bold
    let italic_text = "hello *world* there";
    let italic_outer = selection_of(italic_text, "*world*");
    let italic_inner = selection_of(italic_text, "world");
    assert!(is_inline_active(italic_text, italic_outer, Inline::Italic));
    assert!(is_inline_active(italic_text, italic_inner, Inline::Italic));
    assert!(!is_inline_active(italic_text, italic_outer, Inline::Bold));
    assert!(!is_inline_active(italic_text, italic_inner, Inline::Bold));

    // Both bold and italic can be applied simultaneously
    let bold_and_italic = "hello ***world*** there";
    let both_outer = selection_of(bold_and_italic, "***world***");
    let both_inner = selection_of(bold_and_italic, "world");
    assert!(is_inline_active(bold_and_italic, both_outer, Inline::Bold));
    assert!(is_inline_active(
        bold_and_italic,
        both_outer,
        Inline::Italic
    ));
    assert!(is_inline_active(bold_and_italic, both_inner, Inline::Bold));
    assert!(is_inline_active(
        bold_and_italic,
        both_inner,
        Inline::Italic
    ));

    // Adding italic to bold text results in bold+italic without stripping bold
    let added_italic = toggle_inline(text, outer_sel, Inline::Italic).unwrap();
    assert_eq!(added_italic.text, "hello ***world*** there");

    // Removing italic from bold+italic restores bold
    let removed_italic =
        toggle_inline(&added_italic.text, added_italic.selection, Inline::Italic).unwrap();
    assert_eq!(removed_italic.text, "hello **world** there");
    assert!(is_inline_active(
        &removed_italic.text,
        removed_italic.selection,
        Inline::Bold
    ));
    assert!(!is_inline_active(
        &removed_italic.text,
        removed_italic.selection,
        Inline::Italic
    ));
}
