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
    let text = String::from_utf8_lossy(&output.stdout);
    let last = text
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or(l).trim())
        .rfind(|l| !l.is_empty())
        .unwrap_or("winget could not update this program.");
    Err(last.to_string())
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
    fn upgrade_one_is_exact_and_silent() {
        let args = upgrade_args(Some("Google.Chrome"));
        assert_eq!(&args[..4], ["upgrade", "--id", "Google.Chrome", "--exact"]);
        assert!(args.contains(&"--silent".to_string()));
        assert!(upgrade_args(None).contains(&"--all".to_string()));
    }
}
