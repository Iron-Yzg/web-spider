// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
mod models;
mod services;

pub use models::{AppConfig, DownloadProgress, LocalStorageItem, ScrapeResult, VideoItem, VideoStatus};
pub use services::{AppState, AppState as AppStateTrait};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(services::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::update_config,
            commands::select_directory,
            commands::get_videos,
            commands::scrape_video,
            commands::download_video,
            commands::batch_download,
            commands::delete_video,
            commands::clear_downloaded,
            commands::check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
