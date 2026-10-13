use std::path::PathBuf;
use std::sync::Mutex;

use clearwin_core::registry::{self, BackupInfo, FixReport, RegistryIssue};
use clearwin_core::startup::{self, StartupItem};
use clearwin_core::uninstall::{self, InstalledApp};
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct ToolsState {
    /// Issues from the last registry scan. Only these can be fixed.
    registry_issues: Mutex<Vec<RegistryIssue>>,
}

fn backup_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("RegistryBackups"))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_startup() -> Result<Vec<StartupItem>, String> {
    blocking(startup::list).await
}

#[tauri::command]
pub fn set_startup_enabled(id: String, enabled: bool) -> Result<(), String> {
    startup::set_enabled(&id, enabled)
}

#[tauri::command]
pub async fn list_apps() -> Result<Vec<InstalledApp>, String> {
    blocking(uninstall::list).await
}

#[tauri::command]
pub fn uninstall_app(id: String) -> Result<(), String> {
    uninstall::uninstall(&id)
}

#[tauri::command]
pub async fn scan_registry(state: State<'_, ToolsState>) -> Result<Vec<RegistryIssue>, String> {
    let issues = blocking(registry::scan).await?;
    *state.registry_issues.lock().unwrap() = issues.clone();
    Ok(issues)
}

#[tauri::command]
pub async fn fix_registry(
    app: AppHandle,
    state: State<'_, ToolsState>,
    ids: Vec<u32>,
) -> Result<FixReport, String> {
    let chosen: Vec<RegistryIssue> = {
        let mut issues = state.registry_issues.lock().unwrap();
        let (chosen, rest) = issues.drain(..).partition(|i| ids.contains(&i.id));
        *issues = rest;
        chosen
    };
    let root = backup_root(&app)?;
    let report = blocking(move || registry::fix(&chosen, &root)).await?;
    crate::settings::record("Registry entries fixed", report.fixed as u64, 0, None);
    Ok(report)
}

#[tauri::command]
pub fn list_registry_backups(app: AppHandle) -> Result<Vec<BackupInfo>, String> {
    Ok(registry::list_backups(&backup_root(&app)?))
}

#[tauri::command]
pub async fn restore_registry_backup(app: AppHandle, path: PathBuf) -> Result<u32, String> {
    // Only folders ClearWin created can be restored.
    let root = backup_root(&app)?;
    if path.parent() != Some(root.as_path()) {
        return Err("That is not a ClearWin backup.".into());
    }
    blocking(move || registry::restore(&path)).await?
}
