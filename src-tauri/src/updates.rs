use clearwin_core::drivers::{self, DriverScan, InstallResult};
use clearwin_core::software::{self, Fix, SoftwareUpdate, UpdateFailure};

use crate::security::blocking;

#[tauri::command]
pub async fn list_software_updates() -> Result<Vec<SoftwareUpdate>, String> {
    blocking(software::list).await
}

/// Updates one program, or every program when `id` is empty.
#[tauri::command]
pub async fn update_software(id: Option<String>) -> Result<(), UpdateFailure> {
    blocking(move || Ok(software::upgrade(id.as_deref())))
        .await
        .map_err(UpdateFailure::from)?
        .map_err(|mut failure| {
            // Already administrator: running as administrator again won't help.
            if failure.fix == Some(Fix::RunAsAdmin) && crate::elevation::is_admin() {
                failure.fix = None;
            }
            failure
        })?;
    crate::settings::record("Programs updated", 1, 0, None);
    Ok(())
}

/// Tries the fix ClearWin offered for a failed update. Asks for administrator rights
/// unless ClearWin already has them, since every fix may need them.
#[tauri::command]
pub async fn fix_software_update(fix: Fix, update: SoftwareUpdate) -> Result<(), UpdateFailure> {
    let elevate = !crate::elevation::is_admin();
    blocking(move || Ok(software::apply_fix(fix, &update, elevate)))
        .await
        .map_err(UpdateFailure::from)??;
    crate::settings::record("Programs updated", 1, 0, None);
    Ok(())
}

/// Stops offering updates for a program that can't be updated.
#[tauri::command]
pub async fn ignore_software_update(id: String) -> Result<(), String> {
    blocking(move || software::ignore(&id)).await
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
