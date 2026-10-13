//! Microsoft Defender, driven through its PowerShell commands.
//!
//! Defender already scans continuously and keeps itself updated; ClearWin
//! shows its state and starts scans, it does not scan files itself.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DefenderStatus {
    pub service_enabled: bool,
    pub antivirus_enabled: bool,
    pub real_time_protection: bool,
    pub tamper_protection: bool,
    /// "Normal" when Defender is the active antivirus, "Passive Mode" when another one is.
    pub running_mode: String,
    pub signature_version: String,
    /// Seconds since the Unix epoch.
    pub signature_updated: Option<i64>,
    pub signature_age_days: i64,
    pub quick_scan_ended: Option<i64>,
    pub full_scan_ended: Option<i64>,
    /// Antivirus products registered with Windows Security.
    pub products: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Threat {
    pub name: String,
    pub severity: i64,
    pub detected: Option<i64>,
    pub resources: Vec<String>,
    pub status_id: i64,
    pub action_success: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanKind {
    Quick,
    Full,
    Custom,
}

/// Defender's threat status codes, in words.
pub fn threat_status(id: i64) -> &'static str {
    match id {
        1 => "Detected",
        2 => "Cleaned",
        3 => "Quarantined",
        4 => "Removed",
        5 => "Allowed",
        6 => "Blocked",
        102 => "Quarantine failed",
        103 => "Removal failed",
        104 => "Allow failed",
        105 => "Abandoned",
        107 => "Block failed",
        _ => "Unknown",
    }
}

pub fn severity_name(id: i64) -> &'static str {
    match id {
        1 => "Low",
        2 => "Moderate",
        4 => "High",
        5 => "Severe",
        _ => "Unknown",
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
const STATUS_SCRIPT: &str = r#"
$s = Get-MpComputerStatus
$products = @()
try { $products = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | ForEach-Object { [string]$_.displayName }) } catch {}
ConvertTo-Json -Compress -InputObject ([pscustomobject]@{
  serviceEnabled = [bool]$s.AMServiceEnabled
  antivirusEnabled = [bool]$s.AntivirusEnabled
  realTimeProtection = [bool]$s.RealTimeProtectionEnabled
  tamperProtection = [bool]$s.IsTamperProtected
  runningMode = [string]$s.AMRunningMode
  signatureVersion = [string]$s.AntivirusSignatureVersion
  signatureUpdated = U $s.AntivirusSignatureLastUpdated
  signatureAgeDays = [int]$s.AntivirusSignatureAge
  quickScanEnded = U $s.QuickScanEndTime
  fullScanEnded = U $s.FullScanEndTime
  products = $products
})
"#;

#[cfg_attr(not(windows), allow(dead_code))]
const THREATS_SCRIPT: &str = r#"
$names = @{}
Get-MpThreat | ForEach-Object { $names[[string]$_.ThreatID] = $_ }
$list = @(Get-MpThreatDetection | Sort-Object InitialDetectionTime -Descending | Select-Object -First 50 | ForEach-Object {
  $t = $names[[string]$_.ThreatID]
  [pscustomobject]@{
    name = if ($t) { [string]$t.ThreatName } else { 'Unknown threat' }
    severity = if ($t) { [int]$t.SeverityID } else { 0 }
    detected = U $_.InitialDetectionTime
    resources = @($_.Resources | ForEach-Object { [string]$_ })
    statusId = [int]$_.ThreatStatusID
    actionSuccess = [bool]$_.ActionSuccess
  }
})
ConvertTo-Json -Compress -Depth 4 -InputObject $list
"#;

pub fn parse_status(json: &str) -> Result<DefenderStatus, String> {
    serde_json::from_str(json.trim()).map_err(|e| format!("Unexpected answer from Defender: {e}"))
}

pub fn parse_threats(json: &str) -> Result<Vec<Threat>, String> {
    let json = json.trim();
    if json.is_empty() || json == "null" {
        return Ok(Vec::new());
    }
    serde_json::from_str(json).map_err(|e| format!("Unexpected answer from Defender: {e}"))
}

#[cfg(windows)]
mod imp {
    use super::*;

    fn powershell(script: &str, env: &[(&str, &str)]) -> Result<String, String> {
        crate::ps::run(script, env)
    }

    pub fn status() -> Result<DefenderStatus, String> {
        parse_status(&powershell(STATUS_SCRIPT, &[])?)
    }

    pub fn threats() -> Result<Vec<Threat>, String> {
        parse_threats(&powershell(THREATS_SCRIPT, &[])?)
    }

    /// Runs a scan and waits for it to finish.
    pub fn scan(kind: ScanKind, path: Option<&str>) -> Result<(), String> {
        let script = match kind {
            ScanKind::Quick => "Start-MpScan -ScanType QuickScan",
            ScanKind::Full => "Start-MpScan -ScanType FullScan",
            // The path travels in an environment variable, never inside the script text.
            ScanKind::Custom => {
                "Start-MpScan -ScanType CustomScan -ScanPath $env:CLEARWIN_SCAN_PATH"
            }
        };
        let path = path.unwrap_or_default();
        if matches!(kind, ScanKind::Custom) && path.is_empty() {
            return Err("Pick a file or folder to scan.".into());
        }
        powershell(script, &[("CLEARWIN_SCAN_PATH", path)]).map(|_| ())
    }

    pub fn update_signatures() -> Result<(), String> {
        powershell("Update-MpSignature", &[]).map(|_| ())
    }

    pub fn open_windows_security() -> Result<(), String> {
        std::process::Command::new("explorer.exe")
            .arg("windowsdefender:")
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    const UNSUPPORTED: &str = "Microsoft Defender is only available on Windows.";

    pub fn status() -> Result<DefenderStatus, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn threats() -> Result<Vec<Threat>, String> {
        Err(UNSUPPORTED.into())
    }

    pub fn scan(_kind: ScanKind, _path: Option<&str>) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    pub fn update_signatures() -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    pub fn open_windows_security() -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

pub use imp::{open_windows_security, scan, status, threats, update_signatures};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status() {
        let json = r#"{"serviceEnabled":true,"antivirusEnabled":true,"realTimeProtection":false,
            "tamperProtection":true,"runningMode":"Normal","signatureVersion":"1.2.3",
            "signatureUpdated":1700000000,"signatureAgeDays":0,"quickScanEnded":null,
            "fullScanEnded":null,"products":["Windows Defender"]}"#;
        let status = parse_status(json).unwrap();
        assert!(!status.real_time_protection);
        assert_eq!(status.signature_updated, Some(1_700_000_000));
        assert_eq!(status.products, vec!["Windows Defender"]);
    }

    #[test]
    fn parses_threats_including_none() {
        assert!(parse_threats("").unwrap().is_empty());
        assert!(parse_threats("[]").unwrap().is_empty());
        let json = r#"[{"name":"Trojan:Win32/Test","severity":5,"detected":1700000000,
            "resources":["file:_C:\\x.exe"],"statusId":3,"actionSuccess":true}]"#;
        let threats = parse_threats(json).unwrap();
        assert_eq!(threats[0].name, "Trojan:Win32/Test");
        assert_eq!(threat_status(threats[0].status_id), "Quarantined");
        assert_eq!(severity_name(threats[0].severity), "Severe");
    }
}
