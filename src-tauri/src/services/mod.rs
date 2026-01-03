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
        // 使用应用数据目录，避免触发 Tauri 开发模式的重新构建
        // macOS: ~/Library/Application Support/web-spider
        let data_dir = if let Some(home_dir) = dirs::home_dir() {
            home_dir.join("Library/Application Support/web-spider")
        } else if let Some(data_dir) = dirs::data_dir() {
            data_dir.join("web-spider")
        } else {
            PathBuf::from("./data")
        };
        let _ = fs::create_dir_all(&data_dir);

        Self { data_dir }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
