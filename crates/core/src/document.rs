//! The document: text, selection, history, and the editing operations.
//!
//! A [`Document`] is what a tab holds and what a command acts on. It owns the
//! buffer, the selection, the undo history, and the line-ending style, and it
//! exposes the editing operations the command registry names — insert, delete,
//! undo, redo, select all, copy, cut, paste.
//!
//! Nothing here talks to a window, a file, or the platform clipboard: the shell
//! drives these methods and reads the results (`docs/architecture.md` §4).

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{LineColumn, TextBuffer};
use crate::caret::{self, CaretMovement, PAGE_LINES};
use crate::clipboard::Clipboard;
use crate::dnd;
use crate::line_ending::{self, LineEnding, LineEndingReport};
use crate::mouse::{self, ClickKind};
use crate::selection::Selection;
use crate::status::StatusPosition;
use crate::tabs::TabStop;
use crate::undo::{Edit, UndoStack};
use crate::wrap::{WrappedLine, wrap_document};

/// The kind of case transformation to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// UPPERCASE
    Upper,
    /// lowercase
    Lower,
    /// Title Case
    Title,
}

/// An open document.
#[derive(Debug, Clone)]
pub struct Document {
    buffer: TextBuffer,
    selection: Selection,
    history: UndoStack,
    line_ending: LineEnding,
    line_ending_report: LineEndingReport,
    original: Option<String>,
    dirty: bool,
    word_wrap: bool,
    tab_stop: TabStop,
    desired_column: Option<usize>,
    overwrite: bool,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// A new, empty document.
    ///
    /// It uses CRLF (`docs/ui-reference.md`) and starts unmodified.
    pub fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
            selection: Selection::caret(0),
            history: UndoStack::new(),
            line_ending: LineEnding::DEFAULT,
            line_ending_report: LineEndingReport::default(),
            original: None,
            dirty: false,
            word_wrap: false,
            tab_stop: TabStop::DEFAULT,
            desired_column: None,
            overwrite: false,
        }
    }

    /// A document over `text`, keeping its line-ending style.
    ///
    /// The exact bytes handed in are remembered, so saving a document that has
    /// not been edited reproduces them byte for byte — including a file with
    /// mixed line endings, whose save behaviour is a design decision.
    pub fn from_text(text: &str) -> Self {
        let report = line_ending::detect(text);
        let mut document = Self::new();
        document.buffer = TextBuffer::from_normalized(&line_ending::to_lf(text));
        document.line_ending = report.style;
        document.line_ending_report = report;
        document.original = Some(text.to_owned());
        document
    }

    /// A document rebuilt from its parts, for session restore (`NC-F011`).
    ///
    /// `text` uses `\n` terminators and is written in `line_ending`. Nothing is
    /// remembered as the unedited original — a restored document is written
    /// from its text — and `dirty` is the unsaved-changes flag the session
    /// recorded, so a restored document with unsaved work still prompts to be
    /// saved.
    pub fn from_parts(text: &str, line_ending: LineEnding, dirty: bool) -> Self {
        let mut document = Self::new();
        document.buffer = TextBuffer::from_normalized(&line_ending::to_lf(text));
        document.line_ending = line_ending;
        document.line_ending_report = LineEndingReport::uniform(line_ending);
        document.dirty = dirty;
        document
    }

    // ---- accessors ------------------------------------------------------

    /// The document text, with `\n` terminators.
    pub fn text(&self) -> &str {
        self.buffer.as_str()
    }

    /// The style the document is saved in.
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// What the text read in looked like.
    pub fn line_ending_report(&self) -> LineEndingReport {
        self.line_ending_report
    }

    /// Whether the text read in used more than one line-ending style.
    pub fn has_mixed_line_endings(&self) -> bool {
        self.line_ending_report.mixed
    }

    /// The number of lines, which is at least one.
    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    /// The length of the text in bytes.
    pub fn byte_len(&self) -> usize {
        self.buffer.len()
    }

    /// The buffer.
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// `line`'s text, without its terminator.
    pub fn line(&self, line: usize) -> Option<&str> {
        self.buffer.line(line)
    }

    /// Every line's text, without terminators.
    pub fn lines(&self) -> std::str::Split<'_, char> {
        self.buffer.lines()
    }

    /// The selection.
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// Whether anything is selected.
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// The selected text, if the selection is not empty.
    pub fn selected_text(&self) -> Option<&str> {
        if self.selection.is_empty() {
            return None;
        }
        self.buffer.slice(self.selection.range())
    }

    /// The caret's byte offset.
    pub fn caret(&self) -> usize {
        self.selection.head()
    }

    /// The caret's line and column.
    pub fn caret_position(&self) -> LineColumn {
        self.buffer.line_column(self.selection.head())
    }

    /// Whether word wrap is on.
    pub fn word_wrap(&self) -> bool {
        self.word_wrap
    }

    /// Turns word wrap on or off.
    pub fn set_word_wrap(&mut self, enabled: bool) {
        self.word_wrap = enabled;
    }

    /// The tab-stop width in use.
    pub fn tab_stop(&self) -> TabStop {
        self.tab_stop
    }

    /// Sets the tab-stop width.
    pub fn set_tab_stop(&mut self, tab_stop: TabStop) {
        self.tab_stop = tab_stop;
    }

    /// Whether typing replaces the text ahead of the caret (overwrite mode).
    ///
    /// The overwrite behaviour is a design decision, and there is no
    /// matrix row for it yet (`ROADMAP.md` M3, "overwrite behavior if
    /// applicable"), so what the engine guarantees is deliberately narrow: an
    /// overwrite replaces one grapheme per grapheme typed and never eats the
    /// line terminator.
    pub fn overwrite(&self) -> bool {
        self.overwrite
    }

    /// Turns overwrite mode on or off.
    pub fn set_overwrite(&mut self, enabled: bool) {
        self.overwrite = enabled;
    }

    /// Flips overwrite mode, as the Insert key does, and returns the new state.
    pub fn toggle_overwrite(&mut self) -> bool {
        self.overwrite = !self.overwrite;
        self.overwrite
    }

    /// Whether the document has been edited since it was last saved.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether there is an edit to undo.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether there is an edit to redo.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The caret's position as the status bar reads it: 1-based line and column
    /// (`docs/feature-matrix.md` NC-E010).
    ///
    /// The position is read from the caret — the *end* of the selection: with
    /// text selected the status bar shows
    /// the end of the selection, and with no selection the caret. The column
    /// counts graphemes within the line.
    pub fn status_position(&self) -> StatusPosition {
        StatusPosition::of(&self.buffer, self.selection.head())
    }

    // ---- selection ------------------------------------------------------

    /// Collapses the selection to `byte` and clears the desired column.
    pub fn set_caret(&mut self, byte: usize) {
        self.selection = Selection::caret(self.buffer.clamp(byte));
        self.desired_column = None;
    }

    /// Sets the selection explicitly, as a drag or Shift+Click does.
    pub fn select(&mut self, anchor: usize, head: usize) {
        self.selection = Selection::new(self.buffer.clamp(anchor), self.buffer.clamp(head));
        self.desired_column = None;
    }

    /// Selects the whole document.
    pub fn select_all(&mut self) {
        self.selection = Selection::new(0, self.buffer.len());
        self.desired_column = None;
    }

    /// Moves the caret, collapsing the selection.
    pub fn move_caret(&mut self, movement: CaretMovement) {
        self.move_caret_by(movement, false);
    }

    /// Moves the caret, extending the selection.
    pub fn move_caret_extending(&mut self, movement: CaretMovement) {
        self.move_caret_by(movement, true);
    }

    /// Moves the caret by `movement`, optionally extending the selection.
    pub fn move_caret_by(&mut self, movement: CaretMovement, extend: bool) {
        let (selection, desired) = caret::apply(
            &self.buffer,
            self.selection,
            movement,
            extend,
            self.desired_column,
            PAGE_LINES,
        );
        self.selection = selection;
        self.desired_column = if movement.is_vertical() {
            desired
        } else {
            None
        };
    }

    // ---- editing --------------------------------------------------------

    /// Inserts `text` at the caret, replacing the selection.
    ///
    /// Any CRLF or CR in `text` — a paste, most likely — is normalised to `\n`
    /// so the buffer keeps its single-terminator invariant. Returns whether
    /// anything changed.
    pub fn insert_str(&mut self, text: &str) -> bool {
        self.insert_impl(text, true)
    }

    /// Inserts `text` without letting it merge into the previous edit.
    ///
    /// Paste uses this: one paste is one undo step even when the caret has not
    /// moved between pastes.
    pub fn insert_uncoalesced(&mut self, text: &str) -> bool {
        self.insert_impl(text, false)
    }

    /// Breaks the line at the caret (Enter).
    pub fn insert_newline(&mut self) -> bool {
        self.insert_impl("\n", false)
    }

    fn insert_impl(&mut self, text: &str, coalesce: bool) -> bool {
        let normalized = line_ending::to_lf(text);
        let range = self.selection.range();
        if normalized.is_empty() && range.is_empty() {
            return false;
        }
        let start = self.selection.start();
        let before = self.selection;
        // Overwrite mode replaces what is already there rather than pushing it
        // aside, one grapheme per grapheme typed. A newline still breaks the
        // line, and the terminator itself is never eaten, so typing over the
        // last character of a line grows that line.
        let range = if self.overwrite && range.is_empty() && !normalized.contains('\n') {
            let typed = UnicodeSegmentation::graphemes(normalized.as_str(), true).count();
            start..self.overwrite_end(start, typed)
        } else {
            range
        };
        let removed = self.buffer.slice(range.clone()).unwrap_or("").to_owned();
        self.buffer.replace(range, &normalized);
        let after = Selection::caret(start + normalized.len());
        self.selection = after;
        let typing =
            coalesce && !normalized.contains('\n') && removed.is_empty() && before.is_empty();
        self.record(
            Edit {
                start,
                removed,
                inserted: normalized,
                selection_before: before,
                selection_after: after,
            },
            typing,
        );
        true
    }

    /// Deletes the selection, or the grapheme before the caret.
    pub fn backspace(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let head = self.caret();
        if head == 0 {
            return false;
        }
        let from = self.buffer.prev_grapheme(head);
        self.delete_range(from..head)
    }

    /// Deletes the selection, or the grapheme after the caret.
    pub fn delete_forward(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let head = self.caret();
        if head >= self.buffer.len() {
            return false;
        }
        let line = self.buffer.line_of(head);
        let end = self.buffer.line_end(line).unwrap_or(self.buffer.len());
        let to = if head >= end {
            // Sitting before the terminator: delete the terminator and join the
            // lines, which is one byte because the buffer holds `\n` only.
            head + 1
        } else {
            self.buffer.next_grapheme(head)
        };
        self.delete_range(head..to)
    }

    /// Deletes the selection.
    pub fn delete_selection(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let range = self.selection.range();
        self.delete_range(range)
    }

    fn delete_range(&mut self, range: Range<usize>) -> bool {
        let removed = self.buffer.remove(range.clone());
        if removed.is_empty() {
            return false;
        }
        let before = self.selection;
        let after = Selection::caret(range.start);
        self.selection = after;
        self.desired_column = None;
        // Consecutive deletions do not merge: the grouping of a Backspace run is
        // a design decision (NC-E001), so each deletion stays
        // its own step rather than guessing.
        self.record(
            Edit {
                start: range.start,
                removed,
                inserted: String::new(),
                selection_before: before,
                selection_after: after,
            },
            false,
        );
        true
    }

    // ---- mouse and drag and drop ----------------------------------------

    /// Places the caret, or selects a word or line, as a click does.
    ///
    /// The click kind is the engine's, not the viewport's: the window shell
    /// counts clicks and reports which kind it saw
    /// (`docs/feature-matrix.md` NC-E005).
    pub fn click(&mut self, byte: usize, kind: ClickKind) {
        self.selection = mouse::click_selection(&self.buffer, byte, kind);
        self.desired_column = None;
    }

    /// Extends the selection, as Shift+Click or a drag does.
    ///
    /// The anchor stays where the selection began — the click that started the
    /// gesture — and the head follows the pointer. `kind` is that click's kind,
    /// so a drag begun with a double-click extends by whole words.
    pub fn extend_to(&mut self, byte: usize, kind: ClickKind) {
        self.selection = mouse::drag_selection(&self.buffer, self.selection.anchor(), byte, kind);
        self.desired_column = None;
    }

    /// Moves the selected text to `target`, as dropping a dragged selection
    /// does (`docs/feature-matrix.md` NC-E009).
    ///
    /// A drop inside the selection, or at either end of it, does nothing.
    /// Otherwise the move is *one* undo step however far apart the two places
    /// are: undo puts the text back where it was dragged from. Returns whether
    /// anything moved.
    pub fn move_selection_to(&mut self, target: usize) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let range = self.selection.range();
        let Some(at) = dnd::drop_target(range.clone(), self.buffer.clamp(target)) else {
            return false;
        };
        let text = self.buffer.slice(range.clone()).unwrap_or("").to_owned();
        if text.is_empty() {
            return false;
        }
        let before = self.selection;
        let removed = self.buffer.remove(range.clone());
        self.buffer.insert(at, &text);
        let after = Selection::new(at, at + text.len());
        self.history.record_group(vec![
            Edit {
                start: range.start,
                removed,
                inserted: String::new(),
                selection_before: before,
                selection_after: Selection::caret(at),
            },
            Edit {
                start: at,
                removed: String::new(),
                inserted: text,
                selection_before: Selection::caret(at),
                selection_after: after,
            },
        ]);
        self.selection = after;
        self.desired_column = None;
        self.dirty = true;
        true
    }

    /// Drops text dragged in from another application at `target`, and selects
    /// it.
    ///
    /// The text is normalised to `\n` exactly as a paste is, so the buffer's
    /// single-terminator invariant survives a drop from a program that uses
    /// CRLF. Returns whether anything was inserted.
    pub fn drop_text(&mut self, text: &str, target: usize) -> bool {
        let normalized = line_ending::to_lf(text);
        if normalized.is_empty() {
            return false;
        }
        let at = self.buffer.clamp(target);
        let before = self.selection;
        self.buffer.insert(at, &normalized);
        let after = Selection::new(at, at + normalized.len());
        self.selection = after;
        self.desired_column = None;
        self.record(
            Edit {
                start: at,
                removed: String::new(),
                inserted: normalized,
                selection_before: before,
                selection_after: after,
            },
            false,
        );
        true
    }

    /// The end of the span an overwrite at `start` replaces: `want` whole
    /// graphemes, never crossing the line terminator.
    fn overwrite_end(&self, start: usize, want: usize) -> usize {
        let line = self.buffer.line_of(start);
        let limit = self.buffer.line_end(line).unwrap_or(self.buffer.len());
        let mut end = start;
        let mut taken = 0;
        while end < limit && taken < want {
            end = self.buffer.next_grapheme(end);
            taken += 1;
        }
        end
    }

    // ---- replacing spans ------------------------------------------------

    /// Replaces `range` with `text` as one undo step.
    ///
    /// The range is clamped to the buffer and to character boundaries, so byte
    /// offsets from a search never split a character, and `text` is normalised
    /// to `\n` exactly as a paste is, so the buffer keeps its single-terminator
    /// invariant. The caret ends after the replacement and the desired column is
    /// cleared. Returns whether anything changed.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) -> bool {
        self.replace_ranges(&[(range, text)]) == 1
    }

    /// Replaces several ranges as a single undoable step, the way Replace All
    /// does (`docs/feature-matrix.md` NC-S003: "Count + undo as single action").
    ///
    /// `replacements` must be sorted by start and must not overlap — a search's
    /// match list is exactly that shape — and they are applied left to right,
    /// each one's offsets shifted by the text inserted before it. Every range is
    /// clamped to the buffer and to character boundaries. Returns how many ranges
    /// changed text: a range that is empty and stays empty is not one of them, so
    /// a Replace All over no matches is not an edit the user cannot see.
    ///
    /// The whole run is one entry in the history, so one undo takes the document
    /// back to how it was before the call.
    pub fn replace_ranges(&mut self, replacements: &[(Range<usize>, &str)]) -> usize {
        // Clamp every range against the buffer as it stands now, before any of
        // them is applied: the loop mutates the buffer, and a later range
        // measured against an already-shrunk buffer would be clamped into the
        // wrong place and silently dropped.
        let clamped: Vec<(usize, usize)> = replacements
            .iter()
            .map(|(range, _)| {
                let start = self.buffer.clamp(range.start);
                let end = self.buffer.clamp(range.end.max(start));
                (start, end)
            })
            .collect();

        let mut edits: Vec<Edit> = Vec::new();
        let mut delta = 0isize;
        let mut previous_end = 0usize;
        for (index, (_, text)) in replacements.iter().enumerate() {
            let (start, end) = clamped[index];
            debug_assert!(
                start >= previous_end,
                "replace_ranges wants sorted, non-overlapping ranges"
            );
            previous_end = end;
            if start == end && text.is_empty() {
                continue;
            }
            let normalized = line_ending::to_lf(text);
            let at = (start as isize + delta) as usize;
            let to = at + (end - start);
            let removed = self.buffer.replace(at..to, &normalized);
            let after = Selection::caret(at + normalized.len());
            edits.push(Edit {
                start: at,
                removed,
                inserted: normalized,
                selection_before: Selection::caret(at),
                selection_after: after,
            });
            delta += edits.last().map_or(0, |edit| edit.inserted.len()) as isize
                - (end - start) as isize;
        }
        if edits.is_empty() {
            return 0;
        }
        let count = edits.len();
        self.selection = edits[count - 1].selection_after;
        self.desired_column = None;
        self.dirty = true;
        self.history.record_group(edits);
        count
    }

    fn record(&mut self, edit: Edit, coalesce: bool) {
        self.history.record(edit, coalesce);
        self.dirty = true;
    }

    // ---- history --------------------------------------------------------

    /// Undoes the last edit.
    pub fn undo(&mut self) -> bool {
        let ok = self.history.undo(&mut self.buffer, &mut self.selection);
        if ok {
            self.dirty = true;
            self.desired_column = None;
        }
        ok
    }

    /// Redoes the last undone edit.
    pub fn redo(&mut self) -> bool {
        let ok = self.history.redo(&mut self.buffer, &mut self.selection);
        if ok {
            self.dirty = true;
            self.desired_column = None;
        }
        ok
    }

    /// The number of undoable edits.
    pub fn undo_depth(&self) -> usize {
        self.history.depth()
    }

    // ---- clipboard ------------------------------------------------------

    /// Copies the selection to `clipboard`, as CRLF text.
    ///
    /// Windows puts CRLF on the clipboard whatever the file uses, and paste
    /// normalises back, so a copy/paste round trip inside the document is
    /// faithful either way.
    pub fn copy_to(&self, clipboard: &mut dyn Clipboard) -> bool {
        match self.selected_text() {
            Some(text) if !text.is_empty() => {
                clipboard.set_text(line_ending::from_lf(text, LineEnding::CrLf));
                true
            }
            _ => false,
        }
    }

    /// Copies the selection to `clipboard` and deletes it.
    pub fn cut_to(&mut self, clipboard: &mut dyn Clipboard) -> bool {
        if !self.copy_to(clipboard) {
            return false;
        }
        self.delete_selection()
    }

    /// Replaces the selection with the clipboard's text.
    ///
    /// One paste is one undo step, and any CRLF in the pasted text is
    /// normalised to the buffer's `\n`.
    pub fn paste_from(&mut self, clipboard: &dyn Clipboard) -> bool {
        match clipboard.get_text() {
            Some(text) if !text.is_empty() => self.insert_uncoalesced(&text),
            _ => false,
        }
    }

    // ---- structured editing ---------------------------------------------

    /// Duplicates the selected text, or the current line if there is no selection.
    pub fn duplicate_current(&mut self) -> bool {
        if self.has_selection() {
            let range = self.selection.range();
            let Some(text) = self.buffer.slice(range.clone()).map(ToOwned::to_owned) else {
                return false;
            };
            if text.is_empty() {
                return false;
            }
            let insert_at = range.end;
            self.set_caret(insert_at);
            self.insert_uncoalesced(&text);
            self.select(insert_at, insert_at + text.len());
            true
        } else {
            let caret = self.caret();
            let line_idx = self.buffer.line_of(caret);
            let Some(line_text) = self.buffer.line(line_idx).map(ToOwned::to_owned) else {
                return false;
            };
            let line_start = self.buffer.line_start(line_idx).unwrap_or(0);
            let col_offset = caret.saturating_sub(line_start);

            let is_last_line = line_idx + 1 >= self.buffer.line_count();
            if is_last_line {
                let end = self.buffer.len();
                let to_insert = format!("\n{line_text}");
                self.set_caret(end);
                self.insert_uncoalesced(&to_insert);
                let new_caret = end + 1 + col_offset.min(line_text.len());
                self.set_caret(new_caret);
            } else {
                let next_start = self
                    .buffer
                    .line_start(line_idx + 1)
                    .unwrap_or(self.buffer.len());
                let to_insert = format!("{line_text}\n");
                self.set_caret(next_start);
                self.insert_uncoalesced(&to_insert);
                let new_caret = next_start + col_offset.min(line_text.len());
                self.set_caret(new_caret);
            }
            true
        }
    }

    /// Deletes the current line and moves the caret to the next line.
    pub fn delete_line(&mut self) -> bool {
        let caret = self.caret();
        let line_idx = self.buffer.line_of(caret);
        if self.buffer.line_count() <= 1 {
            if self.buffer.is_empty() {
                return false;
            }
            return self.delete_range(0..self.buffer.len());
        }
        let is_last_line = line_idx + 1 >= self.buffer.line_count();
        let range = if is_last_line {
            let prev_end = self.buffer.line_end(line_idx - 1).unwrap_or(0);
            prev_end..self.buffer.len()
        } else {
            let start = self.buffer.line_start(line_idx).unwrap_or(0);
            let next_start = self
                .buffer
                .line_start(line_idx + 1)
                .unwrap_or(self.buffer.len());
            start..next_start
        };
        self.delete_range(range)
    }

    /// Swaps the current line with the line above it.
    pub fn move_line_up(&mut self) -> bool {
        let caret = self.caret();
        let line_idx = self.buffer.line_of(caret);
        if line_idx == 0 {
            return false;
        }
        let prev_idx = line_idx - 1;
        let col = caret.saturating_sub(self.buffer.line_start(line_idx).unwrap_or(0));

        let prev_text = self.buffer.line(prev_idx).unwrap_or("").to_owned();
        let curr_text = self.buffer.line(line_idx).unwrap_or("").to_owned();

        let start = self.buffer.line_start(prev_idx).unwrap_or(0);
        let end = if line_idx + 1 < self.buffer.line_count() {
            self.buffer
                .line_start(line_idx + 1)
                .unwrap_or(self.buffer.len())
        } else {
            self.buffer.len()
        };

        let is_last = line_idx + 1 >= self.buffer.line_count();
        let replacement = if is_last {
            format!("{curr_text}\n{prev_text}")
        } else {
            format!("{curr_text}\n{prev_text}\n")
        };

        if self.replace_range(start..end, &replacement) {
            let new_caret = start + col.min(curr_text.len());
            self.set_caret(new_caret);
            true
        } else {
            false
        }
    }

    /// Swaps the current line with the line below it.
    pub fn move_line_down(&mut self) -> bool {
        let caret = self.caret();
        let line_idx = self.buffer.line_of(caret);
        if line_idx + 1 >= self.buffer.line_count() {
            return false;
        }
        let next_idx = line_idx + 1;
        let col = caret.saturating_sub(self.buffer.line_start(line_idx).unwrap_or(0));

        let curr_text = self.buffer.line(line_idx).unwrap_or("").to_owned();
        let next_text = self.buffer.line(next_idx).unwrap_or("").to_owned();

        let start = self.buffer.line_start(line_idx).unwrap_or(0);
        let end = if next_idx + 1 < self.buffer.line_count() {
            self.buffer
                .line_start(next_idx + 1)
                .unwrap_or(self.buffer.len())
        } else {
            self.buffer.len()
        };

        let is_last = next_idx + 1 >= self.buffer.line_count();
        let replacement = if is_last {
            format!("{next_text}\n{curr_text}")
        } else {
            format!("{next_text}\n{curr_text}\n")
        };

        if self.replace_range(start..end, &replacement) {
            let new_line_start = start + next_text.len() + 1;
            let new_caret = new_line_start + col.min(curr_text.len());
            self.set_caret(new_caret);
            true
        } else {
            false
        }
    }

    /// Joins the current line with the following line.
    pub fn join_lines(&mut self) -> bool {
        let caret = self.caret();
        let line_idx = self.buffer.line_of(caret);
        if line_idx + 1 >= self.buffer.line_count() {
            return false;
        }
        let line_end = self.buffer.line_end(line_idx).unwrap_or(self.buffer.len());
        let next_start = self
            .buffer
            .line_start(line_idx + 1)
            .unwrap_or(self.buffer.len());
        let next_line = self.buffer.line(line_idx + 1).unwrap_or("");
        let leading_whitespace = next_line.len() - next_line.trim_start().len();
        let replace_end = next_start + leading_whitespace;

        let joiner = if self.buffer.line(line_idx).unwrap_or("").ends_with(' ') {
            ""
        } else {
            " "
        };

        self.replace_range(line_end..replace_end, joiner)
    }

    /// Trims trailing whitespace from all lines in the document.
    pub fn trim_trailing_whitespace(&mut self) -> bool {
        let mut replacements = Vec::new();
        for line_idx in 0..self.buffer.line_count() {
            let start = self.buffer.line_start(line_idx).unwrap_or(0);
            let end = self.buffer.line_end(line_idx).unwrap_or(0);
            if let Some(line) = self.buffer.slice(start..end) {
                let trimmed = line.trim_end();
                if trimmed.len() < line.len() {
                    let ws_start = start + trimmed.len();
                    replacements.push((ws_start..end, ""));
                }
            }
        }
        if replacements.is_empty() {
            return false;
        }
        self.replace_ranges(&replacements) > 0
    }

    /// Sorts the lines covered by the selection (or the whole document) alphabetically.
    pub fn sort_lines(&mut self, ascending: bool) -> bool {
        let (first_line, last_line) = if self.has_selection() {
            let range = self.selection.range();
            let end_line = self.buffer.line_of(range.end);
            let start_line = self.buffer.line_of(range.start);
            if range.end > range.start
                && self.buffer.line_start(end_line) == Some(range.end)
                && end_line > start_line
            {
                (start_line, end_line - 1)
            } else {
                (start_line, end_line)
            }
        } else {
            (0, self.buffer.line_count().saturating_sub(1))
        };

        if first_line >= last_line {
            return false;
        }

        let mut lines: Vec<String> = (first_line..=last_line)
            .map(|i| self.buffer.line(i).unwrap_or("").to_owned())
            .collect();

        let original = lines.clone();
        if ascending {
            lines.sort();
        } else {
            lines.sort_by(|a, b| b.cmp(a));
        }

        if lines == original {
            return false;
        }

        let start = self.buffer.line_start(first_line).unwrap_or(0);
        let end = self.buffer.line_end(last_line).unwrap_or(self.buffer.len());
        let joined = lines.join("\n");

        self.replace_range(start..end, &joined)
    }

    /// Transforms the case of the selection or the word at the caret.
    pub fn transform_case(&mut self, case: CaseKind) -> bool {
        let (range, original) = if self.has_selection() {
            let range = self.selection.range();
            let Some(text) = self.buffer.slice(range.clone()) else {
                return false;
            };
            (range, text.to_owned())
        } else {
            let caret = self.caret();
            let word = self.buffer.word_range(caret);
            if word.is_empty() {
                return false;
            }
            let Some(text) = self.buffer.slice(word.clone()) else {
                return false;
            };
            (word, text.to_owned())
        };

        if original.is_empty() {
            return false;
        }

        let transformed = match case {
            CaseKind::Upper => original.to_uppercase(),
            CaseKind::Lower => original.to_lowercase(),
            CaseKind::Title => {
                let mut title = String::with_capacity(original.len());
                let mut start_word = true;
                for c in original.chars() {
                    if c.is_alphabetic() {
                        if start_word {
                            for uc in c.to_uppercase() {
                                title.push(uc);
                            }
                            start_word = false;
                        } else {
                            for lc in c.to_lowercase() {
                                title.push(lc);
                            }
                        }
                    } else {
                        start_word = true;
                        title.push(c);
                    }
                }
                title
            }
        };

        if transformed == original {
            return false;
        }

        let start = range.start;
        let trans_len = transformed.len();
        if self.replace_range(range, &transformed) {
            self.select(start, start + trans_len);
            true
        } else {
            false
        }
    }

    /// Selects the current line.
    pub fn select_line(&mut self) {
        let caret = self.caret();
        self.click(caret, ClickKind::Triple);
    }

    /// Selects the word around the caret.
    pub fn select_word(&mut self) {
        let caret = self.caret();
        self.click(caret, ClickKind::Double);
    }

    // ---- layout and saving ----------------------------------------------

    /// Lays the document out for a viewport `columns` wide.
    pub fn wrapped_lines(&self, columns: usize) -> Vec<WrappedLine> {
        wrap_document(&self.buffer, columns, self.tab_stop)
    }

    /// The bytes to write to disk.
    ///
    /// An unedited document is handed back exactly as it was read in — the
    /// buffered original, not a reconstruction — so ordinary save can never
    /// change a byte (`docs/test-strategy.md` §5). Once the document has been
    /// edited it is written in the document's style.
    pub fn save_text(&self) -> String {
        if !self.dirty {
            if let Some(original) = &self.original {
                return original.clone();
            }
        }
        line_ending::from_lf(self.buffer.as_str(), self.line_ending)
    }

    /// Records that the document has been saved, and what was written.
    pub fn mark_saved(&mut self) {
        self.original = Some(self.save_text());
        self.dirty = false;
    }

    /// Marks the document as differing from the file on disk without an edit
    /// having been made to it.
    ///
    /// This is what answering *Keep my version* to an outside change does
    /// (`NC-D008`): the text the user kept is not the file's anymore, so the
    /// next save has to write it. The history and the remembered original are
    /// left alone — nothing was undone, and nothing was read.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Forgets the history and the remembered original, as closing and
    /// reopening a document does.
    pub fn reset(&mut self) {
        self.history.clear();
        self.original = None;
        self.dirty = false;
        self.selection = Selection::caret(0);
        self.desired_column = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_line() {
        let mut doc = Document::from_text("first\nsecond");
        doc.set_caret(2);
        assert!(doc.duplicate_current());
        assert_eq!(doc.text(), "first\nfirst\nsecond");

        // Duplicate last line
        let mut doc2 = Document::from_text("alpha\nbeta");
        doc2.set_caret(7);
        assert!(doc2.duplicate_current());
        assert_eq!(doc2.text(), "alpha\nbeta\nbeta");
    }

    #[test]
    fn test_duplicate_selection() {
        let mut doc = Document::from_text("hello world");
        doc.select(0, 5);
        assert!(doc.duplicate_current());
        assert_eq!(doc.text(), "hellohello world");
    }

    #[test]
    fn test_delete_line() {
        let mut doc = Document::from_text("line1\nline2\nline3");
        doc.set_caret(7); // on line2
        assert!(doc.delete_line());
        assert_eq!(doc.text(), "line1\nline3");

        // Delete last line
        assert!(doc.delete_line());
        assert_eq!(doc.text(), "line1");
    }

    #[test]
    fn test_move_lines() {
        let mut doc = Document::from_text("one\ntwo\nthree");
        doc.set_caret(5); // on 'two'
        assert!(doc.move_line_up());
        assert_eq!(doc.text(), "two\none\nthree");
        assert!(doc.move_line_down());
        assert_eq!(doc.text(), "one\ntwo\nthree");
    }

    #[test]
    fn test_join_lines() {
        let mut doc = Document::from_text("hello\n   world");
        doc.set_caret(2);
        assert!(doc.join_lines());
        assert_eq!(doc.text(), "hello world");
    }

    #[test]
    fn test_trim_trailing_whitespace() {
        let mut doc = Document::from_text("line 1   \nline 2\t\t\nline 3");
        assert!(doc.trim_trailing_whitespace());
        assert_eq!(doc.text(), "line 1\nline 2\nline 3");
    }

    #[test]
    fn test_sort_lines() {
        let mut doc = Document::from_text("banana\napple\ncherry");
        assert!(doc.sort_lines(true));
        assert_eq!(doc.text(), "apple\nbanana\ncherry");
        assert!(doc.sort_lines(false));
        assert_eq!(doc.text(), "cherry\nbanana\napple");
    }

    #[test]
    fn test_transform_case() {
        let mut doc = Document::from_text("hello world");
        doc.select(0, 5);
        assert!(doc.transform_case(CaseKind::Upper));
        assert_eq!(doc.text(), "HELLO world");

        doc.select(0, 5);
        assert!(doc.transform_case(CaseKind::Lower));
        assert_eq!(doc.text(), "hello world");

        doc.select(0, 11);
        assert!(doc.transform_case(CaseKind::Title));
        assert_eq!(doc.text(), "Hello World");
    }

    #[test]
    fn test_select_word_and_line() {
        let mut doc = Document::from_text("first second third");
        doc.set_caret(8);
        doc.select_word();
        assert_eq!(doc.selected_text(), Some("second"));

        doc.select_line();
        assert_eq!(doc.selected_text(), Some("first second third"));
    }
}
