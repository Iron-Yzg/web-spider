use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::models::{AppConfig, VideoItem};

// 子模块
mod scraper;
mod downloader;

pub use scraper::scrape_m3u8;
pub use downloader::{
    download_m3u8,
    check_ffmpeg,
    is_downloading,
    batch_download_concurrent,
};

/// 应用状态
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub videos: Mutex<Vec<VideoItem>>,
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

        let config = Self::load_config(&data_dir);
        let videos = Self::load_videos(&data_dir);

        Self {
            config: Mutex::new(config),
            videos: Mutex::new(videos),
            data_dir,
        }
    }

    fn load_config(data_dir: &PathBuf) -> AppConfig {
        let config_path = data_dir.join("config.json");

        if let Ok(content) = fs::read_to_string(&config_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            AppConfig::default()
        }
    }

    fn load_videos(data_dir: &PathBuf) -> Vec<VideoItem> {
        let videos_path = data_dir.join("videos.json");
        if let Ok(content) = fs::read_to_string(&videos_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    pub fn save_config(&self) {
        let config = self.config.lock().unwrap();
        let config_path = self.data_dir.join("config.json");

        if let Ok(content) = serde_json::to_string_pretty(&*config) {
            let _ = fs::write(&config_path, content);
        }
    }

    pub fn save_videos(&self) {
        let videos = self.videos.lock().unwrap();
        let videos_path = self.data_dir.join("videos.json");
        if let Ok(content) = serde_json::to_string_pretty(&*videos) {
            let _ = fs::write(&videos_path, content);
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
