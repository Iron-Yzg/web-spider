// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

mod commands;
mod db;
mod models;
mod services;

use std::path::PathBuf;

pub use models::{AppConfig, DownloadProgress, LocalStorageItem, LocalVideo, ScrapeResult, VideoItem, VideoStatus, Website, YtdlpConfig, YtdlpTask, YtdlpTaskStatus, YtdlpResult};
pub use db::{Database, PaginatedVideos};
pub use services::{AppState, AppState as AppStateTrait};

// 初始化 tracing 用于日志输出
#[cfg(debug_assertions)]
fn init_tracing() {
    use tracing_subscriber::prelude::*;

    // 开发环境：只输出到控制台
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer()
            .with_thread_ids(true)
            .with_target(false))
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .init();

    tracing::info!("[App] 开发模式启动，日志仅输出到控制台");
}

/// 清理旧日志文件（保留最近 7 天）
#[cfg(not(debug_assertions))]
fn cleanup_old_logs(log_dir: &PathBuf, max_days: u32) {
    let now = chrono::Utc::now();
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified_date = chrono::DateTime::<chrono::Utc>::from(modified);
                    let days_diff = (now.date_naive() - modified_date.date_naive()).num_days();
                    if days_diff as u32 > max_days {
                        let _ = std::fs::remove_file(entry.path());
                        tracing::info!("[App] 清理旧日志: {}", entry.path().display());
                    }
                }
            }
        }
    }
}
#[cfg(not(debug_assertions))]
fn init_tracing() -> (tracing_appender::non_blocking::WorkerGuard, PathBuf) {
    use tracing_subscriber::prelude::*;

    // 获取日志目录
    let log_dir = if let Some(app_data) = dirs::data_dir() {
        app_data.join("web-spider").join("logs")
    } else {
        PathBuf::from("./logs")
    };

    // 确保日志目录存在
    let _ = std::fs::create_dir_all(&log_dir);

    // 清理 7 天前的旧日志
    cleanup_old_logs(&log_dir, 7);

    // 创建文件 appender - 使用更细粒度的滚动（每小时）
    let file_appender = tracing_appender::rolling::hourly(&log_dir, "web-spider.log");
    let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);

    // 生产环境：同时输出到控制台和文件
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer()
            .with_thread_ids(true)
            .with_target(false)
            .with_writer(non_blocking_file)
            .with_ansi(false))
        .with(tracing_subscriber::fmt::layer()
            .with_thread_ids(true)
            .with_target(false))
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .init();

    tracing::info!("[App] 生产模式启动，日志输出到控制台和文件");

    (guard, log_dir)
}

pub fn run() {
    // 初始化 tracing - guard 必须在生产环境保持存活
    #[cfg(not(debug_assertions))]
    let (_tracing_guard, _log_dir) = init_tracing();

    #[cfg(debug_assertions)]
    init_tracing();

    tracing::info!("[App] 应用启动");

    let app_state = services::AppState::new();

    // 初始化数据库
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    // macOS: ~/Library/Application Support/web-spider
    // Windows: C:\Users\<user>\AppData\Roaming\web-spider
    let data_dir = if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("web-spider")
    } else if let Some(home_dir) = dirs::home_dir() {
        home_dir.join(".web-spider")
    } else {
        PathBuf::from("./data")
    };

    // 确保目录存在
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        tracing::info!("Warning: Failed to create data directory: {}", e);
    }

    // 输出日志路径
    let log_dir = if let Some(app_data) = dirs::data_dir() {
        app_data.join("web-spider").join("logs")
    } else {
        PathBuf::from("./logs")
    };
    tracing::info!("[App] 日志文件路径: {}", log_dir.display());

    let database = runtime.block_on(async {
        db::Database::new(&data_dir).await.expect("Failed to initialize database")
    });

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(database)
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::update_config,
            commands::select_directory,
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
            commands::get_ytdlp_config,
            commands::update_ytdlp_config,
            commands::get_video_info,
            commands::add_ytdlp_tasks,
            commands::delete_ytdlp_task,
            commands::start_ytdlp_task,
            commands::stop_ytdlp_task,
            commands::get_ytdlp_tasks,
            commands::cleanup_ytdlp_tasks,
            commands::open_path,
            // 本地视频命令
            commands::select_video_files,
            commands::get_file_stats,
            commands::get_media_info,
            commands::get_local_videos,
            commands::add_local_video,
            commands::delete_local_video_db,
            // 视频转码命令
            commands::start_video_transcode,
            commands::stop_video_transcode,
            // 视频解复用/播放命令
            commands::start_video_playback_cmd,
            commands::stop_video_remux,
            commands::open_with_system_player,
            // DLNA 投屏命令
            commands::discover_dlna_devices,
            commands::get_local_ip_address,
            commands::start_dlna_media_server,
            commands::stop_dlna_media_server,
            commands::stop_dlna_playback,
            commands::cast_to_dlna_device,
            commands::discover_cast_devices,
            commands::cast_media,
            commands::stop_cast_playback,
        ]);

    builder
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                // 应用退出时清理所有转码会话和 HLS 服务器
                let runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime");
                runtime.block_on(async {
                    services::cleanup_all_transcodes().await;
                    services::cleanup_all_hls_servers().await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
