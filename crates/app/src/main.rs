// A release build on Windows is a GUI program: no console window opens behind it. A debug
// build keeps the console, where the logs go. `--help` and `--version` still print from a
// terminal, through `console::attach_parent`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    nc_app::run_from_env()
}
