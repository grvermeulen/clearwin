//! Scanning and cleaning engine for ClearWin.
//!
//! The engine works in two phases so nothing is ever deleted unseen:
//! [`scan`] builds a list of exactly what would be removed, and [`clean`]
//! deletes only entries from that list, re-checking each one first.

mod catalog;
pub mod cmdline;
pub mod defender;
pub mod drivers;
pub mod duplicates;
mod engine;
pub mod files;
mod firefox;
pub mod monitor;
mod paths;
pub mod performance;
mod processes;
pub mod ps;
mod recycle_bin;
pub mod registry;
pub mod schedule;
pub mod settings;
pub mod software;
pub mod startup;
pub mod storage;
pub mod system;
pub mod uninstall;

pub use catalog::{catalog, Action, Category, NameFilter, Target};
pub use engine::{
    clean_category, is_excluded, scan_category, scan_category_excluding, CategoryClean,
    CategoryScan, CategorySummary, FileEntry,
};
pub use paths::Paths;
pub use processes::RunningApps;

/// Turns an access error into a hint to restart as administrator when that would help.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn admin_hint(err: std::io::Error, needs_admin: bool) -> String {
    if needs_admin && err.kind() == std::io::ErrorKind::PermissionDenied {
        "Restart ClearWin as administrator to change this.".into()
    } else {
        err.to_string()
    }
}

/// Starts a program with a raw argument string, asking Windows for
/// administrator rights when the program requires them.
#[cfg(windows)]
pub(crate) fn launch(program: &str, args: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const ERROR_ELEVATION_REQUIRED: i32 = 740;
    match std::process::Command::new(program).raw_arg(args).spawn() {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => elevated(program, args),
        Err(e) => Err(format!("Could not start {program}: {e}")),
    }
}

/// Runs a program through the Windows "Run as administrator" prompt.
#[cfg(windows)]
pub fn elevated(program: &str, args: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &str| OsStr::new(s).encode_wide().chain([0]).collect::<Vec<u16>>();
    let (verb, file, params) = (wide("runas"), wide(program), wide(args));
    // SAFETY: all strings are null-terminated UTF-16 and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            if args.is_empty() {
                std::ptr::null()
            } else {
                params.as_ptr()
            },
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values of 32 or less are errors, including the user declining the prompt.
    if result as isize <= 32 {
        return Err("Administrator rights were not granted.".into());
    }
    Ok(())
}
