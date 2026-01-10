use std::fs;
use std::path::PathBuf;

// 子模块
mod scraper;
mod downloader;

// 重新导出 scraper 模块的内容
pub use scraper::{
    Scraper,
    ScraperFactory,
    ScraperInfo,
    get_available_scrapers,
};

pub use downloader::{
    download_m3u8,
    check_ffmpeg,
    is_downloading,
    batch_download_concurrent,
};

/// 应用状态（仅保留数据目录）
pub struct AppState {
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        let data_dir = get_app_data_dir();
        let _ = fs::create_dir_all(&data_dir);

        Self { data_dir }
    }
}

/// 获取应用数据目录，支持 macOS 和 iOS
fn get_app_data_dir() -> PathBuf {
    #[cfg(target_os = "ios")]
    {
        // iOS: 使用 Documents 目录（沙盒内）
        if let Some(documents) = dirs::document_dir() {
            return documents.join("web-spider");
        }
        // 回退到应用可写目录
        PathBuf::from("./Documents/web-spider")
    }

    #[cfg(not(target_os = "ios"))]
    {
        // macOS/Linux/Windows: 使用标准数据目录
        if let Some(home_dir) = dirs::home_dir() {
            // macOS: ~/Library/Application Support/web-spider
            // Linux: ~/.local/share/web-spider
            if home_dir.join("Library/Application Support").exists() {
                return home_dir.join("Library/Application Support/web-spider");
            }
            if let Some(data_dir) = dirs::data_dir() {
                return data_dir.join("web-spider");
            }
        }
        if let Some(data_dir) = dirs::data_dir() {
            data_dir.join("web-spider")
        } else {
            PathBuf::from("./data")
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
