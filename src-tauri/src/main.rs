// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cleaner;
mod elevation;

fn main() {
    tauri::Builder::default()
        .manage(cleaner::CleanerState::default())
        .invoke_handler(tauri::generate_handler![
            cleaner::list_categories,
            cleaner::scan,
            cleaner::clean,
            elevation::is_admin,
            elevation::restart_as_admin,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start ClearWin");
}
