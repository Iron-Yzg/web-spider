// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
mod db;
mod models;
mod services;

pub use models::{AppConfig, DownloadProgress, LocalStorageItem, ScrapeResult, VideoItem, VideoStatus, Website};
pub use services::{AppState, AppState as AppStateTrait};
pub use db::{Database, PaginatedVideos};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = services::AppState::new();

    // 初始化数据库
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let data_dir = {
        // macOS: ~/Library/Application Support/web-spider
        let data_dir = if let Some(home_dir) = dirs::home_dir() {
            home_dir.join("Library/Application Support/web-spider")
        } else if let Some(data_dir) = dirs::data_dir() {
            data_dir.join("web-spider")
        } else {
            std::path::PathBuf::from("./data")
        };
        // 确保目录存在
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            eprintln!("Warning: Failed to create data directory: {}", e);
        }
        data_dir
    };

    // eprintln!("Using data directory: {:?}", data_dir);

    let database = runtime.block_on(async {
        db::Database::new(&data_dir).await.expect("Failed to initialize database")
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .manage(database)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::update_config,
            commands::select_directory,
            commands::get_videos,
            commands::get_videos_paginated,
            commands::search_videos,
            commands::scrape_video,
            commands::download_video,
            commands::batch_download,
            commands::delete_video,
            commands::clear_downloaded,
            commands::check_ffmpeg,
            commands::get_websites,
            commands::save_website,
            commands::delete_website,
            commands::set_default_website,
            commands::get_scrapers,
            commands::get_videos_by_website,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
