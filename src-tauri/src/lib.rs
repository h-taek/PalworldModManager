mod classify;
mod commands;
mod detection;
mod injection;
mod library;
mod logtail;
mod manifest;
mod modstore;
mod paths;
mod profiles;
mod pak_convert;
mod retoc;
mod staging;
mod ue4ss;
mod updater;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::detect_game,
            commands::pick_game_binary,
            commands::list_mods,
            commands::import_mod,
            commands::pick_mod_path,
            commands::set_mod_enabled,
            commands::remove_mod,
            commands::launch_game,
            commands::is_game_process_running,
            commands::stop_game,
            commands::force_stop_game,
            commands::list_profiles,
            commands::create_profile,
            commands::duplicate_profile,
            commands::switch_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::check_updates,
            commands::update_mod,
            commands::ue4ss_status,
            commands::ue4ss_install_update,
            commands::read_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
