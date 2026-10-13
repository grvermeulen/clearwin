//! Finds files with identical contents.
//!
//! Files are grouped by size first, then by a hash of their first 64 KB, and
//! only files still matching are hashed in full, so most files are never read.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use rayon::prelude::*;
use serde::Serialize;
use walkdir::WalkDir;

use crate::storage::{unix_seconds, Progress};

const PREFIX_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    pub path: PathBuf,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    /// Size of each copy.
    pub size: u64,
    /// Oldest first.
    pub files: Vec<DuplicateFile>,
}

impl DuplicateGroup {
    /// Space freed by keeping only one copy.
    pub fn wasted_bytes(&self) -> u64 {
        self.size * (self.files.len() as u64 - 1)
    }
}

fn hash_file(path: &Path, limit: Option<u64>, progress: &Progress) -> io::Result<[u8; 32]> {
    let file = File::open(path)?;
    let mut reader: Box<dyn Read> = match limit {
        Some(n) => Box::new(file.take(n)),
        None => Box::new(file),
    };
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if progress.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        progress.bytes.fetch_add(n as u64, Ordering::Relaxed);
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Splits each candidate group by hash, dropping files that end up alone.
fn split_by_hash(
    groups: Vec<Vec<DuplicateFile>>,
    limit: Option<u64>,
    progress: &Progress,
) -> Vec<Vec<DuplicateFile>> {
    groups
        .into_par_iter()
        .flat_map(|group| {
            let mut by_hash: HashMap<[u8; 32], Vec<DuplicateFile>> = HashMap::new();
            for file in group {
                // Unreadable files (in use, no permission) are simply left out.
                if let Ok(hash) = hash_file(&file.path, limit, progress) {
                    by_hash.entry(hash).or_default().push(file);
                }
            }
            by_hash.into_values().filter(|g| g.len() > 1).collect::<Vec<_>>()
        })
        .collect()
}

/// Finds groups of identical files of at least `min_size` bytes under `roots`.
///
/// Groups are ordered by wasted space, largest first.
pub fn find_duplicates(roots: &[PathBuf], min_size: u64, progress: &Progress) -> Vec<DuplicateGroup> {
    let min_size = min_size.max(1);
    let mut seen = HashSet::new();
    let mut by_size: HashMap<u64, Vec<DuplicateFile>> = HashMap::new();
    for root in roots {
        for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
            if progress.cancelled.load(Ordering::Relaxed) {
                return Vec::new();
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            progress.files.fetch_add(1, Ordering::Relaxed);
            if meta.len() < min_size || !seen.insert(entry.path().to_path_buf()) {
                continue;
            }
            by_size.entry(meta.len()).or_default().push(DuplicateFile {
                path: entry.into_path(),
                modified: unix_seconds(meta.modified()),
            });
        }
    }

    let candidates: Vec<Vec<DuplicateFile>> = by_size.into_values().filter(|g| g.len() > 1).collect();
    let after_prefix = split_by_hash(candidates, Some(PREFIX_BYTES), progress);
    // Files no bigger than the prefix were already hashed completely.
    let (small, large): (Vec<_>, Vec<_>) = after_prefix
        .into_iter()
        .partition(|g| fs_len(&g[0].path).is_some_and(|len| len <= PREFIX_BYTES));
    let mut groups = small;
    groups.extend(split_by_hash(large, None, progress));
    if progress.cancelled.load(Ordering::Relaxed) {
        return Vec::new();
    }

    let mut result: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter_map(|mut files| {
            let size = fs_len(&files[0].path)?;
            files.sort_by_key(|f| (f.modified, f.path.clone()));
            Some(DuplicateGroup { size, files })
        })
        .collect();
    result.sort_by_key(|g| Reverse(g.wasted_bytes()));
    result
}

fn fs_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// Checks that removing `selected` leaves at least one copy in every group,
/// and that every selected file is part of a group.
pub fn check_keeps_a_copy(groups: &[DuplicateGroup], selected: &[PathBuf]) -> Result<(), String> {
    let selected: HashSet<&PathBuf> = selected.iter().collect();
    let mut known = HashSet::new();
    for group in groups {
        let kept = group.files.iter().filter(|f| !selected.contains(&f.path)).count();
        if kept == 0 {
            return Err(format!(
                "Every copy of {} is selected. Keep at least one.",
                group.files[0].path.display()
            ));
        }
        known.extend(group.files.iter().map(|f| &f.path));
    }
    match selected.iter().find(|p| !known.contains(*p)) {
        Some(p) => Err(format!("{} is not in the duplicate list.", p.display())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn finds_identical_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut big_changed_at_end = big.clone();
        *big_changed_at_end.last_mut().unwrap() ^= 1;
        write(&dir.path().join("a/photo.jpg"), &big);
        write(&dir.path().join("b/photo copy.jpg"), &big);
        // Same size and same first 64 KB, different ending: must not match.
        write(&dir.path().join("c/almost.jpg"), &big_changed_at_end);
        write(&dir.path().join("x.txt"), b"hello");
        write(&dir.path().join("y.txt"), b"hello");
        write(&dir.path().join("z.txt"), b"world");

        let groups = find_duplicates(&[dir.path().to_path_buf()], 1, &Progress::default());

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].size, 200_000);
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].files.iter().all(|f| !f.path.ends_with("almost.jpg")));
        assert_eq!(groups[1].files.len(), 2);
        assert_eq!(groups[0].wasted_bytes(), 200_000);
    }

    #[test]
    fn respects_min_size_and_overlapping_roots() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("sub/x.txt"), b"hello");
        write(&dir.path().join("sub/y.txt"), b"hello");
        let roots = [dir.path().to_path_buf(), dir.path().join("sub")];

        assert!(find_duplicates(&roots, 100, &Progress::default()).is_empty());
        let groups = find_duplicates(&roots, 1, &Progress::default());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2, "a file found via two roots counts once");
    }

    #[test]
    fn deleting_every_copy_is_refused() {
        let group = DuplicateGroup {
            size: 1,
            files: vec![
                DuplicateFile { path: "a".into(), modified: None },
                DuplicateFile { path: "b".into(), modified: None },
            ],
        };
        let groups = [group];
        assert!(check_keeps_a_copy(&groups, &["a".into()]).is_ok());
        assert!(check_keeps_a_copy(&groups, &["a".into(), "b".into()]).is_err());
        assert!(check_keeps_a_copy(&groups, &["elsewhere".into()]).is_err());
    }
}
