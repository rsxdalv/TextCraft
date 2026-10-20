# Third-party notices

TextCraft is licensed under the MIT License (see `LICENSE`). Some of its code and design
is derived from the projects below, which are used under their own licences.

## NoteCraft (AlienTech Software)

- Project: NoteCraft, <https://github.com/AlienTechSoftware/notecraft>. It is a different
  application that happens to share its name with this repository's working name; the two
  projects are not affiliated.
- Licence: dual licensed MIT OR Apache-2.0. TextCraft uses it under the MIT licence.
- Copyright (c) 2026 AlienTech Software and the NoteCraft contributors.

Derived files:

| TextCraft file | Derived from (upstream) | What was adapted |
|---|---|---|
| `crates/platform/src/ui/styled_text.rs` | `crates/ui-egui/src/pagetext.rs` | The approach and the scanner: Markdown text is split into styled ranges by a pure function and turned into an egui `LayoutJob` handed to `TextEdit` through a layouter, with the Markdown marks kept in the text but dimmed. Changed for TextCraft: page formats and tags removed, colours taken from TextCraft's palette, the user's monospace face kept for every style, strikethrough added, and a widest-line measurement for the unwrapped editor. |
| `crates/platform/src/ui/editor.rs` | `crates/ui-egui/src/pagetext.rs` | Only the use of the layouter in the Markdown source view. |

The upstream `NOTICE` and `ATTRIBUTION.md` ask for nothing further for this code: their
third-party list concerns the upstream project's own artwork and fonts, none of which
TextCraft uses.

The MIT licence text, as published by the upstream project in `LICENSE-MIT`:

```text
MIT License

Copyright (c) 2026 AlienTech Software and the NoteCraft contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
