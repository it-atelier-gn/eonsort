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
            commands::set_destination,
            commands::list_folders,
            commands::list_all_entries,
            commands::list_skipped,
            commands::list_suspects,
            commands::set_date_override,
            commands::clear_date_override,
            commands::shift_dates,
            commands::reprovider_cluster,
            commands::turn_rotation,
            commands::set_rotation,
            commands::clear_rotation,
            commands::rotate_marked,
            commands::probe_rotation,
            commands::preview_file,
            commands::thumbnail_for,
            commands::check_model,
            commands::install_model,
            commands::cancel_install,
            commands::uninstall_model,
            commands::read_with_model,
            commands::find_bursts,
            commands::set_excluded,
            commands::upright_model_status,
            commands::install_upright_model,
            commands::cancel_upright_install,
            commands::guess_upright,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
