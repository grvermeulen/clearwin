use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clearwin_core::duplicates::{self, DuplicateGroup};
use clearwin_core::files::{self, RecycleReport};
use clearwin_core::storage::{
    self, CloudFolder, Drive, FolderAnalysis, LargeFile, Level, Progress,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct StorageState {
    analysis: Arc<Mutex<Option<FolderAnalysis>>>,
    duplicates: Arc<Mutex<Vec<DuplicateGroup>>>,
    /// Progress of the job that is running now, so it can be cancelled.
    current: Mutex<Arc<Progress>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    files: u64,
    bytes: u64,
}

/// Sends progress to the screen a few times a second until `done` is set.
fn report_progress(app: AppHandle, progress: Arc<Progress>, done: Arc<AtomicBool>) {
    thread::spawn(move || {
        while !done.load(Ordering::Relaxed) {
            let _ = app.emit(
                "storage-progress",
                ProgressEvent {
                    files: progress.files.load(Ordering::Relaxed),
                    bytes: progress.bytes.load(Ordering::Relaxed),
                },
            );
            thread::sleep(Duration::from_millis(300));
        }
    });
}

/// Runs a storage job off the main thread, cancelling any job already running.
async fn run_job<T: Send + 'static>(
    app: AppHandle,
    state: &StorageState,
    job: impl FnOnce(&Progress) -> T + Send + 'static,
) -> Result<T, String> {
    let progress = Arc::new(Progress::default());
    {
        let mut current = state.current.lock().unwrap();
        current.cancel();
        *current = progress.clone();
    }
    let done = Arc::new(AtomicBool::new(false));
    report_progress(app, progress.clone(), done.clone());
    let result = tauri::async_runtime::spawn_blocking(move || job(&progress)).await;
    done.store(true, Ordering::Relaxed);
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_drives() -> Vec<Drive> {
    storage::drives()
}

#[tauri::command]
pub fn cloud_folders() -> Vec<CloudFolder> {
    storage::cloud_folders()
}

#[tauri::command]
pub fn home_folder() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[tauri::command]
pub fn cancel_storage(state: State<'_, StorageState>) {
    state.current.lock().unwrap().cancel();
}

#[tauri::command]
pub async fn analyze_storage(
    app: AppHandle,
    state: State<'_, StorageState>,
    path: PathBuf,
) -> Result<Level, String> {
    if !path.is_dir() {
        return Err(format!("{} is not a folder.", path.display()));
    }
    let analysis = run_job(app, &state, move |p| storage::analyze_folder(&path, p)).await?;
    let level = analysis.level(&[]).ok_or("Analysis failed.")?;
    *state.analysis.lock().unwrap() = Some(analysis);
    Ok(level)
}

#[tauri::command]
pub fn storage_level(
    state: State<'_, StorageState>,
    segments: Vec<String>,
) -> Result<Level, String> {
    let analysis = state.analysis.lock().unwrap();
    let analysis = analysis.as_ref().ok_or("Analyze a folder first.")?;
    analysis
        .level(&segments)
        .ok_or_else(|| "That folder is no longer in the results.".into())
}

#[tauri::command]
pub fn largest_files(state: State<'_, StorageState>) -> Vec<LargeFile> {
    match state.analysis.lock().unwrap().as_ref() {
        Some(analysis) => analysis.largest.clone(),
        None => Vec::new(),
    }
}

#[tauri::command]
pub async fn find_duplicates(
    app: AppHandle,
    state: State<'_, StorageState>,
    roots: Vec<PathBuf>,
    min_size: u64,
) -> Result<Vec<DuplicateGroup>, String> {
    let groups = run_job(app, &state, move |p| {
        duplicates::find_duplicates(&roots, min_size, p)
    })
    .await?;
    *state.duplicates.lock().unwrap() = groups.clone();
    Ok(groups)
}

/// Moves selected duplicates to the Recycle Bin, refusing to remove the last copy of anything.
#[tauri::command]
pub fn recycle_duplicates(
    state: State<'_, StorageState>,
    paths: Vec<PathBuf>,
) -> Result<RecycleReport, String> {
    let mut groups = state.duplicates.lock().unwrap();
    duplicates::check_keeps_a_copy(&groups, &paths)?;
    let report = files::move_to_recycle_bin(&paths);
    crate::settings::record(
        "Duplicates moved to Recycle Bin",
        report.moved.len() as u64,
        0,
        None,
    );
    for group in groups.iter_mut() {
        group.files.retain(|f| !report.moved.contains(&f.path));
    }
    groups.retain(|g| g.files.len() > 1);
    Ok(report)
}

/// Moves files from the largest-files list to the Recycle Bin.
#[tauri::command]
pub fn recycle_large_files(
    state: State<'_, StorageState>,
    paths: Vec<PathBuf>,
) -> Result<RecycleReport, String> {
    let mut analysis = state.analysis.lock().unwrap();
    let analysis = analysis.as_mut().ok_or("Analyze a folder first.")?;
    if let Some(p) = paths
        .iter()
        .find(|p| !analysis.largest.iter().any(|f| &f.path == *p))
    {
        return Err(format!("{} is not in the largest-files list.", p.display()));
    }
    let report = files::move_to_recycle_bin(&paths);
    crate::settings::record(
        "Large files moved to Recycle Bin",
        report.moved.len() as u64,
        0,
        None,
    );
    analysis.largest.retain(|f| !report.moved.contains(&f.path));
    Ok(report)
}

#[tauri::command]
pub fn reveal(path: PathBuf) -> Result<(), String> {
    files::reveal_in_explorer(&path).map_err(|e| e.to_string())
}
