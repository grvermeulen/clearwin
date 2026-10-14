use clearwin_core::performance::{self, HeavyApp, PowerPlan};
use clearwin_core::system::{self, SystemInfo};

use crate::security::blocking;

#[tauri::command]
pub async fn system_info() -> Result<SystemInfo, String> {
    blocking(system::info).await
}

#[tauri::command]
pub async fn list_power_plans() -> Result<Vec<PowerPlan>, String> {
    blocking(performance::power_plans).await
}

#[tauri::command]
pub async fn set_power_plan(guid: String) -> Result<(), String> {
    blocking(move || performance::set_power_plan(&guid)).await
}

#[tauri::command]
pub async fn heavy_apps() -> Result<Vec<HeavyApp>, String> {
    blocking(|| Ok(performance::heavy_apps(15))).await
}

#[tauri::command]
pub fn end_app(name: String) -> Result<u32, String> {
    performance::end_app(&name)
}
