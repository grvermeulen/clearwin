//! Extra facts about installed programs that take longer to collect: the size of
//! the install folder when the program didn't report one, and when it was last used.
//!
//! Windows has no single "last opened" date per program. Two places record when
//! a program file was started, and the latest of the two is used:
//! - UserAssist (in the user's registry): programs started from the Start menu,
//!   taskbar, desktop or Explorer.
//! - Prefetch (`C:\Windows\Prefetch`): any program start, but only readable as administrator.

use std::collections::HashMap;
use std::path::Path;

use rayon::prelude::*;
use serde::Serialize;
use walkdir::WalkDir;

use crate::uninstall::InstalledApp;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppDetails {
    pub id: String,
    /// Size of the install folder; only set when the program reported no size itself.
    pub measured_bytes: Option<u64>,
    /// Seconds since the Unix epoch.
    pub last_used: Option<i64>,
}

/// UserAssist value names are ROT13 of the program path.
pub fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

/// UserAssist writes well-known folders as GUIDs; this puts the real folder back.
pub fn expand_known_folder(path: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    const FOLDERS: &[(&str, &str)] = &[
        ("{6D809377-6AF0-444B-8957-A3773F02200E}", "ProgramW6432"),
        (
            "{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}",
            "ProgramFiles(x86)",
        ),
        ("{905e63b6-c1bf-494e-b29c-65b732d3d21a}", "ProgramFiles"),
        ("{F38BF404-1D43-42F2-9305-67DE0B28FC23}", "SystemRoot"),
        ("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}", "SystemRoot"),
    ];
    for (guid, var) in FOLDERS {
        if path.len() >= guid.len() && path[..guid.len()].eq_ignore_ascii_case(guid) {
            if let Some(base) = lookup(var) {
                let rest = &path[guid.len()..];
                let base = if guid.starts_with("{1AC14E77") {
                    format!("{base}\\System32")
                } else {
                    base
                };
                return format!("{base}{rest}");
            }
        }
    }
    path.to_string()
}

/// Windows FILETIME (100 ns since 1601) to Unix seconds.
pub fn filetime_to_unix(ft: u64) -> Option<i64> {
    const EPOCH_DIFF: i64 = 11_644_473_600;
    (ft > 0).then(|| (ft / 10_000_000) as i64 - EPOCH_DIFF)
}

/// Last-run time stored in a UserAssist value (72 bytes since Windows 7).
pub fn userassist_last_run(data: &[u8]) -> Option<i64> {
    let bytes: [u8; 8] = data.get(60..68)?.try_into().ok()?;
    filetime_to_unix(u64::from_le_bytes(bytes))
}

/// Latest start of any program file inside `dir`, from `(lowercase path, time)` pairs.
pub fn last_run_in(dir: &str, runs: &[(String, i64)]) -> Option<i64> {
    let prefix = format!("{}\\", dir.to_lowercase().trim_end_matches('\\'));
    runs.iter()
        .filter(|(path, _)| path.starts_with(&prefix))
        .map(|(_, t)| *t)
        .max()
}

/// Prefetch files are named like `CHROME.EXE-1A2B3C4D.pf`; this gives `chrome.exe`.
pub fn prefetch_exe_name(file_name: &str) -> Option<String> {
    let stem = file_name
        .strip_suffix(".pf")
        .or_else(|| file_name.strip_suffix(".PF"))?;
    let (exe, hash) = stem.rsplit_once('-')?;
    (hash.len() == 8 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| exe.to_lowercase())
}

#[cfg(windows)]
fn userassist_runs() -> Vec<(String, i64)> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let mut runs = Vec::new();
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist")
    else {
        return runs;
    };
    for guid in root.enum_keys().flatten() {
        let Ok(count) = root.open_subkey(format!(r"{guid}\Count")) else {
            continue;
        };
        for (name, value) in count.enum_values().flatten() {
            if let Some(time) = userassist_last_run(&value.bytes) {
                let path = expand_known_folder(&rot13(&name), |v| std::env::var(v).ok());
                runs.push((path.to_lowercase(), time));
            }
        }
    }
    runs
}

#[cfg(not(windows))]
fn userassist_runs() -> Vec<(String, i64)> {
    Vec::new()
}

/// Last run per program file name, from the Prefetch folder (empty unless administrator).
fn prefetch_runs() -> HashMap<String, i64> {
    let mut runs: HashMap<String, i64> = HashMap::new();
    let Some(windows) = std::env::var_os("SystemRoot") else {
        return runs;
    };
    let Ok(entries) = std::fs::read_dir(Path::new(&windows).join("Prefetch")) else {
        return runs;
    };
    for entry in entries.flatten() {
        let Some(exe) = prefetch_exe_name(&entry.file_name().to_string_lossy()) else {
            continue;
        };
        // Windows rewrites the Prefetch file each time the program starts.
        let modified = entry.metadata().and_then(|m| m.modified());
        if let Some(t) = crate::storage::unix_seconds(modified) {
            let best = runs.entry(exe).or_insert(t as i64);
            *best = (*best).max(t as i64);
        }
    }
    runs
}

/// Total size of the folder and the program files (`.exe`) in it, lower-cased.
fn scan_folder(dir: &Path) -> (u64, Vec<String>) {
    let mut size = 0;
    let mut exes = Vec::new();
    for entry in WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            size += meta.len();
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.ends_with(".exe") {
            exes.push(name);
        }
    }
    (size, exes)
}

/// Measures folders and looks up last use for every program that has a known folder.
pub fn details(apps: &[InstalledApp]) -> Vec<AppDetails> {
    let assist = userassist_runs();
    let prefetch = prefetch_runs();
    apps.par_iter()
        .filter_map(|app| {
            let dir = app.install_dir.as_deref()?;
            let path = Path::new(dir);
            if !path.is_dir() {
                return None;
            }
            let (size, exes) = scan_folder(path);
            let from_prefetch = exes.iter().filter_map(|e| prefetch.get(e)).copied().max();
            let last_used = last_run_in(dir, &assist).max(from_prefetch);
            Some(AppDetails {
                id: app.id.clone(),
                measured_bytes: app.size_bytes.is_none().then_some(size).filter(|&s| s > 0),
                last_used,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_userassist() {
        assert_eq!(
            rot13("P:\\Cebtenz Svyrf\\Mbbz.rkr"),
            "C:\\Program Files\\Zoom.exe"
        );
        let path = expand_known_folder(
            "{6D809377-6AF0-444B-8957-A3773F02200E}\\Zoom\\Zoom.exe",
            |v| (v == "ProgramW6432").then(|| "C:\\Program Files".to_string()),
        );
        assert_eq!(path, "C:\\Program Files\\Zoom\\Zoom.exe");

        let mut data = vec![0u8; 72];
        // 2024-01-01 00:00:00 UTC as FILETIME.
        data[60..68].copy_from_slice(&133_485_408_000_000_000u64.to_le_bytes());
        assert_eq!(userassist_last_run(&data), Some(1_704_067_200));
        assert_eq!(userassist_last_run(&[0u8; 16]), None);
        assert_eq!(userassist_last_run(&[0u8; 72]), None);
    }

    #[test]
    fn matches_runs_to_the_program_folder() {
        let runs = vec![
            ("c:\\program files\\zoom\\bin\\zoom.exe".to_string(), 100),
            ("c:\\program files\\zoom\\zoom.exe".to_string(), 300),
            ("c:\\program files\\zoomit\\zoomit.exe".to_string(), 900),
        ];
        assert_eq!(last_run_in("C:\\Program Files\\Zoom", &runs), Some(300));
        assert_eq!(last_run_in("C:\\Program Files\\Other", &runs), None);
    }

    #[test]
    fn reads_prefetch_names() {
        assert_eq!(
            prefetch_exe_name("CHROME.EXE-1A2B3C4D.pf").as_deref(),
            Some("chrome.exe")
        );
        assert_eq!(
            prefetch_exe_name("R5APEX-DX12.EXE-0099AABB.pf").as_deref(),
            Some("r5apex-dx12.exe")
        );
        assert_eq!(prefetch_exe_name("Layout.ini"), None);
    }

    #[test]
    fn measures_folders_without_a_reported_size() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("game.exe"), vec![0u8; 3000]).unwrap();
        let app = InstalledApp {
            id: "k".into(),
            name: "Game".into(),
            publisher: None,
            version: None,
            install_date: None,
            size_bytes: None,
            install_dir: Some(dir.path().to_string_lossy().into_owned()),
            uninstall_command: String::new(),
        };
        let found = details(&[app]);
        assert_eq!(found[0].measured_bytes, Some(3000));
    }
}
