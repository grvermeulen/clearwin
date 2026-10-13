use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use walkdir::WalkDir;

use crate::{firefox, recycle_bin, Action, Category, RunningApps, Target};

/// Most failures listed per category; the rest are only counted.
const MAX_FAILURES_LISTED: usize = 20;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

/// What a scan found for one category: the exact list `clean` may delete.
#[derive(Clone, Debug, Default)]
pub struct CategoryScan {
    pub id: String,
    pub files: Vec<FileEntry>,
    pub total_bytes: u64,
    /// Files, or entries for categories that are not files (history visits, Recycle Bin items).
    pub item_count: u64,
    /// Set when the category cannot be cleaned right now, e.g. the browser is open.
    pub blocked_reason: Option<String>,
}

/// A scan result trimmed for display: totals plus the largest files.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    pub id: String,
    pub total_bytes: u64,
    pub item_count: u64,
    pub blocked_reason: Option<String>,
    pub largest: Vec<FileEntry>,
}

impl CategoryScan {
    pub fn summary(&self, max_files: usize) -> CategorySummary {
        let mut largest = self.files.clone();
        largest.sort_by_key(|f| std::cmp::Reverse(f.size));
        largest.truncate(max_files);
        CategorySummary {
            id: self.id.clone(),
            total_bytes: self.total_bytes,
            item_count: self.item_count,
            blocked_reason: self.blocked_reason.clone(),
            largest,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryClean {
    pub id: String,
    pub removed_items: u64,
    pub freed_bytes: u64,
    /// Items that could not be removed, usually because they are in use or need admin rights.
    pub failed_items: u64,
    pub failures: Vec<String>,
    /// Set when the whole category was skipped.
    pub skipped_reason: Option<String>,
}

fn blocked_reason(category: &Category, running: &RunningApps) -> Option<String> {
    match &category.close_app {
        Some(app) if running.any_running(&category.blocking_processes) => {
            Some(format!("Close {app} to clean this."))
        }
        _ => None,
    }
}

fn old_enough(modified: io::Result<SystemTime>, target: &Target, now: SystemTime) -> bool {
    match (target.min_age, modified) {
        (None, _) => true,
        (Some(age), Ok(modified)) => now
            .duration_since(modified)
            .map(|d| d >= age)
            .unwrap_or(false),
        (Some(_), Err(_)) => false,
    }
}

fn walk(target: &Target) -> WalkDir {
    let walker = WalkDir::new(&target.root).min_depth(1).follow_links(false);
    if target.recursive {
        walker
    } else {
        walker.max_depth(1)
    }
}

/// Lists what cleaning `category` would remove. Deletes nothing.
pub fn scan_category(category: &Category, running: &RunningApps, now: SystemTime) -> CategoryScan {
    let mut scan = CategoryScan {
        id: category.id.clone(),
        blocked_reason: blocked_reason(category, running),
        ..CategoryScan::default()
    };
    match &category.action {
        Action::Files(targets) => {
            for target in targets {
                for entry in walk(target).into_iter().flatten() {
                    // Symlinks and junctions are never followed or removed.
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    if !target.filter.matches(&entry.file_name().to_string_lossy()) {
                        continue;
                    }
                    let Ok(meta) = entry.metadata() else { continue };
                    if !old_enough(meta.modified(), target, now) {
                        continue;
                    }
                    scan.total_bytes += meta.len();
                    scan.files.push(FileEntry {
                        path: entry.into_path(),
                        size: meta.len(),
                    });
                }
            }
            scan.item_count = scan.files.len() as u64;
        }
        Action::RecycleBin => {
            let (items, bytes) = recycle_bin::query();
            scan.item_count = items;
            scan.total_bytes = bytes;
        }
        Action::FirefoxHistory(dbs) => {
            // While Firefox runs the database is locked, so the count may be unavailable.
            scan.item_count = dbs
                .iter()
                .filter_map(|db| firefox::count_visits(db).ok())
                .sum();
        }
    }
    scan
}

/// True when `path` sits inside one of the category's folders.
fn within_targets(path: &Path, targets: &[Target]) -> bool {
    targets
        .iter()
        .any(|t| path.starts_with(&t.root) && path != t.root)
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            // Read-only files cannot be deleted on Windows until the flag is cleared.
            let mut perms = fs::metadata(path)?.permissions();
            if !perms.readonly() {
                return Err(e);
            }
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(path, perms)?;
            fs::remove_file(path)
        }
        other => other,
    }
}

/// Folders below each recursive target that are old enough to remove once empty.
///
/// Collected before deleting, because deleting files updates a folder's modified time.
fn removable_dirs(targets: &[Target], now: SystemTime) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for target in targets.iter().filter(|t| t.recursive) {
        // Deepest first, so a parent is tried after its children are gone.
        for entry in walk(target).contents_first(true).into_iter().flatten() {
            let modified = entry
                .metadata()
                .map_err(io::Error::from)
                .and_then(|m| m.modified());
            if entry.file_type().is_dir() && old_enough(modified, target, now) {
                dirs.push(entry.into_path());
            }
        }
    }
    dirs
}

fn record_failure(result: &mut CategoryClean, what: &Path, err: &io::Error) {
    result.failed_items += 1;
    if result.failures.len() < MAX_FAILURES_LISTED {
        result.failures.push(format!("{}: {err}", what.display()));
    }
}

/// Deletes what `scan` found for `category`, and nothing else.
///
/// Each file is re-checked first: it must still be a regular file inside the
/// category's folders. Files that changed type or moved are left alone.
pub fn clean_category(
    category: &Category,
    scan: &CategoryScan,
    running: &RunningApps,
    now: SystemTime,
) -> CategoryClean {
    let mut result = CategoryClean {
        id: category.id.clone(),
        ..CategoryClean::default()
    };
    if let Some(reason) = blocked_reason(category, running) {
        result.skipped_reason = Some(reason);
        return result;
    }
    match &category.action {
        Action::Files(targets) => {
            let dirs = removable_dirs(targets, now);
            for file in &scan.files {
                if !within_targets(&file.path, targets) {
                    continue;
                }
                match fs::symlink_metadata(&file.path) {
                    Ok(meta) if meta.file_type().is_file() => {}
                    Ok(_) => continue,
                    // Already gone counts as done.
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        record_failure(&mut result, &file.path, &e);
                        continue;
                    }
                }
                match remove_file(&file.path) {
                    Ok(()) => {
                        result.removed_items += 1;
                        result.freed_bytes += file.size;
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => record_failure(&mut result, &file.path, &e),
                }
            }
            for dir in dirs {
                // Fails harmlessly when the folder is not empty.
                let _ = fs::remove_dir(dir);
            }
        }
        Action::RecycleBin => match recycle_bin::empty() {
            Ok(()) => {
                result.removed_items = scan.item_count;
                result.freed_bytes = scan.total_bytes;
            }
            Err(e) => record_failure(&mut result, Path::new("Recycle Bin"), &e),
        },
        Action::FirefoxHistory(dbs) => {
            for db in dbs {
                match firefox::clear_history(db) {
                    Ok(removed) => result.removed_items += removed,
                    Err(e) => record_failure(&mut result, db, &io::Error::other(e)),
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::NameFilter;

    fn category(targets: Vec<Target>) -> Category {
        Category {
            id: "test.files".into(),
            group: "Test".into(),
            name: "Test".into(),
            description: String::new(),
            default_selected: true,
            needs_admin: false,
            close_app: None,
            blocking_processes: Vec::new(),
            action: Action::Files(targets),
        }
    }

    fn tree(root: &Path) -> Target {
        Target {
            root: root.to_path_buf(),
            recursive: true,
            filter: NameFilter::Any,
            min_age: None,
        }
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn scan_lists_files_and_deletes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.tmp"), 10);
        write(&dir.path().join("sub/b.tmp"), 20);
        let cat = category(vec![tree(dir.path())]);

        let scan = scan_category(&cat, &RunningApps::default(), SystemTime::now());

        assert_eq!(scan.item_count, 2);
        assert_eq!(scan.total_bytes, 30);
        assert!(dir.path().join("sub/b.tmp").exists());
    }

    #[test]
    fn clean_removes_scanned_files_and_empty_folders_only() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.tmp"), 10);
        write(&dir.path().join("sub/b.tmp"), 20);
        let cat = category(vec![tree(dir.path())]);
        let now = SystemTime::now();
        let scan = scan_category(&cat, &RunningApps::default(), now);
        // Created after the scan, so the user never saw it: it must survive.
        write(&dir.path().join("new.tmp"), 5);

        let result = clean_category(&cat, &scan, &RunningApps::default(), now);

        assert_eq!(result.removed_items, 2);
        assert_eq!(result.freed_bytes, 30);
        assert_eq!(result.failed_items, 0);
        assert!(dir.path().exists(), "target folder itself is kept");
        assert!(
            !dir.path().join("sub").exists(),
            "emptied subfolder is removed"
        );
        assert!(dir.path().join("new.tmp").exists());
    }

    #[test]
    fn clean_refuses_paths_outside_targets() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("keep.txt");
        write(&outside, 1);
        let cat = category(vec![tree(&dir.path().join("temp"))]);
        let scan = CategoryScan {
            id: cat.id.clone(),
            files: vec![FileEntry {
                path: outside.clone(),
                size: 1,
            }],
            item_count: 1,
            total_bytes: 1,
            blocked_reason: None,
        };

        let result = clean_category(&cat, &scan, &RunningApps::default(), SystemTime::now());

        assert_eq!(result.removed_items, 0);
        assert!(outside.exists());
    }

    #[test]
    fn recent_files_are_kept_when_min_age_is_set() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("fresh.tmp"), 10);
        let mut target = tree(dir.path());
        target.min_age = Some(Duration::from_secs(3600));
        let cat = category(vec![target]);

        let scan = scan_category(&cat, &RunningApps::default(), SystemTime::now());
        assert_eq!(scan.item_count, 0);

        let later = SystemTime::now() + Duration::from_secs(7200);
        assert_eq!(
            scan_category(&cat, &RunningApps::default(), later).item_count,
            1
        );
    }

    #[test]
    fn name_filter_and_depth_are_respected() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("History"), 10);
        write(&dir.path().join("Bookmarks"), 10);
        write(&dir.path().join("Nested/History"), 10);
        let target = Target {
            root: dir.path().to_path_buf(),
            recursive: false,
            filter: NameFilter::Names(&["History"]),
            min_age: None,
        };
        let cat = category(vec![target]);

        let scan = scan_category(&cat, &RunningApps::default(), SystemTime::now());

        assert_eq!(scan.item_count, 1);
        assert_eq!(scan.files[0].path, dir.path().join("History"));
    }

    #[test]
    fn open_browser_blocks_cleaning() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("cache.bin"), 10);
        let mut cat = category(vec![tree(dir.path())]);
        cat.close_app = Some("Google Chrome".into());
        cat.blocking_processes = vec!["chrome.exe"];
        let running = RunningApps::from_names(["Chrome.exe"]);
        let now = SystemTime::now();

        let scan = scan_category(&cat, &running, now);
        assert_eq!(
            scan.blocked_reason.as_deref(),
            Some("Close Google Chrome to clean this.")
        );

        let result = clean_category(&cat, &scan, &running, now);
        assert!(result.skipped_reason.is_some());
        assert!(dir.path().join("cache.bin").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write(&elsewhere.path().join("precious.txt"), 10);
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("link")).unwrap();
        let cat = category(vec![tree(dir.path())]);
        let now = SystemTime::now();

        let scan = scan_category(&cat, &RunningApps::default(), now);
        assert_eq!(scan.item_count, 0);

        clean_category(&cat, &scan, &RunningApps::default(), now);
        assert!(elsewhere.path().join("precious.txt").exists());
    }

    #[test]
    fn summary_keeps_largest_files() {
        let scan = CategoryScan {
            id: "x".into(),
            files: (1..=5)
                .map(|i| FileEntry {
                    path: PathBuf::from(format!("{i}")),
                    size: i,
                })
                .collect(),
            ..CategoryScan::default()
        };
        let sizes: Vec<u64> = scan.summary(2).largest.iter().map(|f| f.size).collect();
        assert_eq!(sizes, vec![5, 4]);
    }
}
