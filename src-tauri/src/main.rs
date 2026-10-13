// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cleaner;
mod elevation;
mod security;
mod storage;
mod tools;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(cleaner::CleanerState::default())
        .manage(storage::StorageState::default())
        .manage(tools::ToolsState::default())
        .invoke_handler(tauri::generate_handler![
            cleaner::list_categories,
            cleaner::scan,
            cleaner::clean,
            elevation::is_admin,
            elevation::restart_as_admin,
            storage::list_drives,
            storage::home_folder,
            storage::cancel_storage,
            storage::analyze_storage,
            storage::storage_level,
            storage::largest_files,
            storage::find_duplicates,
            storage::recycle_duplicates,
            storage::recycle_large_files,
            storage::reveal,
            tools::list_startup,
            tools::set_startup_enabled,
            tools::list_apps,
            tools::uninstall_app,
            tools::scan_registry,
            tools::fix_registry,
            tools::list_registry_backups,
            tools::restore_registry_backup,
            security::defender_status,
            security::defender_threats,
            security::defender_scan,
            security::defender_update,
            security::open_windows_security,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start ClearWin");
}
