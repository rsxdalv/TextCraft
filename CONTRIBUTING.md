# Contributing

This is the developer guide. For what the app is and how to try it, see the
[README](README.md).

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

The executable is named `notepad` on Windows targets and `notecraft` elsewhere.

## Workspace checks

Rust is the only implementation language in this repository — application code,
tests, build tooling, and CI included. Repository automation lives in
`crates/xtask` and is invoked as `cargo xtask <task>`.

Run the checks before considering a change complete:

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
error handling, and the definition of done — are in [`AGENT.md`](AGENT.md).

## Architecture

The app is built around a captured UI reference and a single authoritative command
registry: the window draws from the registry and routes every menu, toolbar, and
keyboard action back through it, so the same command runs however it is invoked.

It is a Cargo workspace split into headless engines and a thin UI shell.
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

### Repository layout

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
site/         # the GitHub Pages landing page
```

## Documentation

| Document | Contents |
| --- | --- |
| [`ROADMAP.md`](ROADMAP.md) | Project goal, technology strategy, and milestones. |
| [`docs/ui-reference.md`](docs/ui-reference.md) | The captured UI: screens and the exact items each one shows. |
| [`docs/feature-matrix.md`](docs/feature-matrix.md) | Per-feature implementation and verification status — the source of truth. |
| [`docs/architecture.md`](docs/architecture.md) | Crate boundaries, module layout, and dependency direction. |
| [`docs/test-strategy.md`](docs/test-strategy.md) | Unit, integration, UI, and stress-testing approach. |
| [`AGENT.md`](AGENT.md) | Engineering rules and the required workflow. |

## Project status

The project is in active early development and has no release yet. The workspace
builds and the headless engines are largely implemented; several UI surfaces and
session features are still ahead.

Milestone progress, from [`ROADMAP.md`](ROADMAP.md):

- **M0 — specification and UI capture:** complete.
- **M1–M5 — Rust foundation, the `egui`/`eframe` shell, the text engine, file I/O,
  and the editing commands and search:** in progress and partly implemented. Each
  tab is backed by a `TextFile`, and the Find/Replace, Go-to, and Open/Save-as
  dialogs are built on the `nc-textproc` engines, with the unsaved-changes,
  overwrite, and unrepresentable prompts and the dirty indicator on the tab.
- **M6 — tabs and lifecycle:** in progress. Tab switching, Save all, duplicate-open
  identity, New window, recent files, and session restore (which reopens the
  previous documents, including unsaved work) are in place, and seven of its
  behaviour criteria are verified by an acceptance suite.
- **M7 — Markdown:** complete. `nc-markdown` implements the formatting model, the
  shell dispatches every Markdown command against it, and a Markdown document
  switches between its source and a rendering of it.
- **Still ahead:** settings and desktop integration, optional features,
  performance, validation, and release engineering.

See [`docs/feature-matrix.md`](docs/feature-matrix.md) for per-feature status.

## Website

The landing page in [`site/`](site) is published to GitHub Pages by
[`.github/workflows/pages.yml`](.github/workflows/pages.yml) whenever `site/`
changes on `main`.

## Licensing of fonts

The four editor fonts are separate works under the SIL Open Font License 1.1,
shipped in [`crates/platform/assets/fonts`](crates/platform/assets/fonts) with each
family's licence text beside its file and their provenance in that directory's
`README.md`.
