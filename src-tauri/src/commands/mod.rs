use chrono::Utc;
use tokio::sync::broadcast;
use tokio::task;
use tauri::{Emitter, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::db::{Database, PaginatedVideos};
use crate::models::{
    AppConfig, DownloadProgress, LocalVideo, ScrapeResult, VideoItem, VideoStatus, Website,
    YtdlpConfig, YtdlpTask, YtdlpTaskStatus,
};
use crate::services::get_sidecar_path;

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
                            tracing::info!("[rust] 清理临时文件失败: {} - {}", entry.path().display(), e);
                        } else {
                            cleaned.push(filename_str.to_string());
                        }
                    }
                }
            }
        } else {
            tracing::info!("[rust] 忽略非目录项");
        }
    }

    if !cleaned.is_empty() {
        tracing::info!("[rust] 已清理 {} 个临时文件: {:?}", cleaned.len(), cleaned);
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

#[tauri::command]
pub async fn select_directory(window: WebviewWindow) -> Result<Option<String>, String> {
    // 使用 file dialog 选择文件夹
    let result: Option<tauri_plugin_dialog::FilePath> = window
        .dialog()
        .file()
        .set_title("选择下载目录")
        .blocking_pick_folder();

    match result {
        Some(path) => Ok(Some(path.to_string())),
        None => Ok(None),
    }
}

// 打开路径（文件或文件夹）
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    tracing::info!("[rust] 打开路径: {}", path);

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

    tracing::info!("[rust] 实际打开路径: {}", actual_path);

    // 根据操作系统执行不同命令
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(&["/c", "start", &actual_path])
            .spawn()
            .map(|_| ())
    } else {
        // macOS
        std::process::Command::new("open")
            .arg(&actual_path)
            .spawn()
            .map(|_| ())
    };

    match result {
        Ok(_) => {
            tracing::info!("[rust] 打开路径成功");
            Ok(())
        }
        Err(e) => {
            let msg = format!("打开路径失败: {}", e);
            tracing::info!("[rust] {}", msg);
            Err(msg)
        }
    }
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

use crate::services::{batch_download_concurrent, Scraper, ScraperFactory, ScraperInfo, get_available_scrapers};

#[tauri::command]
pub async fn scrape_video(
    window: WebviewWindow,
    db: State<'_, Database>,
    url: String,
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
    let results = scraper.scrape_all(&url, {
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

            // 使用爬虫返回的实际视频ID，如果没有则使用输入的URL
            let actual_video_id = result.video_id.clone().unwrap_or_else(|| url.clone());

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
                file_path: None,
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
            name: format!("第{}页", url),
            m3u8_url: String::new(),
            message: format!("成功爬取 {} / {} 个视频 (新增: {}, 已存在: {})", success_count, total_count, saved_count, duplicate_count),
            video_id: Some(url.clone()),
            view_count: None,
            favorite_count: None,
            cover_url: None,
        })
    } else if let Some(first_fail) = results.iter().find(|r| !r.success) {
        Ok(ScrapeResult {
            success: false,
            name: format!("第{}页", url),
            m3u8_url: String::new(),
            message: first_fail.message.clone(),
            video_id: Some(url.clone()),
            view_count: None,
            favorite_count: None,
            cover_url: None,
        })
    } else {
        Ok(ScrapeResult {
            success: false,
            name: format!("第{}页", url),
            m3u8_url: String::new(),
            message: "爬取失败".to_string(),
            video_id: Some(url.clone()),
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

#[tauri::command]
pub async fn download_video(
    app_handle: tauri::AppHandle,
    window: WebviewWindow,
    db: State<'_, Database>,
    video_id: String,
) -> Result<(), String> {
    // 复用 batch_download 的逻辑
    batch_download(app_handle, window, db, vec![video_id]).await
}

#[tauri::command]
pub async fn clear_downloaded(db: State<'_, Database>) -> Result<(), String> {
    db.clear_downloaded().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_ffmpeg(app_handle: tauri::AppHandle) -> bool {
    crate::services::check_ffmpeg(&app_handle)
}

#[tauri::command]
pub async fn batch_download(
    app_handle: tauri::AppHandle,
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

    let results = batch_download_concurrent(&app_handle, videos_to_download, 3, progress_tx).await;

    for (id, result) in results.iter() {
        if let Ok(ytdlp_result) = result {
            if let Err(e) = db.update_video_status(id, VideoStatus::Downloaded, Some(Utc::now())).await {
                tracing::error!("[DOWNLOAD] 更新下载状态失败: {} - {}", id, e);
            }

            // 添加到本地视频管理
            let local_video = LocalVideo {
                id: uuid::Uuid::new_v4().to_string(),
                name: ytdlp_result.title.clone(),
                file_path: ytdlp_result.file_path.clone(),
                file_size: format_file_size(ytdlp_result.file_size),
                duration: String::new(),
                resolution: String::new(),
                added_at: chrono::Utc::now(),
            };

            if let Err(e) = db.add_local_video(&local_video).await {
                tracing::warn!("[DOWNLOAD] 添加到本地视频失败: {}", e);
            } else {
                tracing::info!("[DOWNLOAD] 已添加到本地视频管理: {}", ytdlp_result.title);
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

// ==================== yt-dlp 下载命令 ====================

#[tauri::command]
pub async fn get_video_info(app_handle: tauri::AppHandle, url: String, quality: u32) -> Result<YtdlpTask, String> {
    crate::services::get_video_info(&app_handle, &url, quality).await
}

#[tauri::command]
pub async fn add_ytdlp_tasks(
    app_handle: tauri::AppHandle,
    db: State<'_, Database>,
    urls: Vec<String>,
    quality: u32,
) -> Result<Vec<YtdlpTask>, String> {
    // 获取视频信息并创建任务
    let mut tasks = Vec::new();
    for url in &urls {
        match crate::services::get_video_info(&app_handle, url, quality).await {
            Ok(task) => {
                // 创建简化版任务
                let ytdlp_task = YtdlpTask {
                    id: task.id,
                    url: task.url,
                    title: task.title,
                    progress: 0,
                    speed: String::new(),
                    file_path: None,
                    status: YtdlpTaskStatus::Pending,
                    message: "等待下载".to_string(),
                    created_at: chrono::Utc::now(),
                    completed_at: None,
                    resolution: task.resolution,
                    file_size: task.file_size,
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

#[tauri::command]
pub async fn delete_ytdlp_task(task_id: String, db: State<'_, Database>) -> Result<(), String> {
    // 从数据库删除
    db.delete_ytdlp_task(&task_id).await.map_err(|e| e.to_string())?;
    // 从内存中移除
    let mut tasks = crate::services::get_all_tasks().await;
    tasks.retain(|t| t.id != task_id);
    Ok(())
}

#[tauri::command]
pub async fn stop_ytdlp_task(task_id: String, db: State<'_, Database>) -> Result<(), String> {
    // 从数据库获取当前进度（在杀死进程前获取）
    let task_opt = db.get_ytdlp_task_by_id(&task_id).await
        .map_err(|e| e.to_string())?;
    
    let current_progress = task_opt.as_ref().map(|t| t.progress).unwrap_or(0);
    tracing::info!("[yt-dlp] 准备暂停任务 {}, 当前进度: {}%", task_id, current_progress);
    
    // 取消下载进程（这会杀死进程树，包括所有子进程）
    let killed = crate::services::cancel_task(&task_id);
    if killed {
        tracing::info!("[yt-dlp] 已发送终止信号到任务 {}", task_id);
    } else {
        tracing::warn!("[yt-dlp] 未找到运行中的进程: {}", task_id);
    }

    // 等待足够时间让进程及其子进程完全终止
    // 子进程（如 ffmpeg）可能需要较长时间才能完全停止
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

    // 更新任务状态为暂停（保留进度）
    if let Some(mut task) = task_opt {
        // 使用杀死进程前保存的进度（current_progress）
        // 避免后台任务在终止过程中错误更新进度
        task.status = YtdlpTaskStatus::Paused;
        task.progress = current_progress; // 使用暂停时的实际进度
        task.message = format!("已暂停 (进度: {}%)", task.progress);
        tracing::info!("[yt-dlp] 任务已暂停, id: {}, progress: {}", task_id, task.progress);
        db.save_ytdlp_task(&task).await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn start_ytdlp_task(
    app_handle: tauri::AppHandle,
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
    tracing::info!("[rust] 开始下载任务 {}, URL: {}, 已保存进度: {}%", task_id, task.url, saved_progress);

    // 只有全新下载时才清理临时文件（进度为0时），保留进度时需要断点续传
    if saved_progress == 0 {
        tracing::info!("[rust] 清理临时文件...");
        clean_temp_files(&output_path, &task.title);
    } else {
        tracing::info!("[rust] 检测到已保存进度 {}%，尝试断点续传...", saved_progress);
    }

    // 更新任务状态为下载中（保留已保存的进度）
    let mut update_task = task.clone();
    update_task.status = YtdlpTaskStatus::Downloading;
    update_task.message = "正在下载...".to_string();
    // 保留之前保存的进度，从该进度继续下载
    update_task.progress = saved_progress;

    db.save_ytdlp_task(&update_task).await.map_err(|e| e.to_string())?;

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

    // 执行下载（使用新的统一下载入口）
    let result = crate::services::download_video(
        &app_handle,
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
    // 先检查当前数据库状态，避免覆盖用户暂停操作
    let current_task = db.get_ytdlp_task_by_id(&task_id).await
        .map_err(|e| e.to_string())?
        .ok_or("任务不存在")?;
    
    // 如果任务已被标记为暂停（用户主动暂停），不要覆盖为失败状态
    if current_task.status == YtdlpTaskStatus::Paused {
        tracing::info!("[rust] 任务已被用户暂停，跳过失败状态更新: {}", task_id);
        return Ok(());
    }
    
    let mut completed_task = task.clone();
    completed_task.status = match result {
        Ok(_) => YtdlpTaskStatus::Completed,
        Err(_) => YtdlpTaskStatus::Failed,
    };
    completed_task.message = match &result {
        Ok(r) => {
            completed_task.file_path = Some(r.file_path.clone());
            tracing::info!("[rust] 下载完成: {}", task_id);
            "下载完成".to_string()
        },
        Err(e) => {
            tracing::info!("[rust] 下载失败: {} - {}", task_id, e);
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

#[tauri::command]
pub async fn get_ytdlp_tasks(db: State<'_, Database>) -> Result<Vec<YtdlpTask>, String> {
    // 直接从数据库获取所有任务
    let tasks = db.get_all_ytdlp_tasks().await.map_err(|e| e.to_string())?;
    Ok(tasks)
}

#[tauri::command]
pub async fn cleanup_ytdlp_tasks(db: State<'_, Database>) -> Result<(), String> {
    // 清理内存中的任务
    crate::services::cleanup_tasks().await;
    // 清理数据库中的任务
    let _ = db.cleanup_ytdlp_tasks().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ==================== 本地视频管理命令 ====================

#[tauri::command]
pub async fn select_video_files(window: WebviewWindow) -> Result<Option<Vec<String>>, String> {
    // 使用 file dialog 选择视频文件
    let result: Option<Vec<tauri_plugin_dialog::FilePath>> = window
        .dialog()
        .file()
        .set_title("选择视频文件")
        .add_filter("视频文件", &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg"])
        .blocking_pick_files();

    match result {
        Some(paths) => {
            let file_paths: Vec<String> = paths.into_iter().map(|p| p.to_string()).collect();
            Ok(Some(file_paths))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn get_file_stats(path: String) -> Result<(u64, String), String> {
    let metadata = std::fs::metadata(&path)
        .map_err(|e| format!("获取文件元数据失败: {}", e))?;
    let size = metadata.len();
    let modified = if let Ok(modified) = metadata.modified() {
        format!("{:?}", modified)
    } else {
        String::from("unknown")
    };
    Ok((size, modified))
}

/// 使用 ffprobe 获取视频信息
#[tauri::command]
pub async fn get_media_info(app_handle: tauri::AppHandle, path: String) -> Result<(String, String, String), String> {
    use tokio::process::Command;

    // ffprobe sidecar 命令获取视频信息
    // 注意：对大文件只读取前 5MB 避免卡死
    let ffprobe_path = get_sidecar_path(&app_handle, "ffprobe")?;
    let output = Command::new(&ffprobe_path)
        .args(&[
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            "-probesize", "5M",      // 限制探测大小为 5MB
            "-analyzeduration", "5M", // 限制分析时长
            &path,
        ])
        .output()
        .await
        .map_err(|e| format!("执行 ffprobe 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe 失败: {}", stderr));
    }

    let json_output = String::from_utf8_lossy(&output.stdout);

    // 解析 JSON
    let json: serde_json::Value = serde_json::from_str(&json_output)
        .map_err(|e| format!("解析 ffprobe 输出失败: {}", e))?;

    // 提取分辨率
    let resolution = json.get("streams")
        .and_then(|v| v.as_array())
        .and_then(|streams| {
            streams.iter().find(|s| s.get("codec_type") == Some(&serde_json::json!("video")))
        })
        .and_then(|video| {
            let width = video.get("width").and_then(|w| w.as_u64()).unwrap_or(0);
            let height = video.get("height").and_then(|h| h.as_u64()).unwrap_or(0);
            if width > 0 && height > 0 {
                Some(format!("{}x{}", width, height))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "未知".to_string());

    // 提取时长
    let duration = json.get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .map(|d| {
            // 转换秒为可读格式
            let secs: f64 = d.parse().unwrap_or(0.0);
            let hours = (secs as u64) / 3600;
            let minutes = ((secs as u64) % 3600) / 60;
            let seconds = (secs as u64) % 60;
            if hours > 0 {
                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{:02}:{:02}", minutes, seconds)
            }
        })
        .unwrap_or_else(|| "未知".to_string());

    // 提取文件大小
    let file_size = json.get("format")
        .and_then(|f| f.get("size"))
        .and_then(|s| s.as_str())
        .map(|s| {
            let bytes: u64 = s.parse().unwrap_or(0);
            format_file_size(bytes)
        })
        .unwrap_or_else(|| "未知".to_string());

    Ok((resolution, duration, file_size))
}

/// 格式化文件大小
fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024.0;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(k).floor() as usize;
    let size = bytes as f64 / k.powi(i as i32);
    format!("{} {}", size.round() as u64, sizes[i])
}

// ==================== 数据库版本地视频管理 ====================

#[tauri::command]
pub async fn get_local_videos(db: State<'_, Database>) -> Result<Vec<LocalVideo>, String> {
    db.get_all_local_videos().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_local_video(db: State<'_, Database>, video: LocalVideo) -> Result<(), String> {
    db.add_local_video(&video).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_local_video_db(db: State<'_, Database>, id: String) -> Result<(), String> {
    db.delete_local_video(&id).await.map_err(|e| e.to_string())
}

// ==================== 视频转码命令 ====================

use crate::services::{start_video_transcode_cmd, stop_video_transcode_cmd};

#[tauri::command]
pub async fn start_video_transcode(
    app_handle: tauri::AppHandle,
    file_path: String,
    session_id: String,
) -> Result<String, String> {
    tracing::info!("[commands] 开始视频转码: session={}, path={}", session_id, file_path);
    start_video_transcode_cmd(app_handle, file_path, session_id).await
}

#[tauri::command]
pub async fn stop_video_transcode(session_id: String) -> Result<(), String> {
    tracing::info!("[commands] 停止视频转码: session={}", session_id);
    stop_video_transcode_cmd(session_id).await
}

// ==================== 视频解复用/播放命令 ====================

use crate::services::{start_video_playback, stop_remux};

/// 启动视频播放（自动选择解复用或转码）
#[tauri::command]
pub async fn start_video_playback_cmd(
    app_handle: tauri::AppHandle,
    file_path: String,
    session_id: String,
) -> Result<(String, bool), String> {
    tracing::info!("[commands] 开始视频播放: session={}, path={}", session_id, file_path);
    start_video_playback(app_handle, file_path, session_id).await
}

/// 停止视频解复用
#[tauri::command]
pub async fn stop_video_remux(session_id: String) -> Result<(), String> {
    tracing::info!("[commands] 停止视频解复用: session={}", session_id);
    stop_remux(&session_id).await
}

/// 使用系统播放器打开视频文件
#[tauri::command]
pub async fn open_with_system_player(app_handle: tauri::AppHandle, file_path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    
    tracing::info!("[commands] 使用系统播放器打开: {}", file_path);
    
    // 使用 opener 插件打开文件
    app_handle
        .opener()
        .open_path(&file_path, None::<&str>)
        .map_err(|e| format!("打开视频失败: {}", e))?;
    
    Ok(())
}

// ==================== DLNA 投屏命令 ====================

use crate::services::DlnaService;
use std::sync::Arc;
use tokio::sync::Mutex;

static DLNA_SERVICE: once_cell::sync::Lazy<Arc<Mutex<DlnaService>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(DlnaService::new())));

#[derive(serde::Serialize)]
pub struct DlnaDeviceInfo {
    pub name: String,
    pub udn: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CastProtocol {
    Auto,
    Sony,
    Dlna,
    Chromecast,
    Airplay,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CastDeviceInfo {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub available: bool,
    pub note: Option<String>,
}

fn is_sony_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("sony") || lower.contains("bravia")
}

#[tauri::command]
pub async fn discover_dlna_devices(timeout_secs: u64) -> Result<Vec<DlnaDeviceInfo>, String> {
    let devices = DlnaService::discover_devices(timeout_secs).await?;
    Ok(devices
        .into_iter()
        .map(|d| DlnaDeviceInfo {
            name: d.name,
            udn: d.udn,
        })
        .collect())
}

#[tauri::command]
pub async fn discover_cast_devices(
    protocol: CastProtocol,
    timeout_secs: u64,
) -> Result<Vec<CastDeviceInfo>, String> {
    match protocol {
        CastProtocol::Auto | CastProtocol::Sony | CastProtocol::Dlna => {
            let mut devices = DlnaService::discover_devices(timeout_secs).await?;

            if matches!(protocol, CastProtocol::Sony) {
                devices.retain(|d| is_sony_name(&d.name));
            }

            // auto 模式优先把 Sony/BRAVIA 放在前面
            if matches!(protocol, CastProtocol::Auto) {
                devices.sort_by_key(|d| if is_sony_name(&d.name) { 0 } else { 1 });
            }

            Ok(devices
                .into_iter()
                .map(|d| {
                    let sony = is_sony_name(&d.name);
                    CastDeviceInfo {
                        id: d.name.clone(),
                        name: d.name,
                        protocol: "dlna".to_string(),
                        available: true,
                        note: if sony {
                            Some("Sony 推荐：已启用稳态参数".to_string())
                        } else if matches!(protocol, CastProtocol::Auto) {
                            Some("自动模式：非 Sony 设备使用通用 DLNA 参数".to_string())
                        } else {
                            None
                        },
                    }
                })
                .collect())
        }
        CastProtocol::Chromecast => Ok(vec![CastDeviceInfo {
            id: "chromecast-not-implemented".to_string(),
            name: "Chromecast (待实现)".to_string(),
            protocol: "chromecast".to_string(),
            available: false,
            note: Some("当前版本优先稳定支持 Sony DLNA，Chromecast 通道预留中".to_string()),
        }]),
        CastProtocol::Airplay => Ok(vec![CastDeviceInfo {
            id: "airplay-not-implemented".to_string(),
            name: "AirPlay (待实现)".to_string(),
            protocol: "airplay".to_string(),
            available: false,
            note: Some("当前版本优先稳定支持 Sony DLNA，AirPlay 通道预留中".to_string()),
        }]),
    }
}

#[tauri::command]
pub async fn get_local_ip_address() -> Result<String, String> {
    DlnaService::get_local_ip().await
}

#[tauri::command]
pub async fn start_dlna_media_server(
    file_path: String,
    port: u16,
) -> Result<String, String> {
    let service = DLNA_SERVICE.lock().await;
    service.start_media_server(file_path, port).await
}

#[tauri::command]
pub async fn stop_dlna_media_server() -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    service.stop_media_server().await
}

#[tauri::command]
pub async fn stop_dlna_playback(device_name: String) -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    service.stop_playback(device_name).await
}

#[tauri::command]
pub async fn cast_to_dlna_device(
    device_name: String,
    video_url: String,
    title: String,
) -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    service.cast_to_device(device_name, video_url, title).await
}

#[tauri::command]
pub async fn cast_media(
    protocol: CastProtocol,
    device_id: String,
    video_url: String,
    title: String,
) -> Result<(), String> {
    match protocol {
        CastProtocol::Auto | CastProtocol::Sony | CastProtocol::Dlna => {
            let service = DLNA_SERVICE.lock().await;
            service.cast_to_device(device_id, video_url, title).await
        }
        CastProtocol::Chromecast => Err("Chromecast casting is not implemented yet in this build".to_string()),
        CastProtocol::Airplay => Err("AirPlay casting is not implemented yet in this build".to_string()),
    }
}

#[tauri::command]
pub async fn stop_cast_playback(protocol: CastProtocol, device_id: String) -> Result<(), String> {
    match protocol {
        CastProtocol::Auto | CastProtocol::Sony | CastProtocol::Dlna => {
            let service = DLNA_SERVICE.lock().await;
            service.stop_playback(device_id).await
        }
        CastProtocol::Chromecast | CastProtocol::Airplay => Ok(()),
    }
}
