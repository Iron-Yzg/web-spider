use tokio::sync::broadcast;
use tokio::task;
use tauri::{Emitter, State, WebviewWindow};

use crate::models::{
    AppConfig, DownloadProgress, ScrapeResult, VideoItem, VideoStatus,
};
use crate::services::download_m3u8;
use crate::services::scrape_m3u8;
use crate::services::{batch_download_concurrent, is_downloading};
use crate::services::AppState;
use std::path::PathBuf;

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn update_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    let mut config_guard = state.config.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    *config_guard = config;
    drop(config_guard);
    state.save_config();
    Ok(())
}

#[tauri::command]
pub fn select_directory(_window: WebviewWindow) -> Result<String, String> {
    // 使用Tauri dialog插件打开目录选择器
    // 这里返回一个特殊的标记，前端会根据这个标记调用dialog
    Err("DIALOG_REQUIRED".to_string())
}

#[tauri::command]
pub fn get_videos(state: State<'_, AppState>) -> Vec<VideoItem> {
    state.videos.lock().unwrap().clone()
}

#[tauri::command]
pub async fn scrape_video(
    window: WebviewWindow,
    state: State<'_, AppState>,
    video_id: String,
) -> Result<ScrapeResult, String> {
    // 获取配置中的localStorage
    let config = state.config.lock().unwrap().clone();
    let local_storage_json = serde_json::to_string(&config.local_storage).unwrap_or_default();

    // 发送开始爬取的日志
    let _ = window.emit("scrape-log", "开始爬取视频...");

    let result = scrape_m3u8(&video_id, &local_storage_json, {
        let window = window.clone();
        move |log: String| {
            let _ = window.emit("scrape-log", log);
        }
    })
    .await;

    // 发送完成日志
    if result.success {
        let _ = window.emit("scrape-log", format!("✅ 爬取成功: {}", result.name));
    } else {
        let _ = window.emit("scrape-log", format!("❌ 爬取失败: {}", result.message));
    }

    if result.success {
        let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
        let _config_guard = state.config.lock().unwrap();

        // 检查是否已存在
        let exists = videos_guard.iter().any(|v| v.m3u8_url == result.m3u8_url);

        if !exists {
            let video = VideoItem {
                id: uuid::Uuid::new_v4().to_string(),
                name: result.name.clone(),
                m3u8_url: result.m3u8_url.clone(),
                status: VideoStatus::Scraped,
                created_at: chrono::Utc::now(),
                downloaded_at: None,
            };
            videos_guard.push(video);
            drop(videos_guard);
            state.save_videos();

            // 通知前端视频列表已更新
            let _ = window.emit("videos-updated", state.videos.lock().unwrap().clone());
        }
    }

    Ok(result)
}

#[tauri::command]
#[allow(dead_code)]
pub async fn download_video(
    window: WebviewWindow,
    state: State<'_, AppState>,
    video_id: String,
) -> Result<(), String> {
    // Clone all needed data before any async operations
    let config = state.config.lock().unwrap().clone();
    let download_path = config.download_path;

    let video = {
        let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
        let video_idx = videos_guard
            .iter()
            .position(|v| v.id == video_id)
            .ok_or("视频不存在")?;

        videos_guard[video_idx].status = VideoStatus::Downloading;
        videos_guard[video_idx].clone()
    };

    // 发送视频状态更新事件
    let _ = window.emit("videos-updated", state.videos.lock().unwrap().clone());

    // 使用 tokio broadcast channel 传递进度
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

    let result = download_m3u8(&video.m3u8_url, &download_path, &video.id, &video.name, progress_callback).await;

    // Update video status after download
    {
        let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
        if let Some(v) = videos_guard.iter_mut().find(|v| v.id == video_id) {
            if result.is_ok() {
                v.status = VideoStatus::Downloaded;
                v.downloaded_at = Some(chrono::Utc::now());
            } else {
                v.status = VideoStatus::Scraped;
            }
        }
    }

    state.save_videos();
    let _ = window.emit("videos-updated", state.videos.lock().unwrap().clone());

    result
}

#[tauri::command]
pub fn delete_video(state: State<'_, AppState>, video_id: String) -> Result<(), String> {
    let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    videos_guard.retain(|v| v.id != video_id);
    drop(videos_guard);
    state.save_videos();
    Ok(())
}

#[tauri::command]
pub fn clear_downloaded(state: State<'_, AppState>) -> Result<(), String> {
    let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    videos_guard.retain(|v| v.status != VideoStatus::Downloaded);
    drop(videos_guard);
    state.save_videos();
    Ok(())
}

#[tauri::command]
pub fn check_ffmpeg() -> bool {
    crate::services::check_ffmpeg()
}

#[tauri::command]
pub async fn batch_download(
    window: WebviewWindow,
    state: State<'_, AppState>,
    video_ids: Vec<String>,
) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();
    let download_path = config.download_path;

    // 获取所有待下载的视频
    let videos_to_download: Vec<(String, String, String, PathBuf)> = {
        let videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
        video_ids
            .into_iter()
            .filter_map(|id| {
                if is_downloading(&id) {
                    return None;
                }
                videos_guard.iter().find(|v| v.id == id).map(|video| {
                    (video.id.clone(), video.name.clone(), video.m3u8_url.clone(), PathBuf::from(&download_path))
                })
            })
            .collect()
    };

    if videos_to_download.is_empty() {
        return Err("没有可下载的视频".to_string());
    }

    // 更新状态为下载中
    {
        let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
        for id in videos_to_download.iter().map(|(id, _, _, _)| id) {
            if let Some(v) = videos_guard.iter_mut().find(|v| v.id == *id) {
                v.status = VideoStatus::Downloading;
            }
        }
        drop(videos_guard);
        let _ = window.emit("videos-updated", state.videos.lock().unwrap().clone());
    }

    // 创建广播通道用于进度更新
    let (progress_tx, _) = broadcast::channel::<DownloadProgress>(100);

    // 在异步任务中监听进度并发送到前端
    let window_clone = window.clone();
    let mut progress_rx = progress_tx.subscribe();
    task::spawn(async move {
        while let Ok(progress) = progress_rx.recv().await {
            let _ = window_clone.emit("download-progress", progress);
        }
    });

    // 执行并发下载（最多3个同时下载）
    let results = batch_download_concurrent(videos_to_download, 3, progress_tx).await;

    // 更新视频状态
    {
        let mut videos_guard = state.videos.lock().map_err(|e: std::sync::PoisonError<_>| e.to_string())?;

        // 更新每个视频的状态
        for (name, result) in results.iter() {
            // 根据视频名称找到对应的视频
            if let Some(video) = videos_guard.iter_mut().find(|v| v.name == *name) {
                if result.is_ok() {
                    video.status = VideoStatus::Downloaded;
                    video.downloaded_at = Some(chrono::Utc::now());
                } else {
                    // 下载失败，重置为已爬取状态
                    video.status = VideoStatus::Scraped;
                }
            }
        }
    }

    state.save_videos();
    let _ = window.emit("videos-updated", state.videos.lock().unwrap().clone());

    // 检查是否有失败的下载
    let failed_count = results.iter().filter(|(_, r)| r.is_err()).count();
    if failed_count > 0 {
        return Err(format!("有 {} 个视频下载失败", failed_count));
    }

    Ok(())
}
