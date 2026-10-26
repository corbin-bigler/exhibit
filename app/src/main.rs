// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;

fn main() {
    let state = commands::AppState::new();
    state.open_at_launch(std::env::args_os().skip(1).map(Into::into).collect());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::import_files,
            commands::pick_files,
            commands::remove_item,
            commands::clear_items,
            commands::reorder_items,
            commands::split_pack,
            commands::set_copies,
            commands::update_settings,
            commands::preview,
            commands::thumbnail,
            commands::check_export,
            commands::export_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("Exhibit failed to start");
}
