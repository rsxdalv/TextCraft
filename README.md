# TextCraft

A fast, modern text and Markdown editor written in Rust, drawn with
[`egui`](https://github.com/emilk/egui) and hosted by
[`eframe`](https://github.com/emilk/egui/tree/master/crates/eframe).

TextCraft is built around a captured UI reference and a single authoritative
command registry: the window draws from the registry and routes every menu,
toolbar, and keyboard action back through it, so the same command runs however
it is invoked.

- One portable UI stack — the same build runs everywhere `eframe` supports.
- Headless engines for documents, text processing, and commands, kept free of
  any window dependency so they build and test anywhere.
- Data integrity first: atomic saves, explicit encoding and line-ending
  handling, and no silent loss of document content.
- Test-backed behaviour rather than a UI that merely looks familiar.

## Status

TextCraft is in active early development and has no release yet. The workspace
builds and the headless engines are largely implemented; several UI surfaces and
session features are still ahead.

Milestone progress from [`ROADMAP.md`](ROADMAP.md). M0 — the specification and
UI capture — is complete. M1 through M5 — the Rust foundation, the
`egui`/`eframe` shell, the text engine, file I/O, and the editing commands and
search — are in progress and partly implemented; the shell backs each tab with a
`TextFile`, and the Find/Replace, Go-to, and Open/Save-as dialogs are built on
the `nc-textproc` engines, with the unsaved-changes, overwrite, and
unrepresentable prompts and the dirty indicator on the tab. M6 — tabs and
lifecycle — is in progress: tab switching, Save all, duplicate-open identity,
New window, recent files, and session restore (which reopens the previous
documents, including unsaved work) are in place, and seven of its behaviour
criteria are verified by an acceptance suite. M7 is complete: `nc-markdown` implements
the formatting model, the shell dispatches every Markdown command against it, and a
Markdown document switches between its source and a rendering of it, so bold, headings,
lists, nesting, links, tables and clear formatting are reachable from the toolbar, the
menus and the shortcuts. Settings and desktop integration, optional features,
performance, validation, and release engineering are still ahead.

See [`docs/feature-matrix.md`](docs/feature-matrix.md) for per-feature
implementation and verification status — it is the source of truth.

## Features

**File and document lifecycle**

- New document and new tab (`Ctrl+N`), close tab (`Ctrl+W`).
- Open (`Ctrl+O`), Save (`Ctrl+S`), Save As with encoding selection
  (`Ctrl+Shift+S`).
- Session restore: the previous session's tabs, including unsaved work, reopen
  on the next start when no files are named on the command line.
- Recent files: `File ▸ Recent` lists the documents opened so far, newest first,
  and choosing one reopens it — focusing the tab if it is already open.
- Encoding detection and transcoding, with atomic writes that never overwrite a
  file before the replacement data is safely prepared.
- Explicit BOM, line-ending (CRLF/LF/CR), and dirty-state handling.

**Editing**

- Multilevel undo/redo, cut/copy/paste with EOL conversion.
- Caret navigation across word, line, and paragraph boundaries.
- Mouse selection (click, drag, double-click, triple-click).
- Word wrap, tab stops, drag-and-drop text.
- Unicode-correct editing, including astral characters, combining marks, emoji,
  and IME composition.

**Find and navigation**

- Find (`Ctrl+F`), find next/previous (`F3` / `Shift+F3`).
- Replace and Replace All (`Ctrl+H`), applied as a single undoable action.
- Search options: match case and whole word, with wrap-around and boundary
  notices.
- Go to line (`Ctrl+G`).

**UI shell**

- Tab strip and title bar, File/Edit/View menu bar, formatting toolbar, editor
  surface, status bar, and a Settings page, matching
  [`docs/ui-reference.md`](docs/ui-reference.md).
- Find/Replace, Go to line, and Open/Save-as dialogs, with the unsaved-changes,
  overwrite, and unrepresentable prompts.
- A dirty indicator on the tab, and a status bar with one-based line and column
  reporting.
- Keyboard accelerators dispatched from the command registry, so every
  implemented command runs the same whether it is invoked from the menu or the
  keyboard; `F5` inserts the current time and date.

## Architecture

TextCraft is a Cargo workspace split into headless engines and a thin UI shell.
Dependencies flow in one direction: the engines know nothing about the window
layer, and `cargo xtask check-boundaries` enforces that direction as a CI gate.

| Crate | Package | Role |
| --- | --- | --- |
| [`crates/core`](crates/core) | `nc-core` | Document model: text buffer, selection, caret, undo/redo, clipboard, word wrap, tabs, line endings. Headless. |
| [`crates/textproc`](crates/textproc) | `nc-textproc` | Encoding detection and transcoding, search/replace, time/date insertion, atomic file I/O. Headless. |
| [`crates/markdown`](crates/markdown) | `nc-markdown` | Loss-keeping Markdown parse, and formatting commands expressed as edits to the source. Headless. |
| [`crates/commands`](crates/commands) | `nc-commands` | The command registry, menu model, shortcuts, and availability rules. Headless. |
| [`crates/platform`](crates/platform) | `nc-platform` | The `egui`/`eframe` shell: window, tab strip, menus, toolbar, editor surface, status bar, settings page. |
| [`crates/app`](crates/app) | `nc-app` | Startup, logging, command line, and process lifecycle. Produces the executable. |
| [`crates/xtask`](crates/xtask) | `xtask` | Repository maintenance tasks, run as `cargo xtask <task>`. |

`crates/commands` is the single authoritative definition of the command
surface: every menu item, toolbar control, label, shortcut, and availability
rule lives there, including the availability rules that must visibly disable a
control instead of leaving it a silent no-op.

## Getting started

### Prerequisites

- Rust stable, pinned through [`rust-toolchain.toml`](rust-toolchain.toml)
  (edition 2024, minimum Rust 1.85). Install via [rustup](https://rustup.rs/).
- A desktop environment supported by `eframe` (Linux, macOS, or Windows).

### Build and run

```bash
# Build the workspace
cargo build

# Run the editor
cargo run -p nc-app
```

Release builds are optimised for size (`opt-level = "z"`, thin LTO, stripped):

```bash
cargo build --release
```

The executable is named `textcraft` on every platform.

## Releasing

Pushing a tag builds and publishes every package (`.github/workflows/release.yml`):

| Platform | Package |
| --- | --- |
| Windows x64 | `.msi` installer and portable `.zip` |
| macOS (Apple silicon + Intel) | `TextCraft.app` in a `.zip` |
| Linux x86_64 | `.tar.gz` (binary, `.desktop` launcher, icon) |

1. Set `version` under `[workspace.package]` in `Cargo.toml`, and commit it with `Cargo.lock`.
2. `git tag v0.1.0 && git push origin v0.1.0` (the tag must equal `v` + that version; the
   workflow stops at once if they differ). A suffix such as `v0.2.0-rc.1` makes a pre-release.

The Actions tab's **Run workflow** button builds all packages and keeps them as workflow
artifacts without publishing anything, which is how to try a change to the workflow.

The Windows installer also registers TextCraft under the name `notepad`, so Win+R and the
Start menu's "notepad" open it instead of Windows Notepad (uninstalling restores Notepad;
the real `notepad.exe` is never touched). Delete the `App Paths\notepad.exe` entry in
`packaging/windows/textcraft.wxs` to opt out.

The packages are unsigned, so the first launch asks for confirmation (Windows SmartScreen:
More info, Run anyway; macOS: System Settings, Privacy & Security, Open Anyway). The Linux
build needs glibc 2.39 or newer, because it is built on Ubuntu 24.04.

To build the Windows packages locally, install WiX v5 (`dotnet tool install --global wix
--version 5.0.2`) and run `pwsh packaging/windows/package.ps1`; the results are in `dist/release`.
`packaging/linux/package.sh` and `packaging/macos/package.sh` take a built binary. The app icon
files in `assets/` are generated by `cargo test -p nc-platform write_icon_file -- --ignored`.

## Development

Rust is the only implementation language in this repository — application code,
tests, build tooling, and CI included. Repository automation lives in
`crates/xtask` and is invoked as `cargo xtask <task>`.

Run the workspace checks before considering a change complete:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask check-boundaries
```

`cargo xtask check-boundaries` verifies the crate dependency direction described
under [Architecture](#architecture) and acts as the CI gate.

Contribution rules — workflow, architecture constraints, testing requirements,
error handling, and the definition of done — are in
[`AGENT.md`](AGENT.md).

## Documentation

| Document | Contents |
| --- | --- |
| [`ROADMAP.md`](ROADMAP.md) | Project goal, technology strategy, and milestones. |
| [`docs/ui-reference.md`](docs/ui-reference.md) | The captured UI: screens and the exact items each one shows. |
| [`docs/feature-matrix.md`](docs/feature-matrix.md) | Per-feature implementation and verification status. |
| [`docs/architecture.md`](docs/architecture.md) | Crate boundaries, module layout, and dependency direction. |
| [`docs/test-strategy.md`](docs/test-strategy.md) | Unit, integration, UI, and stress-testing approach. |
| [`AGENT.md`](AGENT.md) | Engineering rules and the required workflow. |

## Repository layout

```text
crates/
  app/        # nc-app — startup, logging, CLI, lifecycle
  commands/   # nc-commands — command registry (headless)
  core/       # nc-core — document model (headless)
  markdown/   # nc-markdown — Markdown formatting model (headless)
  platform/   # nc-platform — egui/eframe shell
  textproc/   # nc-textproc — encoding, search, atomic I/O (headless)
  xtask/      # cargo xtask tasks
docs/         # architecture, feature matrix, test strategy, UI reference
```

## License

The code is licensed under the MIT License. See [`LICENSE`](LICENSE) for details.

The four editor fonts are separate works under the SIL Open Font License 1.1, shipped in
[`crates/platform/assets/fonts`](crates/platform/assets/fonts) with each family's licence
text beside its file and their provenance in that directory's `README.md`.

Repository: <https://github.com/CMSpeedAU/TextCraft>
