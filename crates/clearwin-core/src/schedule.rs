//! The scheduled background clean, run by Windows Task Scheduler.

use std::path::Path;
use std::time::SystemTime;

use crate::settings::{self, Frequency, HistoryEntry, Schedule};
use crate::{catalog, clean_category, scan_category_excluding, Category, Paths, RunningApps};

pub const TASK_NAME: &str = "ClearWin automatic clean";
/// Command-line flag the scheduled task starts ClearWin with.
pub const AUTO_CLEAN_FLAG: &str = "--auto-clean";

/// The categories ticked in the Cleaner (or the defaults if never changed).
fn ticked(settings: &settings::Settings) -> Vec<Category> {
    catalog(&Paths::from_env())
        .into_iter()
        .filter(|c| match &settings.cleaner_selection {
            Some(ids) => ids.contains(&c.id),
            None => c.default_selected,
        })
        .collect()
}

/// How many bytes cleaning the ticked categories would free right now. Deletes nothing.
pub fn measure_junk(config_dir: &Path) -> u64 {
    let settings = settings::load(config_dir);
    let running = RunningApps::snapshot();
    let now = SystemTime::now();
    ticked(&settings)
        .iter()
        .map(|c| scan_category_excluding(c, &running, now, &settings.exclusions).summary(0))
        .filter(|s| s.blocked_reason.is_none())
        .map(|s| s.total_bytes)
        .sum()
}

/// Cleans the categories ticked in the Cleaner and records it in the history as `action`.
pub fn clean_ticked(config_dir: &Path, action: &str) -> HistoryEntry {
    let settings = settings::load(config_dir);
    let running = RunningApps::snapshot();
    let now = SystemTime::now();
    let (mut items, mut bytes, mut skipped) = (0, 0, 0);
    for category in ticked(&settings) {
        let scan = scan_category_excluding(&category, &running, now, &settings.exclusions);
        let result = clean_category(&category, &scan, &running, now);
        items += result.removed_items;
        bytes += result.freed_bytes;
        skipped += result.failed_items + u64::from(result.skipped_reason.is_some());
    }
    let details = (skipped > 0).then(|| format!("{skipped} items in use or skipped"));
    let entry = HistoryEntry::now(action, items, bytes, details);
    let _ = settings::append_history(config_dir, &entry);
    entry
}

/// Cleans the categories saved in settings, without a window, and records it in the history.
pub fn run_auto_clean(config_dir: &Path) -> HistoryEntry {
    clean_ticked(config_dir, "Automatic clean")
}

/// Arguments for `schtasks /Create` that run `exe --auto-clean` on the schedule.
pub fn create_task_args(exe: &Path, schedule: &Schedule) -> Option<Vec<String>> {
    let time = settings::parse_time(&schedule.time)?;
    let mut args = vec![
        "/Create".into(),
        "/F".into(),
        "/TN".into(),
        TASK_NAME.into(),
        "/TR".into(),
        format!("\"{}\" {AUTO_CLEAN_FLAG}", exe.display()),
        "/ST".into(),
        time,
    ];
    match schedule.frequency {
        Frequency::Daily => args.extend(["/SC".into(), "DAILY".into()]),
        Frequency::Weekly => {
            args.extend(["/SC".into(), "WEEKLY".into(), "/D".into(), "SUN".into()])
        }
    }
    Some(args)
}

#[cfg(windows)]
fn schtasks(args: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(not(windows))]
fn schtasks(_args: &[String]) -> Result<(), String> {
    Err("Scheduling is only available on Windows.".into())
}

/// Creates, replaces or removes the scheduled task to match `schedule`.
pub fn apply(exe: &Path, schedule: Option<&Schedule>) -> Result<(), String> {
    match schedule {
        Some(schedule) => {
            schtasks(&create_task_args(exe, schedule).ok_or("Enter a time like 10:00.")?)
        }
        None => {
            // Deleting a task that doesn't exist is fine.
            let _ = schtasks(&[
                "/Delete".into(),
                "/F".into(),
                "/TN".into(),
                TASK_NAME.into(),
            ]);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_weekly_task() {
        let schedule = Schedule {
            frequency: Frequency::Weekly,
            time: "9:30".into(),
        };
        let args = create_task_args(Path::new(r"C:\Apps\ClearWin.exe"), &schedule).unwrap();
        assert!(args.contains(&r#""C:\Apps\ClearWin.exe" --auto-clean"#.to_string()));
        assert!(args.windows(2).any(|w| w == ["/ST", "09:30"]));
        assert!(args.windows(2).any(|w| w == ["/D", "SUN"]));
        let bad = Schedule {
            frequency: Frequency::Daily,
            time: "25:00".into(),
        };
        assert!(create_task_args(Path::new("x"), &bad).is_none());
    }

    #[test]
    fn auto_clean_uses_saved_selection_and_records_history() {
        let dir = tempfile::tempdir().unwrap();
        let s = settings::Settings {
            cleaner_selection: Some(vec![]),
            ..Default::default()
        };
        settings::save(dir.path(), &s).unwrap();
        let entry = run_auto_clean(dir.path());
        assert_eq!(entry.items, 0);
        assert_eq!(settings::read_history(dir.path()).len(), 1);
    }
}
