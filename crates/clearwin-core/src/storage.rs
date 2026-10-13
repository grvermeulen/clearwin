//! Where does the space go: folder sizes, the largest files, and drives.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use rayon::prelude::*;
use serde::Serialize;

/// How many of the largest files a folder analysis keeps.
pub const LARGEST_FILES: usize = 500;

/// Shared between a running analysis and whoever watches it.
#[derive(Debug, Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub cancelled: AtomicBool,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// A folder and the total size of everything below it.
#[derive(Clone, Debug, Default)]
pub struct DirNode {
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    /// Bytes in files directly inside this folder (not in subfolders).
    pub own_file_bytes: u64,
    pub own_file_count: u64,
    /// Subfolders, largest first.
    pub children: Vec<DirNode>,
    /// The folder or something inside it could not be read.
    pub incomplete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    // Field order matters: files are ordered by size first.
    pub size: u64,
    pub path: PathBuf,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
}

pub struct FolderAnalysis {
    pub root: PathBuf,
    pub tree: DirNode,
    /// Largest files, biggest first.
    pub largest: Vec<LargeFile>,
}

pub fn unix_seconds(time: std::io::Result<SystemTime>) -> Option<u64> {
    time.ok()?.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// Keeps the N largest files seen across all threads.
struct TopFiles {
    limit: usize,
    heap: Mutex<BinaryHeap<Reverse<LargeFile>>>,
}

impl TopFiles {
    fn offer(&self, file: LargeFile) {
        let mut heap = self.heap.lock().unwrap();
        if heap.len() < self.limit {
            heap.push(Reverse(file));
        } else if heap.peek().is_some_and(|Reverse(smallest)| file.size > smallest.size) {
            heap.pop();
            heap.push(Reverse(file));
        }
    }

    fn into_sorted(self) -> Vec<LargeFile> {
        let mut files: Vec<LargeFile> =
            self.heap.into_inner().unwrap().into_iter().map(|Reverse(f)| f).collect();
        files.sort_by(|a, b| b.cmp(a));
        files
    }
}

fn analyze_dir(path: &Path, name: String, top: &TopFiles, progress: &Progress) -> DirNode {
    let mut node = DirNode { name, ..DirNode::default() };
    if progress.is_cancelled() {
        node.incomplete = true;
        return node;
    }
    let Ok(entries) = fs::read_dir(path) else {
        node.incomplete = true;
        return node;
    };
    let mut subdirs = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            node.incomplete = true;
            continue;
        };
        // Does not follow links: symlinks and junctions are neither counted nor entered,
        // so nothing is counted twice and the walk cannot loop.
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            subdirs.push(entry);
        } else if file_type.is_file() {
            let Ok(meta) = entry.metadata() else { continue };
            node.own_file_bytes += meta.len();
            node.own_file_count += 1;
            progress.files.fetch_add(1, Ordering::Relaxed);
            progress.bytes.fetch_add(meta.len(), Ordering::Relaxed);
            top.offer(LargeFile {
                size: meta.len(),
                path: entry.path(),
                modified: unix_seconds(meta.modified()),
            });
        }
    }
    node.children = subdirs
        .par_iter()
        .map(|e| analyze_dir(&e.path(), e.file_name().to_string_lossy().into_owned(), top, progress))
        .collect();
    node.children.sort_by_key(|c| Reverse(c.size));
    node.size = node.own_file_bytes + node.children.iter().map(|c| c.size).sum::<u64>();
    node.file_count = node.own_file_count + node.children.iter().map(|c| c.file_count).sum::<u64>();
    node.incomplete |= node.children.iter().any(|c| c.incomplete);
    node
}

/// Measures every folder below `root` and collects the largest files.
pub fn analyze_folder(root: &Path, progress: &Progress) -> FolderAnalysis {
    let top = TopFiles { limit: LARGEST_FILES, heap: Mutex::new(BinaryHeap::new()) };
    let name = root.to_string_lossy().into_owned();
    let tree = analyze_dir(root, name, &top, progress);
    FolderAnalysis { root: root.to_path_buf(), tree, largest: top.into_sorted() }
}

/// One entry in a folder view: a subfolder, or all files directly in the folder.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelEntry {
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    /// False for the "files in this folder" entry.
    pub is_dir: bool,
    pub has_children: bool,
    pub incomplete: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub path: PathBuf,
    pub size: u64,
    pub file_count: u64,
    pub entries: Vec<LevelEntry>,
}

impl FolderAnalysis {
    /// The folder reached by following `segments` (subfolder names) from the root.
    pub fn level(&self, segments: &[String]) -> Option<Level> {
        let mut node = &self.tree;
        let mut path = self.root.clone();
        for segment in segments {
            node = node.children.iter().find(|c| &c.name == segment)?;
            path.push(segment);
        }
        let mut entries: Vec<LevelEntry> = node
            .children
            .iter()
            .map(|c| LevelEntry {
                name: c.name.clone(),
                size: c.size,
                file_count: c.file_count,
                is_dir: true,
                has_children: !c.children.is_empty() || c.own_file_count > 0,
                incomplete: c.incomplete,
            })
            .collect();
        if node.own_file_count > 0 {
            entries.push(LevelEntry {
                name: "Files in this folder".into(),
                size: node.own_file_bytes,
                file_count: node.own_file_count,
                is_dir: false,
                has_children: false,
                incomplete: false,
            });
        }
        entries.sort_by_key(|e| Reverse(e.size));
        Some(Level { path, size: node.size, file_count: node.file_count, entries })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    pub mount_point: PathBuf,
    pub name: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

pub fn drives() -> Vec<Drive> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut drives: Vec<Drive> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| Drive {
            mount_point: d.mount_point().to_path_buf(),
            name: d.name().to_string_lossy().into_owned(),
            total_bytes: d.total_space(),
            free_bytes: d.available_space(),
        })
        .collect();
    drives.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    drives.dedup_by(|a, b| a.mount_point == b.mount_point);
    drives
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn sizes_roll_up_and_children_are_sorted() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.bin"), 5);
        write(&dir.path().join("small/x.bin"), 10);
        write(&dir.path().join("big/y.bin"), 100);
        write(&dir.path().join("big/deeper/z.bin"), 50);

        let analysis = analyze_folder(dir.path(), &Progress::default());

        assert_eq!(analysis.tree.size, 165);
        assert_eq!(analysis.tree.file_count, 4);
        let names: Vec<_> = analysis.tree.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["big", "small"]);
        assert_eq!(analysis.tree.children[0].size, 150);

        let sizes: Vec<u64> = analysis.largest.iter().map(|f| f.size).collect();
        assert_eq!(sizes, vec![100, 50, 10, 5]);
    }

    #[test]
    fn level_navigation_includes_loose_files() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.bin"), 5);
        write(&dir.path().join("big/deeper/z.bin"), 50);
        let analysis = analyze_folder(dir.path(), &Progress::default());

        let top = analysis.level(&[]).unwrap();
        assert_eq!(top.entries.len(), 2);
        assert!(top.entries[0].is_dir);
        assert!(!top.entries[1].is_dir);

        let big = analysis.level(&["big".into()]).unwrap();
        assert_eq!(big.path, dir.path().join("big"));
        assert_eq!(big.entries[0].name, "deeper");
        assert!(analysis.level(&["missing".into()]).is_none());
    }

    #[test]
    fn top_files_keeps_only_the_largest() {
        let top = TopFiles { limit: 2, heap: Mutex::new(BinaryHeap::new()) };
        for size in [3, 9, 1, 7] {
            top.offer(LargeFile { size, path: PathBuf::from(size.to_string()), modified: None });
        }
        let sizes: Vec<u64> = top.into_sorted().iter().map(|f| f.size).collect();
        assert_eq!(sizes, vec![9, 7]);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_counted() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        write(&other.path().join("huge.bin"), 1000);
        std::os::unix::fs::symlink(other.path(), dir.path().join("link")).unwrap();

        assert_eq!(analyze_folder(dir.path(), &Progress::default()).tree.size, 0);
    }

    #[test]
    fn cancelled_analysis_is_marked_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.bin"), 5);
        let progress = Progress::default();
        progress.cancel();
        assert!(analyze_folder(dir.path(), &progress).tree.incomplete);
    }
}
