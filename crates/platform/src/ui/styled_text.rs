//! Markdown "styled while you type": the source view of a Markdown document, with the
//! headings larger and the words of bold, italic, code and links drawn in their own look.
//!
//! Adapted from AlienTechSoftware/notecraft `crates/ui-egui/src/pagetext.rs`
//! (MIT OR Apache-2.0, Copyright (c) 2026 AlienTech Software and the NoteCraft
//! contributors). Changes made for TextCraft: the page-format and tag handling is gone,
//! the colours come from [`Palette`], the user's monospace face is kept for every style
//! (size, colour, italics, underline and background carry the look instead), strikethrough
//! is recognised, and the widest-line measurement for the unwrapped editor uses the same
//! styling. See `THIRD_PARTY_NOTICES.md` at the repository root.
//!
//! The text itself is never changed. The marks (`**`, `#`, `>`, bullets) stay in the
//! buffer, because they are what the file holds, but they are drawn dimmed so the eye
//! follows the words. The result is handed to `egui::TextEdit` as its layouter, so the
//! caret, the selection and undo behave exactly as they do in a plain text box.
//!
//! [`pieces`] is pure and tested without a window; [`layout_job`] only turns its answer
//! into an `egui` layout.

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, FontId, Stroke};

use crate::ui::theme::Palette;

/// One piece of the text and how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Ordinary words.
    Body,
    /// A heading, 1 to 3 (deeper headings use 3).
    Heading(u8),
    Bold,
    Italic,
    Strike,
    /// Inline code, or a line of a fenced block.
    Code,
    /// A list bullet, a heading's hashes, a quote mark, an emphasis mark: part of the syntax.
    Marker,
    Quote,
    Link,
    Math,
}

impl Style {
    /// How much larger than the base size this style is drawn.
    fn scale(self) -> f32 {
        match self {
            Style::Heading(1) => 1.6,
            Style::Heading(2) => 1.35,
            Style::Heading(_) => 1.15,
            _ => 1.0,
        }
    }

    /// The look of this style. The family is the base font's, so the monospace face the
    /// user picked in Settings is the one every style is drawn in.
    fn format(self, base: &FontId, p: &Palette) -> TextFormat {
        let font_id = FontId::new(base.size * self.scale(), base.family.clone());
        let plain = TextFormat {
            font_id,
            color: p.text,
            ..Default::default()
        };
        match self {
            Style::Body => plain,
            // Size alone marks a heading; the colour stays the page's so a link in the
            // same line is still the only thing in the accent.
            Style::Heading(_) => plain,
            // A monospace face has no bold, so strong text gets a faint wash instead.
            Style::Bold => TextFormat {
                background: wash(p.accent_soft, 0.45),
                ..plain
            },
            Style::Italic => TextFormat {
                italics: true,
                ..plain
            },
            Style::Strike => TextFormat {
                color: p.text_dim,
                strikethrough: Stroke::new(1.0, p.text_dim),
                ..plain
            },
            Style::Code => TextFormat {
                background: code_background(p),
                ..plain
            },
            Style::Marker => TextFormat {
                color: p.text_dim,
                ..plain
            },
            Style::Quote => TextFormat {
                color: p.text_dim,
                italics: true,
                ..plain
            },
            Style::Link => TextFormat {
                color: p.accent,
                underline: Stroke::new(1.0, p.accent),
                ..plain
            },
            Style::Math => TextFormat {
                color: p.accent,
                italics: true,
                ..plain
            },
        }
    }
}

/// A subtle code background: the popup surface pushed a step towards the hover fill, so it
/// reads on both the dark and the light page without a hard-coded colour.
fn code_background(p: &Palette) -> Color32 {
    p.popup.blend(wash(p.row_hover, 0.6))
}

/// `colour` made translucent, for a wash that lets the page show through.
fn wash(colour: Color32, strength: f32) -> Color32 {
    colour.gamma_multiply(strength)
}

/// Splits `text` into styled pieces that cover it exactly once, in order. Pure.
pub fn pieces(text: &str) -> Vec<(Range<usize>, Style)> {
    let mut out = Vec::new();
    block_pieces(text, &mut out);
    merge(out, text.len())
}

/// Fills the gaps with body text and drops empty pieces, so the result covers the text once.
fn merge(mut pieces: Vec<(Range<usize>, Style)>, len: usize) -> Vec<(Range<usize>, Style)> {
    pieces.retain(|(r, _)| r.start < r.end && r.end <= len);
    pieces.sort_by_key(|(r, _)| r.start);
    let mut out = Vec::with_capacity(pieces.len() * 2 + 1);
    let mut at = 0usize;
    for (range, style) in pieces {
        if range.start < at {
            continue; // overlapping: the first one wins
        }
        if range.start > at {
            out.push((at..range.start, Style::Body));
        }
        at = range.end;
        out.push((range, style));
    }
    if at < len {
        out.push((at..len, Style::Body));
    }
    out
}

fn block_pieces(text: &str, out: &mut Vec<(Range<usize>, Style)>) {
    let mut fenced = false;
    for (start, line) in line_spans(text) {
        let end = start + line.len();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            out.push((start..end, Style::Code));
            continue;
        }
        if fenced {
            out.push((start..end, Style::Code));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            out.push((start + indent..start + indent + 1, Style::Marker));
            out.push((
                start + indent + 1..start + indent + 1 + rest.len(),
                Style::Quote,
            ));
            continue;
        }
        let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
            out.push((start + indent..start + indent + hashes + 1, Style::Marker));
            let level = hashes.min(3) as u8;
            out.push((start + indent + hashes + 1..end, Style::Heading(level)));
            continue;
        }
        if let Some(marker) = list_marker(trimmed) {
            out.push((start + indent..start + indent + marker, Style::Marker));
            inline(text, start + indent + marker, end, out);
            continue;
        }
        inline(text, start, end, out);
    }
}

/// The length of a list marker at the start of `s`, including the space and any task box.
fn list_marker(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut n = match bytes.first() {
        Some(b'-' | b'*' | b'+') if bytes.get(1) == Some(&b' ') => 2,
        _ => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits > 0
                && matches!(bytes.get(digits), Some(b'.' | b')'))
                && bytes.get(digits + 1) == Some(&b' ')
            {
                digits + 2
            } else {
                0
            }
        }
    };
    if n == 0 {
        return None;
    }
    let rest = s.get(n..).unwrap_or("");
    for task in ["[ ] ", "[x] ", "[X] "] {
        if rest.starts_with(task) {
            n += task.len();
            break;
        }
    }
    Some(n)
}

/// Inline styles inside one line: code, maths, bold, strikethrough, italic and links.
fn inline(text: &str, from: usize, to: usize, out: &mut Vec<(Range<usize>, Style)>) {
    let line = text.get(from..to).unwrap_or("");
    let mut i = 0usize;
    while i < line.len() {
        let rest = line.get(i..).unwrap_or("");
        // The marks are dimmed and the words inside them carry the style, so the text
        // reads as formatted text with the marks out of the way.
        let mut emphasis = |mark: &str, style: Style| -> Option<usize> {
            let len = wrapped(rest, mark)?;
            let m = mark.len();
            out.push((from + i..from + i + m, Style::Marker));
            out.push((from + i + m..from + i + len - m, style));
            out.push((from + i + len - m..from + i + len, Style::Marker));
            Some(len)
        };
        let consumed = emphasis("`", Style::Code)
            .or_else(|| emphasis("$", Style::Math))
            .or_else(|| emphasis("**", Style::Bold))
            .or_else(|| emphasis("~~", Style::Strike))
            .or_else(|| emphasis("*", Style::Italic));
        if let Some(len) = consumed {
            i += len;
        } else if rest.starts_with('[')
            && let Some(len) = link_len(rest)
        {
            out.push((from + i..from + i + len, Style::Link));
            i += len;
        } else {
            i += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
}

/// The length of `mark ... mark` at the start of `s`, if it closes on the same line and has
/// words directly inside it (`2 * 3 * 4` is arithmetic, not emphasis).
fn wrapped(s: &str, mark: &str) -> Option<usize> {
    let rest = s.strip_prefix(mark)?;
    if rest.starts_with(' ') || rest.is_empty() {
        return None;
    }
    let end = rest.find(mark)?;
    if end == 0 {
        return None;
    }
    Some(mark.len() + end + mark.len())
}

/// The length of a `[text](target)` link at the start of `s`.
fn link_len(s: &str) -> Option<usize> {
    let close = s.find("](")?;
    let end = s.get(close..)?.find(')')? + close + 1;
    Some(end)
}

/// The lines of `text` as `(start byte, line)`, without their line endings.
fn line_spans(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for line in text.split_inclusive('\n') {
        let without = line.strip_suffix('\n').unwrap_or(line);
        let without = without.strip_suffix('\r').unwrap_or(without);
        out.push((at, without));
        at += line.len();
    }
    out
}

/// The layout `egui::TextEdit` draws: the pieces above, turned into one job.
///
/// `base` supplies the size and family (the user's monospace face); everything else comes
/// from `palette`.
pub fn layout_job(text: &str, base: &FontId, wrap_width: f32, palette: &Palette) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: TextWrapping {
            max_width: wrap_width,
            ..Default::default()
        },
        ..Default::default()
    };
    for (range, style) in pieces(text) {
        if let Some(part) = text.get(range) {
            job.append(part, 0.0, style.format(base, palette));
        }
    }
    if job.sections.is_empty() {
        // An empty document still needs one section, or the caret has no height.
        job.append("", 0.0, Style::Body.format(base, palette));
    }
    job
}

/// The line that will lay out widest, for sizing the editor when word wrap is off.
///
/// Headings are drawn larger, so the line with the most characters is not necessarily the
/// widest: each character counts for its piece's scale. Allocation free past the pieces.
pub fn widest_line(text: &str) -> &str {
    let mut best = (0.0f32, 0..0);
    let mut line_start = 0usize;
    let mut weight = 0.0f32;
    let mut check = |weight: f32, range: Range<usize>| {
        if weight > best.0 {
            best = (weight, range);
        }
    };
    for (range, style) in pieces(text) {
        let scale = style.scale();
        let mut at = range.start;
        for ch in text[range.clone()].chars() {
            if ch == '\n' {
                check(weight, line_start..at);
                weight = 0.0;
                at += 1;
                line_start = at;
            } else {
                weight += scale;
                at += ch.len_utf8();
            }
        }
    }
    check(weight, line_start..text.len());
    text.get(best.1).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str) -> Vec<(String, Style)> {
        pieces(text)
            .into_iter()
            .map(|(r, s)| (text[r].to_string(), s))
            .collect()
    }

    fn style_of(text: &str, needle: &str) -> Option<Style> {
        styled(text)
            .into_iter()
            .find(|(t, _)| t == needle)
            .map(|(_, s)| s)
    }

    fn covers(text: &str) {
        let got = pieces(text);
        let mut at = 0;
        for (range, _) in &got {
            assert_eq!(range.start, at, "gap or overlap in {text:?}");
            assert!(
                text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
                "range off a char boundary in {text:?}"
            );
            at = range.end;
        }
        assert_eq!(at, text.len(), "the pieces must cover {text:?}");
    }

    #[test]
    fn headings_one_to_three_and_deeper() {
        let got = styled("# Title\n\nSome words\n");
        assert_eq!(got[0], ("# ".to_string(), Style::Marker));
        assert_eq!(got[1], ("Title".to_string(), Style::Heading(1)));
        assert_eq!(style_of("## Two", "Two"), Some(Style::Heading(2)));
        assert_eq!(style_of("### Three", "Three"), Some(Style::Heading(3)));
        assert_eq!(style_of("###### Six", "Six"), Some(Style::Heading(3)));
        // A hash without a space, or seven of them, is not a heading.
        assert_eq!(styled("#tag"), vec![("#tag".to_string(), Style::Body)]);
        assert!(
            styled("####### seven")
                .iter()
                .all(|(_, s)| *s == Style::Body)
        );
    }

    #[test]
    fn bold_italic_strike_code_link_and_math() {
        let text =
            "a **bold** and *slanted* and ~~gone~~ and `code` and [link](x.md) and $x^2$ end";
        assert_eq!(style_of(text, "bold"), Some(Style::Bold));
        assert_eq!(style_of(text, "slanted"), Some(Style::Italic));
        assert_eq!(style_of(text, "gone"), Some(Style::Strike));
        assert_eq!(style_of(text, "code"), Some(Style::Code));
        assert_eq!(style_of(text, "[link](x.md)"), Some(Style::Link));
        assert_eq!(style_of(text, "x^2"), Some(Style::Math));
        let marks = |m: &str| {
            styled(text)
                .iter()
                .filter(|(t, s)| t == m && *s == Style::Marker)
                .count()
        };
        assert_eq!(marks("**"), 2, "both bold marks are dimmed");
        assert_eq!(marks("~~"), 2);
        assert_eq!(marks("*"), 2);
        assert_eq!(marks("`"), 2);
        covers(text);
    }

    #[test]
    fn lists_quotes_and_fences() {
        let list = styled("- one\n1. two\n- [ ] task\n");
        assert_eq!(list[0], ("- ".to_string(), Style::Marker));
        assert!(list.contains(&("1. ".to_string(), Style::Marker)));
        assert!(list.contains(&("- [ ] ".to_string(), Style::Marker)));
        let quote = styled("> quoted\n");
        assert_eq!(quote[0], (">".to_string(), Style::Marker));
        assert_eq!(quote[1], (" quoted".to_string(), Style::Quote));
        let fenced = "```rust\nlet x = **1**;\n```\nafter\n";
        let fence = styled(fenced);
        assert_eq!(
            fence.iter().filter(|(_, s)| *s == Style::Code).count(),
            3,
            "the fences and the line inside"
        );
        assert!(
            !fence.iter().any(|(_, s)| *s == Style::Bold),
            "no emphasis inside a fence"
        );
        assert!(fence.contains(&("\nafter\n".to_string(), Style::Body)));
        // An unterminated fence runs to the end.
        assert_eq!(style_of("```\ncode", "code"), Some(Style::Code));
        covers(fenced);
    }

    #[test]
    fn a_bullet_is_not_italic_and_arithmetic_is_not_emphasis() {
        assert_eq!(styled("* item")[0], ("* ".to_string(), Style::Marker));
        assert!(styled("2 * 3 * 4").iter().all(|(_, s)| *s == Style::Body));
        assert!(styled("****").iter().all(|(_, s)| *s != Style::Bold));
    }

    #[test]
    fn empty_and_unterminated_text() {
        assert!(pieces("").is_empty());
        for text in [
            "\n\n\n",
            "**unclosed",
            "*",
            "`",
            "$",
            "~~",
            "[half](",
            "[half]",
            "- ",
            "#",
            "# ",
            "######## too deep",
            ">",
            "```",
            "a\r\nb\r\n",
            "# head\r\n**b**\r\n",
        ] {
            covers(text);
        }
        assert_eq!(
            styled("**unclosed"),
            vec![("**unclosed".to_string(), Style::Body)]
        );
        covers(&"word ".repeat(5000));
    }

    #[test]
    fn multibyte_text_stays_on_char_boundaries() {
        for text in [
            "\u{1F600} emoji **bold**",
            "mixed \u{4e2d}\u{6587} *text*",
            "# \u{0645}\u{0631}\u{062d}\u{0628}\u{0627}",
            "[\u{0647}\u{0646}\u{0627}](../\u{0647}\u{0646}\u{0627}/) `\u{00e9}`",
            "> \u{00fc}ber ~~\u{00df}~~ - \u{00e4}",
        ] {
            covers(text);
        }
        assert_eq!(
            style_of("x **\u{00e9}\u{00e8}** y", "\u{00e9}\u{00e8}"),
            Some(Style::Bold)
        );
    }

    #[test]
    fn the_styles_really_differ_in_both_palettes() {
        let base = FontId::monospace(15.0);
        for palette in [Palette::DARK, Palette::LIGHT] {
            let body = Style::Body.format(&base, &palette);
            for style in [
                Style::Heading(1),
                Style::Bold,
                Style::Italic,
                Style::Strike,
                Style::Code,
                Style::Marker,
                Style::Quote,
                Style::Link,
                Style::Math,
            ] {
                let f = style.format(&base, &palette);
                assert!(
                    f.color != body.color
                        || f.font_id != body.font_id
                        || f.italics != body.italics
                        || f.background != body.background
                        || f.underline != body.underline
                        || f.strikethrough != body.strikethrough,
                    "{style:?} looks like body text"
                );
                assert_eq!(f.font_id.family, base.family, "the face is the user's");
            }
            assert!(
                Style::Heading(1).format(&base, &palette).font_id.size
                    > Style::Heading(2).format(&base, &palette).font_id.size
            );
            assert!(Style::Heading(3).format(&base, &palette).font_id.size > base.size);
        }
    }

    #[test]
    fn the_job_holds_every_character() {
        let text = "# Title\n\n- one **two**\n";
        let job = layout_job(text, &FontId::monospace(15.0), 400.0, &Palette::DARK);
        assert_eq!(job.text, text);
        assert!(job.sections.len() > 3);
        let empty = layout_job("", &FontId::monospace(15.0), 400.0, &Palette::DARK);
        assert_eq!(empty.sections.len(), 1, "an empty text still has a section");
    }

    #[test]
    fn the_widest_line_counts_heading_size() {
        // The heading has fewer characters but draws wider.
        let text = "plain line\n# Head1234\nx";
        assert_eq!(widest_line(text), "# Head1234");
        assert_eq!(widest_line("short\nlonger line\n"), "longer line");
        assert_eq!(widest_line(""), "");
        assert_eq!(widest_line("one"), "one");
        assert_eq!(widest_line("a\r\nbb\r\n"), "bb\r");
        // Multibyte lines are sliced on boundaries.
        assert_eq!(
            widest_line("\u{00e9}\n\u{00e9}\u{00e9}\u{00e9}"),
            "\u{00e9}\u{00e9}\u{00e9}"
        );
    }
}
