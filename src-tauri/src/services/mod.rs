use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

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

/// 使用 Tauri 2.x Sidecar API 获取 sidecar 的实际路径
/// Tauri 会自动处理平台后缀，只需要提供基础名称
#[allow(dead_code)]
pub fn get_sidecar_path(app_handle: &AppHandle, name: &str) -> Result<PathBuf, String> {
    // Tauri 会自动处理平台后缀，只需要提供基础名称
    // 注意：sidecar() 参数是 "bin/ffmpeg"（不带扩展名和平台后缀）
    let sidecar_name = format!("binaries/{}", name);

    // 使用 Tauri ShellExt API 验证 sidecar 配置是否正确
    // 这会验证 capabilities 中的配置是否正确
    let _ = app_handle
        .shell()
        .sidecar(&sidecar_name)
        .map_err(|e| format!("获取 sidecar 命令失败: {}", e))?;

    // Tauri SidecarCommand 没有直接的 path 方法
    // 我们通过执行一个简单命令来获取输出，同时验证 sidecar 可用性
    // 然后使用回退路径作为 ffmpeg-location 的值
    // 对于实际执行，使用 sidecar.output() 或 sidecar.spawn()

    // 获取可执行文件所在目录的回退路径
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("获取当前 exe 路径失败: {}", e))?;
    let bin_dir = exe_path.parent()
        .ok_or_else(|| String::from("无法获取 exe 所在目录"))?
        .to_path_buf();

    // 尝试在 bin 目录或 Resources 目录中查找
    let binaries_dir = bin_dir.join("binaries");
    let resources_dir = bin_dir.join("..").join("Resources");

    let search_paths: Vec<PathBuf> = vec![
        binaries_dir.clone(),
        resources_dir.clone(),
        bin_dir.clone(),
    ];

    // 根据平台生成可能的文件名
    let possible_names = get_sidecar_names(name);

    for search_path in search_paths {
        if !search_path.exists() {
            continue;
        }
        for file_name in &possible_names {
            let candidate = search_path.join(file_name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // 如果都找不到，返回一个基于配置路径的占位符
    // Tauri 在实际执行时会解析正确的路径
    tracing::warn!("[sidecar] 未找到 {}, 使用配置路径", name);
    Ok(PathBuf::from(format!("bin/{}", name)))
}

/// 获取 sidecar 可能的所有文件名（跨平台）
fn get_sidecar_names(name: &str) -> Vec<String> {
    let base = match name {
        "yt-dlp" => "yt-dlp",
        "ffmpeg" => "ffmpeg",
        "ffprobe" => "ffprobe",
        _ => name,
    };

    vec![
        format!("{}-aarch64-apple-darwin", base),
        format!("{}-x86_64-apple-darwin", base),
        format!("{}-x86_64-pc-windows-msvc.exe", base),
        format!("{}-x86_64-unknown-linux-gnu", base),
        base.to_string(),
    ]
}

/// 获取 sidecar 二进制文件所在目录（用于 --ffmpeg-location 等参数）
#[allow(dead_code)]
pub fn get_sidecar_bin_dir(_app_handle: &AppHandle, name: &str) -> Result<PathBuf, String> {
    // 获取可执行文件所在目录
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("获取当前 exe 路径失败: {}", e))?;
    let bin_dir = exe_path.parent()
        .ok_or_else(|| String::from("无法获取 exe 所在目录"))?
        .to_path_buf();

    tracing::debug!("[sidecar] get_sidecar_bin_dir: exe={}, bin_dir={}", exe_path.display(), bin_dir.display());

    // 优先检查二进制文件是否直接在 bin_dir 下（macOS app bundle 场景）
    let possible_names = get_sidecar_names(name);
    for file_name in &possible_names {
        let candidate = bin_dir.join(file_name);
        if candidate.exists() {
            tracing::debug!("[sidecar] 找到 {} 在: {}", name, bin_dir.display());
            return Ok(bin_dir);
        }
    }

    // 检查 binaries 目录
    let binaries_dir = bin_dir.join("binaries");
    if binaries_dir.exists() {
        for file_name in &possible_names {
            let candidate = binaries_dir.join(file_name);
            if candidate.exists() {
                tracing::debug!("[sidecar] 找到 {} 在: {}", name, binaries_dir.display());
                return Ok(binaries_dir);
            }
        }
    }

    // 回退到 bin_dir
    tracing::warn!("[sidecar] 未找到 {}，回退到: {}", name, bin_dir.display());
    Ok(bin_dir)
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

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
