use sqlx::sqlite::{SqlitePool, SqliteRow, SqliteConnectOptions};
use sqlx::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use std::str::FromStr;

pub use crate::models::{VideoItem, VideoStatus};

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

    Ok(VideoItem {
        id,
        name,
        m3u8_url,
        status,
        created_at,
        downloaded_at,
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
pub struct Database {
    pool: SqlitePool,
    data_dir: PathBuf,
}

impl Database {
    /// 创建新的数据库实例
    pub async fn new(data_dir: &PathBuf) -> Result<Self, sqlx::Error> {
        let db_path = data_dir.join("web-spider.db");
        eprintln!("DB: Connecting to database: {:?}", db_path);

        // 确保目录存在
        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                eprintln!("DB: Creating directory: {:?}", parent);
                std::fs::create_dir_all(parent)?;
            }
        }

        // 使用 SqliteConnectOptions 来正确处理包含空格的路径
        let options = SqliteConnectOptions::from_str(&format!("file:{}", db_path.to_string_lossy()))
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?
            .create_if_missing(true);
        eprintln!("DB: Using SQLiteConnectOptions");

        let pool = SqlitePool::connect_with(options).await?;

        let db = Self {
            pool,
            data_dir: data_dir.clone(),
        };

        // 运行迁移
        db.run_migrations().await?;

        // 迁移 JSON 数据
        db.migrate_from_json().await?;

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
                downloaded_at TEXT
            )
        "#).execute(&self.pool).await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_videos_created_at ON videos(created_at DESC)").execute(&self.pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_videos_status ON videos(status)").execute(&self.pool).await?;

        Ok(())
    }

    /// 从 JSON 文件迁移数据
    async fn migrate_from_json(&self) -> Result<(), sqlx::Error> {
        let json_path = self.data_dir.join("videos.json");
        if !json_path.exists() {
            return Ok(());
        }

        // 检查是否已经迁移过
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM videos")
            .fetch_one(&self.pool)
            .await?;

        if count > 0 {
            return Ok(());
        }

        // 读取 JSON 文件
        let content = std::fs::read_to_string(&json_path).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let videos: Vec<VideoItem> = serde_json::from_str(&content)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        // 插入数据
        for video in videos {
            let status_str = serde_json::to_string(&video.status).unwrap_or_default();
            let created_at_str = video.created_at.to_rfc3339();
            let downloaded_at_str = video.downloaded_at.map(|d| d.to_rfc3339());
            sqlx::query(r#"
                INSERT OR REPLACE INTO videos (id, name, m3u8_url, status, created_at, downloaded_at)
                VALUES (?, ?, ?, ?, ?, ?)
            "#)
                .bind(video.id.clone())
                .bind(video.name.clone())
                .bind(video.m3u8_url.clone())
                .bind(status_str)
                .bind(created_at_str)
                .bind(downloaded_at_str)
                .execute(&self.pool).await?;
        }

        Ok(())
    }

    /// 获取所有视频（按创建时间倒序）
    pub async fn get_all_videos(&self) -> Result<Vec<VideoItem>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at FROM videos ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;

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
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at FROM videos ORDER BY created_at DESC LIMIT ? OFFSET ?")
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
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at FROM videos WHERE UPPER(name) LIKE ? OR UPPER(id) LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
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
        let rows = sqlx::query("SELECT id, name, m3u8_url, status, created_at, downloaded_at FROM videos WHERE status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
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

    /// 添加视频
    pub async fn add_video(&self, video: &VideoItem) -> Result<(), sqlx::Error> {
        let status_str = serde_json::to_string(&video.status).unwrap_or_default();
        let created_at_str = video.created_at.to_rfc3339();
        let downloaded_at_str = video.downloaded_at.map(|d| d.to_rfc3339());
        sqlx::query(r#"
            INSERT OR REPLACE INTO videos (id, name, m3u8_url, status, created_at, downloaded_at)
            VALUES (?, ?, ?, ?, ?, ?)
        "#)
            .bind(video.id.clone())
            .bind(video.name.clone())
            .bind(video.m3u8_url.clone())
            .bind(status_str)
            .bind(created_at_str)
            .bind(downloaded_at_str)
            .execute(&self.pool).await?;
        Ok(())
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

    /// 关闭数据库连接
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
