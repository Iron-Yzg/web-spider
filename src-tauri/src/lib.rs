// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
mod db;
mod models;

#[cfg(feature = "desktop")]
mod services;

use std::path::PathBuf;

pub use models::{AppConfig, DownloadProgress, LocalStorageItem, ScrapeResult, VideoItem, VideoStatus, Website, YtdlpConfig, YtdlpTask, YtdlpTaskStatus, YtdlpResult, VideoQuality};
pub use db::{Database, PaginatedVideos};

#[cfg(feature = "desktop")]
pub use services::{AppState, AppState as AppStateTrait};

// 初始化 tracing 用于日志输出
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_thread_ids(true)
        .with_target(false)
        .init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化 tracing
    init_tracing();
    tracing::info!("[App] 应用启动");
    // 桌面端才需要 AppState（用于爬虫和下载状态管理）
    #[cfg(feature = "desktop")]
    let app_state = services::AppState::new();

    // 初始化数据库
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let data_dir = {
        #[cfg(target_os = "ios")]
        {
            // iOS: 使用 Documents 目录（沙盒内）
            let data_dir = if let Some(documents) = dirs::document_dir() {
                documents.join("web-spider")
            } else {
                PathBuf::from("./Documents/web-spider")
            };
            // 确保目录存在
            if let Err(e) = std::fs::create_dir_all(&data_dir) {
                tracing::info!("Warning: Failed to create data directory: {}", e);
            }
            data_dir
        }

        #[cfg(not(target_os = "ios"))]
        {
            // macOS/Linux/Windows: 使用标准数据目录
            let data_dir = if let Some(home_dir) = dirs::home_dir() {
                if home_dir.join("Library/Application Support").exists() {
                    home_dir.join("Library/Application Support/web-spider")
                } else if let Some(data_dir) = dirs::data_dir() {
                    data_dir.join("web-spider")
                } else {
                    PathBuf::from("./data")
                }
            } else if let Some(data_dir) = dirs::data_dir() {
                data_dir.join("web-spider")
            } else {
                PathBuf::from("./data")
            };
            // 确保目录存在
            if let Err(e) = std::fs::create_dir_all(&data_dir) {
                tracing::info!("Warning: Failed to create data directory: {}", e);
            }
            data_dir
        }
    };

    // tracing::info!("Using data directory: {:?}", data_dir);

    let database = runtime.block_on(async {
        db::Database::new(&data_dir).await.expect("Failed to initialize database")
    });

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .manage(database);

    // 仅桌面端管理 AppState 和爬虫相关命令
    #[cfg(feature = "desktop")]
    {
        builder = builder
            .manage(app_state)
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
                commands::get_website_by_name,
                commands::save_website,
                commands::delete_website,
                commands::set_default_website,
                commands::get_scrapers,
                commands::get_videos_by_website,
                // yt-dlp 命令
                commands::check_ytdlp,
                commands::get_ytdlp_version,
                commands::get_ytdlp_config,
                commands::update_ytdlp_config,
                commands::get_video_info,
                commands::download_ytdlp_video,
                commands::add_ytdlp_tasks,
                commands::cancel_ytdlp_task,
                commands::delete_ytdlp_task,
                commands::start_ytdlp_task,
                commands::stop_ytdlp_task,
                commands::get_ytdlp_tasks,
                commands::cleanup_ytdlp_tasks,
                commands::open_path,
            ]);
    }

    #[cfg(not(feature = "desktop"))]
    {
        // 移动端只需要基本的视频播放功能
        builder = builder
            .invoke_handler(tauri::generate_handler![
                commands::get_config,
                commands::update_config,
                commands::get_videos,
                commands::get_videos_paginated,
                commands::search_videos,
                commands::delete_video,
                commands::get_websites,
                commands::get_videos_by_website,
            ]);
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
