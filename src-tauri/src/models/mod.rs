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
    /// 爬取时输入的视频ID
    pub scrape_id: String,
    /// 来源网站名称
    pub website_name: String,
    /// 封面图片地址
    pub cover_url: Option<String>,
    /// 收藏数
    pub favorite_count: Option<i64>,
    /// 播放数
    pub view_count: Option<i64>,
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
            scrape_id: String::new(),
            website_name: String::new(),
            cover_url: None,
            favorite_count: None,
            view_count: None,
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
    /// 视频ID（SRL爬虫使用实际视频ID，页码爬虫使用页码号）
    pub video_id: Option<String>,
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

/// 网站配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Website {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub local_storage: Vec<LocalStorageItem>,
    pub is_default: bool,
    /// 使用的爬虫名称，如 "d1"
    pub spider: String,
}

impl Default for Website {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: String::new(),
            base_url: String::new(),
            local_storage: Vec::new(),
            is_default: false,
            spider: "d1".to_string(),
        }
    }
}
