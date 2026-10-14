//! Settings and the activity history, stored as JSON next to each other.
//!
//! Both the app and the scheduled background clean read these, so they live
//! in fixed per-user folders rather than in the window's browser storage.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Must match `identifier` in tauri.conf.json, which names the app's folders.
pub const APP_ID: &str = "com.grvermeulen.clearwin";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Cleaner categories ticked by the user; `None` until they first change it.
    pub cleaner_selection: Option<Vec<String>>,
    /// Folders and files the cleaner never touches.
    pub exclusions: Vec<PathBuf>,
    pub schedule: Option<Schedule>,
    /// Clean a browser's ticked categories as soon as it is closed.
    pub clean_on_browser_close: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub frequency: Frequency,
    /// 24-hour time, `HH:MM`.
    pub time: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Frequency {
    Daily,
    Weekly,
}

/// `%APPDATA%\com.grvermeulen.clearwin`, falling back to the temp folder elsewhere.
pub fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_ID)
}

pub fn load(dir: &Path) -> Settings {
    fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, settings: &Settings) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    // Write then rename, so a crash never leaves a half-written file.
    let tmp = dir.join("settings.json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, dir.join("settings.json"))
}

/// Checks a `HH:MM` time and returns it normalised.
pub fn parse_time(time: &str) -> Option<String> {
    let (h, m) = time.trim().split_once(':')?;
    let (h, m): (u8, u8) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then(|| format!("{h:02}:{m:02}"))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Seconds since the Unix epoch.
    pub time: u64,
    /// What happened, e.g. "Cleaned", "Automatic clean", "Registry fixed".
    pub action: String,
    pub items: u64,
    pub bytes: u64,
    pub details: Option<String>,
}

impl HistoryEntry {
    pub fn now(action: &str, items: u64, bytes: u64, details: Option<String>) -> Self {
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            time,
            action: action.to_string(),
            items,
            bytes,
            details,
        }
    }
}

const HISTORY: &str = "history.jsonl";
/// Older entries are dropped once the history grows past this.
const MAX_HISTORY: usize = 500;

pub fn append_history(dir: &Path, entry: &HistoryEntry) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let line = serde_json::to_string(entry).map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(HISTORY))?;
    writeln!(file, "{line}")?;
    drop(file);
    let all = read_history(dir);
    if all.len() > MAX_HISTORY {
        // read_history is newest first; keep the newest, rewrite oldest first.
        let keep: Vec<String> = all[..MAX_HISTORY]
            .iter()
            .rev()
            .filter_map(|e| serde_json::to_string(e).ok())
            .collect();
        fs::write(dir.join(HISTORY), keep.join("\n") + "\n")?;
    }
    Ok(())
}

/// History entries, newest first. Damaged lines are skipped.
pub fn read_history(dir: &Path) -> Vec<HistoryEntry> {
    let text = fs::read_to_string(dir.join(HISTORY)).unwrap_or_default();
    let mut entries: Vec<HistoryEntry> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_default_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), Settings::default());
        let settings = Settings {
            cleaner_selection: Some(vec!["windows.user_temp".into()]),
            exclusions: vec![PathBuf::from(r"C:\Keep")],
            schedule: Some(Schedule {
                frequency: Frequency::Weekly,
                time: "10:00".into(),
            }),
            clean_on_browser_close: true,
        };
        save(dir.path(), &settings).unwrap();
        assert_eq!(load(dir.path()), settings);
    }

    #[test]
    fn times_are_validated() {
        assert_eq!(parse_time("9:05"), Some("09:05".into()));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("noon"), None);
    }

    #[test]
    fn history_is_newest_first_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(MAX_HISTORY as u64 + 3) {
            let mut e = HistoryEntry::now("Cleaned", i, 0, None);
            e.time = i;
            append_history(dir.path(), &e).unwrap();
        }
        fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join(HISTORY))
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        let history = read_history(dir.path());
        assert_eq!(history.len(), MAX_HISTORY);
        assert_eq!(history[0].items, MAX_HISTORY as u64 + 2);
    }
}
