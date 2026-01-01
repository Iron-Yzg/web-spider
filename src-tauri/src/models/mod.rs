use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 视频条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoItem {
    pub id: String,
    pub name: String,
    pub m3u8_url: String,
    pub status: VideoStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub downloaded_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for VideoItem {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: String::new(),
            m3u8_url: String::new(),
            status: VideoStatus::Pending,
            created_at: chrono::Utc::now(),
            downloaded_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VideoStatus {
    Pending,      // 待爬取
    Scraped,      // 已爬取待下载
    Downloading,  // 下载中
    Downloaded,   // 已下载
    Failed,       // 失败
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub download_path: String,
    pub local_storage: Vec<LocalStorageItem>,
    pub default_quality: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            download_path: "./downloads".to_string(),
            local_storage: Vec::new(),
            default_quality: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageItem {
    pub key: String,
    pub value: String,
}

/// 爬取结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResult {
    pub success: bool,
    pub name: String,
    pub m3u8_url: String,
    pub message: String,
}

/// 下载进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub video_id: String,
    pub progress: u8,
    pub status: String,
    pub speed: String,
    pub eta: String,
}
