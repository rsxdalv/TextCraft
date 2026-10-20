//! Output for a program that has no console of its own.
//!
//! A release build on Windows uses the GUI subsystem, so Windows gives it no console and
//! its standard handles are empty: `textcraft --version` would print nothing. Where one
//! was started from a terminal, [`attach_parent`] joins that terminal's console and points
//! the standard output and error handles at it. Everywhere else it does nothing.

/// Joins the parent process's console, if it has one and the standard handles need it.
///
/// A handle that is already valid — output redirected to a file or a pipe, as in a script
/// or `| more` — is left alone, so redirection keeps working.
#[cfg(all(windows, not(debug_assertions)))]
pub fn attach_parent() {
    use std::fs::OpenOptions;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };

    // SAFETY: `GetStdHandle` only reads the process's handle table.
    let missing = |which| {
        let handle = unsafe { GetStdHandle(which) };
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    };
    let (out_missing, err_missing) = (missing(STD_OUTPUT_HANDLE), missing(STD_ERROR_HANDLE));
    if !out_missing && !err_missing {
        return;
    }
    // SAFETY: a plain Win32 call with no pointers; failure just means no console.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }
    let Ok(console) = OpenOptions::new().write(true).open("CONOUT$") else {
        return;
    };
    let handle = console.as_raw_handle();
    // SAFETY: `handle` is a live console handle; it is leaked below so it stays valid for
    // the life of the process, which is as long as the standard handles use it.
    unsafe {
        if out_missing {
            SetStdHandle(STD_OUTPUT_HANDLE, handle.cast());
        }
        if err_missing {
            SetStdHandle(STD_ERROR_HANDLE, handle.cast());
        }
    }
    std::mem::forget(console);
}

/// Joins the parent process's console, if it has one.
#[cfg(not(all(windows, not(debug_assertions))))]
pub fn attach_parent() {}
