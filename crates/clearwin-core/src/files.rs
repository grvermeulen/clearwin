//! Actions on individual files the user picked in the Storage page.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleReport {
    pub moved: Vec<PathBuf>,
    pub failures: Vec<String>,
}

/// Moves files to the Recycle Bin, so the user can still restore them.
///
/// Only regular files are accepted; folders and links are refused.
pub fn move_to_recycle_bin(paths: &[PathBuf]) -> RecycleReport {
    let mut report = RecycleReport::default();
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_file() => {}
            Ok(_) => {
                report
                    .failures
                    .push(format!("{}: not a regular file", path.display()));
                continue;
            }
            Err(e) => {
                report.failures.push(format!("{}: {e}", path.display()));
                continue;
            }
        }
        match trash::delete(path) {
            Ok(()) => report.moved.push(path.clone()),
            Err(e) => report.failures.push(format!("{}: {e}", path.display())),
        }
    }
    report
}

/// Opens File Explorer with the file or folder selected.
pub fn reveal_in_explorer(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // explorer.exe needs "/select," and the path as one argument, quoted as-is.
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "only available on Windows",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let report = move_to_recycle_bin(&[dir.path().to_path_buf()]);
        assert!(report.moved.is_empty());
        assert_eq!(report.failures.len(), 1);
        assert!(dir.path().exists());
    }
}
