use std::path::PathBuf;

use clearwin_core::schedule;
use clearwin_core::settings::{self, config_dir, HistoryEntry, Schedule, Settings};

/// Records something ClearWin did; a failure to write history never fails the action.
pub fn record(action: &str, items: u64, bytes: u64, details: Option<String>) {
    if items > 0 || bytes > 0 {
        let _ = settings::append_history(
            &config_dir(),
            &HistoryEntry::now(action, items, bytes, details),
        );
    }
}

pub fn current() -> Settings {
    settings::load(&config_dir())
}

fn update(change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    let mut s = current();
    change(&mut s);
    settings::save(&config_dir(), &s).map_err(|e| format!("Could not save settings: {e}"))?;
    Ok(s)
}

#[tauri::command]
pub fn get_settings() -> Settings {
    current()
}

#[tauri::command]
pub fn set_cleaner_selection(ids: Vec<String>) -> Result<Settings, String> {
    update(|s| s.cleaner_selection = Some(ids))
}

#[tauri::command]
pub fn set_exclusions(paths: Vec<PathBuf>) -> Result<Settings, String> {
    update(|s| s.exclusions = paths)
}

#[tauri::command]
pub fn set_schedule(schedule: Option<Schedule>) -> Result<Settings, String> {
    let schedule = match schedule {
        Some(mut sch) => {
            sch.time = settings::parse_time(&sch.time).ok_or("Enter a time like 10:00.")?;
            Some(sch)
        }
        None => None,
    };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    schedule::apply(&exe, schedule.as_ref())?;
    update(|s| s.schedule = schedule)
}

#[tauri::command]
pub fn get_history() -> Vec<HistoryEntry> {
    settings::read_history(&config_dir())
}
