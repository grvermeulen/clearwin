//! Registry cleanup, limited to entries that clearly point at files that no
//! longer exist. Every fix is preceded by a `.reg` backup that can be restored.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistryIssue {
    pub id: u32,
    /// What kind of entry this is, in words.
    pub kind: String,
    /// Full key path, e.g. `HKEY_CURRENT_USER\Software\...`.
    pub key: String,
    /// The value to delete, or `None` to delete the whole key.
    pub value: Option<String>,
    /// The file the entry points at, which no longer exists.
    pub missing_path: String,
    pub needs_admin: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixReport {
    pub fixed: u32,
    pub failures: Vec<String>,
    pub backup: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: PathBuf,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub issues: Vec<RegistryIssue>,
}

const MANIFEST: &str = "backup.json";

/// Keys to export before fixing: the key that holds each value, or the key itself.
pub fn keys_to_back_up(issues: &[RegistryIssue]) -> Vec<String> {
    issues
        .iter()
        .map(|i| i.key.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Backups in `root`, newest first.
pub fn list_backups(root: &Path) -> Vec<BackupInfo> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut backups: Vec<BackupInfo> = entries
        .flatten()
        .filter_map(|e| {
            let text = fs::read_to_string(e.path().join(MANIFEST)).ok()?;
            let mut info: BackupInfo = serde_json::from_str(&text).ok()?;
            info.path = e.path();
            Some(info)
        })
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.created));
    backups
}

#[cfg_attr(not(windows), allow(dead_code))]
fn new_backup_folder(root: &Path) -> std::io::Result<(PathBuf, u64)> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut folder = root.join(format!("backup-{now}"));
    let mut n = 1;
    while folder.exists() {
        n += 1;
        folder = root.join(format!("backup-{now}-{n}"));
    }
    fs::create_dir_all(&folder)?;
    Ok((folder, now))
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ, KEY_SET_VALUE,
    };
    use winreg::RegKey;

    use super::*;
    use crate::cmdline::{is_missing, is_windows_absolute, target_path};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const HKLM: &str = "HKEY_LOCAL_MACHINE";
    const HKCU: &str = "HKEY_CURRENT_USER";

    fn open(full: &str, flags: u32) -> Option<RegKey> {
        let (hive, path) = full.split_once('\\')?;
        let root = match hive {
            HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
            HKCU => RegKey::predef(HKEY_CURRENT_USER),
            _ => return None,
        };
        root.open_subkey_with_flags(path, flags).ok()
    }

    struct Scan {
        issues: Vec<RegistryIssue>,
    }

    impl Scan {
        fn push(&mut self, kind: &str, key: &str, value: Option<String>, missing: &Path) {
            self.issues.push(RegistryIssue {
                id: self.issues.len() as u32,
                kind: kind.to_string(),
                key: key.to_string(),
                value,
                missing_path: missing.display().to_string(),
                needs_admin: key.starts_with(HKLM),
            });
        }

        /// Values whose names are file paths (SharedDLLs).
        fn path_named_values(&mut self, kind: &str, key: &str) {
            let Some(k) = open(key, KEY_READ) else { return };
            for (name, _) in k.enum_values().flatten() {
                if is_windows_absolute(&name) && is_missing(Path::new(&name)) {
                    self.push(kind, key, Some(name.clone()), Path::new(&name));
                }
            }
        }

        /// Values whose data is a command line (Run keys).
        fn command_values(&mut self, kind: &str, key: &str) {
            let Some(k) = open(key, KEY_READ) else { return };
            for (name, data) in k.enum_values().flatten() {
                if let Some(path) = target_path(&data.to_string()).filter(|p| is_missing(p)) {
                    self.push(kind, key, Some(name), &path);
                }
            }
        }

        /// Subkeys whose default value is a program path (App Paths).
        fn app_paths(&mut self, key: &str) {
            let Some(k) = open(key, KEY_READ) else { return };
            for name in k.enum_keys().flatten() {
                let Ok(sub) = k.open_subkey_with_flags(&name, KEY_READ) else {
                    continue;
                };
                let Ok(default) = sub.get_value::<String, _>("") else {
                    continue;
                };
                if let Some(path) = target_path(&default).filter(|p| is_missing(p)) {
                    self.push(
                        "Program shortcut (App Paths)",
                        &format!(r"{key}\{name}"),
                        None,
                        &path,
                    );
                }
            }
        }

        /// MuiCache value names are `<path>.FriendlyAppName` or `<path>.ApplicationCompany`.
        fn mui_cache(&mut self, key: &str) {
            let Some(k) = open(key, KEY_READ) else { return };
            for (name, _) in k.enum_values().flatten() {
                let path = [".FriendlyAppName", ".ApplicationCompany"]
                    .iter()
                    .find_map(|suffix| name.strip_suffix(suffix))
                    .unwrap_or(&name);
                if is_windows_absolute(path) && is_missing(Path::new(path)) {
                    self.push(
                        "Program name cache (MuiCache)",
                        key,
                        Some(name.clone()),
                        Path::new(path),
                    );
                }
            }
        }

        /// Uninstall entries whose uninstaller and install folder are both gone.
        fn uninstall_entries(&mut self, key: &str) {
            let Some(k) = open(key, KEY_READ) else { return };
            for name in k.enum_keys().flatten() {
                let Ok(sub) = k.open_subkey_with_flags(&name, KEY_READ) else {
                    continue;
                };
                let Ok(command) = sub.get_value::<String, _>("UninstallString") else {
                    continue;
                };
                let Some(uninstaller) = target_path(&command).filter(|p| is_missing(p)) else {
                    continue;
                };
                let location = sub
                    .get_value::<String, _>("InstallLocation")
                    .unwrap_or_default();
                let location = location.trim().trim_matches('"');
                if !location.is_empty() && !is_missing(Path::new(location)) {
                    continue;
                }
                self.push(
                    "Leftover uninstall entry",
                    &format!(r"{key}\{name}"),
                    None,
                    &uninstaller,
                );
            }
        }
    }

    pub fn scan() -> Vec<RegistryIssue> {
        let mut s = Scan { issues: Vec::new() };
        for key in [
            r"HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\SharedDLLs",
            r"HKEY_LOCAL_MACHINE\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\SharedDLLs",
        ] {
            s.path_named_values("Missing shared file", key);
        }
        for key in [
            r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run",
            r"HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Run",
            r"HKEY_LOCAL_MACHINE\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        ] {
            s.command_values("Startup entry for a missing program", key);
        }
        for key in [
            r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\App Paths",
            r"HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\App Paths",
        ] {
            s.app_paths(key);
        }
        s.mui_cache(r"HKEY_CURRENT_USER\Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache");
        for key in [
            r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKEY_LOCAL_MACHINE\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            s.uninstall_entries(key);
        }
        s.issues
    }

    fn reg(args: &[&str]) -> Result<(), String> {
        let output = Command::new("reg.exe")
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

    fn delete(issue: &RegistryIssue) -> Result<(), String> {
        match &issue.value {
            Some(value) => {
                let key = open(&issue.key, KEY_SET_VALUE).ok_or("Could not open the key.")?;
                key.delete_value(value)
                    .map_err(|e| crate::admin_hint(e, issue.needs_admin))
            }
            None => {
                let (parent, name) = issue.key.rsplit_once('\\').ok_or("Bad key path.")?;
                let parent = open(parent, KEY_ALL_ACCESS).ok_or_else(|| {
                    crate::admin_hint(
                        std::io::ErrorKind::PermissionDenied.into(),
                        issue.needs_admin,
                    )
                })?;
                parent
                    .delete_subkey_all(name)
                    .map_err(|e| crate::admin_hint(e, issue.needs_admin))
            }
        }
    }

    pub fn fix(issues: &[RegistryIssue], backup_root: &Path) -> FixReport {
        let mut report = FixReport::default();
        // An issue that was fixed elsewhere since the scan is skipped.
        let issues: Vec<&RegistryIssue> = issues
            .iter()
            .filter(|i| is_missing(Path::new(&i.missing_path)))
            .collect();
        if issues.is_empty() {
            return report;
        }
        let (folder, created) = match new_backup_folder(backup_root) {
            Ok(f) => f,
            Err(e) => {
                report
                    .failures
                    .push(format!("Could not create a backup folder: {e}"));
                return report;
            }
        };
        let owned: Vec<RegistryIssue> = issues.iter().map(|i| (*i).clone()).collect();
        let mut backed_up = BTreeSet::new();
        for (n, key) in keys_to_back_up(&owned).iter().enumerate() {
            let file = folder.join(format!("{n:03}.reg"));
            match reg(&["export", key, &file.to_string_lossy(), "/y"]) {
                Ok(()) => {
                    backed_up.insert(key.clone());
                }
                Err(e) => report
                    .failures
                    .push(format!("{key}: backup failed, left unchanged ({e})")),
            }
        }
        let fixed: Vec<RegistryIssue> = owned
            .into_iter()
            .filter(|i| backed_up.contains(&i.key))
            .collect();
        let manifest = BackupInfo {
            path: folder.clone(),
            created,
            issues: fixed.clone(),
        };
        if let Err(e) = serde_json::to_string_pretty(&manifest)
            .map_err(std::io::Error::other)
            .and_then(|json| fs::write(folder.join(MANIFEST), json))
        {
            report.failures.push(format!(
                "Could not save the backup list, nothing was changed: {e}"
            ));
            return report;
        }
        for issue in &fixed {
            match delete(issue) {
                Ok(()) => report.fixed += 1,
                Err(e) => report.failures.push(format!("{}: {e}", issue.key)),
            }
        }
        report.backup = Some(folder);
        report
    }

    pub fn restore(backup: &Path) -> Result<u32, String> {
        let mut files: Vec<PathBuf> = fs::read_dir(backup)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("reg")))
            .collect();
        files.sort();
        for file in &files {
            reg(&["import", &file.to_string_lossy()]).map_err(|e| {
                format!(
                    "{}: {e}. Restoring all-users entries needs administrator rights.",
                    file.display()
                )
            })?;
        }
        Ok(files.len() as u32)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn scan() -> Vec<RegistryIssue> {
        Vec::new()
    }

    pub fn fix(_issues: &[RegistryIssue], _backup_root: &Path) -> FixReport {
        FixReport {
            failures: vec!["Only available on Windows.".into()],
            ..FixReport::default()
        }
    }

    pub fn restore(_backup: &Path) -> Result<u32, String> {
        Err("Only available on Windows.".into())
    }
}

pub use imp::{fix, restore, scan};

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: u32, key: &str, value: Option<&str>) -> RegistryIssue {
        RegistryIssue {
            id,
            kind: "test".into(),
            key: key.into(),
            value: value.map(Into::into),
            missing_path: r"C:\gone.exe".into(),
            needs_admin: false,
        }
    }

    #[test]
    fn each_key_is_backed_up_once() {
        let issues = [
            issue(0, r"HKCU\A", Some("x")),
            issue(1, r"HKCU\A", Some("y")),
            issue(2, r"HKCU\B", None),
        ];
        assert_eq!(
            keys_to_back_up(&issues),
            vec![r"HKCU\A".to_string(), r"HKCU\B".to_string()]
        );
    }

    #[test]
    fn backups_are_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        for created in [100u64, 300, 200] {
            let (folder, _) = new_backup_folder(dir.path()).unwrap();
            let info = BackupInfo {
                path: PathBuf::new(),
                created,
                issues: vec![issue(0, "k", None)],
            };
            fs::write(folder.join(MANIFEST), serde_json::to_string(&info).unwrap()).unwrap();
        }
        fs::create_dir(dir.path().join("not-a-backup")).unwrap();

        let created: Vec<u64> = list_backups(dir.path()).iter().map(|b| b.created).collect();
        assert_eq!(created, vec![300, 200, 100]);
    }
}
