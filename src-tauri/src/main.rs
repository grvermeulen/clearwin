// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cleaner;
mod elevation;
mod performance;
mod security;
mod settings;
mod storage;
mod tools;
mod updates;

fn main() {
    // Started by the scheduled task: clean in the background, show nothing.
    if std::env::args().any(|a| a == clearwin_core::schedule::AUTO_CLEAN_FLAG) {
        clearwin_core::schedule::run_auto_clean(&clearwin_core::settings::config_dir());
        return;
    }
    // Started at sign-in when "clean browsers when they close" is on.
    if std::env::args().any(|a| a == clearwin_core::monitor::MONITOR_FLAG) {
        clearwin_core::monitor::run_monitor(&clearwin_core::settings::config_dir());
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(cleaner::CleanerState::default())
        .manage(storage::StorageState::default())
        .manage(tools::ToolsState::default())
        .invoke_handler(tauri::generate_handler![
            cleaner::list_categories,
            cleaner::scan,
            cleaner::clean,
            cleaner::measure_junk,
            cleaner::clean_junk_now,
            elevation::is_admin,
            elevation::restart_as_admin,
            storage::list_drives,
            storage::home_folder,
            storage::cloud_folders,
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
            tools::app_details,
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
            settings::get_settings,
            settings::set_cleaner_selection,
            settings::set_exclusions,
            settings::set_schedule,
            settings::get_history,
            settings::set_browser_close_cleaning,
            updates::list_software_updates,
            updates::update_software,
            updates::scan_drivers,
            updates::install_drivers,
            performance::system_info,
            performance::list_power_plans,
            performance::set_power_plan,
            performance::heavy_apps,
            performance::end_app,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start ClearWin");
}
