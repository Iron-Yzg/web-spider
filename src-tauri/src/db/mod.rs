use std::path::PathBuf;
use sqlx::sqlite::{SqlitePool, SqliteRow, SqliteConnectOptions};
use sqlx::prelude::*;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::str::FromStr;

pub use crate::models::{AppConfig, LocalStorageItem, LocalVideo, VideoItem, VideoStatus, Website, YtdlpConfig, YtdlpTask, YtdlpTaskStatus};

/// 从数据库行解析 VideoItem
fn row_to_video_item(row: &SqliteRow) -> Result<VideoItem, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    let m3u8_url: String = row.try_get("m3u8_url")?;
    let status_str: String = row.try_get("status")?;
    let status: VideoStatus = serde_json::from_str(&status_str)
        .unwrap_or(VideoStatus::Scraped);
    let created_at_str: String = row.try_get("created_at")?;
    let created_at: DateTime<Utc> = created_at_str.parse()
        .unwrap_or_else(|_| Utc::now());

    let downloaded_at: Option<DateTime<Utc>> = row.try_get("downloaded_at")
        .ok()
        .and_then(|s: String| s.parse().ok());

    // 新字段，可能为空（兼容旧数据）
    let scrape_id: String = row.try_get("scrape_id").unwrap_or_default();
    let website_name: String = row.try_get("website_name").unwrap_or_default();
    let cover_url: Option<String> = row.try_get("cover_url").ok().filter(|s: &String| !s.is_empty());
    let favorite_count: i64 = row.try_get("favorite_count").unwrap_or(0);
    let view_count: i64 = row.try_get("view_count").unwrap_or(0);

    Ok(VideoItem {
        id,
        name,
        m3u8_url,
        status,
        created_at,
        downloaded_at,
        scrape_id,
        website_name,
        cover_url,
        favorite_count: Some(favorite_count),
        view_count: Some(view_count),
    })
}

/// 从数据库行解析 YtdlpTask（简化版）
fn row_to_ytdlp_task(row: &SqliteRow) -> Result<YtdlpTask, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let url: String = row.try_get("url")?;
    let title: String = row.try_get("title")?;
    let progress: i64 = row.try_get("progress")?;
    let file_path: Option<String> = row.try_get("file_path")?;
    let status_str: String = row.try_get("status")?;
    let status: YtdlpTaskStatus = match status_str.as_str() {
        "Downloading" => YtdlpTaskStatus::Downloading,
        "Paused" => YtdlpTaskStatus::Paused,
        "Completed" => YtdlpTaskStatus::Completed,
        "Failed" => YtdlpTaskStatus::Failed,
        "Cancelled" => YtdlpTaskStatus::Cancelled,
        "Queued" => YtdlpTaskStatus::Queued,
        _ => YtdlpTaskStatus::Pending,
    };
    let message: String = row.try_get("message")?;
    let created_at_str: String = row.try_get("created_at")?;
    let created_at: DateTime<Utc> = created_at_str.parse()
        .unwrap_or_else(|_| Utc::now());
    let completed_at: Option<DateTime<Utc>> = row.try_get("completed_at")
        .ok()
        .and_then(|s: String| s.parse().ok());
    // 新字段：resolution 和 file_size（数据库可能没有这些列，使用默认值）
    let resolution: String = row.try_get("resolution").ok().unwrap_or_default();
    let file_size: String = row.try_get("file_size").ok().unwrap_or_default();

    Ok(YtdlpTask {
        id,
        url,
        title,
        progress: progress as u8,
        speed: String::new(), // speed 是实时广播的，不入库
        file_path,
        status,
        message,
        created_at,
        completed_at,
        resolution,
        file_size,
    })
}

/// 分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedVideos {
    pub videos: Vec<VideoItem>,
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    pub has_more: bool,
}

/// 数据库管理器
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// 创建新的数据库实例
    pub async fn new(data_dir: &PathBuf) -> Result<Self, sqlx::Error> {
        let db_path = data_dir.join("web-spider.db");

        // 确保目录存在
        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 使用 SqliteConnectOptions 来正确处理包含空格的路径
        let options = SqliteConnectOptions::from_str(&format!("file:{}", db_path.to_string_lossy()))
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        let db = Self { pool };

        // 运行迁移
        db.run_migrations().await?;

        Ok(db)
    }

    /// 运行数据库迁移
    async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS videos (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                m3u8_url TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'Scraped',
                created_at TEXT NOT NULL,
                downloaded_at TEXT,
                scrape_id TEXT DEFAULT '',
                website_name TEXT DEFAULT '',
                cover_url TEXT,
                favorite_count INTEGER DEFAULT 0,
                view_count INTEGER DEFAULT 0
            )
        "#).execute(&self.pool).await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_videos_created_at ON videos(created_at DESC)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_videos_status ON videos(status)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_videos_scrape_id ON videos(scrape_id)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_videos_website_name ON videos(website_name)").execute(&self.pool).await?;

        // 配置表 (key-value 结构)
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
        "#).execute(&self.pool).await?;

        // 网站配置表
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS websites (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                base_url TEXT NOT NULL,
                local_storage TEXT NOT NULL DEFAULT '[]',
                is_default INTEGER NOT NULL DEFAULT 0,
                spider TEXT NOT NULL DEFAULT 'd1'
            )
        "#).execute(&self.pool).await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_websites_is_default ON websites(is_default DESC)").execute(&self.pool).await?;

        // yt-dlp 下载任务表（简化版，不带 thumbnail 列）
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS ytdlp_tasks (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                progress INTEGER NOT NULL DEFAULT 0,
                file_path TEXT,
                status TEXT NOT NULL,
                message TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                completed_at TEXT
            )
        "#).execute(&self.pool).await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_ytdlp_tasks_status ON ytdlp_tasks(status)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_ytdlp_tasks_created_at ON ytdlp_tasks(created_at DESC)").execute(&self.pool).await?;

        // 本地视频表
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS local_videos (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_size TEXT DEFAULT '',
                duration TEXT DEFAULT '',
                resolution TEXT DEFAULT '',
                added_at TEXT NOT NULL
            )
        "#).execute(&self.pool).await?;

        // 删除旧的 thumbnail_path 列（SQLite 不支持 DROP COLUMN，通过重命名表实现）
        // 这里我们只删除索引，字段保留但不使用
        let _ = sqlx::query("DROP INDEX IF EXISTS idx_local_videos_thumbnail")
            .execute(&self.pool)
            .await;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_local_videos_added_at ON local_videos(added_at DESC)").execute(&self.pool).await?;

        Ok(())
    }

    /// 获取所有视频（按创建时间倒序）
    pub async fn get_all_videos(&self) -> Result<Vec<VideoItem>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count FROM videos ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(row_to_video_item(&row)?);
        }
        Ok(videos)
    }

    /// 根据视频ID列表获取视频
    pub async fn get_videos_by_ids(&self, ids: &[String]) -> Result<Vec<VideoItem>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // 构建占位符: ?,?,?
        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count FROM videos WHERE id IN ({}) ORDER BY created_at DESC",
            placeholders.join(",")
        );

        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(&self.pool).await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(row_to_video_item(&row)?);
        }
        Ok(videos)
    }

    /// 分页获取视频
    pub async fn get_videos_paginated(
        &self,
        page: i32,
        page_size: i32,
    ) -> Result<PaginatedVideos, sqlx::Error> {
        let offset = (page - 1) * page_size;

        // 获取总数
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM videos")
            .fetch_one(&self.pool)
            .await?;

        // 获取分页数据
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count FROM videos ORDER BY created_at DESC LIMIT ? OFFSET ?")
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(row_to_video_item(&row)?);
        }

        let videos_len = videos.len();
        Ok(PaginatedVideos {
            videos,
            total,
            page,
            page_size,
            has_more: (offset as i64) + (videos_len as i64) < total,
        })
    }

    /// 搜索视频
    pub async fn search_videos(
        &self,
        query: &str,
        page: i32,
        page_size: i32,
    ) -> Result<PaginatedVideos, sqlx::Error> {
        let search_pattern = format!("%{}%", query.to_uppercase());
        let offset = (page - 1) * page_size;

        // 获取总数
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM videos WHERE UPPER(name) LIKE ? OR UPPER(id) LIKE ?"
        )
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(&self.pool)
            .await?;

        // 获取分页数据
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count FROM videos WHERE UPPER(name) LIKE ? OR UPPER(id) LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(row_to_video_item(&row)?);
        }

        let videos_len = videos.len();
        Ok(PaginatedVideos {
            videos,
            total,
            page,
            page_size,
            has_more: (offset as i64) + (videos_len as i64) < total,
        })
    }

    /// 按状态筛选视频
    pub async fn get_videos_by_status(
        &self,
        status: VideoStatus,
        page: i32,
        page_size: i32,
    ) -> Result<PaginatedVideos, sqlx::Error> {
        let status_str = serde_json::to_string(&status).unwrap_or_default();
        let offset = (page - 1) * page_size;

        // 获取总数
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM videos WHERE status = ?"
        )
            .bind(&status_str)
            .fetch_one(&self.pool)
            .await?;

        // 获取分页数据
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count FROM videos WHERE status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
            .bind(&status_str)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(row_to_video_item(&row)?);
        }

        let videos_len = videos.len();
        Ok(PaginatedVideos {
            videos,
            total,
            page,
            page_size,
            has_more: (offset as i64) + (videos_len as i64) < total,
        })
    }

    /// 按网站名称获取视频（分页）
    pub async fn get_videos_by_website(
        &self,
        website_name: &str,
        page: i32,
        page_size: i32,
    ) -> Result<PaginatedVideos, sqlx::Error> {
        let offset = (page - 1) * page_size;

        // 获取总数
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM videos WHERE website_name = ?"
        )
            .bind(website_name)
            .fetch_one(&self.pool)
            .await?;

        // 获取分页数据
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count FROM videos WHERE website_name = ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
            .bind(website_name)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(row_to_video_item(&row)?);
        }

        let videos_len = videos.len();
        Ok(PaginatedVideos {
            videos,
            total,
            page,
            page_size,
            has_more: (offset as i64) + (videos_len as i64) < total,
        })
    }

    /// 添加视频
    pub async fn add_video(&self, video: &VideoItem) -> Result<(), sqlx::Error> {
        let status_str = serde_json::to_string(&video.status).unwrap_or_default();
        let created_at_str = video.created_at.to_rfc3339();
        let downloaded_at_str = video.downloaded_at.map(|d| d.to_rfc3339());
        sqlx::query(r#"
            INSERT OR REPLACE INTO videos (id, name, m3u8_url, status, created_at, downloaded_at, scrape_id, website_name, cover_url, favorite_count, view_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#)
            .bind(video.id.clone())
            .bind(video.name.clone())
            .bind(video.m3u8_url.clone())
            .bind(status_str)
            .bind(created_at_str)
            .bind(downloaded_at_str)
            .bind(video.scrape_id.clone())
            .bind(video.website_name.clone())
            .bind(video.cover_url.clone())
            .bind(video.favorite_count.unwrap_or(0))
            .bind(video.view_count.unwrap_or(0))
            .execute(&self.pool).await?;
        Ok(())
    }

    /// 检查视频是否已存在（通过 scrape_id 和 website_name）
    pub async fn video_exists(&self, scrape_id: &str, website_name: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM videos WHERE scrape_id = ? AND website_name = ?"
        )
            .bind(scrape_id)
            .bind(website_name)
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }

    /// 更新视频状态
    pub async fn update_video_status(
        &self,
        id: &str,
        status: VideoStatus,
        downloaded_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        let status_str = serde_json::to_string(&status).unwrap_or_default();
        let downloaded_at_str = downloaded_at.map(|d| d.to_rfc3339());
        sqlx::query(r#"
            UPDATE videos
            SET status = ?, downloaded_at = ?
            WHERE id = ?
        "#)
            .bind(status_str)
            .bind(downloaded_at_str)
            .bind(id)
            .execute(&self.pool).await?;
        Ok(())
    }

    /// 删除视频
    pub async fn delete_video(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM videos WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 根据名称更新视频状态
    pub async fn update_video_status_by_name(
        &self,
        name: &str,
        status: VideoStatus,
        downloaded_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        let status_str = serde_json::to_string(&status).unwrap_or_default();
        let downloaded_at_str = downloaded_at.map(|d| d.to_rfc3339());
        sqlx::query(r#"
            UPDATE videos
            SET status = ?, downloaded_at = ?
            WHERE name = ?
        "#)
            .bind(status_str)
            .bind(downloaded_at_str)
            .bind(name)
            .execute(&self.pool).await?;
        Ok(())
    }

    /// 清空已下载的视频
    pub async fn clear_downloaded(&self) -> Result<(), sqlx::Error> {
        let status_str = serde_json::to_string(&VideoStatus::Downloaded).unwrap_or_default();
        sqlx::query("DELETE FROM videos WHERE status = ?")
            .bind(status_str)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ===== 配置管理 =====

    /// 获取配置
    pub async fn get_config(&self) -> Result<AppConfig, sqlx::Error> {
        let download_path = self.get_setting("download_path").await?
            .unwrap_or_else(|| "./downloads".to_string());

        let default_quality = self.get_setting("default_quality").await?
            .unwrap_or_else(|| "auto".to_string());

        let local_storage_json = self.get_setting("local_storage").await?;
        let local_storage: Vec<LocalStorageItem> = if let Some(json) = local_storage_json {
            serde_json::from_str(&json).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(AppConfig {
            download_path,
            local_storage,
            default_quality,
        })
    }

    /// 保存完整配置
    pub async fn save_config(&self, config: &AppConfig) -> Result<(), sqlx::Error> {
        self.set_setting("download_path", &config.download_path).await?;
        self.set_setting("default_quality", &config.default_quality).await?;
        let local_storage_json = serde_json::to_string(&config.local_storage)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        self.set_setting("local_storage", &local_storage_json).await?;
        Ok(())
    }

    /// 获取 yt-dlp 配置
    pub async fn get_ytdlp_config(&self) -> Result<YtdlpConfig, sqlx::Error> {
        let quality = self.get_setting("ytdlp_quality").await?
            .unwrap_or_else(|| "Best".to_string());
        let format = self.get_setting("ytdlp_format").await?
            .unwrap_or_else(|| "mp4".to_string());
        let audio_format = self.get_setting("ytdlp_audio_format").await?
            .unwrap_or_else(|| "mp3".to_string());
        let concurrent_downloads: i32 = self.get_setting("ytdlp_concurrent_downloads").await?
            .unwrap_or_else(|| "3".to_string())
            .parse()
            .unwrap_or(3);

        let subtitles_str = self.get_setting("ytdlp_subtitles").await?
            .unwrap_or_else(|| "false".to_string());
        let subtitles = subtitles_str.parse().unwrap_or(false);

        let subtitle_langs = self.get_setting("ytdlp_subtitle_langs").await?
            .unwrap_or_else(|| "zh-CN,zh-Hans,zh-Hant,en".to_string());

        let thumbnail_str = self.get_setting("ytdlp_thumbnail").await?
            .unwrap_or_else(|| "false".to_string());
        let thumbnail = thumbnail_str.parse().unwrap_or(false);

        let audio_only_str = self.get_setting("ytdlp_audio_only").await?
            .unwrap_or_else(|| "false".to_string());
        let audio_only = audio_only_str.parse().unwrap_or(false);

        let merge_video_str = self.get_setting("ytdlp_merge_video").await?
            .unwrap_or_else(|| "true".to_string());
        let merge_video = merge_video_str.parse().unwrap_or(true);

        let extra_options = self.get_setting("ytdlp_extra_options").await?
            .unwrap_or_default();

        Ok(YtdlpConfig {
            quality: quality.parse().unwrap_or(0),
            format,
            subtitles,
            subtitle_langs,
            thumbnail,
            audio_only,
            audio_format,
            merge_video,
            concurrent_downloads: concurrent_downloads as u8,
            extra_options,
        })
    }

    /// 保存 yt-dlp 配置
    pub async fn save_ytdlp_config(&self, config: &YtdlpConfig) -> Result<(), sqlx::Error> {
        self.set_setting("ytdlp_quality", &config.quality.to_string()).await?;
        self.set_setting("ytdlp_format", &config.format).await?;
        self.set_setting("ytdlp_audio_format", &config.audio_format).await?;
        self.set_setting("ytdlp_concurrent_downloads", &config.concurrent_downloads.to_string()).await?;
        self.set_setting("ytdlp_subtitles", &config.subtitles.to_string()).await?;
        self.set_setting("ytdlp_subtitle_langs", &config.subtitle_langs).await?;
        self.set_setting("ytdlp_thumbnail", &config.thumbnail.to_string()).await?;
        self.set_setting("ytdlp_audio_only", &config.audio_only.to_string()).await?;
        self.set_setting("ytdlp_merge_video", &config.merge_video.to_string()).await?;
        self.set_setting("ytdlp_extra_options", &config.extra_options).await?;
        Ok(())
    }

    /// 获取单个配置项
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>, sqlx::Error> {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = ?"
        )
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result)
    }

    /// 设置单个配置项
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)"
        )
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ===== 网站管理 =====

    /// 获取所有网站
    pub async fn get_all_websites(&self) -> Result<Vec<Website>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, name, base_url, local_storage, is_default, spider FROM websites ORDER BY is_default DESC, name ASC")
            .fetch_all(&self.pool)
            .await?;

        let mut websites = Vec::new();
        for row in rows {
            let local_storage_json: String = row.try_get("local_storage")?;
            let local_storage: Vec<LocalStorageItem> = serde_json::from_str(&local_storage_json)
                .unwrap_or_default();
            let is_default: i32 = row.try_get("is_default")?;
            let spider: String = row.try_get("spider")?;

            websites.push(Website {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                base_url: row.try_get("base_url")?,
                local_storage,
                is_default: is_default == 1,
                spider,
            });
        }
        Ok(websites)
    }

    /// 获取默认网站
    pub async fn get_default_website(&self) -> Result<Option<Website>, sqlx::Error> {
        let row = sqlx::query("SELECT id, name, base_url, local_storage, is_default, spider FROM websites WHERE is_default = 1 LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let local_storage_json: String = row.try_get("local_storage")?;
            let local_storage: Vec<LocalStorageItem> = serde_json::from_str(&local_storage_json)
                .unwrap_or_default();
            let spider: String = row.try_get("spider")?;

            Ok(Some(Website {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                base_url: row.try_get("base_url")?,
                local_storage,
                is_default: true,
                spider,
            }))
        } else {
            Ok(None)
        }
    }

    /// 根据网站名称获取网站配置
    pub async fn get_website_by_name(&self, name: &str) -> Result<Option<Website>, sqlx::Error> {
        let row = sqlx::query("SELECT id, name, base_url, local_storage, is_default, spider FROM websites WHERE name = ? LIMIT 1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let local_storage_json: String = row.try_get("local_storage")?;
            let local_storage: Vec<LocalStorageItem> = serde_json::from_str(&local_storage_json)
                .unwrap_or_default();
            let is_default: i32 = row.try_get("is_default")?;
            let spider: String = row.try_get("spider")?;

            Ok(Some(Website {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                base_url: row.try_get("base_url")?,
                local_storage,
                is_default: is_default == 1,
                spider,
            }))
        } else {
            Ok(None)
        }
    }

    /// 添加或更新网站
    pub async fn save_website(&self, website: &Website) -> Result<(), sqlx::Error> {
        let local_storage_json = serde_json::to_string(&website.local_storage)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let is_default = if website.is_default { 1 } else { 0 };

        sqlx::query(r#"
            INSERT OR REPLACE INTO websites (id, name, base_url, local_storage, is_default, spider)
            VALUES (?, ?, ?, ?, ?, ?)
        "#)
            .bind(website.id.clone())
            .bind(website.name.clone())
            .bind(website.base_url.clone())
            .bind(local_storage_json)
            .bind(is_default)
            .bind(website.spider.clone())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 删除网站
    pub async fn delete_website(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM websites WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 设置默认网站
    pub async fn set_default_website(&self, id: &str) -> Result<(), sqlx::Error> {
        // 先清除所有默认标记
        sqlx::query("UPDATE websites SET is_default = 0")
            .execute(&self.pool)
            .await?;

        // 设置新的默认网站
        sqlx::query("UPDATE websites SET is_default = 1 WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ===== yt-dlp 任务管理 =====

    /// 添加或更新下载任务（简化版）
    pub async fn save_ytdlp_task(&self, task: &YtdlpTask) -> Result<(), sqlx::Error> {
        let created_at = task.created_at.to_rfc3339();
        let completed_at = task.completed_at.map(|d| d.to_rfc3339());

        sqlx::query(r#"
            INSERT OR REPLACE INTO ytdlp_tasks
            (id, url, title, progress, file_path, status, message, created_at, completed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#)
            .bind(task.id.clone())
            .bind(task.url.clone())
            .bind(task.title.clone())
            .bind(task.progress as i64)
            .bind(task.file_path.clone())
            .bind(format!("{:?}", task.status))
            .bind(task.message.clone())
            .bind(created_at)
            .bind(completed_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 获取所有下载任务
    pub async fn get_all_ytdlp_tasks(&self) -> Result<Vec<YtdlpTask>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM ytdlp_tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row_to_ytdlp_task(&row)?);
        }
        Ok(tasks)
    }

    /// 按 ID 获取下载任务
    pub async fn get_ytdlp_task_by_id(&self, id: &str) -> Result<Option<YtdlpTask>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM ytdlp_tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(r) = row {
            Ok(Some(row_to_ytdlp_task(&r)?))
        } else {
            Ok(None)
        }
    }

    /// 获取下载任务（按状态）
    pub async fn get_ytdlp_tasks_by_status(&self, status: &str) -> Result<Vec<YtdlpTask>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM ytdlp_tasks WHERE status = ? ORDER BY created_at DESC")
            .bind(status)
            .fetch_all(&self.pool)
            .await?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row_to_ytdlp_task(&row)?);
        }
        Ok(tasks)
    }

    /// 删除下载任务
    pub async fn delete_ytdlp_task(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM ytdlp_tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 只更新任务进度（用于断点续传）
    pub async fn update_ytdlp_task_progress(&self, id: &str, progress: u8, file_path: Option<String>) -> Result<(), sqlx::Error> {
        sqlx::query(r#"
            UPDATE ytdlp_tasks
            SET progress = ?, file_path = ?
            WHERE id = ?
        "#)
            .bind(progress as i64)
            .bind(file_path)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 清理已完成/失败的任务
    pub async fn cleanup_ytdlp_tasks(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM ytdlp_tasks WHERE status IN ('Completed', 'Failed', 'Cancelled')")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ===== 本地视频管理 =====

    /// 从数据库行解析 LocalVideo
    fn row_to_local_video(row: &SqliteRow) -> Result<LocalVideo, sqlx::Error> {
        let id: String = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let file_path: String = row.try_get("file_path")?;
        let file_size: String = row.try_get("file_size").unwrap_or_default();
        let duration: String = row.try_get("duration").unwrap_or_default();
        let resolution: String = row.try_get("resolution").unwrap_or_default();
        let added_at_str: String = row.try_get("added_at")?;
        let added_at: DateTime<Utc> = added_at_str.parse()
            .unwrap_or_else(|_| Utc::now());

        Ok(LocalVideo {
            id,
            name,
            file_path,
            file_size,
            duration,
            resolution,
            added_at,
        })
    }

    /// 添加本地视频
    pub async fn add_local_video(&self, video: &LocalVideo) -> Result<(), sqlx::Error> {
        let added_at_str = video.added_at.to_rfc3339();
        sqlx::query(r#"
            INSERT OR REPLACE INTO local_videos (id, name, file_path, file_size, duration, resolution, added_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        "#)
            .bind(video.id.clone())
            .bind(video.name.clone())
            .bind(video.file_path.clone())
            .bind(video.file_size.clone())
            .bind(video.duration.clone())
            .bind(video.resolution.clone())
            .bind(added_at_str)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 获取所有本地视频
    pub async fn get_all_local_videos(&self) -> Result<Vec<LocalVideo>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, name, file_path, file_size, duration, resolution, added_at FROM local_videos ORDER BY added_at DESC")
            .fetch_all(&self.pool)
            .await?;

        let mut videos = Vec::new();
        for row in rows {
            videos.push(Self::row_to_local_video(&row)?);
        }
        Ok(videos)
    }

    /// 删除本地视频
    pub async fn delete_local_video(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM local_videos WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 检查本地视频是否已存在（通过文件路径）
    pub async fn local_video_exists(&self, file_path: &str) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_videos WHERE file_path = ?"
        )
            .bind(file_path)
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }

    /// 关闭数据库连接
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
