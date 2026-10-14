//! Software updates through winget, the package manager built into Windows 10 and 11.

use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
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
    if let Some(text) = code.and_then(known_error) {
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
pub fn upgrade(id: Option<&str>) -> Result<(), String> {
    let output = winget(&upgrade_args(id))?;
    if output.status.success() {
        return Ok(());
    }
    Err(failure_reason(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
    ))
}

#[cfg(not(windows))]
pub fn list() -> Result<Vec<SoftwareUpdate>, String> {
    Err("Only available on Windows.".into())
}

#[cfg(not(windows))]
pub fn upgrade(_id: Option<&str>) -> Result<(), String> {
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
}
