mod commands;
mod preview;
mod settings;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::check_folder_pattern,
            commands::cancel_job,
            commands::start_scan,
            commands::start_copy,
            commands::start_verify,
            commands::open_plan,
            commands::list_folders,
            commands::list_entries,
            commands::list_skipped,
            commands::preview_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
