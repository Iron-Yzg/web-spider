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
    /// 播放数
    pub view_count: Option<i64>,
    /// 收藏数
    pub favorite_count: Option<i64>,
    /// 封面图片URL（页面有URL则用URL，否则用视频第一帧的Base64）
    pub cover_url: Option<String>,
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

// ==================== yt-dlp 下载相关模型 ====================

/// yt-dlp 下载配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtdlpConfig {
    /// 视频质量预设
    pub quality: VideoQuality,
    /// 视频格式 (mp4, webm, etc.)
    pub format: String,
    /// 是否下载字幕
    pub subtitles: bool,
    /// 字幕语言 (多个用逗号分隔)
    pub subtitle_langs: String,
    /// 是否下载封面
    pub thumbnail: bool,
    /// 是否下载音频
    pub audio_only: bool,
    /// 音频格式 (mp3, m4a, etc.)
    pub audio_format: String,
    /// 下载后是否合并视频
    pub merge_video: bool,
    /// 下载线程数
    pub concurrent_downloads: u8,
    /// 其他 yt-dlp 选项 (格式为 "--option value")
    pub extra_options: String,
}

impl Default for YtdlpConfig {
    fn default() -> Self {
        Self {
            quality: VideoQuality::Best,
            format: "mp4".to_string(),
            subtitles: false,
            subtitle_langs: "zh-CN,zh-Hans,zh-Hant,en".to_string(),
            thumbnail: false,
            audio_only: false,
            audio_format: "mp3".to_string(),
            merge_video: true,
            concurrent_downloads: 3,
            extra_options: String::new(),
        }
    }
}

/// 视频质量预设
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoQuality {
    Best,       // 最佳质量
    High,       // 1080p
    Medium,     // 720p
    Low,        // 480p
    Worst,      // 最差质量
    AudioOnly,  // 仅音频
}

impl std::fmt::Display for VideoQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoQuality::Best => write!(f, "Best"),
            VideoQuality::High => write!(f, "High"),
            VideoQuality::Medium => write!(f, "Medium"),
            VideoQuality::Low => write!(f, "Low"),
            VideoQuality::Worst => write!(f, "Worst"),
            VideoQuality::AudioOnly => write!(f, "AudioOnly"),
        }
    }
}

impl VideoQuality {
    /// 转换为 yt-dlp 的 format 参数
    pub fn to_format_string(&self) -> String {
        match self {
            VideoQuality::Best => "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best".to_string(),
            VideoQuality::High => "bestvideo[height<=1080][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height<=1080]+bestaudio/best".to_string(),
            VideoQuality::Medium => "bestvideo[height<=720][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height<=720]+bestaudio/best".to_string(),
            VideoQuality::Low => "bestvideo[height<=480][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height<=480]+bestaudio/best".to_string(),
            VideoQuality::Worst => "worstvideo[ext=mp4]+worstaudio[ext=m4a]/worstvideo+worstaudio".to_string(),
            VideoQuality::AudioOnly => "bestaudio[ext=m4a]".to_string(),
        }
    }

    /// 从字符串创建
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "high" => VideoQuality::High,
            "medium" => VideoQuality::Medium,
            "low" => VideoQuality::Low,
            "worst" => VideoQuality::Worst,
            "audioonly" | "audio_only" => VideoQuality::AudioOnly,
            _ => VideoQuality::Best,
        }
    }
}

/// yt-dlp 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum YtdlpTaskStatus {
    Pending,      // 等待中
    Queued,       // 已加入队列
    Downloading,   // 下载中
    Paused,       // 已暂停
    Completed,    // 已完成
    Failed,       // 失败
    Cancelled,    // 已取消
}

/// yt-dlp 下载任务（简化版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtdlpTask {
    pub id: String,
    /// 视频URL（下载页地址）
    pub url: String,
    /// 任务标题（视频名称）
    pub title: String,
    /// 视频封面地址
    pub thumbnail: Option<String>,
    /// 下载进度 (0-100)
    pub progress: u8,
    /// 下载速度 (如 "229.80KiB/s")
    pub speed: String,
    /// 下载后的本地文件路径
    pub file_path: Option<String>,
    /// 下载状态
    pub status: YtdlpTaskStatus,
    /// 状态消息
    pub message: String,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 完成时间
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for YtdlpTask {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            url: String::new(),
            title: String::new(),
            thumbnail: None,
            progress: 0,
            speed: String::new(),
            file_path: None,
            status: YtdlpTaskStatus::Pending,
            message: String::new(),
            created_at: chrono::Utc::now(),
            completed_at: None,
        }
    }
}

/// yt-dlp 下载结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtdlpResult {
    pub success: bool,
    pub title: String,
    pub file_path: String,
    pub file_size: u64,
    pub thumbnail: Option<String>,
    pub message: String,
}
