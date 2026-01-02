use chrono::Utc;
use tokio::sync::broadcast;
use tokio::task;
use tauri::{Emitter, State, WebviewWindow};

use crate::db::{Database, PaginatedVideos};
use crate::models::{
    AppConfig, DownloadProgress, ScrapeResult, VideoItem, VideoStatus,
};
use crate::services::download_m3u8;
use crate::services::scrape_m3u8;
use crate::services::{batch_download_concurrent, is_downloading};

#[tauri::command]
pub async fn get_config(db: State<'_, Database>) -> Result<AppConfig, String> {
    db.get_config().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_config(db: State<'_, Database>, config: AppConfig) -> Result<(), String> {
    db.save_config(&config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn select_directory(_window: WebviewWindow) -> Result<String, String> {
    Err("DIALOG_REQUIRED".to_string())
}

// 获取所有视频（从数据库）
#[tauri::command]
pub async fn get_videos(db: State<'_, Database>) -> Result<Vec<VideoItem>, String> {
    db.get_all_videos().await.map_err(|e| e.to_string())
}

// 分页获取视频
#[tauri::command]
pub async fn get_videos_paginated(
    db: State<'_, Database>,
    page: i32,
    page_size: i32,
) -> Result<PaginatedVideos, String> {
    db.get_videos_paginated(page, page_size)
        .await
        .map_err(|e| e.to_string())
}

// 搜索视频
#[tauri::command]
pub async fn search_videos(
    db: State<'_, Database>,
    query: String,
    page: i32,
    page_size: i32,
) -> Result<PaginatedVideos, String> {
    db.search_videos(&query, page, page_size)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scrape_video(
    window: WebviewWindow,
    db: State<'_, Database>,
    video_id: String,
) -> Result<ScrapeResult, String> {
    let config = db.get_config().await.map_err(|e| e.to_string())?;
    let local_storage_json = serde_json::to_string(&config.local_storage).unwrap_or_default();

    let _ = window.emit("scrape-log", "开始爬取视频...");

    let result = scrape_m3u8(&video_id, &local_storage_json, {
        let window = window.clone();
        move |log: String| {
            let _ = window.emit("scrape-log", log);
        }
    })
    .await;

    if result.success {
        let _ = window.emit("scrape-log", format!("爬取成功: {}", result.name));
    } else {
        let _ = window.emit("scrape-log", format!("爬取失败: {}", result.message));
    }

    if result.success {
        // 检查是否已存在
        let all_videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
        let exists = all_videos.iter().any(|v| v.m3u8_url == result.m3u8_url);

        if !exists {
            let video = VideoItem {
                id: uuid::Uuid::new_v4().to_string(),
                name: result.name.clone(),
                m3u8_url: result.m3u8_url.clone(),
                status: VideoStatus::Scraped,
                created_at: Utc::now(),
                downloaded_at: None,
            };
            db.add_video(&video).await.map_err(|e| e.to_string())?;

            // 通知前端视频列表已更新
            let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
            let _ = window.emit("videos-updated", videos);
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn download_video(
    window: WebviewWindow,
    db: State<'_, Database>,
    video_id: String,
) -> Result<(), String> {
    let config = db.get_config().await.map_err(|e| e.to_string())?;
    let download_path = config.download_path;

    let video = {
        let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
        let video = videos
            .iter()
            .find(|v| v.id == video_id)
            .ok_or("视频不存在")?
            .clone();

        db.update_video_status(&video_id, VideoStatus::Downloading, None)
            .await
            .map_err(|e| e.to_string())?;

        video
    };

    let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
    let _ = window.emit("videos-updated", videos);

    let (progress_tx, _) = broadcast::channel::<DownloadProgress>(100);

    let window_clone = window.clone();
    let mut progress_rx = progress_tx.subscribe();
    task::spawn(async move {
        while let Ok(progress) = progress_rx.recv().await {
            let _ = window_clone.emit("download-progress", progress);
        }
    });

    let progress_callback = move |p: DownloadProgress| {
        let _ = progress_tx.send(p);
    };

    let result = download_m3u8(
        &video.m3u8_url,
        &download_path,
        &video.id,
        &video.name,
        progress_callback,
    )
    .await;

    if result.is_ok() {
        db.update_video_status(&video_id, VideoStatus::Downloaded, Some(Utc::now()))
            .await
            .map_err(|e| e.to_string())?;
    } else {
        db.update_video_status(&video_id, VideoStatus::Scraped, None)
            .await
            .map_err(|e| e.to_string())?;
    }

    let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
    let _ = window.emit("videos-updated", videos);

    result
}

#[tauri::command]
pub async fn delete_video(db: State<'_, Database>, video_id: String) -> Result<(), String> {
    db.delete_video(&video_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_downloaded(db: State<'_, Database>) -> Result<(), String> {
    db.clear_downloaded().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_ffmpeg() -> bool {
    crate::services::check_ffmpeg()
}

#[tauri::command]
pub async fn batch_download(
    window: WebviewWindow,
    db: State<'_, Database>,
    video_ids: Vec<String>,
) -> Result<(), String> {
    let config = db.get_config().await.map_err(|e| e.to_string())?;
    let download_path = config.download_path;

    let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;

    let videos_to_download: Vec<(String, String, String, std::path::PathBuf)> = video_ids
        .into_iter()
        .filter_map(|id| {
            if is_downloading(&id) {
                return None;
            }
            videos.iter().find(|v| v.id == id).map(|video| {
                (
                    video.id.clone(),
                    video.name.clone(),
                    video.m3u8_url.clone(),
                    std::path::PathBuf::from(&download_path),
                )
            })
        })
        .collect();

    if videos_to_download.is_empty() {
        return Err("没有可下载的视频".to_string());
    }

    for id in videos_to_download.iter().map(|(id, _, _, _)| id) {
        db.update_video_status(id, VideoStatus::Downloading, None)
            .await
            .map_err(|e| e.to_string())?;
    }

    let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
    let _ = window.emit("videos-updated", videos);

    let (progress_tx, _) = broadcast::channel::<DownloadProgress>(100);

    let window_clone = window.clone();
    let mut progress_rx = progress_tx.subscribe();
    task::spawn(async move {
        while let Ok(progress) = progress_rx.recv().await {
            let _ = window_clone.emit("download-progress", progress);
        }
    });

    let results = batch_download_concurrent(videos_to_download, 3, progress_tx).await;

    for (name, result) in results.iter() {
        if result.is_ok() {
            db.update_video_status_by_name(name, VideoStatus::Downloaded, Some(Utc::now()))
                .await
                .map_err(|e| e.to_string())?;
        } else {
            db.update_video_status_by_name(name, VideoStatus::Scraped, None)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
    let _ = window.emit("videos-updated", videos);

    let failed_count = results.iter().filter(|(_, r)| r.is_err()).count();
    if failed_count > 0 {
        return Err(format!("有 {} 个视频下载失败", failed_count));
    }

    Ok(())
}
