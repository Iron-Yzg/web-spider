use chrono::Utc;
use tokio::sync::broadcast;
use tokio::task;
use tauri::{Emitter, State, WebviewWindow};

use crate::db::{Database, PaginatedVideos};
use crate::models::{
    AppConfig, DownloadProgress, ScrapeResult, VideoItem, VideoStatus, Website,
};
use crate::services::download_m3u8;
use crate::services::{batch_download_concurrent, is_downloading, Scraper, ScraperFactory, ScraperInfo, get_available_scrapers};

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
    website_id: Option<String>,
) -> Result<ScrapeResult, String> {
    // 获取网站配置
    let website = if let Some(id) = website_id {
        let websites = db.get_all_websites().await.map_err(|e| e.to_string())?;
        websites.into_iter().find(|w| w.id == id)
    } else {
        db.get_default_website().await.map_err(|e| e.to_string())?
    };

    // 获取网站配置，如果找不到则使用默认
    let (website, website_name) = if let Some(site) = website {
        let name = site.name.clone();
        (site, name)
    } else {
        // 如果没有配置任何网站，使用默认配置
        (Website {
            id: "default".to_string(),
            name: "默认网站".to_string(),
            base_url: "https://d1ibyof3mbdf0n.cloudfront.net/".to_string(),
            local_storage: vec![],
            is_default: true,
            spider: "d1".to_string(),
        }, "默认网站".to_string())
    };

    // 注意：不再对 SRL 爬虫进行整体页面重复检查
    // 因为 SRL 爬虫可能返回多个新视频，每个视频通过 m3u8_url 单独检查重复

    let _ = window.emit("scrape-log", format!("使用网站配置: {}", website_name));

    // 使用工厂模式创建对应的爬虫
    let scraper = ScraperFactory::create_scraper(&website);
    let _ = window.emit("scrape-log", format!("使用爬虫: {}", scraper.id()));

    // 调用 scrape_all 获取所有结果（SRL 爬虫会返回多个视频）
    // 注意：不再检查整个页面是否已爬取，因为 SRL 爬虫可能返回多个新视频
    let results = scraper.scrape_all(&video_id, {
        let window = window.clone();
        move |log: String| {
            let _ = window.emit("scrape-log", log);
        }
    })
    .await;

    // 保存每个成功的视频到数据库
    let mut saved_count = 0;
    let mut duplicate_count = 0;
    for result in results.iter() {
        if result.success {
            // 对于 SRL 爬虫，多个视频来自同一页，使用 m3u8_url 检查重复
            // 获取所有视频，检查是否已存在相同的 m3u8_url
            let all_videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
            let exists = all_videos.iter().any(|v| v.m3u8_url == result.m3u8_url);

            if exists {
                duplicate_count += 1;
                let _ = window.emit("scrape-log", format!("视频已存在，跳过: {}", result.name));
                continue;
            }

            // 使用爬虫返回的实际视频ID，如果没有则使用输入的页码
            let actual_video_id = result.video_id.clone().unwrap_or_else(|| video_id.clone());

            let video = VideoItem {
                id: uuid::Uuid::new_v4().to_string(),
                name: result.name.clone(),
                m3u8_url: result.m3u8_url.clone(),
                status: VideoStatus::Scraped,
                created_at: Utc::now(),
                downloaded_at: None,
                scrape_id: actual_video_id,
                website_name: website_name.clone(),
            };
            match db.add_video(&video).await {
                Ok(_) => {
                    saved_count += 1;
                    let _ = window.emit("scrape-log", format!("保存成功: {}", result.name));
                }
                Err(e) => {
                    let _ = window.emit("scrape-log", format!("保存失败: {} - {}", result.name, e));
                }
            }
        }
    }

    // 通知前端视频列表已更新
    let videos = db.get_all_videos().await.map_err(|e| e.to_string())?;
    let _ = window.emit("videos-updated", videos);

    // 返回汇总结果
    let success_count = results.iter().filter(|r| r.success).count();
    let total_count = results.len();

    if success_count > 0 {
        Ok(ScrapeResult {
            success: true,
            name: format!("第{}页", video_id),
            m3u8_url: String::new(),
            message: format!("成功爬取 {} / {} 个视频 (新增: {}, 已存在: {})", success_count, total_count, saved_count, duplicate_count),
            video_id: Some(video_id.clone()),
        })
    } else if let Some(first_fail) = results.iter().find(|r| !r.success) {
        Ok(ScrapeResult {
            success: false,
            name: format!("第{}页", video_id),
            m3u8_url: String::new(),
            message: first_fail.message.clone(),
            video_id: Some(video_id.clone()),
        })
    } else {
        Ok(ScrapeResult {
            success: false,
            name: format!("第{}页", video_id),
            m3u8_url: String::new(),
            message: "爬取失败".to_string(),
            video_id: Some(video_id.clone()),
        })
    }
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
            let _ = window_clone.emit("event", progress);
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
            let _ = window_clone.emit("event", progress);
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

// ===== 网站管理命令 =====

#[tauri::command]
pub async fn get_websites(db: State<'_, Database>) -> Result<Vec<Website>, String> {
    db.get_all_websites().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_website(db: State<'_, Database>, website: Website) -> Result<(), String> {
    db.save_website(&website).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_website(db: State<'_, Database>, website_id: String) -> Result<(), String> {
    db.delete_website(&website_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_default_website(db: State<'_, Database>, website_id: String) -> Result<(), String> {
    db.set_default_website(&website_id).await.map_err(|e| e.to_string())
}

// ===== 爬虫管理命令 =====

#[tauri::command]
pub fn get_scrapers() -> Vec<ScraperInfo> {
    get_available_scrapers()
}

#[tauri::command]
pub async fn get_videos_by_website(
    db: State<'_, Database>,
    website_name: String,
    page: i32,
    page_size: i32,
) -> Result<PaginatedVideos, String> {
    db.get_videos_by_website(&website_name, page, page_size)
        .await
        .map_err(|e| e.to_string())
}
