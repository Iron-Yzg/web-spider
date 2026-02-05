use crate::models::{YtdlpConfig, YtdlpResult, YtdlpTask, YtdlpTaskStatus};
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

/// yt-dlp 任务存储
static YTDLP_TASKS: std::sync::LazyLock<Mutex<Vec<YtdlpTask>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// 运行中的下载进程 PID 存储（用于停止下载）
static RUNNING_PIDS: std::sync::LazyLock<Mutex<std::collections::HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// 获取 yt-dlp 路径（从 bin 目录查找）
fn get_ytdlp_path() -> PathBuf {
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

    // 查找路径列表
    let search_paths = vec![
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
        // 4. 父目录的 bin（如 src-tauri 在子目录）
        std::env::current_dir()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|p| p.join("bin")),
    ];

    // 尝试所有路径
    for bin_path in search_paths.into_iter().flatten() {
        let ytdlp_path = bin_path.join(ytdlp_name);
        if ytdlp_path.exists() {
            return ytdlp_path;
        }
    }

    // 最后回退到系统 PATH
    tracing::warn!("[yt-dlp] 未找到，回退到系统 PATH: {}", ytdlp_name);
    PathBuf::from(ytdlp_name)
}

/// 获取 ffmpeg 路径（从 bin 目录查找）
fn get_ffmpeg_path() -> PathBuf {
    // 根据平台确定文件名
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

    // 查找路径列表（与 yt-dlp 相同的查找逻辑）
    let search_paths = vec![
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
    ];

    // 尝试所有路径
    for bin_path in search_paths.into_iter().flatten() {
        let ffmpeg_path = bin_path.join(ffmpeg_name);
        if ffmpeg_path.exists() {
            return ffmpeg_path;
        }
    }

    // 最后回退到系统 PATH
    tracing::warn!("[ffmpeg] 未找到，回退到系统 PATH: {}", ffmpeg_name);
    PathBuf::from(ffmpeg_name)
}

/// 检查 yt-dlp 是否可用
pub fn check_ytdlp() -> bool {
    let path = get_ytdlp_path();
    let output = StdCommand::new(&path)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    output.is_ok()
}

/// 获取 yt-dlp 版本
pub async fn get_ytdlp_version() -> String {
    let path = get_ytdlp_path();
    let output = Command::new(&path)
        .arg("--version")
        .output()
        .await;
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => "未知".to_string(),
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

/// 获取视频信息（不下载）
pub async fn get_video_info(url: &str) -> Result<YtdlpTask, String> {
    let ytdlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();

    let output = Command::new(&ytdlp_path)
        .args(&[
            "--dump-json",
            "--no-download",
            "--ffmpeg-location", ffmpeg_path.to_str().unwrap_or("ffmpeg"),
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("执行 yt-dlp 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("获取视频信息失败: {}", stderr));
    }

    let json_output = String::from_utf8_lossy(&output.stdout);

    // 解析 JSON
    let json: serde_json::Value = serde_json::from_str(&json_output)
        .map_err(|e| format!("解析视频信息失败: {}", e))?;

    let title = json.get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("未知标题")
        .to_string();

    let thumbnail = json.get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(YtdlpTask {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        title,
        thumbnail,
        progress: 0,
        speed: String::new(),
        file_path: None,
        status: YtdlpTaskStatus::Pending,
        message: "等待下载".to_string(),
        created_at: chrono::Utc::now(),
        completed_at: None,
    })
}

/// 下载单个视频（带进度回调）
pub async fn download_video(
    url: &str,
    output_path: &str,
    task_id: &str,
    config: &YtdlpConfig,
    mut progress_callback: impl FnMut(YtdlpTask) + Send,
) -> Result<YtdlpResult, String> {
    let ytdlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();
    let output_dir = PathBuf::from(output_path);
    let _ = std::fs::create_dir_all(&output_dir);

    // 构建 yt-dlp 参数
    let ffmpeg_location = ffmpeg_path.to_str().unwrap_or("ffmpeg").to_string();
    let output_template = format!("{}/%(title)s.%(ext)s", output_path);

    // 添加质量参数
    let quality_str = if config.audio_only {
        String::new()
    } else {
        config.quality.to_format_string()
    };

    let mut args: Vec<String> = vec![
        "--newline".to_string(),
        "--no-continue".to_string(),
        "--no-part".to_string(),
        "--progress".to_string(),
        // 使用简单格式模板，方便解析
        "--progress-template".to_string(),
        "[download:%(progress._percent_str)s][%(progress._speed_str)s][%(progress._eta_str)s]".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg_location,
        "-o".to_string(),
        output_template,
    ];

    // 添加质量参数
    if config.audio_only {
        args.push("--extract-audio".to_string());
        args.push("--audio-format".to_string());
        args.push(config.audio_format.clone());
    } else {
        args.push("-f".to_string());
        args.push(quality_str);
    }

    // 合并视频流 - 确保音视频合并
    if config.merge_video && !config.audio_only {
        args.push("--merge-output-format".to_string());
        args.push("mp4".to_string()); // 合并为 mp4 格式
    }

    // 字幕
    if config.subtitles {
        args.push("--write-subs".to_string());
        args.push("--sub-langs".to_string());
        args.push(config.subtitle_langs.clone());
    }

    // 封面
    if config.thumbnail {
        args.push("--write-thumbnail".to_string());
    }

    // 额外选项
    if !config.extra_options.is_empty() {
        for opt in config.extra_options.split_whitespace() {
            args.push(opt.to_string());
        }
    }

    args.push(url.to_string());

    // 打印完整命令用于调试
    let full_command = format!("{} {}", ytdlp_path.display(), args.join(" "));
    tracing::info!("[yt-dlp] Full command: {}", full_command);

    // 发送初始状态（使用已有的任务ID）
    progress_callback(YtdlpTask {
        id: task_id.to_string(),
        url: url.to_string(),
        title: String::new(),
        thumbnail: None,
        progress: 2,
        speed: String::new(),
        file_path: None,
        status: YtdlpTaskStatus::Downloading,
        message: "正在初始化...".to_string(),
        created_at: chrono::Utc::now(),
        completed_at: None,
    });

    let mut child = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 yt-dlp 失败: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // 使用 tokio::io::BufReader 读取输出
    let mut reader = BufReader::new(stdout);
    let mut buffer = String::new();
    let downloaded_size = 0u64;

    loop {
        tokio::select! {
            result = reader.read_line(&mut buffer) => {
                match result {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line = buffer.trim().to_string();
                        buffer.clear();

                        let (progress, speed, eta) = parse_progress(&line);

                        // 提取标题
                        let title = if line.contains("[download] Destination:") {
                            let parts: Vec<&str> = line.split("Destination:").collect();
                            parts.last()
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|| String::new())
                        } else {
                            String::new()
                        };

                        // 发送进度更新（使用相同的任务ID）
                        // 只要有下载进度信息就更新
                        if line.contains("[download") && (progress > 0 || !speed.is_empty() || !eta.is_empty()) {
                            progress_callback(YtdlpTask {
                                id: task_id.to_string(),
                                url: url.to_string(),
                                title: title.clone(),
                                thumbnail: None,
                                progress,
                                speed: speed.clone(),
                                file_path: None,
                                status: YtdlpTaskStatus::Downloading,
                                message: if progress > 0 {
                                    format!("下载中 {}%", progress)
                                } else {
                                    "正在连接...".to_string()
                                },
                                created_at: chrono::Utc::now(),
                                completed_at: None,
                            });
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

    let status = child.wait().await
        .map_err(|e| format!("等待 yt-dlp 失败: {}", e))?;

    if !status.success() {
        let mut error_msg = String::new();
        let mut reader = BufReader::new(stderr);
        let mut buf = String::new();
        while let Ok(n) = reader.read_line(&mut buf).await {
            if n == 0 { break; }
            error_msg.push_str(&buf);
            buf.clear();
        }
        return Err(format!("下载失败: {}", error_msg));
    }

    Ok(YtdlpResult {
        success: true,
        title: String::new(),
        file_path: output_path.to_string(),
        file_size: 0,
        thumbnail: None,
        message: "下载完成".to_string(),
    })
}

/// 下载单个视频（支持断点续传）
pub async fn download_video_with_continue(
    url: &str,
    output_path: &str,
    task_id: &str,
    title: &str,  // 任务标题，用于重命名最终文件
    config: &YtdlpConfig,
    mut progress_callback: impl FnMut(YtdlpTask) + Send,
) -> Result<YtdlpResult, String> {
    let ytdlp_path = get_ytdlp_path();
    let ffmpeg_path = get_ffmpeg_path();
    let output_dir = PathBuf::from(output_path);
    let _ = std::fs::create_dir_all(&output_dir);

    // 构建 yt-dlp 参数
    let ffmpeg_location = ffmpeg_path.to_str().unwrap_or("ffmpeg").to_string();

    // 使用任务ID作为临时文件名，确保断点续传时文件名一致
    let temp_template = format!("{}/{}", output_path, task_id);
    let output_template = format!("{}.%(ext)s", temp_template);


    // 追踪视频标题（用于进度显示）
    let mut video_title = String::new();

    // 添加质量参数
    let quality_str = if config.audio_only {
        String::new()
    } else {
        config.quality.to_format_string()
    };

    // 注意：去掉 --no-continue 和 --no-part 以支持断点续传
    // 使用 --output-na-placeholder 处理特殊字符，避免重命名错误
    // 不使用 --merge-output-format，手动合并
    let mut args: Vec<String> = vec![
        "--newline".to_string(),
        "--output-na-placeholder".to_string(),  // 使用简单的占位符代替特殊字符
        "NA".to_string(),
        "--fixup".to_string(),  // 修复常见问题
        "warn".to_string(),     // 只警告，不退出
        "--continue".to_string(),
        "--progress".to_string(),
        "--progress-template".to_string(),
        "[download:%(progress._percent_str)s][%(progress._speed_str)s][%(progress._eta_str)s]".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg_location,
        "-o".to_string(),
        output_template,
    ];

    // 添加质量参数
    if config.audio_only {
        args.push("--extract-audio".to_string());
        args.push("--audio-format".to_string());
        args.push(config.audio_format.clone());
    } else {
        args.push("-f".to_string());
        args.push(quality_str);
    }

    // 合并视频流 - 确保音视频合并
    if config.merge_video && !config.audio_only {
        args.push("--merge-output-format".to_string());
        args.push("mp4".to_string());
    }

    // 字幕
    if config.subtitles {
        args.push("--write-subs".to_string());
        args.push("--sub-langs".to_string());
        args.push(config.subtitle_langs.clone());
    }

    // 封面
    if config.thumbnail {
        args.push("--write-thumbnail".to_string());
    }

    // 额外选项
    if !config.extra_options.is_empty() {
        for opt in config.extra_options.split_whitespace() {
            args.push(opt.to_string());
        }
    }

    args.push(url.to_string());

    // 打印完整命令用于调试
    let full_command = format!("{} {}", ytdlp_path.display(), args.join(" "));
    eprintln!("[yt-dlp] 开始下载: {}", full_command);

    // 发送初始状态
    progress_callback(YtdlpTask {
        id: task_id.to_string(),
        url: url.to_string(),
        title: String::new(),
        thumbnail: None,
        progress: 0,
        speed: String::new(),
        file_path: None,
        status: YtdlpTaskStatus::Downloading,
        message: "正在初始化...".to_string(),
        created_at: chrono::Utc::now(),
        completed_at: None,
    });

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

                        // 提取文件名（用于进度显示）
                        if line.contains("[download] Destination:") {
                            let parts: Vec<&str> = line.split("Destination:").collect();
                            if let Some(s) = parts.last() {
                                let full_path = s.trim();
                                if let Some(name) = std::path::Path::new(full_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                {
                                    video_title = name;
                                    eprintln!("[yt-dlp] 下载文件: {}", video_title);
                                }
                            }
                        }

                        // 提取临时文件名（从 [download] Destination: 行）
                        if line.contains("[download] Destination:") {
                            let parts: Vec<&str> = line.split("Destination:").collect();
                            if let Some(s) = parts.last() {
                                let full_path = s.trim();
                                eprintln!("[yt-dlp] 临时文件: {}", full_path);
                            }
                        }

                        // 发送进度更新
                        if progress > 0 || !speed.is_empty() {
                            // 打印进度到控制台（使用 eprintln! 直接输出，确保能看到）
                            eprintln!("[yt-dlp-progress] {}% | {} | {} | {}",
                                progress, speed, eta, video_title);

                            progress_callback(YtdlpTask {
                                id: task_id.to_string(),
                                url: url.to_string(),
                                title: video_title.clone(),
                                thumbnail: None,
                                progress,
                                speed: speed.clone(),
                                file_path: None,
                                status: YtdlpTaskStatus::Downloading,
                                message: if progress < 100 {
                                    format!("下载中 {}%", progress)
                                } else {
                                    "处理中...".to_string()
                                },
                                created_at: chrono::Utc::now(),
                                completed_at: None,
                            });
                        } else {
                            // 没有进度信息时也打印一下，便于调试
                            eprintln!("[yt-dlp-progress] 无进度信息: {}", line);
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
        eprintln!("[yt-dlp] stderr 内容: {}", error_msg);
    }

    eprintln!("[yt-dlp] 开始处理下载结果...");

    // 步骤1：查找下载的分段文件
    // yt-dlp 下载后会生成 .mp4.part 或 .m4a.part 文件
    let mut mp4_file = String::new();
    let mut m4a_file = String::new();
    let mut part_files: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(output_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // 检查文件名是否以任务ID开头
            if filename.starts_with(task_id) {
                let full_path = path.to_string_lossy().to_string();
                eprintln!("[yt-dlp] 找到文件: {}", full_path);

                // 获取扩展名列表（如 "mp4.part" -> ["mp4", "part"]）
                let exts: Vec<String> = path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default()
                    .split('.')
                    .map(|s| s.to_string())
                    .collect();

                // 检查是否是 .xxx.part 文件
                let is_part_file = exts.last().map(|s| s == "part").unwrap_or(false);

                // 记录 .part 文件用于后续处理
                if is_part_file {
                    part_files.push(full_path.clone());
                }

                // 识别 mp4 和 m4a 文件（检查倒数第二个扩展名）
                if exts.len() >= 2 {
                    let base_ext = &exts[exts.len() - 2];
                    if base_ext == "mp4" {
                        mp4_file = full_path;
                    } else if base_ext == "m4a" {
                        m4a_file = full_path;
                    }
                } else if exts.len() == 1 {
                    let ext = &exts[0];
                    if ext == "mp4" {
                        mp4_file = full_path;
                    } else if ext == "m4a" {
                        m4a_file = full_path;
                    }
                }
            }
        }
    }

    // 步骤2：重命名 .part 文件为正式文件
    eprintln!("[yt-dlp] 重命名分段文件...");
    for part_file in &part_files {
        let part_path = std::path::Path::new(part_file);
        if let Some(ext) = part_path.extension().map(|e| e.to_string_lossy().to_string()) {
            if ext == "part" {
                // 去掉 .part 后缀
                let new_path = part_path.with_extension("");
                if let Some(filename) = part_path.file_name().map(|n| n.to_string_lossy().to_string()) {
                    eprintln!("[yt-dlp] 重命名: {} -> {}", filename, new_path.display());
                }
                if std::fs::rename(part_path, &new_path).is_ok() {
                    let new_ext = new_path.extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    if new_ext == "mp4" {
                        mp4_file = new_path.to_string_lossy().to_string();
                    } else if new_ext == "m4a" {
                        m4a_file = new_path.to_string_lossy().to_string();
                    }
                } else {
                    // 如果重命名失败，尝试使用原文件（可能已经重命名过了）
                    let without_part = new_path.to_string_lossy().to_string();
                    if std::path::Path::new(&without_part).exists() {
                        let new_ext = new_path.extension()
                            .map(|e| e.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if new_ext == "mp4" {
                            mp4_file = without_part;
                        } else if new_ext == "m4a" {
                            m4a_file = without_part;
                        }
                    }
                }
            }
        }
    }

    eprintln!("[yt-dlp] MP4文件: {}, M4A文件: {}", mp4_file, m4a_file);

    // 步骤3：处理最终文件
    let mut final_file = String::new();
    let mut final_file_size = 0u64;
    let video_title = title.to_string();

    if !mp4_file.is_empty() && !m4a_file.is_empty() {
        // 两个文件都存在，使用 ffmpeg 合并
        eprintln!("[yt-dlp] 检测到视频和音频文件，开始合并...");

        // 最终输出文件（临时命名）
        let merged_file = format!("{}_merged.mp4", temp_template);

        // ffmpeg 合并命令
        let merge_result = std::process::Command::new(&ffmpeg_path)
            .args(&[
                "-i", &mp4_file,
                "-i", &m4a_file,
                "-c", "copy",
                "-y",  // 覆盖已存在的文件
                &merged_file,
            ])
            .output();

        match merge_result {
            Ok(output) => {
                if output.status.success() {
                    eprintln!("[yt-dlp] ffmpeg 合并成功: {}", merged_file);
                    final_file = merged_file.clone();
                    final_file_size = std::fs::metadata(&merged_file).map(|m| m.len()).unwrap_or(0);

                    // 删除原始分段文件
                    let _ = std::fs::remove_file(&mp4_file);
                    let _ = std::fs::remove_file(&m4a_file);
                    eprintln!("[yt-dlp] 已删除原始分段文件");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("[yt-dlp] ffmpeg 合并失败: {}", stderr);
                    // 合并失败，使用 mp4 文件
                    final_file = mp4_file.clone();
                    final_file_size = std::fs::metadata(&mp4_file).map(|m| m.len()).unwrap_or(0);
                }
            }
            Err(e) => {
                eprintln!("[yt-dlp] 执行 ffmpeg 失败: {}", e);
                // 使用 mp4 文件
                final_file = mp4_file.clone();
                final_file_size = std::fs::metadata(&mp4_file).map(|m| m.len()).unwrap_or(0);
            }
        }
    } else if !mp4_file.is_empty() {
        // 只有 mp4 文件（可能是纯视频或 yt-dlp 已经合并过了）
        eprintln!("[yt-dlp] 只有视频文件，直接使用");
        final_file = mp4_file.clone();
        final_file_size = std::fs::metadata(&mp4_file).map(|m| m.len()).unwrap_or(0);
    } else if !m4a_file.is_empty() {
        // 只有 m4a 文件（可能是纯音频）
        eprintln!("[yt-dlp] 只有音频文件");
        final_file = m4a_file.clone();
        final_file_size = std::fs::metadata(&m4a_file).map(|m| m.len()).unwrap_or(0);
    } else {
        // 没找到任何文件
        eprintln!("[yt-dlp] 未找到下载的文件");
        return Err(format!("下载完成但未能找到视频文件"));
    }

    // 步骤4：使用任务标题重命名最终文件
    eprintln!("[yt-dlp] 使用标题重命名最终文件: {}", video_title);
    let sanitized_title = video_title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    if !sanitized_title.is_empty() {
        let final_ext = std::path::Path::new(&final_file)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "mp4".to_string());

        let renamed_file = format!("{}/{}.{}", output_path, sanitized_title, final_ext);

        if std::path::Path::new(&renamed_file).exists() {
            eprintln!("[yt-dlp] 目标文件已存在，删除旧文件");
            let _ = std::fs::remove_file(&renamed_file);
        }

        if std::fs::rename(&final_file, &renamed_file).is_ok() {
            eprintln!("[yt-dlp] 重命名成功: {} -> {}", final_file, renamed_file);
            final_file = renamed_file;
        } else {
            eprintln!("[yt-dlp] 重命名失败，保持原文件名");
        }
    }

    // 步骤5：清理临时文件
    eprintln!("[yt-dlp] 清理临时文件...");
    if let Ok(entries) = std::fs::read_dir(output_path) {
        for entry in entries.flatten() {
            let filename = entry.file_name().to_string_lossy().to_string();

            // 清理以任务ID开头的临时文件
            if filename.starts_with(task_id) {
                let path = entry.path();

                // 获取扩展名列表，检查是否是 .xxx.part 文件
                let exts: Vec<String> = path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default()
                    .split('.')
                    .map(|s| s.to_string())
                    .collect();

                // 只清理明确的临时文件（.part, .temp, .ytdlp 或 .xxx.part）
                let is_temp_file = exts.last()
                    .map(|s| s == "part" || s == "temp" || s == "ytdlp")
                    .unwrap_or(false);

                if is_temp_file {
                    if path.exists() {
                        if let Err(e) = std::fs::remove_file(&path) {
                            eprintln!("[yt-dlp] 删除临时文件失败: {} - {}", path.display(), e);
                        } else {
                            eprintln!("[yt-dlp] 已删除临时文件: {}", path.display());
                        }
                    }
                }
            }
        }
    }

    eprintln!("[yt-dlp] 下载完成: {}", final_file);
    Ok(YtdlpResult {
        success: true,
        title: video_title,
        file_path: final_file,
        file_size: final_file_size,
        thumbnail: None,
        message: "下载完成".to_string(),
    })
}

/// 取消下载任务（真正杀死进程）
pub fn cancel_task(task_id: &str) -> bool {
    let result = futures::executor::block_on(async {
        // 尝试通过 PID 杀死运行中的进程
        let mut pids = RUNNING_PIDS.lock().await;
        if let Some(pid) = pids.remove(task_id) {
            eprintln!("[yt-dlp] 杀死进程: {} (PID: {})", task_id, pid);

            // 在 macOS 上使用 kill 命令
            #[cfg(target_os = "macos")]
            {
                let output = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .output();
                match output {
                    Ok(o) => {
                        if !o.status.success() {
                            eprintln!("[yt-dlp] kill 命令失败: {}", String::from_utf8_lossy(&o.stderr));
                        }
                    }
                    Err(e) => {
                        eprintln!("[yt-dlp] 执行 kill 失败: {}", e);
                    }
                }
            }

            // 在 Rust 的其他平台上尝试使用 kill
            #[cfg(not(target_os = "macos"))]
            {
                use std::os::unix::process::ProcessId;
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGKILL);
                }
            }

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

/// 根据ID获取任务
pub async fn get_task_by_id(task_id: &str) -> Option<YtdlpTask> {
    YTDLP_TASKS.lock().await
        .iter()
        .find(|t| t.id == task_id)
        .cloned()
}

/// 按状态获取任务
pub async fn get_tasks_by_status(status: YtdlpTaskStatus) -> Vec<YtdlpTask> {
    YTDLP_TASKS.lock().await
        .iter()
        .filter(|t| t.status == status)
        .cloned()
        .collect()
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
