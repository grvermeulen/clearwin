//! What is inside this PC: processor, memory, graphics, disks and Windows itself.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemInfo {
    pub computer: String,
    pub manufacturer: String,
    pub model: String,
    pub os: String,
    pub os_version: String,
    pub os_build: String,
    pub installed: Option<i64>,
    pub last_boot: Option<i64>,
    pub cpu: String,
    pub cores: u32,
    pub threads: u32,
    pub cpu_mhz: u32,
    pub memory_bytes: u64,
    pub memory_modules: Vec<MemoryModule>,
    pub gpus: Vec<Gpu>,
    pub disks: Vec<PhysicalDisk>,
    pub motherboard: String,
    pub bios: String,
    pub battery_percent: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MemoryModule {
    pub slot: String,
    pub capacity_bytes: u64,
    pub speed_mhz: u32,
    pub manufacturer: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Gpu {
    pub name: String,
    pub driver_version: String,
    pub resolution: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PhysicalDisk {
    pub name: String,
    /// SSD, HDD or Unspecified.
    pub media: String,
    /// NVMe, SATA, USB, ...
    pub bus: String,
    pub size_bytes: u64,
    /// Windows' own verdict: Healthy, Warning or Unhealthy.
    pub health: String,
}

#[cfg_attr(not(windows), allow(dead_code))]
const SCRIPT: &str = r#"
function First($items) { @($items)[0] }
$os = First (Get-CimInstance Win32_OperatingSystem)
$cs = First (Get-CimInstance Win32_ComputerSystem)
$cpu = First (Get-CimInstance Win32_Processor)
$board = First (Get-CimInstance Win32_BaseBoard)
$bios = First (Get-CimInstance Win32_BIOS)
$battery = $null
try { $b = First (Get-CimInstance Win32_Battery); if ($b) { $battery = [int]$b.EstimatedChargeRemaining } } catch {}
$disks = @()
try {
  $disks = @(Get-PhysicalDisk | Sort-Object DeviceId | ForEach-Object {
    [pscustomobject]@{ name = [string]$_.FriendlyName; media = [string]$_.MediaType; bus = [string]$_.BusType; sizeBytes = [int64]$_.Size; health = [string]$_.HealthStatus }
  })
} catch {}
ConvertTo-Json -Compress -Depth 4 -InputObject ([pscustomobject]@{
  computer = [string]$cs.Name
  manufacturer = [string]$cs.Manufacturer
  model = [string]$cs.Model
  os = [string]$os.Caption
  osVersion = [string]$os.Version
  osBuild = [string]$os.BuildNumber
  installed = U $os.InstallDate
  lastBoot = U $os.LastBootUpTime
  cpu = ([string]$cpu.Name).Trim()
  cores = [int]$cpu.NumberOfCores
  threads = [int]$cpu.NumberOfLogicalProcessors
  cpuMhz = [int]$cpu.MaxClockSpeed
  memoryBytes = [int64]$cs.TotalPhysicalMemory
  memoryModules = @(Get-CimInstance Win32_PhysicalMemory | ForEach-Object {
    [pscustomobject]@{ slot = [string]$_.DeviceLocator; capacityBytes = [int64]$_.Capacity; speedMhz = [int]$_.Speed; manufacturer = ([string]$_.Manufacturer).Trim() }
  })
  gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object {
    $res = if ($_.CurrentHorizontalResolution) { "$($_.CurrentHorizontalResolution) x $($_.CurrentVerticalResolution)" } else { '' }
    [pscustomobject]@{ name = [string]$_.Name; driverVersion = [string]$_.DriverVersion; resolution = $res }
  })
  disks = $disks
  motherboard = ("$($board.Manufacturer) $($board.Product)").Trim()
  bios = ("$($bios.Manufacturer) $($bios.SMBIOSBIOSVersion)").Trim()
  batteryPercent = $battery
})
"#;

pub fn parse(json: &str) -> Result<SystemInfo, String> {
    serde_json::from_str(json.trim()).map_err(|e| format!("Unexpected system information: {e}"))
}

/// Collects the system information. Takes a few seconds.
pub fn info() -> Result<SystemInfo, String> {
    parse(&crate::ps::run(SCRIPT, &[])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_info_and_tolerates_nulls() {
        let info = parse(
            r#"{"computer":"PC","cpu":"AMD Ryzen 7","cores":8,"threads":16,"memoryBytes":34359738368,
                "installed":null,"batteryPercent":null,
                "disks":[{"name":"Samsung 980","media":"SSD","bus":"NVMe","sizeBytes":1000204886016,"health":"Healthy"}],
                "gpus":[{"name":"NVIDIA GeForce RTX 4070","driverVersion":"31.0","resolution":"2560 x 1440"}]}"#,
        )
        .unwrap();
        assert_eq!(info.threads, 16);
        assert_eq!(info.disks[0].health, "Healthy");
        assert_eq!(info.battery_percent, None);
        assert!(info.memory_modules.is_empty());
    }
}
