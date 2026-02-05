use chrono::Utc;
use tokio::sync::broadcast;
use tokio::task;
use tauri::{Emitter, State, WebviewWindow};

use crate::db::{Database, PaginatedVideos};
use crate::models::{
    AppConfig, DownloadProgress, ScrapeResult, VideoItem, VideoStatus, Website,
    YtdlpConfig, YtdlpResult, YtdlpTask, YtdlpTaskStatus,
};

/// 清理下载临时文件（.part 文件等）
fn clean_temp_files(output_path: &str, title: &str) {
    if title.is_empty() {
        return;
    }

    let output_dir = std::path::PathBuf::from(output_path);
    if !output_dir.exists() {
        return;
    }

    // 清理该标题相关的临时文件

    let mut cleaned = Vec::new();
    for entry in std::fs::read_dir(&output_dir).unwrap_or_else(|_| std::fs::read_dir(".").unwrap()) {
        if let Ok(entry) = entry {
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // 检查是否是该任务的临时文件
            if filename_str.starts_with(title) {
                // 将 Cow<str> 转换为 String 以用于 Path
                let path = std::path::PathBuf::from(filename_str.as_ref());
                let ext = path.extension().map(|e| e.to_string_lossy().to_string());
                if let Some(ext_str) = ext {
                    // 只清理 .part 和 .temp 文件（这些是明确的临时文件）
                    if ext_str == "part" || ext_str == "temp" {
                        if let Err(e) = std::fs::remove_file(entry.path()) {
                            eprintln!("[rust] 清理临时文件失败: {} - {}", entry.path().display(), e);
                        } else {
                            cleaned.push(filename_str.to_string());
                        }
                    }
                }
            }
        }
    }

    if !cleaned.is_empty() {
        eprintln!("[rust] 已清理 {} 个临时文件: {:?}", cleaned.len(), cleaned);
    }
}

#[tauri::command]
pub async fn get_config(db: State<'_, Database>) -> Result<AppConfig, String> {
    db.get_config().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_config(db: State<'_, Database>, config: AppConfig) -> Result<(), String> {
    db.save_config(&config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_ytdlp_config(db: State<'_, Database>) -> Result<YtdlpConfig, String> {
    db.get_ytdlp_config().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_ytdlp_config(db: State<'_, Database>, config: YtdlpConfig) -> Result<(), String> {
    db.save_ytdlp_config(&config).await.map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn select_directory(_window: WebviewWindow) -> Result<String, String> {
    Err("DIALOG_REQUIRED".to_string())
}

#[cfg(not(feature = "desktop"))]
#[tauri::command]
pub fn select_directory() -> Result<String, String> {
    Err("DIALOG_REQUIRED".to_string())
}

// 打开路径（文件或文件夹）
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    eprintln!("[rust] 打开路径: {}", path);

    // 获取实际路径（如果是文件则打开其所在文件夹）
    let actual_path = if std::path::Path::new(&path).is_file() {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            parent.to_string_lossy().to_string()
        } else {
            path.clone()
        }
    } else {
        path.clone()
    };

    eprintln!("[rust] 实际打开路径: {}", actual_path);

    // 根据操作系统执行不同命令
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(&["/c", "start", &actual_path])
            .spawn()
            .map(|_| ())
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg(&actual_path)
            .spawn()
            .map(|_| ())
    } else {
        // Linux
        std::process::Command::new("xdg-open")
            .arg(&actual_path)
            .spawn()
            .map(|_| ())
    };

    match result {
        Ok(_) => {
            eprintln!("[rust] 打开路径成功");
            Ok(())
        }
        Err(e) => {
            let msg = format!("打开路径失败: {}", e);
            eprintln!("[rust] {}", msg);
            Err(msg)
        }
    }
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

// ===== 桌面端爬虫相关命令 =====

#[cfg(feature = "desktop")]
use crate::services::{batch_download_concurrent, Scraper, ScraperFactory, ScraperInfo, get_available_scrapers};

#[cfg(feature = "desktop")]
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
            // 获取所有视频，用于重复检查
            let all_videos = db.get_all_videos().await.map_err(|e| e.to_string())?;

            // 检查是否已存在：
            // 1. 如果 m3u8_url 非空，使用 m3u8_url 检查（SRL/D1 爬虫）
            // 2. 如果 m3u8_url 为空（列表爬虫如 D2），使用 video_id 或 name 检查
            let exists = if !result.m3u8_url.is_empty() {
                // 有 m3u8_url 的情况：检查 URL 是否已存在
                all_videos.iter().any(|v| v.m3u8_url == result.m3u8_url)
            } else {
                // 没有 m3u8_url 的情况（列表爬虫）：检查 video_id 是否已存在
                let result_video_id = result.video_id.clone().unwrap_or_default();
                if !result_video_id.is_empty() {
                    all_videos.iter().any(|v| v.scrape_id == result_video_id)
                } else {
                    // 都没有的话，检查 name 是否已存在
                    all_videos.iter().any(|v| v.name == result.name)
                }
            };

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
                // 优先使用 cover_url，没有就用视频第一帧的Base64
                cover_url: result.cover_url.clone(),
                favorite_count: result.favorite_count,
                view_count: result.view_count,
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
            view_count: None,
            favorite_count: None,
            cover_url: None,
        })
    } else if let Some(first_fail) = results.iter().find(|r| !r.success) {
        Ok(ScrapeResult {
            success: false,
            name: format!("第{}页", video_id),
            m3u8_url: String::new(),
            message: first_fail.message.clone(),
            video_id: Some(video_id.clone()),
            view_count: None,
            favorite_count: None,
            cover_url: None,
        })
    } else {
        Ok(ScrapeResult {
            success: false,
            name: format!("第{}页", video_id),
            m3u8_url: String::new(),
            message: "爬取失败".to_string(),
            video_id: Some(video_id.clone()),
            view_count: None,
            favorite_count: None,
            cover_url: None,
        })
    }
}

#[tauri::command]
pub async fn delete_video(db: State<'_, Database>, video_id: String) -> Result<(), String> {
    db.delete_video(&video_id).await.map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn download_video(
    window: WebviewWindow,
    db: State<'_, Database>,
    video_id: String,
) -> Result<(), String> {
    // 复用 batch_download 的逻辑
    batch_download(window, db, vec![video_id]).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn clear_downloaded(db: State<'_, Database>) -> Result<(), String> {
    db.clear_downloaded().await.map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn check_ffmpeg() -> bool {
    crate::services::check_ffmpeg()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn batch_download(
    window: WebviewWindow,
    db: State<'_, Database>,
    video_ids: Vec<String>,
) -> Result<(), String> {
    let config = db.get_config().await.map_err(|e| e.to_string())?;
    let download_path = config.download_path;

    // 直接根据 video_ids 获取视频
    let videos = db.get_videos_by_ids(&video_ids).await.map_err(|e| e.to_string())?;

    if videos.is_empty() {
        return Err("没有可下载的视频".to_string());
    }

    // 先批量更新所有视频状态为 downloading
    for video in &videos {
        if let Err(e) = db.update_video_status(&video.id, VideoStatus::Downloading, None).await {
            tracing::error!("[DOWNLOAD] 设置下载中状态失败: {} - {}", video.id, e);
        }
    }

    // 假设批量下载都来自同一个网站，取第一个视频的 website_name 获取 token
    let website_name = &videos[0].website_name;
    let website_opt = db.get_website_by_name(website_name).await.map_err(|e| e.to_string())?;

    // 获取 token
    let mut token = None;
    if let Some(website) = website_opt {
        for item in &website.local_storage {
            if item.key == "token" {
                token = Some(item.value.clone());
                break;
            }
        }
    }

    // 准备下载列表（使用已更新 token 的 URL）
    let videos_to_download: Vec<(String, String, String, std::path::PathBuf)> = videos
        .into_iter()
        .map(|video| {
            // 更新 URL 中的 token
            let final_url = if let Some(ref t) = token {
                if let Ok(parsed) = url::Url::parse(&video.m3u8_url) {
                    if let Some(query) = parsed.query() {
                        let new_query: String = query
                            .split('&')
                            .map(|p| {
                                if p.starts_with("token=") {
                                    format!("token={}", t)
                                } else {
                                    p.to_string()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("&");
                        let mut new_url = parsed;
                        new_url.set_query(Some(&new_query));
                        new_url.to_string()
                    } else {
                        video.m3u8_url.clone()
                    }
                } else {
                    video.m3u8_url.clone()
                }
            } else {
                video.m3u8_url.clone()
            };

            (video.id.clone(), video.name, final_url, std::path::PathBuf::from(&download_path))
        })
        .collect();

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

    for (id, result) in results.iter() {
        if result.is_ok() {
            if let Err(e) = db.update_video_status(id, VideoStatus::Downloaded, Some(Utc::now())).await {
                tracing::error!("[DOWNLOAD] 更新下载状态失败: {} - {}", id, e);
            }
        } else {
            if let Err(e) = db.update_video_status(id, VideoStatus::Scraped, None).await {
                tracing::error!("[DOWNLOAD] 更新失败状态失败: {} - {}", id, e);
            }
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
pub async fn get_website_by_name(db: State<'_, Database>, name: String) -> Result<Option<Website>, String> {
    db.get_website_by_name(&name).await.map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn save_website(db: State<'_, Database>, website: Website) -> Result<(), String> {
    db.save_website(&website).await.map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_website(db: State<'_, Database>, website_id: String) -> Result<(), String> {
    db.delete_website(&website_id).await.map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn set_default_website(db: State<'_, Database>, website_id: String) -> Result<(), String> {
    db.set_default_website(&website_id).await.map_err(|e| e.to_string())
}

// ===== 爬虫管理命令 =====

#[cfg(feature = "desktop")]
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

// ==================== yt-dlp 下载命令 ====================

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn check_ytdlp() -> bool {
    crate::services::check_ytdlp()
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_ytdlp_version() -> String {
    crate::services::get_ytdlp_version().await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_video_info(url: String) -> Result<YtdlpTask, String> {
    crate::services::get_video_info(&url).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn download_ytdlp_video(
    window: WebviewWindow,
    url: String,
    output_path: String,
    config: YtdlpConfig,
) -> Result<YtdlpResult, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (progress_tx, _) = broadcast::channel::<YtdlpTask>(100);

    let window_clone = window.clone();
    let mut progress_rx = progress_tx.subscribe();
    task::spawn(async move {
        while let Ok(progress) = progress_rx.recv().await {
            let _ = window_clone.emit("ytdlp-progress", progress);
        }
    });

    let result = crate::services::download_video(
        &url,
        &output_path,
        &task_id,
        &config,
        |p| {
            let _ = progress_tx.send(p);
        }
    ).await?;

    Ok(result)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn add_ytdlp_tasks(
    db: State<'_, Database>,
    urls: Vec<String>,
    output_path: String,
) -> Result<Vec<YtdlpTask>, String> {
    // 获取视频信息并创建任务
    let mut tasks = Vec::new();
    for url in &urls {
        match crate::services::get_video_info(url).await {
            Ok(task) => {
                // 创建简化版任务
                let ytdlp_task = YtdlpTask {
                    id: task.id,
                    url: task.url,
                    title: task.title,
                    thumbnail: task.thumbnail,
                    progress: 0,
                    speed: String::new(),
                    file_path: None,
                    status: YtdlpTaskStatus::Pending,
                    message: "等待下载".to_string(),
                    created_at: chrono::Utc::now(),
                    completed_at: None,
                };
                tasks.push(ytdlp_task);
            }
            Err(e) => {
                tracing::error!("获取视频信息失败: {} - {}", url, e);
            }
        }
    }

    if tasks.is_empty() {
        return Err("没有可下载的视频".to_string());
    }

    // 保存所有任务到数据库
    for task in &tasks {
        db.save_ytdlp_task(task).await.map_err(|e| e.to_string())?;
    }

    Ok(tasks)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn cancel_ytdlp_task(task_id: String, db: State<'_, Database>) -> Result<bool, String> {
    let cancelled = crate::services::cancel_task(&task_id);
    if cancelled {
        if let Some(mut task) = crate::services::get_task_by_id(&task_id).await {
            task.status = YtdlpTaskStatus::Cancelled;
            task.message = "已取消".to_string();
            let _ = db.save_ytdlp_task(&task).await.map_err(|e| e.to_string());
        }
    }
    Ok(cancelled)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_ytdlp_task(task_id: String, db: State<'_, Database>) -> Result<(), String> {
    // 从数据库删除
    db.delete_ytdlp_task(&task_id).await.map_err(|e| e.to_string())?;
    // 从内存中移除
    let mut tasks = crate::services::get_all_tasks().await;
    tasks.retain(|t| t.id != task_id);
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn stop_ytdlp_task(task_id: String, db: State<'_, Database>) -> Result<(), String> {
    // 取消下载进程
    crate::services::cancel_task(&task_id);

    // 给一点时间让进度保存到数据库
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 从数据库获取任务（包含当前进度）并更新状态
    let task_opt = db.get_ytdlp_task_by_id(&task_id).await
        .map_err(|e| e.to_string())?;

    if let Some(mut task) = task_opt {
        task.status = YtdlpTaskStatus::Paused;
        task.message = format!("已暂停 (进度: {}%)", task.progress);
        tracing::info!("[yt-dlp] 任务已暂停, id: {}, progress: {}", task_id, task.progress);
        db.save_ytdlp_task(&task).await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn start_ytdlp_task(
    window: WebviewWindow,
    db: State<'_, Database>,
    task_id: String,
    output_path: String,
) -> Result<(), String> {
    // 获取任务信息（从数据库获取）
    let task_opt = db.get_ytdlp_task_by_id(&task_id).await
        .map_err(|e| e.to_string())?;

    if task_opt.is_none() {
        return Err("任务不存在".to_string());
    }

    let task = task_opt.unwrap();
    let config = db.get_ytdlp_config().await
        .map_err(|e| e.to_string())?;

    // 获取已保存的进度（用于断点续传）
    let saved_progress = task.progress;
    eprintln!("[rust] 开始下载任务 {}, URL: {}, 已保存进度: {}%", task_id, task.url, saved_progress);

    // 只有全新下载时才清理临时文件（进度为0时），保留进度时需要断点续传
    if saved_progress == 0 {
        eprintln!("[rust] 清理临时文件...");
        clean_temp_files(&output_path, &task.title);
    } else {
        eprintln!("[rust] 检测到已保存进度 {}%，尝试断点续传...", saved_progress);
    }

    // 更新任务状态为下载中（保留已保存的进度）
    let mut update_task = task.clone();
    update_task.status = YtdlpTaskStatus::Downloading;
    update_task.message = "正在下载...".to_string();
    // 保留之前保存的进度，从该进度继续下载
    update_task.progress = saved_progress;

    db.save_ytdlp_task(&update_task).await
        .map_err(|e| e.to_string())?;

    // 启动下载
    let (progress_tx, _) = broadcast::channel::<YtdlpTask>(100);

    let window_clone = window.clone();
    let task_id_clone = task_id.clone();
    let db_for_spawn = db.inner().clone();

    // 启动后台任务监听进度并更新数据库
    let progress_tx_for_spawn = progress_tx.clone();
    task::spawn(async move {
        let mut progress_rx = progress_tx_for_spawn.subscribe();
        while let Ok(progress) = progress_rx.recv().await {
            // 发送进度到前端
            let _ = window_clone.emit("ytdlp-progress", progress.clone());

            // 实时更新数据库中的进度（只更新progress和file_path字段，不创建新记录）
            // speed 是实时广播的，不入库
            let _ = db_for_spawn.update_ytdlp_task_progress(
                &task_id_clone,
                progress.progress,
                progress.file_path.clone()
            ).await;
        }
    });

    // 执行下载（支持断点续传）
    let result = crate::services::download_video_with_continue(
        &task.url,
        &output_path,
        &task_id,
        &task.title,  // 传递任务标题用于重命名文件
        &config,
        move |p| {
            let _ = progress_tx.send(p);
        }
    ).await;

    // 更新最终状态到数据库（更新同一记录，不创建新记录）
    let mut completed_task = task.clone();
    completed_task.status = match result {
        Ok(_) => YtdlpTaskStatus::Completed,
        Err(_) => YtdlpTaskStatus::Failed,
    };
    completed_task.message = match &result {
        Ok(r) => {
            completed_task.file_path = Some(r.file_path.clone());
            eprintln!("[rust] 下载完成: {}", task_id);
            "下载完成".to_string()
        },
        Err(e) => {
            eprintln!("[rust] 下载失败: {} - {}", task_id, e);
            format!("下载失败: {}", e)
        },
    };
    completed_task.completed_at = Some(chrono::Utc::now());
    completed_task.progress = 100;

    db.save_ytdlp_task(&completed_task).await
        .map_err(|e| e.to_string())?;

    // 发送完成事件，通知前端刷新状态
    let _ = window.emit("ytdlp-complete", completed_task.clone());

    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn get_ytdlp_tasks(db: State<'_, Database>) -> Result<Vec<YtdlpTask>, String> {
    // 直接从数据库获取所有任务
    let tasks = db.get_all_ytdlp_tasks().await.map_err(|e| e.to_string())?;
    Ok(tasks)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn cleanup_ytdlp_tasks(db: State<'_, Database>) -> Result<(), String> {
    // 清理内存中的任务
    crate::services::cleanup_tasks().await;
    // 清理数据库中的任务
    let _ = db.cleanup_ytdlp_tasks().await.map_err(|e| e.to_string())?;
    Ok(())
}
