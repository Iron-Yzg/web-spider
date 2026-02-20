//! 视频实时解复用服务 - 不换编码只换容器
//!
//! 使用 ffmpeg -c copy 快速将 MKV/AVI 转为 MP4/HLS，速度极快

use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// 检测视频是否需要解复用（而非转码）
/// 如果视频编码已经是 H.264/H.265/VP9，只需要换容器即可
pub async fn check_video_codecs(file_path: &str, ffprobe_path: &PathBuf) -> Result<(bool, String, String), String> {
    let output = Command::new(ffprobe_path)
        .args(&[
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            "-probesize", "10M",
            "-analyzeduration", "10M",
            file_path,
        ])
        .output()
        .await
        .map_err(|e| format!("执行 ffprobe 失败: {}", e))?;

    if !output.status.success() {
        return Err("无法检测视频编码".to_string());
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析 ffprobe 输出失败: {}", e))?;

    // 查找视频流
    let mut video_codec = "unknown".to_string();
    let mut audio_codec = "unknown".to_string();
    
    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            if let Some(codec_type) = stream.get("codec_type").and_then(|c| c.as_str()) {
                if codec_type == "video" {
                    if let Some(codec) = stream.get("codec_name").and_then(|c| c.as_str()) {
                        video_codec = codec.to_string();
                    }
                } else if codec_type == "audio" {
                    if let Some(codec) = stream.get("codec_name").and_then(|c| c.as_str()) {
                        audio_codec = codec.to_string();
                    }
                }
            }
        }
    }

    // 检查是否支持直接复制（不解码重编码）
    // 支持的编码：H.264 (avc1), H.265 (hevc), VP8, VP9, AAC, MP3, OPUS
    let supported_video = ["h264", "hevc", "h265", "vp8", "vp9", "mpeg4", "mpeg2video"];
    let supported_audio = ["aac", "mp3", "opus", "vorbis", "flac", "ac3", "eac3"];
    
    let can_copy = supported_video.iter().any(|&c| video_codec.to_lowercase().contains(c))
        && supported_audio.iter().any(|&c| audio_codec.to_lowercase().contains(c));

    Ok((can_copy, video_codec, audio_codec))
}

/// 启动实时解复用为 HLS 流
/// 使用 -c copy 直接复制数据，不解码，速度极快
pub async fn start_remux_to_hls(
    file_path: String,
    session_id: String,
    ffmpeg_path: PathBuf,
) -> Result<String, String> {
    let transcode_dir = std::env::temp_dir().join("web-spider-remux").join(&session_id);
    
    // 创建输出目录
    tokio::fs::create_dir_all(&transcode_dir)
        .await
        .map_err(|e| format!("创建目录失败: {}", e))?;

    let playlist_path = transcode_dir.join("playlist.m3u8");
    let segment_pattern = transcode_dir.join("segment_%03d.ts");

    tracing::info!("[remux] 开始解复用 - session: {}, path: {}", session_id, file_path);

    // 使用 -c copy 直接复制流，不解码重编码
    // 这是关键：速度极快，CPU占用低
    let child = Command::new(&ffmpeg_path)
        .args(&[
            "-hide_banner",
            "-loglevel", "warning",
            "-i", &file_path,
            "-c", "copy",           // 直接复制，不解码
            "-bsf:a", "aac_adtstoasc", // AAC音频需要这个滤镜
            "-f", "hls",
            "-hls_time", "6",       // 6秒分片
            "-hls_list_size", "0",  // 保留所有分片
            "-hls_segment_filename", &segment_pattern.to_string_lossy(),
            "-hls_flags", "delete_segments", // 自动删除旧分片
            &playlist_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 ffmpeg 失败: {}", e))?;

    let pid = child.id();

    // 存储进程信息以便后续停止
    if let Some(pid_value) = pid {
        let mut pids = RUNNING_REMUX_PIDS.lock().await;
        pids.insert(session_id.clone(), pid_value);
    }

    // 等待 playlist 生成（解复用很快，通常2-5秒）
    let mut retries = 0;
    while retries < 20 {
        if playlist_path.exists() {
            // 检查是否有实际分片
            if let Ok(content) = tokio::fs::read_to_string(&playlist_path).await {
                if content.lines().any(|l| l.contains(".ts")) {
                    tracing::info!("[remux] 解复用成功，启动 HTTP 服务器...");

                    // 启动 HTTP 服务器提供 HLS 流
                    let hls_url = crate::services::hls_server::start_hls_server(
                        session_id.clone(),
                        transcode_dir.clone()
                    ).await?;

                    tracing::info!("[remux] HTTP 播放地址: {}", hls_url);
                    return Ok(hls_url);
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        retries += 1;
    }

    // 停止进程
    if let Some(pid) = pid {
        if cfg!(target_os = "windows") {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
        } else {
            let _ = std::process::Command::new("pkill")
                .args(["-9", "-P", &pid.to_string()])
                .output();
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
        }
    }

    Err("解复用失败，可能需要转码".to_string())
}

/// 停止解复用
pub async fn stop_remux(session_id: &str) -> Result<(), String> {
    // 停止 HTTP 服务器
    crate::services::hls_server::stop_hls_server(session_id).await.ok();

    let mut pids = RUNNING_REMUX_PIDS.lock().await;
    if let Some(pid) = pids.remove(session_id) {
        tracing::info!("[remux] 停止解复用: {} (PID: {})", session_id, pid);

        if cfg!(target_os = "windows") {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
        } else {
            let _ = std::process::Command::new("pkill")
                .args(["-9", "-P", &pid.to_string()])
                .output();
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
        }
    }

    // 清理临时文件
    let transcode_dir = std::env::temp_dir().join("web-spider-remux").join(session_id);
    if transcode_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&transcode_dir).await;
    }

    Ok(())
}

/// 运行中的解复用进程
static RUNNING_REMUX_PIDS: std::sync::LazyLock<tokio::sync::Mutex<std::collections::HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));

/// 启动视频播放（自动选择解复用或转码）
pub async fn start_video_playback(
    app_handle: tauri::AppHandle,
    file_path: String,
    session_id: String,
) -> Result<(String, bool), String> {
    use crate::services::get_sidecar_path;
    
    let ffmpeg_path = get_sidecar_path(&app_handle, "ffmpeg")?;
    let ffprobe_path = get_sidecar_path(&app_handle, "ffprobe")?;
    
    // 首先检测视频编码
    match check_video_codecs(&file_path, &ffprobe_path).await {
        Ok((can_copy, video_codec, audio_codec)) => {
            tracing::info!(
                "[playback] 视频编码检测 - can_copy: {}, video: {}, audio: {}",
                can_copy, video_codec, audio_codec
            );

            if can_copy {
                // 直接解复用，速度快
                match start_remux_to_hls(file_path.clone(), session_id.clone(), ffmpeg_path).await {
                    Ok(url) => return Ok((url, false)), // false = 不解码
                    Err(e) => {
                        tracing::warn!("[playback] 解复用失败，尝试转码: {}", e);
                        // 回退到转码
                        let url = crate::services::transcode::start_video_transcode_cmd(
                            app_handle, file_path, session_id
                        ).await?;
                        return Ok((url, true)); // true = 需要解码
                    }
                }
            } else {
                // 需要转码
                tracing::info!("[playback] 视频编码不支持直接复制，使用转码");
                let url = crate::services::transcode::start_video_transcode_cmd(
                    app_handle, file_path, session_id
                ).await?;
                return Ok((url, true));
            }
        }
        Err(e) => {
            tracing::warn!("[playback] 无法检测编码，尝试转码: {}", e);
            // 无法检测时尝试转码
            let url = crate::services::transcode::start_video_transcode_cmd(
                app_handle, file_path, session_id
            ).await?;
            return Ok((url, true));
        }
    }
}
