//! Running PowerShell scripts without a console window and without quoting problems.

/// Shared start of every script: stop on errors, UTF-8 output, and `U`, which
/// turns PowerShell dates into Unix seconds (`$null` stays empty).
pub const PRELUDE: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
function U($d) { if ($d) { ([DateTimeOffset]$d).ToUnixTimeSeconds() } else { $null } }
"#;

/// Base64 of the script as UTF-16LE, the form `powershell -EncodedCommand` takes.
/// This avoids every quoting problem with paths and special characters.
pub fn encode_command(script: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Starts a program with no console window flashing up.
#[cfg(windows)]
pub fn hidden(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Runs `script` (after [`PRELUDE`]) and returns its standard output.
/// Values that come from outside, like paths, should go in through `env`.
#[cfg(windows)]
pub fn run(script: &str, env: &[(&str, &str)]) -> Result<String, String> {
    let output = hidden("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ])
        .arg(encode_command(&format!("{PRELUDE}{script}")))
        .envs(env.iter().copied())
        .output()
        .map_err(|e| format!("Could not start PowerShell: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        let first = err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("PowerShell did not respond.");
        Err(first.trim().to_string())
    }
}

#[cfg(not(windows))]
pub fn run(_script: &str, _env: &[(&str, &str)]) -> Result<String, String> {
    Err("Only available on Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_powershell_expects() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('dir'))
        assert_eq!(encode_command("dir"), "ZABpAHIA");
        assert_eq!(encode_command("ab"), "YQBiAA==");
        assert_eq!(encode_command(""), "");
    }
}
