use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tauri::AppHandle;
use tokio::process::Command;

use crate::services::get_sidecar_path;

/// 正在运行的转换进程 PID 注册表（用于取消任务）
static CONVERT_PROCESSES: Lazy<Mutex<HashMap<String, u32>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// 注册转换进程 PID
fn register_convert_process(task_id: &str, pid: u32) {
    if let Ok(mut map) = CONVERT_PROCESSES.lock() {
        map.insert(task_id.to_string(), pid);
    }
}

/// 移除转换进程注册
fn unregister_convert_process(task_id: &str) {
    if let Ok(mut map) = CONVERT_PROCESSES.lock() {
        map.remove(task_id);
    }
}

/// 停止转换进程
pub fn stop_convert_process(task_id: &str) -> bool {
    if let Ok(mut map) = CONVERT_PROCESSES.lock() {
        if let Some(pid) = map.remove(task_id) {
            tracing::info!("[converter] 正在终止转换进程 PID: {}", pid);
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .spawn();
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .spawn();
            }
            return true;
        }
    }
    false
}

/// 转换任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertTask {
    pub id: String,
    pub input_path: String,
    pub output_path: String,
    pub output_format: String,
    pub progress: u8,
    pub status: ConvertStatus,
    pub message: String,
}

/// 转换状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConvertStatus {
    Pending,
    Converting,
    Completed,
    Failed,
}

/// 转换选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertOptions {
    /// 输出格式: mp4, mkv, webm, avi, mov, mp3, m4a, wav, flac, gif
    pub format: String,
    /// 视频编码: copy, h264, h265, vp9
    pub video_codec: Option<String>,
    /// 音频编码: copy, aac, mp3, opus
    pub audio_codec: Option<String>,
    /// 视频分辨率: 保持原始, 1920x1080, 1280x720, 854x480 等
    pub resolution: Option<String>,
    /// 视频码率(kbps): 如 2000, 5000 等
    pub video_bitrate: Option<u32>,
    /// 音频码率(kbps): 如 128, 192, 320 等
    pub audio_bitrate: Option<u32>,
    /// 帧率: 如 24, 30, 60
    pub fps: Option<u32>,
    /// 是否仅提取音频
    pub audio_only: bool,
    /// 裁剪开始时间(秒)
    pub start_time: Option<f64>,
    /// 裁剪结束时间(秒)
    pub end_time: Option<f64>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            format: "mp4".to_string(),
            video_codec: None,
            audio_codec: None,
            resolution: None,
            video_bitrate: None,
            audio_bitrate: None,
            fps: None,
            audio_only: false,
            start_time: None,
            end_time: None,
        }
    }
}

/// 构建 ffmpeg 转换命令参数
pub fn build_ffmpeg_args(
    input_path: &str,
    output_path: &str,
    options: &ConvertOptions,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-y".to_string()]; // 覆盖已有文件

    // 裁剪开始时间 (放在 -i 前面，seek 更快)
    if let Some(start) = options.start_time {
        args.push("-ss".to_string());
        args.push(format!("{:.2}", start));
    }

    // 输入文件
    args.push("-i".to_string());
    args.push(input_path.to_string());

    // 裁剪结束时间
    if let Some(end) = options.end_time {
        if let Some(start) = options.start_time {
            // 使用持续时间而非绝对时间
            let duration = end - start;
            if duration > 0.0 {
                args.push("-t".to_string());
                args.push(format!("{:.2}", duration));
            }
        } else {
            args.push("-to".to_string());
            args.push(format!("{:.2}", end));
        }
    }

    if options.audio_only {
        // 仅提取音频
        args.push("-vn".to_string());

        // 音频编码
        if let Some(ref codec) = options.audio_codec {
            if codec == "copy" {
                args.push("-acodec".to_string());
                args.push("copy".to_string());
            } else {
                args.push("-acodec".to_string());
                args.push(codec.clone());
            }
        }

        // 音频码率
        if let Some(bitrate) = options.audio_bitrate {
            args.push("-b:a".to_string());
            args.push(format!("{}k", bitrate));
        }
    } else {
        // GIF 特殊处理
        if options.format == "gif" {
            // 使用高质量 GIF 生成
            if let Some(ref res) = options.resolution {
                let parts: Vec<&str> = res.split('x').collect();
                if parts.len() == 2 {
                    args.push("-vf".to_string());
                    args.push(format!(
                        "fps={},scale={}:{}:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
                        options.fps.unwrap_or(10),
                        parts[0],
                        parts[1]
                    ));
                }
            } else {
                args.push("-vf".to_string());
                args.push(format!(
                    "fps={},scale=-1:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
                    options.fps.unwrap_or(10)
                ));
            }
            args.push("-loop".to_string());
            args.push("0".to_string());
        } else {
            // 视频编码
            if let Some(ref codec) = options.video_codec {
                if codec == "copy" {
                    args.push("-c:v".to_string());
                    args.push("copy".to_string());
                } else {
                    args.push("-c:v".to_string());
                    args.push(codec_to_ffmpeg(codec));
                }
            }

            // 音频编码
            if let Some(ref codec) = options.audio_codec {
                if codec == "copy" {
                    args.push("-c:a".to_string());
                    args.push("copy".to_string());
                } else {
                    args.push("-c:a".to_string());
                    args.push(codec_to_ffmpeg(codec));
                }
            }

            // 分辨率
            if let Some(ref res) = options.resolution {
                args.push("-s".to_string());
                args.push(res.clone());
            }

            // 视频码率
            if let Some(bitrate) = options.video_bitrate {
                args.push("-b:v".to_string());
                args.push(format!("{}k", bitrate));
            }

            // 音频码率
            if let Some(bitrate) = options.audio_bitrate {
                args.push("-b:a".to_string());
                args.push(format!("{}k", bitrate));
            }

            // 帧率
            if let Some(fps) = options.fps {
                args.push("-r".to_string());
                args.push(fps.to_string());
            }
        }
    }

    // 进度输出
    args.push("-progress".to_string());
    args.push("pipe:1".to_string());

    // 输出文件
    args.push(output_path.to_string());

    args
}

/// 编码名称转 ffmpeg 编码器名称
fn codec_to_ffmpeg(codec: &str) -> String {
    match codec {
        "h264" => "libx264".to_string(),
        "h265" | "hevc" => "libx265".to_string(),
        "vp9" => "libvpx-vp9".to_string(),
        "aac" => "aac".to_string(),
        "mp3" => "libmp3lame".to_string(),
        "opus" => "libopus".to_string(),
        "flac" => "flac".to_string(),
        other => other.to_string(),
    }
}

/// 执行格式转换（支持取消）
pub async fn convert_video(
    app_handle: &AppHandle,
    task_id: &str,
    input_path: &str,
    output_path: &str,
    options: &ConvertOptions,
    progress_callback: impl Fn(u8, String) + Send + 'static,
) -> Result<String, String> {
    let ffmpeg_path = get_sidecar_path(app_handle, "ffmpeg")?;
    let args = build_ffmpeg_args(input_path, output_path, options);

    tracing::info!(
        "[converter] ffmpeg {} {}",
        ffmpeg_path.display(),
        args.join(" ")
    );

    // 先获取输入文件时长用于计算进度
    let duration = get_video_duration(app_handle, input_path).await.unwrap_or(0.0);

    let mut child = Command::new(&ffmpeg_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 ffmpeg 失败: {}", e))?;

    // 注册进程 PID，以支持取消
    if let Some(pid) = child.id() {
        register_convert_process(task_id, pid);
    }

    // 读取 stdout (progress 输出)
    if let Some(stdout) = child.stdout.take() {
        let duration_clone = duration;
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            #[allow(unused_assignments)]
            let mut current_time: f64 = 0.0;

            while let Ok(Some(line)) = lines.next_line().await {
                if line.starts_with("out_time_ms=") {
                    if let Ok(ms) = line[12..].parse::<f64>() {
                        current_time = ms / 1_000_000.0;
                        if duration_clone > 0.0 {
                            let progress =
                                ((current_time / duration_clone) * 100.0).min(99.0) as u8;
                            progress_callback(progress, format!("{:.1}s / {:.1}s", current_time, duration_clone));
                        }
                    }
                } else if line.starts_with("progress=end") {
                    progress_callback(100, "转换完成".to_string());
                }
            }
        });
    }

    let task_id_owned = task_id.to_string();
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("等待 ffmpeg 完成失败: {}", e))?;

    // 清理注册
    unregister_convert_process(&task_id_owned);

    if output.status.success() {
        Ok(output_path.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 如果是被取消的，返回特定错误
        if stderr.contains("Exiting normally") || stderr.is_empty() {
            Err("任务已取消".to_string())
        } else {
            Err(format!("转换失败: {}", stderr))
        }
    }
}

/// 视频截图：在指定时间点截取帧
pub async fn screenshot_video_frame(
    app_handle: &AppHandle,
    input_path: &str,
    timestamp: f64,
    output_path: Option<String>,
) -> Result<String, String> {
    let ffmpeg_path = get_sidecar_path(app_handle, "ffmpeg")?;

    let output = output_path.unwrap_or_else(|| {
        let path = PathBuf::from(input_path);
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        parent.join(format!("{}_frame_{:.1}s.png", stem, timestamp))
            .to_string_lossy().to_string()
    });

    tracing::info!("[converter] 截图: {} @ {:.3}s -> {}", input_path, timestamp, output);

    let result = Command::new(&ffmpeg_path)
        .args([
            "-y",
            "-ss", &format!("{:.3}", timestamp),
            "-i", input_path,
            "-vframes", "1",
            "-q:v", "2",
            &output,
        ])
        .output()
        .await
        .map_err(|e| format!("截图失败: {}", e))?;

    if result.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(format!("截图失败: {}", stderr))
    }
}

/// 获取视频时长(秒)
async fn get_video_duration(app_handle: &AppHandle, input_path: &str) -> Result<f64, String> {
    let ffprobe_path = get_sidecar_path(app_handle, "ffprobe")?;
    let output = Command::new(&ffprobe_path)
        .args(&[
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-probesize", "5M",
            "-analyzeduration", "5M",
            input_path,
        ])
        .output()
        .await
        .map_err(|e| format!("执行 ffprobe 失败: {}", e))?;

    if !output.status.success() {
        return Err("ffprobe 执行失败".to_string());
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("解析 ffprobe 输出失败: {}", e))?;

    json.get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f64>().ok())
        .ok_or_else(|| "无法获取视频时长".to_string())
}

/// 生成输出文件路径
pub fn generate_output_path(input_path: &str, format: &str) -> String {
    let path = PathBuf::from(input_path);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let output = parent.join(format!("{}_converted.{}", stem, format));
    output.to_string_lossy().to_string()
}
