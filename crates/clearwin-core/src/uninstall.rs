//! Installed programs, read from the same registry keys "Apps & features" uses.

use serde::Serialize;

use crate::cmdline::split_command;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    /// Registry key the entry lives under, used to find it again.
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// `YYYY-MM-DD`, when known.
    pub install_date: Option<String>,
    pub size_bytes: Option<u64>,
    #[serde(skip)]
    pub uninstall_command: String,
}

/// Raw values of one Uninstall registry key.
#[derive(Clone, Debug, Default)]
pub struct RawEntry {
    pub key: String,
    pub display_name: Option<String>,
    pub publisher: Option<String>,
    pub display_version: Option<String>,
    pub install_date: Option<String>,
    /// In kilobytes, as Windows stores it.
    pub estimated_size_kb: Option<u32>,
    pub uninstall_string: Option<String>,
    pub system_component: bool,
    pub parent_key_name: Option<String>,
    pub release_type: Option<String>,
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Turns `20240131` into `2024-01-31`.
fn format_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    (raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{}-{}-{}", &raw[..4], &raw[4..6], &raw[6..]))
}

/// Keeps entries "Apps & features" would show: named, uninstallable, and not
/// system components or updates of another program.
pub fn visible_apps(raw: Vec<RawEntry>) -> Vec<InstalledApp> {
    let mut apps: Vec<InstalledApp> = raw
        .into_iter()
        .filter(|e| !e.system_component && non_empty(&e.parent_key_name).is_none())
        .filter(|e| {
            !matches!(
                non_empty(&e.release_type).as_deref(),
                Some("Update" | "Hotfix" | "Security Update")
            )
        })
        .filter_map(|e| {
            Some(InstalledApp {
                name: non_empty(&e.display_name)?,
                uninstall_command: non_empty(&e.uninstall_string)?,
                publisher: non_empty(&e.publisher),
                version: non_empty(&e.display_version),
                install_date: e.install_date.as_deref().and_then(format_date),
                size_bytes: e
                    .estimated_size_kb
                    .filter(|&kb| kb > 0)
                    .map(|kb| kb as u64 * 1024),
                id: e.key,
            })
        })
        .collect();
    apps.sort_by_key(|a| (a.name.to_lowercase(), a.version.clone()));
    // The same program can be registered for both 32- and 64-bit views.
    apps.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    apps
}

/// Program and arguments to run to uninstall. Windows Installer entries often
/// say `/I` (repair or change); `/X` is the actual uninstall.
pub fn uninstall_invocation(command: &str) -> Option<(String, String)> {
    let (program, args) = split_command(command)?;
    let is_msiexec = program
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .ends_with("msiexec");
    let args = if is_msiexec {
        args.replacen("/I{", "/X{", 1).replacen("/i{", "/X{", 1)
    } else {
        args
    };
    Some((program, args))
}

#[cfg(windows)]
mod imp {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    use super::*;

    const ROOTS: &[(bool, &str)] = &[
        (true, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        (
            true,
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            false,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    fn read_raw() -> Vec<RawEntry> {
        let mut out = Vec::new();
        for &(machine, path) in ROOTS {
            let hive = RegKey::predef(if machine {
                HKEY_LOCAL_MACHINE
            } else {
                HKEY_CURRENT_USER
            });
            let Ok(root) = hive.open_subkey_with_flags(path, KEY_READ) else {
                continue;
            };
            for name in root.enum_keys().flatten() {
                let Ok(key) = root.open_subkey_with_flags(&name, KEY_READ) else {
                    continue;
                };
                let s = |v: &str| key.get_value::<String, _>(v).ok();
                out.push(RawEntry {
                    key: format!(
                        r"{}\{}\{}",
                        if machine {
                            "HKEY_LOCAL_MACHINE"
                        } else {
                            "HKEY_CURRENT_USER"
                        },
                        path,
                        name
                    ),
                    display_name: s("DisplayName"),
                    publisher: s("Publisher"),
                    display_version: s("DisplayVersion"),
                    install_date: s("InstallDate"),
                    estimated_size_kb: key.get_value::<u32, _>("EstimatedSize").ok(),
                    uninstall_string: s("UninstallString"),
                    system_component: key.get_value::<u32, _>("SystemComponent").ok() == Some(1),
                    parent_key_name: s("ParentKeyName"),
                    release_type: s("ReleaseType"),
                });
            }
        }
        out
    }

    pub fn list() -> Vec<InstalledApp> {
        visible_apps(read_raw())
    }

    /// Starts the program's own uninstaller. Windows asks for permission when it needs it.
    pub fn uninstall(id: &str) -> Result<(), String> {
        let app = list()
            .into_iter()
            .find(|a| a.id == id)
            .ok_or("That program is no longer installed.")?;
        let (program, args) = uninstall_invocation(&app.uninstall_command)
            .ok_or("The uninstall command could not be read.")?;
        crate::launch(&program, &args)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::InstalledApp;

    pub fn list() -> Vec<InstalledApp> {
        Vec::new()
    }

    pub fn uninstall(_id: &str) -> Result<(), String> {
        Err("Only available on Windows.".into())
    }
}

pub use imp::{list, uninstall};

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> RawEntry {
        RawEntry {
            key: format!("k\\{name}"),
            display_name: Some(name.into()),
            uninstall_string: Some(r#""C:\x\unins000.exe""#.into()),
            ..RawEntry::default()
        }
    }

    #[test]
    fn hides_components_updates_and_unnamed_entries() {
        let mut hidden_component = entry("Runtime");
        hidden_component.system_component = true;
        let mut update = entry("KB123");
        update.release_type = Some("Update".into());
        let mut child = entry("Plugin");
        child.parent_key_name = Some("Parent".into());
        let mut no_uninstaller = entry("Broken");
        no_uninstaller.uninstall_string = None;
        let mut app = entry("Zoom");
        app.install_date = Some("20240131".into());
        app.estimated_size_kb = Some(2);

        let apps = visible_apps(vec![
            hidden_component,
            update,
            child,
            no_uninstaller,
            app,
            entry("7-Zip"),
        ]);

        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["7-Zip", "Zoom"]);
        assert_eq!(apps[1].install_date.as_deref(), Some("2024-01-31"));
        assert_eq!(apps[1].size_bytes, Some(2048));
    }

    #[test]
    fn msi_repair_becomes_uninstall() {
        assert_eq!(
            uninstall_invocation("MsiExec.exe /I{ABC-123}"),
            Some(("MsiExec.exe".into(), "/X{ABC-123}".into()))
        );
        assert_eq!(
            uninstall_invocation(r#""C:\App\unins000.exe" /SILENT"#),
            Some((r"C:\App\unins000.exe".into(), "/SILENT".into()))
        );
    }
}
