use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::Paths;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// One thing the user can tick in the cleaner, such as "Chrome cache".
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    /// Stable id, e.g. `chrome.cache`.
    pub id: String,
    /// Heading the category is listed under, e.g. `Google Chrome`.
    pub group: String,
    pub name: String,
    pub description: String,
    pub default_selected: bool,
    /// Most files here can only be removed when ClearWin runs as administrator.
    pub needs_admin: bool,
    /// Friendly name of the app that must be closed first, if any.
    pub close_app: Option<String>,
    #[serde(skip)]
    pub blocking_processes: Vec<&'static str>,
    #[serde(skip)]
    pub action: Action,
}

#[derive(Clone, Debug)]
pub enum Action {
    /// Delete files matched by these targets.
    Files(Vec<Target>),
    /// Empty the Windows Recycle Bin on every drive.
    RecycleBin,
    /// Remove visits from these Firefox `places.sqlite` databases, keeping bookmarks.
    FirefoxHistory(Vec<PathBuf>),
}

/// A folder whose matching files belong to a category.
#[derive(Clone, Debug)]
pub struct Target {
    pub root: PathBuf,
    /// Include files in subfolders, not just directly inside `root`.
    pub recursive: bool,
    pub filter: NameFilter,
    /// Skip files modified more recently than this, so in-flight temp files survive.
    pub min_age: Option<Duration>,
}

#[derive(Clone, Debug)]
pub enum NameFilter {
    Any,
    /// File name equals one of these (case-insensitive).
    Names(&'static [&'static str]),
    /// File name starts with this prefix and ends with this suffix (case-insensitive).
    Affix(&'static str, &'static str),
}

impl NameFilter {
    pub fn matches(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        match self {
            NameFilter::Any => true,
            NameFilter::Names(names) => names.iter().any(|n| n.to_lowercase() == name),
            NameFilter::Affix(prefix, suffix) => {
                name.starts_with(&prefix.to_lowercase()) && name.ends_with(&suffix.to_lowercase())
            }
        }
    }
}

impl Target {
    fn tree(root: PathBuf) -> Self {
        Self {
            root,
            recursive: true,
            filter: NameFilter::Any,
            min_age: None,
        }
    }

    fn files(root: PathBuf, filter: NameFilter) -> Self {
        Self {
            root,
            recursive: false,
            filter,
            min_age: None,
        }
    }

    fn older_than(mut self, age: Duration) -> Self {
        self.min_age = Some(age);
        self
    }
}

struct Builder {
    group: &'static str,
    close_app: Option<(&'static str, Vec<&'static str>)>,
    out: Vec<Category>,
}

impl Builder {
    fn new(group: &'static str) -> Self {
        Self {
            group,
            close_app: None,
            out: Vec::new(),
        }
    }

    fn close_first(mut self, app: &'static str, processes: Vec<&'static str>) -> Self {
        self.close_app = Some((app, processes));
        self
    }

    fn add(
        &mut self,
        id: &str,
        name: &str,
        description: &str,
        default_selected: bool,
        needs_admin: bool,
        action: Action,
    ) {
        if let Action::Files(targets) = &action {
            if targets.is_empty() {
                return;
            }
        }
        if let Action::FirefoxHistory(dbs) = &action {
            if dbs.is_empty() {
                return;
            }
        }
        let (close_app, blocking_processes) = match &self.close_app {
            Some((app, procs)) => (Some(app.to_string()), procs.clone()),
            None => (None, Vec::new()),
        };
        self.out.push(Category {
            id: id.to_string(),
            group: self.group.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            default_selected,
            needs_admin,
            close_app,
            blocking_processes,
            action,
        });
    }
}

/// Every category available on this machine, in display order.
///
/// Browsers that are not installed are left out.
pub fn catalog(paths: &Paths) -> Vec<Category> {
    let mut out = windows_categories(paths);
    if let Some(local) = &paths.local_app_data {
        out.extend(chromium_categories(
            "Google Chrome",
            "chrome",
            &local.join(r"Google\Chrome\User Data"),
            vec!["chrome.exe"],
        ));
        out.extend(chromium_categories(
            "Microsoft Edge",
            "edge",
            &local.join(r"Microsoft\Edge\User Data"),
            vec!["msedge.exe"],
        ));
        out.extend(chromium_categories(
            "Brave",
            "brave",
            &local.join(r"BraveSoftware\Brave-Browser\User Data"),
            vec!["brave.exe"],
        ));
    }
    out.extend(firefox_categories(paths));
    out
}

fn windows_categories(paths: &Paths) -> Vec<Category> {
    let mut b = Builder::new("Windows");
    let local = paths.local_app_data.as_ref();
    let windows = paths.windows.as_ref();

    b.add(
        "windows.user_temp",
        "Temporary files",
        "Files apps left behind in your temp folder. Files changed in the last 24 hours are kept.",
        true,
        false,
        Action::Files(
            paths
                .temp
                .iter()
                .map(|t| Target::tree(t.clone()).older_than(DAY))
                .collect(),
        ),
    );
    b.add(
        "windows.system_temp",
        "Windows temporary files",
        "The shared Windows temp folder. Files changed in the last 24 hours are kept.",
        true,
        true,
        Action::Files(
            windows
                .iter()
                .map(|w| Target::tree(w.join("Temp")).older_than(DAY))
                .collect(),
        ),
    );
    if cfg!(windows) {
        b.add(
            "windows.recycle_bin",
            "Recycle Bin",
            "Permanently removes everything in the Recycle Bin on all drives.",
            true,
            false,
            Action::RecycleBin,
        );
    }
    b.add(
        "windows.thumbnails",
        "Thumbnail cache",
        "Picture previews File Explorer saves. Windows rebuilds them when needed.",
        true,
        false,
        Action::Files(
            local
                .iter()
                .map(|l| {
                    Target::files(
                        l.join(r"Microsoft\Windows\Explorer"),
                        NameFilter::Affix("thumbcache_", ".db"),
                    )
                })
                .collect(),
        ),
    );
    b.add(
        "windows.shader_cache",
        "DirectX shader cache",
        "Graphics data games and apps rebuild on their next run.",
        true,
        false,
        Action::Files(
            local
                .iter()
                .map(|l| Target::tree(l.join("D3DSCache")))
                .collect(),
        ),
    );
    let mut crash = Vec::new();
    if let Some(l) = local {
        crash.push(Target::tree(l.join("CrashDumps")));
    }
    if let Some(w) = windows {
        crash.push(Target::tree(w.join("Minidump")));
        crash.push(Target::files(
            w.to_path_buf(),
            NameFilter::Names(&["MEMORY.DMP"]),
        ));
    }
    if let Some(p) = &paths.program_data {
        crash.push(Target::tree(p.join(r"Microsoft\Windows\WER\ReportArchive")));
        crash.push(Target::tree(p.join(r"Microsoft\Windows\WER\ReportQueue")));
    }
    b.add(
        "windows.crash_dumps",
        "Crash dumps and error reports",
        "Memory dumps and error reports saved after apps or Windows crashed.",
        true,
        false,
        Action::Files(crash),
    );
    b.add(
        "windows.update_cache",
        "Windows Update downloads",
        "Update files Windows has already installed. Windows downloads them again if needed.",
        false,
        true,
        Action::Files(
            windows
                .iter()
                .map(|w| Target::tree(w.join(r"SoftwareDistribution\Download")))
                .collect(),
        ),
    );
    b.out
}

/// Profile folders inside a Chromium `User Data` folder ("Default", "Profile 1", ...).
fn chromium_profiles(user_data: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(user_data) else {
        return Vec::new();
    };
    let mut profiles: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name == "Default" || name.starts_with("Profile ") || name == "Guest Profile"
        })
        .map(|e| e.path())
        .collect();
    profiles.sort();
    profiles
}

fn chromium_categories(
    group: &'static str,
    key: &str,
    user_data: &Path,
    processes: Vec<&'static str>,
) -> Vec<Category> {
    let profiles = chromium_profiles(user_data);
    if profiles.is_empty() {
        return Vec::new();
    }
    let mut b = Builder::new(group).close_first(group, processes);

    let cache = profiles
        .iter()
        .flat_map(|p| {
            [
                "Cache",
                "Code Cache",
                "GPUCache",
                r"Service Worker\CacheStorage",
            ]
            .map(|d| Target::tree(p.join(d)))
        })
        .collect();
    b.add(
        &format!("{key}.cache"),
        "Cache",
        "Copies of web pages and images saved to load sites faster.",
        true,
        false,
        Action::Files(cache),
    );

    const HISTORY: &[&str] = &[
        "History",
        "History-journal",
        "Visited Links",
        "Top Sites",
        "Top Sites-journal",
        "Shortcuts",
        "Shortcuts-journal",
        "Network Action Predictor",
        "Network Action Predictor-journal",
    ];
    b.add(
        &format!("{key}.history"),
        "History",
        "The list of sites you visited and downloads you made. Bookmarks are kept.",
        false,
        false,
        Action::Files(
            profiles
                .iter()
                .map(|p| Target::files(p.clone(), NameFilter::Names(HISTORY)))
                .collect(),
        ),
    );

    const COOKIES: &[&str] = &["Cookies", "Cookies-journal"];
    b.add(
        &format!("{key}.cookies"),
        "Cookies",
        "Signs you out of websites. Sites will ask you to log in again.",
        false,
        false,
        Action::Files(
            profiles
                .iter()
                .flat_map(|p| {
                    [
                        Target::files(p.join("Network"), NameFilter::Names(COOKIES)),
                        Target::files(p.clone(), NameFilter::Names(COOKIES)),
                    ]
                })
                .collect(),
        ),
    );
    b.out
}

/// Firefox profile folder names under `Profiles`, which match between Roaming and Local.
fn firefox_profile_names(profiles_root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(profiles_root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

fn firefox_categories(paths: &Paths) -> Vec<Category> {
    let Some(roaming) = &paths.app_data else {
        return Vec::new();
    };
    let roaming_profiles = roaming.join(r"Mozilla\Firefox\Profiles");
    let names = firefox_profile_names(&roaming_profiles);
    if names.is_empty() {
        return Vec::new();
    }
    let mut b = Builder::new("Mozilla Firefox").close_first("Mozilla Firefox", vec!["firefox.exe"]);

    let mut cache = Vec::new();
    if let Some(local) = &paths.local_app_data {
        let local_profiles = local.join(r"Mozilla\Firefox\Profiles");
        for name in &names {
            for dir in ["cache2", "startupCache", "thumbnails"] {
                cache.push(Target::tree(local_profiles.join(name).join(dir)));
            }
        }
    }
    b.add(
        "firefox.cache",
        "Cache",
        "Copies of web pages and images saved to load sites faster.",
        true,
        false,
        Action::Files(cache),
    );

    let places = names
        .iter()
        .map(|n| roaming_profiles.join(n).join("places.sqlite"))
        .filter(|p| p.is_file())
        .collect();
    b.add(
        "firefox.history",
        "History",
        "The list of sites you visited. Bookmarks are kept.",
        false,
        false,
        Action::FirefoxHistory(places),
    );

    const COOKIES: &[&str] = &["cookies.sqlite", "cookies.sqlite-wal", "cookies.sqlite-shm"];
    b.add(
        "firefox.cookies",
        "Cookies",
        "Signs you out of websites. Sites will ask you to log in again.",
        false,
        false,
        Action::Files(
            names
                .iter()
                .map(|n| Target::files(roaming_profiles.join(n), NameFilter::Names(COOKIES)))
                .collect(),
        ),
    );
    b.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_filters_ignore_case() {
        assert!(NameFilter::Affix("thumbcache_", ".db").matches("THUMBCACHE_256.DB"));
        assert!(!NameFilter::Affix("thumbcache_", ".db").matches("iconcache_256.db"));
        assert!(NameFilter::Names(&["History"]).matches("history"));
        assert!(!NameFilter::Names(&["History"]).matches("History Provider Cache"));
    }

    #[test]
    fn browsers_only_listed_when_installed() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("Local");
        fs::create_dir_all(local.join(r"Google\Chrome\User Data").join("Default")).unwrap();
        let paths = Paths {
            local_app_data: Some(local),
            ..Paths::default()
        };

        let ids: Vec<String> = catalog(&paths).into_iter().map(|c| c.id).collect();
        assert!(ids.contains(&"chrome.cache".to_string()));
        assert!(!ids
            .iter()
            .any(|id| id.starts_with("edge.") || id.starts_with("firefox.")));
    }

    #[test]
    fn categories_without_folders_are_omitted() {
        let ids: Vec<String> = catalog(&Paths::default())
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert!(!ids.contains(&"windows.user_temp".to_string()));
    }
}
