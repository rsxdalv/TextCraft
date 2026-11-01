//! The editor surface: a monospaced, zoomable text area over the document.
//!
//! The text edit lives inside a scroll area enabled on **both** axes, and that
//! enclosure is doing more than drawing bars. A `TextEdit` whose caret moves inside it
//! — a click, an arrow key, typing — asks `egui` to keep that caret in view by calling
//! `Ui::scroll_to_rect`; with no scroll area enclosing it, as here once, the request
//! went nowhere and a caret moved past the bottom of the window was simply painted off
//! screen, unreachable. A document taller than the window now scrolls, and so does a
//! line too wide for it when [`Shell::word_wrap`] is off (see [`widest_line_width`]).
//!
//! What the widget cannot see for itself is a caret moved *from outside* it. Find, Go to
//! line, Select all and the rest queue a selection which is written into the widget's
//! saved state between frames, so within its own frame the cursor never changed and it
//! does not scroll; [`scroll_caret_into_view`] is the shell making the same request on
//! those commands' behalf.
//!
//! [`Shell::word_wrap`]: crate::ui::state::Shell::word_wrap
//!
//! The text is not a `String` the shell owns: it is the `nc-core` [`Document`]
//! inside the tab's [`TextFile`](nc_textproc::TextFile), edited through a thin
//! [`egui::TextBuffer`] adapter. That keeps one authoritative copy of the text
//! — the one the file saves and the searches read.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use egui::text::{CCursor, CCursorRange, CharIndex};
use egui::{Color32, Event, FontId, Galley, Key, Shape, Stroke, WidgetType};

use crate::ui::spacing;
use nc_core::Document;

use crate::ui::a11y;
use crate::ui::markdown_view;
use crate::ui::state::{EditorMode, Shell, Tab};

/// The editor widget's id. Fixed so a queued selection can be stored under it
/// and tests can read the caret back.
pub const EDITOR_ID: &str = "textcraft_editor";

/// The colour a misspelled word is underlined in (`NC-P004`). Chosen distinct
/// from the accent so a squiggle is never read as the active-tab or selection
/// colour; the appearance suite looks for exactly this ink.
pub(crate) const SPELL_INK: Color32 = Color32::from_rgb(200, 60, 60);

/// The editor's font size in points at 100% zoom. The formatted view scales from
/// the same number, so a normal line measures the same in either view.
const EDIT_FONT_SIZE: f32 = 15.0;

/// Width of the line-numbers gutter on the left of the source editor.
pub const GUTTER_WIDTH: f32 = 40.0;

/// How much room to leave past the end of the longest line.
///
/// The text edit keeps four points of text margin inside its own rect, and the caret
/// sits *after* the last glyph rather than over it, so a rect exactly as wide as the
/// text would break the line it was sized for.
const LINE_HEADROOM: f32 = 16.0;

/// The width `text`'s longest line wants, in points, at `font`.
///
/// Laid out with [`egui::text::LayoutJob::simple`], which is what the text edit's own
/// layouter uses, so the measurement cannot disagree with the layout it is sizing the
/// widget for. Only the longest line is laid out: the rest of the document cannot be
/// wider than it, and a document of any size would otherwise be measured twice per
/// frame in the unwrapped case.
fn widest_line_width(text: &str, font: &FontId, ui: &egui::Ui) -> f32 {
    let longest = text
        .split('\n')
        .max_by_key(|line| line.chars().count())
        .unwrap_or("");
    let job = egui::text::LayoutJob::simple(
        longest.to_owned(),
        font.clone(),
        ui.visuals().strong_text_color(),
        f32::INFINITY,
    );
    ui.fonts_mut(|fonts| fonts.layout_job(job)).rect.width() + LINE_HEADROOM
}

/// Lays out `text` for the Code Editor, applying syntax highlighting (comments, strings,
/// keywords, HTML/XML tags, Markdown markers, numbers, and `<span style="color: #RRGGBB">`
/// colour runs) while keeping every byte of source in place.
fn layout_source_text(
    ui: &egui::Ui,
    text: &str,
    font: &FontId,
    palette: crate::ui::theme::Palette,
    wrap_width: f32,
) -> std::sync::Arc<Galley> {
    let base_color = ui.visuals().text_color();
    if text.is_empty() {
        let job =
            egui::text::LayoutJob::simple(text.to_owned(), font.clone(), base_color, wrap_width);
        return ui.fonts_mut(|fonts| fonts.layout_job(job));
    }

    let dim = egui::TextFormat::simple(font.clone(), palette.text_dim);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let mut cursor = 0usize;
    for span in nc_markdown::spans(text) {
        let nc_markdown::SpanKind::Color([r, g, b]) = span.kind else {
            continue;
        };
        if span.range.start > cursor {
            append_code_tokens(
                &mut job,
                &text[cursor..span.range.start],
                font,
                base_color,
                palette,
            );
        }
        job.append(
            &text[span.range.start..span.content.start],
            0.0,
            dim.clone(),
        );
        job.append(
            &text[span.content.start..span.content.end],
            0.0,
            egui::TextFormat::simple(font.clone(), Color32::from_rgb(r, g, b)),
        );
        job.append(&text[span.content.end..span.range.end], 0.0, dim.clone());
        cursor = span.range.end;
    }
    if cursor < text.len() {
        append_code_tokens(&mut job, &text[cursor..], font, base_color, palette);
    }
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// Appends syntax-highlighted tokens of `segment` into `job` using `font` so character
/// metrics and byte offsets match the raw source identically.
fn append_code_tokens(
    job: &mut egui::text::LayoutJob,
    segment: &str,
    font: &FontId,
    base_color: Color32,
    palette: crate::ui::theme::Palette,
) {
    let plain = egui::TextFormat::simple(font.clone(), base_color);
    let comment_fmt = egui::TextFormat::simple(font.clone(), palette.text_dim);
    let string_color = if palette.dark {
        Color32::from_rgb(105, 219, 124)
    } else {
        Color32::from_rgb(43, 138, 62)
    };
    let keyword_color = if palette.dark {
        Color32::from_rgb(177, 151, 252)
    } else {
        Color32::from_rgb(112, 72, 232)
    };
    let tag_color = if palette.dark {
        Color32::from_rgb(255, 135, 135)
    } else {
        Color32::from_rgb(201, 42, 42)
    };
    let number_color = if palette.dark {
        Color32::from_rgb(255, 192, 120)
    } else {
        Color32::from_rgb(217, 72, 15)
    };
    let string_fmt = egui::TextFormat::simple(font.clone(), string_color);
    let keyword_fmt = egui::TextFormat::simple(font.clone(), keyword_color);
    let tag_fmt = egui::TextFormat::simple(font.clone(), tag_color);
    let number_fmt = egui::TextFormat::simple(font.clone(), number_color);
    let accent_fmt = egui::TextFormat::simple(font.clone(), palette.accent);

    let bytes = segment.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut line_start = true;

    while i < len {
        let b = bytes[i];
        if b == b'\n' {
            job.append("\n", 0.0, plain.clone());
            i += 1;
            line_start = true;
            continue;
        }

        // Line comments `//...`
        if i + 1 < len && b == b'/' && bytes[i + 1] == b'/' {
            let end = segment[i..].find('\n').map(|rel| i + rel).unwrap_or(len);
            job.append(&segment[i..end], 0.0, comment_fmt.clone());
            i = end;
            line_start = false;
            continue;
        }

        // Markdown ATX heading prefix at start of line (`# `, `## `, etc.)
        if line_start && b == b'#' {
            let mut hashes = i;
            while hashes < len && bytes[hashes] == b'#' {
                hashes += 1;
            }
            if hashes - i <= 6 && hashes < len && bytes[hashes] == b' ' {
                let end = hashes + 1;
                job.append(&segment[i..end], 0.0, accent_fmt.clone());
                i = end;
                line_start = false;
                continue;
            }
        }

        // HTML/XML tag `<...>` on the same line
        if b == b'<' {
            let rest = &segment[i..];
            let line_end = rest.find('\n').unwrap_or(rest.len());
            if let Some(gt) = rest[..line_end].find('>') {
                if gt > 1 {
                    let end = i + gt + 1;
                    job.append(&segment[i..end], 0.0, tag_fmt.clone());
                    i = end;
                    line_start = false;
                    continue;
                }
            }
        }

        // String literal `"..."` or `'...'` on the same line
        if b == b'"' || b == b'\'' {
            let quote = b;
            let mut j = i + 1;
            let mut closed = false;
            while j < len && bytes[j] != b'\n' {
                if bytes[j] == b'\\' && j + 1 < len {
                    j += 2;
                    continue;
                }
                if bytes[j] == quote {
                    j += 1;
                    closed = true;
                    break;
                }
                j += 1;
            }
            if closed {
                job.append(&segment[i..j], 0.0, string_fmt.clone());
                i = j;
                line_start = false;
                continue;
            }
        }

        // Inline markdown delimiters (`**`, `~~`, `==`, `` ` ``)
        if i + 1 < len
            && ((b == b'*' && bytes[i + 1] == b'*')
                || (b == b'~' && bytes[i + 1] == b'~')
                || (b == b'=' && bytes[i + 1] == b'='))
        {
            job.append(&segment[i..i + 2], 0.0, accent_fmt.clone());
            i += 2;
            line_start = false;
            continue;
        }
        if b == b'`' {
            job.append(&segment[i..i + 1], 0.0, accent_fmt.clone());
            i += 1;
            line_start = false;
            continue;
        }

        // Identifiers and keywords
        if b.is_ascii_alphabetic() || b == b'_' {
            let mut j = i + 1;
            while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let word = &segment[i..j];
            let fmt = if is_code_keyword(word) {
                keyword_fmt.clone()
            } else {
                plain.clone()
            };
            job.append(word, 0.0, fmt);
            i = j;
            line_start = false;
            continue;
        }

        // Numeric literals
        if b.is_ascii_digit() {
            let mut j = i + 1;
            while j < len
                && (bytes[j].is_ascii_hexdigit()
                    || bytes[j] == b'.'
                    || bytes[j] == b'_'
                    || bytes[j] == b'x')
            {
                j += 1;
            }
            job.append(&segment[i..j], 0.0, number_fmt.clone());
            i = j;
            line_start = false;
            continue;
        }

        // Any other character (UTF-8 safe)
        let ch = segment[i..].chars().next().unwrap_or(' ');
        let step = ch.len_utf8();
        if !ch.is_whitespace() {
            line_start = false;
        }
        job.append(&segment[i..i + step], 0.0, plain.clone());
        i += step;
    }
}

fn is_code_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let"
            | "mut"
            | "pub"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "use"
            | "mod"
            | "return"
            | "if"
            | "else"
            | "match"
            | "for"
            | "while"
            | "loop"
            | "break"
            | "continue"
            | "async"
            | "await"
            | "const"
            | "static"
            | "type"
            | "where"
            | "in"
            | "as"
            | "true"
            | "false"
            | "Self"
            | "self"
            | "super"
            | "crate"
            | "def"
            | "class"
            | "import"
            | "from"
            | "function"
            | "var"
            | "export"
            | "default"
            | "interface"
    )
}

/// Visual styling accumulated for a run of visible characters in [`EditorMode::Text`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct RichStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    highlight: bool,
    code: bool,
    link: bool,
    dim: bool,
    color: Option<[u8; 3]>,
    scale: f32,
}

impl Default for RichStyle {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            highlight: false,
            code: false,
            link: false,
            dim: false,
            color: None,
            scale: 1.0,
        }
    }
}

/// A styled slice of `RichProjection::visible`.
#[derive(Clone, Debug, PartialEq)]
struct RichRun {
    vis_range: Range<usize>,
    style: RichStyle,
}

/// Line-level style modifier (`> ` blockquote or `# `..`###### ` heading) active for
/// the current line until the next `\n`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct LineStyleMod {
    scale: f32,
    bold: bool,
    italic: bool,
    dim: bool,
}

/// Bidirectional projection between raw document source (`tab.text()`) and the
/// tag-free visible string displayed in [`EditorMode::Text`].
#[derive(Clone, Debug, Default)]
struct RichProjection {
    /// Visible text with inline tags/markers (`<span style="color: ...">`, `**`, `*`,
    /// `<u>`, `~~`, `==`, `` ` ``, `# `) stripped, including across multi-line spans.
    visible: String,
    /// For each character in `visible`, the start byte offset in `source`.
    vis_to_src_start: Vec<usize>,
    /// For each character in `visible`, the end byte offset in `source`.
    vis_to_src_end: Vec<usize>,
    /// Styled runs covering `0..visible.len()`.
    runs: Vec<RichRun>,
    /// `(visible_char_index, source_byte_offset)` pairs for empty formatting wrappers
    /// or empty heading prefixes so typing at that caret inserts inside the wrapper.
    empty_anchors: Vec<(usize, usize)>,
    /// Byte length of the underlying `source`.
    source_len: usize,
    /// Whether the parser is currently positioned at the start of a line (before any
    /// visible character on that line).
    at_line_start: bool,
    ///Whether a line-start prefix (`> ` or `# `) has already been consumed on the
    /// current line.
    line_prefix_consumed: bool,
    /// Active line-level modifier (heading scale/bold or quote italic/dim) until `\n`.
    current_line_style: Option<LineStyleMod>,
}

impl RichProjection {
    fn build(source: &str) -> Self {
        let mut proj = Self {
            visible: String::with_capacity(source.len()),
            vis_to_src_start: Vec::with_capacity(source.len()),
            vis_to_src_end: Vec::with_capacity(source.len()),
            runs: Vec::new(),
            empty_anchors: Vec::new(),
            source_len: source.len(),
            at_line_start: true,
            line_prefix_consumed: false,
            current_line_style: None,
        };

        proj.parse_segment(source, 0..source.len(), RichStyle::default());
        proj
    }

    /// Consumes a line-start blockquote (`> `) or ATX heading (`# `..`###### `) prefix
    /// if the parser is at the start of a visible line.
    fn try_consume_line_prefix(&mut self, source: &str, i: usize, limit: usize) -> usize {
        if !self.at_line_start || self.line_prefix_consumed || i >= limit {
            return 0;
        }
        let line_end = source[i..limit]
            .find('\n')
            .map(|rel| i + rel)
            .unwrap_or(limit);
        let line = &source[i..line_end];
        let mut consumed = 0usize;
        let mut modifier = LineStyleMod {
            scale: 1.0,
            bold: false,
            italic: false,
            dim: false,
        };

        let after_quote = if let Some(rest) = line.strip_prefix("> ") {
            consumed += 2;
            modifier.italic = true;
            modifier.dim = true;
            rest
        } else {
            line
        };

        let hashes = after_quote.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hashes) && after_quote.as_bytes().get(hashes) == Some(&b' ') {
            consumed += hashes + 1;
            modifier.bold = true;
            modifier.scale = match hashes {
                1 => 1.65,
                2 => 1.40,
                3 => 1.22,
                4 => 1.12,
                5 => 1.05,
                _ => 1.0,
            };
        }

        if consumed > 0 {
            self.line_prefix_consumed = true;
            self.current_line_style = Some(modifier);
            if i + consumed == line_end {
                self.empty_anchors
                    .push((self.vis_to_src_start.len(), i + consumed));
            }
        }
        consumed
    }

    fn parse_segment(&mut self, source: &str, range: Range<usize>, mut style: RichStyle) {
        let mut i = range.start;
        while i < range.end {
            // Newline resets line-start state and emits `\n`.
            if source.as_bytes()[i] == b'\n' {
                self.push_char('\n', i, i + 1, RichStyle::default());
                i += 1;
                continue;
            }

            // At the start of a line, check for `> ` or `# `..`###### ` prefixes.
            let prefix_len = self.try_consume_line_prefix(source, i, range.end);
            if prefix_len > 0 {
                i += prefix_len;
                continue;
            }

            let s = &source[i..range.end];

            // 1. `<span style="color: #RRGGBB">...</span>` (single-line or multi-line!)
            if let Some((open_len, inner_len, close_len, rgb)) = match_color_span(s) {
                let inner_start = i + open_len;
                let inner_end = inner_start + inner_len;
                let mut inner_style = style;
                inner_style.color = Some(rgb);
                if inner_start == inner_end {
                    self.empty_anchors
                        .push((self.vis_to_src_start.len(), inner_start));
                } else {
                    self.parse_segment(source, inner_start..inner_end, inner_style);
                }
                i = inner_end + close_len;
                continue;
            }

            // Stray/unclosed `<span style="color: #RRGGBB">` or `</span>`: hide tag and apply colour.
            if let Some((open_len, rgb)) = match_open_color_tag(s) {
                style.color = Some(rgb);
                i += open_len;
                continue;
            }
            if s.starts_with("</span>") {
                i += "</span>".len();
                continue;
            }

            // 2. `<u>...</u>` (single-line or multi-line!)
            if let Some((open_len, inner_len, close_len)) = match_delimited(s, "<u>", "</u>") {
                let inner_start = i + open_len;
                let inner_end = inner_start + inner_len;
                let mut inner_style = style;
                inner_style.underline = true;
                if inner_start == inner_end {
                    self.empty_anchors
                        .push((self.vis_to_src_start.len(), inner_start));
                } else {
                    self.parse_segment(source, inner_start..inner_end, inner_style);
                }
                i = inner_end + close_len;
                continue;
            }
            if s.starts_with("<u>") {
                style.underline = true;
                i += 3;
                continue;
            }
            if s.starts_with("</u>") {
                style.underline = false;
                i += 4;
                continue;
            }

            // 3. `**...**` (single-line or multi-line!)
            if let Some((open_len, inner_len, close_len)) = match_delimited(s, "**", "**") {
                let inner_start = i + open_len;
                let inner_end = inner_start + inner_len;
                let mut inner_style = style;
                inner_style.bold = true;
                if inner_start == inner_end {
                    self.empty_anchors
                        .push((self.vis_to_src_start.len(), inner_start));
                } else {
                    self.parse_segment(source, inner_start..inner_end, inner_style);
                }
                i = inner_end + close_len;
                continue;
            }

            // 4. `~~...~~` (single-line or multi-line!)
            if let Some((open_len, inner_len, close_len)) = match_delimited(s, "~~", "~~") {
                let inner_start = i + open_len;
                let inner_end = inner_start + inner_len;
                let mut inner_style = style;
                inner_style.strike = true;
                if inner_start == inner_end {
                    self.empty_anchors
                        .push((self.vis_to_src_start.len(), inner_start));
                } else {
                    self.parse_segment(source, inner_start..inner_end, inner_style);
                }
                i = inner_end + close_len;
                continue;
            }

            // 5. `==...==` (single-line or multi-line!)
            if let Some((open_len, inner_len, close_len)) = match_delimited(s, "==", "==") {
                let inner_start = i + open_len;
                let inner_end = inner_start + inner_len;
                let mut inner_style = style;
                inner_style.highlight = true;
                if inner_start == inner_end {
                    self.empty_anchors
                        .push((self.vis_to_src_start.len(), inner_start));
                } else {
                    self.parse_segment(source, inner_start..inner_end, inner_style);
                }
                i = inner_end + close_len;
                continue;
            }

            // 6. `` `...` ``
            if !s.starts_with("```") {
                if let Some((open_len, inner_len, close_len)) = match_delimited(s, "`", "`") {
                    let inner_start = i + open_len;
                    let inner_end = inner_start + inner_len;
                    let mut inner_style = style;
                    inner_style.code = true;
                    if inner_start == inner_end {
                        self.empty_anchors
                            .push((self.vis_to_src_start.len(), inner_start));
                    } else {
                        for (rel, ch) in source[inner_start..inner_end].char_indices() {
                            let b0 = inner_start + rel;
                            self.push_char(ch, b0, b0 + ch.len_utf8(), inner_style);
                        }
                    }
                    i = inner_end + close_len;
                    continue;
                }
            }

            // 7. `*...*` (single asterisk italic, single-line or multi-line, not `**` or bullet `* `)
            if s.starts_with('*')
                && !s.starts_with("**")
                && !s.starts_with("* ")
                && !s.starts_with("*\n")
            {
                if let Some(close_rel) = find_single_asterisk_close(&s[1..]) {
                    let inner_start = i + 1;
                    let inner_end = inner_start + close_rel;
                    let mut inner_style = style;
                    inner_style.italic = true;
                    if inner_start == inner_end {
                        self.empty_anchors
                            .push((self.vis_to_src_start.len(), inner_start));
                    } else {
                        self.parse_segment(source, inner_start..inner_end, inner_style);
                    }
                    i = inner_end + 1;
                    continue;
                }
            }

            // 8. `[label](url)` link
            if s.starts_with('[') {
                if let Some((label_len, total_len)) = match_markdown_link(s) {
                    let inner_start = i + 1;
                    let inner_end = inner_start + label_len;
                    let mut inner_style = style;
                    inner_style.link = true;
                    inner_style.underline = true;
                    if inner_start == inner_end {
                        self.empty_anchors
                            .push((self.vis_to_src_start.len(), inner_start));
                    } else {
                        self.parse_segment(source, inner_start..inner_end, inner_style);
                    }
                    i += total_len;
                    continue;
                }
            }

            let ch = s.chars().next().unwrap_or(' ');
            let len = ch.len_utf8();
            self.push_char(ch, i, i + len, style);
            i += len;
        }
    }

    fn push_char(&mut self, ch: char, src_start: usize, src_end: usize, mut style: RichStyle) {
        if ch == '\n' {
            self.at_line_start = true;
            self.line_prefix_consumed = false;
            self.current_line_style = None;
            style = RichStyle::default();
        } else {
            self.at_line_start = false;
            if let Some(line_mod) = self.current_line_style {
                style.scale *= line_mod.scale;
                style.bold |= line_mod.bold;
                style.italic |= line_mod.italic;
                style.dim |= line_mod.dim;
            }
        }

        let vis_start = self.visible.len();
        self.visible.push(ch);
        let vis_end = self.visible.len();
        self.vis_to_src_start.push(src_start);
        self.vis_to_src_end.push(src_end);

        if let Some(last) = self.runs.last_mut() {
            if last.style == style && last.vis_range.end == vis_start {
                last.vis_range.end = vis_end;
                return;
            }
        }
        self.runs.push(RichRun {
            vis_range: vis_start..vis_end,
            style,
        });
    }

    fn has_bold(&self) -> bool {
        self.runs.iter().any(|run| run.style.bold)
    }

    /// Maps a byte offset in `source` to the corresponding character index in `visible`.
    fn src_byte_to_vis_char(&self, byte: usize) -> usize {
        for (idx, (&start, &end)) in self
            .vis_to_src_start
            .iter()
            .zip(self.vis_to_src_end.iter())
            .enumerate()
        {
            if byte <= start {
                return idx;
            }
            if byte < end {
                return idx;
            }
        }
        self.vis_to_src_start.len()
    }

    /// Maps a visible character index back to the byte offset in `source` where typed
    /// text should be inserted.
    fn vis_char_to_src_insert_byte(&self, vis_char: usize, doc_caret: usize) -> usize {
        // Honour an explicit empty wrapper anchor at this visible index first.
        for &(anchor_vis, anchor_byte) in &self.empty_anchors {
            if anchor_vis == vis_char && anchor_byte == doc_caret {
                return anchor_byte;
            }
        }
        if let Some(&(_, anchor_byte)) = self
            .empty_anchors
            .iter()
            .find(|&&(anchor_vis, _)| anchor_vis == vis_char)
        {
            // If the visible line or document is empty at `vis_char`, insert inside the
            // heading/wrapper anchor.
            let at_line_start = vis_char == 0
                || self
                    .visible
                    .chars()
                    .nth(vis_char.saturating_sub(1))
                    .is_some_and(|ch| ch == '\n');
            let at_line_end = vis_char >= self.vis_to_src_start.len()
                || self
                    .visible
                    .chars()
                    .nth(vis_char)
                    .is_some_and(|ch| ch == '\n');
            if at_line_start && at_line_end {
                return anchor_byte;
            }
        }

        if vis_char == 0 {
            return self.vis_to_src_start.first().copied().unwrap_or(0);
        }

        // If the previous visible character is on the same line (not `\n`), inserting
        // right at its `src_end` keeps the new character inside any active inline span
        // (`**...**`, `<span style="color: ...">...</span>`, etc.).
        let prev_idx = vis_char - 1;
        if let Some(&prev_end) = self.vis_to_src_end.get(prev_idx) {
            let prev_is_newline = self
                .visible
                .chars()
                .nth(prev_idx)
                .is_some_and(|ch| ch == '\n');
            if !prev_is_newline {
                return prev_end;
            }
        }

        if let Some(&cur_start) = self.vis_to_src_start.get(vis_char) {
            return cur_start;
        }

        self.source_len
    }

    /// Maps a non-empty visible character range `vis_start..vis_end` to the spanning
    /// byte range in `source`.
    fn vis_range_to_src_bytes(&self, vis_start: usize, vis_end: usize) -> (usize, usize) {
        if self.vis_to_src_start.is_empty() || vis_start >= vis_end {
            let b = self.vis_char_to_src_insert_byte(vis_start, 0);
            return (b, b);
        }
        let clamped_start = vis_start.min(self.vis_to_src_start.len() - 1);
        let clamped_end = vis_end.min(self.vis_to_src_end.len()) - 1;
        let start_byte = self.vis_to_src_start[clamped_start];
        let end_byte = self.vis_to_src_end[clamped_end];
        (start_byte, end_byte.max(start_byte))
    }
}

fn match_color_span(s: &str) -> Option<(usize, usize, usize, [u8; 3])> {
    const PREFIX: &str = "<span style=\"color: ";
    const CLOSE: &str = "</span>";
    if !s.starts_with(PREFIX) {
        return None;
    }
    let after_prefix = &s[PREFIX.len()..];
    let quote_gt = after_prefix.find("\">")?;
    let rgb = parse_hex_color(&after_prefix[..quote_gt])?;
    let open_len = PREFIX.len() + quote_gt + 2;

    // Find the matching `</span>`, accounting for nested `<span style="color: ...">` tags.
    let rest = &s[open_len..];
    let mut depth = 1usize;
    let mut pos = 0usize;
    while pos < rest.len() {
        if rest[pos..].starts_with(PREFIX) {
            depth += 1;
            pos += PREFIX.len();
        } else if rest[pos..].starts_with(CLOSE) {
            depth -= 1;
            if depth == 0 {
                return Some((open_len, pos, CLOSE.len(), rgb));
            }
            pos += CLOSE.len();
        } else {
            let step = rest[pos..].chars().next().map_or(1, char::len_utf8);
            pos += step;
        }
    }
    None
}

fn match_open_color_tag(s: &str) -> Option<(usize, [u8; 3])> {
    const PREFIX: &str = "<span style=\"color: ";
    if !s.starts_with(PREFIX) {
        return None;
    }
    let after_prefix = &s[PREFIX.len()..];
    let quote_gt = after_prefix.find("\">")?;
    let rgb = parse_hex_color(&after_prefix[..quote_gt])?;
    Some((PREFIX.len() + quote_gt + 2, rgb))
}

fn parse_hex_color(raw: &str) -> Option<[u8; 3]> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

fn match_delimited(s: &str, open: &str, close: &str) -> Option<(usize, usize, usize)> {
    if !s.starts_with(open) {
        return None;
    }
    let rest = &s[open.len()..];
    let close_rel = rest.find(close)?;
    Some((open.len(), close_rel, close.len()))
}

fn find_single_asterisk_close(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'*' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2;
                continue;
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

fn match_markdown_link(s: &str) -> Option<(usize, usize)> {
    if !s.starts_with('[') {
        return None;
    }
    let rest = &s[1..];
    let mid = rest.find("](")?;
    let after_mid = &rest[mid + 2..];
    let close = after_mid.find(')')?;
    Some((mid, 1 + mid + 2 + close + 1))
}

/// Removes empty inline formatting wrappers (`****`, `<u></u>`, `~~~~`, `====`,
/// ```` `` ````, `<span style="color: #RRGGBB"></span>`) left behind when the last
/// visible character inside a formatted word is deleted in [`EditorMode::Text`].
fn cleanup_empty_wrappers(text: &mut String) {
    loop {
        let before = text.len();
        for pat in ["****", "~~~~", "====", "<u></u>", "``"] {
            if text.contains(pat) {
                *text = text.replace(pat, "");
            }
        }
        while let Some(range) = find_empty_color_span(text) {
            text.replace_range(range, "");
        }
        if text.len() == before {
            break;
        }
    }
}

fn find_empty_color_span(text: &str) -> Option<Range<usize>> {
    const PREFIX: &str = "<span style=\"color: ";
    const EMPTY_TAIL: &str = "\"></span>";
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(PREFIX) {
        let start = search_from + rel;
        let after = start + PREFIX.len();
        if let Some(tail_rel) = text[after..].find(EMPTY_TAIL) {
            if parse_hex_color(&text[after..after + tail_rel]).is_some() {
                let end = after + tail_rel + EMPTY_TAIL.len();
                return Some(start..end);
            }
        }
        search_from = after;
    }
    None
}

/// Lays out `text` (`projection.visible`) for [`EditorMode::Text`] with visual styles
/// (font scaling for headings, custom text colours, highlight background, italics,
/// underline, strikethrough, inline code chip, and link styling).
fn layout_rich_text(
    ui: &egui::Ui,
    text: &str,
    projection: &RichProjection,
    base_font: &FontId,
    palette: crate::ui::theme::Palette,
    wrap_width: f32,
) -> std::sync::Arc<Galley> {
    let base_color = ui.visuals().text_color();
    if text.is_empty() || projection.visible != text {
        let job = egui::text::LayoutJob::simple(
            text.to_owned(),
            base_font.clone(),
            base_color,
            wrap_width,
        );
        return ui.fonts_mut(|fonts| fonts.layout_job(job));
    }

    let highlight_bg = if palette.dark {
        Color32::from_rgba_unmultiplied(250, 176, 5, 75)
    } else {
        Color32::from_rgba_unmultiplied(255, 224, 102, 160)
    };

    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    for run in &projection.runs {
        let slice = &text[run.vis_range.clone()];
        let s = run.style;
        let font_id = FontId::new(base_font.size * s.scale, base_font.family.clone());
        let fg = if let Some([r, g, b]) = s.color {
            Color32::from_rgb(r, g, b)
        } else if s.link || s.code {
            palette.accent
        } else if s.dim {
            palette.text_dim
        } else {
            base_color
        };
        let mut format = egui::TextFormat::simple(font_id, fg);
        format.italics = s.italic;
        if s.underline {
            format.underline = Stroke::new(1.0, fg);
        }
        if s.strike {
            format.strikethrough = Stroke::new(1.0, fg);
        }
        if s.highlight {
            format.background = highlight_bg;
        } else if s.code {
            format.background = palette.chrome;
        }
        job.append(slice, 0.0, format);
    }

    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// Paints a slight horizontal overprint pass for runs with `bold: true` so bold words
/// and headings in [`EditorMode::Text`] appear visually bold in any font and colour.
fn paint_rich_bold_pass(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    projection: &RichProjection,
    base_font: &FontId,
    palette: crate::ui::theme::Palette,
) {
    if !projection.has_bold() || projection.visible.is_empty() {
        return;
    }
    let base_color = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = output.galley.job.wrap.max_width;

    for run in &projection.runs {
        let slice = &projection.visible[run.vis_range.clone()];
        let s = run.style;
        let font_id = FontId::new(base_font.size * s.scale, base_font.family.clone());
        let fg = if s.bold {
            if let Some([r, g, b]) = s.color {
                Color32::from_rgb(r, g, b)
            } else if s.link || s.code {
                palette.accent
            } else if s.dim {
                palette.text_dim
            } else {
                base_color
            }
        } else {
            Color32::TRANSPARENT
        };
        let mut format = egui::TextFormat::simple(font_id, fg);
        format.italics = s.italic;
        job.append(slice, 0.0, format);
    }

    let bold_galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    ui.painter().galley(
        output.galley_pos + egui::vec2(0.6, 0.0),
        bold_galley,
        Color32::TRANSPARENT,
    );
}

/// Draws the editor for the active tab, updates the caret position, and applies
/// any selection the shell has queued (Find / Go to).
pub fn show(shell: &mut Shell, ui: &mut egui::Ui) {
    // Taken before the document is borrowed: a search or Go to sets this and
    // the editor applies it this frame.
    let pending = shell.pending_selection.take();

    egui::CentralPanel::no_frame()
        .frame(
            egui::Frame::NONE
                .fill(shell.palette.bg)
                .inner_margin(spacing::margin(spacing::LG, spacing::MD)),
        )
        .show(ui, |ui| {
            let index = shell.active;
            if index >= shell.tabs.len() {
                return;
            }
            let palette = shell.palette;
            let zoom = shell.zoom as f32 / 100.0;

            // The formatted view is derived from the source and read-only, so it
            // claims no caret: the document keeps the selection it already holds for
            // the whole time it is shown, which is why the switch cannot lose one.
            // All the view has to keep honest is the status bar's line and column.
            let caret = if shell.formatted_view() {
                let tab = &shell.tabs[index];
                markdown_view::show(tab, ui, EDIT_FONT_SIZE * zoom, palette);
                // A rendering of the text is not a text field, so the document is
                // exposed to a screen reader here instead: without this the body
                // of the window reads as empty (`NC-P007`).
                a11y::document(
                    ui.ctx(),
                    ui.id().with(("formatted_document", index)),
                    ui.max_rect(),
                    tab.title(),
                    tab.text(),
                );
                char_index(tab.text(), tab.document().caret())
            } else if shell.split_view() {
                let mut caret = shell.caret;
                ui.columns(2, |columns| {
                    let (left, right) = columns.split_at_mut(1);
                    caret = source_view(shell, index, &mut left[0], pending);
                    let split_x = right[0].max_rect().left() - spacing::SM * 0.5;
                    right[0].painter().vline(
                        split_x,
                        right[0].max_rect().y_range(),
                        egui::Stroke::new(1.0, palette.line),
                    );
                    let tab = &shell.tabs[index];
                    markdown_view::show(tab, &mut right[0], EDIT_FONT_SIZE * zoom, palette);
                });
                caret
            } else {
                source_view(shell, index, ui, pending)
            };

            shell.caret = caret;
            let (line, column) = line_column(shell.tabs[index].text(), caret);
            shell.line = line;
            shell.column = column;
        });
}

/// Dispatches to either the WYSIWYG Text Editor (`EditorMode::Text`) or the full
/// Code Editor (`EditorMode::Code`) and returns the source character index of the caret.
fn source_view(
    shell: &mut Shell,
    index: usize,
    ui: &mut egui::Ui,
    pending: Option<(usize, usize)>,
) -> usize {
    match shell.editor_mode {
        EditorMode::Text => rich_text_view(shell, index, ui, pending),
        EditorMode::Code => code_editor_view(shell, index, ui, pending),
    }
}

/// Draws the WYSIWYG rich text editor (`EditorMode::Text`), hiding inline formatting
/// tags (`<span style="color: ...">`, `**`, `*`, `<u>`, `~~`, `==`, `# `) and omitting
/// the code line-numbers gutter while keeping the underlying document editable.
fn rich_text_view(
    shell: &mut Shell,
    index: usize,
    ui: &mut egui::Ui,
    pending: Option<(usize, usize)>,
) -> usize {
    let wrap = shell.word_wrap;
    let spell_check = shell.spell_check;
    let zoom = shell.zoom as f32 / 100.0;
    let font = FontId::monospace(EDIT_FONT_SIZE * zoom);
    let initial_proj = RichProjection::build(shell.tabs[index].text());
    let natural = if wrap {
        0.0
    } else {
        widest_line_width(&initial_proj.visible, &font, ui)
    };

    let palette = shell.palette;
    let viewport = ui.available_height();
    egui::ScrollArea::both()
        .id_salt("editor_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let title = shell.tabs[index].title();
            let tab = &mut shell.tabs[index];
            let shared_proj = Rc::new(RefCell::new(initial_proj.clone()));
            let mut buffer = RichDocBuffer {
                document: tab.document_mut(),
                projection: initial_proj,
                shared: Rc::clone(&shared_proj),
            };

            let output = {
                let shared_for_layout = Rc::clone(&shared_proj);
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                    let proj = shared_for_layout.borrow();
                    layout_rich_text(ui, buf.as_str(), &proj, &font, palette, wrap_width)
                };
                let edit = egui::TextEdit::multiline(&mut buffer)
                    .id(egui::Id::new(EDITOR_ID))
                    .font(font.clone())
                    .layouter(&mut layouter)
                    .frame(egui::Frame::NONE);
                if wrap {
                    edit.desired_width(ui.available_width())
                        .min_size(egui::vec2(ui.available_width(), viewport))
                        .show(ui)
                } else {
                    edit.min_size(egui::vec2(natural.max(ui.available_width()), viewport))
                        .desired_width(natural)
                        .show(ui)
                }
            };

            let projection = buffer.projection;
            paint_rich_bold_pass(ui, &output, &projection, &font, palette);

            a11y::label(&output.response, WidgetType::TextEdit, &title);

            // Convert between visible character indices (spoken by the widget) and
            // source byte/character indices (spoken by the Document and Shell).
            let (mut caret_src_char, mut vis_caret) = match pending {
                Some((primary_src_char, secondary_src_char)) => {
                    let (primary_byte, secondary_byte) = {
                        let text = tab.text();
                        (
                            byte_index(text, primary_src_char),
                            byte_index(text, secondary_src_char),
                        )
                    };
                    let vis_primary = projection.src_byte_to_vis_char(primary_byte);
                    let vis_secondary = projection.src_byte_to_vis_char(secondary_byte);
                    apply_selection(ui.ctx(), &output, vis_primary, vis_secondary);
                    tab.document_mut().select(secondary_byte, primary_byte);
                    scroll_caret_into_view(ui, &output, vis_primary);
                    (primary_src_char, vis_primary)
                }
                None => match output.cursor_range {
                    Some(range) => {
                        let vis_primary = usize::from(range.primary.index);
                        let vis_secondary = usize::from(range.secondary.index);
                        let sel = tab.document().selection();
                        let (cur_anchor, cur_head) = (sel.anchor(), sel.head());
                        let (anchor_byte, head_byte) = if vis_primary == vis_secondary {
                            if cur_anchor == cur_head
                                && projection.src_byte_to_vis_char(cur_head) == vis_primary
                            {
                                (cur_head, cur_head)
                            } else {
                                let b =
                                    projection.vis_char_to_src_insert_byte(vis_primary, cur_head);
                                (b, b)
                            }
                        } else {
                            let vis_min = vis_primary.min(vis_secondary);
                            let vis_max = vis_primary.max(vis_secondary);
                            let (b_min, b_max) =
                                projection.vis_range_to_src_bytes(vis_min, vis_max);
                            if vis_primary >= vis_secondary {
                                (b_min, b_max)
                            } else {
                                (b_max, b_min)
                            }
                        };
                        tab.document_mut().select(anchor_byte, head_byte);
                        (char_index(tab.text(), head_byte), vis_primary)
                    }
                    None => (
                        shell.caret,
                        projection.src_byte_to_vis_char(byte_index(tab.text(), shell.caret)),
                    ),
                },
            };

            if let Some((new_vis_primary, new_vis_secondary)) =
                step_page_cursor(ui, &output, vis_caret, viewport)
            {
                apply_selection(ui.ctx(), &output, new_vis_primary, new_vis_secondary);
                let cur_head = tab.document().caret();
                let (anchor_byte, head_byte) = if new_vis_primary == new_vis_secondary {
                    let b = projection.vis_char_to_src_insert_byte(new_vis_primary, cur_head);
                    (b, b)
                } else {
                    let vis_min = new_vis_primary.min(new_vis_secondary);
                    let vis_max = new_vis_primary.max(new_vis_secondary);
                    let (b_min, b_max) = projection.vis_range_to_src_bytes(vis_min, vis_max);
                    if new_vis_primary >= new_vis_secondary {
                        (b_min, b_max)
                    } else {
                        (b_max, b_min)
                    }
                };
                tab.document_mut().select(anchor_byte, head_byte);
                scroll_caret_into_view(ui, &output, new_vis_primary);
                caret_src_char = char_index(tab.text(), head_byte);
                vis_caret = new_vis_primary;
            }

            let metrics = (resolved_face(ui.ctx()), font.size);
            let key = egui::Id::new("editor-laid-out-in");
            let moved = ui
                .ctx()
                .data(|data| data.get_temp::<(String, f32)>(key))
                .is_none_or(|(face, size)| face != metrics.0 || size != metrics.1);
            if moved {
                scroll_caret_into_view(ui, &output, vis_caret);
            }
            ui.ctx().data_mut(|data| data.insert_temp(key, metrics));

            if spell_check {
                tab.refresh_spellings();
                paint_misspellings_rich(ui, &output, tab, &projection);
            }
            caret_src_char
        })
        .inner
}

/// Draws the full Code Editor (`EditorMode::Code`) with raw source tags, syntax
/// highlighting, and the line-numbers gutter.
fn code_editor_view(
    shell: &mut Shell,
    index: usize,
    ui: &mut egui::Ui,
    pending: Option<(usize, usize)>,
) -> usize {
    let wrap = shell.word_wrap;
    let spell_check = shell.spell_check;
    let zoom = shell.zoom as f32 / 100.0;
    let font = FontId::monospace(EDIT_FONT_SIZE * zoom);
    // Measured before the document is borrowed for the widget below, and only when it
    // is needed: the longest line alone, not a second layout of the whole document.
    let natural = if wrap {
        0.0
    } else {
        widest_line_width(shell.tabs[index].text(), &font, ui)
    };

    // One area, both axes. Vertical, because a document is usually taller than the
    // window and the rows below the fold have to be reachable — and because the text
    // edit's own keep-the-caret-in-view request needs an area to hear it. Horizontal,
    // because with wrapping off a long line is wider than any window and is the text
    // edit's real content width, not an overflow to be clipped away.
    //
    // `auto_shrink` off so the area fills the panel, and the text edit is given the
    // panel's whole size as its minimum, so the empty space under (and beside) a short
    // document is part of the widget: a click anywhere in the page starts editing there.
    //
    // The height the area is about to fill is how far the eye moves on a Page key; read
    // it now, before the area claims the space, so paging knows how many rows a screenful
    // is here ([`page_navigation`]).
    let palette = shell.palette;
    let total_lines = shell.tabs[index].text().split('\n').count();
    let total_bytes = shell.tabs[index].text().len();
    let file_name = shell.tabs[index].workspace_name();

    // Code Editor mode header bar: clearly distinguishes Code Editor mode from Text Editor mode.
    let (banner_rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(banner_rect, 4.0, palette.gutter_bg);
    ui.painter().text(
        egui::pos2(banner_rect.left() + spacing::SM, banner_rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("</> CODE EDITOR  •  {file_name}"),
        FontId::monospace(11.5),
        palette.accent,
    );
    ui.painter().text(
        egui::pos2(banner_rect.right() - spacing::SM, banner_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{total_lines} lines  •  {total_bytes} bytes"),
        FontId::monospace(11.0),
        palette.text_dim,
    );
    ui.add_space(spacing::XS);

    let viewport = ui.available_height();
    egui::ScrollArea::both()
        .id_salt("editor_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let title = shell.tabs[index].title();
            let tab = &mut shell.tabs[index];
            let gutter_stride = (GUTTER_WIDTH + spacing::SM) as i8;
            let output = egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: gutter_stride,
                    right: 0,
                    top: 0,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    let mut buffer = DocBuffer(tab.document_mut());
                    let mut layouter =
                        |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                            layout_source_text(ui, buf.as_str(), &font, palette, wrap_width)
                        };
                    let edit = egui::TextEdit::multiline(&mut buffer)
                        .id(egui::Id::new(EDITOR_ID))
                        .font(font.clone())
                        .layouter(&mut layouter)
                        .frame(egui::Frame::NONE);
                    if wrap {
                        edit.desired_width(ui.available_width())
                            .min_size(egui::vec2(ui.available_width(), viewport))
                            .show(ui)
                    } else {
                        // Unwrapped, the editor has to be as wide as its widest line, and
                        // `egui` will not work that out for you: a `TextEdit` clamps its
                        // desired width to the space it is given before laying text out, so
                        // `desired_width(f32::INFINITY)` measures the document at the
                        // panel's width and wraps it anyway — and a scroll area does not
                        // rescue it either, because 0.36 tells the area's inner UI to prefer
                        // wrapping to a horizontal bar, on purpose. The measured width is
                        // therefore handed to the widget as `min_size`, the one input that
                        // outranks the clamp, and the area it sits in turns that width into
                        // a bar the user can drag.
                        edit.min_size(egui::vec2(natural.max(ui.available_width()), viewport))
                            .desired_width(natural)
                            .show(ui)
                    }
                })
                .inner;

            paint_line_numbers(ui, &output, &font, palette);

            // The document the editor is editing, so a screen reader says which
            // one it has arrived in rather than only that there is a text field
            // (`NC-P007`). The text itself `egui` reports as the field's value.
            a11y::label(&output.response, WidgetType::TextEdit, &title);

            // The caret a queued selection moves to, or the one the user left
            // the editor with.
            let caret = match pending {
                Some((primary, secondary)) => {
                    apply_selection(ui.ctx(), &output, primary, secondary);
                    set_document_selection(tab, primary, secondary);
                    scroll_caret_into_view(ui, &output, primary);
                    primary
                }
                None => match output.cursor_range {
                    Some(range) => {
                        let primary = usize::from(range.primary.index);
                        let secondary = usize::from(range.secondary.index);
                        set_document_selection(tab, primary, secondary);
                        primary
                    }
                    None => shell.caret,
                },
            };

            // Page Up / Page Down: the widget moves the caret on the arrow keys but
            // does nothing on the page keys, so the shell steps it a screenful here.
            let caret = page_navigation(ui, &output, tab, caret, viewport);

            // The other case that puts the caret outside the window without asking: the
            // document changed size underneath it. A new face or a new zoom reflows every
            // line, so the row the caret is on moves — and the caret itself did not change,
            // so neither this widget nor any command has reason to follow it. When the
            // metrics moved this frame, follow it anyway.
            //
            // The size is not enough to compare, and neither is the `FontId`: both ask for
            // `FontFamily::Monospace` whichever face is behind that name, so the thing that
            // moved is what the context resolves that family to, read back from the context
            // rather than from what the widget was handed.
            let metrics = (resolved_face(ui.ctx()), font.size);
            let key = egui::Id::new("editor-laid-out-in");
            let moved = ui
                .ctx()
                .data(|data| data.get_temp::<(String, f32)>(key))
                .is_none_or(|(face, size)| face != metrics.0 || size != metrics.1);
            if moved {
                scroll_caret_into_view(ui, &output, caret);
            }
            ui.ctx().data_mut(|data| data.insert_temp(key, metrics));

            // The squiggle under a likely misspelling (`NC-P004`). It is painted
            // from the word ranges the tab cached and the galley the text edit has
            // just laid out, so it follows wrapping and scrolling for free — the
            // same coordinate arithmetic the caret uses above.
            if spell_check {
                tab.refresh_spellings();
                paint_misspellings(ui, &output, tab);
            }
            caret
        })
        .inner
}

/// Pages the caret up or down a screenful, for the two keys `egui` leaves dead.
///
/// A `TextEdit` moves the caret on the arrows — and, in this version, on nothing
/// else: `Page Up` and `Page Down` reach it and are ignored, so in a document taller
/// than the window they did nothing at all. The shell answers them by stepping the
/// caret as many rows as fit in the window the document is laid out in, one row at a
/// time with the very primitive the arrows use ([`egui::Galley::cursor_down_one_row`]),
/// so the column the caret sat in is kept and the ends clamp the way an arrow clamps —
/// repeated pages stop at the top and bottom instead of overshooting. The caret is then
/// brought back on screen with the same request every other outside-the-widget move
/// makes. Holding Shift moves the caret and leaves the anchor where it was, extending the
/// selection exactly as Shift+arrow does; a Ctrl or Alt chord is left in the queue for
/// anything that wants it. Only the page movement is the shell's — word and line
/// navigation stay the widget's own (`NC-E004`).
fn page_navigation(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    tab: &mut Tab,
    caret: usize,
    viewport: f32,
) -> usize {
    let Some((primary, secondary)) = step_page_cursor(ui, output, caret, viewport) else {
        return caret;
    };
    apply_selection(ui.ctx(), output, primary, secondary);
    set_document_selection(tab, primary, secondary);
    scroll_caret_into_view(ui, output, primary);
    primary
}

/// Computes the new `(primary, secondary)` galley cursor indices when `Page Up` or
/// `Page Down` is pressed while the editor has focus.
fn step_page_cursor(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    caret: usize,
    viewport: f32,
) -> Option<(usize, usize)> {
    // The keys belong to the editor and to nothing else: a focused dialog keeps its
    // own Page keys.
    if !output.response.has_focus() {
        return None;
    }

    // Which page key the frame carries, as `(down, extend)` — at most one, and only a
    // bare or Shift one. Consumed here so it is not seen again further down the frame.
    let mut page: Option<(bool, bool)> = None;
    ui.ctx().input_mut(|input| {
        input.events.retain(|event| {
            let Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            if !matches!(key, Key::PageDown | Key::PageUp) {
                return true;
            }
            if modifiers.ctrl || modifiers.command || modifiers.alt || page.is_some() {
                return true;
            }
            page = Some((matches!(key, Key::PageDown), modifiers.shift));
            false
        });
    });
    let (down, extend) = page?;

    // A page is the window's height in rows. The rows are uniform, so the first one's
    // height stands for all; without a laid-out document (heights not yet known, or a
    // single row that cannot be paged through) there is nowhere to go.
    let galley = &output.galley;
    let row_height = galley.rows.first().map(|row| row.height())?;
    if row_height <= 0.0 || galley.rows.len() < 2 {
        return None;
    }
    let page_rows = ((viewport / row_height).floor() as usize).max(1);

    // Walk the page one row at a time, handing the horizontal position from each step to
    // the next so the caret keeps its column across the whole jump.
    let mut cursor = CCursor::new(caret);
    let mut column: Option<f32> = None;
    for _ in 0..page_rows {
        let (next, kept) = if down {
            galley.cursor_down_one_row(&cursor, column)
        } else {
            galley.cursor_up_one_row(&cursor, column)
        };
        cursor = next;
        column = kept;
    }
    let primary = usize::from(cursor.index);
    if primary == caret {
        return None;
    }

    // Shift keeps the far end of the selection as the anchor; otherwise the caret
    // collapses onto where it landed. Then the frame's own bring-it-into-view request.
    let secondary = if extend {
        output
            .cursor_range
            .map(|range| usize::from(range.secondary.index))
            .unwrap_or(caret)
    } else {
        primary
    };
    Some((primary, secondary))
}

/// The face the context resolves the document's family slot to right now, which is the
/// face the text edit has just laid the document out in.
fn resolved_face(ctx: &egui::Context) -> String {
    ctx.fonts(|fonts| {
        fonts
            .definitions()
            .families
            .get(&egui::FontFamily::Monospace)
            .and_then(|names| names.first().cloned())
            .unwrap_or_default()
    })
}

/// Paints the line-numbers gutter to the left of the source editor (`palette.gutter_bg`,
/// `palette.line`, and right-aligned 1-based line numbers in `palette.gutter_text`).
fn paint_line_numbers(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    font: &FontId,
    palette: crate::ui::theme::Palette,
) {
    let right = output.response.rect.left() - spacing::SM;
    let left = right - GUTTER_WIDTH;
    let gutter_rect = egui::Rect::from_min_max(
        egui::pos2(left, output.response.rect.top()),
        egui::pos2(right, output.response.rect.bottom()),
    );
    let painter = ui.painter();
    painter.rect_filled(gutter_rect, 0.0, palette.gutter_bg);
    painter.vline(
        right,
        gutter_rect.y_range(),
        egui::Stroke::new(1.0, palette.line),
    );

    let mut line_num = 1usize;
    for (idx, placed) in output.galley.rows.iter().enumerate() {
        let is_line_start = idx == 0 || output.galley.rows[idx - 1].ends_with_newline;
        if is_line_start {
            let y = output.galley_pos.y + placed.min_y();
            painter.text(
                egui::pos2(right - spacing::SM, y),
                egui::Align2::RIGHT_TOP,
                line_num.to_string(),
                font.clone(),
                palette.gutter_text,
            );
            line_num += 1;
        }
    }
}

/// Paints a red squiggle under every misspelled word the tab cached
/// (`NC-P004`). Each byte range is turned into the character indices the galley
/// speaks and mapped to screen with the same translation the caret uses, so the
/// underline sits where the word does whether or not it wrapped to a second row.
fn paint_misspellings(ui: &egui::Ui, output: &egui::text_edit::TextEditOutput, tab: &Tab) {
    if tab.spellings.is_empty() {
        return;
    }
    let text = tab.text();
    let shift = output.galley_pos.to_vec2() - egui::vec2(output.galley.rect.left(), 0.0);
    let painter = ui.painter();
    for range in &tab.spellings {
        let from = char_index(text, range.start);
        let to = char_index(text, range.end);
        for band in word_bands(&output.galley, from, to) {
            draw_squiggle(painter, band.translate(shift));
        }
    }
}

/// Paints a red squiggle under misspelled visible words in [`EditorMode::Text`],
/// ignoring any tag words hidden inside `<span style="color: ...">` or link URLs.
fn paint_misspellings_rich(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    tab: &Tab,
    projection: &RichProjection,
) {
    if tab.spellings.is_empty() {
        return;
    }
    let shift = output.galley_pos.to_vec2() - egui::vec2(output.galley.rect.left(), 0.0);
    let painter = ui.painter();
    for range in &tab.spellings {
        let from = projection.src_byte_to_vis_char(range.start);
        let to = projection.src_byte_to_vis_char(range.end);
        if from >= to {
            continue;
        }
        for band in word_bands(&output.galley, from, to) {
            draw_squiggle(painter, band.translate(shift));
        }
    }
}

/// The rectangle to underline for the characters `from..to` of the galley, split
/// per row so a word that wrapped is underlined on each row it occupies. The
/// rectangles are in the galley's own coordinates.
fn word_bands(galley: &Galley, from: usize, to: usize) -> Vec<egui::Rect> {
    let start = galley.layout_from_cursor(CCursor::new(from));
    let end = galley.layout_from_cursor(CCursor::new(to));
    let mut bands = Vec::new();
    for row in start.row..=end.row {
        let Some(placed) = galley.rows.get(row) else {
            break;
        };
        let full = placed.char_count_excluding_newline();
        let first = if row == start.row {
            start.column
        } else {
            CharIndex::ZERO
        };
        let last = if row == end.row { end.column } else { full };
        if !(first < last) {
            continue;
        }
        let x0 = placed.pos.x + placed.x_offset(first);
        let x1 = placed.pos.x + placed.x_offset(last);
        if x1 <= x0 {
            continue;
        }
        // One point above the row's bottom, so the wave sits under the glyphs.
        let y = placed.max_y() - 1.0;
        bands.push(egui::Rect::from_min_max(
            egui::pos2(x0, y),
            egui::pos2(x1, y),
        ));
    }
    bands
}

/// The little wave drawn under one row's span of a misspelled word.
fn draw_squiggle(painter: &egui::Painter, band: egui::Rect) {
    const AMPLITUDE: f32 = 1.2;
    const WAVELENGTH: f32 = 5.0;
    let step = WAVELENGTH / 4.0;
    let x0 = band.min.x;
    let y0 = band.min.y;
    let mut points = vec![egui::pos2(x0, y0)];
    let mut x = x0;
    while x < band.max.x {
        x = (x + step).min(band.max.x);
        let t = (x - x0) / WAVELENGTH * std::f32::consts::TAU;
        points.push(egui::pos2(x, y0 + AMPLITUDE * t.sin()));
    }
    painter.add(Shape::line(points, egui::Stroke::new(1.0, SPELL_INK)));
}

/// Scrolls enough of the frame to bring a caret the shell just moved to back on
/// screen.
///
/// The rectangle is the one the text edit would have used for itself: `egui` positions
/// a caret with [`egui::Galley::pos_from_cursor`], in the galley's own coordinates, and
/// paints that galley shifted left by the galley's left edge — so the translation here
/// matches its `cursor_rect` arithmetic rather than approximating it.
fn scroll_caret_into_view(ui: &egui::Ui, output: &egui::text_edit::TextEditOutput, primary: usize) {
    let rect = output
        .galley
        .pos_from_cursor(CCursor::new(primary))
        .translate(output.galley_pos.to_vec2() - egui::vec2(output.galley.rect.left(), 0.0));
    ui.scroll_to_rect(rect, None);
}

/// Moves the editor's caret and selection to the given character indices.
///
/// `egui` keeps the selection in per-widget state, so this stores the new
/// cursor back under the widget's own id and focuses the editor.
fn apply_selection(
    ctx: &egui::Context,
    output: &egui::text_edit::TextEditOutput,
    primary: usize,
    secondary: usize,
) {
    let mut state = output.state.clone();
    state.cursor.set_char_range(Some(CCursorRange {
        primary: CCursor::new(primary),
        secondary: CCursor::new(secondary),
        h_pos: None,
    }));
    state.store(ctx, output.response.id);
    output.response.request_focus();
}

/// Points the document's own selection at the range the editor widget holds.
///
/// `egui` keeps the caret and selection in per-widget state; commands routed
/// through the shell (Delete, Select all, Cut) act on the document's selection,
/// so the two are kept in step every frame — the document is the single place
/// those commands read.
fn set_document_selection(tab: &mut Tab, primary: usize, secondary: usize) {
    let (anchor, head) = {
        let text = tab.text();
        (byte_index(text, secondary), byte_index(text, primary))
    };
    tab.document_mut().select(anchor, head);
}

/// An [`egui::TextBuffer`] over an `nc-core` document for [`EditorMode::Text`],
/// presenting the tag-free `projection.visible` string to the widget while applying
/// insertions and deletions directly to the underlying [`Document`] in a single
/// undoable step.
struct RichDocBuffer<'a> {
    document: &'a mut Document,
    projection: RichProjection,
    shared: Rc<RefCell<RichProjection>>,
}

impl egui::TextBuffer for RichDocBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.projection.visible
    }

    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        if text.is_empty() {
            return 0;
        }
        let vis_idx = usize::from(char_index);
        let cur_caret = self.document.caret();
        let insert_byte = self
            .projection
            .vis_char_to_src_insert_byte(vis_idx, cur_caret);
        self.document.set_caret(insert_byte);
        self.document.insert_str(text);
        self.projection = RichProjection::build(self.document.text());
        *self.shared.borrow_mut() = self.projection.clone();
        text.chars().count()
    }

    fn delete_char_range(&mut self, range: Range<CharIndex>) {
        let start = usize::from(range.start);
        let end = usize::from(range.end);
        if start >= end || self.projection.vis_to_src_start.is_empty() {
            return;
        }
        let clamped_start = start.min(self.projection.vis_to_src_start.len());
        let clamped_end = end.min(self.projection.vis_to_src_start.len());
        if clamped_start >= clamped_end {
            return;
        }

        // Remove each visible character's byte slice from a working copy of `source`
        // (preserving any non-empty formatting tags around remaining characters),
        // then strip any wrapper pair (`****`, `<span style="color: ..."></span>`, etc.)
        // that became completely empty, and commit as a single `replace_range` edit.
        let old_src = self.document.text();
        let mut next_src = String::with_capacity(old_src.len());
        let mut cursor = 0usize;
        for vis_i in clamped_start..clamped_end {
            let b0 = self.projection.vis_to_src_start[vis_i];
            let b1 = self.projection.vis_to_src_end[vis_i];
            if b0 > cursor {
                next_src.push_str(&old_src[cursor..b0]);
            }
            cursor = b1.max(cursor);
        }
        if cursor < old_src.len() {
            next_src.push_str(&old_src[cursor..]);
        }
        cleanup_empty_wrappers(&mut next_src);

        if next_src != old_src {
            let prefix = old_src
                .bytes()
                .zip(next_src.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            // Align `prefix` to a UTF-8 char boundary.
            let mut diff_start = prefix.min(old_src.len()).min(next_src.len());
            while diff_start > 0
                && (!old_src.is_char_boundary(diff_start) || !next_src.is_char_boundary(diff_start))
            {
                diff_start -= 1;
            }
            let suffix = old_src[diff_start..]
                .bytes()
                .rev()
                .zip(next_src[diff_start..].bytes().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let mut old_end = old_src.len() - suffix;
            let mut new_end = next_src.len() - suffix;
            while old_end < old_src.len()
                && (!old_src.is_char_boundary(old_end) || !next_src.is_char_boundary(new_end))
            {
                old_end += 1;
                new_end += 1;
            }
            self.document
                .replace_range(diff_start..old_end, &next_src[diff_start..new_end]);
        }

        self.projection = RichProjection::build(self.document.text());
        *self.shared.borrow_mut() = self.projection.clone();
    }

    fn type_id(&self) -> core::any::TypeId {
        core::any::TypeId::of::<RichDocBuffer<'_>>()
    }
}

/// An [`egui::TextBuffer`] over an `nc-core` document.
///
/// `egui` addresses text by character index; the document by byte offset, so
/// every call converts at the boundary — the rule `AGENT.md` §4 sets for the
/// editor.
struct DocBuffer<'a>(&'a mut Document);

impl egui::TextBuffer for DocBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.0.text()
    }

    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        if text.is_empty() {
            return 0;
        }
        let byte = byte_index(self.0.text(), usize::from(char_index));
        self.0.set_caret(byte);
        self.0.insert_str(text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, range: Range<CharIndex>) {
        let start = usize::from(range.start);
        let end = usize::from(range.end);
        if start >= end {
            return;
        }
        let text = self.0.text();
        let start_byte = byte_index(text, start);
        let end_byte = byte_index(text, end);
        self.0.replace_range(start_byte..end_byte, "");
    }

    fn type_id(&self) -> core::any::TypeId {
        core::any::TypeId::of::<DocBuffer<'_>>()
    }
}

/// The character index of a byte offset, clamped to the text.
///
/// `egui` addresses text by character index and the document by byte offset, so a
/// selection has to travel back across the same boundary it came from — the rule
/// `AGENT.md` §4 sets for the editor.
pub(crate) fn char_index(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    text[..byte].chars().count()
}

/// The byte offset of a character index, clamped to the text.
fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

/// Converts a character index into a 1-based (line, column).
fn line_column(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in text.chars().enumerate() {
        if index >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::TextBuffer;

    #[test]
    fn caret_position_is_one_based() {
        assert_eq!(line_column("", 0), (1, 1));
        assert_eq!(line_column("abc", 0), (1, 1));
        assert_eq!(line_column("abc", 2), (1, 3));
        assert_eq!(line_column("ab\ncd", 3), (2, 1));
        assert_eq!(line_column("ab\ncd", 5), (2, 3));
    }

    #[test]
    fn the_buffer_edits_the_document_at_character_indices() {
        let mut document = Document::from_text("héllo wörld");
        {
            let mut buffer = DocBuffer(&mut document);
            // Insert after the first 'é' (character index 2).
            buffer.insert_text("!", CharIndex(2));
            assert_eq!(buffer.as_str(), "hé!llo wörld");
            // Delete "!" (character range 2..3).
            buffer.delete_char_range(CharIndex(2)..CharIndex(3));
            assert_eq!(buffer.as_str(), "héllo wörld");
        }
        assert_eq!(document.text(), "héllo wörld");
    }

    #[test]
    fn rich_buffer_hides_color_and_style_tags_while_editing_source() {
        let src = "<span style=\"color: #1971C2\">sdfadfasdfasdfasdfasdfasdf</span>";
        let mut document = Document::from_text(src);
        let proj = RichProjection::build(document.text());
        assert_eq!(proj.visible, "sdfadfasdfasdfasdfasdfasdf");
        assert_eq!(proj.runs.len(), 1);
        assert_eq!(proj.runs[0].style.color, Some([0x19, 0x71, 0xC2]));

        let shared = Rc::new(RefCell::new(proj.clone()));
        let mut buffer = RichDocBuffer {
            document: &mut document,
            projection: proj,
            shared,
        };
        // Typing "!" at the end of the visible text inserts inside the color span.
        let end_vis = buffer.as_str().chars().count();
        buffer.insert_text("!", CharIndex(end_vis));
        assert_eq!(buffer.as_str(), "sdfadfasdfasdfasdfasdfasdf!");
        assert_eq!(
            buffer.document.text(),
            "<span style=\"color: #1971C2\">sdfadfasdfasdfasdfasdfasdf!</span>"
        );

        // Deleting all visible characters also cleans up the empty `<span...>` wrapper.
        let total = buffer.as_str().chars().count();
        buffer.delete_char_range(CharIndex(0)..CharIndex(total));
        assert_eq!(buffer.as_str(), "");
        assert_eq!(document.text(), "");
    }

    #[test]
    fn multiline_color_spans_and_nested_italic_underline_hide_all_tags() {
        let src = "<span style=\"color: #E8590C\">Título do Bug: Vazamento de Estado Global nos Diários/Observações Individuais (Registros compartilhados entre todos os alunos)\n</span>\n\n\n<span style=\"color: #C92A2A\">  Comportamento Atual (Incorreto):\n  Os registros do diário individual estão sendo salvos e lidos a partir de um estado ou array global sem vínculo de chave estrangeira (studentId), ou o componente de listagem não está filtrando os dados pelo aluno atualmente selecionado.</span>\n\n*<u>Comportamento Esperado (Correto):\nCada registro de diário/observação individual deve pertencer única e exclusivamente ao aluno em cuja ficha a anotação foi criada. Ao alternar entre a ficha do \"Aluno X\" e a do \"Aluno Y\", a interface deve renderizar apenas o histórico correspondente ao id daquele estudante específico.</u>*";
        let proj = RichProjection::build(src);
        assert!(
            !proj.visible.contains("<span")
                && !proj.visible.contains("</span>")
                && !proj.visible.contains("<u>")
                && !proj.visible.contains("</u>")
                && !proj.visible.contains("*"),
            "all multi-line and nested tags must be hidden in Text Editor mode, got: {:?}",
            proj.visible
        );
        assert!(proj.visible.contains("Título do Bug:"));
        assert!(proj.visible.contains("Comportamento Atual (Incorreto):"));
        assert!(proj.visible.contains("Comportamento Esperado (Correto):"));
        assert!(
            proj.runs
                .iter()
                .any(|r| r.style.color == Some([0xE8, 0x59, 0x0C])),
            "first paragraph should carry #E8590C color"
        );
        assert!(
            proj.runs
                .iter()
                .any(|r| r.style.color == Some([0xC9, 0x2A, 0x2A])),
            "second paragraph should carry #C92A2A color"
        );
        assert!(
            proj.runs
                .iter()
                .any(|r| r.style.italic && r.style.underline),
            "third paragraph should carry both italic and underline across both lines"
        );
    }

    #[test]
    fn byte_and_char_indices_agree_across_multibyte_text() {
        let text = "aé€b";
        assert_eq!(byte_index(text, 0), 0);
        assert_eq!(byte_index(text, 1), 1);
        assert_eq!(byte_index(text, 2), 3);
        assert_eq!(byte_index(text, 3), 6);
        assert_eq!(byte_index(text, 4), 7);
        assert_eq!(byte_index(text, 99), 7);
    }
}
