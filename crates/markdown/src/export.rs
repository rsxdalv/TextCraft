//! HTML export for Markdown documents.

/// Exports `markdown` as a self-contained HTML5 document with embedded styling.
pub fn export_html(title: &str, markdown: &str) -> String {
    let mut body = String::with_capacity(markdown.len() * 2);
    let mut in_code_block = false;
    let mut in_list = false;
    let mut list_is_ordered = false;

    let lines: Vec<&str> = markdown.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Fenced code blocks
        if trimmed.starts_with("```") {
            if in_code_block {
                body.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                if in_list {
                    body.push_str(if list_is_ordered {
                        "</ol>\n"
                    } else {
                        "</ul>\n"
                    });
                    in_list = false;
                }
                let code_block_lang = trimmed.trim_start_matches('`').trim();
                body.push_str("<pre><code");
                if !code_block_lang.is_empty() {
                    body.push_str(" class=\"language-");
                    body.push_str(&escape_html(code_block_lang));
                    body.push('\"');
                }
                body.push('>');
                in_code_block = true;
            }
            i += 1;
            continue;
        }

        if in_code_block {
            body.push_str(&escape_html(line));
            body.push('\n');
            i += 1;
            continue;
        }

        // Horizontal rules
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            if in_list {
                body.push_str(if list_is_ordered {
                    "</ol>\n"
                } else {
                    "</ul>\n"
                });
                in_list = false;
            }
            body.push_str("<hr />\n");
            i += 1;
            continue;
        }

        // Headings
        if let Some((level, text)) = parse_heading(trimmed) {
            if in_list {
                body.push_str(if list_is_ordered {
                    "</ol>\n"
                } else {
                    "</ul>\n"
                });
                in_list = false;
            }
            body.push_str(&format!(
                "<h{level}>{}</h{level}>\n",
                render_inlines(text.trim())
            ));
            i += 1;
            continue;
        }

        // Blockquotes
        if let Some(quote_text) = trimmed.strip_prefix("> ") {
            if in_list {
                body.push_str(if list_is_ordered {
                    "</ol>\n"
                } else {
                    "</ul>\n"
                });
                in_list = false;
            }
            body.push_str("<blockquote><p>");
            body.push_str(&render_inlines(quote_text));
            body.push_str("</p></blockquote>\n");
            i += 1;
            continue;
        }

        // Tables
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            if in_list {
                body.push_str(if list_is_ordered {
                    "</ol>\n"
                } else {
                    "</ul>\n"
                });
                in_list = false;
            }
            // Check if next line is a separator row
            if i + 1 < lines.len() && is_table_sep(lines[i + 1].trim()) {
                body.push_str("<table>\n<thead>\n<tr>\n");
                for cell in parse_table_row(trimmed) {
                    body.push_str(&format!("<th>{}</th>\n", render_inlines(&cell)));
                }
                body.push_str("</tr>\n</thead>\n<tbody>\n");
                i += 2; // skip header and sep
                while i < lines.len() && lines[i].trim().starts_with('|') {
                    body.push_str("<tr>\n");
                    for cell in parse_table_row(lines[i].trim()) {
                        body.push_str(&format!("<td>{}</td>\n", render_inlines(&cell)));
                    }
                    body.push_str("</tr>\n");
                    i += 1;
                }
                body.push_str("</tbody>\n</table>\n");
                continue;
            }
        }

        // Checklists
        if let Some(task_body) = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
        {
            if !in_list {
                body.push_str("<ul class=\"task-list\">\n");
                in_list = true;
                list_is_ordered = false;
            }
            body.push_str("<li class=\"task-item\"><input type=\"checkbox\" disabled /> ");
            body.push_str(&render_inlines(task_body));
            body.push_str("</li>\n");
            i += 1;
            continue;
        }
        if let Some(task_body) = trimmed
            .strip_prefix("- [x] ")
            .or_else(|| trimmed.strip_prefix("- [X] "))
            .or_else(|| trimmed.strip_prefix("* [x] "))
        {
            if !in_list {
                body.push_str("<ul class=\"task-list\">\n");
                in_list = true;
                list_is_ordered = false;
            }
            body.push_str("<li class=\"task-item\"><input type=\"checkbox\" checked disabled /> ");
            body.push_str(&render_inlines(task_body));
            body.push_str("</li>\n");
            i += 1;
            continue;
        }

        // Unordered lists
        if let Some(item_body) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            if !in_list || list_is_ordered {
                if in_list {
                    body.push_str("</ol>\n");
                }
                body.push_str("<ul>\n");
                in_list = true;
                list_is_ordered = false;
            }
            body.push_str("<li>");
            body.push_str(&render_inlines(item_body));
            body.push_str("</li>\n");
            i += 1;
            continue;
        }

        // Ordered lists
        if let Some(item_body) = parse_ordered_list(trimmed) {
            if !in_list || !list_is_ordered {
                if in_list {
                    body.push_str("</ul>\n");
                }
                body.push_str("<ol>\n");
                in_list = true;
                list_is_ordered = true;
            }
            body.push_str("<li>");
            body.push_str(&render_inlines(item_body));
            body.push_str("</li>\n");
            i += 1;
            continue;
        }

        // Empty lines reset lists
        if trimmed.is_empty() {
            if in_list {
                body.push_str(if list_is_ordered {
                    "</ol>\n"
                } else {
                    "</ul>\n"
                });
                in_list = false;
            }
            i += 1;
            continue;
        }

        // Regular paragraph
        if in_list {
            body.push_str(if list_is_ordered {
                "</ol>\n"
            } else {
                "</ul>\n"
            });
            in_list = false;
        }
        body.push_str("<p>");
        body.push_str(&render_inlines(trimmed));
        body.push_str("</p>\n");
        i += 1;
    }

    if in_code_block {
        body.push_str("</code></pre>\n");
    }
    if in_list {
        body.push_str(if list_is_ordered {
            "</ol>\n"
        } else {
            "</ul>\n"
        });
    }

    let escaped_title = escape_html(title);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>{escaped_title}</title>
<style>
  :root {{
    --bg: #FAF9F6;
    --text: #1E2623;
    --text-dim: #4F645B;
    --line: #C7D5CF;
    --accent: #2D6A4F;
    --code-bg: #EBF0EE;
    --quote-bg: #F3F1EC;
    --mark-bg: #FFE066;
  }}
  @media (prefers-color-scheme: dark) {{
    :root {{
      --bg: #1E1F1F;
      --text: #F1F5F4;
      --text-dim: #879B93;
      --line: #3B4E47;
      --accent: #3D8B68;
      --code-bg: #272B29;
      --quote-bg: #232826;
      --mark-bg: #876E00;
    }}
  }}
  body {{
    background-color: var(--bg);
    color: var(--text);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    font-size: 16px;
    line-height: 1.6;
    margin: 0;
    padding: 2.5rem 1.5rem;
  }}
  main {{
    max-width: 820px;
    margin: 0 auto;
  }}
  h1, h2, h3, h4, h5, h6 {{
    color: var(--text);
    font-weight: 600;
    line-height: 1.25;
    margin-top: 1.8rem;
    margin-bottom: 0.6rem;
  }}
  h1 {{ font-size: 2rem; border-bottom: 1px solid var(--line); padding-bottom: 0.4rem; }}
  h2 {{ font-size: 1.5rem; border-bottom: 1px solid var(--line); padding-bottom: 0.3rem; }}
  h3 {{ font-size: 1.25rem; }}
  p {{ margin: 0.8rem 0; }}
  a {{ color: var(--accent); text-decoration: none; }}
  a:hover {{ text-decoration: underline; }}
  blockquote {{
    margin: 1rem 0;
    padding: 0.5rem 1rem;
    background: var(--quote-bg);
    border-left: 4px solid var(--accent);
    color: var(--text-dim);
  }}
  code {{
    background-color: var(--code-bg);
    font-family: "JetBrains Mono", "IBM Plex Mono", Consolas, monospace;
    font-size: 0.9em;
    padding: 0.2em 0.4em;
    border-radius: 4px;
  }}
  pre {{
    background-color: var(--code-bg);
    padding: 1rem;
    border-radius: 6px;
    overflow-x: auto;
    border: 1px solid var(--line);
  }}
  pre code {{
    background: none;
    padding: 0;
    font-size: 0.88rem;
  }}
  hr {{
    border: none;
    border-top: 1px solid var(--line);
    margin: 2rem 0;
  }}
  table {{
    border-collapse: collapse;
    width: 100%;
    margin: 1.2rem 0;
  }}
  th, td {{
    border: 1px solid var(--line);
    padding: 0.55rem 0.85rem;
    text-align: left;
  }}
  th {{
    background-color: var(--code-bg);
    font-weight: 600;
  }}
  ul, ol {{ padding-left: 2rem; margin: 0.8rem 0; }}
  li {{ margin: 0.25rem 0; }}
  ul.task-list {{ list-style: none; padding-left: 0.5rem; }}
  li.task-item {{ display: flex; align-items: baseline; gap: 0.5rem; }}
  mark {{ background-color: var(--mark-bg); color: inherit; padding: 0.1em 0.3em; border-radius: 2px; }}
  img {{ max-width: 100%; height: auto; border-radius: 4px; }}
</style>
</head>
<body>
<main>
{body}</main>
</body>
</html>
"#
    )
}

fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut count = 0;
    while count < bytes.len() && bytes[count] == b'#' && count < 6 {
        count += 1;
    }
    if count > 0 && count < bytes.len() && bytes[count] == b' ' {
        Some((count, &line[count + 1..]))
    } else {
        None
    }
}

fn parse_ordered_list(line: &str) -> Option<&str> {
    let dot = line.find(". ")?;
    if dot > 0 && line[..dot].chars().all(|c| c.is_ascii_digit()) {
        Some(&line[dot + 2..])
    } else {
        None
    }
}

fn is_table_sep(line: &str) -> bool {
    line.starts_with('|')
        && line.ends_with('|')
        && line
            .chars()
            .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
}

fn parse_table_row(line: &str) -> Vec<String> {
    let trimmed = line.trim_matches('|');
    trimmed.split('|').map(|s| s.trim().to_owned()).collect()
}

/// Escapes HTML special characters.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Renders inline Markdown syntax (bold, italic, strike, underline, highlight, code, links, images).
fn render_inlines(mut text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);

    while !text.is_empty() {
        // Raw HTML tags like `<span style="color: #RRGGBB">...</span>` or `<u>...</u>`
        if text.starts_with("<span style=\"color:") {
            if let Some(end) = text.find("</span>") {
                let close = end + "</span>".len();
                out.push_str(&text[..close]);
                text = &text[close..];
                continue;
            }
        }
        if text.starts_with("<u>") {
            if let Some(end) = text.find("</u>") {
                let close = end + "</u>".len();
                out.push_str("<u>");
                out.push_str(&render_inlines(&text[3..end]));
                out.push_str("</u>");
                text = &text[close..];
                continue;
            }
        }

        // Inline code `` `code` ``
        if text.starts_with('`') {
            if let Some(end) = text[1..].find('`') {
                let inner = &text[1..=end];
                out.push_str("<code>");
                out.push_str(&escape_html(inner));
                out.push_str("</code>");
                text = &text[end + 2..];
                continue;
            }
        }

        // Images `![alt](url)`
        if text.starts_with("![") {
            if let Some(close_bracket) = text.find(']') {
                if text[close_bracket..].starts_with("](") {
                    let rest = &text[close_bracket + 2..];
                    if let Some(close_paren) = rest.find(')') {
                        let alt = &text[2..close_bracket];
                        let url = &rest[..close_paren];
                        out.push_str(&format!(
                            "<img src=\"{}\" alt=\"{}\" />",
                            escape_html(url),
                            escape_html(alt)
                        ));
                        text = &rest[close_paren + 1..];
                        continue;
                    }
                }
            }
        }

        // Links `[label](url)`
        if text.starts_with('[') {
            if let Some(close_bracket) = text.find(']') {
                if text[close_bracket..].starts_with("](") {
                    let rest = &text[close_bracket + 2..];
                    if let Some(close_paren) = rest.find(')') {
                        let label = &text[1..close_bracket];
                        let url = &rest[..close_paren];
                        out.push_str(&format!(
                            "<a href=\"{}\">{}</a>",
                            escape_html(url),
                            render_inlines(label)
                        ));
                        text = &rest[close_paren + 1..];
                        continue;
                    }
                }
            }
        }

        // Bold `**text**`
        if text.starts_with("**") {
            if let Some(end) = text[2..].find("**") {
                let inner = &text[2..2 + end];
                out.push_str("<strong>");
                out.push_str(&render_inlines(inner));
                out.push_str("</strong>");
                text = &text[4 + end..];
                continue;
            }
        }

        // Italic `*text*`
        if text.starts_with('*') && !text.starts_with("**") {
            if let Some(end) = text[1..].find('*') {
                let inner = &text[1..1 + end];
                out.push_str("<em>");
                out.push_str(&render_inlines(inner));
                out.push_str("</em>");
                text = &text[2 + end..];
                continue;
            }
        }

        // Strikethrough `~~text~~`
        if text.starts_with("~~") {
            if let Some(end) = text[2..].find("~~") {
                let inner = &text[2..2 + end];
                out.push_str("<del>");
                out.push_str(&render_inlines(inner));
                out.push_str("</del>");
                text = &text[4 + end..];
                continue;
            }
        }

        // Highlight `==text==`
        if text.starts_with("==") {
            if let Some(end) = text[2..].find("==") {
                let inner = &text[2..2 + end];
                out.push_str("<mark>");
                out.push_str(&render_inlines(inner));
                out.push_str("</mark>");
                text = &text[4 + end..];
                continue;
            }
        }

        // Plain character
        let ch = text.chars().next().unwrap();
        let len = ch.len_utf8();
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
        text = &text[len..];
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_basic_document_structure() {
        let md = "# Title\n\nParagraph with **bold**, *italic*, `code`, and [link](https://example.com).\n\n- Item 1\n- Item 2\n";
        let html = export_html("Test", md);
        assert!(html.contains("<title>Test</title>"));
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<a href=\"https://example.com\">link</a>"));
        assert!(html.contains("<ul>\n<li>Item 1</li>"));
    }

    #[test]
    fn exports_table_and_task_list() {
        let md = "- [ ] Task open\n- [x] Task done\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
        let html = export_html("Tasks", md);
        assert!(html.contains("<input type=\"checkbox\" disabled /> Task open"));
        assert!(html.contains("<input type=\"checkbox\" checked disabled /> Task done"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>A</th>"));
        assert!(html.contains("<td>1</td>"));
    }
}
