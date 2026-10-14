//! Driver updates from Windows Update, and devices that are not working.
//!
//! Drivers come only from Microsoft's own Windows Update catalogue (the same
//! source as Settings > Windows Update > Optional updates), never from a
//! third-party download site.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DriverUpdate {
    /// Windows Update id, used to pick which ones to install.
    pub id: String,
    pub title: String,
    pub manufacturer: String,
    pub driver_class: String,
    /// Driver date, seconds since the Unix epoch.
    pub date: Option<i64>,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProblemDevice {
    pub name: String,
    pub class: String,
    pub status: String,
    /// Device Manager problem code, e.g. 28 for "drivers not installed".
    pub problem: i64,
    /// The problem code in words, filled in by [`parse_scan`].
    pub description: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DriverScan {
    pub updates: Vec<DriverUpdate>,
    pub problem_devices: Vec<ProblemDevice>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallResult {
    pub installed: u64,
    pub failed: u64,
    pub reboot_required: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
const SCAN_SCRIPT: &str = r#"
$session = New-Object -ComObject Microsoft.Update.Session
$found = $session.CreateUpdateSearcher().Search("IsInstalled=0 and Type='Driver' and IsHidden=0")
$updates = @($found.Updates | ForEach-Object {
  [pscustomobject]@{
    id = [string]$_.Identity.UpdateID
    title = [string]$_.Title
    manufacturer = [string]$_.DriverManufacturer
    driverClass = [string]$_.DriverClass
    date = U $_.DriverVerDate
    sizeBytes = [int64]$_.MaxDownloadSize
  }
})
$devices = @()
try {
  $devices = @(Get-PnpDevice -PresentOnly | Where-Object { $_.Status -eq 'Error' -or $_.Status -eq 'Degraded' } | ForEach-Object {
    [pscustomobject]@{
      name = [string]$_.FriendlyName
      class = [string]$_.Class
      status = [string]$_.Status
      problem = [int]$_.Problem
    }
  })
} catch {}
ConvertTo-Json -Compress -Depth 4 -InputObject ([pscustomobject]@{ updates = $updates; problemDevices = $devices })
"#;

#[cfg_attr(not(windows), allow(dead_code))]
const INSTALL_SCRIPT: &str = r#"
$ids = $env:CLEARWIN_UPDATE_IDS -split ','
$session = New-Object -ComObject Microsoft.Update.Session
$found = $session.CreateUpdateSearcher().Search("IsInstalled=0 and Type='Driver'")
$chosen = New-Object -ComObject Microsoft.Update.UpdateColl
foreach ($u in $found.Updates) {
  if ($ids -contains $u.Identity.UpdateID) {
    if (-not $u.EulaAccepted) { $u.AcceptEula() }
    [void]$chosen.Add($u)
  }
}
if ($chosen.Count -eq 0) { throw 'These drivers are no longer offered by Windows Update.' }
$downloader = $session.CreateUpdateDownloader()
$downloader.Updates = $chosen
[void]$downloader.Download()
$installer = $session.CreateUpdateInstaller()
$installer.Updates = $chosen
$result = $installer.Install()
$ok = 0
for ($i = 0; $i -lt $chosen.Count; $i++) {
  # 2 = succeeded, 3 = succeeded with errors
  if ($result.GetUpdateResult($i).ResultCode -in 2, 3) { $ok++ }
}
ConvertTo-Json -Compress -InputObject ([pscustomobject]@{
  installed = $ok
  failed = $chosen.Count - $ok
  rebootRequired = [bool]$result.RebootRequired
})
"#;

/// Device Manager's problem codes that people actually meet, in words.
pub fn problem_text(code: i64) -> &'static str {
    match code {
        1 | 18 | 28 => "Driver missing",
        3 => "Driver damaged or out of memory",
        10 => "Device cannot start",
        12 => "Not enough free resources",
        22 => "Device is disabled",
        31 => "Driver could not be loaded",
        39 | 41 => "Driver damaged or missing",
        43 => "Windows stopped this device after it reported problems",
        45 => "Device is not connected",
        52 => "Driver is not signed",
        _ => "Device is not working properly",
    }
}

pub fn parse_scan(json: &str) -> Result<DriverScan, String> {
    let mut scan: DriverScan = serde_json::from_str(json.trim())
        .map_err(|e| format!("Unexpected answer from Windows Update: {e}"))?;
    for device in &mut scan.problem_devices {
        device.description = problem_text(device.problem).to_string();
    }
    Ok(scan)
}

pub fn parse_install(json: &str) -> Result<InstallResult, String> {
    serde_json::from_str(json.trim())
        .map_err(|e| format!("Unexpected answer from Windows Update: {e}"))
}

/// Asks Windows Update for newer drivers. This can take a minute.
pub fn scan() -> Result<DriverScan, String> {
    parse_scan(&crate::ps::run(SCAN_SCRIPT, &[])?)
}

/// Downloads and installs the chosen drivers. Needs administrator rights.
pub fn install(ids: &[String]) -> Result<InstallResult, String> {
    if ids.is_empty() {
        return Ok(InstallResult::default());
    }
    let joined = ids.join(",");
    parse_install(&crate::ps::run(
        INSTALL_SCRIPT,
        &[("CLEARWIN_UPDATE_IDS", &joined)],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scan() {
        let scan = parse_scan(
            r#"{"updates":[{"id":"a1","title":"Intel - Display - 31.0","manufacturer":"Intel","driverClass":"Display","date":1700000000,"sizeBytes":5000}],
                "problemDevices":[{"name":"Unknown device","class":"","status":"Error","problem":28}]}"#,
        )
        .unwrap();
        assert_eq!(scan.updates[0].driver_class, "Display");
        assert_eq!(scan.problem_devices[0].problem, 28);
        assert_eq!(scan.problem_devices[0].description, "Driver missing");
    }

    #[test]
    fn reads_empty_lists() {
        // PowerShell writes an empty array as [] and a missing date as null.
        let scan = parse_scan(r#"{"updates":[],"problemDevices":[]}"#).unwrap();
        assert!(scan.updates.is_empty());
        let result = parse_install(r#"{"installed":1,"failed":0,"rebootRequired":true}"#).unwrap();
        assert!(result.reboot_required);
    }
}
