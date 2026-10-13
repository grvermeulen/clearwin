use clearwin_core::defender::{self, DefenderStatus, ScanKind};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreatView {
    name: String,
    severity: &'static str,
    status: &'static str,
    detected: Option<i64>,
    resources: Vec<String>,
    /// Defender dealt with it (quarantined, removed or blocked).
    handled: bool,
}

pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn defender_status() -> Result<DefenderStatus, String> {
    blocking(defender::status).await
}

#[tauri::command]
pub async fn defender_threats() -> Result<Vec<ThreatView>, String> {
    let threats = blocking(defender::threats).await?;
    Ok(threats
        .into_iter()
        .map(|t| ThreatView {
            severity: defender::severity_name(t.severity),
            status: defender::threat_status(t.status_id),
            handled: t.action_success && matches!(t.status_id, 2 | 3 | 4 | 6),
            // Defender prefixes resources with their type, e.g. "file:_C:\x.exe".
            resources: t
                .resources
                .iter()
                .map(|r| {
                    r.split_once(":_")
                        .map_or(r.as_str(), |(_, p)| p)
                        .to_string()
                })
                .collect(),
            name: t.name,
            detected: t.detected,
        })
        .collect())
}

#[tauri::command]
pub async fn defender_scan(kind: ScanKind, path: Option<String>) -> Result<(), String> {
    blocking(move || defender::scan(kind, path.as_deref())).await
}

#[tauri::command]
pub async fn defender_update() -> Result<(), String> {
    blocking(defender::update_signatures).await
}

#[tauri::command]
pub fn open_windows_security() -> Result<(), String> {
    defender::open_windows_security()
}
