use crate::models::{YtdlpConfig, YtdlpResult, YtdlpTask, YtdlpTaskStatus};
use crate::services::{get_sidecar_path, get_sidecar_bin_dir};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use std::path::PathBuf;
use tauri::AppHandle;

/// yt-dlp 任务存储
static YTDLP_TASKS: std::sync::LazyLock<Mutex<Vec<YtdlpTask>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// 运行中的下载进程 PID 存储（用于停止下载）
static RUNNING_PIDS: std::sync::LazyLock<Mutex<std::collections::HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// 杀死指定 PID 的进程（macOS 使用 kill 命令，Windows 使用 taskkill）
fn kill_process(pid: u32) {
    if cfg!(target_os = "windows") {
        let _ = std::process::Command::new("taskkill")
            .args(&["/F", "/PID", &pid.to_string()])
            .output();
    } else {
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .output();
    }
}

/// 解析 yt-dlp 输出获取进度
fn parse_progress(output: &str) -> (u8, String, String) {
    let mut progress = 0u8;
    let mut speed = String::new();
    let mut eta = String::new();

    // 跳过空行和非下载行
    if output.trim().is_empty() || !output.contains("[download") {
        return (progress, speed, eta);
    }

    // 调试：打印原始输出
    tracing::debug!("[yt-dlp] 原始输出: {}", output.trim());

    // 方法1: 解析 progress-template 格式 [download:45.2%][2.50MiB/s][03:25]
    // 注意：_percent_str 可能输出 "Unknown" 或空字符串，需要检查是否为有效数字
    // 注意：yt-dlp 输出的百分号前可能有空格，如 [download:  0.0%]
    if let Some(caps) = regex::Regex::new(r#"\[download:\s*([\d.]+)\s*%\]\[([^\]]*)\]\[([^\]]*)\]"#)
        .unwrap()
        .captures(output)
    {
        if let Some(p) = caps.get(1) {
            let percent_str = p.as_str();
            // 只解析有效的数字百分比
            if let Ok(pct) = percent_str.parse::<f64>() {
                if pct >= 0.0 && pct <= 100.0 {
                    progress = pct as u8;
                }
            }
        }
        if let Some(s) = caps.get(2) {
            let speed_str = s.as_str().trim();
            // 跳过 "Unknown" 或空字符串
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
        tracing::debug!("[yt-dlp] 模板解析: progress={}%, speed={}, eta={}", progress, speed, eta);
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
        tracing::debug!("[yt-dlp] 标准解析: progress={}%, speed={}, eta={}", progress, speed, eta);
        return (progress, speed, eta);
    }

    (progress, speed, eta)
}

/// 获取视频信息（不下载）- 使用 --print 模板简洁输出
pub async fn get_video_info(app_handle: &AppHandle, url: &str, quality: u32) -> Result<YtdlpTask, String> {
    // 构建格式字符串: bestvideo[height<=quality]+bestaudio/best
    let format_str = build_format_string(quality);

    let args: Vec<String> = vec![
        "--skip-download".to_string(),
        "--no-check-certificates".to_string(),
        "--cookies-from-browser".to_string(),
        "chrome".to_string(), // 或者 "safari", "edge", "firefox"
        "--impersonate".to_string(),
        "chrome".to_string(),
        "-f".to_string(),
        format_str.to_string(),
        "--print".to_string(),
        "%(title)s|%(resolution)s|%(filesize_approx)s B|%(ext)s".to_string(),
        url.to_string(),
    ];

    // 使用 Tauri Sidecar API 获取 yt-dlp 路径
    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;
    let output = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("执行 yt-dlp 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("获取视频信息失败: {}", stderr));
    }

    // 解析输出: "标题|分辨率|文件大小 B|扩展名"
    let output_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = output_str.trim().split('|').collect();

    if parts.len() < 4 {
        return Err(format!("解析视频信息失败: {}", output_str));
    }

    let title = parts[0].to_string();
    let resolution = parts[1].to_string();
    let file_size_approx = parts[2].to_string();
    let _ext = parts[3].to_string();

    // 格式化文件大小
    let file_size = if file_size_approx.ends_with(" B") && file_size_approx != "None B" {
        let bytes: u64 = file_size_approx.trim_end_matches(" B").parse().unwrap_or(0);
        format_file_size(bytes)
    } else {
        "未知大小".to_string()
    };

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
        resolution,
        file_size,
    })
}

/// 格式化文件大小
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

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

/// 下载单个视频（支持断点续传）
pub async fn download_video_with_continue(
    app_handle: &AppHandle,
    url: &str,
    output_path: &str,
    task_id: &str,
    title: &str,  // 任务标题，用于重命名最终文件
    config: &YtdlpConfig,
    mut progress_callback: impl FnMut(YtdlpTask) + Send,
) -> Result<YtdlpResult, String> {
    // 获取 ffmpeg sidecar 所在目录，用于 --ffmpeg-location
    let ffmpeg_bin_dir = get_sidecar_bin_dir(app_handle, "ffmpeg")?;
    tracing::info!("[yt-dlp] ffmpeg bin dir: {}", &ffmpeg_bin_dir.to_string_lossy().to_string());

    // 检查 ffmpeg 是否可用
    let ffmpeg_path = get_sidecar_path(app_handle, "ffmpeg")?;
    let ffmpeg_check = Command::new(&ffmpeg_path)
        .args(["-version"])
        .output()
        .await
        .map_err(|e| format!("执行 ffmpeg 失败: {}", e))?;

    if !ffmpeg_check.status.success() {
        return Err("ffmpeg 不可用".to_string());
    }

    // 重要：在启动新下载前，先确保杀死可能存在的旧进程
    tracing::info!("[yt-dlp] 检查是否有未杀死的旧进程...");
    let mut old_pids = RUNNING_PIDS.lock().await;
    if let Some(old_pid) = old_pids.remove(task_id) {
        tracing::info!("[yt-dlp] 发现旧进程 PID: {}，正在杀死...", old_pid);
        kill_process(old_pid);
        tracing::info!("[yt-dlp] 已发送 kill 信号，等待进程完全退出...");
        // 等待进程完全退出
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    drop(old_pids);

    // 使用视频标题作为文件名模板，yt-dlp 会自动处理所有逻辑
    // 重要：不要使用 task_id 作为文件名，让 yt-dlp 使用标题
    // 去掉 --no-overwrites，让 yt-dlp 自己决定是否覆盖
    let output_template = format!("{}/{}.%(ext)s", output_path, task_id);

    // 追踪视频标题（用于进度显示）
    let video_title = title.to_string();

    let mut args: Vec<String> = vec![
        "--newline".to_string(),
        "--output-na-placeholder".to_string(),  // 使用简单的占位符代替特殊字符
        "NA".to_string(),
        "--fixup".to_string(),  // 修复常见问题
        "warn".to_string(),     // 只警告，不退出
        "--continue".to_string(), // 支持断点续传
        "--progress".to_string(),
        "--progress-template".to_string(),
        "[download:%(progress._percent_str)s][%(progress._speed_str)s][%(progress._eta_str)s]".to_string(),
        // 传递 ffmpeg 所在目录
        "--ffmpeg-location".to_string(),
        ffmpeg_bin_dir.to_string_lossy().to_string(),
        "--postprocessor-args".to_string(), "ffmpeg:-movflags +faststart".to_string(),
        "-o".to_string(),
        output_template,
    ];

    // 在 build_args 中添加
    args.push("--cookies-from-browser".to_string());
    args.push("chrome".to_string()); // 或者 "safari", "edge", "firefox"
    args.push("--impersonate".to_string());
    args.push("chrome".to_string()); // 模拟 Chrome 的 TLS 指纹

    // 封面（yt-dlp 会自动调用 ffmpeg 完成）
    if config.thumbnail {
        args.push("--write-thumbnail".to_string());
        args.push("--embed-thumbnail".to_string());
        args.push("--convert-thumbnails".to_string());
        args.push("jpg".to_string());
    }

    // 添加质量参数
    if config.audio_only {
        args.push("--extract-audio".to_string());
        args.push("--audio-format".to_string());
        args.push(config.audio_format.clone());
    } else {
        args.push("-f".to_string());
        let format_str = build_format_string(config.quality);
        args.push(format_str);
    }

    // 合并视频流
    if config.merge_video {
        args.push("--merge-output-format".to_string());
        args.push("mp4".to_string());
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

    args.push(url.to_string());

    // 打印完整命令用于调试
    tracing::info!("[yt-dlp] 开始下载: yt-dlp {}", args.join(" "));

    // 发送初始状态
    progress_callback(YtdlpTask {
        id: task_id.to_string(),
        url: url.to_string(),
        title: video_title.clone(),
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

    // 使用 Tauri Sidecar API 获取路径
    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;

    tracing::info!("[yt-dlp] 启动 yt-dlp: {}", ytdlp_path.display());
    let mut child = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 yt-dlp 失败: {}", e))?;

    // 获取进程的 PID 并保存以便停止
    let pid = child.id().unwrap_or(0);
    let task_id_clone = task_id.to_string();
    let mut pids = RUNNING_PIDS.lock().await;
    pids.insert(task_id_clone.clone(), pid);
    drop(pids);

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut reader = BufReader::new(stdout);
    let mut buffer = String::new();

    loop {
        tokio::select! {
            result = reader.read_line(&mut buffer) => {
                match result {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = buffer.trim().to_string();
                        buffer.clear();

                        let (progress, speed, eta) = parse_progress(&line);

                        // 发送进度更新
                        if progress > 0 || !speed.is_empty() {
                            tracing::info!("[yt-dlp-progress] {}% | {} | {} | {}", progress, speed, eta, video_title);

                            progress_callback(YtdlpTask {
                                id: task_id.to_string(),
                                url: url.to_string(),
                                title: video_title.clone(),
                                progress,
                                speed: speed.clone(),
                                file_path: None,
                                status: YtdlpTaskStatus::Downloading,
                                resolution: String::new(),
                                file_size: String::new(),
                                message: if progress < 100 {
                                    format!("下载中 {}%", progress)
                                } else {
                                    "处理中...".to_string()
                                },
                                created_at: chrono::Utc::now(),
                                completed_at: None,
                            });
                        } else {
                            tracing::info!("[yt-dlp-progress] {}", line);
                        }
                    }
                    Err(_) => break,
                }
            }
            _ = child.wait() => {
                break;
            }
        }
    }

    // 下载完成后移除进程 PID
    let mut pids = RUNNING_PIDS.lock().await;
    pids.remove(&task_id_clone);
    drop(pids);

    let status = child.wait().await
        .map_err(|e| format!("等待 yt-dlp 失败: {}", e))?;

    // 读取 stderr（无论成功与否）
    let mut error_msg = String::new();
    let mut reader = BufReader::new(stderr);
    let mut buf = String::new();
    while let Ok(n) = reader.read_line(&mut buf).await {
        if n == 0 { break; }
        error_msg.push_str(&buf);
        buf.clear();
    }

    // 打印 stderr 内容用于调试
    if !error_msg.is_empty() {
        tracing::info!("[yt-dlp] stderr: {}", error_msg);
    }

    if !status.success() {
        return Err(format!("yt-dlp 执行出错: {}", error_msg));
    }

    // 1. 给系统 500ms 缓冲，让 yt-dlp 彻底释放文件句柄
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 2. 构造我们要搜索的文件前缀
    let file_prefix = format!("{}.", task_id);
    let mut video_file: Option<PathBuf> = None;
    let mut image_files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(output_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();

            if name.starts_with(&file_prefix) {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                match ext {
                    "mp4" | "mkv" | "webm" => video_file = Some(path),
                    "jpg" | "webp" | "png" | "jpeg" => image_files.push(path),
                    _ => {}
                }
            }
        }
    }

    // 3. 查找视频文件
    let path = video_file.ok_or("无法找到视频文件")?;

    // 4. 重命名文件
    let sanitized_title = sanitize_filename(title);
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    let final_path = PathBuf::from(output_path).join(format!("{}.{}", sanitized_title, ext));

    // 如果最终目标已存在，先删除
    if final_path.exists() {
        let _ = std::fs::remove_file(&final_path);
    }

    std::fs::rename(&path, &final_path)
        .map_err(|_| "无法移动文件".to_string())?;

    // 5. 清理残留的封面图
    for img in image_files {
        let _ = std::fs::remove_file(img);
    }

    Ok(YtdlpResult {
        success: true,
        title: video_title,
        file_path: final_path.to_string_lossy().to_string(),
        file_size: 0,
        message: "下载完成".to_string(),
    })
}

// 辅助函数：处理文件名非法字符
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\?%*:|\"<>".contains(c) { '_' } else { c })
        .collect()
}

pub fn build_format_string(quality: u32) -> String {
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

/// 取消下载任务（真正杀死进程）
pub fn cancel_task(task_id: &str) -> bool {
    let result = futures::executor::block_on(async {
        // 尝试通过 PID 杀死运行中的进程
        let mut pids = RUNNING_PIDS.lock().await;
        if let Some(pid) = pids.remove(task_id) {
            tracing::info!("[yt-dlp] 杀死进程: {} (PID: {})", task_id, pid);
            kill_process(pid);

            return Some(true);
        }
        None
    });

    // 更新任务状态
    futures::executor::block_on(async {
        let mut tasks = YTDLP_TASKS.lock().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = YtdlpTaskStatus::Cancelled;
            task.message = "已取消".to_string();
        }
    });

    result.unwrap_or(false)
}

/// 获取所有任务
pub async fn get_all_tasks() -> Vec<YtdlpTask> {
    YTDLP_TASKS.lock().await.clone()
}

/// 清理已完成/失败的任务
pub async fn cleanup_tasks() {
    YTDLP_TASKS.lock().await
        .retain(|t| {
            t.status == YtdlpTaskStatus::Pending ||
            t.status == YtdlpTaskStatus::Queued ||
            t.status == YtdlpTaskStatus::Downloading
        });
}
