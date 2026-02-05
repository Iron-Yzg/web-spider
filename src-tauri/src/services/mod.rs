use std::fs;
use std::path::PathBuf;

// 子模块
mod scraper;
mod downloader;
mod ytdlp;

// 重新导出 scraper 模块的内容
pub use scraper::{
    Scraper,
    ScraperFactory,
    ScraperInfo,
    get_available_scrapers,
};

pub use downloader::{
    check_ffmpeg,
    batch_download_concurrent,
};

pub use ytdlp::{
    get_video_info,
    download_video_with_continue,
    cancel_task,
    get_all_tasks,
    get_task_by_id,
    cleanup_tasks,
};

/// 统一获取 yt-dlp 和 ffmpeg 路径
/// 根据平台返回可执行文件路径，查找顺序：
/// 1. 可执行文件同目录下的 bin（生产环境）
/// 2. 项目根目录的 src-tauri/bin（开发环境）
/// 3. 项目根目录的 bin
/// 4. 父目录的 bin
/// 5. 回退到系统 PATH

/// 获取 yt-dlp 路径
pub fn get_ytdlp_path() -> PathBuf {
    let ytdlp_name = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "yt-dlp-aarch64-apple-darwin"
        } else {
            "yt-dlp-x86_64-apple-darwin"
        }
    } else if cfg!(target_os = "windows") {
        "yt-dlp-x86_64-pc-windows-msvc.exe"
    } else {
        "yt-dlp-x86_64-unknown-linux-gnu"
    };

    let search_paths = get_bin_search_paths();

    for bin_path in search_paths.into_iter().flatten() {
        let tool_path = bin_path.join(ytdlp_name);
        if tool_path.exists() {
            return tool_path;
        }
    }

    tracing::warn!("[yt-dlp] 未找到，回退到系统 PATH: {}", ytdlp_name);
    PathBuf::from(ytdlp_name)
}

/// 获取 ffmpeg 路径
pub fn get_ffmpeg_path() -> PathBuf {
    let ffmpeg_name = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "ffmpeg-aarch64-apple-darwin"
        } else {
            "ffmpeg-x86_64-apple-darwin"
        }
    } else if cfg!(target_os = "windows") {
        "ffmpeg-x86_64-pc-windows-msvc.exe"
    } else {
        "ffmpeg-x86_64-unknown-linux-gnu"
    };

    let search_paths = get_bin_search_paths();

    for bin_path in search_paths.into_iter().flatten() {
        let tool_path = bin_path.join(ffmpeg_name);
        if tool_path.exists() {
            return tool_path;
        }
    }

    tracing::warn!("[ffmpeg] 未找到，回退到系统 PATH: {}", ffmpeg_name);
    PathBuf::from(ffmpeg_name)
}

/// 获取 ffprobe 路径
pub fn get_ffprobe_path() -> PathBuf {
    let ffprobe_name = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "ffprobe-aarch64-apple-darwin"
        } else {
            "ffprobe-x86_64-apple-darwin"
        }
    } else if cfg!(target_os = "windows") {
        "ffprobe-x86_64-pc-windows-msvc.exe"
    } else {
        "ffprobe-x86_64-unknown-linux-gnu"
    };

    let search_paths = get_bin_search_paths();

    for bin_path in search_paths.into_iter().flatten() {
        let tool_path = bin_path.join(ffprobe_name);
        if tool_path.exists() {
            return tool_path;
        }
    }

    tracing::warn!("[ffprobe] 未找到，回退到系统 PATH: {}", ffprobe_name);
    PathBuf::from(ffprobe_name)
}

/// 获取 bin 目录搜索路径列表
fn get_bin_search_paths() -> Vec<Option<PathBuf>> {
    vec![
        // 1. 可执行文件同目录下的 bin（生产环境）
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.join("bin")),
        // 2. 项目根目录的 src-tauri/bin（开发环境）
        std::env::current_dir()
            .ok()
            .map(|p| p.join("src-tauri").join("bin")),
        // 3. 项目根目录的 bin（直接放在根目录）
        std::env::current_dir()
            .ok()
            .map(|p| p.join("bin")),
        // 4. 父目录的 bin
        std::env::current_dir()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.join("bin")),
    ]
}

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
