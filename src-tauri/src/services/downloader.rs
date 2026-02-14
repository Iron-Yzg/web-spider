use crate::models::{DownloadProgress, LocalVideo};
use crate::services::{get_sidecar_path, get_sidecar_bin_dir};
use crate::Database;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use futures::stream::{self, StreamExt};
use tauri::{AppHandle, Manager};

/// 正在下载的视频ID集合
pub static DOWNLOADING_VIDEOS: std::sync::LazyLock<Arc<Mutex<Vec<String>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// 运行中的下载进程 PID 存储
static RUNNING_PIDS: std::sync::LazyLock<Mutex<std::collections::HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// 标记视频开始下载
pub fn start_download(video_id: &str) {
    let mut downloading = DOWNLOADING_VIDEOS.lock().unwrap();
    if !downloading.contains(&video_id.to_string()) {
        downloading.push(video_id.to_string());
    }
}

/// 标记视频下载完成
pub fn finish_download(video_id: &str) {
    let mut downloading = DOWNLOADING_VIDEOS.lock().unwrap();
    downloading.retain(|id| id != video_id);
}

/// 检查 yt-dlp 是否可用
pub fn check_ytdlp(app_handle: &AppHandle) -> bool {
    match get_sidecar_path(app_handle, "yt-dlp") {
        Ok(path) => {
            std::process::Command::new(&path)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        Err(_) => false,
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

/// 解析 yt-dlp 进度输出
/// 标准格式: [download:50.0%][1.5 MB/s][ETA 00:30]
/// 或: [download:100.0%][3.50MiB/s][NA]
fn parse_ytdlp_progress(output: &str) -> (u8, String, String) {
    let mut progress = 0u8;
    let mut speed = String::new();
    let mut eta = String::new();

    // 跳过空行和非下载行
    if output.trim().is_empty() || !output.contains("[download") {
        return (progress, speed, eta);
    }
    // 查找进度部分开始位置
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

    (0, String::from("0 MB/s"), String::from("--:--"))
}

/// 解码 URL（处理数据库中存储的编码 URL）
/// 修复常见编码问题：%5C -> /,双重编码等
fn decode_url(url: &str) -> String {
    // 首先尝试标准的 URL 解码
    if let Ok(decoded) = percent_encoding::percent_decode_str(url).decode_utf8() {
        let decoded = decoded.into_owned();

        // 修复常见的编码错误：%5C 被错误地用作路径分隔符
        // 例如: https:%5C/%5C/hls.nigoxi.cn%5C/videos5%5C/... -> https://hls.nigoxi.cn/videos5/...
        let fixed = decoded
            .replace("%5C/", "/")
            .replace("%5C%5C", "/")
            .replace("\\/", "/")
            .replace("\\\\", "/");

        // 如果解码后看起来是有效的 URL，直接返回
        if fixed.starts_with("http://") || fixed.starts_with("https://") {
            return fixed;
        }

        // 否则返回原始解码结果
        decoded
    } else {
        // 如果 UTF-8 解码失败，尝试直接修复编码问题后返回
        url.replace("%5C/", "/")
           .replace("%5C%5C", "/")
           .replace("\\/", "/")
           .replace("\\\\", "/")
    }
}

/// 使用 yt-dlp 下载视频（支持 m3u8 和普通视频）
pub async fn download_m3u8(
    app_handle: &AppHandle,
    m3u8_url: &str,
    output_path: &str,
    video_id: &str,
    video_name: &str,
    mut progress_callback: impl FnMut(DownloadProgress),
) -> Result<(), String> {
    // 从 AppHandle 获取数据库
    let db = app_handle.state::<Database>();
    // 尝试解码 URL（处理数据库中存储的编码 URL）
    let decoded_url = decode_url(m3u8_url);
    tracing::info!("[DOWNLOAD] URL 解码: {} -> {}", m3u8_url, decoded_url);

    // 检查 yt-dlp 是否可用
    if !check_ytdlp(app_handle) {
        return Err("未找到 yt-dlp，请确保已正确配置 sidecar".to_string());
    }

    // 检查 ffmpeg 是否可用
    if !check_ffmpeg(app_handle) {
        return Err("未找到 ffmpeg，请确保已正确配置 sidecar".to_string());
    }

    let output_dir = PathBuf::from(output_path);
    let _ = fs::create_dir_all(&output_dir);

    // 生成安全的文件名
    let safe_filename = sanitize_filename(video_name);

    tracing::info!("[DOWNLOAD] 原文件名：{}，生成的文件名: {}", video_name, safe_filename);

    // 获取 ffmpeg 所在目录
    let ffmpeg_bin_dir = get_sidecar_bin_dir(app_handle, "ffmpeg")?;
    tracing::info!("[DOWNLOAD] ffmpeg bin dir: {}", ffmpeg_bin_dir.display());

    // 获取 yt-dlp 路径
    let ytdlp_path = get_sidecar_path(app_handle, "yt-dlp")?;

    // 构建 yt-dlp 参数
    let mut args: Vec<String> = vec![
        "--newline".to_string(),
        "--no-check-certificate".to_string(), // 1. 忽略 SSL 证书错误（解决当前报错）
        "--prefer-insecure".to_string(),      // 2. 强制使用不安全连接（备选保障）
        "--output-na-placeholder".to_string(),
        "NA".to_string(),
        "--continue".to_string(),
        "--progress".to_string(),
        // 保持你原有的进度模板，这样你的解析函数 parse_ytdlp_progress 无需修改
        "--progress-template".to_string(),
        "[download:%(progress._percent_str)s][%(progress._speed_str)s][%(progress._eta_str)s]".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg_bin_dir.to_string_lossy().to_string(),
        
        // --- 核心修复：强制重编码逻辑 ---
        "--merge-output-format".to_string(), "mp4".to_string(),
        "--postprocessor-args".to_string(), 
        "ffmpeg:-c:v copy -c:a aac -bsf:a aac_adtstoasc -threads 2".to_string(),
        
        "-o".to_string(),
        format!("{}/{}.%(ext)s", output_path, safe_filename),
    ];

    // 在 build_args 中添加
    args.push("--cookies-from-browser".to_string());
    args.push("chrome".to_string()); // 或者 "safari", "edge", "firefox"
    args.push("--impersonate".to_string());
    args.push("chrome".to_string()); // 模拟 Chrome 的 TLS 指纹

    // // 添加常见请求头，模拟浏览器访问
    // args.push("--add-header".to_string());
    // args.push("User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string());
    // args.push("--add-header".to_string());
    // args.push("Accept: */*".to_string());
    // args.push("--add-header".to_string());
    // args.push("Accept-Language: zh-CN,zh;q=0.9,en;q=0.8".to_string());
    // args.push("--add-header".to_string());
    // args.push("Referer: https://www.google.com/".to_string());

    // 如果是 m3u8 URL
    if decoded_url.contains(".m3u8") {
        args.push("-N".to_string());
        args.push("8".to_string());
        // 关键：不要使用 --hls-prefer-ffmpeg，使用内置下载器才能看到进度条
        tracing::info!("[DOWNLOAD] 检测到 m3u8，启用多线程内置下载器以显示进度");
    }

    args.push(decoded_url.to_string());

    tracing::info!("[DOWNLOAD] 开始下载: {}", args.join(" "));

    // 发送初始状态
    progress_callback(DownloadProgress {
        video_id: video_id.to_string(),
        progress: 0,
        status: "正在初始化...".to_string(),
        speed: "0 MB/s".to_string(),
        eta: "--:--".to_string(),
    });

    // 杀掉可能存在的旧进程
    {
        let old_pid = {
            let mut pids = RUNNING_PIDS.lock().unwrap();
            pids.remove(video_id)
        };
        if let Some(pid) = old_pid {
            tracing::info!("[DOWNLOAD] 发现旧进程 PID: {}，正在杀死...", pid);
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
            tracing::info!("[DOWNLOAD] 已发送 kill 信号，等待进程完全退出...");
            // 等待进程完全退出
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    // 启动 yt-dlp
    let mut child = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 yt-dlp 失败: {}", e))?;

    // 保存 PID
    if let Some(pid) = child.id() {
        let mut pids = RUNNING_PIDS.lock().unwrap();
        pids.insert(video_id.to_string(), pid);
        tracing::info!("[DOWNLOAD] 下载进程 PID: {}", pid);
    }

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut final_file_path: Option<PathBuf> = None;
    let mut has_error = false;
    let mut error_messages: Vec<String> = Vec::new();

    // 用于检测进程是否卡住的计时器
    let _hang_timeout = std::time::Duration::from_secs(30);
    let mut _last_activity_time = std::time::Instant::now();

    loop {
        // 使用 select 异步读取 stdout 或 stderr
        let line = tokio::select! {
            line = stdout_reader.next_line() => {
                match line {
                    Ok(Some(l)) => Some((l, true)),
                    Ok(None) => {
                        // stdout 结束，检查 stderr
                        match stderr_reader.next_line().await {
                            Ok(Some(l)) => Some((l, false)),
                            _ => break,
                        }
                    }
                    Err(_) => break,
                }
            }
            line = stderr_reader.next_line() => {
                match line {
                    Ok(Some(l)) => Some((l, false)),
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        };

        match line {
            Some((line, is_stdout)) => {
                // 更新活动时间
                _last_activity_time = std::time::Instant::now();

                tracing::info!("[DOWNLOAD] {}: {}", if is_stdout { "stdout" } else { "stderr" }, line);

                if is_stdout {
                    // 解析进度
                    let (progress, speed, eta) = parse_ytdlp_progress(&line);

                    if progress > 0 {
                        progress_callback(DownloadProgress {
                            video_id: video_id.to_string(),
                            progress: progress.clamp(0, 99),
                            status: format!("下载中... {}%", progress),
                            speed: speed.clone(),
                            eta: eta.clone(),
                        });

                        // 检查输出文件是否已创建
                        let expected_ext = if decoded_url.contains(".m3u8") { "mp4" } else { "mkv" };
                        let candidate = output_dir.join(format!("{}.{}", safe_filename, expected_ext));
                        if candidate.exists() {
                            final_file_path = Some(candidate);
                        }
                    }

                    // 收集警告和错误信息
                    let lower = line.to_lowercase();
                    if lower.contains("warning") || lower.contains("error") || lower.contains("failed") {
                        error_messages.push(line.clone());
                    }
                } else {
                    // stderr 收集错误信息
                    let lower = line.to_lowercase();
                    if lower.contains("error") || lower.contains("failed") || lower.contains("warning") {
                        error_messages.push(line.clone());
                    }
                }
            }
            None => break,
        }

        // 检查进程是否卡住（30秒没有任何输出）
        if _last_activity_time.elapsed() > _hang_timeout {
            tracing::warn!("[DOWNLOAD] 检测到进程可能卡住（30秒无输出）");
            error_messages.push("下载进程超时（30秒无响应）".to_string());
            has_error = true;
            break;
        }
    }

    // 等待进程结束
    let status = child.wait().await.map_err(|e| format!("等待 yt-dlp 失败: {}", e))?;

    // 清理 PID
    {
        let mut pids = RUNNING_PIDS.lock().unwrap();
        pids.remove(video_id);
    }

    if status.success() && !has_error {
        // 查找下载的文件
        let downloaded_path = final_file_path
            .filter(|p| p.exists())
            .or_else(|| {
                // 尝试查找可能的其他扩展名
                let extensions = ["mp4", "mkv", "webm", "mov"];
                for ext in extensions {
                    let candidate = output_dir.join(format!("{}.{}", safe_filename, ext));
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
                None
            })
            .unwrap_or_else(|| output_dir.join(format!("{}.mp4", safe_filename)));

        tracing::info!("[DOWNLOAD] 下载的文件: {}", downloaded_path.display());

        // 确定最终的文件路径（使用原始视频名称）
        let actual_final_path = if video_name != safe_filename {
            // 使用原始视频名称构建目标路径
            let ext = downloaded_path.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
            let target_path = output_dir.join(format!("{}.{}", video_name, ext));

            // 如果目标路径不存在，执行重命名
            if !target_path.exists() {
                if let Err(e) = fs::rename(&downloaded_path, &target_path) {
                    tracing::warn!("[DOWNLOAD] 重命名失败: {}，使用原文件名", e);
                    downloaded_path
                } else {
                    tracing::info!("[DOWNLOAD] 已重命名: {} -> {}", downloaded_path.display(), target_path.display());
                    // 同时重命名缩略图文件（如果有）
                    for ext in ["jpg", "jpeg", "png", "webp"] {
                        let thumb_src = downloaded_path.with_extension(ext);
                        let thumb_dst = target_path.with_extension(ext);
                        if thumb_src.exists() {
                            let _ = fs::rename(&thumb_src, &thumb_dst);
                            tracing::info!("[DOWNLOAD] 已重命名缩略图: {}", thumb_dst.display());
                            break;
                        }
                    }
                    target_path
                }
            } else {
                // 目标已存在，删除下载的临时文件
                let _ = fs::remove_file(&downloaded_path);
                tracing::warn!("[DOWNLOAD] 目标文件已存在: {}", target_path.display());
                target_path
            }
        } else {
            downloaded_path
        };

        // 获取文件信息
        let file_size = if actual_final_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&actual_final_path) {
                let bytes = metadata.len();
                if bytes >= 1_073_741_824 {
                    format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
                } else if bytes >= 1_048_576 {
                    format!("{:.2} MB", bytes as f64 / 1_048_576.0)
                } else if bytes >= 1024 {
                    format!("{:.2} KB", bytes as f64 / 1024.0)
                } else {
                    format!("{} B", bytes)
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // 添加到本地视频管理
        let local_video = LocalVideo {
            id: uuid::Uuid::new_v4().to_string(),
            name: video_name.to_string(),
            file_path: actual_final_path.to_string_lossy().to_string(),
            file_size,
            duration: String::new(),
            resolution: String::new(),
            added_at: chrono::Utc::now(),
        };

        if let Err(e) = db.add_local_video(&local_video).await {
            tracing::warn!("[DOWNLOAD] 添加到本地视频失败: {}", e);
        } else {
            tracing::info!("[DOWNLOAD] 已添加到本地视频管理: {}", video_name);
        }

        progress_callback(DownloadProgress {
            video_id: video_id.to_string(),
            progress: 100,
            status: "下载完成".to_string(),
            speed: "0 MB/s".to_string(),
            eta: "00:00".to_string(),
        });

        Ok(())
    } else {
        // 构建详细的错误信息
        let error_msg = if !error_messages.is_empty() {
            // 去重并限制错误信息长度
            let unique_errors: std::collections::HashSet<&str> = error_messages.iter().map(|s| s.as_str()).collect();
            let errors: Vec<&str> = unique_errors.iter().copied().take(3).collect();
            errors.join("; ")
        } else if has_error {
            String::from("yt-dlp 进程异常终止")
        } else {
            format!("yt-dlp 下载失败 (退出码: {})", status.code().map(|c| c.to_string()).unwrap_or_else(|| String::from("未知")))
        };

        tracing::error!("[DOWNLOAD] 下载失败: {}", error_msg);

        progress_callback(DownloadProgress {
            video_id: video_id.to_string(),
            progress: 0,
            status: format!("下载失败: {}", error_msg),
            speed: "0 MB/s".to_string(),
            eta: "--:--".to_string(),
        });

        Err(error_msg)
    }
}

/// 并发批量下载视频
pub async fn batch_download_concurrent(
    app_handle: &AppHandle,
    videos: Vec<(String, String, String, PathBuf)>,
    max_concurrent: usize,
    progress_sender: broadcast::Sender<DownloadProgress>,
) -> Vec<(String, Result<(), String>)> {
    // 使用 tokio::stream 并发执行下载
    let results = stream::iter(videos.into_iter().map(|(id, name, m3u8_url, output_dir)| {
        let sender = progress_sender.clone();
        async move {
            let video_id = id.clone();
            let sender_for_callback = sender.clone();

            // 标记开始下载
            start_download(&video_id);

            // 发送开始下载消息
            let _ = sender.send(DownloadProgress {
                video_id: video_id.clone(),
                progress: 0,
                status: "准备下载...".to_string(),
                speed: "0 MB/s".to_string(),
                eta: "--:--".to_string(),
            });

            // 定义进度回调
            let progress_callback = move |p: DownloadProgress| {
                let _ = sender_for_callback.send(p);
            };

            // 执行下载
            let result = download_m3u8(app_handle, &m3u8_url, &output_dir.to_string_lossy(), &video_id, &name, progress_callback).await;

            // 标记下载完成
            finish_download(&video_id);

            // 发送完成消息
            if result.is_ok() {
                let _ = sender.send(DownloadProgress {
                    video_id: video_id.clone(),
                    progress: 100,
                    status: "下载完成".to_string(),
                    speed: "0 MB/s".to_string(),
                    eta: "00:00".to_string(),
                });
            } else if let Err(ref err) = result {
                let _ = sender.send(DownloadProgress {
                    video_id: video_id.clone(),
                    progress: 0,
                    status: format!("下载失败: {}", err),
                    speed: "0 MB/s".to_string(),
                    eta: "--:--".to_string(),
                });
            }

            (video_id.clone(), result)
        }
    }))
    .buffer_unordered(max_concurrent)
    .collect()
    .await;

    results
}

// 辅助函数：处理文件名非法字符
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\?%*:|\"<>".contains(c) { '_' } else { c })
        .collect()
}
