//! yt-dlp 统一下载模块
//!
//! 支持直接视频链接（m3u8/mp4/mkv等）和平台视频（YouTube/B站等）的下载
use crate::models::{DownloadProgress, YtdlpConfig, YtdlpResult, YtdlpTask, YtdlpTaskStatus};
use crate::services::{get_sidecar_path, get_sidecar_bin_dir};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tauri::AppHandle;

// ==================== 常量定义 ====================

/// URL 类型枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UrlType {
    /// HLS 流 (m3u8)
    Hls,
    /// 直接视频链接 (mp4/mkv/webm 等)
    DirectVideo,
    /// 平台视频 (YouTube/B站等)
    Platform,
}

// ==================== 静态变量 ====================

/// 运行中的下载进程 PID 存储（用于停止下载）
static RUNNING_PIDS: std::sync::LazyLock<tokio::sync::Mutex<std::collections::HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));

/// 记录被取消的任务（用于区分用户暂停和真实错误）
static CANCELLED_TASKS: std::sync::LazyLock<tokio::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(std::collections::HashSet::new()));

// ==================== 工具函数模块 ====================

/// 杀死指定 PID 的进程及其所有子进程
fn kill_process(pid: u32) {
    tracing::info!("[ytdlp-download] 开始终止进程树，主进程 PID: {}", pid);
    
    if cfg!(target_os = "windows") {
        // Windows: 使用 /T 参数终止进程及其所有子进程
        let output = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output();
        
        match output {
            Ok(result) => {
                if result.status.success() {
                    tracing::info!("[ytdlp-download] 成功终止进程树 PID: {}", pid);
                } else {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    tracing::warn!("[ytdlp-download] 终止进程失败 PID: {}, 错误: {}", pid, stderr);
                }
            }
            Err(e) => {
                tracing::error!("[ytdlp-download] 执行 taskkill 失败: {}", e);
            }
        }
    } else {
        // macOS/Linux: 先杀死所有子进程，再杀死主进程
        // 步骤1: 使用 pkill -P 终止所有子进程
        let child_output = std::process::Command::new("pkill")
            .args(["-9", "-P", &pid.to_string()])
            .output();
        
        match child_output {
            Ok(result) => {
                if result.status.success() {
                    tracing::info!("[ytdlp-download] 已终止所有子进程 of PID: {}", pid);
                }
                // pkill 返回 1 表示没有找到子进程，这也是正常的
            }
            Err(e) => {
                tracing::warn!("[ytdlp-download] 终止子进程失败: {}", e);
            }
        }
        
        // 给子进程一点时间终止
        std::thread::sleep(std::time::Duration::from_millis(200));
        
        // 步骤2: 杀死主进程
        let main_output = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .output();
        
        match main_output {
            Ok(result) => {
                if result.status.success() {
                    tracing::info!("[ytdlp-download] 成功终止主进程 PID: {}", pid);
                } else {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    tracing::warn!("[ytdlp-download] 终止主进程失败 PID: {}, 错误: {}", pid, stderr);
                }
            }
            Err(e) => {
                tracing::error!("[ytdlp-download] 执行 kill 失败: {}", e);
            }
        }
    }
    
    // 额外等待时间确保进程完全终止
    std::thread::sleep(std::time::Duration::from_millis(300));
}

/// 清理文件名中的非法字符
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\?%*:|\"<>".contains(c) { '_' } else { c })
        .collect()
}

/// 格式化文件大小
fn format_file_size(bytes: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    const KB: u64 = 1024;

    if bytes >= GB {
        format!("{:.2}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// URL 解码（处理数据库中存储的编码 URL）
fn decode_url(url: &str) -> String {
    use percent_encoding::percent_decode_str;

    if let Ok(decoded) = percent_decode_str(url).decode_utf8() {
        let decoded = decoded.into_owned();
        // 修复常见的编码错误
        decoded
            .replace("%5C/", "/")
            .replace("%5C%5C", "/")
            .replace("\\/", "/")
            .replace("\\\\", "/")
    } else {
        url.to_string()
    }
}

/// 检测 URL 类型
pub fn detect_url_type(url: &str) -> UrlType {
    let url_lower = url.to_lowercase();

    // HLS 流
    if url_lower.contains(".m3u8") {
        return UrlType::Hls;
    }

    // 直接视频链接（常见视频扩展名）
    let video_extensions = [".mp4", ".mkv", ".webm", ".mov", ".avi", ".flv", ".wmv"];
    for ext in video_extensions {
        if url_lower.contains(ext) {
            return UrlType::DirectVideo;
        }
    }

    // 其他都视为平台视频
    UrlType::Platform
}

pub async fn get_cast_stream_url(app_handle: &AppHandle, input_url: &str) -> Result<String, String> {
    let url = decode_url(input_url);
    if detect_url_type(&url) != UrlType::Platform {
        return Ok(url);
    }

    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;

    let primary_args = vec![
        "-g".to_string(),
        "--no-playlist".to_string(),
        "-f".to_string(),
        "b[ext=mp4]/bv*[ext=mp4]+ba[ext=m4a]/b".to_string(),
        "--cookies-from-browser".to_string(),
        "chrome".to_string(),
        url.clone(),
    ];

    let output = Command::new(&ytdlp_path)
        .args(&primary_args)
        .output()
        .await
        .map_err(|e| format!("执行 yt-dlp 获取直链失败: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut lines: Vec<String> = stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("http://") || l.starts_with("https://"))
        .collect();

    if lines.is_empty() {
        let fallback_args = vec![
            "-g".to_string(),
            "--no-playlist".to_string(),
            "-f".to_string(),
            "b".to_string(),
            url.clone(),
        ];
        let fallback = Command::new(&ytdlp_path)
            .args(&fallback_args)
            .output()
            .await
            .map_err(|e| format!("执行 yt-dlp fallback 获取直链失败: {}", e))?;
        let fb_stdout = String::from_utf8_lossy(&fallback.stdout).to_string();
        lines = fb_stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| l.starts_with("http://") || l.starts_with("https://"))
            .collect();
    }

    if let Some(first) = lines.first() {
        tracing::info!("[ytdlp-cast] 解析到可投屏直链: {}", first);
        Ok(first.clone())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("未解析到可用视频直链，yt-dlp 输出: {}", stderr))
    }
}

/// 检查 ffmpeg 是否可用
pub fn check_ffmpeg(app_handle: &AppHandle) -> bool {
    match get_sidecar_path(app_handle, "ffmpeg") {
        Ok(path) => {
            std::process::Command::new(&path)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        Err(_) => false,
    }
}

// ==================== 进度解析模块 ====================

/// 解析 yt-dlp 输出获取进度
fn parse_progress(output: &str) -> (u8, String, String) {
    let mut progress = 0u8;
    let mut speed = String::new();
    let mut eta = String::new();

    // 跳过空行和非下载行
    if output.trim().is_empty() || !output.contains("[download") {
        return (progress, speed, eta);
    }

    // 方法1: 解析 progress-template 格式 [download:45.2%][2.50MiB/s][03:25]
    if let Some(caps) = regex::Regex::new(r#"\[download:\s*([\d.]+)\s*%\]\[([^\]]*)\]\[([^\]]*)\]"#)
        .unwrap()
        .captures(output)
    {
        if let Some(p) = caps.get(1) {
            if let Ok(pct) = p.as_str().parse::<f64>() {
                if pct >= 0.0 && pct <= 100.0 {
                    progress = pct as u8;
                }
            }
        }
        if let Some(s) = caps.get(2) {
            let speed_str = s.as_str().trim();
            if !speed_str.is_empty() && speed_str != "Unknown" {
                speed = speed_str.to_string();
            }
        }
        if let Some(e) = caps.get(3) {
            let eta_str = e.as_str().trim();
            if !eta_str.is_empty() && eta_str != "Unknown" {
                eta = eta_str.to_string();
            }
        }
        return (progress, speed, eta);
    }

    // 方法2: 解析标准格式 [download] 45.2% of 1.50GiB at 2.50MiB/s ETA 03:25
    if let Some(caps) = regex::Regex::new(r#"\[\s*download\s*\]\s*([\d.]+)\s*%.*?([\d.]+[KMG]?i?B/s).*?ETA\s*(\S+)"#)
        .unwrap()
        .captures(output)
    {
        if let Some(p) = caps.get(1) {
            progress = (p.as_str().parse::<f64>().unwrap_or(0.0) as u8).min(100);
        }
        if let Some(s) = caps.get(2) {
            speed = s.as_str().to_string();
        }
        if let Some(e) = caps.get(3) {
            eta = e.as_str().to_string();
        }
        return (progress, speed, eta);
    }

    (progress, speed, eta)
}

// ==================== 参数构建模块 ====================

/// 构建 yt-dlp 通用参数
fn build_common_args(
    output_path: &str,
    task_id: &str,
    ffmpeg_bin_dir: &PathBuf,
) -> Vec<String> {
    vec![
        "--newline".to_string(),
        "--no-check-certificate".to_string(), // 1. 忽略 SSL 证书错误（解决当前报错）
        "--prefer-insecure".to_string(),      // 2. 强制使用不安全连接（备选保障）
        "--output-na-placeholder".to_string(),
        "NA".to_string(),
        "--continue".to_string(), // 支持断点续传
        "--progress".to_string(),
        "--progress-template".to_string(),
        "[download:%(progress._percent_str)s][%(progress._speed_str)s][%(progress._eta_str)s]".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg_bin_dir.to_string_lossy().to_string(),
        "-o".to_string(),
        format!("{}/{}.%(ext)s", output_path, task_id),
    ]
}

/// 添加认证和模拟参数
fn add_auth_args(args: &mut Vec<String>) {
    args.push("--cookies-from-browser".to_string());
    args.push("chrome".to_string());
    args.push("--impersonate".to_string());
    args.push("chrome".to_string());
}

/// 为直链视频（m3u8/直接视频链接）构建参数
fn build_m3u8_video_args(
    args: &mut Vec<String>,
    config: &YtdlpConfig,
) {
    // 直链视频：使用简单处理
    // 注意：--continue 参数会自动处理断点续传，不需要特殊处理

    tracing::info!("[ytdlp-download] 直链视频：使用原生下载模式");

    args.push("-N".to_string());
    args.push("8".to_string());

    // --- 始终添加后处理参数 ---
    // yt-dlp 的 --continue 会自动处理已下载的部分
    // 如果临时文件损坏，yt-dlp 会自动重新下载
    args.push("--merge-output-format".to_string());
    args.push(config.format.clone());
    args.push("--postprocessor-args".to_string());
    args.push("ffmpeg:-c:v copy -c:a aac -bsf:a aac_adtstoasc -threads 2".to_string());
}

fn build_direct_video_args(
    args: &mut Vec<String>,
    config: &YtdlpConfig,
) {
    // 直链视频：使用简单处理
    // 注意：--continue 参数会自动处理断点续传，不需要特殊处理

    tracing::info!("[ytdlp-download] 直链视频：使用原生下载模式");

    args.push("-N".to_string());
    args.push("8".to_string());

    // --- 始终添加后处理参数 ---
    args.push("--merge-output-format".to_string());
    args.push(config.format.clone());
    args.push("--postprocessor-args".to_string());
    args.push("ffmpeg:-c:v copy -c:a aac -bsf:a aac_adtstoasc -threads 2".to_string());
}

/// 为平台视频构建参数
fn build_platform_video_args(
    args: &mut Vec<String>,
    config: &YtdlpConfig,
) {
    // 平台视频：使用完整的后处理

    // 添加后处理参数
    args.push("--postprocessor-args".to_string());
    args.push("ffmpeg:-movflags +faststart".to_string());

    // 封面
    if config.thumbnail {
        args.push("--write-thumbnail".to_string());
        args.push("--embed-thumbnail".to_string());
        args.push("--convert-thumbnails".to_string());
        args.push("jpg".to_string());
    }

    // 质量参数
    if config.audio_only {
        args.push("--extract-audio".to_string());
        args.push("--audio-format".to_string());
        args.push(config.audio_format.clone());
    } else {
        args.push("-f".to_string());
        args.push(build_format_string(config.quality));
    }

    // 合并格式
    if config.merge_video {
        args.push("--merge-output-format".to_string());
        args.push(config.format.clone());
    }

    // 字幕
    if config.subtitles {
        args.push("--write-subs".to_string());
        args.push("--sub-langs".to_string());
        args.push(config.subtitle_langs.clone());
    }

    // 额外选项
    if !config.extra_options.is_empty() {
        for opt in config.extra_options.split_whitespace() {
            args.push(opt.to_string());
        }
    }

    tracing::info!("[ytdlp-download] 平台视频：使用完整后处理模式");
}

/// 构建格式字符串
fn build_format_string(quality: u32) -> String {

    if quality <= 1080 {
        // 兼容性优先模式：强制 H.264 + AAC
        format!(
            "bestvideo[vcodec^=avc1][height<={h}]+bestaudio[acodec^=mp4a]/best[vcodec^=avc1][height<={h}]/best",
            h = quality
        )
    } else {
        // 画质优先模式：允许 VP9 (YouTube) 和 HEVC (B站)
        // 顺序：尝试 H.264 (如果有4K) -> 尝试任何4K -> 1080P H.264 保底
        format!(
            "bestvideo[height<={h}][vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo[height<={h}]+bestaudio/best[height<={h}]/best",
            h = quality
        )
    }
}

// ==================== 下载核心模块 ====================

/// 检查依赖工具是否可用
async fn check_dependencies(app_handle: &AppHandle) -> Result<PathBuf, String> {
    // 检查 yt-dlp
    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;
    let ytdlp_check = Command::new(&ytdlp_path)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("执行 yt-dlp 失败: {}", e))?;

    if !ytdlp_check.status.success() {
        return Err("yt-dlp 不可用".to_string());
    }

    // 检查 ffmpeg
    let ffmpeg_path = get_sidecar_path(app_handle, "ffmpeg")?;
    let ffmpeg_check = Command::new(&ffmpeg_path)
        .arg("-version")
        .output()
        .await
        .map_err(|e| format!("执行 ffmpeg 失败: {}", e))?;

    if !ffmpeg_check.status.success() {
        return Err("ffmpeg 不可用".to_string());
    }

    // 获取 ffmpeg 所在目录
    let ffmpeg_bin_dir = get_sidecar_bin_dir(app_handle, "ffmpeg")?;
    tracing::info!("[ytdlp-download] ffmpeg bin dir: {}", ffmpeg_bin_dir.display());

    Ok(ffmpeg_bin_dir)
}

/// 杀死可能存在的旧进程
async fn kill_old_process(task_id: &str) {
    let mut pids = RUNNING_PIDS.lock().await;
    if let Some(old_pid) = pids.remove(task_id) {
        tracing::info!("[ytdlp-download] 发现旧进程 PID: {}，正在终止进程树...", old_pid);
        kill_process(old_pid);
        // 等待足够时间让进程及其子进程完全终止
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    }
}

/// 执行 yt-dlp 下载并实时回调进度
async fn execute_ytdlp_download(
    ytdlp_path: &PathBuf,
    url: &str,
    args: Vec<String>,
    task_id: &str,
    title: &str,
    mut progress_callback: impl FnMut(YtdlpTask) + Send,
) -> Result<YtdlpResult, String> {
    // 记录 PID
    let task_id_clone = task_id.to_string();
    let title_clone = title.to_string();

    let mut child = Command::new(ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 yt-dlp 失败: {}", e))?;

    let pid = child.id().unwrap_or(0);
    {
        let mut pids = RUNNING_PIDS.lock().await;
        pids.insert(task_id_clone.clone(), pid);
    }

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut reader = BufReader::new(stdout);
    let mut buffer = String::new();

    // 进度回调循环
    loop {
        tokio::select! {
            result = reader.read_line(&mut buffer) => {
                match result {
                    Ok(0) => {
                        // stdout 关闭了，等待进程结束
                        tracing::info!("[ytdlp-download] stdout 已关闭，等待进程结束...");
                        break;
                    }
                    Ok(_) => {
                        let line = buffer.trim().to_string();
                        buffer.clear();

                        let (progress, speed, _eta) = parse_progress(&line);

                        // 只有当进度在0-99之间时才发送，100%不发送（等待合并完成）
                        if progress > 0 {
                            progress_callback(YtdlpTask {
                                id: task_id.to_string(),
                                url: url.to_string(),
                                title: title_clone.clone(),
                                progress: progress.clamp(0, 99),
                                speed: speed.clone(),
                                file_path: None,
                                status: YtdlpTaskStatus::Downloading,
                                resolution: String::new(),
                                file_size: String::new(),
                                message: format!("下载中 {}%", progress),
                                created_at: chrono::Utc::now(),
                                completed_at: None,
                            });
                        }
                    }
                    Err(_) => break,
                }
            }
            _ = child.wait() => {
                tracing::info!("[ytdlp-download] 进程已退出");
                break;
            }
        }
    }

    // 清理 PID
    {
        let mut pids = RUNNING_PIDS.lock().await;
        pids.remove(&task_id_clone);
    }

    let status = child.wait().await
        .map_err(|e| format!("等待 yt-dlp 失败: {}", e))?;

    // 读取错误信息
    let mut error_msg = String::new();
    let mut stderr_reader = BufReader::new(stderr);
    let mut buf = String::new();
    while let Ok(n) = stderr_reader.read_line(&mut buf).await {
        if n == 0 { break; }
        error_msg.push_str(&buf);
        buf.clear();
    }

    if !error_msg.is_empty() {
        tracing::info!("[ytdlp-download] stderr: {}", error_msg);
    }

    if !status.success() {
        let exit_code = status.code().unwrap_or(-1);
        
        // 检查是否是用户主动取消（被标记为取消的任务，退出码通常是 -9 或被信号终止）
        let was_cancelled = {
            let cancelled = CANCELLED_TASKS.lock().await;
            cancelled.contains(&task_id_clone)
        };
        
        if was_cancelled {
            // 用户主动取消，不输出错误日志
            tracing::info!("[ytdlp-download] 进程被用户终止，退出码: {}", exit_code);
            return Err(format!("进程被用户终止"));
        }
        
        let err_msg = if error_msg.is_empty() {
            format!("yt-dlp 进程退出，退出码: {} (可能是因为临时文件不存在或损坏，请重试从头下载)", exit_code)
        } else {
            format!("yt-dlp 执行出错 (退出码 {}): {}", exit_code, error_msg)
        };
        tracing::error!("[ytdlp-download] {}", err_msg);
        return Err(err_msg);
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    Ok(YtdlpResult {
        success: true,
        title: String::new(),
        file_path: String::new(),
        file_size: 0,
        message: "下载完成".to_string(),
    })
}

/// 查找并重命名输出文件
async fn find_and_rename_output(
    output_path: &str,
    task_id: &str,
    title: &str,
) -> Result<(PathBuf, u64), String> {
    let file_prefix = format!("{}.", task_id);
    let mut video_file: Option<PathBuf> = None;
    let mut image_files: Vec<PathBuf> = Vec::new();

    // 查找视频文件和封面图
    if let Ok(entries) = std::fs::read_dir(output_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();

            if name.starts_with(&file_prefix) {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                match ext {
                    "mp4" | "mkv" | "webm" | "mov" | "ts" => video_file = Some(path),
                    "jpg" | "webp" | "png" | "jpeg" => image_files.push(path),
                    _ => {}
                }
            }
        }
    }

    let path = video_file.ok_or("无法找到视频文件")?;

    // 获取文件大小
    let file_size = std::fs::metadata(&path)
        .map(|m| m.len())
        .unwrap_or(0);

    // 重命名文件
    if !title.is_empty() {
        let sanitized_title = sanitize_filename(title);
        let ext = path.extension().unwrap_or_default().to_string_lossy();
        let final_path = PathBuf::from(output_path).join(format!("{}.{}", sanitized_title, ext));

        if final_path.exists() {
            let _ = std::fs::remove_file(&final_path);
        }

        if std::fs::rename(&path, &final_path).is_ok() {
            // 清理残留的封面图
            for img in image_files {
                let _ = std::fs::remove_file(img);
            }
            return Ok((final_path, file_size));
        }
    }

    Ok((path, file_size))
}

// ==================== 公开接口 ====================

/// 获取视频信息（不下载）
pub async fn get_video_info(
    app_handle: &AppHandle,
    url: &str,
    quality: u32,
) -> Result<YtdlpTask, String> {
    // 检查依赖
    check_dependencies(app_handle).await?;

    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;

    let args = vec![
        "--dump-json".to_string(),
        "--no-download".to_string(),
        "-f".to_string(),
        build_format_string(quality),
        url.to_string(),
    ];

    let output = Command::new(&ytdlp_path)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("获取视频信息失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("获取视频信息失败: {}", stderr));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析视频信息失败: {}", e))?;

    let title = json["title"].as_str().unwrap_or("未知标题").to_string();
    let resolution = json["resolution"].as_str().unwrap_or("").to_string();
    let file_size = json["filesize"].as_u64().unwrap_or(0);

    Ok(YtdlpTask {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        title,
        progress: 0,
        speed: String::new(),
        file_path: None,
        status: YtdlpTaskStatus::Pending,
        message: "等待下载".to_string(),
        created_at: chrono::Utc::now(),
        completed_at: None,
        resolution: resolution.to_string(),
        file_size: format_file_size(file_size),
    })
}

/// 下载视频（统一入口）
///
/// # 参数
/// - `app_handle`: Tauri 应用句柄
/// - `url`: 视频 URL（支持 m3u8/mp4/平台视频等）
/// - `output_path`: 输出目录
/// - `task_id`: 任务 ID（用于临时文件名）
/// - `title`: 视频标题（用于重命名最终文件）
/// - `config`: 下载配置
/// - `progress_callback`: 进度回调函数
pub async fn download_video(
    app_handle: &AppHandle,
    url: &str,
    output_path: &str,
    task_id: &str,
    title: &str,
    config: &YtdlpConfig,
    mut progress_callback: impl FnMut(YtdlpTask) + Send,
) -> Result<YtdlpResult, String> {
    // 1. 解码 URL
    let decoded_url = decode_url(url);
    tracing::info!("[ytdlp-download] URL 解码: {} -> {}", url, decoded_url);

    // 2. 检测 URL 类型
    let url_type = detect_url_type(&decoded_url);
    tracing::info!("[ytdlp-download] URL 类型: {:?}", url_type);

    // 3. 检查依赖
    let ffmpeg_bin_dir = check_dependencies(app_handle).await?;

    // 4. 确保输出目录存在
    std::fs::create_dir_all(output_path)
        .map_err(|e| format!("创建输出目录失败: {}", e))?;

    // 5. 杀死可能存在的旧进程
    kill_old_process(task_id).await;

    // 6. 构建参数（始终使用相同参数，--continue 会自动处理断点续传）
    let mut args = build_common_args(output_path, task_id, &ffmpeg_bin_dir);

    // 7. 根据 URL 类型添加特定参数
    match url_type {
        UrlType::Hls => {
            build_m3u8_video_args(&mut args, config);
        }
        UrlType::DirectVideo => {
            build_direct_video_args(&mut args, config);
        }
        UrlType::Platform => {
            build_platform_video_args(&mut args, config);
        }
    }

    // 9. 添加认证参数
    add_auth_args(&mut args);

    // 10. 添加 URL
    args.push(decoded_url.clone());

    // 11. 打印完整命令
    let full_cmd = format!("yt-dlp {}", args.join(" "));
    tracing::info!("[ytdlp-download] 开始下载: {}", full_cmd);

    // 12. 发送初始状态
    progress_callback(YtdlpTask {
        id: task_id.to_string(),
        url: decoded_url.clone(),
        title: title.to_string(),
        progress: 0,
        speed: String::new(),
        file_path: None,
        status: YtdlpTaskStatus::Downloading,
        message: "正在初始化...".to_string(),
        created_at: chrono::Utc::now(),
        completed_at: None,
        resolution: String::new(),
        file_size: String::new(),
    });

    // 13. 执行下载
    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;
    let result = execute_ytdlp_download(&ytdlp_path, &decoded_url, args, task_id, title, |task| {
        progress_callback(task);
    }).await;

    // 14. 处理结果
    match result {
        Ok(mut ytdlp_result) => {
            // 15. 查找并重命名输出文件
            match find_and_rename_output(output_path, task_id, title).await {
                Ok((final_path, file_size)) => {
                    ytdlp_result.title = title.to_string();
                    ytdlp_result.file_path = final_path.to_string_lossy().to_string();
                    ytdlp_result.file_size = file_size;
                    ytdlp_result.message = "下载完成".to_string();
                    ytdlp_result.success = true;

                    tracing::info!("[ytdlp-download] 下载完成: {}", final_path.display());

                    progress_callback(YtdlpTask {
                        id: task_id.to_string(),
                        url: decoded_url.clone(),
                        title: title.to_string(),
                        progress: 100,
                        speed: String::new(),
                        file_path: Some(ytdlp_result.file_path.clone()),
                        status: YtdlpTaskStatus::Completed,
                        message: "下载完成".to_string(),
                        created_at: chrono::Utc::now(),
                        completed_at: Some(chrono::Utc::now()),
                        resolution: String::new(),
                        file_size: format_file_size(ytdlp_result.file_size),
                    });
                }
                Err(e) => {
                    tracing::warn!("[ytdlp-download] 查找文件失败: {}", e);
                }
            }

            // 16. 发送完成状态
            progress_callback(YtdlpTask {
                id: task_id.to_string(),
                url: decoded_url.clone(),
                title: title.to_string(),
                progress: 100,
                speed: String::new(),
                file_path: Some(ytdlp_result.file_path.clone()),
                status: YtdlpTaskStatus::Completed,
                message: "下载完成".to_string(),
                created_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                resolution: String::new(),
                file_size: format_file_size(ytdlp_result.file_size),
            });

            Ok(ytdlp_result)
        }
        Err(e) => {
            // 检查是否是用户主动暂停（被取消）
            let was_cancelled = {
                let mut cancelled = CANCELLED_TASKS.lock().await;
                cancelled.remove(task_id)
            };

            if was_cancelled {
                tracing::info!("[ytdlp-download] 任务被用户暂停: {}", task_id);
                // 不发送失败状态，让调用方(stop_ytdlp_task)处理暂停状态
                // 避免进度被重置为0
                return Err(e);
            }

            tracing::error!("[ytdlp-download] 下载失败: {}", e);

            // 发送失败状态
            progress_callback(YtdlpTask {
                id: task_id.to_string(),
                url: decoded_url.clone(),
                title: title.to_string(),
                progress: 0,
                speed: String::new(),
                file_path: None,
                status: YtdlpTaskStatus::Failed,
                message: format!("下载失败: {}", e),
                created_at: chrono::Utc::now(),
                completed_at: None,
                resolution: String::new(),
                file_size: String::new(),
            });

            Err(e)
        }
    }
}

/// 取消下载任务
pub fn cancel_task(task_id: &str) -> bool {
    let result = futures::executor::block_on(async {
        // 标记任务为被取消（用户主动暂停）
        {
            let mut cancelled = CANCELLED_TASKS.lock().await;
            cancelled.insert(task_id.to_string());
        }
        
        let mut pids = RUNNING_PIDS.lock().await;
        if let Some(pid) = pids.remove(task_id) {
            tracing::info!("[ytdlp-download] 杀死进程: {} (PID: {})", task_id, pid);
            kill_process(pid);
            return Some(true);
        }
        None
    });

    result.is_some()
}

/// 并发批量下载视频
/// 参数:
/// - app_handle: Tauri 应用句柄
/// - videos: 下载列表，每项为 (视频ID, 视频标题, 视频URL, 输出目录)
/// - max_concurrent: 最大并发数
/// - progress_sender: 进度发送通道
/// 返回: 每项为 (视频ID, 下载结果)
pub async fn batch_download_concurrent(
    app_handle: &AppHandle,
    videos: Vec<(String, String, String, PathBuf)>,
    max_concurrent: usize,
    progress_sender: broadcast::Sender<DownloadProgress>,
) -> Vec<(String, Result<YtdlpResult, String>)> {
    use futures::stream::StreamExt;

    let config = YtdlpConfig::default();

    // 使用 futures::stream 并发执行下载
    let results = futures::stream::iter(videos.into_iter().map(|(id, name, m3u8_url, output_dir)| {
        let sender = progress_sender.clone();
        let config = config.clone();
        async move {
            let video_id = id.clone();
            let sender_for_callback = sender.clone();

            // 发送开始下载消息
            let _ = sender.send(DownloadProgress {
                video_id: video_id.clone(),
                progress: 0,
                status: "准备下载...".to_string(),
                speed: "0 KB/s".to_string(),
                eta: "--:--".to_string(),
            });

            // 定义进度回调 - 转换 YtdlpTask 到 DownloadProgress
            let progress_callback = move |task: YtdlpTask| {
                let _ = sender_for_callback.send(DownloadProgress {
                    video_id: task.id.clone(),
                    progress: task.progress,
                    status: task.message.clone(),
                    speed: task.speed.clone(),
                    eta: "--:--".to_string(),
                });
            };

            // 调用 download_video 进行下载
            let result = download_video(
                app_handle,
                &m3u8_url,
                &output_dir.to_string_lossy(),
                &video_id,
                &name,
                &config,
                progress_callback,
            ).await;

            // 发送完成消息
            let _ = sender.send(DownloadProgress {
                video_id: video_id.clone(),
                progress: 100,
                status: if result.is_ok() { "下载完成".to_string() } else { "下载失败".to_string() },
                speed: "0 KB/s".to_string(),
                eta: "--:--".to_string(),
            });

            (id, result)
        }
    }))
    .buffer_unordered(max_concurrent)
    .collect()
    .await;

    results
}

/// 获取所有任务（占位实现）
pub async fn get_all_tasks() -> Vec<YtdlpTask> {
    Vec::new()
}

/// 清理已完成的任务（占位实现）
pub async fn cleanup_tasks() {
    // TODO: 实现任务清理
}
