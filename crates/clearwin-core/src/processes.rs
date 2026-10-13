use std::collections::HashSet;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// Snapshot of running process names, lower-cased (e.g. `chrome.exe`).
#[derive(Clone, Debug, Default)]
pub struct RunningApps(HashSet<String>);

impl RunningApps {
    pub fn snapshot() -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
        );
        system.refresh_processes(ProcessesToUpdate::All, true);
        Self(
            system
                .processes()
                .values()
                .map(|p| p.name().to_string_lossy().to_lowercase())
                .collect(),
        )
    }

    pub fn from_names<I: IntoIterator<Item = S>, S: AsRef<str>>(names: I) -> Self {
        Self(
            names
                .into_iter()
                .map(|n| n.as_ref().to_lowercase())
                .collect(),
        )
    }

    pub fn any_running(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.0.contains(&n.to_lowercase()))
    }
}
