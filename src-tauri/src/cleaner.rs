use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use clearwin_core::{
    catalog, clean_category, scan_category_excluding, Category, CategoryClean, CategoryScan,
    CategorySummary, Paths, RunningApps,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Largest files sent to the screen per category for the preview.
const PREVIEW_FILES: usize = 200;

#[derive(Default)]
pub struct CleanerState {
    categories: Arc<Mutex<Vec<Category>>>,
    /// Results of the last scan, by category id. Cleaning only deletes what is in here.
    scans: Arc<Mutex<HashMap<String, CategoryScan>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    done: usize,
    total: usize,
}

fn selected(categories: &[Category], ids: &[String]) -> Vec<Category> {
    categories
        .iter()
        .filter(|c| ids.contains(&c.id))
        .cloned()
        .collect()
}

#[tauri::command]
pub fn list_categories(state: State<'_, CleanerState>) -> Vec<Category> {
    let categories = catalog(&Paths::from_env());
    *state.categories.lock().unwrap() = categories.clone();
    categories
}

#[tauri::command]
pub async fn scan(
    app: AppHandle,
    state: State<'_, CleanerState>,
    ids: Vec<String>,
) -> Result<Vec<CategorySummary>, String> {
    let chosen = selected(&state.categories.lock().unwrap(), &ids);
    let scans = state.scans.clone();
    let excluded = crate::settings::current().exclusions;
    tauri::async_runtime::spawn_blocking(move || {
        let running = RunningApps::snapshot();
        let now = SystemTime::now();
        let mut fresh = HashMap::new();
        let mut summaries = Vec::new();
        for (i, category) in chosen.iter().enumerate() {
            let _ = app.emit(
                "scan-progress",
                Progress {
                    id: category.id.clone(),
                    done: i,
                    total: chosen.len(),
                },
            );
            let scan = scan_category_excluding(category, &running, now, &excluded);
            summaries.push(scan.summary(PREVIEW_FILES));
            fresh.insert(category.id.clone(), scan);
        }
        // A new scan replaces the old one entirely, so stale results can't be cleaned.
        *scans.lock().unwrap() = fresh;
        summaries
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clean(
    state: State<'_, CleanerState>,
    ids: Vec<String>,
) -> Result<Vec<CategoryClean>, String> {
    let chosen = selected(&state.categories.lock().unwrap(), &ids);
    let scans = state.scans.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut scans = scans.lock().unwrap();
        let running = RunningApps::snapshot();
        let now = SystemTime::now();
        let results: Vec<CategoryClean> = chosen
            .iter()
            .filter_map(|category| {
                // Only categories the user has seen a preview for are cleaned.
                let scan = scans.remove(&category.id)?;
                Some(clean_category(category, &scan, &running, now))
            })
            .collect();
        crate::settings::record(
            "Cleaned",
            results.iter().map(|r| r.removed_items).sum(),
            results.iter().map(|r| r.freed_bytes).sum(),
            None,
        );
        results
    })
    .await
    .map_err(|e| e.to_string())
}
