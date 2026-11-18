mod scanner_backend;
pub mod commands;

use commands::{
    export_project_cmd, export_skiplist_cmd, import_project_cmd, list_samples_cmd, scan_image_cmd,
    scan_video_cmd, set_flags_cmd, show_warning_cmd,
};
use tauri::generate_context;
use tauri::generate_handler;

#[tauri::command]
fn get_app_version() -> String {
    "0.1.0".to_string()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(generate_handler![
            commands::scan_video_cmd,
            commands::scan_image_cmd,
            commands::rescan_sample_cmd,
            commands::import_project_cmd,
            commands::export_project_cmd,
            commands::export_skiplist_cmd,
            commands::set_flags_cmd,
            commands::show_warning_cmd,
            commands::list_samples_cmd,
            commands::list_project_samples_cmd,
            commands::read_sample_cmd,
            commands::open_dialog_select,
            get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("Error running Tauri application");
}
