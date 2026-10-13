//! Speeding the PC up: the Windows power plan, and finding programs that use the most memory.

use std::collections::HashMap;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PowerPlan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

/// Reads `powercfg /list`. Lines look like
/// `Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *`,
/// with the label translated, so only the GUID, the name in brackets and the
/// star marking the active plan are used.
pub fn parse_power_plans(output: &str) -> Vec<PowerPlan> {
    output
        .lines()
        .filter_map(|line| {
            let guid = line.split_whitespace().find(|w| is_guid(w))?;
            let open = line.find('(')?;
            let close = line.rfind(')')?;
            (close > open).then(|| PowerPlan {
                guid: guid.to_lowercase(),
                name: line[open + 1..close].trim().to_string(),
                active: line.trim_end().ends_with('*'),
            })
        })
        .collect()
}

fn is_guid(word: &str) -> bool {
    let parts: Vec<&str> = word.split('-').collect();
    parts.len() == 5
        && parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts
            .iter()
            .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(windows)]
fn powercfg(args: &[&str]) -> Result<String, String> {
    let output = crate::ps::hidden("powercfg.exe")
        .args(args)
        .output()
        .map_err(|e| format!("Could not start powercfg: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(text)
    } else {
        Err(text.trim().to_string())
    }
}

#[cfg(windows)]
pub fn power_plans() -> Result<Vec<PowerPlan>, String> {
    Ok(parse_power_plans(&powercfg(&["/list"])?))
}

#[cfg(windows)]
pub fn set_power_plan(guid: &str) -> Result<(), String> {
    if !is_guid(guid) {
        return Err("Unknown power plan.".into());
    }
    powercfg(&["/setactive", guid]).map(|_| ())
}

#[cfg(not(windows))]
pub fn power_plans() -> Result<Vec<PowerPlan>, String> {
    Err("Only available on Windows.".into())
}

#[cfg(not(windows))]
pub fn set_power_plan(_guid: &str) -> Result<(), String> {
    Err("Only available on Windows.".into())
}

/// Windows itself, security software and ClearWin: never offered for closing.
const PROTECTED: &[&str] = &[
    "system",
    "registry",
    "idle",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "dwm.exe",
    "explorer.exe",
    "fontdrvhost.exe",
    "sihost.exe",
    "ctfmon.exe",
    "taskhostw.exe",
    "runtimebroker.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "audiodg.exe",
    "spoolsv.exe",
    "conhost.exe",
    "memory compression",
    "msmpeng.exe",
    "nissrv.exe",
    "securityhealthservice.exe",
    "securityhealthsystray.exe",
    "mpdefendercoreservice.exe",
    "smartscreen.exe",
    "wmiprvse.exe",
    "dllhost.exe",
    "clearwin.exe",
];

pub fn is_protected(name: &str) -> bool {
    PROTECTED.contains(&name.to_lowercase().as_str())
}

/// A program (all its processes together) and how much it uses.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeavyApp {
    /// Process name, e.g. `chrome.exe`; also the key for closing it.
    pub name: String,
    pub processes: u32,
    pub memory_bytes: u64,
    /// Percent of one core, summed over its processes.
    pub cpu_percent: f32,
}

/// Adds up processes by name and returns the heaviest by memory, leaving out protected ones.
pub fn group_processes(
    processes: impl IntoIterator<Item = (String, u64, f32)>,
    limit: usize,
) -> Vec<HeavyApp> {
    let mut by_name: HashMap<String, HeavyApp> = HashMap::new();
    for (name, memory, cpu) in processes {
        if name.is_empty() || is_protected(&name) {
            continue;
        }
        let app = by_name
            .entry(name.to_lowercase())
            .or_insert_with(|| HeavyApp {
                name: name.clone(),
                processes: 0,
                memory_bytes: 0,
                cpu_percent: 0.0,
            });
        app.processes += 1;
        app.memory_bytes += memory;
        app.cpu_percent += cpu;
    }
    let mut apps: Vec<_> = by_name.into_values().collect();
    apps.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes));
    apps.truncate(limit);
    apps
}

fn process_refresh() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu()
        .with_exe(UpdateKind::Never)
}

/// The programs using the most memory right now, measured over about a second.
pub fn heavy_apps(limit: usize) -> Vec<HeavyApp> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh());
    // CPU use is measured between two refreshes.
    std::thread::sleep(
        sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(800)),
    );
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, process_refresh());
    let own = std::process::id();
    group_processes(
        system
            .processes()
            .values()
            .filter(|p| p.pid() != Pid::from_u32(own))
            .map(|p| {
                (
                    p.name().to_string_lossy().into_owned(),
                    p.memory(),
                    p.cpu_usage(),
                )
            }),
        limit,
    )
}

/// Closes every process with this name, like "End task" in Task Manager.
/// Unsaved work in that program is lost.
pub fn end_app(name: &str) -> Result<u32, String> {
    if is_protected(name) {
        return Err(format!(
            "{name} is part of Windows and can't be closed here."
        ));
    }
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut closed = 0;
    for process in system.processes().values() {
        if process.name().to_string_lossy().eq_ignore_ascii_case(name) && process.kill() {
            closed += 1;
        }
    }
    if closed == 0 {
        return Err(format!(
            "{name} could not be closed. It may need administrator rights."
        ));
    }
    Ok(closed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_power_plans_in_any_language() {
        let english = "Existing Power Schemes (* Active)
-----------------------------------
Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *
Power Scheme GUID: 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c  (High performance)
";
        let plans = parse_power_plans(english);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].name, "Balanced");
        assert!(plans[0].active);
        assert!(!plans[1].active);

        let dutch =
            "GUID van energieschema: A1841308-3541-4FAB-BC81-F71556F20B4A  (Energiebesparing)";
        let plans = parse_power_plans(dutch);
        assert_eq!(plans[0].guid, "a1841308-3541-4fab-bc81-f71556f20b4a");
        assert_eq!(plans[0].name, "Energiebesparing");
    }

    #[test]
    fn groups_by_name_and_skips_windows() {
        let apps = group_processes(
            [
                ("chrome.exe".to_string(), 300, 1.0),
                ("Chrome.exe".to_string(), 200, 2.0),
                ("svchost.exe".to_string(), 9000, 0.0),
                ("Teams.exe".to_string(), 400, 0.5),
            ],
            10,
        );
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].memory_bytes, 500);
        assert_eq!(apps[0].processes, 2);
        assert_eq!(apps[1].name, "Teams.exe");
        assert!(end_app("explorer.exe").is_err());
    }
}
