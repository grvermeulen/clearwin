use clearwin_core::drivers::{self, DriverScan, InstallResult};
use clearwin_core::software::{self, SoftwareUpdate};

use crate::security::blocking;

#[tauri::command]
pub async fn list_software_updates() -> Result<Vec<SoftwareUpdate>, String> {
    blocking(software::list).await
}

/// Updates one program, or every program when `id` is empty.
#[tauri::command]
pub async fn update_software(id: Option<String>) -> Result<(), String> {
    blocking(move || software::upgrade(id.as_deref())).await?;
    crate::settings::record("Programs updated", 1, 0, None);
    Ok(())
}

#[tauri::command]
pub async fn scan_drivers() -> Result<DriverScan, String> {
    blocking(drivers::scan).await
}

#[tauri::command]
pub async fn install_drivers(ids: Vec<String>) -> Result<InstallResult, String> {
    if !crate::elevation::is_admin() {
        return Err("Installing drivers needs administrator rights. Restart ClearWin as administrator first.".into());
    }
    let result = blocking(move || drivers::install(&ids)).await?;
    crate::settings::record("Drivers installed", result.installed, 0, None);
    Ok(result)
}
