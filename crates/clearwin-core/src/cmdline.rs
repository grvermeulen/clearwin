//! Reading file paths out of Windows command lines and registry values.
//!
//! These helpers are deliberately conservative: when a string is ambiguous
//! they return `None`, so callers never flag something as broken on a guess.

use std::path::{Path, PathBuf};

const EXTENSIONS: &[&str] = &[
    ".exe", ".dll", ".com", ".bat", ".cmd", ".lnk", ".ocx", ".sys", ".cpl", ".msc", ".scr", ".vbs",
    ".ps1",
];

/// Replaces `%NAME%` with the value `lookup` returns; unknown names are left as-is.
pub fn expand_env(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name).filter(|_| !name.is_empty()) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Expands variables from this process's environment.
pub fn expand_env_vars(s: &str) -> String {
    expand_env(s, |name| std::env::var(name).ok())
}

/// True for `C:\...` and `\\server\...` paths.
pub fn is_windows_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || s.starts_with("\\\\")
}

/// Byte offsets just past each known extension that is followed by a separator.
fn program_ends(command: &str) -> Vec<usize> {
    let lower = command.to_ascii_lowercase();
    let mut ends = Vec::new();
    for ext in EXTENSIONS {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(ext) {
            let end = from + pos + ext.len();
            if matches!(
                lower[end..].chars().next(),
                None | Some(' ' | ',' | '"' | '\t')
            ) {
                ends.push(end);
            }
            from = end;
        }
    }
    ends.sort_unstable();
    ends
}

/// Splits a command line into the program and the rest of the arguments.
pub fn split_command(command: &str) -> Option<(String, String)> {
    let command = command.trim();
    if let Some(quoted) = command.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some((
            quoted[..end].to_string(),
            quoted[end + 1..].trim_start().to_string(),
        ));
    }
    // Unquoted paths may contain spaces, so find where the program's extension ends.
    // Several spots can qualify ("C:\tools.exe dir\run.exe"); prefer one that exists.
    let ends = program_ends(command);
    if let Some(&end) = ends
        .iter()
        .find(|&&end| Path::new(&command[..end]).exists())
        .or(ends.first())
    {
        return Some((
            command[..end].to_string(),
            command[end..].trim_start().to_string(),
        ));
    }
    match command.split_once(' ') {
        Some((program, args)) => Some((program.to_string(), args.trim_start().to_string())),
        None if !command.is_empty() => Some((command.to_string(), String::new())),
        None => None,
    }
}

/// The file a command line or registry value points at, when it is an absolute path.
pub fn target_path(value: &str) -> Option<PathBuf> {
    let expanded = expand_env_vars(value.trim());
    let (program, _) = split_command(&expanded)?;
    let program = program.trim().trim_matches('"');
    is_windows_absolute(program).then(|| PathBuf::from(program))
}

/// True when `path` does not exist, also checking where 32-bit apps really keep
/// "System32" files, so those are not reported missing by mistake.
pub fn is_missing(path: &Path) -> bool {
    if path.exists() {
        return false;
    }
    let text = path.to_string_lossy();
    let lower = text.to_ascii_lowercase();
    for (from, to) in [
        ("\\system32\\", "\\SysWOW64\\"),
        ("\\sysnative\\", "\\System32\\"),
    ] {
        if let Some(pos) = lower.find(from) {
            let alt = format!("{}{}{}", &text[..pos], to, &text[pos + from.len()..]);
            if Path::new(&alt).exists() {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_variables_only() {
        let lookup =
            |n: &str| (n.eq_ignore_ascii_case("windir")).then(|| "C:\\Windows".to_string());
        assert_eq!(
            expand_env("%WINDIR%\\notepad.exe", lookup),
            "C:\\Windows\\notepad.exe"
        );
        assert_eq!(expand_env("%NOPE%\\x", lookup), "%NOPE%\\x");
        assert_eq!(expand_env("100% sure", lookup), "100% sure");
    }

    #[test]
    fn splits_quoted_and_unquoted_commands() {
        assert_eq!(
            split_command(r#""C:\Program Files\App\app.exe" --minimized"#),
            Some((r"C:\Program Files\App\app.exe".into(), "--minimized".into()))
        );
        assert_eq!(
            split_command(r"C:\Program Files\App\app.exe /background"),
            Some((r"C:\Program Files\App\app.exe".into(), "/background".into()))
        );
        assert_eq!(
            split_command("MsiExec.exe /X{1234}"),
            Some(("MsiExec.exe".into(), "/X{1234}".into()))
        );
        assert_eq!(
            split_command(r"C:\Windows\system32\rundll32.exe C:\x\y.dll,Entry"),
            Some((
                r"C:\Windows\system32\rundll32.exe".into(),
                r"C:\x\y.dll,Entry".into()
            ))
        );
        // ".exe" inside a folder name is not the end of the program.
        assert_eq!(
            split_command(r"C:\tools.exe stuff\run.exe -a"),
            Some((r"C:\tools.exe".into(), r"stuff\run.exe -a".into()))
        );
    }

    #[test]
    fn prefers_the_program_that_exists() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("tools.exe stuff").join("run.exe");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, b"").unwrap();
        let command = format!("{} -a", program.display());
        assert_eq!(
            split_command(&command),
            Some((program.display().to_string(), "-a".into()))
        );
    }

    #[test]
    fn target_path_needs_an_absolute_path() {
        assert_eq!(
            target_path(r#""C:\a b\c.exe" -x"#),
            Some(PathBuf::from(r"C:\a b\c.exe"))
        );
        assert_eq!(target_path("MsiExec.exe /X{1}"), None);
        assert_eq!(
            target_path(r"\\server\share\tool.exe"),
            Some(PathBuf::from(r"\\server\share\tool.exe"))
        );
        assert_eq!(target_path(""), None);
    }

    #[test]
    fn existing_files_are_not_missing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.exe");
        std::fs::write(&file, b"").unwrap();
        assert!(!is_missing(&file));
        assert!(is_missing(&dir.path().join("b.exe")));
    }
}
