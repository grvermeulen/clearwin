//! Programs that start with Windows.
//!
//! Turning an entry off uses the same switch as Task Manager's Startup tab
//! (the `StartupApproved` registry values), so it is reversible and Task
//! Manager shows the same state.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// Stable id: `<source>|<name>`.
    pub id: String,
    pub name: String,
    pub command: String,
    /// Where the entry lives, in words.
    pub location: String,
    pub enabled: bool,
    /// The program the entry runs no longer exists.
    pub target_missing: bool,
    /// Changing it needs administrator rights.
    pub needs_admin: bool,
}

/// Task Manager stores "enabled" as an even first byte and "disabled" as odd.
/// A missing value means enabled.
pub fn approved_is_enabled(bytes: Option<&[u8]>) -> bool {
    match bytes.and_then(|b| b.first()) {
        Some(first) => first % 2 == 0,
        None => true,
    }
}

/// The 12-byte value Task Manager writes: a state byte, padding, then the
/// FILETIME the entry was disabled (zero when enabled).
pub fn approved_bytes(enabled: bool, filetime: u64) -> Vec<u8> {
    let mut bytes = vec![0u8; 12];
    if enabled {
        bytes[0] = 0x02;
    } else {
        bytes[0] = 0x03;
        bytes[4..].copy_from_slice(&filetime.to_le_bytes());
    }
    bytes
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE};
    use winreg::{RegKey, RegValue};

    use super::*;
    use crate::cmdline::{is_missing, target_path};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    struct Source {
        key: &'static str,
        machine: bool,
        kind: Kind,
        approved: &'static str,
        location: &'static str,
    }

    enum Kind {
        Registry(&'static str),
        Folder,
    }

    const SOURCES: &[Source] = &[
        Source {
            key: "user-run",
            machine: false,
            kind: Kind::Registry(RUN),
            approved: "Run",
            location: "Registry, this user",
        },
        Source {
            key: "machine-run",
            machine: true,
            kind: Kind::Registry(RUN),
            approved: "Run",
            location: "Registry, all users",
        },
        Source {
            key: "machine-run32",
            machine: true,
            kind: Kind::Registry(RUN32),
            approved: "Run32",
            location: "Registry, all users (32-bit)",
        },
        Source {
            key: "user-folder",
            machine: false,
            kind: Kind::Folder,
            approved: "StartupFolder",
            location: "Startup folder, this user",
        },
        Source {
            key: "machine-folder",
            machine: true,
            kind: Kind::Folder,
            approved: "StartupFolder",
            location: "Startup folder, all users",
        },
    ];

    fn hive(machine: bool) -> RegKey {
        RegKey::predef(if machine {
            HKEY_LOCAL_MACHINE
        } else {
            HKEY_CURRENT_USER
        })
    }

    fn startup_folder(machine: bool) -> Option<PathBuf> {
        let base = std::env::var_os(if machine { "ProgramData" } else { "APPDATA" })?;
        Some(PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
    }

    fn approved(source: &Source, name: &str) -> bool {
        let value = hive(source.machine)
            .open_subkey_with_flags(format!(r"{APPROVED}\{}", source.approved), KEY_READ)
            .ok()
            .and_then(|k| k.get_raw_value(name).ok());
        approved_is_enabled(value.as_ref().map(|v| &v.bytes[..]))
    }

    fn entries(source: &Source) -> Vec<(String, String)> {
        match source.kind {
            Kind::Registry(path) => {
                let Ok(key) = hive(source.machine).open_subkey_with_flags(path, KEY_READ) else {
                    return Vec::new();
                };
                key.enum_values()
                    .flatten()
                    .filter(|(name, _)| !name.is_empty())
                    .map(|(name, value)| (name, value.to_string()))
                    .collect()
            }
            Kind::Folder => {
                let Some(dir) = startup_folder(source.machine) else {
                    return Vec::new();
                };
                let Ok(read) = std::fs::read_dir(&dir) else {
                    return Vec::new();
                };
                read.flatten()
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.eq_ignore_ascii_case("desktop.ini"))
                    .map(|n| (n.clone(), dir.join(&n).display().to_string()))
                    .collect()
            }
        }
    }

    pub fn list() -> Vec<StartupItem> {
        let mut items = Vec::new();
        for source in SOURCES {
            for (name, command) in entries(source) {
                let target_missing = matches!(source.kind, Kind::Registry(_))
                    && target_path(&command).is_some_and(|p| is_missing(&p));
                items.push(StartupItem {
                    id: format!("{}|{}", source.key, name),
                    enabled: approved(source, &name),
                    location: source.location.to_string(),
                    needs_admin: source.machine,
                    target_missing,
                    name: name.trim_end_matches(".lnk").to_string(),
                    command,
                });
            }
        }
        items.sort_by_key(|i| i.name.to_lowercase());
        items
    }

    pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
        let (source_key, name) = id.split_once('|').ok_or("Unknown startup entry.")?;
        let source = SOURCES
            .iter()
            .find(|s| s.key == source_key)
            .ok_or("Unknown startup entry.")?;
        if !entries(source).iter().any(|(n, _)| n == name) {
            return Err("That startup entry no longer exists.".into());
        }
        let (key, _) = hive(source.machine)
            .create_subkey_with_flags(format!(r"{APPROVED}\{}", source.approved), KEY_SET_VALUE)
            .map_err(|e| crate::admin_hint(e, source.machine))?;
        // FILETIME counts 100 ns steps since 1601.
        let filetime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_nanos() / 100) as u64 + 116_444_736_000_000_000)
            .unwrap_or(0);
        let value = RegValue {
            bytes: approved_bytes(enabled, filetime).into(),
            vtype: winreg::enums::REG_BINARY,
        };
        key.set_raw_value(name, &value)
            .map_err(|e| crate::admin_hint(e, source.machine))
    }
}

#[cfg(not(windows))]
mod imp {
    use super::StartupItem;

    pub fn list() -> Vec<StartupItem> {
        Vec::new()
    }

    pub fn set_enabled(_id: &str, _enabled: bool) -> Result<(), String> {
        Err("Only available on Windows.".into())
    }
}

pub use imp::{list, set_enabled};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_values_round_trip() {
        assert!(approved_is_enabled(None));
        assert!(approved_is_enabled(Some(&approved_bytes(true, 0))));
        assert!(!approved_is_enabled(Some(&approved_bytes(false, 123))));
        // Task Manager also writes 0x06 for enabled and 0x07 for disabled.
        assert!(approved_is_enabled(Some(&[0x06, 0, 0, 0])));
        assert!(!approved_is_enabled(Some(&[0x07, 0, 0, 0])));
        assert_eq!(&approved_bytes(false, 0x0102)[4..6], &[0x02, 0x01]);
    }
}
