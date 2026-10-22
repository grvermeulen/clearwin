//! Software updates through winget, the package manager built into Windows 10 and 11.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUpdate {
    pub name: String,
    /// winget package id, e.g. `Google.Chrome`.
    pub id: String,
    pub version: String,
    pub available: String,
    pub source: String,
}

/// Reads the table `winget upgrade` prints.
///
/// The headers are translated with Windows, so columns are found by where the
/// header words start rather than by their names. The table may be followed by
/// a second one (packages that need explicit targeting), which is read too.
pub fn parse_upgrade_table(output: &str) -> Vec<SoftwareUpdate> {
    // winget draws a spinner with carriage returns; only the text after the last one counts.
    let lines: Vec<&str> = output
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or(l))
        .collect();
    let mut updates = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let is_rule =
            lines[i + 1].trim().len() > 10 && lines[i + 1].trim().chars().all(|c| c == '-');
        if !is_rule {
            i += 1;
            continue;
        }
        let header: Vec<char> = lines[i].chars().collect();
        let starts: Vec<usize> = (0..header.len())
            .filter(|&k| header[k] != ' ' && (k == 0 || header[k - 1] == ' '))
            .collect();
        i += 2;
        if starts.len() < 4 {
            continue;
        }
        while i < lines.len() {
            let row: Vec<char> = lines[i].chars().collect();
            // A row is as wide as the table; anything shorter is the summary line.
            if row.len() <= starts[starts.len() - 1] || lines[i].trim().is_empty() {
                break;
            }
            let cell = |n: usize| -> String {
                let from = starts[n].min(row.len());
                let to = starts
                    .get(n + 1)
                    .copied()
                    .unwrap_or(row.len())
                    .min(row.len());
                row[from..to].iter().collect::<String>().trim().to_string()
            };
            let update = SoftwareUpdate {
                name: cell(0),
                id: cell(1),
                version: cell(2),
                available: cell(3),
                source: if starts.len() > 4 {
                    cell(4)
                } else {
                    String::new()
                },
            };
            // Truncated ids end in an ellipsis and can't be upgraded by id.
            if !update.id.is_empty() && !update.id.ends_with('…') && !update.available.is_empty()
            {
                updates.push(update);
            }
            i += 1;
        }
    }
    updates
}

/// Arguments for a silent `winget upgrade` of one package, or of everything when `id` is `None`.
pub fn upgrade_args(id: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec!["upgrade".into()];
    match id {
        Some(id) => args.extend(["--id".into(), id.into(), "--exact".into()]),
        None => args.push("--all".into()),
    }
    args.extend(
        [
            "--silent",
            // Without it winget refuses programs whose installed version it can't tell,
            // even though `list` (which passes it too) offered them.
            "--include-unknown",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]
        .map(String::from),
    );
    args
}

/// winget's own error codes that people run into, in words.
fn known_error(code: u32) -> Option<&'static str> {
    Some(match code {
        0x8A15_0011 => "The download did not match the expected file, so it was not installed",
        0x8A15_002B => "No update is available for how this program was installed",
        0x8A15_008E => "The new version installs in a different way than the one you have, so winget won't replace it",
        0x8007_0020 => "A file the update needs is in use. Close the program and try again",
        0x8007_0005 => "Access was denied. The update needs administrator rights",
        0x8007_02E4 => "The update needs administrator rights",
        0x8007_3D28 => "Windows only lets an administrator install this update",
        0x8A15_0101 => "The program is open. Close it and try again",
        0x8A15_0102 => "Another installation is already running. Try again when it has finished",
        0x8A15_0103 => "A file the update needs is in use. Close the program and try again",
        0x8A15_0104 => "Something this update needs is missing",
        0x8A15_0105 => "The disk is full",
        0x8A15_0106 => "Not enough memory",
        0x8A15_0107 => "No internet connection",
        0x8A15_0109 => "Installed, but Windows needs a restart to finish",
        0x8A15_010A => "Windows needs a restart before this can be installed",
        0x8A15_010C => "The installation was cancelled",
        0x8A15_010D => "Another version of this program is already installed",
        0x8A15_010F => "Blocked by a policy on this PC",
        _ => return None,
    })
}

/// Why an update failed: winget's code in words when it is a known one, otherwise
/// the last real message winget printed (not its progress bars or banners).
pub fn failure_reason(exit_code: Option<i32>, stdout: &str) -> String {
    let code = exit_code.map(|c| c as u32);
    // winget's own code first, then the installer's, which winget prints (`0x80070020 : ...`).
    if let Some(text) = code
        .into_iter()
        .chain(printed_codes(stdout))
        .find_map(known_error)
    {
        return format!("{text}.");
    }
    let noise = |l: &str| {
        l.chars().count() <= 3
            || l.contains('█')
            || l.contains('▒')
            || l.starts_with("Found ")
            || l.starts_with("Downloading ")
            || l.starts_with("This application is licensed")
            || l.starts_with("Microsoft is not responsible")
            || l.starts_with("Successfully verified")
            || l.starts_with("Starting package install")
    };
    let message = stdout
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or(l).trim())
        .rfind(|l| !l.is_empty() && !noise(l))
        .unwrap_or("winget could not update this program");
    match code {
        Some(c) if c != 0 && !message.contains(&format!("{c:#x}")) => {
            format!("{message} (code {c:#X})")
        }
        _ => message.to_string(),
    }
}

/// Hex error codes such as `0x80073d28` that winget printed, in order.
fn printed_codes(stdout: &str) -> Vec<u32> {
    let lower = stdout.to_ascii_lowercase();
    lower
        .match_indices("0x")
        .filter_map(|(i, _)| {
            let hex: String = lower[i + 2..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .collect();
            (hex.len() == 8).then(|| u32::from_str_radix(&hex, 16).ok())?
        })
        .collect()
}

/// Something ClearWin can try when an update fails.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Fix {
    /// Run the update again as administrator (Windows asks for permission).
    RunAsAdmin,
    /// Close the program (and stop its services), update, then start the services again.
    CloseAndRetry,
    /// Uninstall the old version and install the new one, for when winget won't upgrade in place.
    Reinstall,
}

/// Why an update failed, and what ClearWin can do about it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFailure {
    pub reason: String,
    pub fix: Option<Fix>,
}

impl From<String> for UpdateFailure {
    fn from(reason: String) -> Self {
        Self { reason, fix: None }
    }
}

/// Which fix suits a failed update, judging by winget's code and the installer's.
pub fn fix_for(exit_code: Option<i32>, stdout: &str) -> Option<Fix> {
    let codes: Vec<u32> = exit_code
        .map(|c| c as u32)
        .into_iter()
        .chain(printed_codes(stdout))
        .collect();
    let lower = stdout.to_ascii_lowercase();
    codes
        .iter()
        .find_map(|&code| match code {
            0x8007_3D28 | 0x8007_0005 | 0x8007_02E4 => Some(Fix::RunAsAdmin),
            0x8007_0020 | 0x8A15_0101 | 0x8A15_0103 => Some(Fix::CloseAndRetry),
            0x8A15_008E | 0x8A15_002B => Some(Fix::Reinstall),
            _ => None,
        })
        .or_else(|| {
            if lower.contains("administrator privileges are required") {
                Some(Fix::RunAsAdmin)
            } else if lower.contains("being used by another process") {
                Some(Fix::CloseAndRetry)
            } else if lower.contains("install technology is different") {
                Some(Fix::Reinstall)
            } else {
                None
            }
        })
}

#[cfg_attr(not(windows), allow(dead_code))]
fn failure(exit_code: Option<i32>, stdout: &str) -> UpdateFailure {
    UpdateFailure {
        reason: failure_reason(exit_code, stdout),
        fix: fix_for(exit_code, stdout),
    }
}

/// Arguments to remove a program through winget.
pub fn uninstall_args(id: &str) -> Vec<String> {
    ["uninstall", "--id", id, "--exact", "--silent"]
        .into_iter()
        .chain(["--accept-source-agreements", "--disable-interactivity"])
        .map(String::from)
        .collect()
}

/// Arguments to install the newest version of a program through winget.
pub fn install_args(id: &str, source: &str) -> Vec<String> {
    let mut args: Vec<String> = ["install", "--id", id, "--exact", "--silent"]
        .map(String::from)
        .into();
    if !source.is_empty() {
        args.extend(["--source".into(), source.into()]);
    }
    args.extend(
        [
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]
        .map(String::from),
    );
    args
}

/// A PowerShell single-quoted string, which takes everything literally.
fn ps_str(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        out.push(c);
        // PowerShell also ends a quoted string at typographic single quotes.
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn ps_array(args: &[String]) -> String {
    format!(
        "@({})",
        args.iter()
            .map(|a| ps_str(a))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// winget's "no installed package found", which only matters as "already uninstalled".
const NOT_INSTALLED: i32 = 0x8A15_0014_u32 as i32;

/// The PowerShell script that carries out `fix` for `update`. winget's output goes
/// to `log` and the script exits with winget's code, so the result can be read
/// back even when the script ran elevated in a separate process.
///
/// `program_dir` is the program's own folder, needed by [`Fix::CloseAndRetry`].
pub fn fix_script(
    fix: Fix,
    update: &SoftwareUpdate,
    program_dir: Option<&str>,
    log: &str,
) -> String {
    let mut script = format!(
        r#"
$log = {log}
[IO.File]::WriteAllText($log, '')
function Winget($a, [int[]]$okCodes = @()) {{
    $text = & winget.exe @a | Out-String
    $code = $LASTEXITCODE
    [IO.File]::AppendAllText($log, $text)
    if ($code -ne 0 -and $okCodes -notcontains $code) {{ exit $code }}
}}
"#,
        log = ps_str(log)
    );
    let upgrade = ps_array(&upgrade_args(Some(&update.id)));
    match fix {
        Fix::RunAsAdmin => script.push_str(&format!("Winget {upgrade}\n")),
        Fix::Reinstall => script.push_str(&format!(
            "Winget {} @({NOT_INSTALLED})\nWinget {}\n",
            ps_array(&uninstall_args(&update.id)),
            ps_array(&install_args(&update.id, &update.source)),
        )),
        Fix::CloseAndRetry => {
            let dir = program_dir.unwrap_or_default().trim_end_matches('\\');
            script.push_str(&format!(
                r#"
$dir = {dir}
$inside = {{ param($p) $p -and $p.Trim('"').StartsWith($dir + '\', 'OrdinalIgnoreCase') }}
$services = @(Get-CimInstance Win32_Service | Where-Object {{ $_.State -eq 'Running' -and (& $inside $_.PathName) }})
$services | ForEach-Object {{ Stop-Service -Name $_.Name -Force -ErrorAction SilentlyContinue }}
Get-Process | Where-Object {{ & $inside $_.Path }} | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
$text = & winget.exe @({upgrade_items}) | Out-String
$code = $LASTEXITCODE
[IO.File]::AppendAllText($log, $text)
$services | ForEach-Object {{ Start-Service -Name $_.Name -ErrorAction SilentlyContinue }}
exit $code
"#,
                dir = ps_str(dir),
                upgrade_items = upgrade.trim_start_matches("@(").trim_end_matches(')'),
            ));
        }
    }
    script.push_str("exit 0\n");
    script
}

/// Finds the folder of an installed program by the name winget showed for it.
pub fn program_dir_for(name: &str, apps: &[crate::uninstall::InstalledApp]) -> Option<String> {
    let name = name.trim();
    let matches = |app_name: &str| match name.strip_suffix('…') {
        Some(prefix) => app_name.starts_with(prefix.trim_end()),
        None => app_name.eq_ignore_ascii_case(name),
    };
    let found: Vec<_> = apps.iter().filter(|a| matches(&a.name)).collect();
    // A cut-off name that fits several programs could close the wrong one.
    if name.ends_with('…') && found.len() != 1 {
        return None;
    }
    found.into_iter().find_map(|a| a.install_dir.clone())
}

#[cfg(windows)]
fn winget(args: &[String]) -> Result<std::process::Output, String> {
    crate::ps::hidden("winget.exe")
        .args(args)
        .output()
        .map_err(|_| {
            "winget is not available. Install \"App Installer\" from the Microsoft Store."
                .to_string()
        })
}

/// Lists installed programs that have a newer version available.
#[cfg(windows)]
pub fn list() -> Result<Vec<SoftwareUpdate>, String> {
    let output = winget(
        &[
            "upgrade",
            "--include-unknown",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]
        .map(String::from),
    )?;
    Ok(parse_upgrade_table(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Updates one program (or all of them), waiting until winget has finished.
#[cfg(windows)]
pub fn upgrade(id: Option<&str>) -> Result<(), UpdateFailure> {
    let output = winget(&upgrade_args(id))?;
    if output.status.success() {
        return Ok(());
    }
    Err(failure(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
    ))
}

/// Carries out `fix` for `update`. With `elevate`, the work runs in a separate
/// PowerShell started as administrator, so Windows asks for permission once.
#[cfg(windows)]
pub fn apply_fix(fix: Fix, update: &SoftwareUpdate, elevate: bool) -> Result<(), UpdateFailure> {
    let program_dir = match fix {
        Fix::CloseAndRetry => Some(
            program_dir_for(&update.name, &crate::uninstall::list()).ok_or_else(|| {
                format!("ClearWin could not find where {} is installed. Close it yourself and press Retry.", update.name)
            })?,
        ),
        _ => None,
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let log = std::env::temp_dir().join(format!(
        "clearwin-winget-{}-{stamp}.log",
        std::process::id()
    ));
    let log_path = log.to_string_lossy().into_owned();
    let script = format!(
        "{}{}",
        crate::ps::PRELUDE,
        fix_script(fix, update, program_dir.as_deref(), &log_path)
    );
    // 1223 is Windows' "cancelled by the user", here: the permission prompt was declined.
    let runner = if elevate {
        r#"
try {
    $p = Start-Process powershell.exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand',$env:CLEARWIN_SCRIPT
} catch { exit 1223 }
exit $p.ExitCode
"#
        .to_string()
    } else {
        script.clone()
    };
    let output = crate::ps::hidden("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ])
        .arg(crate::ps::encode_command(&runner))
        .env("CLEARWIN_SCRIPT", crate::ps::encode_command(&script))
        .output()
        .map_err(|e| format!("Could not start PowerShell: {e}"))?;
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    match output.status.code() {
        Some(0) => Ok(()),
        Some(1223) if elevate => Err("Administrator rights were not granted.".to_string().into()),
        code => {
            let mut failed = failure(code, &text);
            // Offering the same fix again would just repeat this.
            if failed.fix == Some(fix) {
                failed.fix = None;
            }
            Err(failed)
        }
    }
}

/// Stops winget from offering updates for a program (`winget pin`), for updates that can't be installed.
#[cfg(windows)]
pub fn ignore(id: &str) -> Result<(), String> {
    let output = winget(
        &[
            "pin",
            "add",
            "--id",
            id,
            "--exact",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]
        .map(String::from),
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failure_reason(
            output.status.code(),
            &String::from_utf8_lossy(&output.stdout),
        ))
    }
}

#[cfg(not(windows))]
pub fn list() -> Result<Vec<SoftwareUpdate>, String> {
    Err("Only available on Windows.".into())
}

#[cfg(not(windows))]
pub fn upgrade(_id: Option<&str>) -> Result<(), UpdateFailure> {
    Err("Only available on Windows.".to_string().into())
}

#[cfg(not(windows))]
pub fn apply_fix(_fix: Fix, _update: &SoftwareUpdate, _elevate: bool) -> Result<(), UpdateFailure> {
    Err("Only available on Windows.".to_string().into())
}

#[cfg(not(windows))]
pub fn ignore(_id: &str) -> Result<(), String> {
    Err("Only available on Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGLISH: &str = "\r   - \r   \\ \rName                       Id                     Version   Available Source
------------------------------------------------------------------------------------
Google Chrome              Google.Chrome          118.0.1   119.0.2   winget
7-Zip 22.01 (x64)          7zip.7zip              22.01     23.01     winget
Some Very Long Program N…  Vendor.SomeVeryLongPr… 1.0       2.0       winget
3 upgrades available.

The following packages have an upgrade available, but require explicit targeting for upgrade:
Name    Id              Version Available Source
------------------------------------------------
Discord Discord.Discord 1.0.1   1.0.2     winget
";

    #[test]
    fn reads_both_tables_and_skips_truncated_ids() {
        let updates = parse_upgrade_table(ENGLISH);
        let ids: Vec<_> = updates.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(ids, ["Google.Chrome", "7zip.7zip", "Discord.Discord"]);
        assert_eq!(updates[1].name, "7-Zip 22.01 (x64)");
        assert_eq!(updates[0].version, "118.0.1");
        assert_eq!(updates[0].available, "119.0.2");
        assert_eq!(updates[2].source, "winget");
    }

    #[test]
    fn reads_translated_headers() {
        let dutch = "Naam          Id             Versie Beschikbaar Bron
--------------------------------------------------------
Notepad++ (é) Notepad++.Npp  8.5    8.6         winget
1 upgrades beschikbaar.
";
        let updates = parse_upgrade_table(dutch);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "Notepad++ (é)");
        assert_eq!(updates[0].id, "Notepad++.Npp");
        assert_eq!(updates[0].available, "8.6");
    }

    #[test]
    fn nothing_to_update() {
        assert!(
            parse_upgrade_table("No installed package found matching input criteria.").is_empty()
        );
    }

    #[test]
    fn explains_failures() {
        // 0x8A150101 as winget's signed exit code.
        assert_eq!(
            failure_reason(Some(0x8A15_0101_u32 as i32), ""),
            "The program is open. Close it and try again."
        );
        let out = "Found Git [Git.Git] Version 2.55.0\r\nThis application is licensed to you by its owner.\r\n\
                   Downloading https://example.com/git.exe\r\n  ██████████  60 MB / 60 MB\r\n\
                   Successfully verified installer hash\r\nStarting package install...\r\n\
                   Installer failed with exit code: 1603\r\n";
        assert_eq!(
            failure_reason(Some(-1978335226), out),
            "Installer failed with exit code: 1603 (code 0x8A150006)"
        );
        assert_eq!(
            failure_reason(None, ""),
            "winget could not update this program"
        );
    }

    #[test]
    fn upgrade_one_is_exact_and_silent() {
        let args = upgrade_args(Some("Google.Chrome"));
        assert_eq!(&args[..4], ["upgrade", "--id", "Google.Chrome", "--exact"]);
        assert!(args.contains(&"--silent".to_string()));
        assert!(upgrade_args(None).contains(&"--all".to_string()));
    }

    #[test]
    fn finds_a_fix_for_each_known_failure() {
        let admin = "Installer failed with exit code: 0x80073d28 : The package installation failed because administrator privileges are required.";
        assert_eq!(
            fix_for(Some(0x8A15_0006_u32 as i32), admin),
            Some(Fix::RunAsAdmin)
        );
        assert_eq!(
            failure_reason(Some(0x8A15_0006_u32 as i32), admin),
            "Windows only lets an administrator install this update."
        );
        let in_use = "0x80070020 : The process cannot access the file because it is being used by another process.";
        assert_eq!(
            fix_for(Some(0x8007_0020_u32 as i32), in_use),
            Some(Fix::CloseAndRetry)
        );
        assert_eq!(
            fix_for(Some(0x8A15_008E_u32 as i32), ""),
            Some(Fix::Reinstall)
        );
        assert_eq!(
            fix_for(Some(0x8A15_002B_u32 as i32), ""),
            Some(Fix::Reinstall)
        );
        assert_eq!(
            fix_for(
                Some(0x8A15_0006_u32 as i32),
                "Installer failed with exit code: 1603"
            ),
            None
        );
    }

    #[test]
    fn fix_scripts_quote_and_order_their_steps() {
        let update = SoftwareUpdate {
            name: "Jellyfin Server 12.1.0".into(),
            id: "Jellyfin.Server".into(),
            version: "12.0".into(),
            available: "12.1.0".into(),
            source: "winget".into(),
        };
        let reinstall = fix_script(Fix::Reinstall, &update, None, r"C:\Temp\it's.log");
        assert!(reinstall.contains(r"$log = 'C:\Temp\it''s.log'"));
        assert_eq!(ps_str("O\u{2019}Brien"), "'O\u{2019}\u{2019}Brien'");
        let uninstall = reinstall
            .find("'uninstall', '--id', 'Jellyfin.Server'")
            .unwrap();
        let install = reinstall
            .find("'install', '--id', 'Jellyfin.Server'")
            .unwrap();
        assert!(uninstall < install);
        let close = fix_script(
            Fix::CloseAndRetry,
            &update,
            Some(r"C:\Program Files\Jellyfin\Server\"),
            "x",
        );
        assert!(close.contains(r"$dir = 'C:\Program Files\Jellyfin\Server'"));
        assert!(close.find("Stop-Process").unwrap() < close.find("'upgrade'").unwrap());
        assert!(close.find("'upgrade'").unwrap() < close.find("Start-Service").unwrap());
    }

    #[test]
    fn finds_program_folder_by_winget_name() {
        let app = |name: &str, dir: &str| crate::uninstall::InstalledApp {
            id: name.into(),
            name: name.into(),
            publisher: None,
            version: None,
            install_date: None,
            size_bytes: None,
            install_dir: Some(dir.into()),
            uninstall_command: String::new(),
        };
        let apps = [
            app("Ollama version 0.40.5", r"C:\Ollama"),
            app("Jellyfin Server 12.1.0", r"C:\Jellyfin"),
        ];
        assert_eq!(
            program_dir_for("Ollama version 0.40.5", &apps).as_deref(),
            Some(r"C:\Ollama")
        );
        assert_eq!(
            program_dir_for("Jellyfin Server…", &apps).as_deref(),
            Some(r"C:\Jellyfin")
        );
        assert_eq!(program_dir_for("Slack", &apps), None);
        let twins = [
            app("Jellyfin Server", r"C:\A"),
            app("Jellyfin Media Player", r"C:\B"),
        ];
        assert_eq!(program_dir_for("Jellyfin…", &twins), None);
    }
}
