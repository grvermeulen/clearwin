//! Cleaning a browser as soon as it is closed.
//!
//! When turned on, a small background copy of ClearWin (`--monitor`) starts
//! with Windows. It checks every few seconds which browsers are open, and when
//! one closes it cleans that browser's categories ticked in the Cleaner.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::settings::{self, HistoryEntry};
use crate::{catalog, clean_category, scan_category_excluding, Category, Paths, RunningApps};

/// Command-line flag that starts the background monitor.
pub const MONITOR_FLAG: &str = "--monitor";
/// Name of the value in the Windows "Run" key that starts the monitor at sign-in.
pub const RUN_VALUE: &str = "ClearWin browser cleaning";
const CHECK_EVERY: Duration = Duration::from_secs(5);

/// The ticked categories that belong to an app that must be closed first (the browsers),
/// grouped by that app.
pub fn browser_groups<'a>(
    categories: &'a [Category],
    selection: Option<&[String]>,
) -> Vec<(String, Vec<&'a Category>)> {
    let mut groups: Vec<(String, Vec<&Category>)> = Vec::new();
    for category in categories {
        let Some(app) = &category.close_app else {
            continue;
        };
        let ticked = match selection {
            Some(ids) => ids.contains(&category.id),
            None => category.default_selected,
        };
        if !ticked {
            continue;
        }
        match groups.iter_mut().find(|(a, _)| a == app) {
            Some((_, list)) => list.push(category),
            None => groups.push((app.clone(), vec![category])),
        }
    }
    groups
}

/// Whether the app went from open to fully closed between two snapshots.
pub fn just_closed(category: &Category, before: &RunningApps, after: &RunningApps) -> bool {
    before.any_running(&category.blocking_processes)
        && !after.any_running(&category.blocking_processes)
}

/// Cleans the categories of one browser that has just closed and records it in the history.
pub fn clean_closed(
    config_dir: &Path,
    app: &str,
    categories: &[&Category],
    exclusions: &[PathBuf],
) -> HistoryEntry {
    let running = RunningApps::snapshot();
    let now = SystemTime::now();
    let (mut items, mut bytes) = (0, 0);
    for category in categories {
        let scan = scan_category_excluding(category, &running, now, exclusions);
        let result = clean_category(category, &scan, &running, now);
        items += result.removed_items;
        bytes += result.freed_bytes;
    }
    let entry = HistoryEntry::now(&format!("Cleaned after closing {app}"), items, bytes, None);
    if items > 0 || bytes > 0 {
        let _ = settings::append_history(config_dir, &entry);
    }
    entry
}

fn pid_file(config_dir: &Path) -> PathBuf {
    config_dir.join("monitor.pid")
}

/// Another monitor is already running (its process id is saved and still alive as ClearWin).
fn already_running(config_dir: &Path) -> bool {
    let Some(pid) = fs::read_to_string(pid_file(config_dir))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    let mut system = System::new();
    let pid = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let own_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    system
        .process(pid)
        .is_some_and(|p| Some(p.name().to_string_lossy().to_lowercase()) == own_name)
}

/// Runs until browser cleaning is turned off in Settings.
pub fn run_monitor(config_dir: &Path) {
    if already_running(config_dir) {
        return;
    }
    let _ = fs::create_dir_all(config_dir);
    let _ = fs::write(pid_file(config_dir), std::process::id().to_string());
    let paths = Paths::from_env();
    let mut before = RunningApps::snapshot();
    loop {
        std::thread::sleep(CHECK_EVERY);
        let settings = settings::load(config_dir);
        if !settings.clean_on_browser_close {
            break;
        }
        let after = RunningApps::snapshot();
        let categories = catalog(&paths);
        for (app, list) in browser_groups(&categories, settings.cleaner_selection.as_deref()) {
            if just_closed(list[0], &before, &after) {
                clean_closed(config_dir, &app, &list, &settings.exclusions);
            }
        }
        before = after;
    }
    let _ = fs::remove_file(pid_file(config_dir));
}

/// Turns the monitor on or off: starts it with Windows, and right now.
#[cfg(windows)]
pub fn apply(exe: &Path, enabled: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let run = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?
        .0;
    if enabled {
        run.set_value(RUN_VALUE, &format!("\"{}\" {MONITOR_FLAG}", exe.display()))
            .map_err(|e| e.to_string())?;
        crate::ps::hidden(&exe.to_string_lossy())
            .arg(MONITOR_FLAG)
            .spawn()
            .map_err(|e| format!("Could not start browser cleaning: {e}"))?;
    } else {
        // The running monitor sees the setting is off and stops by itself.
        let _ = run.delete_value(RUN_VALUE);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn apply(_exe: &Path, _enabled: bool) -> Result<(), String> {
    Err("Only available on Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;

    fn category(id: &str, app: Option<&str>, default_selected: bool) -> Category {
        Category {
            id: id.into(),
            group: String::new(),
            name: String::new(),
            description: String::new(),
            default_selected,
            needs_admin: false,
            close_app: app.map(String::from),
            blocking_processes: if app.is_some() {
                vec!["chrome.exe"]
            } else {
                vec![]
            },
            action: Action::RecycleBin,
        }
    }

    #[test]
    fn groups_ticked_browser_categories() {
        let all = vec![
            category("windows.temp", None, true),
            category("chrome.cache", Some("Google Chrome"), true),
            category("chrome.history", Some("Google Chrome"), false),
        ];
        let groups = browser_groups(&all, None);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 1);
        let chosen = vec!["chrome.cache".to_string(), "chrome.history".to_string()];
        assert_eq!(browser_groups(&all, Some(&chosen))[0].1.len(), 2);
        assert!(browser_groups(&all, Some(&[])).is_empty());
    }

    #[test]
    fn notices_a_browser_closing() {
        let chrome = category("chrome.cache", Some("Google Chrome"), true);
        let open = RunningApps::from_names(["chrome.exe", "explorer.exe"]);
        let closed = RunningApps::from_names(["explorer.exe"]);
        assert!(just_closed(&chrome, &open, &closed));
        assert!(!just_closed(&chrome, &closed, &closed));
        assert!(!just_closed(&chrome, &open, &open));
    }
}
