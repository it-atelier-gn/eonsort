mod commands;
mod preview;
mod settings;
#[cfg(feature = "screenshot")]
mod shot;
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
            commands::find_bursts,
            commands::find_lookalikes,
            commands::find_duplicates,
            commands::remove_extra_copies,
            commands::set_excluded,
            commands::upright_model_status,
            commands::install_upright_model,
            commands::cancel_upright_install,
            commands::guess_upright,
            commands::tag_model_status,
            commands::pictures_like,
            commands::plan_offsets,
            commands::gazetteer_status,
            commands::install_gazetteer,
            commands::cancel_gazetteer_install,
            commands::remove_gazetteer,
            commands::quality_model_status,
            commands::install_quality_model,
            commands::cancel_quality_install,
            commands::install_tag_model,
            commands::cancel_tag_install,
            commands::start_tagging,
            commands::cancel_tagging,
            commands::list_tags,
            commands::face_status,
            commands::list_faces,
            commands::start_face_hunt,
            commands::cancel_face_hunt,
            commands::list_names,
            commands::name_face,
            commands::search_pictures,
            commands::save_screenshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
